# Builder Track Weekly Report — Week 7

**Name:** Adokiye

## ✅ Completed Tasks

- Transitioned the `a_token` dual-script architecture from development to robust integration testing.
- Created an extensive test suite (`test_a_token.rs`) utilizing the `ckb-testtool` to strictly validate both `a_udt_type` and `a_udt_lock` scripts against CKB's simulated RISC-V VM.
- Simulated and verified 7 unique transaction lifecycle states covering Issuer Minting, Standard Transfers, Vault (Allowance) Creation, Delegated Spending with explicit Capacity Refunds, and flexible `ckb_auth` Signature Fallbacks.

## 📚 Key Learning Areas & Test Coverage

### 1. Owner Bypass Minting (`test_type_owner_bypass_mint`)
- **Aim:** Verify the central issuer can freely mint tokens without being constrained by the `input_tokens >= output_tokens` validation.
- **Approach:** The token Type script was initialized with the Owner's Lock Script Hash. A transaction was built with zero input tokens but 100 output tokens, signed by the Owner.
- **Outcome:** The transaction executes successfully (0 error code), confirming the Type Script bypasses standard conservation logic when the Issuer's signature is present in the inputs.

### 2. Conservation of Balance (`test_type_conservation_of_balance_success` & `failure`)
- **Aim:** Ensure normal users cannot arbitrarily inflate the token supply.
- **Approach:** Simulated standard User Mode transfers. Tested a valid scenario mapping exactly 100 tokens as input to 100 tokens as output. Tested an invalid scenario attempting to turn 50 input tokens into 100 output tokens.
- **Outcome:** The valid scenario passes, while the invalid inflation attempt correctly aborts, throwing `ERROR_INSUFFICIENT_BALANCE` (error code 52).

### 3. Vault Generation (`test_type_create_vault`)
- **Aim:** Validate the secure creation of Delegated Allowances (Vaults) and ensure they are bound to unique Vault IDs.
- **Approach:** A transaction inputs 100 normal tokens and splits the output into 50 normal tokens and one 50-token Vault cell (`variant = 1`). We reproduce the Type Script's security logic locally by using `Blake2b` to hash the first `CellInput` OutPoint, arriving at the expected ID.
- **Outcome:** The test succeeds. The Type Script confirmed a Vault was being born, autonomously calculated the mandatory Vault ID, and asserted the output Vault cell correctly adopted it to prevent replay attacks.

### 4. Delegated Spending & Capacity Rent (`test_type_spend_vault_with_refund`)
- **Aim:** Enforce the strict rules surrounding the consumption of a Vault (Allowance), specifically guaranteeing the Vault is entirely burned and the CKB rent capacity is securely returned to the original owner.
- **Approach:** An input Vault cell (100 delegated tokens) storing the owner's `pubkey_hash` is consumed. The output converts these back into 100 normal tokens and explicitly creates a strict refund output cell holding 1000 CKB, locked strictly to the Vault owner's hash.
- **Outcome:** The transaction passes because the Type Script verifies the Vault is burned (no partial allowances) and successfully locates the explicit 1000 CKB refund cell bound to the correct owner lock among the transaction outputs.

### 5. Dual-Lock Fallback Strategy (`test_lock_allowance_fallback_success` & `failure`)
- **Aim:** Verify `a_udt_lock` acts as a multi-strategy lock: falling back to token-specific Allowance rules if standard `ckb_auth` signature validation fails.
- **Approach:**
  - *Success Case:* The Lock script is deployed with a deliberately incorrect pubkey hash (causing automatic signature failure). However, Input 0 (Normal Token) correctly reads its `target_token_id` and scans the remaining inputs, finding Input 1 (an Allowance Cell issued by the same Type script containing the matching ID).
  - *Failure Case:* The identical scenario is run but the Allowance Cell (Input 1) is completely omitted.
- **Outcome:** The Success Case safely passes verification (delegation confirmed). The Failure Case correctly hard-fails, throwing either `ERROR_SIGNATURE_INVALID` (-3) or `ERROR_ALLOWANCE_NOT_FOUND` (50) since neither the Owner signature nor a valid Permission Slip was present.

