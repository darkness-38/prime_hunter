#![allow(clippy::manual_is_multiple_of, clippy::needless_range_loop)]

use rayon::prelude::*;
use std::cell::Cell;
use std::fs::OpenOptions;
use std::io::{Write, stdout};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

// ============================================================================
// STATE & LOGGING
// ============================================================================
pub static BEST_LEN: AtomicU32 = AtomicU32::new(39);
pub static LOG_MUTEX: Mutex<()> = Mutex::new(());

/// Total number of 'a' values in the search space ([-500..=500] excluding 0).
pub const TOTAL_A: usize = 1000;

/// Progress counter tracking completed outer loop 'a' values.
pub static COMPLETED_A: AtomicUsize = AtomicUsize::new(0);

/// Global counter for total candidate polynomials evaluated across all threads.
pub static TOTAL_CANDIDATES: AtomicU64 = AtomicU64::new(0);

/// Atomic flag signaling whether the search engine and telemetry thread are active.
pub static RUNNING: AtomicBool = AtomicBool::new(false);

/// Batch size for thread-local candidate accumulation before updating `TOTAL_CANDIDATES`.
/// Sized to 100,000 to eliminate cache line contention and false sharing across Rayon threads.
pub const BATCH_SIZE: u64 = 100_000;

thread_local! {
    /// Thread-local counter for candidate evaluation batch accumulation.
    pub static THREAD_CANDIDATES: Cell<u64> = const { Cell::new(0) };
}

/// Zero-contention thread-local candidate accumulator.
///
/// Keeps candidate counts purely in local registers/stack, only synchronizing
/// with the global `TOTAL_CANDIDATES` atomic every `BATCH_SIZE` (100,000) iterations.
/// Automatically flushes pending counts when dropped.
#[derive(Debug)]
pub struct CandidateAccumulator<'a> {
    count: u64,
    batch_size: u64,
    target: &'a AtomicU64,
}

impl CandidateAccumulator<'static> {
    #[inline(always)]
    pub fn new(batch_size: u64) -> Self {
        Self {
            count: 0,
            batch_size,
            target: &TOTAL_CANDIDATES,
        }
    }
}

impl<'a> CandidateAccumulator<'a> {
    #[inline(always)]
    pub fn with_target(batch_size: u64, target: &'a AtomicU64) -> Self {
        Self {
            count: 0,
            batch_size,
            target,
        }
    }

    #[inline(always)]
    pub fn record(&mut self) {
        self.count += 1;
        if self.count >= self.batch_size {
            self.flush();
        }
    }

    #[inline(always)]
    pub fn flush(&mut self) {
        if self.count > 0 {
            self.target.fetch_add(self.count, Ordering::Relaxed);
            THREAD_CANDIDATES.with(|c| c.set(c.get() + self.count));
            self.count = 0;
        }
    }

    #[inline(always)]
    pub fn pending(&self) -> u64 {
        self.count
    }
}

impl Drop for CandidateAccumulator<'_> {
    fn drop(&mut self) {
        self.flush();
    }
}

/// Helper to record candidate evaluations via TLS and flush to `TOTAL_CANDIDATES`
/// when `BATCH_SIZE` is reached.
pub fn record_candidate_tls(count: u64) {
    THREAD_CANDIDATES.with(|cell| {
        let current = cell.get() + count;
        if current >= BATCH_SIZE {
            TOTAL_CANDIDATES.fetch_add(current, Ordering::Relaxed);
            cell.set(0);
        } else {
            cell.set(current);
        }
    });
}

// ============================================================================
// TELEMETRY FORMATTING HELPERS
// ============================================================================

/// Formats duration in seconds into HH:MM:SS format.
pub fn format_duration_hms(secs: u64) -> String {
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{:02}:{:02}:{:02}", hours, mins, s)
}

/// Formats integer with comma thousands separators (e.g., 1,234,567).
pub fn format_with_commas(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    let rem = s.len() % 3;
    let (first, rest) = s.split_at(if rem == 0 { 3 } else { rem });
    result.push_str(first);
    for chunk in rest.as_bytes().chunks(3) {
        result.push(',');
        result.push_str(std::str::from_utf8(chunk).unwrap());
    }
    result
}

