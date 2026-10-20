# BreakoutProp Rust port

Branch: `feat/rust-breakoutprop`, based on `origin/feat/rust-rewrite`.
Specification: `RUST_REWRITE.md`; process/gates: `VENUE_PORT_BRIEF.md`.

## Scope and architecture

The TS adapter derives only from BaseExchange. It speaks the legacy DXtrade
session REST API, not Binance or another ported venue. Reuse the engine Store,
stream splitter, numeric steps, typed commands and host transport. Do not
copy a venue protocol or introduce an exchange socket. Keep the production and
preview enablement flags unchanged. Books and live candles are quote-derived
synthetic data, preserving the existing limitations.

Primary payload oracle: https://demo-xt.dx.trade/specs/DX_REST_API.html .
TS/runtime contracts: `packages/safe-cex/src/exchanges/breakoutprop/CLAUDE.md`.

## Phases and verification

1. Public data and auth foundation: typed crate, REST session/account startup,
   streaming instrument/detail join and quotes, quote books/candles and polling.
   Unit/scripted payload tests; independent phase review; fix and commit.
2. Private account: atomic portfolio balance/positions/orders/settings,
   selected-account isolation, stale snapshots and successful-write revision
   fences; polling failure recovery. Scripted documented payloads, concurrent
   login/401/disposal cases; independent review; fix and commit.
3. Trading: whole-batch preflight, LIMIT/MARKET/STOP, generated IDs, bounded
   sequenced cancel batches and safe cancel-before-replace; unsupported setting
   writes fail explicitly. Exact payload and uncertain-write tests; independent
   review; fix and commit.
4. Queries and integration: candles, inherited query behavior, wrapper/wasm
   registration, transport configuration and account options. Tests through
   native engine and wrapper; independent review; fix and commit.
5. Harness and gates: generic REST-polling replay support, strict session/write
   oracle, documented private tape, public fixture, all brief gates and Binance
   regression gates; independent review; fix and commit. Update roster.
6. Landing: refresh landing branch without rebase, push and PR into
   `feat/rust-rewrite`; independent correctness reviews posted on PR; fix and
   repeat, green CI before merge, then fresh follow-up branch.

## Session and account plan

- POST `/login` JSON `{domain:"default", username:key, password:secret}`.
  Validate nonempty token and finite positive HH:MM:SS timeout, cache deadline.
  Coalesce all waiters behind exactly one login. Never publish credential/token
  bodies in errors. Use real legacy API origin; host applies routing policy.
- Session generation travels with each request. A first explicit non-login
  HTTP 401 queues that operation behind one relogin and permits one resend.
  Concurrent stale-generation 401s reuse the newer session instead of clearing
  it. Login 401 and a second operation 401 terminate; network errors never
  resend an order/cancel. Read-only network retries are bounded at three.
- Discover `/users/{encoded username}`, filter FULL_TRADING accounts; require
  subaccount when ambiguous and select an exact account suffix/code, never an
  ambiguous substring. Retain selected account across token refresh. Validate
  portfolio identity before mutation and URL-encode all path segments.
- Market data requires credentials and account even in public-data-only mode.
  Session login is a prerequisite, not a fake public socket. Queue operations
  in typed continuations and pace REST at five requests per second.
- Portfolio derives margin/PnL from TS's documented estimates. Parse the whole
  selected snapshot successfully before committing any section. Successful
  writes fence older order snapshots. Failed reads keep last known good state.
- Fixed one-way/cross model; do not invent leverage or mode mutation endpoints.

## Gate limitations discovered before implementation

The live recorder currently assumes anonymous public data and at least one WS
frame. Breakout has neither: all market reads require a legacy login, and its
real updates are REST polling. Add generic polling capability, never fake WS
traffic. A documented synthetic REST tape is independently verifiable but must
not be described as a live capture. The literal 20-second live gate requires
legacy account credentials; record its actual outcome without claiming a mock
capture is live. No live trading is necessary for private documented-payload
verification.

