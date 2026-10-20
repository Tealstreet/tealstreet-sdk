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
- Signing audit complete: TS startup recognizes remoteSigner, while its only
  current signer adapter requires a local secret. Source and regression tests
  establish unsigned account polling without delegated transaction signing.

## Credential contract

TS `LighterExchange.start()` classifies startup separately from its inherited
`canSign`/`isPublicDataOnly` getter. The Rust wrapper follows the startup formula,
including keyless local signer accounts, rather than using that inconsistent
inherited getter as permission to skip private startup.

- Explicit public mode, missing API key index, or neither secret nor remoteSigner:
  public startup. A payloadSigner callback alone does not enable private startup.
- RemoteSigner without a secret: unsigned account identity/snapshot reads and
  polling, loaded orders with no authenticated order request; no private socket,
  authentication token or transactions. Callbacks are never invoked.
- A valid local secret with either callback: local transaction/auth signing.
  No callback or credential-bearing closure is serialized into WASM.
- Invalid local signing key/index or legacy provider: account snapshots continue;
  no signing/private socket/write. Current TS supports only `tealstreet-sdk` provider.

This resolves the existing modes; it does not introduce remote transaction
signing, which current TS's adapter cannot perform. Standard host routing remains
in force, and venue code emits only the deployment's real REST/socket URLs.

## Phase reviews and verification

- Public/deployment independent review PASS: exact four profile URLs, quote,
  chain isolation, distinct alias registration, explicit builder override.
- Private/signing review found invalid API signing-key index blocked unsigned
  snapshots; corrected before gates. Follow-up review also found validation
  accepted that index; added the existing TS parser and pre-start validation.
  Independent final private phase re-review PASS.
- Trading/query review PASS: shared typed command state machine, caller-ID
  rejection, nonce retries, independent TS/Go-WASM verification, unsigned private
  histories empty and public trades remain available.
- Native verification: 95 tests PASS including final history/index edge
  coverage; bans, crate fmt/Clippy PASS. Wrapper + TS mode regressions: 56 PASS;
  package typecheck/lint PASS.
- WASM host Clippy and formatting PASS. Serialized focused harness checks:
  145 PASS (Lighter, venue registration, tape and gate tests).
- Combined release WASM build and replay gates in progress.
- Production phases committed as `1ee5bb304d`; all phase reviews PASS after fixes.
