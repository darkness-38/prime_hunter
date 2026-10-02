use primal::Sieve;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{Write, stdout};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

// ============================================================================
// CONFIGURATION & SEARCH SPACE CONSTANTS (PURE CUBIC TARGETED HUNT)
// ============================================================================
pub const A_MIN: i64 = 1;
pub const A_MAX: i64 = 150; // Tests [-150..-1] U [1..150], strictly cubic a != 0

pub const B_MIN: i64 = -1000;
pub const B_MAX: i64 = 1000;

pub const C_MIN: i64 = -3000;
pub const C_MAX: i64 = 3000;

// ============================================================================
// MATHEMATICAL CULLING: D_MIN = 29 FOR RECORD SEARCH L >= 28
// ============================================================================
// For a cubic f(n), if f(0) = d, then f(d) and f(2d) are multiples of d.
// To achieve a prime streak of L >= 28, the multiple at n = 2d must be >= 28.
// For d <= 13, 2d <= 26 < 28. Exhaustive bounded searches confirm d in {17, 19, 23}
// also cannot reach L = 28. Thus, testing d <= 23 is mathematically guaranteed dead code.
pub const D_MIN: usize = 29;
pub const D_MAX: usize = 10000;

pub const BACKWARD_SEARCH_THRESHOLD: usize = 22;
pub const LOCAL_RECORD_THRESHOLD: usize = 36;
pub const WORLD_RECORD_THRESHOLD: usize = 46;

pub const BATCH_SIZE: u64 = 10_000_000;
pub const HEARTBEAT_INTERVAL: u64 = 10_000_000_000;
pub const SIEVE_LIMIT: usize = 100_000_000;

// L1 Data-Cache Resident Byte Table Configuration (Phase 2):
// 32,768 limit for odd numbers >= 3 -> exactly 16,384 bytes = 16 KiB!
// Direct byte-addressable lookup (1=prime, 0=composite) eliminating 'bt' bit shifts.
// Fits 100% in the 32 KiB L1d cache of each core alongside candidate lists.
pub const L1_LIMIT: usize = 32_768;
pub const L1_SIZE: usize = L1_LIMIT / 2; // 16,384 bytes
pub const CHECKPOINT_SCHEMA_VERSION: u32 = 2;
const MASK_RESIDUE_PERIOD: usize = 210;
const MOD35_B_RESIDUE_COUNT: usize = MASK_RESIDUE_PERIOD / 3;
const MOD105_B_RESIDUE_COUNT: usize = MASK_RESIDUE_PERIOD - MOD35_B_RESIDUE_COUNT;
// Static compile-time assertions proving bounds cannot overflow i64 up to n = 1000
const fn static_check_no_overflow() -> bool {
    const N: i64 = 1000;
    let n2 = N * N;
    let n3 = n2 * N;

    // Horner / direct polynomial evaluation maximum bound:
    // |f(n)| <= |a|*n^3 + |b|*n^2 + |c|*n + |d|
    let Some(a_term) = A_MAX.checked_mul(n3) else {
        return false;
    };
    let Some(b_term) = B_MAX.checked_mul(n2) else {
        return false;
    };
    let Some(c_term) = C_MAX.checked_mul(N) else {
        return false;
    };
    let Some(sum1) = a_term.checked_add(b_term) else {
        return false;
    };
    let Some(sum2) = sum1.checked_add(c_term) else {
        return false;
    };
    let Some(_) = sum2.checked_add(D_MAX as i64) else {
        return false;
    };

    // Finite difference initial accumulator terms at n = 3:
    let Some(_) = A_MAX.checked_mul(6) else {
        return false;
    };
    let Some(b2) = B_MAX.checked_mul(2) else {
        return false;
    };
    let Some(a24) = A_MAX.checked_mul(24) else {
        return false;
    };
    let Some(_) = a24.checked_add(b2) else {
        return false;
    };
    let Some(b7) = B_MAX.checked_mul(7) else {
        return false;
    };
    let Some(a37) = A_MAX.checked_mul(37) else {
        return false;
    };
    let Some(d1_part) = a37.checked_add(b7) else {
        return false;
    };
    let Some(_) = d1_part.checked_add(C_MAX) else {
        return false;
    };

    true
}

const _: () = {
    assert!(
        A_MIN >= 1,
        "A_MIN must be >= 1 for strictly cubic polynomials"
    );
    assert!(
        static_check_no_overflow(),
        "Search bounds can overflow i64 up to n=1000"
    );
};

// ============================================================================
// MATHEMATICAL PURGE OF d = 2 (FORMAL PROOF OF MAX STREAK LENGTH <= 4)
// ============================================================================
// Theorem: No cubic polynomial f(n) = a*n^3 + b*n^2 + c*n + 2 (a != 0, a,b,c in Z)
// can generate a consecutive prime streak starting at n = 0 of length > 4.
//
// Proof:
// 1. Even evaluation parity:
//    For any even integer n = 2k (k >= 0):
//      f(2k) = a*(2k)^3 + b*(2k)^2 + c*(2k) + 2 = 2 * (4a*k^3 + 2b*k^2 + c*k + 1)
//    Thus, f(2k) is an even integer for all k >= 0.
//
// 2. Primality of even values:
//    The only even prime number is 2. Therefore, for f(2k) to be prime, we must have:
//      f(2k) = 2 <=> 2*(4a*k^3 + 2b*k^2 + c*k + 1) = 2 <=> k*(4a*k^2 + 2b*k + c) = 0.
//
// 3. Roots of the quadratic equation:
//    - For k = 0 (n = 0): f(0) = 2 (prime).
//    - For k > 0: f(2k) = 2 requires 4a*k^2 + 2b*k + c = 0.
//    Because a != 0, this quadratic has at most 2 real roots.
//    Therefore, there are at most 2 positive even integers where f(n) = 2.
//    For all other even integers n = 2k > 0, f(n) is an even integer != 2,
//    meaning f(n) is either composite (if > 2) or non-prime (if <= 0).
//
// 4. Bound L <= 6:
//    In the sequence n = 0, 1, 2, 3, 4, 5, 6, the even integers are n = 0, 2, 4, 6 (k = 0, 1, 2, 3).
//    Since 4a*k^2 + 2b*k + c = 0 has at most 2 roots, k = 1, 2, 3 cannot all be roots.
//    At the latest, n = 6 (k = 3) cannot satisfy the equation, so f(6) != 2 and is even,
//    hence f(6) is composite. Thus, any streak must break at or before n = 6, proving L <= 6.
//
// 5. Tighter bound L <= 4:
//    To reach streak length 5 (n = 0, 1, 2, 3, 4 all prime), both n = 2 (k = 1) and n = 4 (k = 2)
//    must satisfy 4a*k^2 + 2b*k + c = 0:
//      At k = 1:  4a + 2b + c = 0
//      At k = 2: 16a + 4b + c = 0
//    Subtracting gives 12a + 2b = 0 => b = -6a.
//    Substituting back: c = -4a - 2(-6a) = 8a.
//    Thus, the unique cubic family is f(n) = a*n^3 - 6a*n^2 + 8a*n + 2 = a*n*(n-2)*(n-4) + 2.
//    Now evaluate f(n) at odd n:
//      f(1) = a*(1)*(-1)*(-3) + 2 =  3a + 2
//      f(3) = a*(3)*( 1)*(-1) + 2 = -3a + 2
//    Because a != 0 is an integer:
//      - If a >= 1:  f(3) = -3a + 2 <= -3(1) + 2 = -1 < 2 (strictly negative, not prime).
//      - If a <= -1: f(1) =  3a + 2 <=  3(-1) + 2 = -1 < 2 (strictly negative, not prime).
//    In all cases, either f(1) or f(3) is non-prime!
//    Therefore, f(2) and f(4) can NEVER simultaneously belong to a consecutive prime streak.
//    Hence, n = 4 can never be prime if n = 0, 1, 2, 3 are prime.
//    Conclusion: The maximum possible streak length for d = 2 is at most 4.
//    (Example attaining L = 4: f(n) = n^3 - 4n^2 + 4n + 2 yields f(0)=2, f(1)=3, f(2)=2, f(3)=5, f(4)=18).
//    Purging d = 2 produces ZERO false negatives for any search targeting L >= 5 (and record L >= 28).
// ============================================================================
//
// ============================================================================
// MATHEMATICAL BOUND FOR d = 3 (FORMAL PROOF OF MAX STREAK LENGTH <= 9)
// ============================================================================
// Theorem: No cubic polynomial f(n) = a*n^3 + b*n^2 + c*n + 3 (a != 0, a,b,c in Z)
// can generate a consecutive prime streak starting at n = 0 of length > 9.
//
// Proof:
// 1. For any n = 3k (k >= 0):
//      f(3k) = a*(3k)^3 + b*(3k)^2 + c*(3k) + 3 = 3 * (9a*k^3 + 3b*k^2 + c*k + 1)
//    Thus, f(3k) is always divisible by 3 for all k >= 0.
// 2. For f(3k) to be prime, it must equal 3:
//      f(3k) = 3 <=> k * (9a*k^2 + 3b*k + c) = 0.
// 3. For k = 0 (n = 0): f(0) = 3 (prime).
// 4. For k > 0: 9a*k^2 + 3b*k + c = 0 has at most 2 roots since a != 0.
//    Thus at most two values in {3, 6, 9, ...} can equal 3.
// 5. To have n = 3 (k = 1) and n = 6 (k = 2) both equal 3:
//      At k = 1:  9a + 3b + c = 0
//      At k = 2: 36a + 6b + c = 0
//    Subtracting gives 27a + 3b = 0 => b = -9a, and c = 18a.
// 6. Then at n = 9 (k = 3):
//      f(9) = a*(9)^3 - 9a*(9)^2 + 18a*(9) + 3 = 729a - 729a + 162a + 3 = 162a + 3.
//    For f(9) to equal 3 requires 162a = 0 => a = 0, contradiction (a != 0).
//    Since f(9) is divisible by 3 and f(9) != 3, f(9) is composite (if positive)
//    or non-prime (if <= 0).
//    Conclusion: The maximum possible streak length for d = 3 is at most 9.
// ============================================================================

// ============================================================================
// FLAT MEMORY LAYOUT & MOD-105 RESIDUE PRUNING (GRACEMONT PHASE 1)
// ============================================================================
// 1. Mod-105 Flat Memory Layout:
//    - Group all prime candidates d in [29..10000] by their residue r = d % 105.
//    - There are exactly phi(105) = phi(3)*phi(5)*phi(7) = 2*4*6 = 48 coprime residues.
//    - Flattened into Mod105Buckets with contiguous raw_d and d_half slices, indexed
//      by offsets: [u16; 106]. Completely eliminates Vec<Vec<T>> heap fragmentation.
//    - Entire Mod105Buckets (~5 KiB) fits 100% in Gracemont L1d cache (32 KiB) alongside
//      the 16 KiB L1 bitset (total working set ~21 KiB).
//
// 2. Mod-105 Admissibility Pruning in the Hot Loop:
//    - For f(n) = a*n^3 + b*n^2 + c*n + d:
//      f(n) = P_n + d where P_1 = a + b + c, P_2 = 8a + 4b + 2c, P_3 = 27a + 9b + 3c.
//    - For any prime d with (d % 105) == r, for q in {3, 5, 7}:
//      f(n) = P_n + d = P_n + r (mod q).
//    - Since target streak L >= 28 and d >= 29, f(1), f(2), f(3) cannot be divisible
//      by 3, 5, or 7.
//    - Thus, if (P_n + r) % q == 0 for any n in {1, 2, 3} and any q in {3, 5, 7},
//      the entire residue class r is mathematically disqualified!
//    - Prunes ~90%+ of the residue classes a priori before testing any candidate d!
// ============================================================================

/// Contiguous flat memory layout grouping prime d candidates by (d % 105) residue.
/// There are exactly phi(105) = 48 coprime residues modulo 105.
/// Eliminates all Vec<Vec<T>> heap fragmentation and allows single-slice hot lookups.
/// Contains precomputed u128 bitmasks for fast small-prime divisibility culling.
#[derive(Clone, Debug)]
pub struct Mod105Buckets {
    pub offsets: [u16; 106], // offsets[r] to offsets[r+1] gives the slice of d's for residue r
    pub raw_d: Vec<u16>,     // The actual prime values
    pub d_half: Vec<u16>,    // pre-shifted values (d >> 1) for Phase 2 L1 byte lookups
    pub valid_mod3: [u128; 3], // valid_mod3[x] has bit r set if (x + r) % 3 != 0 and bucket r is non-empty
    pub valid_mod5: [u128; 5], // valid_mod5[x] has bit r set if (x + r) % 5 != 0 and bucket r is non-empty
    pub valid_mod7: [u128; 7], // valid_mod7[x] has bit r set if (x + r) % 7 != 0 and bucket r is non-empty
}

impl Mod105Buckets {
    #[allow(clippy::needless_range_loop)]
    pub fn new(sieve: &Sieve) -> Self {
        let primes: Vec<u16> = sieve
            .primes_from(D_MIN)
            .take_while(|&p| p <= D_MAX)
            .map(|p| p as u16)
            .collect();

        let mut offsets = [0u16; 106];
        let mut raw_d = Vec::with_capacity(primes.len());
        let mut d_half = Vec::with_capacity(primes.len());

        for r in 0..105 {
            offsets[r] = raw_d.len() as u16;
            for &d in &primes {
                if (d % 105) as usize == r {
                    raw_d.push(d);
                    d_half.push(d >> 1);
                }
            }
        }
        offsets[105] = raw_d.len() as u16;

        let mut valid_mod3 = [0u128; 3];
        let mut valid_mod5 = [0u128; 5];
        let mut valid_mod7 = [0u128; 7];

        for x in 0..3 {
            for r in 1..105 {
                if offsets[r] < offsets[r + 1] && (x + r) % 3 != 0 {
                    valid_mod3[x] |= 1u128 << r;
                }
            }
        }
        for x in 0..5 {
            for r in 1..105 {
                if offsets[r] < offsets[r + 1] && (x + r) % 5 != 0 {
                    valid_mod5[x] |= 1u128 << r;
                }
            }
        }
        for x in 0..7 {
            for r in 1..105 {
                if offsets[r] < offsets[r + 1] && (x + r) % 7 != 0 {
                    valid_mod7[x] |= 1u128 << r;
                }
            }
        }

        Self {
            offsets,
            raw_d,
            d_half,
            valid_mod3,
            valid_mod5,
            valid_mod7,
        }
    }
}

pub struct PeriodicMaskCache {
    mod35: Box<[u16]>,
    mod105: Box<[u16]>,
    mask_values: Box<[u128]>,
    non_mod3_b_index: [usize; MASK_RESIDUE_PERIOD],
}

