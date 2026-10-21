# WOO perpetual Rust port

Scope: `woo` only. `woofi` and `kodiakfi` inherit OrderlyExchange and belong
with the Orderly family. Follow VENUE_PORT_BRIEF.md and RUST_REWRITE.md.
Branch `feat/rust-woo`, after MEXC parity PR #1311 merges.

## Phases

1. Public data: borrowed market/ticker/funding schemas, legacy stream,
   buffered timestamp-anchored full depth, candles and public reads, watchdog.
   Use current active TS protocol, not inert `woo.v3.*` migration foundations.
   Native tests and independent phase review before commit.
2. Private data: exact V1/V3 signing; account mode and balances; paged regular
   and algo snapshots; positions/settings; auth and private events; monotonic
   fills, terminal precedence and stale-read guards. Failed reads preserve
   known state. Independent review and native tests before commit.
3. Trading: typed sequenced jobs for ordinary/split/bracket/stop/trailing
   placement, native amend, parent-aware cancel, cancel-all, leverage and mode.
   Complete batch preflight before writes. Review and regression tests.
4. Queries: streamed histories, route-specific paging/time units, V1 ledger
   income and V1 per-order realized PnL; retain WOO's order-based journal fold
   in the shell. Query results never mutate live state. Review and tests.
5. Harness/gates: registrations, exact signed private tape, two-book live
   capture and committed fixture, all brief checks plus Binance regressions,
   CPU/memory/wasm budgets. Review, commit, merge latest landing branch, PR
   into feat/rust-rewrite, adversarial reviews posted, fixes/rereviews, green
   CI, merge and fresh follow-up branch.

## Venue-specific decisions and risks

- Active API is a V1/V3 hybrid. V1 signs sorted form data plus `|timestamp`;
  V3 signs timestamp + method + exact path/query + exact JSON body. Signed
  bytes must be the bytes sent. Reads retry boundedly; ambiguous writes never
  retry. Private WS signs `|timestamp`, uses applicationId in socket URL.
- Public snapshot subscription precedes HTTP. Legacy updates have timestamp,
  not V3 prevTs. Buffer boundedly, discard buffered updates at/before snapshot,
  invalidate generations on reconnect/unsubscribe, validate whole frames.
- Preserve IOC market emulation with bounded slippage/current touch, post-only
  crossing guards, hedge close side, broker IDs, positional paired protections,
  sibling preservation, absolute-distance trailing callback and full-position
  quantity. Native regular amend always sends quantity, including price-only.
- Caller client IDs must be canonical positive decimal safe-integer strings,
  <=9007199254740991, exactly one native plain order, no split/bracket/algo.
  Validate all batch legs and duplicate IDs before writes.
- TS supports cross margin; isolated mutation has no implementation. Opening
  Conditional is not an implemented TS algo type. Record capability limits
  with source evidence; do not confuse sized/full-position TP/SL with opening
  triggers.
- V3 transactionHistory PnL is absent, not zero. V1 ledger supports REALIZED_PNL
  (possibly daily account aggregates with no symbol) and other income kinds.
  Preserve aggregate scope. `/v1/orders?realized_pnl=true` supplies per-order
  PnL, fees and position side. Order request times are milliseconds; response
  created_time/updated_time are fractional seconds. Do not use hist_trades
  (recorded persistent500) or pretend ignored V3 page/fromId parameters page.
- Preserve WOO's order-based historical-position fold; inheriting the generic
  fill-based fold would lose journal rows when fill PnL is absent.
- Genuine TS repairs must be fixture-backed and listed in KNOWN_TS_FIXES and
  the PR. Never weaken gate comparisons or copy Rust output into TS.

## Preparation

Read current TS, cross-venue guidance, official legacy docs and live public
REST payloads. Public info/futures/orderbook routes work without credentials.
Independent inventory confirms the scope and protocol decisions above.
No WOO implementation started before Part 1 merge.

## Public phase

- Implemented borrowed REST seeds, independent ticker topics, deferred funding
  refresh, buffered full depth with timestamp anchor and generations, candle
  reads/streams, subscription reference counts and ping/watchdog recovery.
- Sub-agent review found four defects: delayed snapshots reviving invalidated
  depth, non-ASCII interval panic, early funding request, and premature price-cap
  publication. Fixed all four, with regressions; bounded rereview PASS.