## Progress

- Read current landing brief/spec, root/package/exchange guidance and Breakout
  CLAUDE first. Fresh branch created from fetched landing head.
- Initial independent auth/account and surface/harness plan audits running.

- Initial plan audits confirmed no protocol inheritance. Phase 1 foundation:
  20 scripted native tests, strict clippy and fmt pass. Independent review is
  finding subscription/scheduler boundary bugs; fixes are in progress.
- TS reference fixes public-only quote polling and duplicate placeholder book
  levels; targeted TS suite: 15 passing.
- Credential-name check: `oneshot-secret ls` reports no store. No credential
  values read or searched. Live recording remains blocked.
- Generic polling harness: 263 tests pass before venue registration. Public
  fixture metadata will explicitly say `documented-scripted`, never live.
- Phase 1 independent review found four defects: empty active-symbol masks,
  synthetic interval coverage, earlier dispatch deadlines, and reciprocal
  leverage overflow. All fixed with regressions: 26 public/auth tests passing;
  strict all-target clippy and fmt passing. Public auth remains socket-free.
  Future-phase files are present but not part of this phase commit.

- Phase 2 parser review caught a further inherited TS bug: native DXtrade lots
  shared a symbol-derived ID and DefaultStore silently collapsed exposure.
  Preserve native positionCode through the existing optional frame ID tail,
  and fix the TS reference mapper. Nested account identities and order quantity
  conservation are checked before atomic commit. Current account tests: 12
  passing; public/auth regressions remain 26 passing.
- Phase 2 independent re-review clear: 44 tests pass (7 parser, 8 account,
  29 public/auth), strict clippy/fmt pass. Review fixed nested account isolation,
  native lot identity, native pending statuses, order quantity consistency,
  initial portfolio failure hot loops and auth-failure fanout. Definitive bad
  credentials stop timers/deactivate; discovery/config errors explicitly settle
  wrapper startup. Zero-size placeholders remain represented by SETTINGS per
  the engine spec. Successful-write tombstone population is tested in phase 3.
- Phase 3 independent review cleared native payloads, ACK-to-code routing,
  cancellation locks, no ambiguous write retries, read revision fences and
  cancellation tombstones. Review caught a poll-only amendment race: DELETE
  success has no fill evidence. Replacements now require a documented filtered
  history read proving the exact chain is CANCELED with zero fills. Missing,
  inconsistent, filled or failed proof aborts replacement. Explicit zero/CLOSE
  payloads remain supported without asserting close-all semantics.
- Phase 4 review found proxy JWT overwriting DXAPI, inert history capability
  branding, public-only validation, and incident slowdown control gaps. Routing
  uses the existing separate Tealstreet-Authorization policy; direct/HOW/native
  preserve DXAPI. Typed live interval controls support slowdown, pause and
  resume; multiplier listeners are removed on disposal.
- Public gate initially exposed an actual startup latency regression: TS permits
  a five-request burst, while the initial Rust scheduler inserted 200ms after
  every call. Replace it with a bounded rolling five-per-second budget.
  No CPU/memory threshold or economic comparison is relaxed.
- Latest scripted native result before final controls review: 89 tests pass;
  strict clippy/fmt pass. Wrapper/utility suites: 34 pass. Generic harness: 271
  pass. Compiled frame/host tests: 3 pass before adding interval control proof.
- Phase 3 and core control re-review clear: 90 crate tests pass, including 26
  trading integration contracts; workspace all-target clippy passes. The poll
  control is idempotent so repeated unchanged worker configuration cannot starve
  polling. Native history fallbacks and exact candle requests are also tested.
- Phase 4 final independent review is clear. A separate reviewer verified the
  DXAPI/proxy-JWT routing changes, including the native loopback wire test.
  Selected compiled WASM integration: 4/4 pass (native lot identity, disposal,
  definitive auth rejection, explicit interval control and pause/resume).
  Root typecheck: 44 tasks pass; root lint: 28 tasks pass. Combined default WASM
  strict clippy also passes. Full root/native regressions are running.
