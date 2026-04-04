//! Integration tests for the Deposit action through pool_type + pool_lock.
//!
//! Structure mirrors lenderr_tests/tests/deposit/deposit.test.ts:
//!
//!  1. Deploy pool_type, pool_lock, vault_lock, test_token_type (sUDT).
//!  2. Create three token type scripts: asset_a, asset_b, lp_token.
//!  3. Create the singleton pool cell (lock=pool_lock, type=pool_type).
//!  4. Create a lender Asset-B cell to fund the deposit.
//!  5. Submit a deposit transaction and assert pool_type accepts/rejects it.
//!
//! Transaction cell layout (deposit):
//!
//!  Inputs
//!  ------
//!  [0] pool_cell        lock=pool_lock   type=pool_type  data=initial_PoolData
//!  [1] lender_asset_b   lock=lender_lock type=asset_b    data=lender_balance (u128 LE)
//!
//!  Outputs
//!  -------
//!  [0] new_pool_cell    lock=pool_lock   type=pool_type  data=updated_PoolData
//!  [1] pool_custody     lock=pool_lock   type=asset_b    data=deposit_amount (u128 LE)
//!  [2] lender_change    lock=lender_lock type=asset_b    data=balance - deposit_amount
//!  [3] lender_lp        lock=lender_lock type=lp_token   data=expected_lp (u128 LE)
//!
//!  Header dep: block header whose timestamp (÷1000) == new_pool.timestamp_last_update

use ckb_testtool::{
    ckb_types::{
        bytes::Bytes,
        core::{HeaderBuilder, TransactionBuilder},
        packed::{CellDep, CellInput, CellOutput},
        prelude::*,
    },
    context::Context,
};
use lenderr_common::{
    math::{exchange_rate, interest_adjustment, lp_tokens_for_deposit, RATE_PRECISION},
    pool_data::PoolData,
};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Maximum CKB-VM cycles per test run.
const MAX_CYCLES: u64 = 100_000_000;

/// Deposit amount: 10 000 Asset-B with 8-decimal precision.
const DEPOSIT_AMOUNT: u128 = 10_000 * 100_000_000;

/// Lender's initial Asset-B balance (5× the deposit so there is always change).
const LENDER_INITIAL_BALANCE: u128 = DEPOSIT_AMOUNT * 5;

/// Pool state initial timestamp (seconds).
const T_INITIAL: u64 = 1_000_000;

// ── Binary loaders ────────────────────────────────────────────────────────────

fn load_pool_type(ctx: &mut Context) -> ckb_testtool::ckb_types::packed::OutPoint {
    let bin = std::fs::read("../target/riscv64imac-unknown-none-elf/release/pool_type")
        .expect("missing pool_type binary — run: cargo build -p pool_script --release --target riscv64imac-unknown-none-elf");
    ctx.deploy_cell(bin.into())
}

fn load_pool_lock(ctx: &mut Context) -> ckb_testtool::ckb_types::packed::OutPoint {
    let bin = std::fs::read("../target/riscv64imac-unknown-none-elf/release/pool_lock")
        .expect("missing pool_lock binary — run: cargo build -p pool_lock --release --target riscv64imac-unknown-none-elf");
    ctx.deploy_cell(bin.into())
}

fn load_vault_lock(ctx: &mut Context) -> ckb_testtool::ckb_types::packed::OutPoint {
    let bin = std::fs::read("../target/riscv64imac-unknown-none-elf/release/vault_lock")
        .expect("missing vault_lock binary — run: cargo build -p vault_scripts --release --target riscv64imac-unknown-none-elf");
    ctx.deploy_cell(bin.into())
}

fn load_token_type(ctx: &mut Context) -> ckb_testtool::ckb_types::packed::OutPoint {
    let bin = std::fs::read("../target/riscv64imac-unknown-none-elf/release/test_token_type")
        .expect("missing test_token_type binary — run: cargo build -p test_token_type --release --target riscv64imac-unknown-none-elf");
    ctx.deploy_cell(bin.into())
}

