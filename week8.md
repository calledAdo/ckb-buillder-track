# Builder Track Weekly Report — Week 8

**Name:** Adokiye

## ✅ Completed Tasks

- Conceived and formally designed the **LendNerv Protocol** — a priceless, oracle-free lending protocol built natively on CKB's UTXO cell model.
- Settled on the core economic innovation: replacing Loan-To-Value (LTV) ratios with a **time-pricing (duration formula)** approach that requires no external price feed.
- Designed the full multi-crate Cargo workspace (`lenderr_project/`) with five members: `lenderr-common`, `pool_script`, `vault_scripts`, `intent_scripts`, and `tests`.
- Drafted the complete **PoolData** binary layout (224 bytes, raw LE, no Molecule yet) and the **VaultData** binary layout for vault cells.
- Designed the Molecule schema (`lendnerv.mol`) for on-chain `PoolState` serialization.
- Identified and resolved the `no_std` workspace compilation challenge — choosing unconditional `#![no_std]` for `lenderr-common` to avoid feature-unification conflicts.
- Established the naming conventions, error code taxonomy, and architectural boundaries between all five crates.

## 📚 Key Learning Areas

### 1. Priceless Lending Protocol Design
- **The Oracle Problem on CKB:** Most DeFi lending protocols (Aave, Compound, etc.) require real-time price feeds (oracles) to assess collateral value and trigger liquidations. On CKB, running a decentralized oracle reliably is non-trivial and introduces liveness risks. LendNerv sidesteps this entirely.
- **Time-Pricing as an Alternative:** Instead of asking "what is the collateral worth in USD?", the protocol asks "given the borrower's collateral ratio Q = collateral_tokens / loan_tokens, how long should this loan be allowed to run?" The higher the collateral ratio and the lower current pool utilization, the longer the loan duration. No oracle is needed — the ratio Q is derived from the physical token quantities submitted in the transaction.
- **Core Duration Formula:**
  ```
  D = min(t_max, t_max × ((Q − k_min) / (k_target − k_min)) × (1 − U))
  ```
  Where `k_min` and `k_target` are governance-set collateral thresholds, `t_max` is the maximum loan duration, and `U` is current pool utilization. If `Q < k_min`, the borrow is rejected outright as undercollateralized.

### 2. UTXO Cell Architecture for a Lending Protocol
- **The Statefulness Problem:** Traditional lending protocols (smart contracts on EVM) maintain a continuously updated global state. On CKB, there is no globally mutable storage — state lives inside individual cells that are created and destroyed per transaction. This required rethinking how interest accrual, pool accounting, and vault tracking would work.
- **Singleton Pool Cell:** The protocol uses a single "pool cell" (enforced by the Type ID mechanism) whose data field encodes the entire pool state as a Molecule-serialized blob. Every protocol action (deposit, borrow, repay, etc.) must consume this cell as an input and produce an updated version as an output.
- **Intent Cells for Atomic Operations:** Because CKB cells can only be spent by their lock script, a deposit cannot directly credit the pool. Instead, the user creates a "deposit intent" cell whose lock script, upon being satisfied, guarantees the correct pool output is present. This makes multi-party interactions atomic.

### 3. Workspace `no_std` Strategy
- **The `no_std` Requirement for Contract Crates:** CKB contract binaries (compiled to `riscv64imac-unknown-none-elf`) run in a bare-metal RISC-V VM with no operating system. They must be `#![no_std]`, using only the `core` crate (and optionally `alloc` for heap-allocated types).
- **Feature Unification Conflict:** Initially considered making `lenderr-common` conditionally `std` (for the test harness) and `no_std` (for contracts). However, Cargo's feature unification means that if any crate in the workspace enables the `std` feature, it propagates to all dependents. This caused a `duplicate panic_impl` linker error when contract crates used `ckb_std::entry!()`.
- **Solution — Unconditional `no_std`:** `lenderr-common` was made unconditionally `#![no_std]`. The `tests` crate can still use `lenderr-common` directly on the host since `core` is always available, and the host test runner supplies the necessary `panic` handler automatically.

### 4. Fixed-Point Math Design
- **Why Fixed-Point?** Smart contracts cannot use floating-point arithmetic because IEEE 754 results vary across hardware implementations, breaking determinism. All fractional math is performed with integer scaling.
- **Precision Constants:**
  - `RATE_PRECISION = 10^12` — scales per-second rate values (`r_vault`) high enough that interest doesn't round to zero for short time windows on small principals.
  - `UTIL_PRECISION = 10^6` — scales utilization from `0` (empty) to `1_000_000` (100% borrowed).
- **`r_vault` Computation:** The per-second rate stored on each vault is:
  ```
  r_vault = principal × rate_bps × RATE_PRECISION / (10_000 × SECONDS_PER_YEAR)
  ```
  This allows interest to be computed retroactively as `r_vault × elapsed_seconds / RATE_PRECISION` with no precision loss for practical loan sizes.

### 5. Molecule Serialization
- **What is Molecule?** Molecule is CKB's canonical binary serialization format — deterministic, minimal, and suitable for on-chain parsing in bare-metal RISC-V environments.
- Learned how to write a `.mol` schema file, compile it with `moleculec`, and import the generated Rust bindings (`lendnerv.rs`) into `lenderr-common`.
- The `PoolState` table in the schema encodes all 18 fields: liquidity, debt, interest accrued, rate_net, LP supply, timestamps, governance parameters, and type hash identifiers.

## 🛑 Assumptions Corrected

1. **Pool State Could Be Stored in a Plain Cell Data Field**
   - *Assumption*: Assumed the pool state could simply be raw bytes appended to a cell's data field.
   - *Correction*: While technically possible, raw bytes are brittle and lack any schema versioning. Molecule provides length-prefixed tables with forward-compatible field ordering, making on-chain parsing robust and upgradeable. The `PoolState` Molecule table was adopted as the canonical encoding from the start.

2. **Single Crate Could Handle Everything**
   - *Assumption*: A single Rust crate with conditional compilation could serve as both the shared library and the individual contract binaries.
   - *Correction*: CKB contracts require separate compiled binaries. The `[[bin]]` target mechanism in Cargo was used: `pool_script` produces a `pool_type` binary, `vault_scripts` produces `vault_lock` and `vault_type` binaries from the same crate using different source entry points, and `intent_scripts` produces three separate intent binaries.

3. **Vault Lock Hash Identifies Vault Cells Uniquely**
   - *Assumption*: Storing the full `vault_lock_hash` (a hash of the complete lock script including args) in `PoolData` would be sufficient to identify vault cells belonging to the pool.
   - *Correction*: Each vault cell has a unique full lock hash because the lock args differ per borrower. The pool cannot enumerate vaults by full lock hash since that value is only known after the vault is created. Instead, the `vault_lock_code_hash` — the hash of the lock script binary itself, which is constant across all vaults — combined with the `asset_a_type_hash` (collateral token type), forms the correct vault-identification filter.

## 🧪 Verification

The workspace compiles (host targets) with:

```bash
cd lenderr_project
cargo check -p lenderr-common -p pool_script -p vault_scripts -p intent_scripts
```

All five workspace members resolve their dependency graph correctly. The math module in `lenderr-common` is fully implemented and ready for unit testing.
