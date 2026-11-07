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

The broad CI run on phase-2 commit `5d95bf7078` (`39418652867`) exposed
seven more missing-STATUS fixture timeouts in Orderly transport, BreakoutProp
wrapper and spot validation: 7,770 pass / 7 fail. Those three fixtures now emit
native STATUS without changing assertions/deadlines, with independent review
PASS. The later full phase-3a run passes all 7,807 SDK unit cases.

## Phase 3 audit notes

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

- The actual CLI executor catch/router previously erased structured failures;
  phase 3a preserves them for propagated errors (including actual buy/sell).
  Follow-up account/aggregate commands still need production-impact checks:
  CloseCommand's global-close catch prints then returns success, Swarm drops
  rejected allSettled reasons/partial IDs, and Nuke starts unawaited work. Carry
  those concrete consumer findings into phase 4's nuke/account work, retaining
  acknowledged earlier calls when a later multi-symbol placement fails. Do not
  claim the outer executor alone fixes these internally converted outcomes.

### Phase 3a implementation plan

- Host-only command completion has one settlement path: native done, malformed
  result, synchronous rejection, deadline, fatal trap and normal disposal each
  remove pending work and clear its timer once. Add configurable 120 s default
  deadline with per-call positive bounded integer override outside native JSON.
  Long paginated queries can override it; it is a caller waiting limit, never a
  cancellation or automatic retry. Start timers only for still-pending commands
  after synchronous native processing, accounting for admission elapsed time.
- Interruption codes are ENGINE_DEAD, COMMAND_TIMEOUT and ENGINE_DISPOSED.
  Interrupted writes explicitly retain unknown outcome and currently available
  caller IDs. Native-generated identity coverage remains phase 3b work, so this
  checkpoint makes no complete attribution claim. Ordinary cleanup is best
  effort; one throwing transport disposer cannot strand sibling commands.
- Process the rest of an action batch after an unknown action, then surface the
  first unknown-action error. Keep ordinary callback/transport errors distinct.
  Preserve native partial rejection fields and settle malformed JSON visibly.
- Worker disposal fences state immediately, retains a bounded pending-RPC grace
  window for known acknowledgements or structured host rejections, then sends a
  fallback only for still-owned requests. Repeated disposal cannot erase that
  grace window. Extend bounded error serialization for interruption/partial-order
  fields and additive exec error details where the original Error is available.
- Bridge cancels retain each venue's existing command operation and add caller
  descriptors containing id, symbol and type; generic ID-only commands retain
  ids for compatible dispatch. Venue handling of unknown IDs remains owned by
  the startup-broker lane.
- Verification: fake-clock completion/deadline boundaries, late/duplicate done,
  malformed results, unknown-action batch tails, throwing cleanup, concurrent
  reads/writes, retained partial results, known-ack-before-dispose, repeated
  dispose, stale-generation suppression and account isolation. Run host and
  affected SDK tests/types/lint, independent review/fixes and stable commit.

### Phase 3a implementation and review

- One host settlement path owns pending replies and deadline cleanup. Default
  wait is 120 s; per-host/per-call overrides remain outside native JSON. SDK
  exposes `extra.rustCommandTimeoutMs`; Rust worker RPCs allow one additional
  second for delivery while TS keeps its existing five-second wait. Register
  main RPC listeners before dispatch, including synchronous replies.
- Interrupted writes retain unknown outcomes and known caller IDs on timeout,
  ordinary disposal, fatal traps, post-dispatch deadline setup failure and
  malformed/unsupported protocol results. Review fixes add distinct
  COMMAND_DEADLINE_FAILED and ENGINE_PROTOCOL_ERROR codes, preserving known
  native failures, partial results and already acknowledged successes. A
  callback throwing undefined remains a rejection. No timeout cancels venue
  work, retries it or claims native-generated identities are already covered.
- Worker disposal immediately fences state, then retains one bounded correlated
  reply grace window. Known acknowledgements and command-specific errors win
  before fallback; repeat disposal cannot erase the window. Explicit allowlist
  serialization survives worker, CLI executor/router and standalone REPL
  boundaries. No-reqId CLI failures honor errorReported and print once.
- The actual standalone retry loop now stops for an unknown outcome or accepted
  partial members, preserving IDs and original failure metadata. Fully rejected
  operations retain the user's explicit retry budget. Workflow docs describe
  the changed behavior. Cancel descriptors preserve id/symbol/type alongside
  existing operation/legacy IDs. Unknown-ID venue behavior stays out of scope.
- Independent host, wrapper/fixtures, worker/CLI, and standalone retry reviews
  PASS after fixes. Verification: 121 host tests (23 new command cases), all
  7,807 SDK unit tests in 529 files, all 450 SDK WASM tests in 18 files, and
  589 harness tests PASS. Scoped leaf checks: 67 worker/helper/handler, 45 CLI
  core/router, 14 standalone actual-command/retry and 66 final RPC/fixture
  cases PASS. SDK/CLI-core/app CLI typechecks and strict scoped lint PASS;
  the existing repl file has six pre-existing unused-variable warnings and no
  lint errors. Initial new-test type/fixture errors were corrected; no gate,
  business assertion or test deadline was weakened. Native code/artifact is
  unchanged at this checkpoint.
- Remaining phase 3 work is native-generated identity progress and command
  error-report correlation. The dedicated identity plan records source hook
  inventory rather than claiming runtime proof. Account/aggregate consumer
  findings and phases 4–7 remain outstanding; no PR/landing claim yet.

### Phase 3b implementation plan

- Native emits typed command progress before the matching write action. Required
  receipt fields are symbol, identity kind and opaque string value; capture all
  actual parent/split/child/replacement identities without changing entropy,
  signatures, native payloads or retry behavior. No existing consumer requires
  reconstructed input indexes; keep them optional rather than refactoring every
  planner. Namespace client IDs, Lighter indices and Breakout order codes.
- Cx owns no receipt ledger/Store section. WASM serializes the notice; native
  hosts observe it in action order and server routes it to the initiating client
  with the original command number. Do not broadcast receipts to other clients
  or claim durable remote delivery before dispatch.