- 15 public tests, native clippy and formatting PASS. Includes 1,501 depth
  levels, bounded buffering, pre-seed subscriptions, stale snapshots, control
  precedence and candle output decoding. Registration remains for gate phase.

## Private, trading and query phases

The three state machines were prepared in disjoint files and integrated
together, so their shared hooks compile without unused placeholder code.
Each received its own independent phase review before this checkpoint.

- Private: staged account/positions/paged ordinary+algo reads, exact V1/V3
  signing and WS auth, hedge settings, monotonic lifecycle/fills, per-execution
  dedup, bounded signature-aware position reconciliation, metadata expiry and
  bounded tombstones. Review fixed failed-page atomicity, equity/uPNL races,
  stale config in positions, object algo children, cache expiry and lifetime
  growth. STOP kind follows closing side/source; buy trailing uses plus distance.
  Independent bounded rereview PASS.
- Trading: complete batch preflight, safe numeric caller IDs, exact splitting,
  protected IOC touch reads, paired/sized/trailing protections, native quantity
  amendments, parent-aware cancel, both-family cancel-all and merged symbol
  cancellation. Config intent guards and authoritative mode/leverage readback.
  Review fixed rounded-zero amendments and deferred trailing replacement sizing.
  Independent bounded rereview PASS.
- Queries: streamed V3 fills and V1 legacy fallback with distinct pagers,
  V1 orders requesting realized_pnl, and paced V1 ledger with aggregate scope.
  Bounded income ranges walk beyond 500 by default. Native queries never write
  the store. Review fixed absent/zero updated_time; rereview PASS.
- WOO-only order-based journal folding is retained in the SDK shell through a
  shared pure mapper. `woofi` and `kodiakfi` remain with the Orderly lane, as
  explicitly confirmed by the overseer.

## Harness work in progress

- Public live capture: 20 seconds, two books. Correctness matches after handling
  a live null index as an absent frame value, rather than rejecting the seed.
- CPU initially failed on the small recording. Numeric visitors avoid generic
  untagged decoding, native ticker indexes avoid repeated symbol allocation and
  lookup, and WOO retains level-3 optimization inside a level-z aggregate.
  Initial reruns were marginal (63.2 ms TS / 64.8 ms Rust overall;
  32.4 / 32.6 ms steady). The next optimization removes redundant REST decoding
  and preallocates atomic WS batches. Acceptance is pending an integrated rerun.
- Eight-venue wasm was 2,741,039 bytes at level-s. Level-z aggregate plus WOO
  level-3 is 2,605,267 bytes, below the unchanged 2,621,440-byte budget.
- Private fixture is being reconciled against actual command stages. Exact
  request bodies/signatures, required write consumption, auth and subscriptions
  remain enforced. TS reliability repairs and final replay gates are pending.

## Integration checkpoint

- Integrated landing `b265a0bab0` (Backpack and BloFin). Preserve all protocol
  guards in the new shared `tape.mjs`, including WOO connection resets and form
  handling, and the generic global-cancel intent bridge. Integrated harness:
  78 tests PASS. Landing now owns a 3 MiB budget; no budget increase by this lane.
- Shared `maxSlippagePct` is optional; every explicit order constructor, including
  new landing venues, defaults to None. No behavior change for those venues.
- Crypto cleanup: deleted unused HMAC-SHA512. Retained SHA256 after landing
  introduced a Backpack caller; no unused WOO replacement helpers retained.
- WOO native phase coverage: 2 signing + 18 public + 26 account + 22 trading +
  12 query tests. Public batch regressions retain atomic rejection and work past
  the allocation capacity hint. Complete workspace checks are running.
- Independent gate audit fixed sorted V1 parameter validation and server-ping
  replies. Final fixture: 129 records, 18 commands, 22 seconds, 53 replay frames.
  Includes opening entry with attached SL+TP, sized STOP inference, positional
  sibling preservation, trailing, mode/leverage, and both V1 PnL/income axes.
- Independent SDK audit fixed order-axis source provenance, implicit public-only
  metadata reads, and JSON-aware exact integer decoding that preserves quoted
  venue error messages. The original regex helper corrupts numbers inside text.
- Root typecheck 44 tasks and lint 28 tasks passed before integration. Full tests
  and the integrated release artifact are running; final acceptance remains open.

## Final landing integration and size checkpoint