- Phase 5 adds an explicit HTTP-only polling timeline. A bounded final-read
  fence lets each implementation's next natural poll observe the last eligible
  snapshot. It neither injects requests nor rewinds response time, and fails
  closed on missing/superseded reads or unmatched writes. Independent tests
  cover phase offsets, skipped intermediate snapshots, pause/missing reads,
  unchanged socket replays, validators and mutation consumption. Harness: 280
  passing before the final recorder account-selection regressions.
- Full native workspace: 1,570 tests pass before final zero-CLOSE follow-ups.
  Targeted wrapper/config/routing integration: 232 tests pass. Root test run
  hit two existing dynamic-import 5-second timeouts in capability-branding;
  full safe-cex rerun uses two workers without changing assertions/timeouts.
- Final adversarial review added account/private-tape coverage for exact zero
  CLOSE conservation and found zero-CLOSE amendments incorrectly rejected by
  the positive-remaining precondition. A safe zero-CLOSE amendment must still
  obtain exact terminal zero-fill history proof before any replacement.
- Recorder review caught fixture subaccount defaults leaking into live options
  and sole-account anonymization differing from the native selector. Live
  recording now requires caller-supplied account selection when ambiguous;
  sole eligible account inference follows native selection. Credential/token
  and escaped diagnostic guards remain fail-closed.
- Phase 5 independent fence/privacy review is clear. Final harness: 282/282
  pass; full safe-cex CI rerun: 7,020/7,020 pass across 478 files, with the
  existing 5-second timeout unchanged. Root's other 25 tasks passed/cached.
  The CI matrix now runs the honestly scripted Breakout public/private gates
  plus the compiled host/Store/config tests; live provenance is not implied.
- The adversarial zero-CLOSE/identity findings are fixed: exact zero CLOSE
  LIMIT/STOP price amendments still require terminal zero-fill history proof;
  empty optional client IDs cannot merge native cancellation identities,
  bypass busy locks, authorize unrelated proof, or create broad tombstones.
  Account tests cover successful atomic publication and failed conservation;
  private tape includes valid native zero-CLOSE rows, including empty client ID.
  Trading integration: 33/33 pass; strict all-target clippy passes.
- Final native crate result after the account/trading follow-ups: 100/100 pass;
  workspace formatting passes. Pre-landing checks are being refreshed against
  the fetched landing head `1f9e9a5206` before final artifact/replay measurements.
- Landing integration uses actual merge parent `45d7a3fb98` (Phemex joined
  Paradex/ApeX/Kraken while the shared remote ref advanced). Conflict resolutions
  retain incoming layout, binary transport, async readiness, Phemex and every
  venue registration, and Breakout polling/auth/control behavior. A selected
  feature cfg omission for `secret` was repaired. Harness 357/357 and four-venue
  wrapper integration 41/41 pass; selected Breakout WASM cargo check passes.
- Independent merged-source correctness review is clear against ours
  `9e079ad3e5` and incoming `45d7a3fb98`; no unresolved conflicts or weakened
  response validation, economic comparisons, CPU or memory gates.
- Merged selected Breakout WASM: 640,088 bytes (budget 1,258,291), SHA-256
  `f3d483da4e453eef384817602c356b9df791b8425a26db11b97b604d8577b2d8`.
  Compiled integration 4/4 pass. Documented private replay PASS: 5 markets,
  5 tickers, 24 timeline records; main CPU TS 98.4/Rust 45.5 ms and retained
  TS 15.3/Rust 0.9 MB (private performance informational).
- Documented public replay PASS (3 runs): 5 markets, 5 tickers, 2 books,
  15 timeline records; median main CPU TS 28.5/Rust 23.6 ms (1.20x), steady
  TS 12.0/Rust 10.4 ms (1.15x), retained TS 14.1/Rust 0.8 MB (17.14x),
  Rust linear memory 0.3 MB. This is the explicitly scripted fixture, not the
  required live capture. Root typecheck 44/44 and lint 28/28 pass after merge.