- Host stores validated tuple-deduplicated receipts before callbacks, snapshots
  them immutably into interrupted errors, and forwards progress after deadline
  expiry using a host-session receipt key. Existing clientOrderId fields remain
  requested IDs; actualClientOrderIds/orderIdentities describe native values.
  Completion closes the correlated stream without an unbounded tombstone map.
- Wrapper emits typed progress and logs late identity receipts through the
  existing log transport. Worker attaches account and originating generation,
  independent of pending RPCs, without public-feed/frame propagation. Validated
  error serialization retains receipts across consumer boundaries.
- First native slice covers core/hosts plus Coinbase and Txflow; root adds the
  retained typed dispatch hooks for Backpack, GRVT, Lighter and Phemex. Remaining
  venue formatter hooks follow in disjoint reviewed slices. No complete
  generated-ID coverage claim until every actual generation path is verified.
- Verify receipt-before-I/O, unchanged request/signature oracles, numeric-string
  precision, omitted identities, immutable timeout/dead/dispose errors, late
  progress/completion, worker generation/account isolation and no write replay.
  Run scoped native checks, host/SDK tests and independent review before commit.

### Phase 3b first-six checkpoint

- Core/native/WASM hosts publish typed progress in action order; native server
  keeps the original caller command and client ownership until done. The TS
  NativeFrameSession accepts the additive notice without closing the session.
  No public SCXF section or cross-client receipt broadcast is introduced.
- Host snapshots immutable, namespace-separated actual identities before user
  callbacks. A broken progress/log consumer cannot turn an already dispatched
  write into an ordinary rejection. Late SDK progress also uses the existing
  log path; worker messages retain account and engine generation independently
  of pending RPCs. Independent core/native, host/SDK and server/worker reviews
  pass after fixing callback-error handling.
- Native hooks cover Backpack (uint32 IDs, including generated pairs), Coinbase,
  GRVT, Lighter (client-order-index namespace), Phemex and Txflow. Retained typed
  fields supply identities without changing payloads, signatures or entropy.
  Backpack internal repair jobs are excluded from external command receipts.
- Scoped verification: 734 selected native tests across core/host and six venue
  runs, one WASM notice-encoding test, 130 host tests, 37 worker/helper tests and
  100 native TS session tests pass. Selected native and wasm32 strict Clippy,
  bans, format, SDK/server typechecks and scoped lint pass. This is partial
  venue coverage, not all-venue or final gate acceptance.
- Official combined build: 12,178,853 bytes; identical web/Node SHA256
  `727ba33169fbc5676b1c109d1d6532a3936a9e5df6267cc3addf6e6728499e99`.
  All 11 actual Coinbase WASM cases pass, including receipt-before-HTTP,
  immutable timed-out snapshots, a queued generated ID delivered late under
  the same key, throwing listener fallback and exactly two writes. The initial
  new-test 20 ms deadline expired before first dispatch while startup work
  drained; the fixture now observes the first dispatch before queuing command
  two and uses 1 s command deadlines. Existing assertions/deadlines are intact.
- Follow-up on this stable artifact passes all 451 SDK WASM cases in 18 files.
  The first-six checkpoint is committed and pushed as `c3747accfe`.

### Phase 3b remaining venue plans and audit

Three disjoint native slices have independent read-only plan audits. Each must
retain exact final typed identities, emit only for the selected write and leave
signing, entropy, omission and retry behavior unchanged:

- A: Binance, Bybit/Hyro, OKX, Bitget and ApeX Omni. Preserve serialized fallback
  metadata and split/bracket/replacement IDs; Hyro replays existing dispatched
  create links to late waiters, bounded by its existing creates. A rejected link
  replaced before another dispatch is not attributed to new waiters.
- B: Hyperliquid/spot, Kraken futures/spot, Kucoin futures/spot, MEXC, BingX and
  BloFin. Carry IDs through serialization and batching. MEXC reports the final
  refreshed payload's ID, not discarded preflight IDs. Missing trigger/attached
  identities remain absent; BloFin preserves per-batch alignment.
- C: Bitrue, BreakoutProp, Paradex and WEEX. Bitrue reports final per-split child
  IDs; Breakout uses orderCode; Paradex retains receipts through JWT/delegated
  signing queues and preserves identity-free children. Replacement guards run
  before reporting an actual replacement write.

Each slice runs scoped native tests/strict Clippy and independent implementation
review before its checkpoint. Tests compare receipts to final outgoing fields
before I/O, including omitted IDs, caller/generated variants and delayed or
rejected replacements. Combined artifact and broad gates follow stable sources.

Root additionally covers caller-only Crypto.com, Orderly and WOO. Orderly's
retained Place order supplies the actual client ID. Crypto.com retains an
optional receipt on Call::Place before Params serialization; WOO retains it on
Accepted::Regular before query construction. Emit at selected dispatch, never
for cancels/amends/protection without a new ID. Tests prove exact outgoing
caller value and absence when omitted. These hooks close actual-value reporting
for caller-only venues without introducing native ID generation.

Caller-only slice implemented and independently reviewed PASS. All 246 native
tests and strict Clippy for Crypto.com, Orderly and WOO pass; three new cases
exercise supplied/absent IDs and compare final native fields in action order.
Selected crate formatting passes. Combined-artifact verification for these hooks
follows the remaining native slices; the existing 451-case run covers only the
first-six checkpoint artifact.

Group C implemented and independently reviewed PASS across all six production
and seven test/support files. Four-crate native run passes 379 cases in 23
nonempty suites; strict all-target Clippy and scoped fmt pass. Four focused
post-review guard/quote cases pass after adding explicit no-receipt assertions
for rejected replacements and metadata reads. Six new integration cases plus
strengthened existing controls exercise final Bitrue split child IDs, Breakout
order codes, Paradex JWT/delegated dispatch and WEEX batch tails. Review found
no payload, signing, entropy or retry changes. Actual combined WASM and broad
gates remain pending for these newly added hooks.