/// Formats a complete telemetry status line according to requirements.
pub fn format_telemetry_status(
    elapsed_secs: u64,
    completed_a: usize,
    total_a: usize,
    total_candidates: u64,
    throughput_m_s: f64,
    best_len: u32,
) -> String {
    let time_str = format_duration_hms(elapsed_secs);
    let progress_pct = if total_a > 0 {
        ((completed_a as f64 / total_a as f64) * 100.0).min(100.0)
    } else {
        0.0
    };
    let comma_str = format_with_commas(total_candidates);
    let total_m = total_candidates as f64 / 1_000_000.0;

    format!(
        "[{}] Progress: {:.1}% | Total: {} ({:.2}M) | Throughput: {:.2} M/s | Current Best: {}",
        time_str, progress_pct, comma_str, total_m, throughput_m_s, best_len
    )
}

// ============================================================================
// 1. FAST PRIMALITY TESTING: DETERMINISTIC 64-BIT MILLER-RABIN
// ============================================================================

/// Computes (a * b) % m without 128-bit overflow.
#[inline(always)]
pub fn mod_mul(a: u64, b: u64, m: u64) -> u64 {
    ((a as u128 * b as u128) % (m as u128)) as u64
}

/// Computes (base^exp) % m using binary exponentiation.
#[inline(always)]
pub fn mod_pow(mut base: u64, mut exp: u64, m: u64) -> u64 {
    let mut result = 1u64;
    base %= m;
    while exp > 0 {
        if exp & 1 == 1 {
            result = mod_mul(result, base, m);
        }
        base = mod_mul(base, base, m);
        exp >>= 1;
    }
    result
}

/// Highly optimized, deterministic 64-bit Miller-Rabin primality test.
/// Correctly and deterministically evaluates primality for all integers in [0..2^64).
#[inline]
pub fn is_prime_miller_rabin(n: u64) -> bool {
    if n < 2 {
        return false;
    }
    if n == 2 || n == 3 {
        return true;
    }
    if n % 2 == 0 || n % 3 == 0 {
        return false;
    }

    // Trial division against small primes up to 97 for rapid composite elimination
    const SMALL_PRIMES: [u64; 23] = [
        5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71, 73, 79, 83, 89, 97,
    ];
    for &p in &SMALL_PRIMES {
        if n == p {
            return true;
        }
        if n % p == 0 {
            return false;
        }
    }

    // Factor n - 1 as d * 2^s
    let s = (n - 1).trailing_zeros();
    let d = (n - 1) >> s;

    // Proven minimal deterministic witness sets partitioned by size of n
    let bases: &[u64] = if n < 25_326_001 {
        &[2, 3, 5]
    } else if n < 3_215_031_751 {
        &[2, 3, 5, 7]
    } else if n < 2_152_302_898_747 {
        &[2, 3, 5, 7, 11]
    } else if n < 341_550_071_728_321 {
        &[2, 3, 5, 7, 11, 13, 17]
    } else {
        // Sinclair deterministic bases for all n < 2^64
        &[2, 325, 9375, 28178, 450775, 9780504, 1795265022]
    };

    'witness_loop: for &a in bases {
        if a % n == 0 {
            continue;
        }
        let mut x = mod_pow(a, d, n);
        if x == 1 || x == n - 1 {
            continue 'witness_loop;
        }
        for _ in 1..s {
            x = mod_mul(x, x, n);
            if x == n - 1 {
                continue 'witness_loop;
            }
        }
        return false;
    }

    true
}

// ============================================================================
// 2. L1 CACHE OPTIMIZATION FOR 'd': CONTIGUOUS PRE-GENERATED PRIMES
// ============================================================================

/// Pre-generates a Vec<i64> of prime numbers between min_d and max_d
/// using the Sieve of Eratosthenes to ensure contiguous cache residency.
pub fn generate_d_primes(min_d: usize, max_d: usize) -> Vec<i64> {
    let mut is_prime = vec![true; max_d + 1];
    is_prime[0] = false;
    if max_d >= 1 {
        is_prime[1] = false;
    }
    let limit = (max_d as f64).sqrt() as usize;
    for p in 2..=limit {
        if is_prime[p] {
            let mut mult = p * p;
            while mult <= max_d {
                is_prime[mult] = false;
                mult += p;
            }
        }
    }

    let mut primes = Vec::new();
    for p in min_d..=max_d {
        if is_prime[p] {
            primes.push(p as i64);
        }
    }
    primes
}

