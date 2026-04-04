# Builder Track Weekly Report — Week 9

**Name:** Adokiye

## ✅ Completed Tasks

- Fully implemented all five crates in the `lenderr_project` workspace, producing a complete on-chain lending protocol.
- Implemented the **`lenderr-common`** shared library with six modules: `math`, `pool_data`, `vault_data`, `errors`, `utils`, and the Molecule-generated `lendnerv`.
- Implemented **`pool_script`** (`pool_type` binary) — the master rules engine covering all seven protocol actions: deposit, withdraw, borrow, repay, freeze, and liquidate (both single and batch modes).
- Implemented **`vault_scripts`** — both `vault_lock` (authorization for vault spending) and `vault_type` (structural integrity during vault creation) as separate binaries from a single crate.
- Implemented **`intent_scripts`** — three lock scripts: `deposit_intent_lock`, `withdraw_intent_lock`, and `borrow_intent_lock`, each enforcing their respective pool output preconditions atomically.
- Implemented **`pool_lock`** — a minimal administrative lock enforcing pool governance.
- Made a critical architectural migration: moved all vault loan metadata from the xUDT cell data field into the vault **lock script args** to preserve xUDT token compatibility.
- Renamed `vault_lock_hash` → `vault_lock_code_hash` throughout `PoolData`, the Molecule schema, and all validators.
- Merged two near-identical vault-scanning functions (`find_all_vaults` and `find_all_vaults_indexed`) into a single unified function.
- Identified and patched a security vulnerability in `validate_freeze`: the physical Asset B balance check was missing.
- Standardized naming: `asset_b_in`/`asset_b_out` → `pool_asset_b_in`/`pool_asset_b_out` across all action validators.

## 📚 Key Learning Areas

### 1. Lazy Interest Accrual — The UTXO Timing Problem

Traditional lending protocols update interest balances every block. On CKB, the pool cell is only updated when a transaction spends it, so we must calculate all interest retroactively at the moment the pool is touched.

The **`interest_adjustment`** function computes the exact interest earned since the last update:

```rust
// Naive approach: rate_net × elapsed_seconds / RATE_PRECISION
// Problem: expired vaults are still counted in rate_net even after t_exp
//
// Correction: subtract the phantom overcharge
let max_interest = rate_net * elapsed / RATE_PRECISION;
let overcharge   = r_vault * (t_now.saturating_sub(t_exp.max(t_last))) / RATE_PRECISION;
max_interest - overcharge
```

The `r_vault` correction term handles two cases simultaneously:
- Vault expired **before** the last pool update: overcharge starts from `t_last`
- Vault expired **within** the current window: overcharge starts from `t_exp`

The `max(t_last, t_exp)` expression elegantly collapses both cases into a single formula, which was a hard-won insight.

### 2. Pool Action Detection Pattern

Because the CKB VM has no external context for why a transaction was submitted, the `pool_type` script must infer the intended action from the shape of the transaction itself. The `detect_action` function implements this by scanning the transaction:

- **Deposit**: presence of a `deposit_intent_lock` cell in inputs
- **Withdraw**: presence of a `withdraw_intent_lock` cell in inputs
- **Borrow**: presence of a `borrow_intent_lock` cell in inputs
- **Repay**: vault cells in inputs with `is_frozen == 0` and `t_now < t_exp`
- **Freeze**: vault cells in inputs with `is_frozen == 0` and `t_now >= t_exp`
- **Liquidate**: vault cells in inputs with `t_now >= t_exp` (frozen or not)

This detection-first pattern eliminates the need for any explicit "action selector" in transaction witnesses, making the protocol interaction purely declarative.

### 3. xUDT Compatibility — Vault Metadata Migration to Lock Args

The vault cells hold collateral using xUDT (extended User Defined Token) — a standard that stores token balance as a 16-byte LE u128 at `data[0..16]`. Custom xUDT tokens may store additional extension data at `data[16+]`.

The original design stored vault loan metadata (principal, r_vault, t_created, t_exp, is_frozen) in `data[16..65]`, directly conflicting with custom token extensions.

**The fix:** Moved all loan metadata to the vault **lock script args**. The new 113-byte args layout:

| Offset | Length | Field |
|--------|--------|-------|
| `0..32` | 32 bytes | `borrower_lock_hash` |
| `32..64` | 32 bytes | `pool_type_hash` |
| `64..80` | 16 bytes | `principal` (u128 LE) |
| `80..96` | 16 bytes | `r_vault` (u128 LE) |
| `96..104` | 8 bytes | `t_created` (u64 LE) |
| `104..112` | 8 bytes | `t_exp` (u64 LE) |
| `112` | 1 byte | `is_frozen` (0 or 1) |

The vault cell data is now a pure 16-byte xUDT balance. Any standard or custom xUDT token can be used as collateral without conflicts.

`VaultData::from_lock_args(args, collateral_amt)` reads the metadata from args:
```rust
if args.len() < VAULT_ARGS_LEN { return None; }
let principal = u128::from_le_bytes(args[64..80].try_into().unwrap());
let r_vault   = u128::from_le_bytes(args[80..96].try_into().unwrap());
let t_created = u64::from_le_bytes(args[96..104].try_into().unwrap());
let t_exp     = u64::from_le_bytes(args[104..112].try_into().unwrap());
let is_frozen = args[112];
```

### 4. Dutch Auction Liquidation Pricing

When a vault expires, any liquidator can purchase it via a continuous Dutch auction. The auction starts at `base_debt × 1.05` (5% premium) at `t_exp` and decays continuously:

```
price(t) = start_price × RATE_PRECISION / (RATE_PRECISION + (t - t_exp) × 100)
```

