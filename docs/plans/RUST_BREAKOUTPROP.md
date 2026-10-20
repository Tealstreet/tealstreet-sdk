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