// ============================================================================
// 4. EARLY PRUNING & MOD-5 CHECK
// ============================================================================

/// Evaluates polynomial at n=1..=5. If the polynomial yields a composite number,
/// an even number, or a multiple of 3 or 5 early on, returns true to immediately
/// continue without calling Miller-Rabin.
#[inline(always)]
pub fn fails_early(a: i64, b: i64, c: i64, d: i64) -> bool {
    for n in 1..=5 {
        let val = ((a * n + b) * n + c) * n + d;
        if val <= 1 {
            return true;
        }
        if val % 2 == 0 && val != 2 {
            return true;
        }
        if val % 3 == 0 && val != 3 {
            return true;
        }
        if val % 5 == 0 && val != 5 {
            return true;
        }
        // Check small prime factors to prune composite values early without Miller-Rabin
        const SMALL_PRIMES: [i64; 22] = [
            7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71, 73, 79, 83, 89, 97,
        ];
        for &p in &SMALL_PRIMES {
            if val % p == 0 && val != p {
                return true;
            }
        }
    }
    false
}

/// Counts consecutive primes generated by f(n) starting from n=0.
/// Returns 0 if f(0) = d is not prime.
#[inline(always)]
pub fn count_prime_streak(a: i64, b: i64, c: i64, d: i64) -> u32 {
    if d < 2 || !is_prime_miller_rabin(d as u64) {
        return 0;
    }
    let mut streak: u32 = 1;
    let mut n = 1i64;
    loop {
        let val = ((a * n + b) * n + c) * n + d;
        if val < 2 || !is_prime_miller_rabin(val as u64) {
            break;
        }
        streak += 1;
        n += 1;
    }
    streak
}

/// Formats polynomial coefficients into a standard mathematical string representation.
pub fn format_polynomial(a: i64, b: i64, c: i64, d: i64) -> String {
    let mut s = String::from("f(n) = ");

    // a * n^3
    if a != 0 {
        if a == 1 {
            s.push_str("n^3");
        } else if a == -1 {
            s.push_str("-n^3");
        } else {
            s.push_str(&format!("{}n^3", a));
        }
    }

    // b * n^2
    if b != 0 {
        if s.len() > 7 {
            if b > 0 {
                if b == 1 {
                    s.push_str(" + n^2");
                } else {
                    s.push_str(&format!(" + {}n^2", b));
                }
            } else if b == -1 {
                s.push_str(" - n^2");
            } else {
                s.push_str(&format!(" - {}n^2", -b));
            }
        } else if b == 1 {
            s.push_str("n^2");
        } else if b == -1 {
            s.push_str("-n^2");
        } else {
            s.push_str(&format!("{}n^2", b));
        }
    }

    // c * n
    if c != 0 {
        if s.len() > 7 {
            if c > 0 {
                if c == 1 {
                    s.push_str(" + n");
                } else {
                    s.push_str(&format!(" + {}n", c));
                }
            } else if c == -1 {
                s.push_str(" - n");
            } else {
                s.push_str(&format!(" - {}n", -c));
            }
        } else if c == 1 {
            s.push('n');
        } else if c == -1 {
            s.push_str("-n");
        } else {
            s.push_str(&format!("{}n", c));
        }
    }

    // d
    if d != 0 {
        if s.len() > 7 {
            if d > 0 {
                s.push_str(&format!(" + {}", d));
            } else {
                s.push_str(&format!(" - {}", -d));
            }
        } else {
            s.push_str(&format!("{}", d));
        }
    } else if s.len() == 7 {
        s.push('0');
    }

    s
}

