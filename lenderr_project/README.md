# Lenderr Project

`lenderr_project` is a Rust workspace for a CKB-based lending protocol.
It contains on-chain scripts, shared protocol math/state code, and host-side tests.

## Workspace Layout

- `contracts/common`: shared data models, Molecule schema bindings, protocol math
- `contracts/pool_script`: core pool type script state transition validation
- `contracts/pool_lock`: pool lock continuity/singleton guard logic
- `contracts/vault_scripts`: vault lock authorization logic
- `contracts/intent_scripts`: deposit/borrow/withdraw intent lock scripts
- `tests`: host-side Rust tests

## Build

Build all crates:

```bash
cargo build
```

Release build:

```bash
cargo build --release
```

Build a specific contract for CKB-VM target:

```bash
cargo build -p pool_script --release --target riscv64imac-unknown-none-elf
```

Use the same pattern for other contract crates:
`pool_lock`, `vault_scripts`, `intent_scripts`.

## Test

Run all tests in workspace:

```bash
cargo test
```

Run only test crate:

```bash
cargo test -p tests
```

## Notes

- The shared crate (`lenderr-common`) is `no_std`.
- Pool state is serialized with Molecule and parsed via `PoolData`.
- Vault loan metadata lives in vault lock args and is parsed via `VaultData`.