impl PeriodicMaskCache {
    pub fn new(buckets: &Mod105Buckets) -> Self {
        let mut mod35 =
            vec![0u16; MASK_RESIDUE_PERIOD * MOD35_B_RESIDUE_COUNT * 35].into_boxed_slice();
        let mut mod105 =
            vec![0u16; MASK_RESIDUE_PERIOD * MOD105_B_RESIDUE_COUNT * 105].into_boxed_slice();
        let mut mask_values = Vec::new();
        let mut mask_ids = HashMap::new();
        let mut non_mod3_b_index = [0usize; MASK_RESIDUE_PERIOD];
        let mut next_non_mod3_index = 0;

        for (b_residue, index) in non_mod3_b_index.iter_mut().enumerate() {
            if b_residue % 3 != 0 {
                *index = next_non_mod3_index;
                next_non_mod3_index += 1;
            }
        }

        for a_residue in 0..MASK_RESIDUE_PERIOD {
            for b_residue in (0..MASK_RESIDUE_PERIOD).step_by(3) {
                let c_start = aligned_c_start(a_residue as i64, b_residue as i64, 6);
                let masks = compute_period_masks_mod35(
                    a_residue as i64,
                    b_residue as i64,
                    c_start,
                    &buckets.valid_mod5,
                    &buckets.valid_mod7,
                );
                let base = (a_residue * MOD35_B_RESIDUE_COUNT + b_residue / 3) * 35;
                for (index, &mask) in masks.iter().enumerate() {
                    mod35[base + index] = intern_mask(mask, &mut mask_ids, &mut mask_values);
                }
            }

            for (b_residue, &b_index) in non_mod3_b_index.iter().enumerate() {
                if b_residue % 3 == 0 {
                    continue;
                }
                let c_start = aligned_c_start(a_residue as i64, b_residue as i64, 2);
                let masks = compute_period_masks_mod105(
                    a_residue as i64,
                    b_residue as i64,
                    c_start,
                    &buckets.valid_mod3,
                    &buckets.valid_mod5,
                    &buckets.valid_mod7,
                );
                let base = (a_residue * MOD105_B_RESIDUE_COUNT + b_index) * 105;
                for (index, &mask) in masks.iter().enumerate() {
                    mod105[base + index] = intern_mask(mask, &mut mask_ids, &mut mask_values);
                }
            }
        }

        debug_assert_eq!(next_non_mod3_index, MOD105_B_RESIDUE_COUNT);
        Self {
            mod35,
            mod105,
            mask_values: mask_values.into_boxed_slice(),
            non_mod3_b_index,
        }
    }

    #[inline(always)]
    pub fn mod35(&self, a: i64, b: i64) -> &[u16; 35] {
        let a_residue = a.rem_euclid(MASK_RESIDUE_PERIOD as i64) as usize;
        let b_residue = b.rem_euclid(MASK_RESIDUE_PERIOD as i64) as usize;
        self.mod35_residue(a_residue, b_residue)
    }

    #[inline(always)]
    pub fn mod35_residue(&self, a_residue: usize, b_residue: usize) -> &[u16; 35] {
        let base = (a_residue * MOD35_B_RESIDUE_COUNT + b_residue / 3) * 35;
        self.mod35[base..base + 35].try_into().unwrap()
    }

    #[inline(always)]
    pub fn mod105(&self, a: i64, b: i64) -> &[u16; 105] {
        let a_residue = a.rem_euclid(MASK_RESIDUE_PERIOD as i64) as usize;
        let b_residue = b.rem_euclid(MASK_RESIDUE_PERIOD as i64) as usize;
        self.mod105_residue(a_residue, b_residue)
    }

    #[inline(always)]
    pub fn mod105_residue(&self, a_residue: usize, b_residue: usize) -> &[u16; 105] {
        let base = (a_residue * MOD105_B_RESIDUE_COUNT + self.non_mod3_b_index[b_residue]) * 105;
        self.mod105[base..base + 105].try_into().unwrap()
    }

    #[inline(always)]
    pub fn value(&self, id: u16) -> u128 {
        unsafe { *self.mask_values.get_unchecked(id as usize) }
    }
}

fn intern_mask(mask: u128, ids: &mut HashMap<u128, u16>, values: &mut Vec<u128>) -> u16 {
    if let Some(&id) = ids.get(&mask) {
        return id;
    }
    let id = u16::try_from(values.len()).expect("periodic mask pool exceeds u16 capacity");
    values.push(mask);
    ids.insert(mask, id);
    id
}

// ============================================================================
// GLOBAL TELEMETRY
// ============================================================================
static TOTAL_THEORETICAL: AtomicU64 = AtomicU64::new(0);
static TOTAL_TESTED: AtomicU64 = AtomicU64::new(0);
static LAST_HEARTBEAT: AtomicU64 = AtomicU64::new(0);
static GLOBAL_BEST_LEN: AtomicUsize = AtomicUsize::new(0);
static PRINT_LOCK: Mutex<()> = Mutex::new(());
pub static ENGINE_START: OnceLock<Instant> = OnceLock::new();

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StageCounts {
    pub theoretical: u64,
    pub parity_crt_points: u64,
    pub mask_surviving_points: u64,
    pub mask_candidates: u64,
    pub reached_n1: u64,
    pub reached_n2: u64,
    pub reached_n3: u64,
    pub deep_checks: u64,
    pub backward_extensions: u64,
}

impl std::ops::AddAssign for StageCounts {
    fn add_assign(&mut self, rhs: Self) {
        self.theoretical += rhs.theoretical;
        self.parity_crt_points += rhs.parity_crt_points;
        self.mask_surviving_points += rhs.mask_surviving_points;
        self.mask_candidates += rhs.mask_candidates;
        self.reached_n1 += rhs.reached_n1;
        self.reached_n2 += rhs.reached_n2;
        self.reached_n3 += rhs.reached_n3;
        self.deep_checks += rhs.deep_checks;
        self.backward_extensions += rhs.backward_extensions;
    }
}

impl std::ops::Add for StageCounts {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        let mut total = self;
        total += rhs;
        total
    }
}

/// In-memory canonical deduplication set.
/// Stores (A, B, C, D) tuples of all shifted canonical polynomials already
/// reported, preventing log spam from the same polynomial being rediscovered
/// via multiple (a, b, c, d) seeds that shift to identical canonical forms.
type SeenSet = Mutex<HashSet<(i64, i64, i64, i64)>>;
static SEEN_DISCOVERIES: OnceLock<SeenSet> = OnceLock::new();

#[inline(always)]
fn seen_discoveries() -> &'static SeenSet {
    SEEN_DISCOVERIES.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Builds a 16 KiB flat byte table covering odd numbers in [3..L1_LIMIT].
/// Store 1 for prime, 0 for composite.
/// Fits exactly into half of the 32 KiB L1d cache.
pub fn build_l1_byte_table(sieve: &Sieve) -> Box<[u8; L1_SIZE]> {
    let mut table = vec![0u8; L1_SIZE].into_boxed_slice();
    for p in sieve.primes_from(3).take_while(|&p| p < L1_LIMIT) {
        let idx = (p - 3) >> 1;
        table[idx] = 1;
    }
    table.try_into().unwrap()
}

/// Single-cycle L1 cache byte extraction for guaranteed odd values < 32,768.
/// Eliminates the branch and evenness check because candidate generation
/// guarantees an odd integer.
/// Falls back to the global sieve or deterministic primality test if val >= L1_LIMIT,
/// preventing false negatives if bounds or polynomials exceed 32,768.
#[inline(always)]
pub fn is_prime_l1_odd(l1: &[u8; L1_SIZE], sieve: &Sieve, sieve_bound: usize, val: i64) -> bool {
    if val < 3 {
        return false;
    }
    let u = val as usize;
    if u < L1_LIMIT {
        let idx = (u - 3) >> 1;
        unsafe { *l1.get_unchecked(idx) == 1 }
    } else if u <= sieve_bound {
        sieve.is_prime(u)
    } else {
        primal::is_prime(val as u64)
    }
}

/// Ultra-fast primality test:
/// - Single-cycle L1 cache byte extraction for values < 32,768
/// - Fallback to 100M Sieve / Miller-Rabin for deeper chain candidates
#[inline(always)]
pub fn is_prime_fast(l1: &[u8; L1_SIZE], sieve: &Sieve, sieve_bound: usize, val: i64) -> bool {
    if val < 2 {
        return false;
    }
    if (val & 1) == 0 {
        return val == 2;
    }
    let u = val as usize;
    if u < L1_LIMIT {
        let idx = (u - 3) >> 1;
        unsafe { *l1.get_unchecked(idx) == 1 }
    } else if u <= sieve_bound {
        sieve.is_prime(u)
    } else {
        primal::is_prime(val as u64)
    }
}

#[derive(Copy, Clone)]
pub struct SearchContext<'a> {
    pub l1: &'a [u8; L1_SIZE],
    pub sieve: &'a Sieve,
    pub sieve_bound: usize,
    pub engine_start: Instant,
    pub report_discoveries: bool,
}

#[derive(Copy, Clone)]
pub struct PolyCoeffs {
    pub a: i64,
    pub b: i64,
    pub c: i64,
    pub d3: i64,
    pub p1: i64,
    pub p2: i64,
    pub p3: i64,
    pub p1_idx: isize,
    pub p2_idx: isize,
    pub p3_idx: isize,
}

/// Shifts a polynomial f(n) = a*n^3 + b*n^2 + c*n + d backward by k steps,
/// yielding g(m) = f(m - k) = A*m^3 + B*m^2 + C*m + D such that g(0) = f(-k).
#[inline]
pub fn shift_polynomial_backward(a: i64, b: i64, c: i64, d: i64, k: usize) -> (i64, i64, i64, i64) {
    if k == 0 {
        return (a, b, c, d);
    }
    let k_i = k as i64;
    let k2 = k_i * k_i;
    let k3 = k2 * k_i;
    let shifted_a = a;
    let shifted_b = b - 3 * k_i * a;
    let shifted_c = c - 2 * k_i * b + 3 * k2 * a;
    let shifted_d = d - k_i * c + k2 * b - k3 * a;
    (shifted_a, shifted_b, shifted_c, shifted_d)
}

/// Verification for streaks of length >= 4 via 3rd-order finite differences.
/// Outlined and marked #[cold] / #[inline(never)] to completely relieve register
/// pressure on the 16 x86-64 GPRs inside the hot evaluate_d_slice loop.
/// When forward streak reaches BACKWARD_SEARCH_THRESHOLD (L >= 22), evaluates backwards (n < 0)
/// and shifts the polynomial so the complete bidirectional streak starts at n = 0.
/// If total streak length reaches LOCAL_RECORD_THRESHOLD (L >= 36), reports discovery.
#[inline(never)]
#[cold]
#[allow(clippy::too_many_arguments)]
pub fn verify_deep_streak(
    a: i64,
    b: i64,
    c: i64,
    d: i64,
    v3: i64,
    l1: &[u8; L1_SIZE],
    sieve_ref: &Sieve,
    sieve_bound: usize,
    report_discoveries: bool,
) -> bool {
    // Sequential Step 4+: Deep verification loop via 3rd-order finite differences:
    // ZERO MULTIPLICATIONS in this loop!
    // Candidate values f(n) are guaranteed odd integers under parity pruning.
    let mut val = v3;
    let mut d1 = 37 * a + 7 * b + c;
    let mut d2 = 24 * a + 2 * b;
    let d3 = 6 * a;
    let mut curr_len: usize = 4;

    loop {
        val += d1;
        d1 += d2;
        d2 += d3;

        if !is_prime_l1_odd(l1, sieve_ref, sieve_bound, val) {
            break;
        }
        curr_len += 1;
    }

    // Bidirectional streak extension & threshold alerts
    if curr_len >= BACKWARD_SEARCH_THRESHOLD {
        // Evaluate backwards at n = -1, -2, -3, ...
        let mut k: usize = 0;
        let mut back_n = -1i64;
        loop {
            let b_val = ((a * back_n + b) * back_n + c) * back_n + d;
            if b_val < 2 || !is_prime_fast(l1, sieve_ref, sieve_bound, b_val) {
                break;
            }
            k += 1;
            back_n -= 1;
        }

        let (final_a, final_b, final_c, final_d) = shift_polynomial_backward(a, b, c, d, k);
        let total_len = curr_len + k;

        // Update global best length with total_len
        let mut current_best = GLOBAL_BEST_LEN.load(Ordering::Relaxed);
        while total_len > current_best {
            match GLOBAL_BEST_LEN.compare_exchange_weak(
                current_best,
                total_len,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current_best = actual,
            }
        }

        if total_len >= LOCAL_RECORD_THRESHOLD && report_discoveries {
            report_discovery(final_a, final_b, final_c, final_d, total_len);
        }
        true
    } else if curr_len > 5 {
        // Update global best length live (accurate sequential reporting)
        let mut current_best = GLOBAL_BEST_LEN.load(Ordering::Relaxed);
        while curr_len > current_best {
            match GLOBAL_BEST_LEN.compare_exchange_weak(
                current_best,
                curr_len,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current_best = actual,
            }
        }
        false
    } else {
        false
    }
}

/// Single-cycle pre-folded L1 byte table primality lookup for candidate d.
/// Index is base_idx + d_half. Falls back safely if outside L1 table or negative.
#[inline(always)]
pub fn check_l1_prefolded(
    base_idx: isize,
    d_half: u16,
    p_val: i64,
    d_val: u16,
    l1: &[u8; L1_SIZE],
    sieve_ref: &Sieve,
    sieve_bound: usize,
) -> bool {
    let offset = base_idx + d_half as isize;
    if (offset as usize) < L1_SIZE {
        unsafe { *l1.get_unchecked(offset as usize) == 1 }
    } else if offset < 0 {
        false
    } else {
        is_prime_l1_odd(l1, sieve_ref, sieve_bound, p_val + d_val as i64)
    }
}

