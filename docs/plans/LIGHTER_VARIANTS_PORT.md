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

## Landing integration and live capture

- Committed harness/readiness checkpoint `a6af2c7aa2`, then merged landing
  `0063068241` (live-smoke lane) in `59ec279dd5`; no conflicts.
- Combined release compile passed; the running build script was edited before
  its bindings stage and its shell read position became invalid. Syntax check
  and a clean cached rerun both passed; bindings regenerated successfully.
- Integrated harness: 146 PASS. Real WASM boundary: 14 PASS for both aliases,
  including remote-only snapshots, no delegated callback/write and one execution.
- Direct Robinhood REST succeeds, but WebSocket returns venue code 20558,
  restricted jurisdiction; the standard socket is a working direct control.
  The declared public host proxy succeeds without credentials. Recorder's new
  opt-in `--tealstreet-ws-route` uses the existing `createEngineTransport` and
  provider policy; venue code and recorded URLs remain original upstream URLs.
  Existing routing tests: 219 PASS. No network settings changed.
- Live command: `corepack yarn -s rust:record --venue lighter-robinhood
  --tealstreet-ws-route --seconds 20 --books ETH,BTC
  --out sdk-rs/harness/tapes/lighter-robinhood-smoke.jsonl`.
- Trim command: `corepack yarn -s rust:trim-tape
  --tape sdk-rs/harness/tapes/lighter-robinhood-smoke.jsonl --seconds 12
  --out sdk-rs/harness/fixtures/lighter-robinhood-public.jsonl.gz`.
  Fixture: 1,409 records, 178,888 gzip bytes, four real HTTP anchors, one open,
  1,403 socket messages and both books. Twelve seconds retains startup anchors
  and about six seconds of actual socket traffic without rewriting timestamps.
- First mainnet private gate PASS (14 frames). Testnet gate exposed a missing
  periodic authenticated `accountLimits` read when no builder is configured.
  Fix in progress: one shared typed cache for polling and builder eligibility,
  matching TS's five-minute success and one-minute retry deadlines. Failed reads
  retain the cached tier; unsigned accounts never send the request. Independent
  review found malformed nested payloads could mask a root Standard tier; add
  lazy nested decoding and a regression before final gates.

## Account-tier gate correction

Implemented the missing periodic read using one typed venue-owned limits cache.
Polling and builder eligibility coalesce the same authenticated request; success
expires after five minutes and failure after one minute, measured from request
start as TS does. Failed reads retain the last tier. Awaiting placements resume
once on response/failure, and Standard continues to block attributed writes.
Removed the replaced trading-only endpoint/type/handler.

Independent review found eager nested decoding could preserve cached Premium
when a fresh root Standard payload had irrelevant malformed children. Decode
root status separately and nested tier fields lazily in TS precedence order.
Regression coverage warms Premium and proves fresh Standard blocks every write.
Final independent re-review PASS. Native suite now 100 PASS, including five new
limits tests for all deployments, auth absence, coalescing, expiry, failure and
trading coordination. Crate Clippy/fmt/bans PASS; combined WASM checks continuing.

## Gates before TxFlow integration

Runtime commit `c8376a828e`, canonical combined default-feature release WASM.
Bans, crate/host fmt, crate all-target Clippy, WASM host Clippy, 100 native
suite tests, package typecheck/lint, 56 wrapper/TS signing-mode tests and 14
real-WASM boundary tests all PASS. Integrated harness 146 PASS; routing 219 PASS.
Every local public gate below uses three runs and identical output.

| Venue | TS/Rust main CPU ms | TS/Rust steady CPU ms | TS/Rust retained MB | Rust linear MB |
| --- | --- | --- | --- | --- |
| lighter | 805.8 / 633.3 | 513.2 / 392.1 | 16.4 / 3.1 | 1.6 |
| lighter-robinhood | 385.5 / 181.3 | 261.4 / 91.3 | 18.0 / 4.3 | 2.8 |
| binance | 263.7 / 158.8 | 151.3 / 78.8 | 16.0 / 3.2 | 2.0 |

Public frames/markets/tickers/books: lighter 8,875/213/213/2; Robinhood
1,404/58/58/2; Binance 873/917/783/2. All three gates PASS.

Private correctness: lighter mainnet/testnet, Robinhood mainnet/testnet and
Binance all PASS, 14 frames each; each Lighter tape exercises eight commands.
Private CPU/memory are informational: some Rust private CPU figures exceed TS;
no private performance improvement is claimed. Public thresholds are unchanged.
Existing documented Lighter TS corrections (notably one fill per execution and
net-mode acknowledgement) apply identically to the broker alias. No new TS bug
or undocumented output divergence is introduced. TS venues/helpers are retained.

Remaining process: final gate-phase review, PR correctness review comments,
green CI, merge into `feat/rust-rewrite`, and fresh follow-up branch.

Landing advanced to `bf1cf3c99a` (TxFlow PR #1329) during these gates.
Commit this passing evidence, integrate the landing without discarding either
venue, then rebuild combined WASM and repeat gates affected by shared replay
finalization/routing changes before PR creation.

TxFlow integration resolves four overlapping files by retaining both venue
registrations/imports, both startup exceptions, both build selectors and each
readiness row. Combined inventory: 49 exchange names, 45 Rust adapters, nine
Rust defaults, 36 opt-in TS defaults and four names without Rust. The landing's
TxFlow reference/default cutover and replay finalization/routing improvements
are retained. Lighter production TS files remain intact.

## Integrated acceptance

Runtime `c77e0d4bb1`, including landing `bf1cf3c99a`, canonical combined
release WASM. Independent integration review PASS; both venues' registrations,
startup and client-ID behavior remain intact. Native engine/crypto/Lighter
sources are byte-identical to the passed 100-test native suite. Repeated bans,
crate/host fmt, combined-host WASM Clippy/build, package typecheck/lint,
65 Lighter/TxFlow wrapper-mode tests, 14 real-WASM boundary tests and 172
shared harness tests all PASS. The 219-test routing evidence remains applicable:
only the unrelated TxFlow native public-CORS selection changed on integration.

Three-run public gates, identical output and unchanged thresholds:

| Venue | TS/Rust main CPU ms | TS/Rust steady CPU ms | TS/Rust retained MB | Rust linear MB |
| --- | --- | --- | --- | --- |
| lighter | 785.6 / 522.5 | 492.9 / 330.5 | 17.3 / 3.7 | 1.6 |
| lighter-robinhood | 392.4 / 154.2 | 264.3 / 84.5 | 19.8 / 5.8 | 2.8 |
| binance | 205.0 / 108.2 | 111.8 / 47.3 | 17.3 / 4.5 | 2.0 |

All three public gates and all five private correctness gates PASS. Each private
recording has 14 frames; both Lighter deployments cover mainnet and testnet.
Private performance remains informational (Binance steady CPU 14.7/16.0 ms).
No private live-key claim is made; private tapes use documented/offline inputs.
No TS Lighter venue is deleted, no new TS bug divergence is introduced, and no
gate thresholds or existing fixtures are weakened. Implementation, local gates
and phase reviews are complete; the feature PR tracks adversarial review, CI
and landing status.
