# Prime Hunter

`prime_hunter` is a performance-oriented Rust command-line engine that searches strictly cubic
integer polynomials

```text
f(n) = a*n^3 + b*n^2 + c*n + d
```

for long consecutive prime runs starting at `n = 0`.

## Features

- Exhaustive bounded search over non-zero cubic coefficients
- Rayon parallelism over the `b` coefficient
- Mathematical parity, CRT, and Mod-105 residue pruning
- L1-resident odd-prime lookup table
- Finite-difference evaluation for the deep verification loop
- Bidirectional streak extension and canonical polynomial translation
- Durable atomic checkpoints and persistent discovery logging
- Unit tests for mathematical and implementation invariants

## Requirements

- Rust 1.85 or newer
- A 64-bit CPU

The release profile and `.cargo/config.toml` enable native CPU optimizations. The search is
computationally intensive; a release build is strongly recommended.

## Build and test

```bash
cargo check
cargo test
cargo clippy
cargo build --release
```

Run the engine with:

```bash
./target/release/prime_hunter
```

## Search configuration

The current bounds are declared near the top of `src/main.rs`:

```text
a: [-150..-1] U [1..150]
b: [-1000..1000]
c: [-3000..3000]
d: prime values [29..10000]
```

The search writes progress to `checkpoint.txt` and discoveries with sufficiently long runs to
`discoveries.txt`. Checkpoint files include a configuration hash and are written atomically.

## Project layout

- `src/main.rs` — complete search engine and test suite
- `Cargo.toml` — dependencies and release profile
- `.cargo/config.toml` — native CPU compilation settings
- `howto.md` — short execution reference
- `gemini.md` — detailed mathematical and optimization notes

## License

No license has been declared yet.