## Final local gate ledger (2026-10-20)

Implementation and independent phase/merge correctness reviews are complete.
Sam authorized the documented public fixture as the substitute for the live
capture on 2026-10-20, so gate 5 is accepted for landing. The committed fixture
`sdk-rs/harness/fixtures/breakoutprop-public.jsonl.gz` remains honestly labeled
`source: documented-scripted` and CI runs its correctness/CPU/memory gate.
Live connectivity still needs Sam's legacy DXtrade credentials. The recorder
exits 1 before creating `breakoutprop-smoke.jsonl`; `oneshot-secret ls`
previously reported no store. Record that remaining verification gap in the PR
body and roster. Proceed with push/PR, posted correctness reviews, fixes and
green-CI merge, then cut a fresh follow-up branch.

| Brief gate | Result |
| --- | --- |
| 1. Bans | PASS, `./scripts/check-bans.sh` |
| 2. Rust format/clippy/tests | PASS, workspace fmt and all-target strict clippy; 1,976 workspace tests, including 100 Breakout tests |
| 3. WASM clippy | PASS, default combined features and selected Breakout feature, strict wasm32 clippy |
| 4. Default build + documented private replay | PASS; combined 22-venue WASM builds, generated 26-record private tape, 24 timeline records, 5 markets/tickers; exact output/requests |
| 5. Public | PASS with Sam-authorized documented 20-second fixture (three runs, numbers above); live connectivity pending Sam's legacy DXtrade credentials |
| 6. Binance regressions | PASS, public three-run and generated private one-run gates |

Additional checks: 357 harness tests; 41 cross-venue wrapper tests; 77 compiled
WASM adapter tests; 60 JS/native/Workers host tests; four compiled Breakout
host/frame/control tests on both selected and combined artifacts. Root typecheck
44 tasks and lint 28 tasks pass. Full safe-cex CI before the landing merge passed
7,020 tests; the original broad root invocation hit two existing dynamic-import
5-second timeouts, resolved by the full two-worker rerun with unchanged timeouts.

Final default-build private gate: PASS; TS/Rust main CPU 97.5/38.5 ms, retained
15.2/1.7 MB, Rust linear memory 0.8 MB (private performance informational).
Binance public: PASS, 873 frames, 917 markets, 783 tickers, three books; median
main CPU TS/Rust 297.3/159.8 ms (1.86x), steady 173.0/79.2 ms (2.18x), retained
19.2/2.6 MB (7.49x). Binance private: PASS, 14 frames and two markets/tickers.
The combined artifact is 9,320,943 bytes, SHA-256
`a95da8753ae06586ac1f23ad9fce736729d367cef39bb48e6521941f54a729c1`;
the per-venue size gate applies to the 640,088-byte selected artifact above.

Actual TS reference fixes are public-only quote polling, duplicate synthetic
book levels, native lot IDs, SDK string-array placement ACKs and the standard
If-Match cancellation precondition. Additional deliberately safer Rust behavior
is documented in `KNOWN_TS_FIXES.breakoutprop` and native regressions. No
provably unused crypto helpers were found by the independent caller audit.

## Landing progress

