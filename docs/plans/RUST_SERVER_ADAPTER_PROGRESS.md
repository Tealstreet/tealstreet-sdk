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
