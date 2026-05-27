# CLAUDE.md — @tealstreet/lighter-sdk

TypeScript SDK for the Lighter protocol, a high-performance perpetual futures DEX. Provides transaction building, Poseidon2/Schnorr cryptographic signing, and type-safe API clients.

## Architecture

```
src/
├── client/
│   ├── api-client.ts           # Core HTTP client with interceptor support
│   ├── transaction-client.ts   # Transaction building and signing
│   ├── account-api.ts          # Account queries
│   ├── order-api.ts            # Order management
│   ├── candlestick-api.ts      # Price data
│   ├── funding-api.ts          # Funding rates
│   ├── info-api.ts             # Market info
│   └── referral-api.ts         # Referral system
├── signer/
│   ├── signer.ts               # TransactionSigner class
│   └── key-manager.ts          # Key generation and derivation
├── types/                      # Orders, transactions, responses, constants
├── utils/                      # Validation utilities
└── index.ts                    # Barrel exports
```

- Modular client architecture (HTTP, signing, domain-specific APIs)
- Axios-based with request/response/error interceptors
- Depends on `@tealstreet/poseidon-crypto` for Poseidon2 hashing and Schnorr signatures

## Commands

```bash
yarn build-force    # Build with tsup (normal build disabled)
yarn dev-force      # Watch mode
yarn typecheck      # Type checking
yarn test           # Vitest
```

## Gotchas

- **Linting disabled** — treated as third-party SDK
- **Build disabled by default** — uses tsconfig path aliases
- Requires `@tealstreet/poseidon-crypto` to be initialized (`initWasm()`) before use
- Nonce management is automatic with caching — call `resetNonce()` after errors
- Field element conversion required for transaction signing