- Sam authorized the documented public fixture for gate 5. Pushed the reviewed
  branch and opened [PR #1328](https://github.com/Tealstreet/tealstreet-sdk/pull/1328)
  into `feat/rust-rewrite`; live connectivity remains explicitly pending Sam's
  legacy DXtrade credentials in both the PR body and roster.
- Fresh independent code-correctness reviews cover auth/account reconciliation,
  trading/query safety, and shared host/routing/polling/recording boundaries.
  Findings will be posted on the PR and fixed before a green-CI merge.
- Posted three fresh code-correctness reviews on PR #1328. Auth and trading
  independently found the documented top-level order-price schema mismatch;
  the boundary review reproduced unstamped live polling responses bypassing
  final-state completion. Both findings are fixed with native/recorder-shaped
  regressions. Existing bulk-cancel failure semantics remain unchanged.
- The order parser and TS oracle now accept documented limitPrice/stopPrice
  plus legacy leg.price fallback. Three new native integration tests cover
  atomic account publication, invalid-price preservation, and amount-only
  STOP amendment through native cancellation proof with absent/zero leg.price.
  The private tape includes documented and legacy STOP representations.
- Live recording leaves pre-ready HTTP retries in their ordered sequence and
  stamps every post-ready polling read with its recorded eligibility time.
  Missing final pollers fail bounded; future responses remain unavailable.
  Removing the stamp prove-failed both new completion regressions; restoring
  it returned the focused 32 tests to passing.
- Integrated Coinbase landing `5c0e437faf` while retaining Breakout and all
  incoming routing/identity/binary contracts. Native golden matrix now has 56
  distinct cases. Merge verification: 217 TS routing tests, 10 native routing
  tests, 28 loopback tests and paired-feature WASM cargo check pass.
- Current fix verification: 103 Breakout native tests, 30 targeted TS tests,
  391 harness tests, workspace strict all-target clippy and format pass.
  Independent fix re-reviews and refreshed final artifacts/gates are running.
- Independent auth/account and trading re-reviews cleared the P1 fix; the
  trading reviewer reran the exact canonical/legacy-zero STOP amendment
  regression successfully. Auth independently cleared the recorder fix and
  reran all 22 polling tests. Findings and resolutions are posted as
  code-correctness comments on PR #1328.
- Linux CI exposed a test-only startup timer race: a polling interval armed
  before virtual-clock startup could observe an extra real-time seed response.
  Arm it at the first virtual tick instead. Exact response and tick-time
  assertions remain unchanged; independent re-review is clear, and the full
  391-test harness passes. No runtime or performance threshold changed.
- Refreshed selected Breakout artifact: 642,528 bytes, SHA-256
  `e7fc05738c15196f77e8f88a571a91f61527c98ab822f97d2351d6ad4bc6fdc4`.
  Private replay PASS with documented and legacy STOPs. One local public run
  missed the steady CPU threshold by 0.8 ms; an unchanged three-run retry
  passed strictly: main CPU TS/Rust 38.0/27.4 ms, steady 19.6/12.8 ms,
  retained 14.1/0.8 MB. The Linux Breakout replay CI job also passed.
- Refreshed native workspace: 2,096 tests pass, including 103 Breakout tests.
  Root typecheck 44 tasks and lint 28 tasks pass. The final acceptance evidence
  for PR #1328 is recorded in its exact-head checks, body and comments;
  live connectivity still requires Sam's legacy DXtrade credentials.
- Integrated Polymarket landing `a0c11afd871b` after it made the PR conflict.
  Kept the incoming optional prediction-frame sections, full recorder window,
  repeated query serializer and all venue registrations, together with Breakout
  credential sanitization, post-ready response eligibility and native lot IDs.
  Independent merge correctness reviews and refreshed gates run on this head;
  exact-head CI must be green before merge. The documented public-fixture
  authorization and Sam's live-credential gap remain unchanged.
- All 34 checks passed on `9b42e70f9f`, including full repository CI (7,262
  safe-cex tests). Local refresh passed 2,185 native tests, 400 harness tests,
  99 compiled WASM adapter tests and 64 host tests. Selected artifact 645,168
  bytes; public TS/Rust main CPU 23.0/17.9 ms, retained 14.2/0.8 MB. Binance
  public/private gates passed; combined 24-venue private replay passed.
- GRVT landed as `5d7589f296` before this PR could merge. Integrated both
  registrations and wrapper methods; preserved the incoming stricter memory
  collection/before-verifier baseline and request audits, plus Breakout polling
  and credential sanitization. Scoped merge reviews pass (233 wrapper/transport,
  six combined WASM, 51 polling/memory tests). Refreshing exact-head CI before
  merge; the public fixture waiver and live-credential gap remain in force.