Group A implemented and independently reviewed PASS after fixing a genuine
Bybit receipt-transfer omission in the initial single/batch planners. The new
regression failed zero-versus-one receipt before the correction, then passed
both initial/fallback and spot single controls. Five-crate native run passes
635 cases, including 12 new regressions; strict all-target/all-feature Clippy,
targeted formatting and diff checks pass. Tests cover Hyro shared/late waiters,
rejected-link replacement and omitted trading-stop IDs, option generation,
Binance PM/batches, OKX canonical symbols, Bitget rollback and ApeX reminting.
No payload, signing, entropy or retry changes; broad artifact gates remain due.

Group B implemented and independently reviewed PASS across eight production
and ten existing test/support files. Six-crate all-feature native run passes
616 cases in 40 nonempty suites; all-target/all-feature strict Clippy, check,
format and diff checks pass. Ten new cases and strengthened existing request
boundaries cover final regenerated MEXC batch/fallback IDs, BloFin partitions,
Hyperliquid spot/brackets and Kraken/Kucoin futures/spot, plus absent BingX
trigger IDs. The initial new MEXC fixture fired a pacing timer early; it now
uses the emitted at_ms, with a focused pass and final 616-case pass. Production
code was unchanged by that fixture correction. All generated-ID source slices
are implemented; final combined WASM/harness/size acceptance remains outstanding.

### Phase 3c plan for command errors

Plan audit passes with explicit error-only `Notice::CommandError { command,
message }` / `Cx::command_error` and WASM `commandError` action. Native server
routes this notice only to the initiating client/original command. Command zero
without a reply retains ordinary error-log visibility. The host records its
first diagnostic before callbacks, settles known success before final error
delivery and contains diagnostic callback failures through the remaining action
batch. The SDK command catch owns rejected-result error reporting. This leaves
partial-success errors visible without string matching or a permanent ledger.

- Add explicit native command-log attribution at sites where the command is
  known, including errors deferred until batch completion and Hyro waiters.
  Ordinary background logs keep their existing behavior. Avoid message/time
  matching, which could hide an unrelated identical failure.
- Correlated command diagnostics remain visible as logs while pending. The
  host retains only the first diagnostic per pending command, not a permanent
  error ledger. A rejected result is reported once at the SDK command boundary;
  a successful partial/best-effort result with a native error diagnostic still
  emits one visible error. A deadline reports its unknown outcome; later native
  diagnostics remain logs and do not create duplicate command toasts.
- Consumer callback failures must not replace known command outcomes or make a
  dispatched write look safely rejected. Preserve native partial-result fields,
  identity receipts and existing worker/CLI errorReported propagation.
- Cover same-frame and deferred native log+done, pure validation rejection,
  partial success, identical independent failures, background errors, timeout
  followed by late error, and throwing diagnostic listeners. Update native and
  TS session protocol validation if an additive notice is used. Independent
  review and scoped checks precede the command-error checkpoint.

Phase 3c review checkpoints (not final acceptance): core/native 168 cases,
one WASM encoding case and 106 TS native-session cases pass; core strict Clippy,
server types/lint pass. Independent core/server review passes. Host review fixed
an extra error-level report when a diagnostic listener throws; all 141 host
package cases now pass, including 11 diagnostic cases. SDK focused 41 cases,
types/lint pass. Independent review additionally found Bitrue mixed snapshot
reads completing leverage waiters before a global error, and CLI executor
printing a rejection already shown by the exchange listener. Both get scoped
fixes and regression controls before closure. Native source refroze after the
Bitrue fix; a second official combined build is required and underway rather
than using the earlier artifact as proof of that fix. Broad venue proof and
real Orderly WASM rejection/partial-result coverage remain pending.

Those review findings are fixed and independently rereviewed PASS. SDK focused
67 cases, CLI actual-command/error/router 29 cases, SDK/CLI typechecks and scoped
strict lint pass. The CLI actual Buy controls cover reported, frozen and
unreported failures while retaining unknown/partial/requested/actual identity
metadata. Native Bitrue corrected-source account run passes all 17 cases,
including shared-waiter transport/malformed failures and no-waiter background
controls through the existing safe-read retry budget.

Second official combined artifact is 12,210,708 bytes, identical web/Node SHA256
`36bc620b0c33ff60c3163242a732139f6a6001f2b5d55ed879b7d22fced2ea4e`.
All 452 SDK WASM cases in 18 files pass. The 27-case actual Orderly/Coinbase
subset independently passes: two same-text native validation failures emit
two errors, then a partially rejected batch emits exactly one additional
error while preserving its accepted large-string order ID and both identities.
Full SDK unit and final all-feature venue verification remain in progress.

Phase 3c acceptance PASS: all 7,830 SDK unit cases in 530 files, all 452 SDK
WASM cases, all 141 JS host-package cases, 2,637 all-feature venue native cases
across 26 crates, 168 engine/native-host cases, one focused WASM encoder case,
106 TS native-session cases, and 29 focused CLI cases pass. The 67-case SDK
focused run overlaps the full unit run; Bitrue's 17 cases are included in the
2,637 and are not additional. Native all-target/all-feature strict Clippy,
selected core and wasm32 strict Clippy, bans, scoped fmt, SDK/server/CLI types
and scoped strict lint pass. All independent reviews pass after the host
diagnostic fallback, Bybit transfer, Bitrue waiter and CLI printer corrections.
The native scope adds 49 explicit command-error sites across 16 venue crates;
ordinary background and duplicate-inflight diagnostics remain uncorrelated.
No payload, signing, entropy, retry or native result semantics changed.

Next: merge latest landing (`702a62050c` at the last fetch), retaining its
broker/cancel repairs, then refine account-query and polling/resync dependencies.
Phases 4–7 and the queued RUST_QUALITY follow-ups remain outstanding. No PR,
all-harness acceptance, or cutover claim is made at this checkpoint.

Landing integration: merged `702a62050c` after phase 3c. Four conflicts were
resolved by retaining both appended test groups (Bitget/Bitrue/OKX) and landing's
Kucoin account-wide stop cancellation guard plus this lane's typed receipt/error
metadata. Independent four-file review passes; 512 native cases across the four
conflicted crates, strict all-target/all-feature Clippy and scoped fmt pass.
Twenty-nine relevant SDK cases pass. Four incoming broker-fixture cases first
timed out because they supplied only seeded JS readiness; they now emit native
STATUS with the same intended flags, preserving all assertions and the 5 s
deadline. Independent fixture review and strict lint pass. Incoming broker,
unknown-cancel and Kucoin scope fixes are retained. Final artifacts/gates must
be regenerated after further native work.