// ── Encoding helper ───────────────────────────────────────────────────────────

/// Encode a u128 amount as a 16-byte little-endian sUDT data payload.
fn encode_sudt(amount: u128) -> Bytes {
    Bytes::from(amount.to_le_bytes().to_vec())
}

// ── Test Fixture ──────────────────────────────────────────────────────────────

/// Shared setup for every deposit test: deploys all scripts, derives scripts and
/// hashes, and provides helpers for building pool states and transactions.
struct Fixture {
    ctx: Context,

    // Deployed out-points — used when constructing CellDep entries.
    pool_type_op:  ckb_testtool::ckb_types::packed::OutPoint,
    pool_lock_op:  ckb_testtool::ckb_types::packed::OutPoint,
    token_type_op: ckb_testtool::ckb_types::packed::OutPoint,

    // Concrete Script objects.
    pool_type_script: ckb_testtool::ckb_types::packed::Script,
    pool_lock_script: ckb_testtool::ckb_types::packed::Script,
    asset_a_type:     ckb_testtool::ckb_types::packed::Script,
    asset_b_type:     ckb_testtool::ckb_types::packed::Script,
    lp_type:          ckb_testtool::ckb_types::packed::Script,
    lender_lock:      ckb_testtool::ckb_types::packed::Script,

    // Pre-computed hashes embedded in PoolData.
    pool_lock_hash:       [u8; 32],
    vault_lock_code_hash: [u8; 32],
    asset_a_type_hash:    [u8; 32],
    asset_b_type_hash:    [u8; 32],
    lp_type_hash:         [u8; 32],
}

impl Fixture {
    fn new() -> Self {
        let mut ctx = Context::default();

        // ── Deploy all contract binaries ──────────────────────────────────────
        let pool_type_op  = load_pool_type(&mut ctx);
        let pool_lock_op  = load_pool_lock(&mut ctx);
        let vault_lock_op = load_vault_lock(&mut ctx);
        let token_type_op = load_token_type(&mut ctx);

        // ── Lender lock ───────────────────────────────────────────────────────
        // always-success with unique args so its lock hash is deterministic.
        let always_success_op = ctx.deploy_cell(Bytes::from(
            ckb_testtool::builtin::ALWAYS_SUCCESS.to_vec(),
        ));
        let lender_lock = ctx
            .build_script(&always_success_op, Bytes::from_static(b"lender"))
            .expect("build lender lock");
        let lender_lock_hash: [u8; 32] = lender_lock.calc_script_hash().unpack();

        // ── Token type scripts (test_token_type sUDT) ─────────────────────────
        // args layout: [owner_lock_hash (32 bytes) | discriminator (1 byte)]
        // The discriminator makes each token's type hash unique even though they
        // share the same code (test_token_type).
        //
        // Owner mode activates when any input cell has lock_hash == args[0..32].
        // Because the lender's asset-b input is included in every deposit tx,
        // owner mode is active → LP minting is allowed.

        // Asset A — collateral token (not physically present in deposit, but its
        // type hash must be recorded in pool state).
        let mut asset_a_args = lender_lock_hash.to_vec();
        asset_a_args.push(0x01);
        let asset_a_type = ctx
            .build_script(&token_type_op, Bytes::from(asset_a_args))
            .expect("build asset_a type");

        // Asset B — the loan/liquidity token deposited into the pool.
        let mut asset_b_args = lender_lock_hash.to_vec();
        asset_b_args.push(0x02);
        let asset_b_type = ctx
            .build_script(&token_type_op, Bytes::from(asset_b_args))
            .expect("build asset_b type");

        // LP token — minted to the lender on deposit.
        let mut lp_args = lender_lock_hash.to_vec();
        lp_args.push(0x03);
        let lp_type = ctx
            .build_script(&token_type_op, Bytes::from(lp_args))
            .expect("build lp type");

        // ── Hash pre-computation ──────────────────────────────────────────────
        let asset_a_type_hash: [u8; 32] = asset_a_type.calc_script_hash().unpack();
        let asset_b_type_hash: [u8; 32] = asset_b_type.calc_script_hash().unpack();
        let lp_type_hash:      [u8; 32] = lp_type.calc_script_hash().unpack();

        // vault_lock_code_hash = blake2b(vault_lock binary), NOT the full script hash.
        // pool_script uses this to find vault cells by code_hash.
        let vault_lock_dummy = ctx
            .build_script(&vault_lock_op, Bytes::new())
            .expect("build dummy vault_lock script");
        let vault_lock_code_hash: [u8; 32] = vault_lock_dummy.code_hash().unpack();

        // ── pool_type script ──────────────────────────────────────────────────
        // The Type ID check in pool_script only runs on creation (GroupInput empty).
        // For deposit (update) tests we use a fixed placeholder as args.
        let pool_type_script = ctx
            .build_script(&pool_type_op, Bytes::from_static(b"pool-type-id-placeholder"))
            .expect("build pool_type script");
        let pool_type_hash: [u8; 32] = pool_type_script.calc_script_hash().unpack();

        // ── pool_lock script ──────────────────────────────────────────────────
        // args = 32-byte pool_type_hash so pool_lock can find the pool cell.
        let pool_lock_script = ctx
            .build_script(&pool_lock_op, Bytes::from(pool_type_hash.to_vec()))
            .expect("build pool_lock script");
        let pool_lock_hash: [u8; 32] = pool_lock_script.calc_script_hash().unpack();

        Fixture {
            ctx,
            pool_type_op,
            pool_lock_op,
            token_type_op,
            pool_type_script,
            pool_lock_script,
            asset_a_type,
            asset_b_type,
            lp_type,
            lender_lock,
            pool_lock_hash,
            vault_lock_code_hash,
            asset_a_type_hash,
            asset_b_type_hash,
            lp_type_hash,
        }
    }

