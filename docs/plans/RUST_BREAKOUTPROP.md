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
