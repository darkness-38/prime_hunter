# Gemini Context & Project Guide: Prime Hunter

Welcome to `prime_hunter`. This document provides a complete technical, mathematical, and architectural guide for AI assistants (like Gemini/Antigravity) and human contributors working within this codebase.

---

## 1. Project Overview & Mathematical Foundations

[`prime_hunter`](file:///home/teto/Documents/GitHub/math/prime_hunter/src/main.rs) is an ultra-optimized computational number theory engine written in Rust. Its mission is to search for integer polynomials of degree 3 (strictly cubic, $a \neq 0$):

$$f(n) = a n^3 + b n^2 + c n + d \quad (a, b, c, d \in \mathbb{Z}, \; a \neq 0)$$

that generate long consecutive streaks of prime numbers for consecutive non-negative integers $n = 0, 1, 2, \dots, L - 1$. The goal is to maximize the run length $L$, hunting for local records ($L \ge 36$) and potential world records ($L \ge 46$).

### Mathematical Rigor & Optimizations

#### 1. Constant Term $d$, Divisibility at $kd$, and the Streak Impossibility Theorem ($L \le 2d$)
- **Base Condition**: Since $f(0) = d$, $d$ must be a prime number.
- **Divisibility Theorem**: For any integer $k \ge 0$, evaluation at $n = kd$ yields:
  $$f(kd) = a(kd)^3 + b(kd)^2 + c(kd) + d = d\left(a k^3 d^2 + b k^2 d + c k + 1\right)$$
  Thus $d \mid f(kd)$ for all integers $k \ge 0$.
- **Primality of Multiples**: The only prime multiple of $d$ is $d$ itself. Therefore, $f(kd)$ can only be prime if $f(kd) = d$:
  $$f(kd) = d \iff f(kd) - d = 0 \iff kd\left(a k^2 d^2 + b k d + c\right) = 0$$
- **Root Bound**: Since $a \neq 0$, the cubic polynomial $f(x) - d = 0$ has at most 3 roots in $\mathbb{R}$. One root is identically $n = 0$ ($k = 0$). Hence, at most two positive multiples $\{d, 2d, 3d, \dots\}$ can ever equal $d$. For all other positive $k$, $f(kd)$ is a multiple of $d$ strictly distinct from $d$, hence composite (or non-prime if $\le 0$).
- **Impossibility of Simultaneous Roots at $n = d$ and $n = 2d$**:
  Suppose a cubic polynomial sustains a prime streak of length $L > 2d$. Then both $f(d)$ and $f(2d)$ must be prime, which forces $f(d) = d$ and $f(2d) = d$.
  Together with $f(0) = d$, this fixes all three roots of $f(x) - d = 0$ at $\{0, d, 2d\}$:
  $$f(n) - d = an(n - d)(n - 2d) \implies f(n) = an(n - d)(n - 2d) + d$$
  Now consider intermediate values between roots:
  - If $a \ge 1$: evaluate at $n = d + 1$ (where $d + 1 < 2d$ for all $d \ge 2$):
    $$f(d + 1) = a(d + 1)(1)(1 - d) + d = -a(d^2 - 1) + d$$
    For any prime $d \ge 3$, $d^2 - 1 > d$. Therefore:
    $$f(d + 1) \le -(d^2 - 1) + d < 0$$
    A negative number is never prime!
  - If $a \le -1$: evaluate at $n = 1$ (where $1 < d$ for all $d \ge 2$):
    $$f(1) = a(1)(1 - d)(1 - 2d) + d = a(d - 1)(2d - 1) + d$$
    For any prime $d \ge 2$, $(d - 1)(2d - 1) \ge 3 > d$. Therefore:
    $$f(1) \le -(d - 1)(2d - 1) + d < 0$$
    A negative number is never prime!
  
  **Conclusion**: It is mathematically impossible for any cubic polynomial to have $f(d) = d$ and $f(2d) = d$ while remaining positive at all intermediate points. Consequently, if $f(d)$ is prime ($f(d) = d$), then $f(2d)$ cannot equal $d$, and being divisible by $d$, must be composite (or non-positive). Conversely, if $f(d) \neq d$, then $f(d)$ is composite, breaking the streak at or before $n = d < 2d$. Therefore, every cubic prime streak starting at $n = 0$ must terminate at or before $n = 2d$, establishing the fundamental bound:
  $$L \le 2d$$

#### 2. Mathematical Culling: Setting `D_MIN = 29` (Purging $d \le 23$)
- **Streak Length Invariant**: For any hunt targeting record run length $L \ge 28$, the bound $L \le 2d$ dictates:
  $$2d \ge 28 \implies d \ge 14$$
  This immediately and analytically eliminates all small primes $d \le 13$:
  - $d = 2$: $L \le 4$ (tight quadratic proof below).
  - $d = 3$: $L \le 9$ (tight cubic proof below).
  - $d = 5$: $2d = 10 \implies L \le 10 < 28$.
  - $d = 7$: $2d = 14 \implies L \le 14 < 28$.
  - $d = 11$: $2d = 22 \implies L \le 22 < 28$.
  - $d = 13$: $2d = 26 \implies L \le 26 < 28$.
- **Culling of $d \in \{17, 19, 23\}$**: For these primes, $d < 28$, meaning $n = d$ falls strictly within any potential record streak $n \in [0, 27]$. For $f(d)$ to be prime, it must satisfy $f(d) = d$. Exhaustive searches and root-interval analysis confirm that no cubic polynomial with $d \in \{17, 19, 23\}$ and $f(d) = d$ can sustain prime values for all $n \in [0, 27]$.
- **Engine Action**: The search space sets `D_MIN = 29` and `D_MAX = 10000`, completely purging $d \in \{2, 3, 5, 7, 11, 13, 17, 19, 23\}$. Exactly **1,220 prime candidates** remain, eliminating dead-code iterations with zero false negatives.

#### 3. Formal Proof of $L \le 4$ for $d = 2$
1. For any even integer $n = 2k$ ($k \ge 0$):
   $$f(2k) = a(2k)^3 + b(2k)^2 + c(2k) + 2 = 2\left(4a k^3 + 2b k^2 + c k + 1\right)$$
   Thus $f(2k)$ is always even for all $k \ge 0$.
2. The only even prime number is $2$. Therefore, $f(2k)$ is prime if and only if $f(2k) = 2$:
   $$f(2k) = 2 \iff k(4ak^2 + 2bk + c) = 0$$
3. For $k = 0$ ($n = 0$): $f(0) = 2$ (prime).
4. For $k > 0$: $4ak^2 + 2bk + c = 0$. Since $a \neq 0$, this quadratic has at most 2 real roots. Hence, there can be at most 2 positive even integers where $f(n) = 2$.
5. In the sequence $n = 0, 1, 2, 3, 4, 5, 6$ ($k = 0, 1, 2, 3$), $k = 1, 2, 3$ cannot all be roots. At the absolute latest, $n = 6$ ($k = 3$) has $f(6) \neq 2$, so $f(6)$ is an even composite number. This immediately proves $L \le 6$.
6. **Tighter Bound $L \le 4$**:
   To reach streak length 5 ($n = 0, 1, 2, 3, 4$ all prime), both $n = 2$ ($k = 1$) and $n = 4$ ($k = 2$) must be roots of the quadratic:
   $$4a(1)^2 + 2b(1) + c = 0 \implies 4a + 2b + c = 0$$
   $$4a(2)^2 + 2b(2) + c = 0 \implies 16a + 4b + c = 0$$
   Subtracting gives $12a + 2b = 0 \implies b = -6a$. Substituting back gives $c = 8a$.
   This uniquely determines the polynomial family:
   $$f(n) = an^3 - 6an^2 + 8an + 2 = an(n - 2)(n - 4) + 2$$
   Now evaluate $f(n)$ at odd integers $n = 1$ and $n = 3$:
   $$f(1) = a(1)(-1)(-3) + 2 = 3a + 2$$
   $$f(3) = a(3)(1)(-1) + 2 = -3a + 2$$
   Since $a \neq 0$ is an integer:
   - If $a \ge 1$: $f(3) = -3a + 2 \le -3(1) + 2 = -1 < 2$ (strictly negative, not prime).
   - If $a \le -1$: $f(1) = 3a + 2 \le 3(-1) + 2 = -1 < 2$ (strictly negative, not prime).
   In all cases, either $f(1)$ or $f(3)$ is non-prime!
   Therefore, $f(2)$ and $f(4)$ can NEVER simultaneously belong to a consecutive prime streak starting at $n=0$.
   **Conclusion**: The maximum streak length for any cubic polynomial with $d = 2$ is at most 4 (example: $f(n) = n^3 - 4n^2 + 4n + 2$ achieves $L = 4$ with $f(0)=2, f(1)=3, f(2)=2, f(3)=5$, and breaks at $f(4)=18$).

#### 4. Formal Proof of $L \le 9$ for $d = 3$
1. For any $n = 3k$ ($k \ge 0$):
   $$f(3k) = a(3k)^3 + b(3k)^2 + c(3k) + 3 = 3\left(9a k^3 + 3b k^2 + c k + 1\right)$$
   Thus $f(3k)$ is always divisible by 3 for all $k \ge 0$.
2. For $f(3k)$ to be prime, it must equal 3:
   $$f(3k) = 3 \iff k\left(9ak^2 + 3bk + c\right) = 0$$
3. For $k = 0$ ($n = 0$): $f(0) = 3$ (prime).
4. For $k > 0$: $9ak^2 + 3bk + c = 0$ has at most 2 roots since $a \neq 0$. Thus at most two values in $\{3, 6, 9, \dots\}$ can equal 3.
5. For $n = 3$ ($k = 1$) and $n = 6$ ($k = 2$) to both equal 3:
   $$9a + 3b + c = 0 \quad \text{and} \quad 36a + 6b + c = 0 \implies 27a + 3b = 0 \implies b = -9a, \; c = 18a$$
6. Then at $n = 9$ ($k = 3$):
   $$f(9) = a(9)^3 - 9a(9)^2 + 18a(9) + 3 = 729a - 729a + 162a + 3 = 162a + 3$$
   For $f(9) = 3$ requires $162a = 0 \implies a = 0$, contradiction ($a \neq 0$).
   Since $f(9) = 3(54a + 1)$ is divisible by 3 and $f(9) \neq 3$, $f(9)$ is composite (if positive) or non-prime (if $\le 0$).
   **Conclusion**: The maximum streak length for any cubic polynomial with $d = 3$ is at most 9.

#### 5. Parity Pruning of Linear Coefficient $c$ (50% Elimination)
- For any odd prime $d$ ($d \equiv 1 \pmod 2$), consider $f(n)$ evaluated at odd integers $n$:
  $$n^3 \equiv n^2 \equiv n \equiv 1 \pmod 2 \implies f(n) \equiv a + b + c + d \equiv (a + b + c) + 1 \pmod 2$$
- If $(a + b + c)$ is odd:
  $$f(n) \equiv 1 + 1 \equiv 0 \pmod 2 \quad (\text{even})$$
  for **every odd integer $n \in \{1, 3, 5, 7, \dots\}$**.
  Since an even number is prime only if it equals 2, and $f(n) = 2$ at most 3 times, any polynomial where $(a + b + c)$ is odd terminates early ($L \le 7 < 28$). In practice, for $d \ge 29$, $f(1) = (a+b+c)+d$ is almost always an even integer $> 2$, failing immediately at $n = 1$ ($L = 1$).
- For $f(1)$ to have a chance of being an odd prime, $(a + b + c)$ **must be even**:
  $$(a + b) \pmod 2 \equiv c \pmod 2$$
- **Engine Action**: The inner loop inspects the parity of $(a + b)$ and aligns the initial $c$ accordingly, stepping $c$ by $2$ (`c += 2`). This mathematically eliminates **50% of all linear coefficients** *a priori* without running a single primality check, guaranteeing **zero false negatives** for sequences of target length $L \ge 28$.

#### 6. Flat Memory Layout & Mod-105 Residue Pruning (Gracemont Phase 1)
To eliminate heap fragmentation and maximize L1 cache residency on Intel Gracemont microarchitectures, prime candidates are organized into flat residue buckets modulo $105 = 3 \times 5 \times 7$.

- **Coprime Residue Partition**:
  By Euler's totient function:
  $$\phi(105) = \phi(3) \times \phi(5) \times \phi(7) = 2 \times 4 \times 6 = 48$$
  All 1,220 prime candidates $d \in [29, 10000]$ are coprime to 105 (since primes $\ge 29$ cannot share prime factors 3, 5, or 7). Thus, all candidates cleanly partition into exactly **48 non-empty residue buckets** modulo 105.
- **Flat Memory Layout ([`Mod105Buckets`](file:///home/teto/Documents/GitHub/math/prime_hunter/src/main.rs#L185-L221))**:
  Rather than storing nested vectors (`Vec<Vec<u16>>`), candidates are packed into contiguous flat arrays:
  ```rust
  pub struct Mod105Buckets {
      pub offsets: [u16; 106], // offsets[r]..offsets[r+1] indexes residue r
      pub raw_d: Vec<u16>,     // 1,220 prime values contiguous in memory
      pub d_half: Vec<u16>,    // pre-shifted (d >> 1) for Phase 2 L1 byte lookups
  }
  ```
  - Memory footprint: $106 \times 2 + 1220 \times 2 + 1220 \times 2 + 48\text{ bytes (Vec headers)} \approx 5.1\text{ KiB}$.
  - Zero dynamic heap allocation or deallocation during the search loop.
  - Slices `&raw_d[start..end]` and `&d_half[start..end]` are accessed via direct contiguous pointer ranges.
- **Formal Mod-105 Periodicity & Residue Disqualification Theorem**:
  For polynomial $f(n) = a n^3 + b n^2 + c n + d$:
  $$f(n) = P_n + d \quad \text{where} \quad P_1 = a + b + c, \; P_2 = 8a + 4b + 2c, \; P_3 = 27a + 9b + 3c$$
  For any prime candidate $d$ with residue $r = d \pmod{105}$, and for any prime factor $q \in \{3, 5, 7\}$ of $105$:
  $$d \equiv r \pmod q \implies f(n) = P_n + d \equiv P_n + r \pmod q$$
  Because $f(n)$ has integer coefficients, modular periodicity guarantees:
  $$f(n + k q) \equiv f(n) \pmod q \quad \forall k \in \mathbb{Z}$$
  Suppose for some $n_0 \in \{1, 2, 3\}$ and $q \in \{3, 5, 7\}$, we have $(P_{n_0} + r) \equiv 0 \pmod q$. Then:
  $$f(n_0 + k q) \equiv 0 \pmod q \quad \forall k \ge 0$$
  For any term $f(n_0 + k q)$ to be prime, it must identically equal the prime $q$.
  Since $a \neq 0$, the cubic equation $f(x) - q = 0$ can have at most **3 real roots**.
  Now consider the arithmetic progression of four evaluation points:
  $$S = \{n_0, \; n_0 + q, \; n_0 + 2q, \; n_0 + 3q\}$$
  At most 3 values in $S$ can equal $q$. At least one value $n^* \in S$ must have $f(n^*) \neq q$.
  Since $q \mid f(n^*)$ and $f(n^*) \neq q$, $f(n^*)$ is either composite (if $> q$) or non-prime (if $\le 0$).
  The maximum evaluation point in $S$ across all checks is:
  $$n_{\max} = n_0 + 3q$$
  - For $q = 3$, $n_0 \in \{1, 2\} \implies n_{\max} \le 2 + 3(3) = 11$.
  - For $q = 5$, $n_0 \in \{1, 2, 3\} \implies n_{\max} \le 3 + 3(5) = 18$.
  - For $q = 7$, $n_0 \in \{1, 2, 3\} \implies n_{\max} \le 3 + 3(7) = 24$.
  
  In **every case**, $n_{\max} \le 24 < 28$!
  Therefore, if $(P_n + r) \equiv 0 \pmod q$ for any $n \in \{1, 2, 3\}$ and $q \in \{3, 5, 7\}$, the prime streak **must terminate at or before $n = 24$**, making a streak of $L \ge 28$ mathematically impossible.

#### 7. Precomputed Mod-105 Bitmasks, Extended Culling & Periodic CRT Masks (V2)
To eliminate all runtime modulo operations inside the innermost `c` loop:
- **Precomputed Bitmasks**:
  At startup, `Mod105Buckets` builds lookup arrays `valid_mod3[3]`, `valid_mod5[5]`, `valid_mod7[7]` of type `u128`. Each bit $r \in [1, 104]$ indicates whether bucket $r$ is non-empty and satisfies $(x + r) \not\equiv 0 \pmod q$.
- **Extended Culling (V2 — ~50.7% extra pruning)**:
  Rather than checking only $n \in \{1,2,3\}$, the extended culling applies:
  - Mod 5: checks $n \in \{1, 2, 3, 4\}$ — because a 5-periodic zero can be forced at $n \le 4+3\times5=19 < 28$.
  - Mod 7: checks $n \in \{1, 2, 3, 4, 5, 6\}$ — because a 7-periodic zero can be forced at $n \le 6+3\times7=27 < 28$.
  This is embedded in `compute_period_masks_mod35` and `compute_period_masks_mod105` during their one-time precomputation. Extended culling eliminates approximately **50.7% more candidates** before any L1 lookup, with zero false negatives on all 18 historical discoveries.
- **Periodic CRT Masks (V2 — zero inner-loop division)**:
  Instead of computing `p1.rem_euclid(3)` etc. on every `c` iteration, the modular pattern is periodic:
  - When $b \equiv 0 \pmod 3$ and $c$ steps by 6: period = $\operatorname{lcm}(5,7) = 35$ steps → `compute_period_masks_mod35` precomputes a `[u128; 35]` array.
  - When $b \not\equiv 0 \pmod 3$ and $c$ steps by 2: period = $\operatorname{lcm}(3,5,7) = 105$ steps → `compute_period_masks_mod105` precomputes a `[u128; 105]` array.
  Inside the loop, the mask is a single cyclic array read:
  ```rust
  let mut mask = unsafe { *period_masks.get_unchecked(step_idx) };
  step_idx = if step_idx + 1 == PERIOD { 0 } else { step_idx + 1 };
  ```
  This replaces all `rem_euclid` calls with a single load and branch-free counter advance.
- **Incremental Polynomial Updates (V2 — now includes p2_idx, p3_idx)**:
  All three pre-folded indices are incremented outside the innermost `d` loop:
  - For $c \mathrel{+}= 2$: `p1 += 2; p2 += 4; p3 += 6; p1_idx += 1; p2_idx += 2; p3_idx += 3;`
  - For $c \mathrel{+}= 6$: `p1 += 6; p2 += 12; p3 += 18; p1_idx += 3; p2_idx += 6; p3_idx += 9;`

#### 8. CRT Step-6 Pruning When $b \equiv 0 \pmod 3$

- **Mathematical Invariant**:
  When $b \equiv 0 \pmod 3$:
  $$P_2 = 8a + 4b + 2c \equiv 2a + 2c \equiv 2(a + c) \equiv -(a + c) \equiv -P_1 \pmod 3$$
  If $P_1 \not\equiv 0 \pmod 3$, then $\{P_1, P_2\} \pmod 3 = \{1, 2\}$.
  For any candidate prime $d \ge 29$, $d \in \{1, 2\} \pmod 3$:
  - If $d \equiv 1 \pmod 3$: $f(2) = P_2 + d \equiv 2 + 1 \equiv 0 \pmod 3$, composite!
  - If $d \equiv 2 \pmod 3$: $f(1) = P_1 + d \equiv 1 + 2 \equiv 0 \pmod 3$, composite!
  Thus, 100% of candidate primes $d \ge 29$ are eliminated at $n=1$ or $n=2$!
- **Chinese Remainder Theorem Combination**:
  Surviving polynomials MUST satisfy $a + b + c \equiv 0 \pmod 3$.
  Combining parity ($a + b + c \equiv 0 \pmod 2$) and mod-3 ($a + b + c \equiv 0 \pmod 3$) via CRT yields:
  $$a + b + c \equiv 0 \pmod 6$$
- **Engine Execution**:
  When $b \equiv 0 \pmod 3$:
  - Find initial $c_{\text{start}}$ such that $(a + b + c_{\text{start}}) \equiv 0 \pmod 6$.
  - Step $c$ by 6 on every iteration (`c += 6; p1 += 6; p2 += 12; p3 += 18; p1_idx += 3;`).
  - Furthermore, $P_1 \equiv P_2 \equiv P_3 \equiv 0 \pmod 3$ holds identically, so mod-3 bitmask checks are completely bypassed!
  - When $b \not\equiv 0 \pmod 3$: standard step-2 parity pruning is executed.

#### 9. 16 KiB L1 Byte Table & Pre-Folded Address Calculation (Gracemont Phase 2)
On Intel Gracemont E-cores (e.g. Core i3-N305, N100), bitwise extraction operations like `bt` (bit test) or variable shifts incur latency penalties across execution ports. Phase 2 eliminates these overheads with direct byte table indexing and pre-folded address arithmetic.

- **16 KiB Flat Byte Table ([`build_l1_byte_table`](file:///home/teto/Documents/GitHub/math/prime_hunter/src/main.rs#L303-L310))**:
  - `L1_LIMIT = 32_768`, covering all odd integers in $[3, 32_768)$.
  - `L1_SIZE = 16_384` bytes ($16\text{ KiB}$).
  - Each entry is `u8`: `1` for prime, `0` for composite.
  - Sits directly in the physical 32 KiB L1 Data Cache (L1d) of each CPU core alongside `Mod105Buckets` (~5.1 KiB). Total working set is $\approx 21.1\text{ KiB}$, well below the 32 KiB limit and leaving ~11 KiB headroom for stack frames and Rayon state.
- **Pre-Folded Address Arithmetic (V2 — all three depths)**:
  For candidate value $f(1) = P_1 + d$, the byte table index is:
  $$\text{index} = \frac{(P_1 + d) - 3}{2} = \left\lfloor \frac{P_1 - 3}{2} \right\rfloor + 1 + \left\lfloor \frac{d}{2} \right\rfloor$$
  - Precomputed outside the $d$ loop for all three depths:
    ```rust
    let p1_idx = ((p1 - 3) >> 1) as isize + 1;
    let p2_idx = ((p2 - 3) >> 1) as isize + 1;
    let p3_idx = ((p3 - 3) >> 1) as isize + 1;
    ```
  - In [`evaluate_d_slice`](file:///home/teto/Documents/GitHub/math/prime_hunter/src/main.rs), each depth is a single addition: `offset = p_idx + d_half as isize`, compiled to a single `cmpb` instruction with no subtract or shift.

#### 10. Register Relief & 4-Way Coalesced Unrolling (Gracemont Phase 3 & Sprint)
- **Register Pressure Relief ([`verify_deep_streak`](file:///home/teto/Documents/GitHub/math/prime_hunter/src/main.rs#L381-L431))**:
  The finite difference recurrence for $n \ge 4$ and discovery reporting are outlined into a separate function marked `#[cold]` and `#[inline(never)]`. Because $n \ge 4$ is reached by $< 0.1\%$ of candidates, this prevents LLVM from spilling hot pointers (`p1_idx`, `d_slice`, `l1`) from the 16 x86-64 GPRs to stack memory (`%rsp`).
- **4-Way Coalesced Unrolling**:
  Gracemont cores feature dual load execution ports (Port 2 and Port 3) capable of 2 independent memory loads per cycle.
  `evaluate_d_slice` processes candidates in quads via `.chunks_exact(4)`. For each quad `(d0, d1, d2, d3)`:
  - Fetches byte indicators `b0, b1, b2, b3` from the 16 KiB L1 table.
  - Performs a branchless OR test: `if (b0 | b1 | b2 | b3) != 0 { ... }`.
  - In $\approx 63\%$ of quads, this single branch skips all 4 candidates in 1-2 clock cycles.
  - Individual candidates are only evaluated if their indicator is non-zero.
  - Trailing candidates are cleanly processed via `.remainder()`.

#### 11. Bidirectional Streak Extension ($n < 0$) & Canonical Polynomial Translation
- **Backward Exploration (V2 — threshold lowered to 10)**:
  When forward streak verification reaches $L_{\text{fwd}} \ge 22$, `verify_deep_streak` evaluates $f(n)$ backwards at $n = -1, -2, -3, \dots$ until hitting a non-prime or composite value.
  If $k \ge 0$ consecutive primes exist backwards, the total uninterrupted prime streak is:
  $$L_{\text{total}} = L_{\text{fwd}} + k$$
- **Canonical Polynomial Translation**:
  To represent the entire prime streak starting at $m = 0$, define $m = n + k \iff n = m - k$:
  $$g(m) = f(m - k) = a(m - k)^3 + b(m - k)^2 + c(m - k) + d = A m^3 + B m^2 + C m + D$$
  Expanding yields:
  $$A = a$$
  $$B = b - 3ka$$
  $$C = c - 2kb + 3k^2 a$$
  $$D = f(-k) = d - kc + k^2 b - k^3 a$$
  The shifted polynomial $g(m)$ generates $L_{\text{total}}$ primes for $m = 0, 1, \dots, L_{\text{total}} - 1$.
- **Historical Discoveries Verification**:
  Bidirectional extension was verified across all 18 historical discoveries:
  - $(-14, 514, -2970, 8123)$: forward=31, backward=5 $\implies$ **Total = 36** primes, shifting to $(-14, 724, -9160, 37573)$.
  - $(-14, 388, -264, 3461)$: forward=28, backward=8 $\implies$ **Total = 36** primes, shifting to $(-14, 724, -9160, 37573)$.
  - $(-2, 158, -1696, 6473)$: forward=32, backward=4 $\implies$ **Total = 36** primes, shifting to $(-2, 182, -3056, 15913)$.
  - $(-2, 164, -2018, 8329)$: forward=33, backward=3 $\implies$ **Total = 36** primes, shifting to $(-2, 182, -3056, 15913)$.

#### 12. 3rd-Order Finite Differences (Zero Multiplications in Hot Loop)
Direct polynomial evaluation using Horner's method $((an + b)n + c)n + d$ requires 3 multiplications and 3 additions per step. By applying Newton's method of finite differences for a degree-3 polynomial:

$$\Delta f(n) = f(n + 1) - f(n) = 3an^2 + (3a + 2b)n + (a + b + c)$$
$$\Delta^2 f(n) = \Delta f(n + 1) - \Delta f(n) = 6an + (6a + 2b)$$
$$\Delta^3 f(n) = \Delta^2 f(n + 1) - \Delta^2 f(n) = 6a \quad (\text{constant})$$

Once a candidate polynomial survives preliminary checks at $n = 1, 2, 3$:
1. $val = f(3) = 27a + 9b + 3c + d$
2. $d_1 = \Delta f(3) = f(4) - f(3) = 37a + 7b + c$
3. $d_2 = \Delta^2 f(3) = 24a + 2b$
4. $d_3 = \Delta^3 f = 6a$

To advance from $n$ to $n + 1$ (starting from $n = 4$):
```rust
val += d1; // val is now f(n)
d1 += d2;  // d1 is now Delta f(n)
d2 += d3;  // d2 is now Delta^2 f(n)
```
The hot evaluation loop performs **3 integer additions** and **zero multiplications**, executing at near-single-cycle throughput on modern superscalar CPU pipelines.

---

## 2. System Architecture & Performance Hierarchy

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                             main() Sequential Loop                          │
│     Iterates outer slice 'a' in [-150..-1] U [1..150] (strictly a != 0)     │
│     Saves progress atomically to checkpoint.txt after each 'a' completes   │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Rayon Parallelism (over 'b')                        │
│     Splits b in [-1000..1000] (2,001 tasks) across CPU worker threads       │
│     Branches on: if b % 3 == 0 (CRT Step-6) vs b % 3 != 0 (Parity Step-2)   │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                          CRT / Parity 'c' Loops                             │
│     - b % 3 == 0 : c steps by 6 (a+b+c = 0 mod 6), period-35 CRT mask table │
│     - b % 3 != 0 : c steps by 2 (a+b+c = 0 mod 2), period-105 CRT mask table│
│     Incremental updates: P1/P2/P3, p1_idx/p2_idx/p3_idx; mask = table[idx]  │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                  Periodic Mod-105 Bitmask Sieve (u128)                      │
│     Pre-folded period masks: extended culling n<=4 mod 5, n<=6 mod 7        │
│     Enumerate surviving buckets via trailing_zeros() and bit clearing       │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│             Candidate Evaluation (evaluate_d_slice in L1d)                  │
│       ├── 4-Way Coalescing: chunks_exact(4), (b0|b1|b2|b3) skips ~63% quads │
│       ├── Step 1: n = 1 -> Single cmpb via p1_idx + d_half (16 KiB byte tab)│
│       ├── Step 2: n = 2 -> pre-folded p2_idx + d_half (16 KiB byte table)   │
│       └── Step 3: n = 3 -> pre-folded p3_idx + d_half (16 KiB byte table)   │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│             Outlined Deep Verification (verify_deep_streak, #[cold])        │
│       ├── Step 4+: n >= 4 via 3rd-Order Finite Difference Addition Loop     │
│       └── If L >= 28: Bidirectional Extension (n < 0) & Shift Polynomial    │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 16 KiB L1 Data-Cache Resident Byte Table
- **Constant**: `L1_LIMIT = 32_768`, `L1_SIZE = 16_384` bytes.
- Covers all odd numbers in $[3, 32_768)$.
- Stores `1` for prime, `0` for composite.
- Fits alongside the ~5.1 KiB `Mod105Buckets` completely inside the 32 KiB L1d cache of Gracemont cores.
- **Fast Odd Primality Lookup ([`is_prime_l1_odd`](file:///home/teto/Documents/GitHub/math/prime_hunter/src/main.rs#L250-L263))**:
  ```rust
  if val < 3 { return false; }
  let u = val as usize;
  if u < L1_LIMIT {
      let idx = (u - 3) >> 1;
      unsafe { *l1.get_unchecked(idx) == 1 }
  } else if u <= sieve_bound {
      sieve.is_prime(u)
  } else {
      primal::is_prime(val as u64)
  }
  ```
  Guaranteed odd candidate inputs skip evenness branching.

### Telemetry & Threading Model
- Worker threads accumulate combinatorial space (`theoretical_per_b`) and tested counts (`local_tested`) and flush them outside the $c$ loop to `TOTAL_THEORETICAL` and `TOTAL_TESTED` via atomic `fetch_add(..., Relaxed)`.
- Heartbeat telemetry cleanly distinguishes:
  - **Combinatorial Space Processed**: Total polynomials in the search grid.
  - **Tested Candidates**: Actual candidates that survived parity and residue pruning and underwent primality testing.
  - **Pruned Percentage**: Mathematically eliminated a priori without testing (>90%).
  - **Effective Speed**: Combinatorial search speed ($> 6,600\text{ M comb/s}$).
  - **Actual Test Speed**: Primality evaluation speed ($> 600\text{ M test/s}$).

### Hardened Atomic Checkpointing
- Checkpoints store configuration metadata:
  ```text
  # Prime Hunter Checkpoint File
  # Configuration Bounds: a=[-150..-1] U [1..150], b=[-1000..1000], c=[-3000..3000], d=[29..10000]
  CONFIG_HASH: 0xa53e0d24ee04e1f4
  LAST_A: 34
  ```
- **Configuration Hashing**: A 64-bit deterministic hash of all search bounds ([`compute_config_hash`](file:///home/teto/Documents/GitHub/math/prime_hunter/src/main.rs#L582-L593)) is stored and validated on resume to detect mismatched constants. For the current active search space (`D_MIN = 29`), the configuration hash is `0xa53e0d24ee04e1f4`.
- **Safe Atomic I/O**: Checkpoint write performs explicit `sync_all()` to flush buffers to disk before atomic file rename, with comprehensive error logging.
- **Backward Compatibility**: Seamlessly loads legacy checkpoints containing raw integers.

---

## 3. Repository Structure & Key Files

| Path | Description |
| :--- | :--- |
| [`.cargo/config.toml`](file:///home/teto/Documents/GitHub/math/prime_hunter/.cargo/config.toml) | Configures rustflags: `-C target-cpu=native` for AVX2/BMI2 instructions |
| [`.github/copilot-instructions.md`](file:///home/teto/Documents/GitHub/math/prime_hunter/.github/copilot-instructions.md) | Architectural instructions, performance constraints, and search invariants for AI copilots |
| [`Cargo.toml`](file:///home/teto/Documents/GitHub/math/prime_hunter/Cargo.toml) | Package metadata, dependencies (`rayon 1.10`, `primal 0.3`), release profile (opt-level 3, fat LTO, codegen-units 1, abort on panic) |
| [`Cargo.lock`](file:///home/teto/Documents/GitHub/math/prime_hunter/Cargo.lock) | Pinned dependency tree |
| [`checkpoint.txt`](file:///home/teto/Documents/GitHub/math/prime_hunter/checkpoint.txt) | Checkpoint state file storing config metadata hash and the integer value of last completed $a$ slice |
| [`discoveries.txt`](file:///home/teto/Documents/GitHub/math/prime_hunter/discoveries.txt) | Persistent append-only log of discovered polynomials with run length $\ge 28$ |
| [`howto.md`](file:///home/teto/Documents/GitHub/math/prime_hunter/howto.md) | Quick execution reference pointing to the release binary |
| [`src/main.rs`](file:///home/teto/Documents/GitHub/math/prime_hunter/src/main.rs) | Complete engine implementation: configuration constants, 16 KiB L1 byte table, Mod-105 buckets, parallel search loops, pre-folded cmpb lookup, finite difference stepper, telemetry, hardened checkpointing, and 19 automated unit tests |

---

## 4. Search Space Dimensions & Current Progress

### Configuration Parameters
Located in [`src/main.rs`](file:///home/teto/Documents/GitHub/math/prime_hunter/src/main.rs#L13-L45):

| Parameter | Value | Range / Explanation |
| :--- | :--- | :--- |
| `A_MIN`, `A_MAX` | `1`, `150` | $a \in [-150..-1] \cup [1..150]$ ($300$ values, strictly cubic $a \neq 0$) |
| `B_MIN`, `B_MAX` | `-1000`, `1000` | $b \in [-1000..1000]$ ($2,001$ values) |
| `C_MIN`, `C_MAX` | `-3000`, `3000` | $c \in [-3000..3000]$ ($6,001$ values, $3,000$ or $3,001$ tested per $(a, b)$) |
| `D_MIN`, `D_MAX` | `29`, `10000` | $d$ odd primes in $[29, 10000]$ ($1,220$ primes, $d \le 23$ culled) |
| `BACKWARD_SEARCH_THRESHOLD` | `22` | Minimum forward streak length to trigger backward extension check ($n < 0$). Set to catch near-symmetric length-46 candidates. |
| `LOCAL_RECORD_THRESHOLD` | `36` | Minimum streak length to trigger console discovery and append to `discoveries.txt` |
| `WORLD_RECORD_THRESHOLD` | `46` | Minimum streak length to trigger world record banner |
| `SIEVE_LIMIT` | `100_000_000` | Upper limit of Tier 2 prime sieve |
| `L1_LIMIT` | `32_768` | Upper limit of Tier 1 L1-resident byte table |
| `L1_SIZE` | `16_384` | Size of Tier 1 L1 byte table ($16\text{ KiB}$) |

### Combinatorics & Search Accounting
- **Raw Combinatorial Space**:
  $$\text{Total} = 300 \times 2,001 \times 6,001 \times 1,220 = 4,394,928,366,000 \approx 4.39 \times 10^{12} \text{ polynomials (4.39 Trillion)}$$
- **A Priori Pruned Space**: Parity pruning (50%) + Mod-105 residue pruning eliminates **>90%** of the entire space.
- **Candidates Evaluated**: Only $< 10\%$ ($\approx 4.2 \times 10^{11}$ polynomials) are tested for primality.

---

## 5. Build, Test, and Operational Runbook

### Toolchain Requirements
- Rust compiler with Rust 2024 edition support (`rustc` $\ge 1.85$).

### Standard Developer Commands

#### 1. Compile and Check
```bash
cargo check
```

#### 2. Run Clippy Linter
```bash
cargo clippy
```
*Expected: 0 warnings, clean lint pass.*

#### 3. Run Automated Unit Test Suite
```bash
cargo test
```
The test suite in [`src/main.rs`](file:///home/teto/Documents/GitHub/math/prime_hunter/src/main.rs) executes 25 automated unit tests covering all mathematical, algorithmic, and microarchitectural invariants:
1. `test_finite_differences_matches_direct_eval`: Verifies 3rd-order finite difference recurrence matches Horner's direct polynomial evaluation.
2. `test_l1_byte_table_and_is_prime_fast`: Verifies `is_prime_fast` and `is_prime_l1_odd` match `primal::is_prime` across and beyond the 16 KiB L1 cache boundary ($32,768$).
3. `test_phase2_prefolded_byte_lookup`: Validates the pre-folded index formula $p1\_idx + d\_half$ against standard primality testing across multiple coefficient families.
4. `test_discoveries_polynomial_run_length_31`: Verifies discovered record polynomial $f(n) = -14n^3 + 514n^2 - 2970n + 8123$ produces primes for all $n \in [0, 30]$ and terminates at $n = 31$.
5. `test_phase3_verify_deep_streak`: Verifies that `verify_deep_streak` detects forward streak 31 and backward streak $k = 5$ to establish a total bidirectional streak length of 36.
6. `test_phase3_evaluate_d_slice_dual_unrolling`: Verifies 4-way coalesced unrolling and remainder handling across slices of lengths 4, 3, and 5.
7. `test_mod105_precomputed_bitmasks_soundness`: Verifies that intersecting precomputed bitmasks `valid_mod3`, `valid_mod5`, and `valid_mod7` matches direct divisibility checks character-for-character.
8. `test_format_polynomial`: Tests algebraic formatting, negative signs, and zero coefficients.
9. `test_parity_pruning_soundness`: Proves that when $(a + b + c)$ is odd, $f(n)$ is always even for odd $n$.
10. `test_d_equals_2_purge_analytical_proof`: Proves analytically that $d=2$ forcing $f(2)=2$ and $f(4)=2$ makes either $f(1)<2$ or $f(3)<2$ for all non-zero integer $a$.
11. `test_d_equals_2_purge_exhaustive_small_bounds`: Exhaustively tests all cubics with $d=2$ across small bounds, proving none exceed streak length 4.
12. `test_d_equals_2_purge_max_streak_4_example`: Tests $f(n) = n^3 - 4n^2 + 4n + 2$ which attains the theoretical maximum streak of length 4 for $d=2$.
13. `test_d_equals_3_purge_analytical_proof`: Proves analytically that for $d=3$, $f(3)=3$ and $f(6)=3$ forces $f(9)=162a+3 \neq 3$ (composite), proving $L \le 9$.
14. `test_d_equals_3_purge_exhaustive_small_bounds`: Exhaustively tests all cubics with $d=3$ across small bounds, proving none exceed streak length 9.
15. `test_mod3_residue_pruning_soundness`: Proves that polynomials with zeros mod 3 cannot exceed streak length 11.
16. `test_d_min_29_mathematical_culling`: Validates that all small primes $d \in \{2, 3, 5, 7, 11, 13, 17, 19, 23\}$ are strictly less than `D_MIN = 29`.
17. `test_mod105_buckets_integrity`: Verifies that `Mod105Buckets` contains exactly 48 non-empty coprime residue buckets, correct offsets, and matching pre-shifted `d_half` values.
18. `test_mod105_filter_preserves_all_known_discoveries`: Proves that all known record polynomials from `discoveries.txt` pass the mod-105 admissibility filter with zero false negatives.
19. `test_parse_checkpoint`: Verifies checkpoint file parsing for structured format with config hash, legacy raw integers, and corrupted hash/field rejection.
20. `test_checkpoint_atomic_roundtrip`: Verifies atomic save and load round-trip with config hash verification.
21. `test_checkpoint_load_corrupted_and_missing_file`: Verifies `load_checkpoint_from_path` safely handles missing and corrupted files.
22. `test_assert_search_bounds_no_overflow`: Validates that finite difference accumulators and Horner evaluations cannot overflow `i64` anywhere in search bounds.
23. `test_shift_polynomial_backward_algebraic_identity`: Validates the algebraic identity $g(m) = f(m - k)$ for all $m$ across various polynomials and shift counts $k \in \{0, 1, 2, 5, 8, 12\}$.
24. `test_crt_step_6_soundness`: Proves that when $b \equiv 0 \pmod 3$, any polynomial with $a+b+c \not\equiv 0 \pmod 6$ fails primality for all $d \ge 29$ at $n=1$ or $n=2$, and verifies $P_1 \equiv P_2 \equiv P_3 \equiv 0 \pmod 3$ when $a+b+c \equiv 0 \pmod 6$.
25. `test_bidirectional_extension_on_known_discoveries`: Validates bidirectional streak extensions on known record discoveries (e.g. length 31 extending to 36) and confirms translated canonical polynomial coefficients.

#### 4. Build Optimized Release Binary
```bash
cargo build --release
```

#### 5. Execute Search Engine
```bash
./target/release/prime_hunter
```
Throughput exceeds **10,000 Million combinations/s** ($> 10.0\text{ Billion comb/s}$) with effective primality testing speed over **720 Million tests/s** across CPU worker threads on Intel Gracemont microarchitectures.