With `RATE_PRECISION = 10^12` and decay constant `100` per second, the 5% premium erodes over approximately 500 million seconds — ensuring liquidators have a very long window without being forced to act instantly, while still giving the protocol a meaningful price discovery mechanism.

A bid below `price(t_now)` is rejected. The liquidator pays `price(t_now)` and receives all collateral.

### 5. Freeze Action and Phantom Interest Correction

When an expired vault has not been liquidated, it continues contributing its `r_vault` to `rate_net`, causing the pool to overcount interest. The **freeze** action resolves this:

1. `is_frozen` bit in vault lock args flips `0 → 1`
2. `r_vault` is subtracted from `rate_net` permanently
3. **Phantom interest** is clawed back from `interest_accrued`:
   ```
   phantom = r_vault × (t_now − t_exp) / RATE_PRECISION
   interest_accrued = max(0, interest_accrued − phantom)
   ```
4. Pool `total_debt` and `liquidity_available` are **not** changed — the debt is still outstanding, just the rate contribution is removed.

The frozen vault persists as a cell, allowing the liquidation path to process it later without re-subtracting `r_vault` from `rate_net` (since it was already removed at freeze time).

**Security fix applied:** The original `validate_freeze` implementation failed to verify that the pool's physical Asset B token balance was unchanged by the freeze transaction. A malicious transaction could have manipulated the pool balance while passing all declared field checks. The fix enforces:
```rust
let pool_asset_b_in  = compute_sudt_total(Source::Input,  &asset_b_type_hash, &pool_lock);
let pool_asset_b_out = compute_sudt_total(Source::Output, &asset_b_type_hash, &pool_lock);
if pool_asset_b_out != pool_asset_b_in { return ERROR_DEBT_ACCOUNTING; }
```

### 6. Self-Healing `k_min` Ratchet

When a liquidation results in a haircut (the auction price is below `base_debt`), the protocol raises `k_min_current` as a penalty — tightening future borrowing collateral requirements:

```
severity_bps    = haircut × 10_000 / tvl
time_multiplier = 1 + (HEALING_PERIOD − elapsed) / HEALING_PERIOD  [capped at 1 if outside window]
penalty         = k_min_current × severity_bps × time_multiplier / 100_000_000
k_min_current   = min(k_min_current + penalty, k_target × K_MAX_SCALE)
```

The `time_multiplier` amplifies penalties for consecutive haircuts within the 14-day healing window — a protocol that suffers repeated bad liquidations is penalized progressively harder.

Without new haircuts, `k_min_current` decays by `DECAY_STEP = 2` for every 14-day `HEALING_PERIOD` elapsed, eventually returning to the governance floor `k_min`.

### 7. `Source::GroupInput` vs `Source::Input`

A nuanced but important CKB API distinction:
- `Source::GroupInput` / `Source::GroupOutput` — iterates only over cells that belong to the **same script group** (i.e., share the same script hash as the currently executing script). Used for self-referential checks.
- `Source::Input` / `Source::Output` — iterates over **all** inputs/outputs in the transaction. Required when scanning for other cells (e.g., vault cells, intent cells).

Using `Source::GroupInput` when `Source::Input` was needed (or vice versa) was a common early mistake that caused subtle logic errors: the pool type script uses `Source::GroupInput` to find itself but `Source::Input`/`Source::Output` to find vault cells.

## 🛑 Assumptions Corrected

1. **`find_all_vaults` and `find_all_vaults_indexed` Were Separate for Good Reason**
   - *Assumption*: The two vault-scanning functions — one returning `Vec<VaultData>` and one returning `Vec<(usize, VaultData)>` — served genuinely different purposes and needed separate implementations.
   - *Correction*: Both functions were nearly identical loops differing only in whether the output index was retained. After analysis, `validate_freeze` was the only consumer of the indexed version (to match input/output vault cells by position). It was refactored to use the same unified `find_all_vaults` returning `Vec<(usize, VaultData)>` everywhere, with callers stripping the index when not needed via `.map(|(_, vd)| vd)`.

2. **`load_cell_lock_hash` Gives the Same Value as `vault_lock_code_hash`**
   - *Assumption*: In `validate_freeze`, comparing `load_cell_lock_hash(i, Source::Output)` against the stored `vault_lock_code_hash` would correctly identify vault cells.
   - *Correction*: `load_cell_lock_hash` returns a hash of the **complete** lock script (including args), which is unique per vault. `vault_lock_code_hash` stores only the hash of the **binary** (the `code_hash` field of the lock Script object), which is constant. These two values are never equal. The fix uses `load_cell_lock(i, Source::Output)?.code_hash()` to extract just the code hash for comparison.

3. **Renaming `asset_b_in` Was a Simple Find-and-Replace**
   - *Assumption*: Renaming `asset_b_in` → `pool_asset_b_in` across the validator functions was a straightforward text substitution.
   - *Correction*: `validate_borrow` already had a variable called `pool_asset_b_in` (introduced earlier in the function to track incoming pool balance). Applying the rename blindly to that function produced `pool_pool_asset_b_in` — a double-prefixed variable that caused a compilation error. The rename had to be applied selectively, and the pre-existing `pool_asset_b_in` in `validate_borrow` was verified not to be renamed.

## 🧪 Verification

All host-target crates compile cleanly:

```bash
cd lenderr_project
cargo build --release            # host target (x86_64) — confirms logic compiles
cargo build -p lenderr-common    # verifies shared library
```

The RISC-V contract binaries require cross-compilation:

```bash
cargo build --target riscv64imac-unknown-none-elf --release \
  -p pool_script -p vault_scripts -p intent_scripts -p pool_lock
```

*Note: Cross-compilation requires resolving a C compiler dependency (`blake2b-rs`) for the RISC-V bare-metal target. Work on this is ongoing in Week 10.*
