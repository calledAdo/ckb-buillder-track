//! LendNerv Error Codes (`errors.rs`)
//!
//! Technical Overview:
//! This module defines the complete set of error codes used across all
//! LendNerv smart contracts on the Nervos Network. By centralizing these
//! error definitions, the protocol ensures consistent error reporting and
//! simplifies debugging. The errors are categorized into logical groups:
//!   - 10-19: Pool Type Script errors (accounting, minting, parameters).
//!   - 40-49: Origination / singleton errors (undercollateralized, type-id violations).
//!   - 20-29: Vault errors (malformed data, bad repayments, collateral routing).
//!   - 30-39: Intent Lock errors (failure to deliver requested assets).
//!   - 40-49: Origination errors (undercollateralized loans).
//!   - Negative: Generic/system-level errors (syscall failures, encoding issues).

// ── Pool type script errors (10–19) ───────────────────────────────────────────
/// Returned when the `PoolData` structural format from the cell data cannot be parsed.
pub const ERROR_POOL_DATA_MALFORMED: i8 = 10;
/// Returned when the script cannot infer a valid action (Deposit, Borrow, etc.) from the tx inputs and outputs.
pub const ERROR_INVALID_ACTION: i8 = 11;
/// Returned when the calculated exchange rate does not match the expected state transition.
pub const ERROR_EXCHANGE_RATE_MISMATCH: i8 = 12;
/// Returned when LP tokens minted or burned do not perfectly match the calculated required amounts.
pub const ERROR_LP_MINT_INVALID: i8 = 13;
/// Returned when physical Asset B balances in the pool do not correspond to the updated Total Debt and Liquidity ledgers.
pub const ERROR_DEBT_ACCOUNTING: i8 = 14;
/// Returned when the sum of active vault interest rates (Rate Net) is not perfectly updated during a borrow or repayment.
pub const ERROR_RATE_NET_MISMATCH: i8 = 15;
/// Returned when a user attempts to borrow more Asset B than is physically available, preventing 100% utilization lockups.
pub const ERROR_POOL_DRAINED: i8 = 16;
/// Returned when an aggregator attempts to mutate the immutable governance parameters without authorization.
pub const ERROR_PARAMS_MUTATED: i8 = 17;
/// Returned when the lazy accrued interest logic calculates a profit/loss that doesn't match the new cell state.
pub const ERROR_INTEREST_ACCOUNTING: i8 = 18;
/// Returned when a math calculation over/under flows.
pub const ERROR_MATH_OVERFLOW: i8 = 19;

// ── Vault errors (20–29) ───────────────────────────────────────────────────────
/// Returned when the `VaultData` structural format from the cell data cannot be parsed.
pub const ERROR_VAULT_DATA_MALFORMED: i8 = 20;
/// Returned if an invalid public liquidation attempt is made before the vault's expiration timestamp.
pub const ERROR_VAULT_NOT_EXPIRED: i8 = 21;
/// Returned if a repayment or liquidation bid routes less Asset B back to the pool than owed.
pub const ERROR_REPAYMENT_SHORT: i8 = 22;
/// Returned when the collateral Asset A is not correctly routed back to the borrower upon early repayment.
pub const ERROR_COLLATERAL_NOT_RETURNED: i8 = 23;
/// Returned when a vault script fails to locate its corresponding pool cell in the transaction outputs.
pub const ERROR_WRONG_POOL_RECIPIENT: i8 = 24;
/// Returned when a liquidator attempts to bid less than the mathematically required repayment amount during a continuous Dutch auction.
pub const ERROR_BID_TOO_LOW: i8 = 25;
/// Returned if someone attempts to freeze an already frozen vault.
pub const ERROR_VAULT_ALREADY_FROZEN: i8 = 26;
/// Returned if a state transition occurs that alters frozen status illegally.
pub const ERROR_VAULT_NOT_FROZEN: i8 = 27;
/// Returned if freeze is attempted on a vault that has not yet reached its expiration timestamp.
pub const ERROR_VAULT_PREMATURE_FREEZE: i8 = 28;

// ── Intent lock errors (30–39) ─────────────────────────────────────────────────
/// Returned by the Deposit Intent Lock if the aggregator fails to output the newly minted LP tokens to the depositor.
pub const ERROR_LP_NOT_DELIVERED: i8 = 30;
/// Returned by the Borrow Intent Lock if the aggregator fails to output the requested Asset B loan to the borrower.
pub const ERROR_LOAN_NOT_DELIVERED: i8 = 31;
/// Returned by the Borrow Intent Lock if the aggregator fails to properly create the Vault cell locking the collateral.
pub const ERROR_VAULT_NOT_CREATED: i8 = 32;
/// Returned if assets are routed to an incorrect user/lock script.
pub const ERROR_WRONG_RECIPIENT: i8 = 33;
/// Returned by the Withdraw Intent Lock if the aggregator fails to deliver the underlying asset to the withdrawer.
pub const ERROR_ASSET_NOT_DELIVERED: i8 = 34;

// ── Origination / singleton errors (40–49) ──────────────────────────────────────
/// Returned when the ratio of Collateral to Loan (Q) is lower than the pool's minimum safety threshold (k_min).
pub const ERROR_UNDERCOLLATERALIZED: i8 = 40;
/// Returned when the pool cell's args do not match the expected Type ID commitment (hash of creation input outpoint).
pub const ERROR_INVALID_TYPE_ID: i8 = 41;
/// Returned when more than one output carries the pool's type hash during a creation transaction.
pub const ERROR_SINGLETON_VIOLATED: i8 = 42;

// ── Lock script errors (50–59) ─────────────────────────────────────────────────
/// Returned when the Info Cell Lock fails to find the target 32-byte Type Hash anywhere in the transaction.
pub const ERROR_TYPE_HASH_MISMATCH: i8 = 50;

// ── Generic errors ─────────────────────────────────────────────────────────────
/// Standard error for when a CKB underlying syscall fails (e.g., trying to read an out-of-bounds cell).
pub const ERROR_SYSCALL: i8 = -2;
/// Standard error for when script arguments or expected data fields do not meet length requirements.
pub const ERROR_ENCODING: i8 = -1;
/// General authorization failure (e.g., trying to early repay a vault without the borrower's signature).
pub const ERROR_NOT_AUTHORIZED: i8 = -3;