- Integrated Bitget landing `4e468ed2e5` and its completion note `3a7b46407d`.
  The new shared `createTape` retains WOO HMAC validation, socket generation
  resets, typed subscription keys, strict staged snapshots and one-use writes.
  Independent integration review PASS; 118 harness tests PASS.
- Final healthy private fixture: 132 records, 18 commands, 56 replay frames,
  22 seconds. Full terminal algo leaves remain decodable after the server's
  empty snapshot; public server pings require audited pong replies. Independent
  tail review PASS. WOO/Binance required private/public gates passed before this
  final size tuning. No output comparison or performance threshold changed.
- Full repository validation PASS: 44 typecheck tasks, 28 lint tasks, 27 test
  tasks; safe-cex runs both test entrypoints with 6594 tests each. Three initial
  unrelated timeout/in-flight integration failures passed isolated reruns and
  then the complete clean rerun. Latest wasm SDK suite: 27 tests PASS.
- Direct scalar visitors remove duplicate untagged ID/maker representations.
  Required/optional IDs preserve exact u64, null/empty and type-rejection rules;
  independent bounded review PASS, WOO native tests now 84. Workspace native
  baseline was 747 before four new WOO regressions; final rerun in progress.
- Eleven-venue wasm initially 3,782,678 bytes. WOO-wide size optimization hurt
  steady CPU and still missed the cap. Keep WOO, Bitget and engine at O3; use
  compact host dispatch and global wasm level-z (native release unchanged).
  Current artifact: 3,642,686 < inherited 3,670,016 bytes. No lane budget increase.
- The latest shared harness removes enough overhead that WOO steady CPU is
  marginal (21.1 ms TS / 21.7 Rust). A one-pass typed ticker envelope is being
  implemented to remove repeated RawValue scanning before final acceptance.
  Other venue regressions are replaying on this size-compliant artifact.

## Per-venue build integration (final architecture)

Landing `c5bc2c4adf` introduced venue-feature wasm bundles and a per-venue
1,258,291-byte cap. Integrated that structure and discarded all aggregate
compiler-profile experiments. No shared release profile change remains.
WOO is an optional/default host feature, build allowlist entry and CI matrix row.
Independent integration review PASS; 152 strict harness tests PASS.

- WOO single-venue wasm: 813443 bytes < 1258291.
- WOO public: PASS, 77 frames, 222 markets/tickers, two books; CPU 71.1 →
  59.4 ms, steady 34.7 → 27.1 ms, retained 1.6 → 1.1 MB.
- WOO private: PASS, 56 frames, 18 commands, 132 fixture records. Exact bodies,
  HMAC/auth, socket generations, ping replies, request/write consumption,
  account state, fills and query results all match. Informational private CPU
  116.5 → 89.7 ms, retained 4.1 → 0.9 MB.
- Public ticker envelopes decode once. Bitwise no-op guards avoid the store's
  full-field scan while retaining signed-zero updates. Known ticker fallback
  decoding became provably unreachable and was deleted. Independent review
  PASS; WOO native tests now 89, complete workspace 823 PASS.
- Kept shared exact-ID parsing for queries; compact original scalar enum
  decoding was smaller than new ID/maker visitors. All boundary regressions
  remain. New Hyperliquid-only remote client lookup query rejects explicitly
  on WOO; its SDK caller-ID lookup retains the existing local contract.
- Bans, formatting, workspace native clippy, root typecheck (44 tasks) and lint
  (28 tasks) PASS. Final combined SDK tests, Binance feature-bundle regressions,
  PR adversarial reviews and CI remain before landing.

## Ready for PR

All brief gates pass on `3511c66f44` plus this evidence commit. The combined
wasm SDK suite passes 27 tests and Workers host passes 25. Binance feature
bundle public replay passes 873 frames, 917 markets, 783 tickers, three books:
CPU 244.5 → 126.2 ms, steady 140.1 → 56.5 ms, retained 4.3 → 2.4 MB. Binance
private passes 14 frames. WOO/combined/Binance builds all succeed. No performance
normalizer or threshold was changed. Proceed to PR into feat/rust-rewrite,
adversarial local reviews posted on the PR, fixes/rereviews and green CI.

## Resumed PR adversarial fixes

Fetched and merged origin/feat/rust-rewrite after quota restoration: already up
to date. Checkpointed interrupted query work as eed72aef0f; no stash used.
Posted ten concrete adversarial findings on PR #1316. Although the original
PR head has green CI, landing waits for these fixes and renewed verification.