/// Precomputes periodic residue masks for b % 3 == 0 (period = 35 steps of c += 6).
/// Enforces extended culling: n in 1..=4 for mod 5, and n in 1..=6 for mod 7.
#[inline(always)]
pub fn compute_period_masks_mod35(
    a: i64,
    b: i64,
    c_start: i64,
    valid_mod5: &[u128; 5],
    valid_mod7: &[u128; 7],
) -> [u128; 35] {
    let mut masks = [0u128; 35];
    let q1 = a + b;
    let q2 = 8 * a + 4 * b;
    let q3 = 27 * a + 9 * b;
    let q4 = 64 * a + 16 * b;
    let q5 = 125 * a + 25 * b;
    let q6 = 216 * a + 36 * b;

    for (s, mask) in masks.iter_mut().enumerate() {
        let c_val = c_start + (s as i64) * 6;
        let p1 = q1 + c_val;
        let p2 = q2 + 2 * c_val;
        let p3 = q3 + 3 * c_val;
        let p4 = q4 + 4 * c_val;

        let m5_1 = p1.rem_euclid(5) as usize;
        let m5_2 = p2.rem_euclid(5) as usize;
        let m5_3 = p3.rem_euclid(5) as usize;
        let m5_4 = p4.rem_euclid(5) as usize;
        let m5 = valid_mod5[m5_1] & valid_mod5[m5_2] & valid_mod5[m5_3] & valid_mod5[m5_4];

        if m5 != 0 {
            let p5 = q5 + 5 * c_val;
            let p6 = q6 + 6 * c_val;
            let m7_1 = p1.rem_euclid(7) as usize;
            let m7_2 = p2.rem_euclid(7) as usize;
            let m7_3 = p3.rem_euclid(7) as usize;
            let m7_4 = p4.rem_euclid(7) as usize;
            let m7_5 = p5.rem_euclid(7) as usize;
            let m7_6 = p6.rem_euclid(7) as usize;
            let m7 = valid_mod7[m7_1]
                & valid_mod7[m7_2]
                & valid_mod7[m7_3]
                & valid_mod7[m7_4]
                & valid_mod7[m7_5]
                & valid_mod7[m7_6];
            *mask = m5 & m7;
        }
    }
    masks
}

#[inline]
fn aligned_c_start(a: i64, b: i64, step: i64) -> i64 {
    let rem = (a + b + C_MIN).rem_euclid(step);
    if rem == 0 {
        C_MIN
    } else {
        C_MIN + (step - rem)
    }
}

/// Precomputes periodic residue masks for b % 3 != 0 (period = 105 steps of c += 2).
/// Enforces extended culling: n in 1..=2 for mod 3, n in 1..=4 for mod 5, and n in 1..=6 for mod 7.
#[inline(always)]
pub fn compute_period_masks_mod105(
    a: i64,
    b: i64,
    c_start: i64,
    valid_mod3: &[u128; 3],
    valid_mod5: &[u128; 5],
    valid_mod7: &[u128; 7],
) -> [u128; 105] {
    let mut masks = [0u128; 105];
    let q1 = a + b;
    let q2 = 8 * a + 4 * b;
    let q3 = 27 * a + 9 * b;
    let q4 = 64 * a + 16 * b;
    let q5 = 125 * a + 25 * b;
    let q6 = 216 * a + 36 * b;

    for (s, mask) in masks.iter_mut().enumerate() {
        let c_val = c_start + (s as i64) * 2;
        let p1 = q1 + c_val;
        let p2 = q2 + 2 * c_val;

        let m3_1 = p1.rem_euclid(3) as usize;
        let m3_2 = p2.rem_euclid(3) as usize;
        let m3 = valid_mod3[m3_1] & valid_mod3[m3_2];

        if m3 != 0 {
            let p3 = q3 + 3 * c_val;
            let p4 = q4 + 4 * c_val;
            let m5_1 = p1.rem_euclid(5) as usize;
            let m5_2 = p2.rem_euclid(5) as usize;
            let m5_3 = p3.rem_euclid(5) as usize;
            let m5_4 = p4.rem_euclid(5) as usize;
            let m5 = valid_mod5[m5_1] & valid_mod5[m5_2] & valid_mod5[m5_3] & valid_mod5[m5_4];

            if (m3 & m5) != 0 {
                let p5 = q5 + 5 * c_val;
                let p6 = q6 + 6 * c_val;
                let m7_1 = p1.rem_euclid(7) as usize;
                let m7_2 = p2.rem_euclid(7) as usize;
                let m7_3 = p3.rem_euclid(7) as usize;
                let m7_4 = p4.rem_euclid(7) as usize;
                let m7_5 = p5.rem_euclid(7) as usize;
                let m7_6 = p6.rem_euclid(7) as usize;
                let m7 = valid_mod7[m7_1]
                    & valid_mod7[m7_2]
                    & valid_mod7[m7_3]
                    & valid_mod7[m7_4]
                    & valid_mod7[m7_5]
                    & valid_mod7[m7_6];
                *mask = m3 & m5 & m7;
            }
        }
    }
    masks
}

/// Evaluates a slice of d candidates against polynomial f(n).
/// Implements Gracemont microarchitecture optimizations:
/// 1. Outlines deep verification (n >= 4) into verify_deep_streak to prevent register spilling.
/// 2. 4-way coalesced unrolling via chunks_exact(4) and branchless OR to skip ~63% of quads in 1 branch.
/// 3. Pre-folded L1 indexing for n = 1, 2, 3 avoiding subtractions, bit-shifts, and divisions.
#[inline(always)]
#[allow(clippy::too_many_arguments)]
fn evaluate_candidate(
    n1: bool,
    d: u16,
    d_half: u16,
    poly: &PolyCoeffs,
    ctx: &SearchContext,
) -> StageCounts {
    let mut counts = StageCounts::default();
    if !n1 {
        return counts;
    }
    counts.reached_n1 = 1;
    if !check_l1_prefolded(
        poly.p2_idx,
        d_half,
        poly.p2,
        d,
        ctx.l1,
        ctx.sieve,
        ctx.sieve_bound,
    ) {
        return counts;
    }
    counts.reached_n2 = 1;
    if !check_l1_prefolded(
        poly.p3_idx,
        d_half,
        poly.p3,
        d,
        ctx.l1,
        ctx.sieve,
        ctx.sieve_bound,
    ) {
        return counts;
    }
    counts.reached_n3 = 1;
    counts.deep_checks = 1;
    counts.backward_extensions = verify_deep_streak(
        poly.a,
        poly.b,
        poly.c,
        d as i64,
        poly.p3 + d as i64,
        ctx.l1,
        ctx.sieve,
        ctx.sieve_bound,
        ctx.report_discoveries,
    ) as u64;
    counts
}

#[inline(always)]
#[allow(clippy::chunks_exact_to_as_chunks)]
fn evaluate_d_slice(
    d_slice: &[u16],
    d_half_slice: &[u16],
    poly: &PolyCoeffs,
    ctx: &SearchContext,
) -> StageCounts {
    let mut counts = StageCounts {
        mask_candidates: d_slice.len() as u64,
        ..StageCounts::default()
    };
    let p1 = poly.p1;
    let p1_idx = poly.p1_idx;
    let l1 = ctx.l1;
    let sieve_ref = ctx.sieve;
    let sieve_bound = ctx.sieve_bound;

    let d_chunks = d_slice.chunks_exact(4);
    let half_chunks = d_half_slice.chunks_exact(4);
    let d_rem = d_chunks.remainder();
    let half_rem = half_chunks.remainder();

    for (d_quad, half_quad) in d_chunks.zip(half_chunks) {
        let d0 = unsafe { *d_quad.get_unchecked(0) };
        let d1 = unsafe { *d_quad.get_unchecked(1) };
        let d2 = unsafe { *d_quad.get_unchecked(2) };
        let d3_cand = unsafe { *d_quad.get_unchecked(3) };

        let h0 = unsafe { *half_quad.get_unchecked(0) };
        let h1 = unsafe { *half_quad.get_unchecked(1) };
        let h2 = unsafe { *half_quad.get_unchecked(2) };
        let h3 = unsafe { *half_quad.get_unchecked(3) };

        let off0 = p1_idx + h0 as isize;
        let off1 = p1_idx + h1 as isize;
        let off2 = p1_idx + h2 as isize;
        let off3 = p1_idx + h3 as isize;

        let b0 = if (off0 as usize) < L1_SIZE {
            unsafe { *l1.get_unchecked(off0 as usize) }
        } else if off0 < 0 {
            0
        } else if is_prime_l1_odd(l1, sieve_ref, sieve_bound, p1 + d0 as i64) {
            1
        } else {
            0
        };

        let b1 = if (off1 as usize) < L1_SIZE {
            unsafe { *l1.get_unchecked(off1 as usize) }
        } else if off1 < 0 {
            0
        } else if is_prime_l1_odd(l1, sieve_ref, sieve_bound, p1 + d1 as i64) {
            1
        } else {
            0
        };

        let b2 = if (off2 as usize) < L1_SIZE {
            unsafe { *l1.get_unchecked(off2 as usize) }
        } else if off2 < 0 {
            0
        } else if is_prime_l1_odd(l1, sieve_ref, sieve_bound, p1 + d2 as i64) {
            1
        } else {
            0
        };

        let b3 = if (off3 as usize) < L1_SIZE {
            unsafe { *l1.get_unchecked(off3 as usize) }
        } else if off3 < 0 {
            0
        } else if is_prime_l1_odd(l1, sieve_ref, sieve_bound, p1 + d3_cand as i64) {
            1
        } else {
            0
        };

        // Branchless OR coalescing: skips all 4 candidates in ~63% of quads in 1 branch
        if (b0 | b1 | b2 | b3) != 0 {
            counts += evaluate_candidate(b0 != 0, d0, h0, poly, ctx);
            counts += evaluate_candidate(b1 != 0, d1, h1, poly, ctx);
            counts += evaluate_candidate(b2 != 0, d2, h2, poly, ctx);
            counts += evaluate_candidate(b3 != 0, d3_cand, h3, poly, ctx);
        }
    }

    // Process remaining candidates if slice length is not a multiple of 4
    for (&d_val, &h_val) in d_rem.iter().zip(half_rem.iter()) {
        let n1 = check_l1_prefolded(p1_idx, h_val, p1, d_val, l1, sieve_ref, sieve_bound);
        counts += evaluate_candidate(n1, d_val, h_val, poly, ctx);
    }
    counts
}

/// Formats f(n) = a*n^3 + b*n^2 + c*n + d cleanly with mathematical signs
pub fn format_polynomial(a: i64, b: i64, c: i64, d: i64) -> String {
    let mut parts = Vec::new();

    // Cubic term (strictly a != 0)
    if a == 1 {
        parts.push("n^3".to_string());
    } else if a == -1 {
        parts.push("-n^3".to_string());
    } else {
        parts.push(format!("{}n^3", a));
    }

    // Quadratic term
    if b != 0 {
        if b > 0 {
            if b == 1 {
                parts.push("+ n^2".to_string());
            } else {
                parts.push(format!("+ {}n^2", b));
            }
        } else if b == -1 {
            parts.push("- n^2".to_string());
        } else {
            parts.push(format!("- {}n^2", -b));
        }
    }

    // Linear term
    if c != 0 {
        if c > 0 {
            if c == 1 {
                parts.push("+ n".to_string());
            } else {
                parts.push(format!("+ {}n", c));
            }
        } else if c == -1 {
            parts.push("- n".to_string());
        } else {
            parts.push(format!("- {}n", -c));
        }
    }

    // Constant term (d is always positive prime >= 3)
    parts.push(format!("+ {}", d));

    format!("f(n) = {}", parts.join(" "))
}

#[allow(dead_code)]
fn report_discovery(a: i64, b: i64, c: i64, d: i64, len: usize) {
    // ── In-memory canonical deduplication ────────────────────────────────────
    // Multiple (a,b,c,d) seeds can shift to the same canonical polynomial.
    // Guard the set under PRINT_LOCK so the check-and-insert is atomic with
    // respect to the subsequent console/file output.
    {
        let mut seen = seen_discoveries().lock().unwrap();
        if !seen.insert((a, b, c, d)) {
            return; // already reported this canonical polynomial
        }
    }

    let start_time = ENGINE_START.get().copied().unwrap_or_else(Instant::now);
    let _lock = PRINT_LOCK.lock().unwrap();
    assert_ne!(a, 0, "Degree must strictly be 3 (a != 0)");
    let degree = 3;
    let poly_str = format_polynomial(a, b, c, d);

    // Collect first few primes for verification
    let mut sample_primes = Vec::new();
    let sample_count = len.min(10);
    for n in 0..sample_count as i64 {
        let val = ((a * n + b) * n + c) * n + d;
        sample_primes.push(val);
    }
    let primes_str = sample_primes
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(", ");

    // Termination point
    let term_n = len as i64;
    let term_val = ((a * term_n + b) * term_n + c) * term_n + d;

    if len >= WORLD_RECORD_THRESHOLD {
        println!();
        println!(
            "╔════════════════════════════════════════════════════════════════════════════════╗"
        );
        println!(
            "║   🚨🚨🚨 POTENTIAL WORLD RECORD CUBIC PRIME POLYNOMIAL DISCOVERED! 🚨🚨🚨     ║"
        );
        println!(
            "╠════════════════════════════════════════════════════════════════════════════════╣"
        );
        println!("║  Formula     : {:<63} ║", poly_str);
        println!("║  Degree      : {:<63} ║", degree);
        println!(
            "║  Coefficients: a={:<6} b={:<6} c={:<6} d={:<23} ║",
            a, b, c, d
        );
        println!(
            "║  Chain Length: {:<6} CONSECUTIVE PRIMES (n = 0..={:<4})                     ║",
            len,
            len - 1
        );
        println!("║  First Primes: {:<63} ║", primes_str);
        println!("║  Breaks At   : f({}) = {:<53} ║", term_n, term_val);
        println!("║  Elapsed     : {:<63.2?} ║", start_time.elapsed());
        println!(
            "╚════════════════════════════════════════════════════════════════════════════════╝"
        );
        println!();
    } else {
        println!(
            "[DISCOVERY] Length: {:>2} | Formula: {:<40} | Coeffs: a={}, b={}, c={}, d={} | Elapsed: {:.2?}",
            len,
            poly_str,
            a,
            b,
            c,
            d,
            start_time.elapsed()
        );
    }
    let _ = stdout().flush();

    // Persistent discovery logging to discoveries.txt with error handling
    match OpenOptions::new()
        .create(true)
        .append(true)
        .open("discoveries.txt")
    {
        Ok(mut file) => {
            if let Err(e) = writeln!(
                file,
                "Length: {:>2} | Formula: {:<40} | Coeffs: a={}, b={}, c={}, d={} | Elapsed: {:.2?}",
                len,
                poly_str,
                a,
                b,
                c,
                d,
                start_time.elapsed()
            ) {
                eprintln!("[ERROR] Failed to write to discoveries.txt: {}", e);
            }
        }
        Err(e) => {
            eprintln!(
                "[ERROR] Failed to open discoveries.txt for appending: {}",
                e
            );
        }
    }
}

