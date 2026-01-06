# trial-ckb

A JavaScript project for developing smart contracts on the CKB blockchain.

## Overview

This project uses the CKB JavaScript VM (ckb-js-vm) to write smart contracts in typescript. The contracts are compiled to bytecode and can be deployed to the CKB blockchain.

## Project Structure

```
trial-ckb/
├── contracts/           # Smart contract source code
│   └── hello-world/
│       └── src/
│           └── index.ts # Contract implementation
├── tests/              # Contract tests
│   └── hello-world.test.ts
├── scripts/            # Build and utility scripts
│   ├── build-all.js
│   ├── build-contract.js
│   └── add-contract.js
├── dist/               # Compiled output (generated)
│   ├── hello-world.js  # Bundled JavaScript
│   └── hello-world.bc  # Compiled bytecode
├── package.json
├── tsconfig.json       # TypeScript configuration
├── tsconfig.base.json  # Base TypeScript settings
├── jest.config.cjs     # Jest testing configuration
└── README.md
```

## Getting Started

### Prerequisites

- Node.js (v20 or later)
- yarn package manager

### Installation

1. Install dependencies:
   ```bash
   yarn install
   ```

### Building Contracts

Build all contracts:
```bash
yarn run build
```

Build a specific contract:
```bash
yarn run build:contract hello-world
```

Build with debug version:
```bash
yarn run build:debug
```
or for specific contract with debug enabled:
```bash
yarn run build:contract:debug hello-world
```

### Running Tests

Run all tests:
```bash
yarn test
```

Run tests for a specific contract:
```bash
yarn test -- hello-world
```

### Adding New Contracts

Create a new contract:
```bash
yarn run add-contract my-new-contract
```

This will:
- Create a new contract directory under `contracts/`
- Generate a basic contract template
- Create a corresponding test file

## Development

### Contract Development

1. Edit your contract in `contracts/<contract-name>/src/index.ts`
2. Build the contract: `yarn run build:contract <contract-name>`
3. Run tests: `yarn test -- <contract-name>`

### Build Output

All contracts are built to the global `dist/` directory:
- `dist/{contract-name}.js` - Bundled JavaScript code
- `dist/{contract-name}.bc` - Compiled bytecode for CKB execution

### Testing

Tests use the `ckb-testtool` framework to simulate CKB blockchain execution. Each test:
1. Sets up a mock CKB environment
2. Deploys the contract bytecode
3. Executes transactions
4. Verifies results

## Available Scripts

- `build` - Build all contracts
- `build:contract <name>` - Build a specific contract
- `build:debug` - Build all contracts with debug version
- `build:contract:debug <name>` - Build a specific contract with debug version
- `test` - Run all tests
- `add-contract <name>` - Add a new contract
- `deploy` - Deploy contracts to CKB network
- `deploy:debug` - Deploy contracts with debug version to CKB network
- `clean` - Remove all build outputs
- `format` - Format code with Prettier

## Deployment

Deploy your contracts to CKB networks using the built-in deploy script:

### Basic Usage

```bash
# Deploy to devnet (default)
yarn run deploy

# Deploy to testnet
yarn run deploy -- --network testnet

# Deploy to mainnet
yarn run deploy -- --network mainnet
```

Note that you can change the `run deploy` to `run deploy:debug` to deploy the debug version of your smart contracts.

### Advanced Options

```bash
# Deploy with upgradable type ID
yarn run deploy -- --network testnet --type-id

# Deploy with custom private key
yarn run deploy -- --network testnet --privkey 0x...

# Combine multiple options
yarn run deploy -- --network testnet --type-id --privkey 0x...
```

### Available Options

- `--network <network>` - Target network: `devnet`, `testnet`, or `mainnet` (default: `devnet`)
- `--privkey <privkey>` - Private key for deployment (default: uses offckb's deployer account)
- `--type-id` - Enable upgradable type ID for contract updates

### Deployment Artifacts

After successful deployment, artifacts are saved to the `deployment/` directory:
- `deployment/scripts.json` - Contract script information
- `deployment/<network>/<contract>/deployment.toml` - Deployment configuration
- `deployment/<network>/<contract>/migrations/` - Migration history

## Dependencies

### Core Dependencies
- `@ckb-js-std/bindings` - CKB JavaScript VM bindings
- `@ckb-js-std/core` - Core CKB JavaScript utilities

### Development Dependencies
- `ckb-testtool` - Testing framework for CKB contracts
- `esbuild` - Fast JavaScript bundler
- `jest` - JavaScript testing framework
- `typescript` - TypeScript compiler
- `ts-jest` - TypeScript support for Jest
- `prettier` - Code formatter

## Resources

- [CKB JavaScript VM Documentation](https://github.com/nervosnetwork/ckb-js-vm)
- [CKB Developer Documentation](https://docs.nervos.org/docs/script/js/js-quick-start)
- [The Little Book of ckb-js-vm ](https://nervosnetwork.github.io/ckb-js-vm/)

## License

MIT
