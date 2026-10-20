# Lighter deployment and signing variants

Scope: `lighter` and `lighter-robinhood` perpetuals, preserving TS venues and
wallet/account-tier exports. Base: `origin/feat/rust-rewrite`.

## Phases

1. Public/deployment: represent the four TS deployment profiles in the Lighter
   crate; register the explicit broker name in WASM, wrapper and replay harness.
   Prove REST/WS URLs, quote currency and signing domains independently.
2. Private/signing: audit the actual TS remoteSigner and payloadSigner contract,
   including mixed local/remote credentials, missing local secret, explicit
   public-only and legacy signer-provider options. Match supported behavior;
   never invent a remote transaction protocol. Keep callbacks host-side.
3. Trading/queries: verify the shared command/query path under both deployment
   names, independent cryptographic verification, exact IDs, nonce recovery,
   caller-ID preflight and routing. Add targeted regression coverage.
4. Gates/readiness: native bans/fmt/Clippy/tests, combined WASM Clippy/build,
   focused TS/harness checks, public and private replay for both names and
   Binance. Update phase-9 readiness with exact remaining constraints.
5. PR: integrate current landing, push, PR into `feat/rust-rewrite`, independent
   correctness reviews posted as comments, fix/review until clean, green CI,
   merge and fresh follow-up branch.

Each phase gets a plan audit, focused verification and a sub-agent review before
its stable commit. No load tests, TS deletion, credentials or gate relaxation.

## Progress

- Plan: TS profiles have distinct Robinhood main/test URLs, USDG quote and main
  signing chain 466324 (testnet 300). Standard chain IDs remain 304/300.
- Signing investigation in progress: TS startup recognizes remoteSigner, while
  its only current signer adapter requires a local secret; resolve with source
  and regression evidence before implementation.