Planning order adjustment: implement phase 6a's shared periodic-poll scheduler
before phase 4's explicit resync hooks. Timer kicking alone cannot refresh
Hyperliquid/GRVT/OKX's full account, so resync remains a separate completion-aware
venue operation. Phase 6b must add classified native credential evidence;
reflecting manual Base auth-streak mutations alone would not establish actual
credential backoff. No body/log regex or signed-header inference is planned.

## Phase 6a scheduling foundation plan and audit

- Engine owns an explicit periodic-poll timer registry. `Cx::set_poll_timer`
  keeps the logical TimerId expected by venue handlers; a bounded variant keeps
  explicit retry/rate floors. Physical host IDs change on policy replacement so
  queued stale callbacks cannot execute the newly scheduled logical timer.
- Apply valid TickIntervalChanged policy centrally before the legacy venue
  hook. Finite positive values replace fallback cadence; Infinity retains
  logical registrations while removing physical timers. Invalid updates leave
  policy unchanged. `clear_timer` removes paused and active registrations.
  Consume a registration before dispatching its logical callback; asynchronous
  completion rearms against current policy. Ordinary timers remain unchanged.
- Enable the existing polling-control decoder for every WASM venue selection.
  Preserve distinct fallback cadence, absolute cooldown and minimum-backoff
  floors. Do not treat every native default as a minimum (which would ignore a
  faster policy), or overwrite backoff with a faster configured cadence.
- This first slice touches core/host feature plumbing and deterministic core
  regressions. Venue registration and SDK initialization/update propagation
  follow after independent review; it does not yet claim all venue polling is
  fixed. Retain heartbeats, socket recovery, metadata periods, post-write repair,
  valuation and in-flight deadlines. OKX's valuation/GC timer is not REST
  polling. Breakout's public-only poll must register even when initially paused.
- Verify pause at registration, update/resume with fresh physical ID, queued old
  callbacks, clear while paused, multiple polls, floor preservation, in-flight
  completion after policy change and unaffected ordinary timers. Run core/native
  and wasm32 strict checks, parser feature tests and independent review before
  the foundation commit. No resync/network-completion claim in this slice.

Foundation implemented and independently reviewed PASS. Core all-feature run
passes 106 cases (91 unit, eight new scheduler, seven shared-feed); default
feature run passes 100, overlapping those cases rather than adding 100 distinct
cases. Coinbase-only WASM parser case passes, proving polling-control is no
longer dependent on enabling Breakout. Strict core all-target/all-feature and
selected Coinbase/Breakout wasm32 Clippy, bans and scoped fmt pass. Logical
registrations survive pause, obsolete physical callbacks are fenced, floors
remain distinct from fallback cadence, and consumed registrations are removed
before callbacks. Venue timers, SDK policy propagation, classified auth feedback
and actual account resync are still separate implementation work.

## Phase 6a venue and SDK activation plan

- SDK applies resolved `getTickInterval()` after host construction and before
  native start. Send Infinity explicitly as null; do not serialize raw options
  into Orderly's numeric constructor field. Subscribe every Rust venue to server
  multiplier changes, forward Base auth-streak changes, and observe effective
  option changes through the existing flush cadence. Send only changed policy,
  clean listeners on disposal, and preserve errors/startup failure semantics.
  This proves propagation; native classified credential evidence remains 6b.
- Native slice A: Binance/Aster, Bybit, Bitget, BloFin, BingX, Backpack, Bitrue,
  Orderly, Kucoin futures/spot, MEXC, Phemex and WEEX. Replace only recurring
  account timer registration/rearm with the poll API. Preserve actual native
  rate/backoff floors, degraded and multi-tick subcadences, listen-key renewal,
  healing, socket supervision and read deadlines.
- Native slice B: ApeX, Breakout, Coinbase, Crypto.com, GRVT, Hyperliquid,
  Kraken futures/spot, Lighter, OKX, Paradex, Polymarket, Txflow and WOO. Preserve
  urgent post-write repair separately (including Polymarket dirty refresh).
  Hyperliquid's TS HIP-3 position-refresh interval is a real separate safety-net
  floor, not permission to poll every dex at the short Base interval. Inspect TS
  evidence before treating other old native defaults as minimums. OKX valuation
  and GC keep running while periodic network polling is paused. Breakout retains
  a logical registration when initially paused, including public-only accounts.
- Each owner audits exact timer paths/floors before editing and tests actual
  venue request timing with pause/resume, slower/faster policy and completion
  after policy changes. Test high-risk rate/backoff/repair controls explicitly;
  no blind replacement of all `set_timer` calls. Independent source/test review,
  scoped tests/strict Clippy/fmt, then a combined rebuild and actual SDK controls
  precede acceptance. No network-resync completion claim from these timers.

## Phase 4a native account-frame API plan and audit

Root owns this independent core/host slice while venue poll activation proceeds.
`Store::account_snapshot()` selects full private ORDERS/POSITIONS/BALANCE/
SETTINGS/STATUS with the existing position Greeks/prediction extensions. Carry
an unflushed market catalog and its existing extensions before private indexes;
otherwise omit public tables, books, candles and fills. `Engine::account_snapshot_frame`
uses the existing encoder and reusable snapshot buffer without draining dirty
tracking, queued events or ticker/book baselines. Expose it as WASM
`accountSnapshotFrame` and guarded `EngineHost.accountSnapshot()` returning a
frame for the caller to apply/forward explicitly. It does not emit callbacks or
claim network refresh; unsupported older engines fail visibly.

Frame headers contain no blanket snapshot-reset bit; only present sections are
replaced by FrameApplier. Public state therefore survives account-only frames.
Consumers must first accept native catalog readiness; subsequent catalog
updates travel with private reads so changed indexes cannot misattribute rows.
The subsequent SDK slice must synchronize
before private reads and bulk/direct CLI/chaser decisions, retain native close
quantities and never treat this snapshot as successful REST resync.