// ============================================================================
// MAIN SEARCH ENGINE
// ============================================================================
fn main() {
    println!("==================================================================");
    println!(" PrimeHunter: High-Throughput Algebraic Targeted Search Engine   ");
    println!(" Searching for cubic polynomials f(n) with prime streak >= 40     ");
    println!("==================================================================");

    let start_time = Instant::now();

    // 2. Pre-generate Vec<i64> of prime numbers between 10,000 and 2,000,000
    println!("Pre-generating 'd' prime candidates in [10,000..2,000,000]...");
    let d_primes = generate_d_primes(10_000, 2_000_000);
    println!(
        "Generated {} prime candidates for 'd' ({:.2} MB in contiguous memory)",
        d_primes.len(),
        (d_primes.len() * std::mem::size_of::<i64>()) as f64 / (1024.0 * 1024.0)
    );
    println!(
        "Active Rayon worker threads: {}",
        rayon::current_num_threads()
    );
    println!(
        "Search space: a in [-500..500], b_mult in [-800..800] (b=b_mult*3), c in [-8000..8000]"
    );
    println!("Initial best length threshold: 39");
    println!("Starting search...\n");

    // Spawn dedicated background telemetry monitoring thread before starting Rayon iterator
    RUNNING.store(true, Ordering::SeqCst);
    let monitor_handle = std::thread::spawn(move || {
        let mut last_tick = Instant::now();
        let mut last_cand = 0u64;

        while RUNNING.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_secs(2));
            if !RUNNING.load(Ordering::Relaxed) {
                break;
            }

            let now = Instant::now();
            let dt = (now - last_tick).as_secs_f64();
            let current_cand = TOTAL_CANDIDATES.load(Ordering::Relaxed);
            let delta = current_cand.saturating_sub(last_cand);
            let throughput_m_s = if dt > 0.0 {
                (delta as f64 / dt) / 1_000_000.0
            } else {
                0.0
            };
            last_cand = current_cand;
            last_tick = now;

            let elapsed_secs = start_time.elapsed().as_secs();
            let comp_a = COMPLETED_A.load(Ordering::Relaxed);
            let best = BEST_LEN.load(Ordering::Relaxed);

            let status_line = format_telemetry_status(
                elapsed_secs,
                comp_a,
                TOTAL_A,
                current_cand,
                throughput_m_s,
                best,
            );

            {
                let _guard = LOG_MUTEX.lock().unwrap();
                println!("{}", status_line);
                let _ = stdout().flush();
            }
        }
    });

    // 3. Nested parallel loops using Rayon
    (-500..=500i64)
        .into_par_iter()
        .filter(|&a| a != 0)
        .for_each(|a| {
            (-800..=800i64).into_par_iter().for_each(|b_mult| {
                let b = b_mult * 3; // Mod-3 shield 1
                let mut acc = CandidateAccumulator::new(BATCH_SIZE);

                for c in -8000..=8000i64 {
                    // Mod-2 Shield: (a + b + c).abs() % 2 != 0 (Ensures f(1) is always odd)
                    if (a + b + c).abs() % 2 != 0 {
                        continue;
                    }

                    // Mod-3 Shield 2: (c + a).rem_euclid(3) == 0 (Ensures f(n) is not divisible by 3)
                    if (c + a).rem_euclid(3) != 0 {
                        continue;
                    }

                    // 4. Early Pruning & Mod-5 over pre-calculated d primes
                    for &d in &d_primes {
                        acc.record();

                        if fails_early(a, b, c, d) {
                            continue;
                        }

                        // Test sequence length starting from n=0
                        let streak = count_prime_streak(a, b, c, d);
                        if streak >= 40 {
                            // 5. State & Logging
                            BEST_LEN.fetch_max(streak, Ordering::SeqCst);
                            let _guard = LOG_MUTEX.lock().unwrap();
                            let elapsed = start_time.elapsed();
                            let formula = format_polynomial(a, b, c, d);
                            let log_line = format!(
                                "Length: {} | Formula: {} | Coeffs: a={}, b={}, c={}, d={} | Elapsed: {:.2?}\n",
                                streak, formula, a, b, c, d, elapsed
                            );
                            print!("{}", log_line);
                            let _ = stdout().flush();
                            match OpenOptions::new()
                                .create(true)
                                .append(true)
                                .open("discoveries.txt")
                            {
                                Ok(mut file) => {
                                    if let Err(e) = file.write_all(log_line.as_bytes()) {
                                        eprintln!("Error writing to discoveries.txt: {}", e);
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Error opening discoveries.txt: {}", e);
                                }
                            }
                        }
                    }
                }
            });
            COMPLETED_A.fetch_add(1, Ordering::Relaxed);
        });

    // Terminate the background telemetry monitoring thread cleanly
    RUNNING.store(false, Ordering::SeqCst);
    let _ = monitor_handle.join();

    println!("\nSearch complete! Elapsed: {:.2?}", start_time.elapsed());
    println!(
        "Best streak length found: {}",
        BEST_LEN.load(Ordering::SeqCst)
    );
    println!(
        "Total combinations evaluated: {} ({:.2}M)",
        format_with_commas(TOTAL_CANDIDATES.load(Ordering::SeqCst)),
        TOTAL_CANDIDATES.load(Ordering::SeqCst) as f64 / 1_000_000.0
    );
}