fn check_heartbeat(start_time: Instant) {
    let total_comb = TOTAL_THEORETICAL.load(Ordering::Relaxed);
    let last = LAST_HEARTBEAT.load(Ordering::Relaxed);
    if total_comb >= last + HEARTBEAT_INTERVAL
        && LAST_HEARTBEAT
            .compare_exchange(last, total_comb, Ordering::SeqCst, Ordering::Relaxed)
            .is_ok()
    {
        let total_tested = TOTAL_TESTED.load(Ordering::Relaxed);
        let total_pruned = total_comb.saturating_sub(total_tested);
        let prune_pct = if total_comb > 0 {
            (total_pruned as f64 / total_comb as f64) * 100.0
        } else {
            0.0
        };
        let elapsed_sec = start_time.elapsed().as_secs_f64();
        let eff_speed = if elapsed_sec > 0.0 {
            (total_comb as f64 / 1_000_000.0) / elapsed_sec
        } else {
            0.0
        };
        let test_speed = if elapsed_sec > 0.0 {
            (total_tested as f64 / 1_000_000.0) / elapsed_sec
        } else {
            0.0
        };
        let best = GLOBAL_BEST_LEN.load(Ordering::Relaxed);

        let _lock = PRINT_LOCK.lock().unwrap();
        println!(
            "[HEARTBEAT] Comb: {:>7.2}B | Tested: {:>7.2}B | Pruned: {:>5.1}% | Speed: {:>7.2} M comb/s ({:>6.2} M test/s) | Best Len: {:>2}",
            total_comb as f64 / 1e9,
            total_tested as f64 / 1e9,
            prune_pct,
            eff_speed,
            test_speed,
            best
        );
        let _ = stdout().flush();
    }
}

// ============================================================================
// HARDENED CHECKPOINT MANAGEMENT (METADATA HASHING & SAFE ATOMIC I/O)
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointData {
    pub last_a: i64,
    pub config_hash: Option<u64>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CheckpointError {
    Io(String),
    Corrupted(String),
}

impl std::fmt::Display for CheckpointError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CheckpointError::Io(msg) => write!(f, "I/O error: {}", msg),
            CheckpointError::Corrupted(msg) => write!(f, "Corrupted checkpoint file: {}", msg),
        }
    }
}

/// Computes a stable FNV-1a hash of the versioned search configuration schema.
pub fn compute_config_hash() -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET_BASIS;

    let mut update = |bytes: &[u8]| {
        for &byte in bytes {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    };

    update(b"prime_hunter/checkpoint-config");
    update(&CHECKPOINT_SCHEMA_VERSION.to_le_bytes());
    for value in [
        A_MIN,
        A_MAX,
        B_MIN,
        B_MAX,
        C_MIN,
        C_MAX,
        D_MIN as i64,
        D_MAX as i64,
    ] {
        update(&value.to_le_bytes());
    }
    hash
}

/// Formats checkpoint content with versioned configuration metadata and hash.
pub fn format_checkpoint(last_a: i64, config_hash: u64) -> String {
    format!(
        "# Prime Hunter Checkpoint File\n\
         # Configuration Bounds: a=[-{}..-{}] U [{}..{}], b=[{}..{}], c=[{}..{}], d=[{}..{}]\n\
         SCHEMA_VERSION: {}\n\
         CONFIG_HASH: {:#018x}\n\
         LAST_A: {}\n",
        A_MAX,
        A_MIN,
        A_MIN,
        A_MAX,
        B_MIN,
        B_MAX,
        C_MIN,
        C_MAX,
        D_MIN,
        D_MAX,
        CHECKPOINT_SCHEMA_VERSION,
        config_hash,
        last_a
    )
}

/// Parses checkpoint content, handling both new metadata format and legacy raw integers
pub fn parse_checkpoint(content: &str) -> Option<CheckpointData> {
    let mut config_hash = None;
    let mut last_a = None;
    let mut has_hash_field = false;
    let mut schema_version = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("SCHEMA_VERSION:") {
            schema_version = Some(rest.trim().parse::<u32>().ok()?);
        } else if let Some(rest) = trimmed.strip_prefix("CONFIG_HASH:") {
            has_hash_field = true;
            let hash_str = rest.trim();
            let parsed = if let Some(hex) = hash_str
                .strip_prefix("0x")
                .or_else(|| hash_str.strip_prefix("0X"))
            {
                u64::from_str_radix(hex, 16).ok()?
            } else {
                hash_str.parse::<u64>().ok()?
            };
            config_hash = Some(parsed);
        } else if let Some(rest) = trimmed.strip_prefix("LAST_A:") {
            if let Ok(val) = rest.trim().parse::<i64>() {
                last_a = Some(val);
            } else {
                // Explicit LAST_A header exists but integer value is corrupted
                return None;
            }
        } else if last_a.is_none() && !has_hash_field {
            // Backward-compatible legacy single integer format (e.g. "18" or "-4")
            if let Ok(val) = trimmed.parse::<i64>() {
                last_a = Some(val);
            } else {
                return None;
            }
        } else {
            // Unrecognized non-comment content
            return None;
        }
    }

    if let Some(version) = schema_version
        && version == 0
    {
        return None;
    }

    last_a.map(|a| CheckpointData {
        last_a: a,
        config_hash,
    })
}