Verify complete zero-row private sections, retained pending private/public
deltas and queued events, unchanged book baselines, repeatable current balance/
settings, and host trap/dead/dispose boundaries. No raw native JSON readback or
changes to existing position/order unit conversions. Core/host checks and an
independent review precede this API checkpoint; actual SDK consumer proof follows
after policy propagation releases the wrapper file.

### Poll deadline review correction

The unchanged Breakout control caught first-policy delivery postponing an
existing poll when its resolved interval matched the startup fallback. New
mixed-fallback regression proved RED (two replacements instead of one). Keep
the existing physical deadline when the effective cadence, including a real
minimum floor, is unchanged; still record policy for later completion/rearm.
Two new controls cover first delivery and floor-equivalent updates. The
cooldown control also verifies a genuinely different cadence still rearms.
All ten scheduler controls now pass; strict all-feature core Clippy passes.
Independent source review PASS. No ordinary timer behavior changed.

### Account-frame API checkpoint

Implemented the typed native account frame and guarded host API. Independent
review caught the unflushed catalog race; conditional catalog inclusion and a
reordering regression close it without flushing public events. Final core
all-feature checks pass 112 cases (95 unit, ten scheduler, seven shared-feed),
including four new account-read controls. Host package passes 145 cases; selected
Binance WASM host passes 13. Core and selected wasm32 strict Clippy pass; API
source independently reviewed PASS twice. These are core/host evidence, not a
rebuilt combined artifact or actual SDK consumer acceptance yet.

## Phase 4a SDK current-state plan and audit

Use `host.accountSnapshot()` through one synchronous public
`syncAccountState()` seam, applying/forwarding with the existing receiveFrame
path. Before initial native readiness, private async reads await loaded native
data; fail visibly on unsupported/dead/disposed engines rather than answering
with a stale JS mirror. Each orders/positions/balance/position-mode read syncs
the engine state; filtered reads keep their existing shapes. This is an engine
read, not a network resync. Expose the harmless no-op Base seam so direct
CLI/chaser store decisions can synchronize without importing the Rust class.

Default cancel-all must derive its symbols from that native state. Retain
existing venue-global empty-symbol cancellation for WOO, GRVT, KuCoin,
Polymarket and Breakout. Bulk closes will use the same state seam and the
established native-size rules (exact contractsNative, inverse notional divided
by contract size, linear base quantity divided by contract size), not the
published position quantity. Interrupted/unknown writes must not be retried.
Verify before-flush changes, native empties clearing stale mirrors, filters,
frame forwarding without fills/books replay, and wrong-size close regressions.
Network completion/resync and getAccount identity queries remain separate.

### Poll activation checkpoint

All recurring native account loops now use logical configurable polls: 14 paths
in slice A and 13 in slice B, with OKX's non-network valuation/GC left ordinary.
34 new controls cover actual requests, obsolete callbacks, pause/resume,
completion under a changed policy, urgent repair and absolute failure floors.
Final native runs pass 2,688 tests (1,326 A plus 1,362 B); the final A rerun also
passes against the equivalent-deadline core correction. Strict all-target/
all-feature Clippy, bans and scoped fmt pass for all 26 venue crates. Both
native slices received independent cross-reviews PASS.

The SDK sends the resolved interval to every registered Rust name before start,
awaiting a newer policy if it changes during initial delivery. Infinity is
explicit null, duplicate policies are suppressed, failed delivery is retryable,
and disposal removes listeners. SDK checks: 354 wrapper cases, final 56 polling
cases (overlapping the wrapper run except its final added reentry case), 44
additional consumer cases, types and strict scoped lint pass. Independent
source and fixture-preservation review PASS. Actual native credential outcome
feedback remains phase 6b; manually invoking Base auth methods proves only
resolved-policy propagation. No combined-WASM acceptance claimed yet.

### Account reads and observation freshness

Review found that full non-consuming account reads would otherwise manufacture
order update IDs, and then the ordinary flush would count the same native
observation again. That can falsely confirm a chaser's missing order. Add an
optional empty ACCOUNT_READ section to these frames, decoded at every consumer.
FrameApplier passes explicit read-source metadata to Store.update: apply rows
and notify readers without advancing orders/positions observation counters.
Ordinary bulk replacements keep their required counter bumps. Guard recursive
read notification; never restore counters after notifying observers. Tests
cover repeated reads followed by the normal flush and missing-order handling.

Direct consumers synchronize through the Base no-op/Rust native seam before
CLI account-dependent preprocessing, connected command execution and chaser initial position
capture. The structural CLI interface keeps the seam optional for non-SDK
consumers. Failure follows existing visible command error paths; disconnected
session management and ticker-only predicates remain independent of private
snapshots. Lazy account token/loop/condition reads also cover alias/variable
expansion without blocking an unrelated help/disconnect command.

## Phase 5 audited plan

Generic order triggerType is already encoded and decoded but dropped by the
FrameApplier; preserve it beside existing TP/SL aliases. Fills carry native
order identity but omit the event orderId required by chaser attribution; add
that field while retaining existing id. Append optional realizedPnl and fee
columns after the legacy fill prefix, preserving old frame readers and missing
versus zero. Use native typed producer fields on Hyperliquid, MEXC and WEEX;
retain each live event's established fee/PnL convention, including Hyperliquid
close-event net PnL. No new network reads, synthetic financial guesses or fill
count/dedup changes. Verify legacy/new wire decoding, zeros/rebates, exact
native events, worker forwarding and chaser tracking.

Aster already canonicalizes unsupported intervals consistently in fetch,
subscription and provider paths, with an actual-WASM regression. Invalid
numeric steps are already guarded natively. Verify and retain those fixes.
Candle lastSide has no demonstrated repository consumer and native Bybit kline
messages do not supply aggressor side; skip under the production-impact bar
instead of inventing it from candle direction.

## Phase 4d first identity-query batch plan

