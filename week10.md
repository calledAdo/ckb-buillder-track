# Builder Track Weekly Report — Week 10

**Name:** Adokiye

## ✅ Completed Tasks

- Set up the `tests/` crate in the `lenderr_project` workspace with `ckb-testtool` as a development dependency.
- Wrote **19 math unit tests** in `tests/src/math_tests.rs` covering all functions in `lenderr-common/src/math.rs`.
- Successfully executed all 19 math tests on the host target — **all pass**.
- Designed the full integration test plan for each protocol action (deposit, withdraw, borrow, repay, freeze, liquidate) using `ckb-testtool` mock transactions.
- Attempted to cross-compile contract binaries to `riscv64imac-unknown-none-elf` for use in `ckb-testtool` integration tests.
- Diagnosed the RISC-V build failure root cause: the `blake2b-rs` dependency in `pool_script` uses a C implementation of BLAKE2 that requires standard C headers (`string.h`, `stdint.h`) unavailable in the bare-metal cross-compilation environment.
- Investigated multiple C compiler solutions: the system `riscv64-unknown-elf-gcc` (built `--without-newlib`, no headers) and `clang-18` (has `stdint.h` via built-in resource dir, but lacks `string.h` for bare-metal targets).
- Created a stub headers directory (`stub_headers/`) with minimal no-dependency implementations of `string.h` and `stdlib.h` for use with clang's `-isystem` flag.

## 📚 Key Learning Areas

### 1. `ckb-testtool` Framework and the Offline CKB VM

The `ckb-testtool` crate provides a complete simulation of the CKB transaction verification environment without requiring a running node. It:

- Deploys compiled RISC-V binaries into a mock cell store via `context.deploy_cell(binary_data)`, returning an `OutPoint` that can be referenced in `CellDep`s.
- Builds `Script` objects from those deployed binaries to use as lock or type scripts on mock cells.
- Executes the actual RISC-V VM (`ckb-vm`) against the constructed transaction, returning the script exit code.

This means **integration tests run against the real CKB script execution engine** — not a simulation of it — which catches subtle VM-level behavior (e.g., memory access patterns, instruction set support) that host-compiled tests cannot.

```rust
// Example test skeleton for a borrow action
let mut context = Context::default();
let pool_type_bin = fs::read(BINARY_PATHS.pool_type).unwrap();
let pool_type_dep = context.deploy_cell(pool_type_bin.into());

let pool_type_script = context.build_script(&pool_type_dep, pool_args.into()).unwrap();

// Build input: existing pool cell
let pool_input = context.create_cell(
    CellOutput::new_builder()
        .capacity(500u64.pack())
        .type_(Some(pool_type_script.clone()).pack())
        .lock(always_success.clone())
        .build(),
    pool_state_bytes.into(),
);

// Build input: borrow intent cell
let intent_input = context.create_cell(/* ... */);

// Build output: updated pool cell
let pool_output = CellOutput::new_builder()
    .type_(Some(pool_type_script.clone()).pack())
    /* ... */
    .build();

// Execute and assert
let tx = /* build transaction skeleton */;
let result = context.verify_tx(&tx, u64::MAX);
assert!(result.is_ok(), "borrow should succeed");
```

### 2. Math Unit Tests — Coverage and Results

All 19 tests in `math_tests.rs` pass on the host target with `cargo test -p lenderr-tests`:

**Utilization tests (3):**
- Empty pool returns `0` utilization.
- Fully borrowed pool returns `UTIL_PRECISION` (1_000_000).
- 20% borrowed pool returns `UTIL_PRECISION / 5`.

**Annual rate tests (3):**
- Zero utilization → `base_rate` (200 bps).
- Full utilization → `max_rate` (2000 bps).
- Half utilization → midpoint (1100 bps), confirming linearity.

**Duration formula tests (4):**
- `Q < k_min` → `None` (undercollateralized, borrow rejected).
- `Q == k_min` → `Some(0)` (zero-duration loan allowed as edge case).
- `Q == k_target`, `U ≈ 0` → `Some(t_max)` (maximum duration).
- `U = 80%` with `Q = k_target` → `Some(t_max / 5)` confirming the `(1 − U)` factor.

**Exchange rate tests (4):**
- Empty pool → rate equals `RATE_PRECISION` (1:1 baseline).
- No interest → rate stays `RATE_PRECISION`.
- Interest accrued → rate increases proportionally.
- LP tokens computed from rate are correct for deposit and burn scenarios.

**Interest adjustment tests (3):**
- No expired vaults → full `rate_net × elapsed` accrual.
- Vault expired before last update → overcharge calculated from `t_last`.
- Vault expired during current window → overcharge calculated from `t_exp`.

**Vault interest owed (1):**
- `r_vault × elapsed / RATE_PRECISION` matches expected token amount.