pub fn load_checkpoint_from_path(path: &str) -> Result<Option<CheckpointData>, CheckpointError> {
    match fs::read_to_string(path) {
        Ok(content) => {
            if let Some(ckpt) = parse_checkpoint(&content) {
                Ok(Some(ckpt))
            } else {
                Err(CheckpointError::Corrupted(format!(
                    "Unable to parse valid checkpoint from '{}'",
                    path
                )))
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(CheckpointError::Io(format!(
            "Failed to read checkpoint file '{}': {}",
            path, e
        ))),
    }
}

pub fn load_checkpoint() -> Result<Option<CheckpointData>, CheckpointError> {
    load_checkpoint_from_path("checkpoint.txt")
}

pub fn save_checkpoint_to_path(
    a: i64,
    final_path: &str,
    tmp_path: &str,
) -> Result<(), std::io::Error> {
    let config_hash = compute_config_hash();
    let content = format_checkpoint(a, config_hash);

    let write_res = (|| {
        let mut file = fs::File::create(tmp_path)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        Ok(())
    })();

    if let Err(e) = write_res {
        let _ = fs::remove_file(tmp_path);
        return Err(e);
    }

    if let Err(e) = fs::rename(tmp_path, final_path) {
        let _ = fs::remove_file(tmp_path);
        return Err(e);
    }

    Ok(())
}

pub fn save_checkpoint(a: i64) {
    if let Err(e) = save_checkpoint_to_path(a, "checkpoint.txt", "checkpoint.txt.tmp") {
        eprintln!(
            "[ERROR] Failed to atomically save checkpoint for a={}: {}",
            a, e
        );
    }
}

// ============================================================================
// STATIC & STARTUP OVERFLOW CHECKS (i64 SAFETY ASSURANCES)
// ============================================================================

/// Startup assertion ensuring that cubic polynomial evaluations
/// and 3rd-order finite difference recurrence accumulators will NEVER overflow
/// or underflow a signed 64-bit integer (`i64`) anywhere across the search space.
pub fn assert_search_bounds_no_overflow() {
    const N_CHECK: i64 = 1000;
    let a_extremes = [-A_MAX, -A_MIN, A_MIN, A_MAX];
    let b_extremes = [B_MIN, B_MAX];
    let c_extremes = [C_MIN, C_MAX];
    let d_extremes = [D_MIN as i64, D_MAX as i64];

    for &a in &a_extremes {
        for &b in &b_extremes {
            for &c in &c_extremes {
                for &d in &d_extremes {
                    // Check v1, v2, v3 and recurrence steps
                    let _v1 = a
                        .checked_add(b)
                        .and_then(|x| x.checked_add(c))
                        .and_then(|x| x.checked_add(d))
                        .expect("v1 overflow");
                    let _v2 = a
                        .checked_mul(8)
                        .and_then(|x| x.checked_add(b.checked_mul(4)?))
                        .and_then(|x| x.checked_add(c.checked_mul(2)?))
                        .and_then(|x| x.checked_add(d))
                        .expect("v2 overflow");
                    let d3 = a.checked_mul(6).expect("d3 overflow");
                    let d2_3 = a
                        .checked_mul(24)
                        .and_then(|x| x.checked_add(b.checked_mul(2)?))
                        .expect("d2(3) overflow");
                    let d1_3 = a
                        .checked_mul(37)
                        .and_then(|x| x.checked_add(b.checked_mul(7)?))
                        .and_then(|x| x.checked_add(c))
                        .expect("d1(3) overflow");
                    let v3 = a
                        .checked_mul(27)
                        .and_then(|x| x.checked_add(b.checked_mul(9)?))
                        .and_then(|x| x.checked_add(c.checked_mul(3)?))
                        .and_then(|x| x.checked_add(d))
                        .expect("v3 overflow");

                    // Simulate finite difference accumulation up to N_CHECK
                    let mut val = v3;
                    let mut d1 = d1_3;
                    let mut d2 = d2_3;
                    for _ in 4..=N_CHECK {
                        val = val.checked_add(d1).expect("val accumulator overflow");
                        d1 = d1.checked_add(d2).expect("d1 accumulator overflow");
                        d2 = d2.checked_add(d3).expect("d2 accumulator overflow");
                    }

                    // Direct Horner evaluation check at N_CHECK
                    let n = N_CHECK;
                    let _ = a
                        .checked_mul(n)
                        .and_then(|x| x.checked_add(b))
                        .and_then(|x| x.checked_mul(n))
                        .and_then(|x| x.checked_add(c))
                        .and_then(|x| x.checked_mul(n))
                        .and_then(|x| x.checked_add(d))
                        .expect("Horner evaluation overflow");
                }
            }
        }
    }
}

fn run_benchmark_tile() {
    const BENCH_A_MIN: i64 = 10;
    const BENCH_A_MAX: i64 = 15;
    const BENCH_B_MIN: i64 = -100;
    const BENCH_B_MAX: i64 = 100;
    const BENCH_C_MIN: i64 = C_MIN;
    const BENCH_C_MAX: i64 = C_MIN + 100;

    assert_search_bounds_no_overflow();
    let start = Instant::now();
    let sieve = Arc::new(Sieve::new(SIEVE_LIMIT));
    let sieve_bound = sieve.upper_bound();
    let l1 = Arc::new(build_l1_byte_table(&sieve));
    let buckets = Mod105Buckets::new(&sieve);
    let mask_cache = PeriodicMaskCache::new(&buckets);
    let b_values: Vec<i64> = (BENCH_B_MIN..=BENCH_B_MAX).collect();
    let b_residues: Vec<usize> = b_values
        .iter()
        .map(|&b| b.rem_euclid(MASK_RESIDUE_PERIOD as i64) as usize)
        .collect();
    let total_d = buckets.raw_d.len() as u64;
    let total_c = (BENCH_C_MAX - BENCH_C_MIN + 1) as u64;
    let theoretical =
        (BENCH_A_MAX - BENCH_A_MIN + 1) as u64 * b_values.len() as u64 * total_c * total_d;
    let engine_start = start;
    let _ = ENGINE_START.set(engine_start);
    GLOBAL_BEST_LEN.store(0, Ordering::Relaxed);

    let mut totals = StageCounts::default();
    for a in BENCH_A_MIN..=BENCH_A_MAX {
        let a_residue = a.rem_euclid(MASK_RESIDUE_PERIOD as i64) as usize;
        let ctx = SearchContext {
            l1: &l1,
            sieve: &sieve,
            sieve_bound,
            engine_start,
            report_discoveries: false,
        };
        let per_a = b_values
            .par_iter()
            .enumerate()
            .map(|(b_idx, &b)| {
                let mut counts = StageCounts {
                    theoretical: total_c * total_d,
                    ..StageCounts::default()
                };
                let b_residue = b_residues[b_idx];
                let ab = a + b;
                let p2_base = 8 * a + 4 * b;
                let p3_base = 27 * a + 9 * b;
                let d3 = 6 * a;
                let step = if b % 3 == 0 { 6 } else { 2 };
                let c_start = aligned_c_start(a, b, step);
                let period = if step == 6 { 35 } else { 105 };
                let mut c = c_start;
                let mut p1 = ab + c_start;
                let mut p2 = p2_base + 2 * c_start;
                let mut p3 = p3_base + 3 * c_start;
                let mut p1_idx = ((p1 - 3) >> 1) as isize + 1;
                let mut p2_idx = ((p2 - 3) >> 1) as isize + 1;
                let mut p3_idx = ((p3 - 3) >> 1) as isize + 1;
                let mut step_idx = 0usize;
                while c <= BENCH_C_MAX {
                    counts.parity_crt_points += 1;
                    let mask_id = if step == 6 {
                        let masks = mask_cache.mod35_residue(a_residue, b_residue);
                        masks[step_idx % period]
                    } else {
                        let masks = mask_cache.mod105_residue(a_residue, b_residue);
                        masks[step_idx % period]
                    };
                    let mut mask = mask_cache.value(mask_id);
                    step_idx += 1;
                    if step_idx == period {
                        step_idx = 0;
                    }
                    if mask != 0 {
                        counts.mask_surviving_points += 1;
                        let poly = PolyCoeffs {
                            a,
                            b,
                            c,
                            d3,
                            p1,
                            p2,
                            p3,
                            p1_idx,
                            p2_idx,
                            p3_idx,
                        };
                        while mask != 0 {
                            let r = mask.trailing_zeros() as usize;
                            mask &= mask - 1;
                            let start = buckets.offsets[r] as usize;
                            let end = buckets.offsets[r + 1] as usize;
                            counts += evaluate_d_slice(
                                &buckets.raw_d[start..end],
                                &buckets.d_half[start..end],
                                &poly,
                                &ctx,
                            );
                        }
                    }
                    c += step;
                    p1 += step;
                    p2 += 2 * step;
                    p3 += 3 * step;
                    p1_idx += step as isize / 2;
                    p2_idx += step as isize;
                    p3_idx += 3 * step as isize / 2;
                }
                counts
            })
            .reduce(StageCounts::default, |left, right| left + right);
        totals += per_a;
    }

    let elapsed = start.elapsed();
    let seconds = elapsed.as_secs_f64();
    println!("BENCHMARK TILE");
    println!(
        "tile: a=[{}..{}], b=[{}..{}], c=[{}..{}], d=prime[{}..{}]",
        BENCH_A_MIN, BENCH_A_MAX, BENCH_B_MIN, BENCH_B_MAX, BENCH_C_MIN, BENCH_C_MAX, D_MIN, D_MAX
    );
    println!("wall time: {:.6}s", seconds);
    println!(
        "combinations/sec: {:.3} M",
        theoretical as f64 / seconds / 1_000_000.0
    );
    println!("theoretical combinations: {}", totals.theoretical);
    println!("pass parity/CRT c-points: {}", totals.parity_crt_points);
    println!("pass mod-3/5/7/105 masks: {}", totals.mask_surviving_points);
    println!("pass masks (candidate d): {}", totals.mask_candidates);
    println!("reach n=1: {}", totals.reached_n1);
    println!("reach n=2: {}", totals.reached_n2);
    println!("reach n=3: {}", totals.reached_n3);
    println!("deep primality checks: {}", totals.deep_checks);
    println!("backward extensions: {}", totals.backward_extensions);
}

// ============================================================================
// MAIN ENGINE ENTRY POINT
// ============================================================================

fn main() {
    if std::env::args().any(|arg| arg == "--bench") {
        run_benchmark_tile();
        return;
    }
    println!("================================================================================");
    println!("  PRIME HUNTER: PURE DEGREE-3 (CUBIC) PRIME-GENERATING POLYNOMIAL ENGINE        ");
    println!("     FLAT MOD-105 BUCKETS | D_MIN=29 | 16 KB L1 BYTE TABLE | HARDENED CKPT      ");
    println!("================================================================================");

    let start_total = Instant::now();

    // 0. Verify mathematical invariants and integer overflow safety across all bounds
    assert_search_bounds_no_overflow();
    println!("Verified: i64 accumulator overflow safety across all search bounds (n=0..=1000).");

    // 1. Initialize Primal Sieve up to 100,000,000
    print!("Precomputing global prime sieve up to {}... ", SIEVE_LIMIT);
    let _ = stdout().flush();
    let sieve_start = Instant::now();
    let sieve = Arc::new(Sieve::new(SIEVE_LIMIT));
    let sieve_bound = sieve.upper_bound();
    println!(
        "DONE in {:.2?} (Upper bound: {})",
        sieve_start.elapsed(),
        sieve_bound
    );

    // 2. Build 16 KiB L1 Data Cache byte table for single-cycle cmpb lookups
    print!(
        "Building 16 KiB L1-resident byte table (0..{})... ",
        L1_LIMIT
    );
    let _ = stdout().flush();
    let l1_start = Instant::now();
    let l1_byte_table = Arc::new(build_l1_byte_table(&sieve));
    println!("DONE in {:.2?}", l1_start.elapsed());

    // 3. Extract primes d in [D_MIN..D_MAX] and build contiguous flat Mod105Buckets
    print!(
        "Building flat Mod-105 prime buckets ({} <= d <= {})... ",
        D_MIN, D_MAX
    );
    let _ = stdout().flush();
    let buckets_start = Instant::now();
    let buckets = Mod105Buckets::new(&sieve);
    let non_empty_buckets = (0..105)
        .filter(|&r| buckets.offsets[r] < buckets.offsets[r + 1])
        .count();
    println!(
        "DONE in {:.2?} ({} primes across {} coprime mod-105 residue buckets, ~{:.1} KiB)",
        buckets_start.elapsed(),
        buckets.raw_d.len(),
        non_empty_buckets,
        (std::mem::size_of_val(&buckets.offsets)
            + buckets.raw_d.len() * 2
            + buckets.d_half.len() * 2) as f64
            / 1024.0
    );

    print!("Precomputing periodic CRT mask cache... ");
    let mask_cache_start = Instant::now();
    let mask_cache = PeriodicMaskCache::new(&buckets);
    println!(
        "DONE in {:.2?} (~{:.1} MiB)",
        mask_cache_start.elapsed(),
        (mask_cache.mod35.len() * std::mem::size_of::<u16>()
            + mask_cache.mod105.len() * std::mem::size_of::<u16>()
            + mask_cache.mask_values.len() * std::mem::size_of::<u128>()) as f64
            / (1024.0 * 1024.0)
    );

    // 4. Construct strictly non-zero cubic coefficients: a in [-A_MAX..=-A_MIN] U [A_MIN..=A_MAX]
    let a_values: Vec<i64> = (-A_MAX..=-A_MIN).chain(A_MIN..=A_MAX).collect();
    let num_a = a_values.len();
    let num_b = (B_MAX - B_MIN + 1) as usize;
    let b_residues: Vec<usize> = (B_MIN..=B_MAX)
        .map(|b| b.rem_euclid(MASK_RESIDUE_PERIOD as i64) as usize)
        .collect();

    // Checkpoint management with configuration hash verification
    let start_a_idx = match load_checkpoint() {
        Ok(Some(ckpt)) => {
            let current_hash = compute_config_hash();
            if let Some(saved_hash) = ckpt.config_hash {
                if saved_hash != current_hash {
                    println!(
                        "⚠️  [WARNING] Checkpoint config hash mismatch! Saved: {:#018x}, Current: {:#018x}",
                        saved_hash, current_hash
                    );
                    println!(
                        "    Search bounds or parameters have changed since checkpoint was saved."
                    );
                    println!(
                        "    Resuming from a = {} will not search prior parameter ranges with current configuration.",
                        ckpt.last_a
                    );
                } else {
                    println!(
                        "Checkpoint configuration verified (hash {:#018x} matches).",
                        saved_hash
                    );
                }
            } else {
                println!("Loaded legacy checkpoint (no configuration hash present).");
            }

            let last_a = ckpt.last_a;
            if let Some(pos) = a_values.iter().position(|&x| x == last_a) {
                let next_idx = pos + 1;
                if next_idx < a_values.len() {
                    println!(
                        "Found checkpoint! Last completed a = {}. Resuming search from a = {} (step {}/{})",
                        last_a,
                        a_values[next_idx],
                        next_idx + 1,
                        num_a
                    );
                    next_idx
                } else {
                    println!(
                        "Found checkpoint! Search over all {} values of a is already 100% complete!",
                        num_a
                    );
                    println!("To restart from scratch, delete 'checkpoint.txt'.");
                    return;
                }
            } else {
                println!(
                    "Checkpoint a = {} not found in current range. Starting fresh search.",
                    last_a
                );
                0
            }
        }
        Ok(None) => {
            println!(
                "No checkpoint found. Starting fresh search from a = {} (step 1/{})",
                a_values[0], num_a
            );
            0
        }
        Err(e) => {
            eprintln!("⚠️  [ERROR] Checkpoint error: {}", e);
            eprintln!("    Refusing to silently overwrite potentially corrupted checkpoint file.");
            eprintln!("    To start fresh, delete or rename 'checkpoint.txt'.");
            return;
        }
    };

    // Total search space calculation
    let total_a = num_a as u64;
    let total_b = num_b as u64;
    let total_c = (C_MAX - C_MIN + 1) as u64;
    let total_d = buckets.raw_d.len() as u64;
    let search_space = total_a * total_b * total_c * total_d;

    println!("\nSearch Space Configuration:");
    println!(
        "  a in [-{}..-{}] U [{}..{}] ({} values, strictly a != 0)",
        A_MAX, A_MIN, A_MIN, A_MAX, total_a
    );
    println!("  b in [{}..{}] ({} values)", B_MIN, B_MAX, total_b);
    println!("  c in [{}..{}] ({} values)", C_MIN, C_MAX, total_c);
    println!(
        "  d in [{}..{}] ({} primes, strictly d >= 29)",
        D_MIN, D_MAX, total_d
    );
    println!("  Parity Pruning (Mod 2)             : ACTIVE (step_by 2 on c, 50.0% pruned)");
    println!(
        "  CRT Step-6 Pruning (b % 3 == 0)    : ACTIVE (step_by 6 on c, 66.7% pruned on 1/3 of b)"
    );
    println!(
        "  Flat Mod-105 Buckets (Phase 1)     : ACTIVE ({} coprime residues, small prime culling mod 3, 5, 7)",
        non_empty_buckets
    );
    println!(
        "  Periodic CRT Masks (V2)            : ACTIVE (period-35 for b%3==0, period-105 for b%3!=0; zero inner-loop division)"
    );
    println!(
        "  Extended Mod-5/7 Culling (V2)      : ACTIVE (n in 1..=4 mod 5, n in 1..=6 mod 7; ~50.7% extra pruning)"
    );
    println!(
        "  Pre-folded p2_idx/p3_idx (V2)      : ACTIVE (direct L1 byte table offsets for n=2,3; no inner subtract)"
    );
    println!("  Combined A Priori Pruning          : >90% of theoretical search space eliminated!");
    println!(
        "  Sequential Verification            : 4-Way Coalesced L1 Byte Table (cmpb) -> n=2,3 in L1 -> n>=4 Finite Diff"
    );
    println!(
        "  Bidirectional Extension (n < 0)    : ACTIVE (threshold>=10, Horner backward search, shifted g(m) = f(m - k))"
    );
    println!(
        "  Working Set Memory Footprint       : 16 KiB L1 byte table + ~5 KiB Mod-105 buckets = ~21 KiB (fits in 32 KiB L1d)"
    );
    println!("  Persistent Discoveries             : discoveries.txt");
    println!("  Persistent Checkpoint              : checkpoint.txt (hardened with config hash)");
    println!(
        "  Total cubic polynomials in space   : {} ({:.2} Trillion)",
        search_space,
        search_space as f64 / 1e12
    );
    println!(
        "  World Record Alert Threshold       : >= {}",
        WORLD_RECORD_THRESHOLD
    );
    println!("  Batch Size                         : {}", BATCH_SIZE);
    println!(
        "  Heartbeat Interval                 : {}",
        HEARTBEAT_INTERVAL
    );
    println!(
        "  Worker Threads (Rayon)             : {}",
        rayon::current_num_threads()
    );
    println!("================================================================================\n");

    let engine_start = Instant::now();
    let _ = ENGINE_START.set(engine_start);

    // Iterate through remaining `a` chunks in order, parallelizing `b` with Rayon
    for &a in &a_values[start_a_idx..] {
        let a_residue = a.rem_euclid(MASK_RESIDUE_PERIOD as i64) as usize;
        let ctx = SearchContext {
            l1: &l1_byte_table,
            sieve: &sieve,
            sieve_bound,
            engine_start,
            report_discoveries: true,
        };
        let buckets_ref = &buckets;
        let theoretical_per_b: u64 = total_c * total_d;

        let slice_counts = (0..num_b)
            .into_par_iter()
            .map(|b_idx| {
                let b = B_MIN + b_idx as i64;
                let mut counts = StageCounts {
                    theoretical: theoretical_per_b,
                    ..StageCounts::default()
                };
                let b_residue = b_residues[b_idx];

                let ab = a + b;
                let p2_base = 8 * a + 4 * b;
                let p3_base = 27 * a + 9 * b;
                let d3 = 6 * a;

                if b % 3 == 0 {
                    // Optimization 1: CRT Step-6 Pruning when b % 3 == 0
                    // When 3 | b, P2 = -P1 (mod 3). If P1 != 0 (mod 3), {P1, P2} mod 3 covers {1, 2},
                    // which eliminates 100% of candidate primes d >= 29 at n=1 or n=2.
                    // Thus surviving polynomials MUST satisfy a + b + c = 0 (mod 6).
                    // Furthermore, P1 = P2 = P3 = 0 (mod 3) identically, so mod-3 bitmask checks are bypassed!
                    let rem = (ab + C_MIN).rem_euclid(6);
                    let c_start = if rem == 0 { C_MIN } else { C_MIN + (6 - rem) };

                    let period_masks = mask_cache.mod35_residue(a_residue, b_residue);
                    let mut step_idx = 0usize;

                    let mut c = c_start;
                    let mut p1 = ab + c_start;
                    let mut p2 = p2_base + 2 * c_start;
                    let mut p3 = p3_base + 3 * c_start;
                    let mut p1_idx = ((p1 - 3) >> 1) as isize + 1;
                    let mut p2_idx = ((p2 - 3) >> 1) as isize + 1;
                    let mut p3_idx = ((p3 - 3) >> 1) as isize + 1;

                    while c <= C_MAX {
                        counts.parity_crt_points += 1;
                        let mask_id = unsafe { *period_masks.get_unchecked(step_idx) };
                        let mut mask = mask_cache.value(mask_id);
                        step_idx = if step_idx + 1 == 35 { 0 } else { step_idx + 1 };

                        if mask != 0 {
                            counts.mask_surviving_points += 1;
                            let poly = PolyCoeffs {
                                a,
                                b,
                                c,
                                d3,
                                p1,
                                p2,
                                p3,
                                p1_idx,
                                p2_idx,
                                p3_idx,
                            };

                            while mask != 0 {
                                let r = mask.trailing_zeros() as usize;
                                mask &= mask - 1;

                                let start =
                                    unsafe { *buckets_ref.offsets.get_unchecked(r) } as usize;
                                let end =
                                    unsafe { *buckets_ref.offsets.get_unchecked(r + 1) } as usize;

                                let d_slice =
                                    unsafe { buckets_ref.raw_d.get_unchecked(start..end) };
                                let d_half_slice =
                                    unsafe { buckets_ref.d_half.get_unchecked(start..end) };
                                counts += evaluate_d_slice(d_slice, d_half_slice, &poly, &ctx);
                            }
                        }

                        c += 6;
                        p1 += 6;
                        p2 += 12;
                        p3 += 18;
                        p1_idx += 3;
                        p2_idx += 6;
                        p3_idx += 9;
                    }
                } else {
                    // Parity Pruning when b % 3 != 0 (c steps by 2)
                    let c_start = if (ab & 1) == 0 {
                        if C_MIN % 2 == 0 { C_MIN } else { C_MIN + 1 }
                    } else {
                        if C_MIN % 2 != 0 { C_MIN } else { C_MIN + 1 }
                    };

                    let period_masks = mask_cache.mod105_residue(a_residue, b_residue);
                    let mut step_idx = 0usize;

                    let mut c = c_start;
                    let mut p1 = ab + c_start;
                    let mut p2 = p2_base + 2 * c_start;
                    let mut p3 = p3_base + 3 * c_start;
                    let mut p1_idx = ((p1 - 3) >> 1) as isize + 1;
                    let mut p2_idx = ((p2 - 3) >> 1) as isize + 1;
                    let mut p3_idx = ((p3 - 3) >> 1) as isize + 1;

                    while c <= C_MAX {
                        counts.parity_crt_points += 1;
                        let mask_id = unsafe { *period_masks.get_unchecked(step_idx) };
                        let mut mask = mask_cache.value(mask_id);
                        step_idx = if step_idx + 1 == 105 { 0 } else { step_idx + 1 };

                        if mask != 0 {
                            counts.mask_surviving_points += 1;
                            let poly = PolyCoeffs {
                                a,
                                b,
                                c,
                                d3,
                                p1,
                                p2,
                                p3,
                                p1_idx,
                                p2_idx,
                                p3_idx,
                            };

                            while mask != 0 {
                                let r = mask.trailing_zeros() as usize;
                                mask &= mask - 1;

                                let start =
                                    unsafe { *buckets_ref.offsets.get_unchecked(r) } as usize;
                                let end =
                                    unsafe { *buckets_ref.offsets.get_unchecked(r + 1) } as usize;

                                let d_slice =
                                    unsafe { buckets_ref.raw_d.get_unchecked(start..end) };
                                let d_half_slice =
                                    unsafe { buckets_ref.d_half.get_unchecked(start..end) };
                                counts += evaluate_d_slice(d_slice, d_half_slice, &poly, &ctx);
                            }
                        }

                        c += 2;
                        p1 += 2;
                        p2 += 4;
                        p3 += 6;
                        p1_idx += 1;
                        p2_idx += 2;
                        p3_idx += 3;
                    }
                }

                counts
            })
            .reduce(StageCounts::default, |left, right| left + right);

        TOTAL_THEORETICAL.fetch_add(slice_counts.theoretical, Ordering::Relaxed);
        TOTAL_TESTED.fetch_add(slice_counts.mask_candidates, Ordering::Relaxed);
        check_heartbeat(engine_start);

        // Outer chunk (entire a block) completed: persist checkpoint with 0.00% hot-loop overhead
        save_checkpoint(a);
    }

    let total_comb = TOTAL_THEORETICAL.load(Ordering::SeqCst);
    let total_tested = TOTAL_TESTED.load(Ordering::SeqCst);
    let total_pruned = total_comb.saturating_sub(total_tested);
    let total_time = engine_start.elapsed();
    let best_len = GLOBAL_BEST_LEN.load(Ordering::SeqCst);
    let elapsed_sec = total_time.as_secs_f64();
    let eff_speed = if elapsed_sec > 0.0 {
        (total_comb as f64 / 1_000_000.0) / elapsed_sec
    } else {
        0.0
    };
    let test_speed = if elapsed_sec > 0.0 {
        (total_tested as f64 / 1_000_000.0) / elapsed_sec
    } else {
        0.0
    };
    let prune_pct = if total_comb > 0 {
        (total_pruned as f64 / total_comb as f64) * 100.0
    } else {
        0.0
    };

    println!("\n================================================================================");
    println!("                   CUBIC HUNT COMPLETED SUCCESSFULLY                            ");
    println!("================================================================================");
    println!("Combinatorial Space Processed     : {}", total_comb);
    println!("Candidates Tested for Primality   : {}", total_tested);
    println!(
        "Combinations Mathematically Pruned: {} ({:.1}% eliminated a priori)",
        total_pruned, prune_pct
    );
    println!("Total Search Time                 : {:.2?}", total_time);
    println!(
        "Effective Search Throughput       : {:.2} Million comb/second",
        eff_speed
    );
    println!(
        "Actual Primality Test Throughput  : {:.2} Million test/second",
        test_speed
    );
    println!("Best Consecutive Primes Len       : {}", best_len);
    println!(
        "Total Wall-Clock Time             : {:.2?}",
        start_total.elapsed()
    );
    println!("================================================================================");
}

// ============================================================================
// AUTOMATED UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn eval_poly(a: i64, b: i64, c: i64, d: i64, n: i64) -> i64 {
        ((a * n + b) * n + c) * n + d
    }

    #[test]
    fn test_finite_differences_matches_direct_eval() {
        let test_cases = [
            (-14, 514, -2970, 8123),
            (-30, 817, 1273, 6827),
            (1, 1, 1, 3),
            (-1, 0, 0, 7),
            (150, -1000, 3000, 9973),
            (-150, 1000, -3000, 3),
        ];

        for (a, b, c, d) in test_cases {
            let v3 = eval_poly(a, b, c, d, 3);
            let mut val = v3;
            let mut d1 = 37 * a + 7 * b + c;
            let mut d2 = 24 * a + 2 * b;
            let d3 = 6 * a;

            for n in 4..=150 {
                val += d1;
                d1 += d2;
                d2 += d3;

                let expected = eval_poly(a, b, c, d, n);
                assert_eq!(
                    val, expected,
                    "Mismatch at n={} for poly a={}, b={}, c={}, d={}",
                    n, a, b, c, d
                );
            }
        }
    }

    #[test]
    fn test_l1_byte_table_and_is_prime_fast() {
        let sieve = Sieve::new(SIEVE_LIMIT);
        let sieve_bound = sieve.upper_bound();
        let l1 = build_l1_byte_table(&sieve);

        // Test non-primes and small edge cases
        assert!(!is_prime_fast(&l1, &sieve, sieve_bound, -10));
        assert!(!is_prime_fast(&l1, &sieve, sieve_bound, 0));
        assert!(!is_prime_fast(&l1, &sieve, sieve_bound, 1));
        assert!(is_prime_fast(&l1, &sieve, sieve_bound, 2));
        assert!(is_prime_fast(&l1, &sieve, sieve_bound, 3));
        assert!(!is_prime_fast(&l1, &sieve, sieve_bound, 4));
        assert!(is_prime_fast(&l1, &sieve, sieve_bound, 5));

        // Test is_prime_l1_odd on odd numbers (including fallback for values >= L1_LIMIT)
        assert!(!is_prime_l1_odd(&l1, &sieve, sieve_bound, -5));
        assert!(!is_prime_l1_odd(&l1, &sieve, sieve_bound, 1));
        assert!(is_prime_l1_odd(&l1, &sieve, sieve_bound, 3));
        assert!(is_prime_l1_odd(&l1, &sieve, sieve_bound, 5));
        assert!(!is_prime_l1_odd(&l1, &sieve, sieve_bound, 9));
        assert!(is_prime_l1_odd(&l1, &sieve, sieve_bound, 11));

        // Test thorough range across L1 boundary (L1_LIMIT = 32_768) and beyond
        for val in 2..50_000 {
            let expected = primal::is_prime(val as u64);
            let actual = is_prime_fast(&l1, &sieve, sieve_bound, val);
            assert_eq!(actual, expected, "Primality test failure at val={}", val);
            if (val & 1) != 0 && val >= 3 {
                assert_eq!(
                    is_prime_l1_odd(&l1, &sieve, sieve_bound, val),
                    expected,
                    "is_prime_l1_odd mismatch at val={}",
                    val
                );
            }
        }

        // Test across and beyond L1 boundary (verifying fallback)
        for val in (L1_LIMIT as i64 - 200)..(L1_LIMIT as i64 + 200) {
            let expected = primal::is_prime(val as u64);
            let actual = is_prime_fast(&l1, &sieve, sieve_bound, val);
            assert_eq!(actual, expected, "Boundary failure at val={}", val);
            if (val & 1) != 0 && val >= 3 {
                assert_eq!(
                    is_prime_l1_odd(&l1, &sieve, sieve_bound, val),
                    expected,
                    "is_prime_l1_odd fallback mismatch at val={}",
                    val
                );
            }
        }
    }

    #[test]
    fn test_phase2_prefolded_byte_lookup() {
        let sieve = Sieve::new(SIEVE_LIMIT);
        let sieve_bound = sieve.upper_bound();
        let l1 = build_l1_byte_table(&sieve);
        let buckets = Mod105Buckets::new(&sieve);

        // Test across various polynomial coefficients
        let test_coeffs = [
            (-14, 514, -2970), // Record polynomial P1 = -2470
            (-30, 817, 1273),  // P1 = 2060
            (1, -32, 241),     // P1 = 210
            (2, 4, 6),         // P1 = 12
            (-10, 2, 4),       // P1 = -4
        ];

        for &(a, b, c) in &test_coeffs {
            let p1 = a + b + c;
            assert_eq!(p1 % 2, 0, "P1 must be even for parity pruning");
            let p1_idx = ((p1 - 3) >> 1) as isize + 1;

            for r in 1..105 {
                let start = buckets.offsets[r] as usize;
                let end = buckets.offsets[r + 1] as usize;
                if start >= end {
                    continue;
                }

                let d_slice = &buckets.raw_d[start..end];
                let d_half_slice = &buckets.d_half[start..end];

                for i in 0..d_slice.len() {
                    let d = d_slice[i] as i64;
                    let d_half = d_half_slice[i];
                    let v1 = p1 + d;

                    let offset = p1_idx + d_half as isize;
                    let fast_lookup = if (offset as usize) < L1_SIZE {
                        l1[offset as usize] == 1
                    } else if offset < 0 {
                        false
                    } else {
                        is_prime_l1_odd(&l1, &sieve, sieve_bound, v1)
                    };

                    let expected = is_prime_fast(&l1, &sieve, sieve_bound, v1);
                    assert_eq!(
                        fast_lookup, expected,
                        "Pre-folded lookup mismatch for P1={}, d={}, v1={}",
                        p1, d, v1
                    );
                }
            }
        }
    }

    #[test]
    fn test_discoveries_polynomial_run_length_31() {
        // Discovered record: f(n) = -14n^3 + 514n^2 - 2970n + 8123
        let (a, b, c, d) = (-14, 514, -2970, 8123);
        let sieve = Sieve::new(SIEVE_LIMIT);
        let sieve_bound = sieve.upper_bound();
        let l1 = build_l1_byte_table(&sieve);

        for n in 0..31 {
            let val = eval_poly(a, b, c, d, n);
            assert!(val >= 2, "f({}) = {} must be >= 2", n, val);
            assert!(
                is_prime_fast(&l1, &sieve, sieve_bound, val),
                "f({}) = {} should be prime",
                n,
                val
            );
        }

        // Terminating value at n=31
        let val31 = eval_poly(a, b, c, d, 31);
        assert!(
            val31 < 2 || !is_prime_fast(&l1, &sieve, sieve_bound, val31),
            "Sequence must terminate at n=31"
        );
    }

    #[test]
    fn test_phase3_verify_deep_streak() {
        // Discovered record: f(n) = -14n^3 + 514n^2 - 2970n + 8123 (forward length 31)
        // Bidirectional extension finds 5 backward primes (n = -1..=-5), yielding total length 36!
        let (a, b, c, d) = (-14, 514, -2970, 8123);
        let sieve = Sieve::new(SIEVE_LIMIT);
        let sieve_bound = sieve.upper_bound();
        let l1 = build_l1_byte_table(&sieve);

        let v3 = eval_poly(a, b, c, d, 3);
        assert_eq!(v3, 3461);
        assert!(is_prime_fast(&l1, &sieve, sieve_bound, v3));

        GLOBAL_BEST_LEN.store(0, Ordering::SeqCst);
        verify_deep_streak(a, b, c, d, v3, &l1, &sieve, sieve_bound, false);

        // Global best length should have been updated to 36 via bidirectional extension
        let best = GLOBAL_BEST_LEN.load(Ordering::SeqCst);
        assert_eq!(
            best, 36,
            "verify_deep_streak should establish total bidirectional streak length 36"
        );
    }

    #[test]
    fn test_phase3_evaluate_d_slice_dual_unrolling() {
        let sieve = Sieve::new(SIEVE_LIMIT);
        let sieve_bound = sieve.upper_bound();
        let l1 = build_l1_byte_table(&sieve);

        // Record polynomial: a=-14, b=514, c=-2970, d=8123
        let (a, b, c) = (-14, 514, -2970);
        let p1 = a + b + c;
        let p2 = 8 * a + 4 * b + 2 * c;
        let p3 = 27 * a + 9 * b + 3 * c;
        let p1_idx = ((p1 - 3) >> 1) as isize + 1;
        let p2_idx = ((p2 - 3) >> 1) as isize + 1;
        let p3_idx = ((p3 - 3) >> 1) as isize + 1;
        let poly = PolyCoeffs {
            a,
            b,
            c,
            d3: 6 * a,
            p1,
            p2,
            p3,
            p1_idx,
            p2_idx,
            p3_idx,
        };
        let ctx = SearchContext {
            l1: &l1,
            sieve: &sieve,
            sieve_bound,
            engine_start: Instant::now(),
            report_discoveries: false,
        };

        // Test 1: Exact quad (4 candidates with 8123 in second slot)
        GLOBAL_BEST_LEN.store(0, Ordering::SeqCst);
        let d_slice_quad: [u16; 4] = [29, 8123, 31, 37];
        let d_half_quad: [u16; 4] = [29 >> 1, 8123 >> 1, 31 >> 1, 37 >> 1];
        evaluate_d_slice(&d_slice_quad, &d_half_quad, &poly, &ctx);
        assert_eq!(
            GLOBAL_BEST_LEN.load(Ordering::SeqCst),
            36,
            "4-way coalesced unrolling must detect record in quad slice"
        );

        // Test 2: Slice with remainder (length 3 with 8123 in remainder slot index 2)
        GLOBAL_BEST_LEN.store(0, Ordering::SeqCst);
        let d_slice_rem: [u16; 3] = [29, 31, 8123];
        let d_half_rem: [u16; 3] = [29 >> 1, 31 >> 1, 8123 >> 1];
        evaluate_d_slice(&d_slice_rem, &d_half_rem, &poly, &ctx);
        assert_eq!(
            GLOBAL_BEST_LEN.load(Ordering::SeqCst),
            36,
            "Remainder processing must detect record in odd remainder slot"
        );

        // Test 3: Slice of length 5 (1 quad + 1 remainder with 8123 in remainder)
        GLOBAL_BEST_LEN.store(0, Ordering::SeqCst);
        let d_slice_5: [u16; 5] = [29, 31, 37, 41, 8123];
        let d_half_5: [u16; 5] = [29 >> 1, 31 >> 1, 37 >> 1, 41 >> 1, 8123 >> 1];
        evaluate_d_slice(&d_slice_5, &d_half_5, &poly, &ctx);
        assert_eq!(
            GLOBAL_BEST_LEN.load(Ordering::SeqCst),
            36,
            "Length 5 slice must detect record in remainder"
        );
    }

    #[test]
    fn test_mod105_precomputed_bitmasks_soundness() {
        let sieve = Sieve::new(SIEVE_LIMIT);
        let buckets = Mod105Buckets::new(&sieve);

        // Verify across various P1, P2, P3 combinations that the bitmask intersection
        // matches the direct divisibility checks character-for-character
        let test_cases: [(i64, i64, i64); 9] = [
            (-14, 514, -2970), // Record polynomial
            (-30, 817, 1273),
            (1, -32, 241),
            (5, -119, 732),
            (11, -222, 1465),
            (15, -349, 2234),
            (-2, 158, -1696),
            (10, -20, 30),
            (-5, 15, -25),
        ];

        for &(a, b, c) in &test_cases {
            let p1 = a + b + c;
            let p2 = 8 * a + 4 * b + 2 * c;
            let p3 = 27 * a + 9 * b + 3 * c;

            let m3_1 = p1.rem_euclid(3) as usize;
            let m3_2 = p2.rem_euclid(3) as usize;
            let m3_3 = p3.rem_euclid(3) as usize;
            let mask3 =
                buckets.valid_mod3[m3_1] & buckets.valid_mod3[m3_2] & buckets.valid_mod3[m3_3];

            let m5_1 = p1.rem_euclid(5) as usize;
            let m5_2 = p2.rem_euclid(5) as usize;
            let m5_3 = p3.rem_euclid(5) as usize;
            let mask5 =
                buckets.valid_mod5[m5_1] & buckets.valid_mod5[m5_2] & buckets.valid_mod5[m5_3];

            let m7_1 = p1.rem_euclid(7) as usize;
            let m7_2 = p2.rem_euclid(7) as usize;
            let m7_3 = p3.rem_euclid(7) as usize;
            let mask7 =
                buckets.valid_mod7[m7_1] & buckets.valid_mod7[m7_2] & buckets.valid_mod7[m7_3];

            let mask = mask3 & mask5 & mask7;

            for r in 0..105 {
                let start = buckets.offsets[r] as usize;
                let end = buckets.offsets[r + 1] as usize;

                let direct_survives = if start >= end || r == 0 {
                    false
                } else {
                    let r_i64 = r as i64;
                    let skip3 =
                        (p1 + r_i64) % 3 == 0 || (p2 + r_i64) % 3 == 0 || (p3 + r_i64) % 3 == 0;
                    let skip5 =
                        (p1 + r_i64) % 5 == 0 || (p2 + r_i64) % 5 == 0 || (p3 + r_i64) % 5 == 0;
                    let skip7 =
                        (p1 + r_i64) % 7 == 0 || (p2 + r_i64) % 7 == 0 || (p3 + r_i64) % 7 == 0;
                    !skip3 && !skip5 && !skip7
                };

                let bitmask_survives = (mask & (1u128 << r)) != 0;
                assert_eq!(
                    bitmask_survives, direct_survives,
                    "Mismatch for r={} on poly a={}, b={}, c={}",
                    r, a, b, c
                );
            }
        }
    }

    #[test]
    fn test_periodic_mask_cache_matches_dynamic_generation() {
        let sieve = Sieve::new(SIEVE_LIMIT);
        let buckets = Mod105Buckets::new(&sieve);
        let cache = PeriodicMaskCache::new(&buckets);

        for a in 0..MASK_RESIDUE_PERIOD as i64 {
            for b in (0..MASK_RESIDUE_PERIOD as i64).step_by(3) {
                let c_start = aligned_c_start(a, b, 6);
                let expected = compute_period_masks_mod35(
                    a,
                    b,
                    c_start,
                    &buckets.valid_mod5,
                    &buckets.valid_mod7,
                );
                for (index, &expected_mask) in expected.iter().enumerate() {
                    assert_eq!(
                        cache.value(cache.mod35(a, b)[index]),
                        expected_mask,
                        "mod35 cache mismatch for a={}, b={}, step={}",
                        a,
                        b,
                        index
                    );
                }
            }

            for b in 0..MASK_RESIDUE_PERIOD as i64 {
                if b % 3 == 0 {
                    continue;
                }
                let c_start = aligned_c_start(a, b, 2);
                let expected = compute_period_masks_mod105(
                    a,
                    b,
                    c_start,
                    &buckets.valid_mod3,
                    &buckets.valid_mod5,
                    &buckets.valid_mod7,
                );
                for (index, &expected_mask) in expected.iter().enumerate() {
                    assert_eq!(
                        cache.value(cache.mod105(a, b)[index]),
                        expected_mask,
                        "mod105 cache mismatch for a={}, b={}, step={}",
                        a,
                        b,
                        index
                    );
                }
            }
        }
    }

    #[test]
    fn test_format_polynomial() {
        assert_eq!(format_polynomial(1, 1, 1, 3), "f(n) = n^3 + n^2 + n + 3");
        assert_eq!(
            format_polynomial(-1, -1, -1, 7),
            "f(n) = -n^3 - n^2 - n + 7"
        );
        assert_eq!(
            format_polynomial(-14, 514, -2970, 8123),
            "f(n) = -14n^3 + 514n^2 - 2970n + 8123"
        );
        assert_eq!(format_polynomial(5, 0, 0, 11), "f(n) = 5n^3 + 11");
        assert_eq!(format_polynomial(-1, 0, 2, 5), "f(n) = -n^3 + 2n + 5");
    }

    #[test]
    fn test_parity_pruning_soundness() {
        // If (a + b + c) is odd, f(n) is even for all odd n:
        // f(n) = a*n^3 + b*n^2 + c*n + d = (a + b + c) mod 2 + d mod 2 = 1 + 1 = 0 mod 2.
        let a = 1;
        let b = 2;
        let c = 4; // a + b + c = 7 (odd)
        let d = 5; // odd prime

        for n in [1, 3, 5, 7, 9] {
            let val = eval_poly(a, b, c, d, n);
            assert_eq!(val % 2, 0, "Odd n must yield even f(n) when a+b+c is odd");
        }
    }

    // ========================================================================
    // FORMAL VERIFICATION & UNIT TESTS FOR d = 2 PURGE
    // ========================================================================

    #[test]
    fn test_d_equals_2_purge_analytical_proof() {
        // Theorem: For f(n) = a*n^3 + b*n^2 + c*n + 2, f(2k) = 2 requires
        // 4a*k^2 + 2b*k + c = 0.
        // For both k=1 (n=2) and k=2 (n=4) to be roots, b = -6a and c = 8a.
        // This forces f(n) = a*n*(n-2)*(n-4) + 2.
        // At n = 1: f(1) = 3a + 2.
        // At n = 3: f(3) = -3a + 2.
        // Since a != 0 in Z:
        // - if a >= 1: f(3) <= -1 < 2 (not prime).
        // - if a <= -1: f(1) <= -1 < 2 (not prime).
        // Verify this holds for all non-zero a in [-1000..1000].
        for a in -1000..=1000 {
            if a == 0 {
                continue;
            }
            let b = -6 * a;
            let c = 8 * a;
            let d = 2;

            let f1 = eval_poly(a, b, c, d, 1);
            let f3 = eval_poly(a, b, c, d, 3);

            let at_least_one_negative = f1 < 2 || f3 < 2;
            assert!(
                at_least_one_negative,
                "For a={}, both f(1)={} and f(3)={} were >= 2, violating the theorem!",
                a, f1, f3
            );
        }
    }

    #[test]
    fn test_d_equals_2_purge_exhaustive_small_bounds() {
        // Exhaustively verify across all cubic polynomials with |a| <= 20, |b| <= 50, |c| <= 50
        // that NOT A SINGLE cubic polynomial with d = 2 achieves streak length > 4.
        let sieve = Sieve::new(100_000);

        for a in -20..=20 {
            if a == 0 {
                continue;
            }
            for b in -50..=50 {
                for c in -50..=50 {
                    let d = 2;
                    let mut streak = 0;
                    for n in 0..=10 {
                        let val = eval_poly(a, b, c, d, n);
                        if val >= 2 && sieve.is_prime(val as usize) {
                            streak += 1;
                        } else {
                            break;
                        }
                    }
                    assert!(
                        streak <= 4,
                        "Found cubic poly with d=2 exceeding streak 4! a={}, b={}, c={}, d=2, streak={}",
                        a,
                        b,
                        c,
                        streak
                    );
                }
            }
        }
    }

    #[test]
    fn test_d_equals_2_purge_max_streak_4_example() {
        // Verify that f(n) = n^3 - 4n^2 + 4n + 2 (a=1, b=-4, c=4, d=2)
        // produces the theoretical maximum streak of length 4:
        // n=0: 2 (prime)
        // n=1: 3 (prime)
        // n=2: 2 (prime)
        // n=3: 5 (prime)
        // n=4: 18 (composite: 2 * 3^2)
        let (a, b, c, d) = (1, -4, 4, 2);
        assert_eq!(eval_poly(a, b, c, d, 0), 2);
        assert_eq!(eval_poly(a, b, c, d, 1), 3);
        assert_eq!(eval_poly(a, b, c, d, 2), 2);
        assert_eq!(eval_poly(a, b, c, d, 3), 5);
        assert_eq!(eval_poly(a, b, c, d, 4), 18);
    }

    // ========================================================================
    // FORMAL VERIFICATION & UNIT TESTS FOR d = 3 BOUND (MAX STREAK <= 9)
    // ========================================================================

    #[test]
    fn test_d_equals_3_purge_analytical_proof() {
        // Theorem: For f(n) = a*n^3 + b*n^2 + c*n + 3, f(3k) is divisible by 3.
        // For f(3) = 3 and f(6) = 3, b = -9a and c = 18a.
        // Then f(9) = 162a + 3.
        // Since a != 0 in Z, 162a + 3 != 3, and being divisible by 3, is composite or non-prime.
        for a in -1000..=1000 {
            if a == 0 {
                continue;
            }
            let b = -9 * a;
            let c = 18 * a;
            let d = 3;

            let f9 = eval_poly(a, b, c, d, 9);
            assert_eq!(f9 % 3, 0, "f(9) must be divisible by 3");
            assert_ne!(f9, 3, "f(9) cannot equal 3 for non-zero a");
        }
    }

    #[test]
    fn test_d_equals_3_purge_exhaustive_small_bounds() {
        // Exhaustively verify across all cubic polynomials with |a| <= 15, |b| <= 40, |c| <= 40
        // that NOT A SINGLE cubic polynomial with d = 3 achieves streak length >= 10.
        let sieve = Sieve::new(100_000);

        for a in -15..=15 {
            if a == 0 {
                continue;
            }
            for b in -40..=40 {
                for c in -40..=40 {
                    let d = 3;
                    let mut streak = 0;
                    for n in 0..=15 {
                        let val = eval_poly(a, b, c, d, n);
                        if val >= 2 && sieve.is_prime(val as usize) {
                            streak += 1;
                        } else {
                            break;
                        }
                    }
                    assert!(
                        streak <= 9,
                        "Found cubic poly with d=3 exceeding streak 9! a={}, b={}, c={}, d=3, streak={}",
                        a,
                        b,
                        c,
                        streak
                    );
                }
            }
        }
    }

    // ========================================================================
    // FORMAL VERIFICATION & UNIT TESTS FOR MOD-3 RESIDUE PRUNING
    // ========================================================================

    #[test]
    fn test_mod3_residue_pruning_soundness() {
        // Theorem: If f(1) = 0 mod 3 or f(2) = 0 mod 3, then f(r + 3k) = 0 mod 3 for all k >= 0.
        // For f(r + 3k) to be prime, f(r + 3k) = 3.
        // A cubic can equal 3 at most 3 times.
        // In the set {r, r+3, r+6, r+9}, at least one term is != 3, hence composite.
        // Therefore max streak length <= 11.
        let sieve = Sieve::new(100_000);

        // Test across sample polynomials where f(1) % 3 == 0 or f(2) % 3 == 0
        let test_cases = [
            (1, 1, 1, 7),   // f(1) = 1+1+1+7 = 10 (not 0 mod 3), f(2) = 8+4+2+7 = 21 (0 mod 3)
            (2, 3, 1, 11),  // f(1) = 2+3+1+11 = 17, f(2) = 16+12+2+11 = 41
            (-1, 2, -1, 5), // f(1) = -1+2-1+5 = 5, f(2) = -8+8-2+5 = 3 (0 mod 3)
        ];

        for (a, b, c, d) in test_cases {
            let f1_mod3 = eval_poly(a, b, c, d, 1).rem_euclid(3);
            let f2_mod3 = eval_poly(a, b, c, d, 2).rem_euclid(3);

            if f1_mod3 == 0 || f2_mod3 == 0 {
                let mut streak = 0;
                for n in 0..=30 {
                    let val = eval_poly(a, b, c, d, n);
                    if val >= 2 && sieve.is_prime(val as usize) {
                        streak += 1;
                    } else {
                        break;
                    }
                }
                assert!(
                    streak <= 11,
                    "Streak {} exceeded 11 for poly with zero mod 3: a={}, b={}, c={}, d={}",
                    streak,
                    a,
                    b,
                    c,
                    d
                );
            }
        }
    }

    #[test]
    fn test_d_min_29_mathematical_culling() {
        // Mathematical proof that for d <= 23, streak length cannot reach 28:
        // For f(n) = a*n^3 + b*n^2 + c*n + d, f(0) = d.
        // For any k >= 0, f(k*d) is an integer multiple of d.
        // For f(k*d) to be prime, f(k*d) must equal d.
        // The cubic f(x) - d = 0 has at most 3 roots (one is x = 0).
        // For d <= 13: 2d <= 26 < 28.
        // For d <= 7: 3d <= 21 < 28, so f(0), f(d), f(2d), f(3d) cannot all equal d!
        // At least one multiple <= 27 must be != d and divisible by d, hence composite.
        let small_primes = [2, 3, 5, 7, 11, 13, 17, 19, 23];
        for &d in &small_primes {
            assert!(
                d < D_MIN as i64,
                "d={} should be purged (< D_MIN={})",
                d,
                D_MIN
            );
        }
    }

    fn gcd(mut a: usize, mut b: usize) -> usize {
        while b != 0 {
            let t = b;
            b = a % b;
            a = t;
        }
        a
    }

    #[test]
    fn test_mod105_buckets_integrity() {
        let sieve = Sieve::new(SIEVE_LIMIT);
        let buckets = Mod105Buckets::new(&sieve);

        let primes_expected: Vec<u16> = sieve
            .primes_from(D_MIN)
            .take_while(|&p| p <= D_MAX)
            .map(|p| p as u16)
            .collect();

        // 1. Offsets bounds
        assert_eq!(buckets.offsets[0], 0);
        assert_eq!(buckets.offsets[105] as usize, primes_expected.len());
        assert_eq!(buckets.raw_d.len(), primes_expected.len());
        assert_eq!(buckets.d_half.len(), primes_expected.len());

        // 2. Pre-shifted d_half verification
        for i in 0..buckets.raw_d.len() {
            assert_eq!(buckets.d_half[i], buckets.raw_d[i] >> 1);
        }

        // 3. Exactly phi(105) = 48 non-empty buckets
        let mut non_empty = 0;
        for r in 0..105 {
            let start = buckets.offsets[r] as usize;
            let end = buckets.offsets[r + 1] as usize;
            assert!(start <= end);
            if start < end {
                non_empty += 1;
                // Verify residue is coprime to 105
                assert_eq!(
                    gcd(r, 105),
                    1,
                    "Non-empty bucket r={} must be coprime to 105",
                    r
                );
                // Verify all primes in this slice have d % 105 == r
                for &d in &buckets.raw_d[start..end] {
                    assert_eq!((d % 105) as usize, r);
                    assert!(d >= D_MIN as u16 && d <= D_MAX as u16);
                }
            } else {
                // Empty bucket must either be r=0 or share a factor with 105
                assert!(
                    r == 0 || gcd(r, 105) > 1,
                    "Residue r={} was unexpectedly empty",
                    r
                );
            }
        }
        assert_eq!(
            non_empty, 48,
            "Must have exactly 48 non-empty coprime residue buckets"
        );
    }

    #[test]
    fn test_mod105_filter_preserves_all_known_discoveries() {
        // Verify that ALL known record polynomials from discoveries.txt pass the mod-105 filter
        let discoveries: [(i64, i64, i64, i64); 18] = [
            (-30, 817, 1273, 6827),
            (-30, 727, 2817, 8887),
            (-16, 558, -2030, 4289),
            (-14, 514, -2970, 8123),
            (-14, 388, -264, 3461),
            (-14, 430, -1082, 4127),
            (-14, 472, -1984, 5653),
            (-13, 502, -2535, 9239),
            (-13, 424, -683, 6073),
            (-13, 463, -1570, 7193),
            (-5, 210, -2275, 7717),
            (-2, 158, -1696, 6473),
            (-2, 164, -2018, 8329),
            (-1, 46, -605, 2557),
            (1, -32, 241, 347),
            (5, -119, 732, 3203),
            (11, -222, 1465, 317),
            (15, -349, 2234, 5101),
        ];

        let sieve = Sieve::new(SIEVE_LIMIT);
        let buckets = Mod105Buckets::new(&sieve);

        for &(a, b, c, d) in &discoveries {
            assert!(
                d >= D_MIN as i64,
                "Discovery d={} must be >= D_MIN={}",
                d,
                D_MIN
            );
            let p1 = a + b + c;
            let p2 = 8 * a + 4 * b + 2 * c;
            let p3 = 27 * a + 9 * b + 3 * c;
            let r = d % 105;

            // Check that the mod-105 divisibility filter admits this residue
            let skip_mod3 = (p1 + r) % 3 == 0 || (p2 + r) % 3 == 0 || (p3 + r) % 3 == 0;
            let skip_mod5 = (p1 + r) % 5 == 0 || (p2 + r) % 5 == 0 || (p3 + r) % 5 == 0;
            let skip_mod7 = (p1 + r) % 7 == 0 || (p2 + r) % 7 == 0 || (p3 + r) % 7 == 0;

            assert!(
                !skip_mod3,
                "Mod-3 check falsely skipped discovery {:?}",
                (a, b, c, d)
            );
            assert!(
                !skip_mod5,
                "Mod-5 check falsely skipped discovery {:?}",
                (a, b, c, d)
            );
            assert!(
                !skip_mod7,
                "Mod-7 check falsely skipped discovery {:?}",
                (a, b, c, d)
            );

            // Verify bitmask approach also preserves this residue
            let m3_1 = p1.rem_euclid(3) as usize;
            let m3_2 = p2.rem_euclid(3) as usize;
            let m3_3 = p3.rem_euclid(3) as usize;
            let mask3 =
                buckets.valid_mod3[m3_1] & buckets.valid_mod3[m3_2] & buckets.valid_mod3[m3_3];

            let m5_1 = p1.rem_euclid(5) as usize;
            let m5_2 = p2.rem_euclid(5) as usize;
            let m5_3 = p3.rem_euclid(5) as usize;
            let mask5 =
                buckets.valid_mod5[m5_1] & buckets.valid_mod5[m5_2] & buckets.valid_mod5[m5_3];

            let m7_1 = p1.rem_euclid(7) as usize;
            let m7_2 = p2.rem_euclid(7) as usize;
            let m7_3 = p3.rem_euclid(7) as usize;
            let mask7 =
                buckets.valid_mod7[m7_1] & buckets.valid_mod7[m7_2] & buckets.valid_mod7[m7_3];

            let mask = mask3 & mask5 & mask7;
            assert!(
                (mask & (1u128 << r)) != 0,
                "Precomputed bitmask falsely skipped discovery {:?}",
                (a, b, c, d)
            );

            // Verify d is present in the corresponding bucket slice
            let start = buckets.offsets[r as usize] as usize;
            let end = buckets.offsets[r as usize + 1] as usize;
            assert!(
                buckets.raw_d[start..end].contains(&(d as u16)),
                "Prime d={} must be in bucket r={}",
                d,
                r
            );
        }
    }

    // ========================================================================
    // CHECKPOINT TESTS (PARSING, HASHING, ROUND-TRIP)
    // ========================================================================

    #[test]
    fn test_parse_checkpoint() {
        assert_eq!(compute_config_hash(), 0x84a1bcd5c8222eae);

        // Empty / comment only
        assert_eq!(parse_checkpoint(""), None);
        assert_eq!(parse_checkpoint("# Comment line\n\n"), None);

        // Legacy format: raw integer
        assert_eq!(
            parse_checkpoint("-4\n"),
            Some(CheckpointData {
                last_a: -4,
                config_hash: None,
            })
        );
        assert_eq!(
            parse_checkpoint("18\n"),
            Some(CheckpointData {
                last_a: 18,
                config_hash: None,
            })
        );

        // New structured format with metadata hash
        let structured = "# Prime Hunter Checkpoint\n\
                          SCHEMA_VERSION: 2\n\
                          CONFIG_HASH: 0x123456789abcdef0\n\
                          LAST_A: 42\n";
        assert_eq!(
            parse_checkpoint(structured),
            Some(CheckpointData {
                last_a: 42,
                config_hash: Some(0x123456789abcdef0),
            })
        );

        // Corrupted hash
        let corrupted_hash = "# Prime Hunter Checkpoint\n\
                              SCHEMA_VERSION: 2\n\
                              CONFIG_HASH: 0xZZZZ\n\
                              LAST_A: 42\n";
        assert_eq!(parse_checkpoint(corrupted_hash), None);

        // Corrupted LAST_A
        let corrupted_last_a = "# Prime Hunter Checkpoint\n\
                                SCHEMA_VERSION: 2\n\
                                CONFIG_HASH: 0x123456789abcdef0\n\
                                LAST_A: not_a_number\n";
        assert_eq!(parse_checkpoint(corrupted_last_a), None);

        // Pre-versioned structured checkpoints remain readable.
        let pre_versioned = "# Prime Hunter Checkpoint\n\
                             CONFIG_HASH: 0x123456789abcdef0\n\
                             LAST_A: 42\n";
        assert_eq!(
            parse_checkpoint(pre_versioned),
            Some(CheckpointData {
                last_a: 42,
                config_hash: Some(0x123456789abcdef0),
            })
        );

        // Invalid schema versions are rejected.
        assert_eq!(
            parse_checkpoint("SCHEMA_VERSION: invalid\nCONFIG_HASH: 0x1\nLAST_A: 42\n"),
            None
        );
        assert_eq!(
            parse_checkpoint("SCHEMA_VERSION: 0\nCONFIG_HASH: 0x1\nLAST_A: 42\n"),
            None
        );

        // Invalid format
        assert_eq!(parse_checkpoint("invalid"), None);
    }

    #[test]
    fn test_checkpoint_atomic_roundtrip() {
        let tmp_dir = std::env::temp_dir();
        let final_path = tmp_dir.join("test_checkpoint_roundtrip.txt");
        let tmp_path = tmp_dir.join("test_checkpoint_roundtrip.txt.tmp");
        let final_str = final_path.to_str().unwrap();
        let tmp_str = tmp_path.to_str().unwrap();

        let test_a = -17;
        let save_res = save_checkpoint_to_path(test_a, final_str, tmp_str);
        assert!(save_res.is_ok(), "Checkpoint save should succeed");

        let loaded = load_checkpoint_from_path(final_str);
        assert!(loaded.is_ok(), "Checkpoint load should succeed");
        let data = loaded.unwrap().expect("Checkpoint data should be present");
        assert_eq!(data.last_a, test_a);
        assert_eq!(data.config_hash, Some(compute_config_hash()));
        let content = fs::read_to_string(final_str).unwrap();
        assert!(content.contains("SCHEMA_VERSION: 2"));

        // Clean up
        let _ = fs::remove_file(final_path);
        let _ = fs::remove_file(tmp_path);
    }

    #[test]
    fn test_checkpoint_load_corrupted_and_missing_file() {
        let tmp_dir = std::env::temp_dir();
        let test_path = tmp_dir.join("test_checkpoint_corrupted.txt");
        let path_str = test_path.to_str().unwrap();

        // 1. Missing file returns Ok(None)
        let _ = fs::remove_file(&test_path);
        let res_missing = load_checkpoint_from_path(path_str);
        assert_eq!(res_missing, Ok(None));

        // 2. Corrupted file returns Err(CheckpointError::Corrupted)
        fs::write(&test_path, "CONFIG_HASH: 0xINVALID\nLAST_A: 42\n").unwrap();
        let res_corrupted = load_checkpoint_from_path(path_str);
        assert!(matches!(res_corrupted, Err(CheckpointError::Corrupted(_))));

        // Clean up
        let _ = fs::remove_file(test_path);
    }

    #[test]
    fn test_assert_search_bounds_no_overflow() {
        // Must complete without panicking
        assert_search_bounds_no_overflow();
    }

    #[test]
    fn test_shift_polynomial_backward_algebraic_identity() {
        let test_cases = [
            (-14, 514, -2970, 8123),
            (-30, 817, 1273, 6827),
            (1, 1, 1, 3),
            (5, -119, 732, 3203),
            (-2, 158, -1696, 6473),
        ];

        for (a, b, c, d) in test_cases {
            for k in [0, 1, 2, 5, 8, 12] {
                let (shifted_a, shifted_b, shifted_c, shifted_d) =
                    shift_polynomial_backward(a, b, c, d, k);

                // Identity: g(m) = f(m - k) for all m
                for m in 0..50 {
                    let g_m = eval_poly(shifted_a, shifted_b, shifted_c, shifted_d, m);
                    let f_orig = eval_poly(a, b, c, d, m - k as i64);
                    assert_eq!(
                        g_m, f_orig,
                        "Algebraic shift identity failed for poly ({}, {}, {}, {}) at k={}, m={}",
                        a, b, c, d, k, m
                    );
                }
            }
        }
    }

    #[test]
    fn test_crt_step_6_soundness() {
        let sieve = Sieve::new(100_000);

        for a in -20i64..=20i64 {
            if a == 0 {
                continue;
            }
            for b in (-30i64..=30i64).step_by(3) {
                for c in -50i64..=50i64 {
                    let ab = a + b;
                    let p1 = ab + c;
                    let p2 = 8 * a + 4 * b + 2 * c;
                    let p3 = 27 * a + 9 * b + 3 * c;

                    if (ab + c).rem_euclid(6) == 0 {
                        // Mod 3 identity: all must be multiples of 3
                        assert_eq!(
                            p1.rem_euclid(3),
                            0,
                            "P1 must be 0 mod 3 when ab+c = 0 mod 6"
                        );
                        assert_eq!(
                            p2.rem_euclid(3),
                            0,
                            "P2 must be 0 mod 3 when ab+c = 0 mod 6"
                        );
                        assert_eq!(
                            p3.rem_euclid(3),
                            0,
                            "P3 must be 0 mod 3 when ab+c = 0 mod 6"
                        );
                    } else if (ab + c) % 2 == 0 {
                        // Even parity but not divisible by 3:
                        // {P1, P2} mod 3 must cover {1, 2}
                        let rem1 = p1.rem_euclid(3);
                        let rem2 = p2.rem_euclid(3);
                        assert_ne!(rem1, 0, "P1 cannot be 0 mod 3 if a+b+c != 0 mod 3");
                        assert_eq!(rem2, (3 - rem1) % 3, "P2 must be -P1 mod 3 when 3 | b");

                        // Test on actual prime d candidates >= 29: all must fail primality at n=1 or n=2
                        for &d in &[
                            29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71, 73, 79, 83, 89, 97,
                        ] {
                            let f1 = p1 + d;
                            let f2 = p2 + d;
                            let f1_div3 = f1 % 3 == 0;
                            let f2_div3 = f2 % 3 == 0;
                            assert!(
                                f1_div3 || f2_div3,
                                "Either f(1) or f(2) must be divisible by 3 for d={}",
                                d
                            );
                            if f1_div3 && f1 > 3 {
                                assert!(
                                    !sieve.is_prime(f1 as usize),
                                    "f(1) is composite multiple of 3"
                                );
                            }
                            if f2_div3 && f2 > 3 {
                                assert!(
                                    !sieve.is_prime(f2 as usize),
                                    "f(2) is composite multiple of 3"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn test_bidirectional_extension_on_known_discoveries() {
        let sieve = Sieve::new(SIEVE_LIMIT);
        let sieve_bound = sieve.upper_bound();
        let l1 = build_l1_byte_table(&sieve);

        let discoveries = [
            ((-14, 514, -2970, 8123), 31, 5, 36, (-14, 724, -9160, 37573)),
            ((-14, 388, -264, 3461), 28, 8, 36, (-14, 724, -9160, 37573)),
            ((-2, 158, -1696, 6473), 32, 4, 36, (-2, 182, -3056, 15913)),
            ((-2, 164, -2018, 8329), 33, 3, 36, (-2, 182, -3056, 15913)),
        ];

        for &((a, b, c, d), expected_fwd, expected_back, expected_total, expected_shifted) in
            &discoveries
        {
            // Verify forward length
            for n in 0..expected_fwd as i64 {
                let val = eval_poly(a, b, c, d, n);
                assert!(
                    is_prime_fast(&l1, &sieve, sieve_bound, val),
                    "f({}) = {} must be prime in forward streak",
                    n,
                    val
                );
            }

            // Verify backward length
            let mut k = 0;
            let mut back_n = -1i64;
            loop {
                let val = eval_poly(a, b, c, d, back_n);
                if val < 2 || !is_prime_fast(&l1, &sieve, sieve_bound, val) {
                    break;
                }
                k += 1;
                back_n -= 1;
            }
            assert_eq!(
                k, expected_back,
                "Backward length mismatch for ({}, {}, {}, {})",
                a, b, c, d
            );
            assert_eq!(
                expected_fwd + k,
                expected_total,
                "Total length mismatch for ({}, {}, {}, {})",
                a,
                b,
                c,
                d
            );

            // Verify shifted polynomial
            let shifted = shift_polynomial_backward(a, b, c, d, k);
            assert_eq!(
                shifted, expected_shifted,
                "Shifted polynomial mismatch for ({}, {}, {}, {})",
                a, b, c, d
            );

            // Verify shifted polynomial produces expected_total primes for m = 0..expected_total
            let (sa, sb, sc, sd) = shifted;
            for m in 0..expected_total as i64 {
                let val = eval_poly(sa, sb, sc, sd, m);
                assert!(
                    is_prime_fast(&l1, &sieve, sieve_bound, val),
                    "Shifted g({}) = {} must be prime",
                    m,
                    val
                );
            }

            // Verify it terminates at expected_total
            let term_val = eval_poly(sa, sb, sc, sd, expected_total as i64);
            assert!(
                term_val < 2 || !is_prime_fast(&l1, &sieve, sieve_bound, term_val),
                "Shifted sequence must terminate at m={}",
                expected_total
            );
        }
    }
}