Implement rare typed getAccount commands for Backpack, BingX, Phemex and WOO
using existing private request/signing/continuation machinery and the selected
read credentials. Preserve TS identity semantics: Backpack /api/v1/account
accountId or userId; BingX USDT balance userId (fallback first row); Phemex
USDT g-accounts/accountPositions account.userID; WOO /v3/accountinfo response
applicationId and referrerID, not the configured WS application ID. User and
affiliate identifiers remain decimal strings without f64 conversion, including
values above 2^53. Required missing identity or public-only use fails visibly;
failed reads never clear account Store domains. No identity derived from
balances, positions or configured unrelated identifiers. Native command handlers
and route/shape/secondary-credential/failure tests precede SDK dispatch wiring
in the later consolidated identity batch. Existing working account commands
and venues with no TS identity API remain unchanged.

First batch implemented, independently reviewed PASS. Final scoped evidence is
393 unique native cases, including 24 new identity controls, strict all-target/
all-feature Clippy, bans and scoped fmt. An old Phemex unsupported-command list
was corrected only for its newly supported FetchAccount case; its other
assertions and full 38-case trading suite pass. Exact decimal identity, read-key
selection, optional fields, errors/duplicate delivery and unchanged account
state are covered. SDK dispatch and standalone JSON decoder exposure remain
consolidated identity work; this is not consumer getAccount acceptance yet.

## Phase 4a bulk-close plan

Rust nuke/nukePositions selects a single current native position snapshot and
submits native-unit reduce-only market closes as one existing placeOrders
command. Reuse the established nativePositionSize policy by moving its
implementation into safe-cex and retaining the script-api-core export; avoid a
reverse package dependency or a divergent sizing copy. Validate all required
market/amount inputs before dispatch. Keep the native batch's accepted IDs,
partial failures and unknown-outcome metadata unchanged. Do not automatically
retry a close against unchanged positions: repeated Base nuke passes can place
duplicate writes before reconciliation. One pass with visible failure meets
the production bar; retry-count lifecycle parity is deliberately skipped.

Nuke still attempts the requested order cancellation after a failed close and
rethrows the original close failure (each failed native command already owns
its visible error); cancellation alone throws its own error. Preserve isNuking
cleanup on every exit and reject a concurrent nuke visibly. Test preflush state,
linear multipliers, inverse notional, exact native sizing extras, zero rows,
missing/invalid input before any write, partial/unknown rejection with no retry,
and cancel-after-close-failure. Standalone CLI and shared sizing checks must
stay green after the implementation move.

## Phase 4d second identity-query batch plan

Binance USD-M uses /fapi/v3/balance and PM /papi/v1/balance (first row
accountAlias); do not substitute USD-M's different reconciliation account route.
Spot and COIN-M remain unsupported for identity as TS. Bybit linear/inverse/
spot/demo uses /v5/user/query-api userID/inviterID; Hyro remains unsupported.
Bitget products use /api/v2/spot/account/info: truthy parentId is the user,
raw userId the subaccount, and inviterId the affiliate. KuCoin futures uses
/api/v1/user-info userId/subUserId/referrerUserId; retain the working Spot route.
Reuse selected-read credentials, passphrase fallbacks and native retry/routing
machinery. Capture IDs as text/u64, including values above 2^53. Normalize
numeric zero optional affiliate/subaccount sentinels to null, while preserving
explicit text identities such as '0'; never turn a falsey sentinel into a truthy
affiliate. Missing required identities reject visibly. No Store/cycle mutation.
Extend the existing FetchAccount JSON decoder universally for standalone
identity-enabled crates, rather than relying on combined Aster/Polymarket
features. Verify each product's route, read-key signatures, nullable/parent
semantics, large IDs, failures before I/O and once-only completion/error.

## Phase 4d third identity-query batch plan

Crypto.com POST private/get-accounts returns result.this_account.uuid, with
affiliate null; use its primary private-POST signing and existing safe read
signature retry policy. GRVT POST /lite/v1/account_summary validates the chosen
trading subaccount, then returns the authenticated session's main account ID,
not trading_account_id. Reuse login deferral/headers and preserve generation
fences, including legitimate header rotation on the current response. Paradex
GET /account returns account_number through the existing JWT queue, with
affiliate null; pending authentication failures must settle the rare read once.
Dedicated identity continuations leave reconciliation flags and account Store
domains unchanged. Verify routes/signing, required/missing/large IDs,
public-only/missing credentials and malformed/error results. GRVT tests separate
main/trading IDs and stale-session responses; Paradex tests expired JWT queue
and refresh. Native batches precede consolidated SDK/standalone integration.

### SDK current-state and read-freshness checkpoint

Current native queries, default cancel-all symbol selection and direct CLI/
chaser decisions now synchronize the engine state. ACCOUNT_READ applies and
forwards rows without manufacturing streaming observation counters. Independent
SDK, CLI/chaser and read-freshness reviews PASS. Checks pass: 501 SDK cases in
32 files (25 new account controls), 545 CLI cases in 15 files (ten new decision
controls), 18 chaser-instance cases, SDK/CLI types and strict scoped lint.
Core/host marker checks and source reviews pass; ordinary Store bulk counters
and venue-global cancel-all paths retain their assertions.

The new actual Binance case proved RED on the previous combined artifact in
exactly marker presence and worker/main freshness, while native ghost clearing
already worked. After the official rebuild all 48 registered actual venue cases
pass, including real worker transfer, native private baseline restoration,
read marker/counters and public/event preservation. Both web/node artifacts are
12,474,948 bytes, SHA256
e5afbf13da1938ee254d045e34609745ec001205f114d1a44152ee06ba128b38.
The artifact also contains the financial/native-first-identity source frozen
for that rebuild; it does not contain subsequent parser or identity batches.
Public fixtures prove the real account ABI and mapping, not funded private
reconciliation or REST resync completion. Bulk closes are the following slice.

### Phase 5 financial/wire checkpoint

Implemented optional financial fill tails without changing legacy row prefixes
or bytes when metadata is absent. Native HL/MEXC/WEEX producers preserve their
established live conventions and execution dedup/counts; historical gross PnL
remains unchanged. Independent review caught HL's old default-zero raw fields
inventing reported zeroes: native presence now remains optional, net PnL needs
both reported values, and fee presence is independent. Generic triggerType and
fill orderId now reach SDK consumers; the actual chaser accepts decoded fills.
Native/host/SDK financial source and fixture reviews PASS after that fix.