**LP tokens for deposit (1):**
- `lp_tokens = deposit × RATE_PRECISION / rate` correctly computes fewer LP tokens at elevated rates (later depositors buy in at a higher price).

Running the tests:
```bash
cd lenderr_project
cargo test -p lenderr-tests -- --nocapture
```
```
running 19 tests
test test_utilization_empty_pool ... ok
test test_utilization_full ... ok
test test_utilization_20_percent ... ok
test test_rate_at_zero_utilization ... ok
test test_rate_at_full_utilization ... ok
test test_rate_at_half_utilization ... ok
test test_duration_undercollateralized_returns_none ... ok
test test_duration_at_k_min_returns_zero ... ok
test test_duration_at_k_target_low_utilization ... ok
test test_duration_shrinks_at_high_utilization ... ok
test test_exchange_rate_empty_pool_is_one ... ok
test test_exchange_rate_no_interest ... ok
test test_exchange_rate_with_interest ... ok
test test_lp_tokens_at_1_to_1_rate ... ok
test test_lp_tokens_at_1_21_rate ... ok
test test_adjustment_no_vault_closure ... ok
test test_adjustment_with_expired_vault_before_last_update ... ok
test test_adjustment_with_vault_expiring_in_window ... ok
test test_vault_interest_owed ... ok

test result: ok. 19 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### 3. Integration Test Architecture Plan

The following integration tests were designed and scaffolded for `ckb-testtool` execution once the RISC-V binaries compile:

**Deposit tests:**
- First deposit at 1:1 rate mints LP tokens equal to deposit amount.
- Second deposit at an elevated rate (pool has accrued interest) mints fewer LP tokens.
- Attempting to deposit without a valid `deposit_intent_lock` fails with an error.
- Pool `liquidity_available` increases by exactly the deposit amount.

**Withdraw tests:**
- Burning LP tokens returns the proportional share of pool TVL (principal + interest).
- Lender captures yield: Asset B returned exceeds original deposit when interest has accrued.
- Burning more LP than the lender holds is rejected.
- Pool `liquidity_available` decreases by exactly the payout.
- `interest_accrued` and `timestamp_last_update` are NOT committed to state during a withdraw (only projected for rate calculation).

**Borrow tests:**
- Borrow with `Q < k_min` is rejected (undercollateralized).
- Valid borrow produces a vault cell with correct `t_exp`, `r_vault`, and `is_frozen = 0` in lock args.
- Pool `liquidity_available` decreases by principal; `total_debt` and `rate_net` increase accordingly.
- Borrowing more than available liquidity is rejected.

**Repay tests:**
- Repaying less than `principal + interest_owed` is rejected.
- Successful repay removes the vault cell, increases pool liquidity by the repayment amount, and decreases `rate_net` by `r_vault`.
- `interest_accrued` updates correctly via lazy evaluation then reduces by `interest_owed`.

**Freeze tests:**
- Cannot freeze a vault before `t_exp`.
- Freeze flips `is_frozen` from `0` to `1` in vault lock args; all other args fields are unchanged.
- `rate_net` decreases by exactly `r_vault`.
- Phantom interest is correctly subtracted from `interest_accrued`.
- Pool Asset B balance is physically unchanged by freeze.
- Batch freeze: multiple expired vaults frozen in one transaction.

**Liquidation tests:**
- Bid below Dutch auction price is rejected.
- Bid at or above auction price transfers collateral to liquidator and repays pool.
- Frozen vault has no `r_vault` deduction (already removed at freeze time).
- Haircut triggers upward `k_min` ratchet; clean liquidation applies peace-time decay.

### 4. RISC-V Cross-Compilation Challenge: `blake2b-rs`

The integration tests require compiled RISC-V binaries. The build fails because `pool_script` depends on `blake2b-rs`, a Rust crate that wraps the **BLAKE2 reference C implementation**. The C source needs standard headers (`stdint.h`, `string.h`) that are unavailable in the bare-metal environment.

**Root cause chain:**
1. `pool_script` uses `Blake2bBuilder` to compute the Type ID hash in `validate_creation()`.
2. `blake2b-rs` builds `BLAKE2/ref/blake2b-ref.c` using the `cc-rs` build system crate.
3. The `cc-rs` crate invokes a C compiler for the `riscv64imac-unknown-none-elf` target.
4. The system `riscv64-unknown-elf-gcc` was compiled `--without-newlib` — it has no C standard library headers at all.
5. Switching to `clang-18` resolves `stdint.h` (available in Clang's built-in resource directory at `/usr/lib/llvm-18/lib/clang/18/include/`) but `string.h` is still missing — Clang's built-in headers don't provide `memcpy`/`memset`/`memmove` for bare-metal targets.

**Solutions investigated:**

| Approach | Outcome |
|----------|---------|
| `riscv64-unknown-elf-gcc` (system) | No headers at all — fails on `stdint.h` |
| `clang-18` with `-nostdinc` | Fails on `stdint.h` (clang built-ins excluded) |
| `clang-18` without `-nostdinc` | Resolves `stdint.h`, but fails on `string.h` |
| Stub headers via `-isystem` | Under implementation — provides `memcpy`/`memset` as inline functions |

The `stub_headers/` directory approach creates a minimal `string.h` with compiler-builtin implementations of `memset`, `memcpy`, `memmove`, and `memcmp` using only `__SIZE_TYPE__` (always available in any C compiler):

```c
static inline void *memcpy(void *dst, const void *src, size_t n) {
    unsigned char *d = (unsigned char *)dst;
    const unsigned char *s = (const unsigned char *)src;
    while (n--) *d++ = *s++;
    return dst;
}
```

By passing `-isystem /path/to/stub_headers` alongside Clang's built-in resource directory, the BLAKE2 C code gains access to all required headers without any system libc.

### 5. Applying Lessons from `a_token` to LendNerv Integration Tests

Previous experience with the `a_token` `ckb-testtool` suite (weeks 5–7) provided directly transferable patterns:

- **`ALWAYS_SUCCESS` for lock scripts during unit testing:** In integration tests that focus on pool_type logic, vault cells can use the `ALWAYS_SUCCESS` built-in binary as their lock script to avoid running the vault_lock VM — keeping tests focused on the pool behavior.
- **Separate test modules per action:** Structuring tests into separate modules (`test_deposit.rs`, `test_borrow.rs`, etc.) using `ckb-testtool`'s `Context` mirrors the `a_token` test organization and prevents state leakage between scenarios.
- **Blake2b for cell ID computation in tests:** The `a_token` suite manually reproduced the Blake2b vault-ID computation in test code to assert the correct vault ID was embedded. The same approach will be used in LendNerv borrow tests to verify the vault cell's lock args contain the correct `pool_type_hash` and loan parameters.
- **Error code assertions:** `context.verify_tx(&tx, u64::MAX)` returns the script exit code. Asserting specific non-zero error codes (e.g., `ERROR_BID_TOO_LOW`, `ERROR_VAULT_NOT_EXPIRED`) confirms not just that a transaction was rejected, but that it failed for the **correct reason**.

## 🛑 Assumptions Corrected

1. **Math Tests Are Sufficient Without RISC-V Integration Tests**
   - *Assumption*: Verifying the math functions in host-compiled unit tests provides full protocol correctness coverage.
   - *Correction*: The math tests verify formula correctness, but they don't test script execution logic — action detection, cell layout validation, field continuity checks, and cross-cell data reads all run inside the RISC-V VM and can only be tested by `ckb-testtool` integration tests. The math tests are necessary but not sufficient.

2. **`clang --target=riscv64-unknown-none-elf` Would Provide All Required Headers**
   - *Assumption*: Using `clang-18` with the correct target triple would give the bare-metal build everything it needed, since Clang is a full cross-compiler.
   - *Correction*: Clang ships with architecture-independent built-in headers (intrinsics, `stdint.h`, `stddef.h`) in its resource directory, but it does not ship a standard C library (`string.h`, `stdlib.h`) for bare-metal targets. Those headers belong to the C runtime (`libc`/`newlib`/`picolibc`), which must be provided separately. The stub headers approach compensates for this gap by providing the minimal subset that BLAKE2 actually uses.

3. **RISC-V Build Would Just Work After Switching Compilers**
   - *Assumption*: The only issue was the wrong C compiler being selected by `cc-rs`.
   - *Correction*: The compiler selection issue (using the system GCC with no headers) was the first layer. After switching to `clang-18`, a second layer emerged: the `-nostdinc` flag stripped all includes (including Clang's own `stdint.h`), then removing `-nostdinc` resolved `stdint.h` but exposed the missing `string.h`. Each fix uncovered the next underlying constraint. Cross-compilation for bare-metal targets requires careful layering of the toolchain.

## 🧪 Running the Math Tests

Math unit tests run immediately on any machine with a standard Rust toolchain:

```bash
cd lenderr_project
cargo test -p lenderr-tests -- --nocapture
```

Once the RISC-V stub headers fix is complete, the full integration test suite will be built and run with:

```bash
# Build RISC-V binaries
CC_riscv64imac_unknown_none_elf=clang \
CFLAGS_riscv64imac_unknown_none_elf="--target=riscv64-unknown-none-elf \
  -march=rv64imac -mabi=lp64 -ffreestanding -fno-builtin \
  -isystem $(pwd)/stub_headers \
  -isystem $(clang -print-resource-dir)/include" \
cargo build --target riscv64imac-unknown-none-elf --release \
  -p pool_script -p vault_scripts -p intent_scripts -p pool_lock

# Run integration tests (host target, VM simulation)
cargo test -p lenderr-tests -- --nocapture
```
