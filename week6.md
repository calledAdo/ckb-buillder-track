# Builder Track Weekly Report — Week 6

**Name:** Adokiye

## ✅ Completed Tasks

- Built a comprehensive dual-script Token system (`a_token`) encompassing both a custom Type Script (`a_udt_type.rs`) and a custom Lock Script (`a_udt_lock.rs`).
- Implemented a "Vault Validation Engine" within the Type Script to natively support ERC20-style delegated allowances using a cell Variant scheme.
- Enforced Vault creation rules, securely generating unforgeable `Vault ID`s utilizing `Blake2b` hashing of the first input's OutPoint.
- Enforced Vault spending rules, mandating the complete burn of the allowance cell and enforcing a mandatory CKB capacity refund back to the original owner.
- Developed a dual-strategy Lock Script that authenticates transactions either via standard Owner Signature (`ckb_auth` secp256k1 verification) or via an Allowance Fallback mechanism.

## 📚 Key Learning Areas

### Advanced UDT Type Script Logic
- **Variant Tagging:** Learned how to safely multiplex cell data by prefixing payloads with a `variant` byte (`0` for Normal Tokens, `1` for Allowances), parsing `u128` token balances, and enforcing logic dynamically based on cell types.
- **Vault Security:** Understood the necessity of binding a unique Vault ID to the transaction's first `CellInput` to prevent replay attacks and forgery of permission slips across transactions.
- **Rent Protection:** Implemented logic to ensure delegates cannot steal the CKB capacity backing the allowance cell during a delegated spend. Verified that a refund cell mapping back to the owner's lock is explicitly synthesized in the transaction outputs.

### Custom Lock Scripts & `ckb_auth`
- **Dynamic Unlock Strategies:** Learned how to build flexible Lock Scripts that verify standard cryptographic signatures using the `ckb_auth` library (`CkbAuthType`, `CkbEntryType`), and gracefully fallback to contextual validation if signatures fail.
- **Witness & Sighash Construction:** Gained deep practical understanding of hashing the transaction (`tx_hash`) alongside its modified witness arguments (zeroing lock fields) using standard `Blake2b` to generate a valid `sighash_all` message for signature verification.
- **Cross-Cell Context Mapping:** Achieved the Allowance Fallback by loading target "Normal Token" properties (`load_cell_data`, `load_cell_type_hash`) directly from within the Lock Script context and searching the input pool for a matching, cryptographically valid Allowance cell.

## 🛑 Assumptions Corrected (Debugging Learnings)

1. **Lock Script Execution Scope**
   - *Assumption*: Assumed the Lock Script could easily share cached execution state or derived variables with the Type Script seamlessly within the same transaction.
   - *Correction*: Lock and Type scripts operate independently within the CKB VM. Validating an allowance in the Lock Script required independently identifying the corresponding Type Script, fetching its hash, and cross-referencing input cells on its own.

2. **`ckb_auth` Library Usage**
   - *Assumption*: Expected `ckb_auth` to automatically handle witness parsing and sighash message generation out of the box.
   - *Correction*: The developer must manually execute `load_witness`, parse the witness arguments, zero-out the lock field containing the signature, bundle the lengths of the witnesses dynamically, and hash the bytes via `Blake2b` to generate the correct message payload for `ckb_auth` validation.

3. **Allowance UX on UTXO Chains**
   - *Assumption*: Allowances (Vaults) could be naturally partially spent similar to normal ERC-20 workflows.
   - *Correction*: Due to the Cell Model, deterministic state transitions mandate the entire allowance cell (Variant 1) to be evaluated and burned, demanding its exact CKB capacity be returned to the owner to respect the strict UTXO state representation on CKB.

## 🧪 Next Steps / Verification

To verify the compiled `a_token` logic, the test modules should be executed in the `ckb-testtool` environment:

```bash
# Execute within the tests/ directory
cargo test -- --nocapture
```
*Ensure `riscv64imac-unknown-none-elf` cross-compilation targets are active and scripts have been built into the target payload.*