Checks: 368 core plus three-venue native cases, including six new functions and
a strengthened history assertion; strict native Clippy, fmt/bans and whitespace
checks pass. All 149 host JS tests pass, including 20 wire cases. Five new SDK
frame/transfer/chaser cases, SDK types and strict scoped lint pass. An initial
SDK fixture used the wrong enum export and exact float equality for chaser's
derived filledSize; these fixtures were corrected without changing production
math. The independent frame fixture also now encodes generic triggerType.
Actual producer-to-SDK acceptance remains part of final combined private gates;
the component evidence is explicitly separate from the 48 public account tests.

## Phase 4d final identity-query batch plan

Lighter/Robinhood resolves an unsigned lookup selector independently of signer,
secret and API-key-index initialization, including public-only use. Preserve
account index zero/large decimal IDs and referrer from the same selected row;
never invoke identity_ready or mutate signer/nonces/financial Store state.
MEXC validates signed GET /api/v1/private/account/assets (valid empty array
allowed), then returns mexc: plus the first 24 lowercase hex SHA256 characters
of the raw PRIMARY options key. WEEX validates signed GET
/capi/v3/account/accountConfig and canTrade:true, then uses the analogous weex:
primary-key hash. Selected read credentials validate the GET; they never replace
the primary identity source. Capture the raw primary fingerprint/public-only
admission before native constructors consume/trim options. Explicit public-only
and invalid selected credentials reject before private validation. Test exact
signing/routing, hash vectors, read-vs-primary attribution, Lighter fallback and
same-row referrals, errors/duplicates and unchanged account snapshots.

## Phase 6b auth foundation plan

Use the refined in-flight context, pure typed outcome hook and scope-isolated
counters in RUST_AUTH_RESULTS.md. Core owns one complete response observation
point across buffered, streamed and missing-list error delivery. Typed host and
native-server notices carry no secret or body. JS validates context and updates
the exact resolved Base interval after per-slot/per-scope evidence. No generic
log/body regex, public-success reset or intermediate retry rejection. Tests
prove all completion/error paths, no context reuse after duplicate/failure,
mixed credentials and partial permissions, callback/dead/dispose fences and
initial/changed/Infinity/server-multiplier policy. The foundation does not claim
all venue classifiers exist; those must be tagged and verified by native slice.

## Phase 4d identity batch 2 checkpoint

Binance USD-M/PM, four Bybit products including demo, three Bitget products,
and Kucoin Futures now have dedicated native identity continuations. They
preserve selected read signing, broker/passphrase/clock retry behavior and exact
numeric IDs without running account reducers. Binance Spot/Coin-M and Hyro keep
their explicit unsupported identity behavior. Empty secondary passphrases reject
before private I/O; omitted passphrases retain native fallback. Both getAccount
and fetchAccount now decode without unrelated feature flags, including a
standalone Backpack WASM build. Independent parser and venue reviews PASS.

605 native tests in 45 nonempty suites PASS (49 including empty suites);
19 final focused controls overlap that total. Strict Clippy, fmt, bans and diff
checks PASS. Default-core command parser and standalone Backpack host alias
controls PASS. Logs: /tmp/bridge-identity-batch2-*.log and
/tmp/bridge-identity-parser-*.log. SDK routing and rebuilt-WASM identity proof
remain pending; these receipts cover native implementation and feature graphs.

## Phase 4d session-backed identity checkpoint

Crypto.com, GRVT and Paradex identity queries reuse their native sign/session
state machines and dedicated read continuations. Crypto.com retains its one
signature resend and safe-read network retries. GRVT fences old generations,
allows legitimate current-session header rotation, and uses the selected/default
trading subaccount before private startup. Paradex retains JWT acquisition,
expiry refresh and delegated/direct signer routing. Login failures settle
identity waiters once; no identity reducer changes financial state.

309 native tests PASS in 22 nonempty suites (25 including empty doctests),
including 26 new controls. Strict Clippy, fmt, bans and whitespace checks PASS.
Independent implementation review PASS. Logs: /tmp/bridge-identity3-*.log.
SDK dispatch and final combined-WASM acceptance remain separate gates.

## Phase 4d final native identity checkpoint

Lighter/Robinhood resolves unsigned account selectors without initializing a
signer, nonce or private account state. Lookup fallback and deployment routing
are shared with startup; index zero and large decimal IDs remain exact, and
referrals come from the selected row. MEXC and WEEX validate with the selected
read credential, then namespace identity from a fixed SHA256 prefix of the raw
primary key. Explicit public-only and invalid selected credentials reject before
validation I/O; WEEX requires typed canTrade:true. Account/financial state and
existing signing/rate behavior stay unchanged.

285 native tests PASS across 26 nonempty suites (29 including empty doctests),
including 24 new controls. Strict Clippy, fmt, bans and diff checks PASS.
Independent correctness review PASS. Logs: /tmp/bridge-identity-final-*.log.
This completes the planned native identity batches, not SDK dispatch or the
final combined-WASM identity acceptance gate.

## Phase 4a native bulk-close checkpoint

Rust nukePositions now reads one native snapshot and validates the full batch
before submitting reduce-only market closes in venue units, preserving hedge
legs and inverse notional sizing. It never retries a close automatically after
partial/unknown outcome. Nuke also attempts native cancel-all after close failure
and rethrows the original close failure. Concurrent nukes reject and ownership
clears on every exit. The existing sizing implementation moved unchanged to a
safe-cex utility leaf; script-api-core re-exports it without a dependency cycle.

33 account-state controls PASS, including eight new nuke controls. Seven shared
sizing-policy tests and 32 CLI preparation/account-state tests PASS. SDK,
script-api-core and CLI-core typechecks PASS. Independent plan and implementation
reviews PASS; strict scoped lint PASS with ignored tests explicitly included. Initial fixture collisions were corrected to represent distinct hedge
legs; no production assertion was weakened. Logs: /tmp/bridge-phase4a-nuke-*.log
and /tmp/bridge-phase4a-sizing-policy-tests.log. This is current engine state,
not proof of network resynchronization or a new unknown-outcome classifier.