    // ── PoolData construction ─────────────────────────────────────────────────

    /// Pristine pool state: zero liquidity, zero debt, zero LP supply.
    fn initial_pool_data(&self) -> PoolData {
        PoolData {
            liquidity_available:    0,
            total_debt:             0,
            interest_accrued:       0,
            rate_net:               0,
            total_lp_supply:        0,
            timestamp_last_update:  T_INITIAL,
            k_min_current:          10,
            timestamp_last_haircut: T_INITIAL,
            k_min:                  10,
            k_target:               50,
            t_max:                  30 * 86_400,
            base_rate:              200,
            max_rate:               2_000,
            asset_b_type_hash:      self.asset_b_type_hash,
            asset_a_type_hash:      self.asset_a_type_hash,
            lp_token_type_hash:     self.lp_type_hash,
            vault_lock_code_hash:   self.vault_lock_code_hash,
            pool_lock_hash:         self.pool_lock_hash,
        }
    }

    /// Compute the expected new PoolData after depositing `amt` at `t_now`.
    fn new_pool_state(&self, old: &PoolData, amt: u128, t_now: u64) -> PoolData {
        let exp_lp = self.expected_lp(old, amt, t_now);
        PoolData {
            liquidity_available:   old.liquidity_available + amt,
            total_lp_supply:       old.total_lp_supply + exp_lp,
            timestamp_last_update: t_now,
            // all other fields carry over unchanged
            total_debt:             old.total_debt,
            interest_accrued:       old.interest_accrued,
            rate_net:               old.rate_net,
            k_min_current:          old.k_min_current,
            timestamp_last_haircut: old.timestamp_last_haircut,
            k_min:                  old.k_min,
            k_target:               old.k_target,
            t_max:                  old.t_max,
            base_rate:              old.base_rate,
            max_rate:               old.max_rate,
            asset_b_type_hash:      old.asset_b_type_hash,
            asset_a_type_hash:      old.asset_a_type_hash,
            lp_token_type_hash:     old.lp_token_type_hash,
            vault_lock_code_hash:   old.vault_lock_code_hash,
            pool_lock_hash:         old.pool_lock_hash,
        }
    }

