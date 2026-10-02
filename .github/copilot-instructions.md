# Copilot instructions for `prime_hunter`

## Build, test, and lint

Run commands from the repository root:

```bash
cargo check
cargo test
cargo test test_miller_rabin_carmichael_numbers
cargo clippy
cargo build --release
./target/release/prime_hunter
```

The test suite is an inline `#[cfg(test)]` module at the end of `src/main.rs`.
`cargo test <filter>` runs one test or a matching group; use
`cargo test <filter> -- --nocapture` when test output is needed.

Use the release binary for meaningful performance measurements. The release profile
uses `opt-level = 3`, fat LTO, one codegen unit, and `panic = "abort"`.
`.cargo/config.toml` enables `-C target-cpu=native`; preserve both settings when
changing or benchmarking the hot path.

## Architecture

- `src/main.rs` contains the complete executable, search implementation, logging,
  and tests. There is no separate library or integration-test target.
- `main` builds all prime `d` candidates in the configured interval with a sieve,
  then searches the coefficient space using nested Rayon iterators. `a` excludes
  zero, `b` is represented as `b_mult * 3`, and `c` is scanned over its bounded
  interval.
- The `c` loop applies two mathematical shields before examining `d`: parity
  alignment makes `f(1)` odd for odd `d`, and the mod-3 condition rejects
  combinations for which either `f(1)` or `f(2)` is forced divisible by 3.
- `fails_early` evaluates `f(1)` through `f(5)` with Horner-style arithmetic and
  rejects non-positive, even, or small-prime-divisible values before the full
  streak check. `count_prime_streak` validates `d = f(0)` and then evaluates
  consecutive values with deterministic Miller–Rabin.
- `is_prime_miller_rabin` is the exact primality backend for the `u64` domain:
  it handles small cases and trial-divides by small primes before using
  deterministic witness sets appropriate to the input range. Keep the
  `u128` modular multiplication because it prevents overflow.
- Successful runs update the global `BEST_LEN` atomic and serialize console/file
  discovery output with `LOG_MUTEX`. `discoveries.txt` is an append-only runtime
  log and should not be treated as source configuration.
- A dedicated background telemetry thread logs progress, total evaluated
  combinations, throughput, and current best streak every 2 seconds without
  interfering with worker threads, serialized via `LOG_MUTEX`. Global
  candidates are batched locally to avoid cache contention.

## Search invariants and conventions

- The searched polynomial is `f(n) = a*n^3 + b*n^2 + c*n + d`, with `a != 0`.
  Current runtime bounds and thresholds are printed in `main`; update comments,
  tests, and README claims together when changing them.
- Keep `d` prime: `generate_d_primes` must return an ordered, inclusive-range
  candidate list, and `count_prime_streak` must continue to return zero when
  `d` is not prime.
- The parity and mod-3 shields are correctness-preserving pruning rules, not
  merely performance options. Any change to them needs algebraic soundness
  coverage in the inline tests.
- Keep the early-pruning checks consistent with the actual prime backend. A
  candidate equal to the divisor itself is allowed for the small-prime checks;
  other non-positive or divisible values must terminate the candidate.
- `format_polynomial` is the canonical human-readable representation used in
  discovery messages and has edge-case tests for zero and unit coefficients.
- Discovery output is concurrent: update atomics before logging and hold
  `LOG_MUTEX` while writing the complete message and appending to
  `discoveries.txt`. Report file open/write failures rather than silently
  treating them as successful logging.
- Avoid allocations, locks, repeated general-purpose work, or changes to the
  pruning order inside the nested `a`/`b_mult`/`c`/`d` search unless the
  performance impact is measured with a release build.
- The extended mathematical derivations and optimization discussion in
  `gemini.md` and the user-facing search description in `README.md` are useful
  context, but verify implementation claims against `src/main.rs` before
  relying on them.
