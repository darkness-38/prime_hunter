# Copilot instructions for `prime_hunter`

## Project overview

`prime_hunter` is a Rust 2024 command-line engine that searches strictly cubic integer polynomials

```text
f(n) = a*n^3 + b*n^2 + c*n + d
```

for long consecutive prime runs starting at `n = 0`. The search is performance-sensitive and
uses Rayon to parallelize the `b` dimension while keeping the outer `a` dimension ordered for
checkpointing.

## Build, test, and lint

Run commands from the repository root:

```bash
cargo check
cargo test
cargo test test_finite_differences_matches_direct_eval
cargo test checkpoint
cargo clippy
cargo build --release
./target/release/prime_hunter
```

`cargo test <filter>` runs one test or a matching group; add `-- --nocapture` when debugging
test output. The test suite is in the `#[cfg(test)]` module at the end of `src/main.rs`.

The release profile uses fat LTO, one codegen unit, `panic = "abort"`, and
`.cargo/config.toml` enables `-C target-cpu=native`. Preserve these settings when evaluating
performance or making benchmark-related changes.

## Architecture

- `src/main.rs` contains the complete implementation: configuration bounds, mathematical
  pruning, prime tables, the parallel search loop, telemetry, checkpointing, discovery logging,
  and unit tests.
- Startup builds a `primal::Sieve` up to `SIEVE_LIMIT`, then creates the fixed-size L1 byte table
  for odd values below `L1_LIMIT`. Primality checks use the L1 table first, the global sieve
  second, and `primal::is_prime` for larger values.
- The search enumerates non-zero `a` values in `[-A_MAX..-A_MIN]` followed by
  `[A_MIN..A_MAX]`; Rayon parallelizes `b`. The `c` loop advances by two after parity alignment,
  and `d` candidates are stored in contiguous `Mod105Buckets` slices keyed by `d % 105`.
- Before testing a candidate, the engine rejects residue buckets whose values at `n = 1, 2, 3`
  are divisible by 3, 5, or 7. Candidates that survive direct checks at `n = 1, 2, 3` are
  verified from `n >= 4` with third-order finite differences, avoiding multiplications in the
  hot loop.
- Global atomics track theoretical combinations, tested candidates, heartbeat state, and the
  best run length. A mutex serializes discovery and heartbeat output.
- After each complete `a` slice, the engine atomically writes `checkpoint.txt`. It validates a
  configuration hash on resume and still accepts legacy checkpoints containing only an integer.
  Discoveries at or above `LOCAL_RECORD_THRESHOLD` are appended to `discoveries.txt`.

## Search invariants and conventions

- Keep `a != 0`; this is a strictly cubic search. Current bounds are declared at the top of
  `src/main.rs`: `a` ±1..150, `b` -1000..1000, `c` -3000..3000, and prime `d` 29..10000.
- `D_MIN = 29` is intentional for the record target (`L >= 28`), based on the divisibility
  argument for `f(k*d)`. Do not restore `d = 2`, `d = 3`, or other small primes without updating
  the proof comments, culling logic, configuration hash, and tests.
- The `c` parity alignment is coupled to odd `d`: it ensures `f(1)` can be odd. The Mod-105
  filter is coupled to the `P1`, `P2`, and `P3` values maintained as `c` advances; changes to
  coefficient iteration must preserve these incremental updates.
- `Mod105Buckets` uses parallel `raw_d` and pre-shifted `d_half` slices plus `offsets`; keep
  their indexing and residue ordering synchronized. Its 48 non-empty coprime residue buckets
  are covered by integrity tests.
- `is_prime_l1_odd` is only for values known to be odd. It handles negative/small values and
  falls back safely when a value is outside the L1 table or sieve range. Preserve those guards
  when changing candidate evaluation.
- Candidate evaluation starts with direct values at `n = 1, 2, 3`; the finite-difference state
  must remain equivalent to Horner evaluation. Update the corresponding finite-difference tests
  when changing the recurrence or its initialization.
- Mathematical pruning is part of correctness, not merely an optimization. Soundness tests
  cover parity, the `d = 2`/`d = 3` bounds, `D_MIN`, and Mod-105 filtering; known discoveries
  must continue to pass the filter.
- Checkpoint writes must remain atomic and durable (`sync_all` before rename), and checkpoint
  parsing must retain both structured metadata and legacy raw-integer compatibility. Corrupted
  checkpoints are reported rather than silently overwritten.
- Performance changes should be assessed against the release build. Avoid allocations,
  locking, or general-purpose polynomial evaluation inside the `b`/`c`/`d` hot path.

For the detailed mathematical derivations and optimization rationale, consult `gemini.md`.
