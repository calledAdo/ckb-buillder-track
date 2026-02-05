# Builder Track Weekly Report — Week 4

**Name:** Adokiye

## ✅ Completed Tasks

- Started practical CKB script development
- Developed the "no carrot" type script
- Learned and implemented ckb_testtool for script testing
- Successfully compiled scripts to RISC-V target
- Built and executed comprehensive tests

## 📚 Key Learning Areas

### Practical Script Development

- Moving from basic examples (always-succeed, always-fail) to real-world scripts
- Building a type script that validates cell data constraints
- Understanding script behavior and validation logic

### The "Carrot" Script

- Created a script that validates cell data doesn't contain the string "carrot"
- Understanding how type scripts enforce data constraints on cells
- Script execution flow and error handling

### ckb_testtool Framework

- Using `Context` to simulate blockchain state and cell creation
- Understanding `Loader` for loading compiled script binaries
- Building transactions with:
   - Cell dependencies (cell_deps)
   - Input cells and out points
   - Output cells with lock and type scripts
   - Cell data payloads

- Using `verify_tx()` to test script execution against MAX_CYCLES limit

### RISC-V Compilation

- Building scripts with `cargo build --target riscv64imac-unknown-none-elf --release`
- Handling binary placement for test discovery
- Understanding the test infrastructure directory structure

### Test Execution and Debugging

- Writing and running unit tests for scripts
- Handling type annotation ambiguities with `.pack()` calls
- Using `capacity()` builder method correctly
- Managing test dependencies and binary paths

## 🔧 Technical Accomplishments

- Successfully compiled `carrot` script to RISC-V
- Created comprehensive test suite with `test_no_carrot` test case
- Fixed type system issues with CKB types and Pack trait implementations
- Achieved passing test execution with proper transaction verification

## 🧪 Running the Tests

To run the no carrot test, use the following command from the workspace root:

```bash
cd contracts && cargo test -p tests test_no_carrot -- --nocapture
```

This will:

- Compile the test code
- Run the `test_no_carrot` test function
- Display output with `--nocapture` flag
- <b>Note</b>:The ckb scripts are in the contracts/scripts directory

## 🔜 Next Steps

- Expand test coverage with additional edge cases
- Develop more complex type scripts
- Explore lock script development
- Deepen understanding of script groups and arguments