## Phase 6b authentication feedback foundation checkpoint

Core captures signer slot and read scope on the existing in-flight continuation.
The pure venue hook observes one complete final response before the unchanged
callback. Missing expected streamed lists cannot report Accepted; invalid UTF-8,
truncated streams, transport/signature failure and untagged requests emit no
auth notice. Notices carry only bounded typed context through WASM and native
account broadcasts. SDK counters isolate all ten slot/scope contexts, aggregate
their maximum under Base's existing cap, and send the resolved polling policy.
Throwing host consumers preserve the action tail and known command outcomes.
NativeFrameSession strictly validates and orders optional account callbacks.

194 core/native cases in nine nonempty suites PASS, including seven new core
completion-path controls and an actual two-account/two-peer broadcast control.
The standalone Backpack host encoder exact-JSON control PASS. Full host153,
SDK102 and native-session129 controls PASS; package types, scoped strict lint,
all-feature core/hosts/venue compilation, strict Clippy, fmt and bans PASS.
Independent Rust and JS/TS correctness reviews PASS. Logs:
/tmp/bridge-auth-{core-focused,foundation-*,host-all,sdk-*,session-*}.log.
These checks prove the foundation, not the remaining venue classifiers or
rebuilt-WASM native credential feedback.

Coinbase and coinbasespot now drop any defined saved read-credential fields and
their proxy from an immutable effective option copy, with one warning before
host startup. Partial/empty saved read fields no longer fail the primary-only
JWT signer; proxy-only settings without read fields retain their old policy.
342 wrapper controls (six new functions including 12 partial/empty variants),
types and strict lint PASS; independent review PASS. Logs:
/tmp/bridge-coinbase-read-options-*.log. Caller/saved options remain unchanged.

## Landing integration c44f22bd8e checkpoint

Merged 61 landing commits, including leverage-side and wire/cancel repairs.
Fourteen content conflicts were resolved by preserving both contracts, followed
by an independent correctness review of all resolutions. setAllLeverage keeps
native position selection and the selected side. Frame application retains
recoverable section handling, native STATUS readiness, shared-feed validation,
post-forward subscription replay and account-read freshness markers. FILLS
revision one keeps raw symbol references, optional aligned financial columns,
and legacy decoder support; no-financial bytes retain the landing layout.

909 unique tests across seven native venue crates/67 nonempty suites PASS;
133 core cases PASS. The wire33, focused account/recovery73, worker26 (complete
JSON receipt), and earlier merged FrameApplier36 controls PASS. SDK types and
strict scoped lint PASS. Native strict Clippy and remaining final acceptance
checks are separately recorded; no full-SDK or rebuilt-WASM pass is claimed here.
Initial checks exposed three unhandled fixture notice matches, four WIP hook
visibility mistakes, financial offsets after the new symbol tail, one pending
catalog market-only fixture, native readiness fixture setup and the contained
snapshot-trap return expectation. Bounded fixes preserve all business assertions
and add explicit symbol/padding/readiness/terminal-state controls. An obsolete
broad SDK run was stopped after its setup failures; targeted complete receipts
replace it for this integration checkpoint, not final acceptance.

Logs: /tmp/bridge-c44-native7-complete.log plus
/tmp/bridge-landing-c44-weex-complete.log; /tmp/bridge-landing-c44-core-tests.log;
/tmp/bridge-landing-wire-merge-final.log; /tmp/bridge-landing-c44-recovery-verified.log;
/tmp/bridge-landing-worker-ack-results.json. Four initial auth hooks were preserved
as explicit WIP before merge; the full classifier migration remains unproved.

## Phase 4d SDK identity dispatch checkpoint

All 48 registered aliases have explicit identity behavior: 33 supported and
15 unsupported. SDK dispatch exposes the native identity continuations without
starting public streams or waiting for unrelated reconciliation; native code
continues to own signing, sessions, typed ID parsing and namespacing. Existing
ApeX/KuCoin Spot aliases, Coinbase portfolio IDs and Orderly configured IDs stay
intact. Kraken Spot's balance command is never treated as identity.

128 focused tests across six files PASS (61 new controls, complete JSON receipt
/tmp/bridge-identity-sdk-final.json), including unchanged existing query paths.
SDK types and strict scoped lint PASS. Independent correctness review PASS.
See RUST_IDENTITY_ROUTING.md for exact alias inventory and receipts. This is
real EngineHost/fake-ABI proof; combined native-WASM identity acceptance remains
part of the final artifact gates.

## User-requested wind-down checkpoint

The overseer requested a stop at the next clean point, without starting queued
follow-ups. No bridge PR exists, and the remaining resync/leverage/credential
acceptance work is substantial. Preserve/push feat/rust-bridge-fixes rather than
open a misleading ready PR. This checkpoint is explicit WIP, not a completed
phase or a final all-gates claim.

Slice A preserves BingX/MEXC/WEEX dispatch tags and typed observers with six new
controls: 288 native cases across 22 nonempty suites plus three empty doctests
PASS; strict Clippy/fmt/bans PASS. Independent classifier review and broader
matrix are still needed. The remaining ten slice-A crates are unimplemented.
See RUST_AUTH_RESULTS_SLICE_A.md for exact anchors and limits.

Slice B preserves provisional observers/tags in eleven classified crates,
including Kraken Spot; unsigned Hyperliquid/TxFlow remain inert. Cargo check and
13-crate all-target/all-feature strict Clippy PASS; scoped formatting and diff
checks PASS. No new auth venue tests, independent implementation review or
combined-WASM proof exist. Documented acceptance tightening is required before
shipping. See RUST_AUTH_RESULTS_SLICE_B.md; compile success is not correctness
acceptance.

SDK identity routing and earlier reviewed components remain committed. Explicit
native network resync, MEXC/BingX/WEEX fresh leverage queries, rebuilt-artifact
proof, final full gates, PR reviews, green CI, merge and fresh branch remain
unfinished. Queued follow-ups c6nf4w-2/-3/-4 were not started. The requested full
handoff is /Users/sam/cs/tealstreet-sdk/.aimux/plans/handoff-codex-c6nf4w.md.
