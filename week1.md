## Builder Track Weekly Report — Week 1

**Name:** Ali Jouahri  
**Week Ending:** 06-17-2025

### Courses Completed
- Started and CKB Academy course 
   ![Alt text](https://drive.google.com/file/d/1gxLR75SwSTrpUsA7voWgRBhX0Yc9uDfR/view?usp=sharing)
- Completed the first **two modules** of the CKB Academy:
     - **CKB Theoretical Knowledge**:
          - Structure of a cell
          - Structure of a transaction
     - **First CKB Transaction:** [View on Explorer](https://testnet.explorer.nervos.org/transaction/0x7d96cadce61d61f49b2f1ece85adadac933de9a30ebb83830a6b7500ae11a019)
     - **Reading [RFC 0022](https://github.com/nervosnetwork/rfcs/blob/master/rfcs/0022-transaction-structure/0022-transaction-structure.md)** on Transaction Structure:
          - Deep dive into `cell_deps`, `witnesses`, `lock` and `type` scripts
          - Understood code locating and execution mechanisms in CKB
- Studied blockchain fundamentals through the **Bitcoin whitepaper**
- Studied sections 1–3 of the [Rust Book](https://doc.rust-lang.org/book/)

### Key Learnings

- Detailed understanding of the **UTXO-based transaction model in CKB**:
     - Role and structure of **witnesses**, **lock scripts**, **type scripts**
     - How **script grouping** and **syscalls** work in the CKB-VM
- Rust fundamentals:
     - Variables and mutability
     - Functions
     - Data types

### Practical Progress

- **Set up a local CKB dev node**
- Executed a transaction from a **miner address to a secondary account**
- Began using CLI tools:
     - `wallet get-capacity`
     - `transfer`
     - `wallet get-live-cells`
- Wrote and ran a **guided Rust project** (Guessing game) to learn Rust basics for on-chain dev

### Environment

- CKB node and local dev environment installed and functional
- Basic CLI usage and debugging started
- Rust and Cargo installed

### Artifacts / Screenshots

- Transaction on explorer:
     - ![Transaction Screenshot](./images/transaction.png)
- Local node / CLI outputs:
     - ![Node Console](./images/node_console.png)
     - ![CLI Commands](./images/cli_commands.png)
- Guided Rust project (build/run):
     - ![Rust Project](./images/rust_project.png)

(Replace placeholder paths above with actual image files or drag images into the notebook cell.)