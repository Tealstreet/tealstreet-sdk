# Native server session adapter

Branch: `feat/rust-server-adapter`, based on `origin/feat/rust-rewrite`.
Scope: the library in `packages/safe-cex-server`, not a deployment. Keep TS
venues, Redis/cache policies, publicdata, worker shell features and deployment
authorization/provisioning intact. Follow VENUE_PORT_BRIEF process full auto.

## Phases

1. Contract and migration plan. Read the existing native server protocol and
   server library; coordinate with the consumer-blockers owner of
   PublicExchangeFactory startup/refresh. Audit the plan with a sub-agent.
2. Native session transport. A configured account connection relays SCXF binary
   buffers unchanged to a caller-provided asynchronous sink, preserving snapshot
   then deltas. First *binary* message is the snapshot; preceding logs are legal.
   JSON command/result IDs remain client-scoped native protocol IDs. No retry,
   automatic reconnect, second engine or TS exchange facade. Bound queued bytes
   and messages and WebSocket buffered output; fail closed on gaps, malformed
   protocol, sink failure or connection timeout. Commands are gated until the
   snapshot sink completes. Dispose releases the socket and listeners. Validate
   configured URLs/tokens and redact errors. Deterministic transport/lifecycle
   tests and a phase sub-agent review before committing.
3. Library integration. Export a session factory selecting native frame sessions
   for registered Rust venues, with explicit TS selection using the existing
   PublicExchangeFactory handle contract. The trusted embedding caller resolves
   account endpoints/tokens; no client-provided credentials/URLs. Do not change
   cache hydration, startup, refresh, invalidation or disposal policy owned by
   consumer-blockers. Document real client usage including fresh FrameApplier
   per connection and async downstream backpressure. Exercise the actual native
   server over loopback with snapshot, BOOK_DELTA and correlated command results.
   Sub-agent review and fixes before committing.
4. Acceptance and landing. Root typecheck/lint/tests; native tests, Rust bans,
   fmt/clippy/WASM checks and build; frame-wire/host/harness tests; Binance public
   and generated-private gates. No live private requests or deployment. Commit
   progress/evidence. Push PR into feat/rust-rewrite; independent code-correctness
   reviews posted on the PR, fix and repeat, merge only green CI, fresh branch.

## Phase 1 audit

The mirror has no deployed session route to replace. Completion is an exported,
tested adapter/factory with real native loopback evidence and adoption docs;
application deployment remains outside scope. Native notices may precede the
snapshot and include done/error/log/deactivate. Snapshot delivery must finish
before commands enter. A reconnect is a new session and a new delta baseline;
never stack another lossy/coalescing delta layer downstream. PublicExchangeFactory
remains its owner's contract; native sessions do not seed or mutate that TS store.

Progress: initial plan recorded; consumer-blockers coordination requested.

## Phase 2

Implemented the bounded native session with no TS engine/store, command replay
or automatic reconnect. Independent sub-agent review found synchronous abort
inside an injected socket factory could leak its returned socket; fixed by
closing a late-acquired resource before installing listeners. Added regression.
AbortSignal also cancels handshake and pending snapshot delivery. Review now
passes. Focused tests: **92/92**; package typecheck and lint PASS.

## Phase 3

Factory exports explicit native/TypeScript selection. Native calls the trusted
account+venue resolver and never the public factory. TypeScript returns the
original awaited handle; no captured exchange, startup, fetchMarkets, hydration,
refresh or disposal logic is introduced. Coordination messages sent to the
consumer-blockers owner (`codex-c6nf4w`); inspected its current contract, which
keeps get/handle signatures and may replace handle.exchange after Rust refresh.
Callers re-read that field. No PublicExchangeFactory implementation edits here.
Owner explicitly confirmed this seam in coordination thread
`thread-dmajcoys5cu7`: cached hits stay lazy; cold misses await native startup;
successful refresh swaps the engine only after cache persistence, failure keeps
the good handle, and dispose fences in-flight creation/refresh. The overseer
recorded the agreed boundary. The adapter delegates all those policies unchanged.

Plan audit caught inherited registry names reaching the resolver; added own-key
validation and regressions. Disconnect must be wired before awaiting open;
AbortSignal now covers resolution and native transport, with a bounded resolver
deadline and cancellation signal for its external work. Late resolutions never
connect. Real Rust fixture performs no exchange HTTP/sockets and checks actual
snapshot/full BOOK/delta/delete/resync/fresh-reconnect frames through FrameApplier,
byte-identical forwarding, independent clients using the same command ID and
write counters proving no replay. Initial real loopback test **1/1 PASS**.
Full server package **127/127 tests**, typecheck and lint PASS. Independent
phase-3 code-correctness review PASS at `659fe566e6` (factory, documentation,
fixture, harness and CI; excluded reviewer's authored transport). Reviewer
independently ran **8/8** factory tests. No unresolved findings.

## Landing integration and gates

Committed phase-3 checkpoint `1f0aa3b392`, then merged landing `0063068241`
without conflicts (`659fe566e6`). No stashes/rebases/force pushes. No PR yet.
Initial Rust workspace run: **2,370/2,370 tests**, 190 groups, no failed/ignored;
workspace all-target clippy, bans/fmt and workspace wasm32 clippy PASS. Re-running
integration/package/root gates after the merge, then current-head WASM build,
host/harness/frame-wire and Binance public/private acceptance before opening PR.
Phase-4 sub-agent audit confirmed this gate set; added explicit CI fixture
prebuild so cold native/TLS compilation uses the existing job budget rather
than the integration runner's 120-second fixture deadline. No budgets, thresholds
or timeouts changed. No additional production correctness findings.

Final local acceptance (landing `0063068241` integrated):

- Root typecheck **44/44**, lint **28/28**, tests **27/27** tasks. Each SDK
  runner passed **7,328 tests / 505 files**; server **127 tests / 10 files**.
- Rust workspace **2,370 tests / 190 groups**, no failures/ignored; workspace
  all-target clippy, wasm32 clippy, fmt and banned-pattern checks PASS.
- Combined release WASM build PASS. SDK/WASM **376 tests / 11 files**; host
  **68/68**, Rust writer → JS/FrameApplier **1/1**, harness **533/533**.
  An initial parallel WASM run hit the existing five-second KuCoin startup
  deadline and was interrupted; the full suite passed with one worker and
  unchanged deadlines. CI will also run its normal WASM suite.
- Offline real native WebSocket adapter integration **1/1 PASS**. Fixture JSON
  serialization uses typed structs; native all-target clippy and integration
  rechecked after that change. No external venue requests in this fixture.
- Binance public **PASS**: **873 frames, 917 markets, 783 tickers, 3 books**;
  main-thread CPU TS **299.2** / Rust **159.6 ms** (**1.88x**), steady
  **175.9 / 76.7 ms** (**2.29x**), retained **16.4 / 3.3 MB** (**4.92x**).
- Generated Binance private **PASS**: **14 frames, 2 markets, 2 tickers**, no
  books. Private CPU/memory are informational only.
- Isolated Binance WASM **902,790 bytes**, existing budget **1,258,291**.
  Built with the same release/features/bindgen flags to a temporary binding
  directory, preserving the combined local bindings. No size budget changes.

No TS venue bugs fixed, no TS venues removed, no provably unused crypto helpers
introduced. No deploys, live private trading, or changes to the other lane's
factory implementation. Application authorization/provisioning, deployed route
adoption and TS shell features remain application-owned as documented.