    /// Calculate LP tokens the lender would receive for depositing `amt` at `t_now`.
    fn expected_lp(&self, old: &PoolData, amt: u128, t_now: u64) -> u128 {
        // Lazily project interest to t_now (no vault being closed → r_vault=0, t_exp=0).
        let adj = interest_adjustment(old.rate_net, old.timestamp_last_update, t_now, 0, 0);
        let proj_interest = if adj < 0 {
            old.interest_accrued.saturating_sub(adj.unsigned_abs())
        } else {
            old.interest_accrued.saturating_add(adj as u128)
        };

        // Exchange rate at the projected state.
        let rate = exchange_rate(
            old.liquidity_available,
            old.total_debt,
            proj_interest,
            old.total_lp_supply,
        );

        lp_tokens_for_deposit(amt, rate)
    }

    // ── Header dep injection ──────────────────────────────────────────────────

    /// Inject a block header with timestamp = `t_now_secs * 1000` ms and
    /// attach it as a header dep on `tx`.
    ///
    /// pool_script reads `load_header(0, Source::HeaderDep)` and compares it
    /// (within ±300 s) against `new_pool.timestamp_last_update`.
    fn add_header_dep(
        &mut self,
        tx: ckb_testtool::ckb_types::core::TransactionView,
        t_now_secs: u64,
    ) -> ckb_testtool::ckb_types::core::TransactionView {
        let header = HeaderBuilder::default()
            .timestamp(t_now_secs * 1_000) // CKB headers store milliseconds
            .build();
        let hash = header.hash();
        self.ctx.insert_header(header);
        tx.as_advanced_builder().header_dep(hash).build()
    }

    // ── CellDep shorthand ─────────────────────────────────────────────────────

    fn code_dep(op: &ckb_testtool::ckb_types::packed::OutPoint) -> CellDep {
        CellDep::new_builder()
            .out_point(op.clone())
            .dep_type(ckb_testtool::ckb_types::core::DepType::Code)
            .build()
    }

    // ── Core deposit transaction builder ──────────────────────────────────────

