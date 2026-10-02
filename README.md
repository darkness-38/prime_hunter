# PrimeHunter

### A massively optimized Rust engine for discovering record-breaking cubic prime generators

[![Rust](https://img.shields.io/badge/Rust-2024 Edition-orange?logo=rust)](https://www.rust-lang.org/)
[![Parallel](https://img.shields.io/badge/Parallel-Rayon-blue)](https://github.com/rayon-rs/rayon)
[![Math](https://img.shields.io/badge/Domain-Computational%20Number%20Theory-6f42c1)](#why-cubic-prime-generators-are-difficult)
[![Performance](https://img.shields.io/badge/Focus-Performance%20Engineering-e05d44)](#under-the-hood)

PrimeHunter searches integer polynomials of the form

> **f(n) = an³ + bn² + cn + d**

for unusually long consecutive runs of primes. It combines number-theoretic pruning, cache-aware
data structures, finite-difference evaluation, and multi-core Rust execution to explore enormous
bounded search spaces at billions of combinations per second.

---

## The discovery

PrimeHunter recently highlighted two **length-39 consecutive prime-generating cubics** within
the restricted search:

| Polynomial | Prime run |
|---|---:|
| **f(n) = −13n³ + 961n² − 21590n + 155863** | **39 consecutive primes** |
| **f(n) = 13n³ − 521n² + 4870n + 9791** | **39 consecutive primes** |

These results are significant for two reasons:

1. They demonstrate that long prime runs remain discoverable in a strictly cubic, integer-valued
   family under bounded coefficient search.
2. The first canonical polynomial has **d = 155,863**, even though the forward search restricts
   `d` to `29..10,000`. PrimeHunter finds this through **bidirectional streak detection**: promising
   candidates are evaluated at negative indices, then translated so the complete streak begins at
   `n = 0`.

The engine processes a combinatorial search space of approximately **4.38 trillion
possibilities**. On a low-power **Intel Core i3-N305** with eight Gracemont E-cores, a reported
full run completed in approximately **649 seconds**, while the optimized hot path reached a
reported peak throughput of about **21.5 billion combinations per second**.

> These records and performance figures are project results for the configured search bounds.
> The first record is present in the project's discovery history; the second was supplied as a
> project result. Reproduce the run and independently verify all numerical claims before using
> them as formal mathematical records.

## Why cubic prime generators are difficult

For a polynomial

> **f(n) = an³ + bn² + cn + d**

the constant term `d = f(0)` must itself be prime. Beyond that, every coefficient affects the
shape of the sequence, while divisibility by small primes creates periodic failure patterns.
Testing every polynomial directly would waste most of the runtime on candidates that can be
rejected before a full primality test.

PrimeHunter therefore treats mathematical pruning as part of correctness—not merely as an
optimization. The current engine eliminates approximately **92.8% of candidates a priori** before
expensive primality verification.

## Under the hood

### 16 KiB L1-resident primality table

The hottest path uses a compact byte table for odd values below `32,768`. At exactly **16 KiB**,
the table is sized to fit the Intel Gracemont core's private L1 data cache alongside the compact
candidate metadata.

This keeps the most frequent primality lookups local to the core and avoids unnecessary accesses
to the much larger global sieve. Values outside the table use the exact fallback path.

### Zero inner-loop divisions

Naive modular arithmetic in the innermost loop would repeatedly evaluate operations such as
`rem_euclid`. PrimeHunter replaces this with precomputed cyclic residue state and CRT masks:

- a **35-step period** for the CRT-optimized path;
- a **105-step period** for the general path;
- incremental updates as `c` advances.

The result is a hot loop dominated by additions, table lookups, bit operations, and predictable
control flow rather than repeated integer division.

### Mod-105 buckets and layered mathematical pruning

Prime candidates for `d` are stored in flat contiguous buckets keyed by `d mod 105`, where

> **105 = 3 × 5 × 7**

Precomputed masks reject residue classes whose values at early points are forced to be divisible
by `3`, `5`, or `7`. Additional pruning layers include:

- parity alignment of `c`;
- CRT Step-6 pruning when `b ≡ 0 (mod 3)`;
- extended Mod-5 and Mod-7 residue culling;
- early rejection using `f(1)`, `f(2)`, and `f(3)`.

The filters are designed around the proof that a repeated small-prime divisibility pattern must
terminate a target-length prime streak before it can become a valid record.

### Finite differences instead of repeated polynomial evaluation

Once a candidate survives the first direct checks, PrimeHunter switches to third-order finite
differences. For a cubic, the third difference is constant:

> **Δ³f(n) = 6a**

After initialization, each subsequent value is advanced with additions only. This removes the
multiplications from the deep verification loop while preserving equivalence with direct Horner
evaluation.

### Bidirectional streak detection

Most searches only inspect `n = 0, 1, 2, ...`. PrimeHunter also walks backwards for promising
candidates:

```text
..., f(-3), f(-2), f(-1), f(0), f(1), f(2), ...
```

If negative-index values are prime, the polynomial is translated algebraically so the complete
streak has a canonical start at zero. This is the key mechanism behind discovering a canonical
record with `d = 155,863` while retaining a smaller forward-search bound.

### Parallel execution and durable progress

Rayon parallelizes the `b` dimension while the outer `a` dimension remains ordered. This provides
multi-core throughput without sacrificing deterministic progress checkpoints. Completed `a`
slices are persisted atomically, allowing long searches to resume safely after interruption.

The release profile enables aggressive optimization:

- native CPU instructions through `.cargo/config.toml`;
- link-time optimization;
- one code-generation unit;
- abort-on-panic for the release binary.

## Getting started

### Requirements

- Rust with Rust 2024 edition support;
- a 64-bit CPU;
- a machine with enough memory for the configured prime sieve;
- a release build for meaningful performance measurements.

### Clone

```bash
git clone https://github.com/darkness-38/prime_hunter.git
cd prime_hunter
```

### Build

```bash
cargo build --release
```

### Test and lint

```bash
cargo check
cargo test
cargo clippy
```

### Run

```bash
./target/release/prime_hunter
```

The active search bounds are declared near the top of `src/main.rs`:

```text
a: [-150..-1] U [1..150]
b: [-1000..1000]
c: [-3000..3000]
d: prime values [29..10000]
```

Runtime checkpoint and discovery logs are local files and are intentionally ignored by Git.

## Project layout

| Path | Purpose |
|---|---|
| `src/main.rs` | Search engine, pruning logic, verification, checkpointing, and tests |
| `Cargo.toml` | Dependencies and optimized release profile |
| `.cargo/config.toml` | Native CPU compilation settings |
| `howto.md` | Quick execution reference |
| `gemini.md` | Extended mathematical derivations and optimization notes |

## Roadmap

- **Scale to quadrillions:** expand the bounded search and improve distributed workload
  coordination while retaining exact coverage accounting.
- **Mod-11 and Mod-13 sieving:** extend residue analysis beyond Mod-105 where proven pruning
  provides a net end-to-end win.
- **GPU exploration:** investigate CUDA and heterogeneous CPU/GPU search paths for workloads that
  can be expressed efficiently as regular residue and primality batches.
- **Reproducible benchmarking:** publish fixed benchmark tiles, stage-selectivity counters, and
  hardware-specific performance comparisons.
- **Portable acceleration:** preserve a portable exact implementation alongside optional
  architecture-specific fast paths.

## Contributing

Contributions are welcome, especially in:

- mathematical proofs and soundness tests for new pruning rules;
- exact primality backends and differential validation;
- cache-aware Rust optimization;
- benchmark methodology and reproducibility;
- checkpointing and large-scale workload orchestration.

Please keep changes measurable and preserve the invariant that mathematical pruning cannot remove
valid candidates from the configured search target. Run the test suite and release build before
opening a pull request.

## License

No open-source license has been declared for this repository yet. Until a license is added, all
rights remain reserved by the copyright holder.
