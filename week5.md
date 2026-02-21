# Builder Track Weekly Report — Week 5

**Name:** Adokiye

## ✅ Completed Tasks

- Built a custom **SUDT (Simple User-Defined Token)** script from scratch in Rust (`social_token`).
- Implemented and enforced "Token Scarcity" logic to restrict spontaneous minting.
- Addressed cross-compilation configurations (`riscv64imac-unknown-none-elf`) for the new workspace members.
- Created an extensive `ckb-testtool` test suite using mock transactions.
- Solved compilation and API challenges involving Rust's `ckb-types` ecosystem.

## 📚 Key Learning Areas

### Building SUDT Logic
- **Minimal Concern Pattern:** The script successfully implements Token Scarcity through minimal constraints `input_tokens >= output_tokens`. It doesn't restrict transfers, burnings, or multi-cell aggregation so long as tokens aren't synthesized.
- **Handling 128-bit CellData:** Learned how to safely parse `CellData` bytes by stripping the first 16 bytes and converting them via `u128::from_le_bytes`.
- **Authorization Delegation/Owner Mode:** Used the `ckb_std` High Level API (`load_cell_lock_hash`) to query the input transactions and bypass the token restrictions if the `type script args` matched any input `lock hash`. This enables owner permissions.

### Testing Framework (`ckb-testtool`)
- Initialized transaction contexts simulating the CKB blockchain state offline.
- Successfully built dummy inputs (`CellInput`) and outputs (`CellOutput`) equipped with mocked data fields (the `u128` token amounts).
- Learned how to deploy compiled target scripts (`social_token`) to a `Context`, build a `Script`, and assign them as a `type_` to `CellOutput`s for constraint validation tests.

## 🛑 Assumptions Corrected (Debugging Learnings)

While setting up the `testtoken` module, several assumptions had to be corrected through explicit API and environment adjustments:

1. **CellOutput Construction Assumption**
   - *Assumption*: Assumed `CellOutput::new_builder()` possessed an `.into_output_with_data(data)` shortcut.
   - *Correction*: That method doesn't exist for `CellOutput`. When constructing `inputs_cells` or `outputs_cells` using `ckb-testtool`, data (`Bytes`) and boundaries (`CellOutput`) must be manipulated separately. Transactions use `.outputs(outputs).outputs_data(outputs_data.pack())` to manually bundle the payload.

2. **Types and `.pack()` Ambiguity**
   - *Assumption*: `1000u64.pack()` would be automatically recognized by the compiler as a capacity property.
   - *Correction*: The `ckb-gen-types` crate implements packing conversions for multiple structures (e.g., `BeUint64` vs `Uint64`). It is necessary to explicitly declare the binding target before packing: `let cap: Uint64 = 1000u64.pack();`.

3. **Lock Script Simulation Rules**
   - *Assumption*: I could deploy any arbitrary bytes (`"fake_owner_lock"`) and execute it as a lock script for test cells since this was a simulated environment.
   - *Correction*: The offline Context still utilizes a real RISC-V VM. Calling an arbitrary string throws a `VM Internal Error: ElfParseError("Malformed entity: Too small")`. All simulated lock scripts must use valid executable binaries. This was resolved by utilizing `ckb_testtool::builtin::ALWAYS_SUCCESS`.

## 🧪 Running the Tests

To verify the `social_token` execution logic, run the test suite covering transfers, burning, mint failures, and owner operations.

```bash
cd contracts/scripts/tests
cargo test -- --nocapture
```
*Make sure `riscv64imac-unknown-none-elf` targets are installed and `social_token` is compiled properly to the `target` directory beforehand.*