    /// Build a full deposit transaction.
    ///
    /// Parameters allow injecting specific fraud patterns for negative tests:
    /// - `inflate_lp_by`            : extra LP minted above the formula (default 0).
    /// - `pool_asset_b_out_override` : override the pool's Asset-B output amount.
    fn build_deposit_tx(
        &mut self,
        old: &PoolData,
        t_now: u64,
        deposit_amount: u128,
        inflate_lp_by: u128,
        pool_asset_b_out_override: Option<u128>,
    ) -> ckb_testtool::ckb_types::core::TransactionView {
        let new_state  = self.new_pool_state(old, deposit_amount, t_now);
        let minted_lp  = self.expected_lp(old, deposit_amount, t_now) + inflate_lp_by;

        let old_data   = Bytes::from(old.to_bytes());
        let new_data   = Bytes::from(new_state.to_bytes());

        let pool_out_amount = pool_asset_b_out_override.unwrap_or(deposit_amount);
        let lender_change   = LENDER_INITIAL_BALANCE.saturating_sub(deposit_amount);

        // ── Input cells ───────────────────────────────────────────────────────

        // [0] Pool cell input — contains old PoolData.
        let pool_cell_op = self.ctx.create_cell(
            CellOutput::new_builder()
                .capacity(50_000_000_000u64) // 500 CKB
                .lock(self.pool_lock_script.clone())
                .type_(Some(self.pool_type_script.clone()).pack())
                .build(),
            old_data,
        );

        let old_balance = old.total_debt + old.liquidity_available;

        let pool_liquidity_cell_op = self.ctx.create_cell(
            CellOutput::new_builder()
                .capacity(50_000_000_000u64) // 500 CKB
                .lock(self.pool_lock_script.clone())
                .type_(Some(self.asset_b_type.clone()).pack())
                .build(),
            encode_sudt(old_balance),
        );

        // [1] Lender's Asset-B token cell.
        let lender_ab_op = self.ctx.create_cell(
            CellOutput::new_builder()
                .capacity(14_200_000_000u64) // 142 CKB
                .lock(self.lender_lock.clone())
                .type_(Some(self.asset_b_type.clone()).pack())
                .build(),
            ,encode_sudt(LENDER_INITIAL_BALANCE)
        );
        

    
        // ── Transaction assembly ──────────────────────────────────────────────

        let tx = TransactionBuilder::default()
            // inputs
            .input(CellInput::new_builder().previous_output(pool_cell_op).build())
            .input(CellInput::new_builder().previous_output(lender_ab_op).build())
            .input(CellInput::new_builder().previous_output(pool_liquidity_cell_op).build())
            // outputs + data
            // [0] updated pool cell
            .output(
                CellOutput::new_builder()
                    .capacity(50_000_000_000u64)
                    .lock(self.pool_lock_script.clone())
                    .type_(Some(self.pool_type_script.clone()).pack())
                    .build(),
            )
            .output_data(new_data.pack())
            // [1] pool's Asset-B custody cell
            .output(
                CellOutput::new_builder()
                    .capacity(14_200_000_000u64)
                    .lock(self.pool_lock_script.clone())
                    .type_(Some(self.asset_b_type.clone()).pack())
                    .build(),
            )
            .output_data(encode_sudt(pool_out_amount).pack())
            // [2] lender's Asset-B change
            .output(
                CellOutput::new_builder()
                    .capacity(14_200_000_000u64)
                    .lock(self.lender_lock.clone())
                    .type_(Some(self.asset_b_type.clone()).pack())
                    .build(),
            )
            .output_data(encode_sudt(lender_change).pack())
            // [3] lender's LP tokens
            .output(
                CellOutput::new_builder()
                    .capacity(14_200_000_000u64)
                    .lock(self.lender_lock.clone())
                    .type_(Some(self.lp_type.clone()).pack())
                    .build(),
            )
            .output_data(encode_sudt(minted_lp).pack())
            // cell deps
            .cell_dep(Self::code_dep(&self.pool_type_op.clone()))
            .cell_dep(Self::code_dep(&self.pool_lock_op.clone()))
            .cell_dep(Self::code_dep(&self.token_type_op.clone()))
            .build();

        let tx = self.add_header_dep(tx, t_now);
        self.ctx.complete_tx(tx)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

/// Happy path — pristine pool, correct LP mint, correct Asset-B accounting.
/// pool_type and pool_lock must both accept the transaction.
#[test]
fn test_deposit_valid_correct_lp_mint() {
    let mut fix = Fixture::new();
    let old   = fix.initial_pool_data();
    let t_now = T_INITIAL + 1;

    let tx = fix.build_deposit_tx(&old, t_now, DEPOSIT_AMOUNT, 0, None);

    fix.ctx
        .verify_tx(&tx, MAX_CYCLES)
        .expect("valid deposit should be accepted");
}

/// Negative — LP mint inflated by +1 token.
/// pool_type must reject with ERROR_LP_MINT_INVALID (13).
#[test]
fn test_deposit_rejects_inflated_lp_mint() {
    let mut fix = Fixture::new();
    let old   = fix.initial_pool_data();
    let t_now = T_INITIAL + 1;

    let tx = fix.build_deposit_tx(&old, t_now, DEPOSIT_AMOUNT, 1, None);

    let err = fix
        .ctx
        .verify_tx(&tx, MAX_CYCLES)
        .expect_err("inflated LP mint should be rejected");

    assert!(
        err.to_string().contains("error code 13"),
        "expected ERROR_LP_MINT_INVALID (13), got: {err}"
    );
}

/// Negative — pool's Asset-B output is one token less than the deposit delta.
/// pool_type must reject with ERROR_DEBT_ACCOUNTING (14).
#[test]
fn test_deposit_rejects_pool_asset_b_one_short() {
    let mut fix = Fixture::new();
    let old   = fix.initial_pool_data();
    let t_now = T_INITIAL + 1;

    let tx = fix.build_deposit_tx(
        &old,
        t_now,
        DEPOSIT_AMOUNT,
        0,
        Some(DEPOSIT_AMOUNT - 1), // one token short
    );

    let err = fix
        .ctx
        .verify_tx(&tx, MAX_CYCLES)
        .expect_err("short pool Asset-B should be rejected");

    assert!(
        err.to_string().contains("error code 14"),
        "expected ERROR_DEBT_ACCOUNTING (14), got: {err}"
    );
}

/// Negative — robbery pattern: state claims full deposit but pool receives only 1 token.
/// pool_type must reject with ERROR_DEBT_ACCOUNTING (14).
#[test]
fn test_deposit_rejects_robbery_pattern() {
    let mut fix = Fixture::new();
    let old   = fix.initial_pool_data();
    let t_now = T_INITIAL + 1;

    let tx = fix.build_deposit_tx(
        &old,
        t_now,
        DEPOSIT_AMOUNT,
        0,
        Some(1), // almost nothing reaches the pool
    );

    let err = fix
        .ctx
        .verify_tx(&tx, MAX_CYCLES)
        .expect_err("robbery pattern should be rejected");

    assert!(
        err.to_string().contains("error code 14"),
        "expected ERROR_DEBT_ACCOUNTING (14), got: {err}"
    );
}

/// Positive — pool already has outstanding loans and accrued interest.
/// LP mint must use the current exchange rate (rate > 1:1), so the lender
/// receives fewer LP tokens than the raw deposit amount.
#[test]
fn test_deposit_uses_exchange_rate_when_pool_has_debt() {
    let mut fix = Fixture::new();

    // Non-pristine pool: 1 B liquidity, 0.5 B debt, 0.1 B interest, 1 B LP supply.
    // exchange_rate = (1_000_000_000 + 500_000_000 + 100_000_000) / 1_000_000_000
    //               = 1.6  →  LP per deposit < deposit_amount.
    let old = PoolData {
        liquidity_available:    1_000_000_000,
        total_debt:             500_000_000,
        interest_accrued:       100_000_000,
        total_lp_supply:        1_000_000_000,
        rate_net:               123_456,
        timestamp_last_update:  T_INITIAL,
        k_min_current:          10,
        timestamp_last_haircut: T_INITIAL,
        k_min:                  10,
        k_target:               50,
        t_max:                  30 * 86_400,
        base_rate:              200,
        max_rate:               2_000,
        asset_b_type_hash:      fix.asset_b_type_hash,
        asset_a_type_hash:      fix.asset_a_type_hash,
        lp_token_type_hash:     fix.lp_type_hash,
        vault_lock_code_hash:   fix.vault_lock_code_hash,
        pool_lock_hash:         fix.pool_lock_hash,
    };

    let t_now      = T_INITIAL + 1;
    let exp_lp     = fix.expected_lp(&old, DEPOSIT_AMOUNT, t_now);
    let rate       = exchange_rate(
        old.liquidity_available,
        old.total_debt,
        old.interest_accrued,
        old.total_lp_supply,
    );

    // Sanity: rate > 1:1, so lender gets fewer LP tokens than deposit.
    assert!(rate > RATE_PRECISION, "rate should be > 1:1");
    assert!(exp_lp < DEPOSIT_AMOUNT, "LP < deposit when pool has accrued value");

    let tx = fix.build_deposit_tx(&old, t_now, DEPOSIT_AMOUNT, 0, None);

    fix.ctx
        .verify_tx(&tx, MAX_CYCLES)
        .expect("deposit into interest-bearing pool should be accepted");
}

/// Negative — pool_lock detects that the output pool cell's lock was changed
/// to the lender's lock (a classic drain attack).
/// pool_lock must reject with ERROR_POOL_DATA_MALFORMED (10).
#[test]
fn test_deposit_rejects_pool_lock_hijack() {
    let mut fix = Fixture::new();
    let old   = fix.initial_pool_data();
    let t_now = T_INITIAL + 1;

    let new_state  = fix.new_pool_state(&old, DEPOSIT_AMOUNT, t_now);
    let minted_lp  = fix.expected_lp(&old, DEPOSIT_AMOUNT, t_now);
    let lender_chg = LENDER_INITIAL_BALANCE - DEPOSIT_AMOUNT;

    let old_data = Bytes::from(old.to_bytes());
    let new_data = Bytes::from(new_state.to_bytes());

    // Pool cell input.
    let pool_in_op = fix.ctx.create_cell(
        CellOutput::new_builder()
            .capacity(50_000_000_000u64)
            .lock(fix.pool_lock_script.clone())
            .type_(Some(fix.pool_type_script.clone()).pack())
            .build(),
        old_data,
    );

    // Lender Asset-B input.
    let lender_ab_op = fix.ctx.create_cell(
        CellOutput::new_builder()
            .capacity(14_200_000_000u64)
            .lock(fix.lender_lock.clone())
            .type_(Some(fix.asset_b_type.clone()).pack())
            .build(),
        encode_sudt(LENDER_INITIAL_BALANCE),
    );

    // Build tx where output[0] has lender_lock instead of pool_lock — the hijack.
    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(pool_in_op).build())
        .input(CellInput::new_builder().previous_output(lender_ab_op).build())
        // [0] pool cell with HIJACKED lock
        .output(
            CellOutput::new_builder()
                .capacity(50_000_000_000u64)
                .lock(fix.lender_lock.clone()) // ← wrong: should be pool_lock
                .type_(Some(fix.pool_type_script.clone()).pack())
                .build(),
        )
        .output_data(new_data.pack())
        // [1] pool's Asset-B custody
        .output(
            CellOutput::new_builder()
                .capacity(14_200_000_000u64)
                .lock(fix.pool_lock_script.clone())
                .type_(Some(fix.asset_b_type.clone()).pack())
                .build(),
        )
        .output_data(encode_sudt(DEPOSIT_AMOUNT).pack())
        // [2] lender's Asset-B change
        .output(
            CellOutput::new_builder()
                .capacity(14_200_000_000u64)
                .lock(fix.lender_lock.clone())
                .type_(Some(fix.asset_b_type.clone()).pack())
                .build(),
        )
        .output_data(encode_sudt(lender_chg).pack())
        // [3] lender's LP tokens
        .output(
            CellOutput::new_builder()
                .capacity(14_200_000_000u64)
                .lock(fix.lender_lock.clone())
                .type_(Some(fix.lp_type.clone()).pack())
                .build(),
        )
        .output_data(encode_sudt(minted_lp).pack())
        .cell_dep(Fixture::code_dep(&fix.pool_type_op.clone()))
        .cell_dep(Fixture::code_dep(&fix.pool_lock_op.clone()))
        .cell_dep(Fixture::code_dep(&fix.token_type_op.clone()))
        .build();

    let tx = fix.add_header_dep(tx, t_now);
    let tx = fix.ctx.complete_tx(tx);

    let err = fix
        .ctx
        .verify_tx(&tx, MAX_CYCLES)
        .expect_err("pool_lock should reject hijacked output lock");

    assert!(
        err.to_string().contains("error code 10"),
        "expected ERROR_POOL_DATA_MALFORMED (10), got: {err}"
    );
}