Remaining phases:
1. Private/trading: preserve reconciled execution quantities, camel aliases,
   atomic pagination validation and independent cancel-family attempts.
2. Queries: TS-compatible seconds and transaction symbol aliases, shell/native
   regressions; keep the native millisecond contract.
3. Oracle/harness: nested algo fills, terminal parents, atomic malformed-leaf
   validation, persistent request-validator failures and private tape cases.
4. Review each phase with sub-agents, rerun brief gates and root checks, commit
   and push, post adversarial re-review findings, merge only green CI, then cut
   a fresh branch from feat/rust-rewrite.

### PR fix checkpoints

- Committed cancel-family independence as d3cb8a23f4. Both global families are
  attempted once after HTTP/transport/nested business failures; final errors
  identify failed families and release the FIFO. Independent review PASS;
  24 trading tests PASS.
- TS oracle repairs now handle nested child executions and dedup, minimal
  terminal parents, malformed executable leaves, and reconciled optimistic
  quantities. Re-review caught zero-delta fills clearing a newer position guard;
  fixed while preserving refresh scheduling. Expanded tape caught synthetic
  flat margin surviving authoritative/optimistic reopen; explicitly cleared in
  both mappers. Independent TS/harness review PASS; 28 focused tests PASS.
- Request-validator exceptions persist even when the app catches a failed read
  and retries successfully. Independent public/private negative proof PASS;
  153 harness tests PASS. No signature, body, state or performance relaxation.
- Expanded private tape passes: 58 frames, 18 commands, 134 fixture records,
  nested parent-NEW child-FILLED plus duplicate, new camel-only order, chart
  query and native response aliases. Informational CPU 142.9 -> 82.8 ms,
  retained 3.8 -> 0.9 MB. Public 77 frames/222 markets/tickers/two books PASS:
  CPU 72.6 -> 56.4 ms, steady 35.7 -> 24.2 ms, retained 1.6 -> 1.1 MB.
- Root checks: typecheck 44, lint 28, complete tests 27 tasks PASS (6667 safe-cex
  tests in both entrypoints). Later focused guard/reopen regressions pass too.
  Native account auxiliary classification and legacy query follow-ups remain
  in review before final artifact/gate/CI reruns.

### Query parity follow-ups reviewed

- Native queries now try TS's four symbol spellings plus unfiltered V3 before
  the matching legacy sequence, and decode top-level/nested rows, optional
  execution IDs and legacy aggregate fields. First legacy page omits `page`;
  later native pagination remains explicit. New V3 windows reset page state.
- Independent review caught page state crossing windows and then raw-ID versus
  trade-ID precedence in the order fallback. Fixed both; fill and order IDs
  now preserve TS's distinct nullish precedence, including explicit empty IDs.
  Final bounded re-review PASS, 26 history tests and all-target WOO clippy PASS.
- SDK seconds/milliseconds bounds and chart/response aliases retain active TS
  behavior, with eight wrapper cases and native normalization regressions.
- Tape now adds alternate-symbol recovery and id-less legacy aggregation:
  144 records / 20 commands. Final TS replay's first attempt failed the shared
  backing-buffer settling check under concurrent full tests; rerun the same
  unchanged memory gate after builds/tests settle. No gate threshold changed.

### Final fixture and verification audit

Independent fixture review confirmed the added alternate/legacy route cases.
Corrected the synthetic top-level replies to include documented `success:true`
and V1 page metadata proving completion; strict native envelope/pager validation
was retained. Final fixture review PASS. Native workspace now passes 843 tests
(WOO 109), plus bans/fmt/clippy and wasm clippy. Combined SDK 27 and host 25 PASS.
The latest full root test attempt hit host fork EAGAIN and unrelated Hyro import
timeouts under concurrent lane runs. Retrying unchanged coverage with two Vitest
workers and one Turbo task at a time; no timeout or assertion was relaxed.

## Latest landing transport integration