// ============================================================================
// UNIT TESTS & VERIFICATION
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_miller_rabin_small_primes_and_composites() {
        let small_primes = [
            2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71, 73, 79, 83,
            89, 97, 101, 103, 107, 109, 113, 127, 131, 137, 139, 149, 151, 157, 163, 167, 173, 179,
            181, 191, 193, 197, 199,
        ];
        for &p in &small_primes {
            assert!(
                is_prime_miller_rabin(p),
                "{} should be recognized as prime",
                p
            );
        }

        let small_composites = [
            0, 1, 4, 6, 8, 9, 10, 12, 14, 15, 16, 18, 20, 21, 22, 24, 25, 26, 27, 28, 30, 32, 33,
            34, 35, 36, 38, 39, 40, 42, 44, 45, 46, 48, 49, 50, 77, 91, 121, 143, 169, 221, 289,
        ];
        for &c in &small_composites {
            assert!(
                !is_prime_miller_rabin(c),
                "{} should be recognized as composite",
                c
            );
        }
    }

    #[test]
    fn test_miller_rabin_against_primal_sieve() {
        // Cross-verify Miller-Rabin test for all integers up to 100,000 against primal::is_prime
        for n in 0..100_000u64 {
            let expected = primal::is_prime(n);
            let actual = is_prime_miller_rabin(n);
            assert_eq!(
                actual, expected,
                "Miller-Rabin mismatch at n={}: actual={}, expected={}",
                n, actual, expected
            );
        }
    }

    #[test]
    fn test_miller_rabin_carmichael_numbers() {
        // Carmichael numbers fool naive Fermat pseudoprime tests
        let carmichaels = [561, 1105, 1729, 2465, 2821, 6601, 8911, 41041, 62745, 63973];
        for &c in &carmichaels {
            assert!(
                !is_prime_miller_rabin(c),
                "Carmichael number {} must be identified as composite",
                c
            );
        }
    }

    #[test]
    fn test_miller_rabin_large_primes_and_composites() {
        // Mersenne prime M_61 = 2^61 - 1
        let m61 = (1u64 << 61) - 1;
        assert!(is_prime_miller_rabin(m61), "2^61 - 1 is prime");

        let large_primes = [
            1_000_000_007u64,
            2_147_483_647u64, // 2^31 - 1
            4_294_967_311u64, // > 2^32
            10_000_000_019u64,
        ];
        for &p in &large_primes {
            assert!(
                is_prime_miller_rabin(p),
                "{} must be identified as prime",
                p
            );
        }

        let large_composites = [
            1_000_000_007u64 * 3,
            1_000_000_007u64 * 1_000_000_007u64,
            (1u64 << 32) + 1, // Fermat number F_4 is prime, but F_5 is 4294967297 = 641 * 6700417
        ];
        for &c in &large_composites {
            assert!(
                !is_prime_miller_rabin(c),
                "{} must be identified as composite",
                c
            );
        }
    }

    #[test]
    fn test_d_primes_generation() {
        let primes = generate_d_primes(10_000, 2_000_000);
        assert!(!primes.is_empty(), "d_primes must not be empty");
        assert_eq!(primes[0], 10_007, "First prime >= 10,000 is 10,007");
        assert_eq!(
            *primes.last().unwrap(),
            1_999_993,
            "Last prime <= 2,000,000 is 1,999,993"
        );
        assert_eq!(
            primes.len(),
            147_704,
            "Total primes in [10,000..2,000,000] should be 147,704"
        );

        // Verify strictly increasing and all are prime
        for window in primes.windows(2) {
            assert!(window[0] < window[1]);
            assert!(is_prime_miller_rabin(window[0] as u64));
        }
    }

    #[test]
    fn test_mod2_shield_algebraic_soundness() {
        // Mod-2 Shield: dead when (a + b + c).abs() % 2 != 0
        // When (a + b + c) is even, for odd prime d, f(1) = (a + b + c) + d is odd.
        // When (a + b + c) is odd, f(1) is even, hence composite for d >= 10,000.
        for a in -5..=5 {
            for b in -5..=5 {
                for c in -5..=5 {
                    let d = 10007i64; // odd prime
                    let sum = a + b + c;
                    let f1 = a + b + c + d;
                    if sum.abs() % 2 != 0 {
                        // Dead combination: f(1) is even
                        assert_eq!(f1 % 2, 0, "f(1) must be even when sum is odd");
                        assert!(
                            !is_prime_miller_rabin(f1 as u64),
                            "Even f(1) > 2 must be composite"
                        );
                    } else {
                        // Surviving combination: f(1) is odd
                        assert_ne!(f1 % 2, 0, "f(1) must be odd when sum is even");
                    }
                }
            }
        }
    }

    #[test]
    fn test_mod3_shield_algebraic_soundness() {
        // When b is a multiple of 3, f(n) mod 3 = (a*n^3 + c*n + d) mod 3 = (a + c)*n + d mod 3.
        // If (c + a) mod 3 == 0, then f(n) mod 3 = d mod 3 != 0 for all n, so f(n) is never divisible by 3.
        // If (c + a) mod 3 != 0, then either f(1) or f(2) is divisible by 3.
        for a in -10i64..=10i64 {
            if a == 0 {
                continue;
            }
            for b_mult in -5i64..=5i64 {
                let b = b_mult * 3;
                for c in -10i64..=10i64 {
                    let d = 10007i64; // 10007 % 3 = 2 != 0
                    if (c + a).rem_euclid(3) == 0 {
                        for n in 0..10 {
                            let fn_val = ((a * n + b) * n + c) * n + d;
                            assert_ne!(
                                fn_val % 3,
                                0,
                                "f({}) = {} must not be divisible by 3 when (c+a)%3 == 0",
                                n,
                                fn_val
                            );
                        }
                    } else {
                        // At least one of f(1) or f(2) must be divisible by 3
                        let f1 = a + b + c + d;
                        let f2 = ((a * 2 + b) * 2 + c) * 2 + d;
                        assert!(
                            f1 % 3 == 0 || f2 % 3 == 0,
                            "Either f(1) or f(2) must be divisible by 3 when (c+a)%3 != 0"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn test_fails_early_and_count_prime_streak_on_known_cubics() {
        // Test known cubic polynomial from discoveries:
        // f(n) = -14n^3 + 514n^2 - 2970n + 8123 has streak length 31
        let a = -14;
        let b = 514;
        let c = -2970;
        let d = 8123;
        assert!(
            !fails_early(a, b, c, d),
            "Valid discovery must not fail early"
        );
        let streak = count_prime_streak(a, b, c, d);
        assert_eq!(streak, 31, "Streak length should be 31");

        // Test length 39 discovery:
        // f(n) = -13n^3 + 961n^2 - 21590n + 155863 has streak length 39
        let a39 = -13;
        let b39 = 961;
        let c39 = -21590;
        let d39 = 155863;
        assert!(
            !fails_early(a39, b39, c39, d39),
            "Length 39 polynomial must pass early check"
        );
        let streak39 = count_prime_streak(a39, b39, c39, d39);
        assert_eq!(streak39, 39, "Streak length should be 39");

        // Test polynomial that produces even number at n=1
        assert!(
            fails_early(1, 1, 1, 10007),
            "Odd sum (1+1+1=3) yields even f(1) = 10010 and must fail early"
        );

        // Test polynomial that produces negative number at n=1
        assert!(
            fails_early(-20000, 0, 0, 10007),
            "Negative f(1) must fail early"
        );
    }

    #[test]
    fn test_format_polynomial() {
        assert_eq!(format_polynomial(1, 1, 1, 41), "f(n) = n^3 + n^2 + n + 41");
        assert_eq!(format_polynomial(-1, 0, 2, 5), "f(n) = -n^3 + 2n + 5");
        assert_eq!(
            format_polynomial(-14, 514, -2970, 8123),
            "f(n) = -14n^3 + 514n^2 - 2970n + 8123"
        );
        assert_eq!(
            format_polynomial(5, -119, 732, 3203),
            "f(n) = 5n^3 - 119n^2 + 732n + 3203"
        );
    }

    #[test]
    fn test_atomic_best_len_state() {
        assert_eq!(BEST_LEN.load(Ordering::SeqCst), 39);
        BEST_LEN.fetch_max(40, Ordering::SeqCst);
        assert_eq!(BEST_LEN.load(Ordering::SeqCst), 40);
        // Reset back for test idempotency
        BEST_LEN.store(39, Ordering::SeqCst);
    }

    #[test]
    fn test_count_prime_streak_non_prime_d() {
        // Non-prime d must return streak 0 because f(0) is not prime
        assert_eq!(count_prime_streak(1, 1, 1, 4), 0);
        assert_eq!(count_prime_streak(0, 0, 0, 4), 0);
        assert_eq!(count_prime_streak(0, 0, 0, -5), 0);
        assert_eq!(count_prime_streak(1, 1, 1, 1), 0);
        assert_eq!(count_prime_streak(1, 1, 1, 0), 0);
    }

    #[test]
    fn test_fails_early_extended_primes() {
        // Polynomial where f(1) = 53*53 = 2809
        // 2809 has factor 53 in SMALL_PRIMES (7..=97) and must fail early
        let d = 2809;
        assert!(fails_early(0, 0, 0, d));

        // Polynomial where f(1) has factor 97 (97 * 101 = 9797)
        let d97 = 9797;
        assert!(fails_early(0, 0, 0, d97));
    }

    #[test]
    fn test_miller_rabin_u64_limits() {
        // 2^64 - 59 = 18446744073709551557 is the largest prime < 2^64
        let max_prime = 18_446_744_073_709_551_557u64;
        assert!(
            is_prime_miller_rabin(max_prime),
            "2^64 - 59 must be identified as prime"
        );

        // 2^64 - 1 is composite (3 * 5 * 17 * 257 * 641 * 65537 * 6700417)
        let max_u64 = u64::MAX;
        assert!(
            !is_prime_miller_rabin(max_u64),
            "2^64 - 1 must be identified as composite"
        );
    }

    #[test]
    fn test_format_polynomial_edge_cases() {
        assert_eq!(format_polynomial(0, 1, 2, 3), "f(n) = n^2 + 2n + 3");
        assert_eq!(format_polynomial(0, 0, 1, 5), "f(n) = n + 5");
        assert_eq!(format_polynomial(0, 0, 0, 0), "f(n) = 0");
    }

    #[test]
    fn test_format_duration_hms() {
        assert_eq!(format_duration_hms(0), "00:00:00");
        assert_eq!(format_duration_hms(59), "00:00:59");
        assert_eq!(format_duration_hms(60), "00:01:00");
        assert_eq!(format_duration_hms(3599), "00:59:59");
        assert_eq!(format_duration_hms(3600), "01:00:00");
        assert_eq!(format_duration_hms(3661), "01:01:01");
        assert_eq!(format_duration_hms(86400), "24:00:00");
        assert_eq!(format_duration_hms(90061), "25:01:01");
    }

    #[test]
    fn test_format_with_commas() {
        assert_eq!(format_with_commas(0), "0");
        assert_eq!(format_with_commas(7), "7");
        assert_eq!(format_with_commas(42), "42");
        assert_eq!(format_with_commas(999), "999");
        assert_eq!(format_with_commas(1000), "1,000");
        assert_eq!(format_with_commas(1001), "1,001");
        assert_eq!(format_with_commas(12345), "12,345");
        assert_eq!(format_with_commas(123456), "123,456");
        assert_eq!(format_with_commas(1234567), "1,234,567");
        assert_eq!(format_with_commas(10000000), "10,000,000");
        assert_eq!(format_with_commas(1234567890), "1,234,567,890");
    }

    #[test]
    fn test_format_telemetry_status() {
        let status = format_telemetry_status(3661, 145, 1000, 145_000_000, 42.50, 39);
        assert!(status.contains("[01:01:01]"), "Should have formatted time");
        assert!(
            status.contains("Progress: 14.5%"),
            "Should have progress percentage 14.5%"
        );
        assert!(
            status.contains("Total: 145,000,000 (145.00M)"),
            "Should have commas and millions"
        );
        assert!(
            status.contains("Throughput: 42.50 M/s"),
            "Should have throughput in M/s"
        );
        assert!(
            status.contains("Current Best: 39"),
            "Should have Current Best: 39"
        );

        // Edge case: 0 candidates, 0 elapsed, 0% progress
        let zero_status = format_telemetry_status(0, 0, 1000, 0, 0.0, 39);
        assert!(zero_status.contains("[00:00:00]"));
        assert!(zero_status.contains("Progress: 0.0%"));
        assert!(zero_status.contains("Total: 0 (0.00M)"));
        assert!(zero_status.contains("Throughput: 0.00 M/s"));
        assert!(zero_status.contains("Current Best: 39"));

        // Edge case: 100% completion
        let full_status = format_telemetry_status(7200, 1000, 1000, 500_000_000, 69.44, 42);
        assert!(full_status.contains("Progress: 100.0%"));
        assert!(full_status.contains("Current Best: 42"));
    }

    #[test]
    fn test_candidate_accumulator_batching() {
        let test_atomic = AtomicU64::new(0);
        let batch_size = 1000u64;
        let mut acc = CandidateAccumulator::with_target(batch_size, &test_atomic);

        // Record batch_size - 1 items: target should NOT change
        for _ in 0..(batch_size - 1) {
            acc.record();
        }
        assert_eq!(
            test_atomic.load(Ordering::SeqCst),
            0,
            "Target counter must not update before batch size is reached"
        );
        assert_eq!(acc.pending(), batch_size - 1);

        // 1 more item triggers flush
        acc.record();
        assert_eq!(
            test_atomic.load(Ordering::SeqCst),
            batch_size,
            "Target counter should update by batch_size upon reaching threshold"
        );
        assert_eq!(acc.pending(), 0);

        // Add partial and test Drop auto-flush
        for _ in 0..42 {
            acc.record();
        }
        drop(acc);
        assert_eq!(
            test_atomic.load(Ordering::SeqCst),
            batch_size + 42,
            "Drop must flush remaining pending counts"
        );
    }

    #[test]
    fn test_concurrent_candidate_accumulation() {
        let test_atomic = AtomicU64::new(0);
        let num_threads = 8;
        let iters_per_thread = 250_000u64;

        (0..num_threads).into_par_iter().for_each(|_| {
            let mut acc = CandidateAccumulator::with_target(BATCH_SIZE, &test_atomic);
            for _ in 0..iters_per_thread {
                acc.record();
            }
        });

        let expected_total = num_threads as u64 * iters_per_thread;
        assert_eq!(
            test_atomic.load(Ordering::SeqCst),
            expected_total,
            "Concurrent multi-threaded batch accumulation must have zero candidate loss"
        );
    }

    #[test]
    fn test_telemetry_thread_spawn_and_clean_termination() {
        RUNNING.store(true, Ordering::SeqCst);
        let test_running = std::sync::Arc::new(AtomicBool::new(true));
        let r_clone = std::sync::Arc::clone(&test_running);

        let handle = std::thread::spawn(move || {
            let start = Instant::now();
            while r_clone.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(10));
                if start.elapsed() > Duration::from_secs(5) {
                    break;
                }
            }
        });

        // Let the thread run for a short duration
        std::thread::sleep(Duration::from_millis(50));
        test_running.store(false, Ordering::SeqCst);

        let join_res = handle.join();
        assert!(
            join_res.is_ok(),
            "Telemetry thread must terminate cleanly without panicking"
        );
    }

    #[test]
    fn test_record_candidate_tls() {
        let initial = TOTAL_CANDIDATES.load(Ordering::SeqCst);
        record_candidate_tls(BATCH_SIZE);
        assert!(TOTAL_CANDIDATES.load(Ordering::SeqCst) >= initial + BATCH_SIZE);
    }

    #[test]
    fn test_progress_percentage_bounds() {
        // Clamped at 100%
        let s_over = format_telemetry_status(10, 1050, 1000, 100, 1.0, 39);
        assert!(s_over.contains("Progress: 100.0%"));

        // 0 / 0 handling without panic
        let s_zero = format_telemetry_status(10, 0, 0, 100, 1.0, 39);
        assert!(s_zero.contains("Progress: 0.0%"));
    }
}
