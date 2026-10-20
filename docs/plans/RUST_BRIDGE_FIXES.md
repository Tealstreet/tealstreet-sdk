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
- Phase 1 implemented and independently reviewed PASS. Host traps latch terminal
  state, clean up I/O and preserve unknown-write identity; the wrapper wakes
  startup and reports failure once. Running workers reconstruct at most once,
  retain safe demand, stop algos and never replay writes. Shared ports require
  fresh attachment. Native initial empty snapshots discard main book baselines;
  terminal lifecycle updates mark unavailable while retaining last-known venue
  data. Capture freshness IDs before inherited disposal resets the local store.
- Review findings fixed: subId-only unsubscribe replay, synchronous first-slice
  TWAP failure escaping ownership cleanup, replyless control rejections, shared
  client startup acknowledgement during recovery, pending RPC settlement and
  preservation of already acknowledged order IDs, and false recovered status
  if subscription replay traps the replacement. A terminated TWAP's pending
  placement continuation can no longer schedule another slice.
- Phase 1 verification: 98 host tests; 447 SDK WASM cases in 17 files; 107
  relevant worker/handler/TWAP cases across scoped runs (including final 18-case
  recovery suite); whole SDK typecheck and scoped strict lint PASS. Source
  review PASS after fixes. New real minimal-WASM unreachable controls plus two
  actual-native account/stream integration tests establish containment and
  sibling/baseline behavior; they do not claim a live venue panic was observed.
  Native source is unchanged. Local combined artifact is 12,172,135 bytes,
  SHA256 `51e335bd0acd407f75b51b26bf780d083ea49bd720b9533c2fc2cc9906c2d3c3`.
  Broad final gates and PR review remain phase 7 work.
- Phase 1 follow-up CI on `6dcaf5446c` is green (run `39416821651`):
  44 typecheck, 28 lint and 27 test tasks; SDK 7,768 tests in 526 files
  passed twice. Phase 2 now passes all 450 SDK WASM tests in 18 files using
  the same combined native artifact; native source remains unchanged.

## Phase 2 plan and audit

- Retain current listener demand, not a chronological command queue. Use native
  STATUS/MARKETS readiness rather than cache-seeded JS flags. Apply and validate
  native metadata before replay, and forward the catalog before a synchronous
  demand-generated data frame can reach the main consumer.
- Register callbacks before sending the first subscription. Capture both book
  and candle listener-container snapshots before replay; recheck map identity
  and nonempty demand immediately before each send. This avoids replaying a
  removed/replaced topic or duplicating one added reentrantly. Preserve callback
  refcounts, Aster independent listener semantics, canonical intervals and
  symbols containing colons. Shared demand waits for compatible native catalog.
- Public reads on an unstarted object must not return cache seed or empty data
  as native truth. Start the existing single-use engine and await only the
  relevant native market/ticker readiness, with terminal/startup failure and
  bounded waiting; public reads do not wait on unrelated private balances.
  Running reads remain snapshots, not a claimed forced metadata refresh.
- The actual factory cache-poisoning caller is already fixed by #1342. No
  in-repo caller proves successful validateAccount must auto-dispose; retain
  caller ownership and document that lifecycle-only skip. Do not break reuse
  merely to mirror a TS startup/cleanup shape.
- Tests: delayed module and native metadata with seeded JS flags, prestart
  subscribe/unsubscribe, duplicate/refcount behavior, reentrant add/remove and
  immediate callbacks, shared mismatch with zero provider demand, forwarding
  catalog-before-book ordering, and public metadata available while private
  balance is still pending. Run scoped checks, independent review, fixes and
  commit before phase 3.

## Phase 2 implementation and review

- Native STATUS now owns readiness independently of seeded JS flags. Live
  subscription demand replays after native catalog application/validation and
  forwarding; callbacks exist before the first send. Public reads start the
  single-use engine and await only relevant native sections, bounded at 30 s.
- Independent code-correctness review PASS after fixing shared ticker reads
  that could return the receiver's REST seed before provider quotes arrived.
  Market-only reads may finish after native catalog validation; ticker reads
  also await accepted shared quotes. Actual-native regressions cover delayed
  metadata/subscriptions, a rejected shared catalog with zero forwarded demand,
  and delayed valid provider quotes. Nine focused unit cases cover reentrancy,
  seeded readiness, disposal and concurrent reads. All 450 WASM cases pass.
- Existing fake-ABI wrappers that emitted no STATUS now emit independent fixture
  STATUS frames matching their formerly seeded readiness. ApeX's controlled
  account readiness and Aster's hedged flag are modeled as native inputs;
  business assertions and deadlines are retained. The original GRVT case
  failed at its unchanged 5 s deadline before that fixture correction.
  Scoped verification: 25 startup/parity unit cases plus 132 distinct wrapper
  cases across eight files pass; the initial Aster fixture mismatch was corrected
  by supplying native hedged state and its unchanged 20 cases then pass. SDK
  typecheck and strict scoped lint pass. Independent fixture review confirms
  retained assertions/deadlines and private-readiness hold/release semantics.
- Successful validation retains caller ownership: no in-repo consumer proves
  automatic disposal necessary. This lifecycle-only audit item is skipped under
  the production-impact judging rule. General unstarted public reads are fixed
  here in addition to the factory-specific startup/refresh closure in #1342.

## Phase 3 audit notes (implementation not started)

- One host settlement helper must remove the pending entry and clear its
  deadline before resolve/reject, including malformed success JSON. Currently
  `done` deletes the entry before parsing, which can strand the promise.
  Unknown host actions must fail visibly without dropping unrelated completions
  from the remaining batch. Timeouts expire caller waiting, not venue actions;
  no automatic retry or compensation is implied.
- Normal dispose needs best-effort cleanup and typed interruption errors as
  well as the already implemented trap path. Worker dispose currently sends a
  generic RPC failure before the typed host rejection; preserve identity and
  outcome through that ordering. Known native rejections retain partial batch
  metadata and do not become unknown outcomes.
- **Identity coverage is currently caller IDs only.** A host-only timeout or
  dispose fix cannot claim generated native IDs. Add typed native command
  identity/progress, correlated by command/input order/child role where needed,
  from existing formatter/job context before the corresponding write escapes.
  Capture actual native IDs separately from requested IDs and represent partial
  coverage honestly. A deadline may precede a queued job's ID generation, so
  later identity progress needs an account/generation/command-correlated receipt
  path even after its promise settles. Never parse venue HTTP bodies in JS,
  mint incompatible substitute IDs, or query a poisoned engine.
- Native paired command log+done sites have the command ID locally. Prefer
  explicit optional command-ID log metadata/helper there; the SDK can retain
  diagnostic command logs and emit one failure at the public command boundary.
  Background notices remain visible. A batch-local exact-message compatibility
  match is bounded but cannot distinguish an unrelated identical notice, so it
  must not be represented as causal proof or a global string/time deduper.
- Cancel bridge keeps existing `ids` dispatch and sends parallel descriptor
  metadata `{id, symbol, type}`. Do not route every venue through today's
  feature-gated symbol-aware operation. The other lane owns unknown-ID venue
  handling and its generic native consumption seam.
- Fake-clock checks cover completion/deadline boundaries, late/duplicate done,
  concurrent reads/writes, caller/generated/batch/child IDs before dispatch,
  late identity receipts, cleanup throws and partial results. Verify ordinary
  validation, timeout/dispose and native failure each report once, while
  independent identical failures retain independent reports.