Fetched fc18c55874 (transport #1313 and Bitrue #1314) before pushing. Resolved
additive registration/feature/test conflicts keeping both venues and landing
transport policy. The landing already persists validator exceptions, so retained
its equivalent try/catch implementation and WOO's regression. Harness159 PASS;
focused wrapper/routing203 PASS. The refreshed brief requires explicit socket
roles; WOO public/private tagging and initial/reconnect regressions are in progress.
Pre-integration final WOO gates passed: 60 private frames/20 commands/144 records,
public CPU73.6 ->59.1 ms, steady35.3 ->27.9 ms, retained1.6 ->1.1 MB, wasm834024.
Stopped the superseded root test run before mixed-version results could be used;
rebuild and rerun integration gates after the merge and socket-role update.

Socket-role update passes 110 WOO tests and clippy; independent integration
review PASS for registration/options, routing authorization and exact signed
bytes, channel/context propagation, validators, dead crypto cleanup, production
and staging initial/reconnect roles. Host34 and harness159 PASS. Full combined
build found Bitrue's newly landed explicit PlaceOrderOpts initializer missing
WOO's shared optional max_slippage_pct; initialized it to None without changing
Bitrue trading behavior. Rerun all combined/native gates after this compile fix.

## Final post-transport gate evidence

All venue brief gates pass on 32bc472d0d. Independent phase, adversarial and
integration re-reviews PASS; original findings and follow-ups are resolved.

- WOO110 / complete native workspace930 tests PASS; bans, all fmt, workspace
  all-target clippy and wasm clippy PASS.
- Harness159, combined WASM SDK31 and host34 tests PASS. Root typecheck44 and
  lint28 tasks PASS. Full safe-cex test-ci6889 PASS. The duplicate local
  test-unit invocation hit one unrelated Hyro5s timeout under shared load;
  its complete10-test suite passes isolated on Node22. Require complete fresh
  PR CI to be green before merge.
- WOO public77 frames,222 markets/tickers,two books PASS: CPU65.2 ->54.9 ms,
  steady30.2 ->25.1 ms, retained1.6 ->1.0 MB, WASM linear0.7 MB.
- WOO private60 frames /20 commands /144 records PASS: exact bodies/signatures,
  socket roles/generations/heartbeats, account/fills/queries and write/request
  consumption. Informational CPU119.0 ->84.9 ms, retained4.1 ->0.7 MB.
- WOO single-feature WASM835126 <1258291 bytes.
- Binance public873 frames,917 markets,783 tickers,three books PASS: CPU260.1
  ->124.6 ms, steady149.6 ->48.7 ms, retained4.2 ->2.2 MB. Private14 frames PASS.
- Memory investigation found no constructor timers/network: exact baseline
  settled in410ms with unchanged1s deadline. Node22 unchanged gate rerun PASS;
  no measurement, normalization or performance thresholds changed.

Push the feature branch, refresh PR#1316 evidence/re-review comments, wait for
all CI on that exact head, merge into feat/rust-rewrite, and cut a fresh branch.

### Port-review lows plan (2026-10-21)

1. Reverify native algo reduce-only defaults, trigger fallback and missing-
   reference row handling; preserve intentional native semantics and record
   unverified changes as risks.
2. Clarify leverage write-accepted/readback-unconfirmed failures without removing
   bounded confirmation or reporting an unconfirmed setting as successful.
3. Warn once per symbol when isolated exposure is observed while mutations are
   unsupported, keeping the position visible. Cover REST and stream paths.
4. Reproduce behavior gaps, run focused WOO/Binance tests and fmt/strict Clippy/
   bans with CARGO_BUILD_JOBS=4; independently review, push for CI and merge only
   after overseer's exclusive1567 slot ends.

- Reproduced missing isolated warning and misleading accepted-write readback error
  before changes. A valid committed isolated position now warns once per symbol
  using config.invalid while remaining visible. The native private stream carries
  executions, not an independently mapped positions topic; existing fill-triggered
  REST repair shares the same commit path. No unverified stream schema is added.
- Leverage confirmation still retries at most three GETs. Exhaustion after an
  accepted write says accepted/unconfirmed with Unknown outcome, preserves the
  prior cached setting and never sends another POST. Plain reads retain their
  existing failure wording; no unconfirmed success is invented.
- Algo reduceOnly=false matches the native legacy contract:
  https://docs.woox.io/products/woo-x (parent and child defaults).
  Standalone SL fallback matches TS, and STOP without reference already isolates
  its row and re-reads after prices exist. Attached generic-trigger semantics
  and guessed SL/TP classification remain explicitly risky; no write semantics
  are broadened on the legacy surface.

- Validation: both new regressions failed before the repair; 163 WOO plus 246
  Binance tests pass, touched formatting, strict Clippy and bans pass. Independent
  read-only review found no blocker; its stale read-retry comment was corrected.
