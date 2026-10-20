# Rust bridge production fixes

Branch `feat/rust-bridge-fixes`, from landing `fc809a9d85` after PR #1342.
Follow `VENUE_PORT_BRIEF.md` process and the judging rule in `RUST_AUDITS.md`:
fix production loss, incorrect/stale/missing data, invisible errors and broken
consumers; lifecycle shape alone does not require TS parity. No TS deletion,
default switch, deployment or publication. Never stash, rebase or force-push.

## Scope and ownership

Take every finding under **Port reviews → Engine and TS bridge**, plus the
user-assigned **Registration and options** items. Leverage **side** belongs to
`feat/rust-leverage-side`; venue behavior for unknown cancel IDs belongs to
`feat/rust-startup-broker`. This bridge sends cancel `{id, symbol, type}`.
PR #1342 closes ignored privateOnly/legacy externalPublicData and factory cold
startup/refresh. General unstarted fetchMarkets/fetchTickers still needs an
independent consumer-impact check; factory-only closure is not a blanket skip.

## Phases

1. **Engine failure and recovery.** Contain WASM traps, expose a terminal engine
   state, settle pending work, stop poisoned callbacks, and rebuild worker-owned
   engines without replaying writes. Preserve account isolation and shared module
   initialization. Fail closed when recovery needs a fresh shared-feed port.
2. **Startup demand.** Replay live book/candle subscriptions after native
   metadata is ready; retain callback/refcount and shared-provider semantics.
   Check unstarted public reads and validation resource ownership by impact.
3. **Command outcomes.** One visible error per failed operation, command
   deadlines, interrupted-write unknown outcomes with client IDs, complete
   cancel descriptors, and visible unknown-action failures without losing
   unrelated batch completion. Preserve partial-order metadata; never retry
   writes automatically.
4. **Native account queries.** cancelAll/nuke and account reads use authoritative
   native state; add explicit reconciliation/resync, route fetchLeverage for
   MEXC/BingX/WEEX, retain fetchedLeverage in frames, and getAccount where
   consumers need it. Examine removeOrders for actual native/store impact.
5. **Wire and numeric findings.** Fill realized PnL/fees, candle lastSide,
   generic order triggerType, unsupported Aster candle intervals and invalid
   numeric adjustment steps. Test meaningful consumer loss, not TS layout.
6. **Polling and credentials.** Send resolved BaseExchange.getTickInterval()
   whenever it changes to every Rust venue. Include Infinity (stop polling),
   server slowdown and credential backoff; each reconciliation timer honors
   it without disabling startup, socket supervision or explicit reads.
   Coinbase read-only keys get their own JWT/read credential route, or are
   explicitly dropped with a warning rather than failing account startup.
7. **Acceptance and landing.** Root typecheck/lint/tests, bans/fmt/strict
   native+WASM Clippy, native tests, actual WASM/worker/consumer regressions,
   harness and all touched venue/Binance public/private/size gates. PR into
   feat/rust-rewrite, post independent correctness reviews, fix/repeat, merge
   only green CI and cut a fresh branch.

Each phase gets an independent sub-agent correctness review, fixes, scoped
verification and a stable commit. Keep failures and evidence boundaries visible.

## Phase 1 plan and audit

- Files: WASM host and declarations/tests, RustEngineExchange, ExchangeWorker
  and worker ownership/transport declarations only where recovery requires;
  package/worker docs and fresh deterministic regressions.
- Guard native invocation boundaries, not arbitrary user callbacks or ordinary
  native Result/validation rejection. Trap handling latches terminal state
  before cleanup, aborts requests/signing, cancels timers/sockets, settles all
  pending commands once and notifies the owner once. Late callbacks never
  reenter the failed instance. An in-flight write must remain an unknown
  outcome, including caller client ID where supplied; recovery never resends it.
- Guard WS text/actions/UTF-8/binary, HTTP start/chunk/end/failure, timer,
  signing callbacks, start/command, frame/snapshot and account reads. Avoid
  allocating per-message wrapper closures on the streaming fast path.
- Wrapper wakes startup waiters, emits a visible terminal error and disposes
  intervals. Worker reconstruction uses a fresh exchange/generation, full
  snapshot and clean freshness baselines. Explicit dispose cancels recovery;
  retries are bounded. One account's trap does not reset module-wide cached
  wasm-bindgen initialization or sibling engines. Shared-feed recovery cannot
  reuse a closed transferred MessagePort; require a fresh provider attachment.
- Independent plan audit confirms real failures: WS/timer/frame traps escape;
  HTTP catches can feed a poisoned engine again; command catch leaves siblings
  pending; worker heartbeat remains healthy after a native panic. Retained
  pre-start subscription maps also drop native demand (phase 2), and replay
  must wait for metadata rather than just host.start().
- Verification: fake controllable transport exercises every native boundary,
  real minimal WASM unreachable export classifies a trap, and tests distinguish
  ordinary command/user-callback/transport errors. Worker fixtures prove fresh
  reconstruction, stale-generation suppression, explicit-dispose cancellation,
  shared-feed fail-closed behavior and no write replay. Run package scoped
  typecheck/lint plus meaningful host/SDK worker tests, then review and commit.

## Progress

- Plan recorded after PR #1342 merged with 40/40 green checks: 40 public,
  64 private and 30 size gates; 2,794 native tests, 7,748 SDK unit and 443 SDK
  WASM tests. Detailed receipt and original failed trials remain on that PR.
- Phase 1 plan independently reviewed; implementation pending.
