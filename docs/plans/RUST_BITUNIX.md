# Bitunix Rust futures port

Branch: `feat/rust-bitunix`; target: `feat/rust-rewrite`.
The venue brief and `RUST_REWRITE.md` are the acceptance contract. Full auto;
review each phase with a sub-agent. Never touch master, rebase, or force-push.

## Phases

1. Public data: borrowed wire records, streamed market/ticker startup,
   sharded ticker/price sockets, legacy full snapshot books and candles,
   ping/watchdog/reconnect state. Register crate and wasm factory. Verify
   bans, fmt, clippy and documented-payload public tests, then agent audit.
2. Private data: double SHA256 signing, private login, account object/array
   handling, position aliases, settings, debounced reads and tick resync.
   Guard snapshots by generation so in-flight reads cannot undo events.
   `realizedPNL` is already net of fees AND funding: carry verbatim.
   Verify scripted startup, rejection, race and reconnect tests; agent audit.
3. Trading: sequenced jobs for place/batch/brackets/TP-SL, native amend,
   cancel/cancel-all and account settings. Caller IDs must survive exactly
   once or reject the whole batch before writes; never retry unknown writes.
   Preserve sibling protections and raw identities. Verify payloads, batch
   pacing, failure and ID contracts; agent audit.
4. Queries: fills, order history, income, public trades and candle reads in
   engine JSON shapes; time-window paging/dedupe and fee conventions.
   Verify documented payloads and page boundaries; agent audit.
5. Harness/gates: venue adapter, deterministic private tape, live 20-second
   two-book public recording and gzipped fixture; compare TS/Rust including
   outbound requests. List intentional TS bug fixes explicitly. Run all
   brief gates, Binance regressions and repository checks; agent audit.
6. Integrate: merge origin/feat/rust-rewrite into lane, rerun gates affected
   by integration, push, PR into feat/rust-rewrite, adversarial local agent
   reviews posted as PR comments, fix/repeat, merge PR, fresh follow-up branch.

## Audit notes

- Legacy `depthop` is a full book, not a diff sequence; replace both sides.
- Futures socket binary frames are gzip; host must deliver decoded text once.
- Public sockets share URLs: replay must distinguish connection instances.
- TS order reconciliation returns empty on failures; Rust must preserve state
  and document that correction. Streamed envelopes need success validation.
- TS private orders lack per-execution notifications; investigate cumulative
  fill quantities and record any intentional improvement as a known TS fix.
- Read contracts first; do not transliterate the TS class structure.

## Progress

- Planning: repository/engine/venue contracts inspected; official signing docs
  checked. No implementation yet. User's revised process authorizes integration
  after all gates pass and overrides initial never-push/merge instruction.

### Phase 1 review and verification

- Implemented crate and wasm registration, streamed seeds, shard pacing,
  snapshot books, candles and socket watchdogs; gzip frames decoded in hosts.
- Reviewer `public_review` found and prompted fixes for early stream startup,
  duplicate-socket retries, unchecked envelope codes, candle request options,
  native binary support, JS close ordering, malformed streamed arrays and
  gzip truncation. Added opt-in bounded `FieldEnvelope` streaming mode.
- Verification: bans PASS; venue clippy all targets PASS; 7 public tests +
  10 engine tests and doc tests PASS. Browser host ordering test added.
- Environment: corepack is in Volta node 24.21.0 bin; prepend it to PATH.
- Full venue gates remain pending phases 2–5. No gate numbers claimed yet.


### Workspace nesting checkpoint

- Merged `origin/feat/rust-rewrite` at b32847fca6 in b218c4f417; adopted
  automatic `venues/*`, engine/crypto and hosts paths. Preserved phase-2 WIP
  using named stash ef104f904eab6a3fb85ccf179e3a8e4ba5e39ce3 and restored it.
- Phase-1 new host test PASS. Full JS host suite waits for wasm build: existing
  Workers test cannot load the not-yet-generated bundle.
- Crypto audit `public_review`: wallet_utils and zklink/contract have no
  external callers or tests. Removing these first-port facades, keeping
  signing primitives and their independent vectors.
- Phase-2 review `private_review` found alias/null handling, snapshot ordering,
  cached terminal resurrection, parent/virtual-leg corruption, private silent
  heartbeat, wrapped balance extraction, cumulative fills, settings and WS
  signing whitespace issues. Fixes and regressions in progress; not ready.

### Phase 2 checkpoint

- Implemented double-SHA256 REST/WS auth, account object/array/wrapper reads,
  streamed position/order wrappers, settings and position identities, regular
  and compound TP/SL mapping, startup/tick/debounce reconciliation, stale read
  serials/generations and terminal guards, cumulative execution deduplication.
- `realizedPNL` is carried verbatim, net of fees and funding; regression pins
  -3.5 with independent fee/funding fields. TS missing live fills and failed
  reconciliation clearing state are intentional corrections to list in the
  harness allowlist during phase 5, and in the final change report.
- Review fixes include REST advancement of execution baseline, rejecting
  identity-free order rows, updating native TP/SL protection metadata and
  ignoring delayed updates after terminal events.
- Verification: venue clippy PASS; 1 signing + 7 account + 7 public tests and
  10 engine tests PASS; crypto suite 28 unit + 1 independent integration PASS.
  Added another malformed-order snapshot regression before final phase run.
- Remaining phases: trading, queries, harness/gates and integration.

### Phase 3 implementation/review checkpoint

- Sequenced write jobs implement placement, max-five symbol batches paced one
  second apart, full/partial TP/SL, native amendments, cancellations and account
  settings. Whole-batch caller-ID preflight is fail-closed; native amend keeps
  client IDs. Creating/canceling state and failure cleanup are implemented.
- Reviewer `trading_review` found and prompted fixes for unsafe split recovery,
  missing per-item cancellation acknowledgement, wrong standalone target leg,
  premature LIMIT fallback rejection, restore-state gaps and preparation hangs.
- Cancel/recreate now reads the final canceled order detail before sizing the
  replacement, so a fill racing an amend/cancel cannot reopen excess quantity.
  This intentionally fixes TS's original-size replacement and unchecked cancel
  results. Add both to the known-TS-fix gate entry in phase 5.
- 10 trading regressions PASS; additional cancel/fill-race cases added. Clippy
  --fix resolved three collapsible-if warnings; rerun plain clippy before commit.
- Queries will keep TS's unsupported public-trades error: Bitunix TS has no
  REST public-trades implementation; do not invent an endpoint.

- Phase-3 final verification: plain clippy all targets PASS; 15 trading,
  8 account, 7 public and 1 signing tests PASS. Added regressions for strict
  final-fill recovery and preserving job-owned optimistic rows during reads.
  All review blockers addressed; known TS corrections listed in gate.mjs and
  commit message. Tape-derived normalization remains a phase-5 task.

### Phase 4 checkpoint

- Added streamed fills, historical-order and income queries in engine JSON
  shapes. Orders page on overlapping oldest ctime (skip is ignored by venue);
  income pages with skip. Raw IDs drive progress; failed pages are discarded,
  preserving only validated prior income pages. TS public trades are unsupported.
- `public_review` found unvalidated-page leakage, timestamp/fee/wrapper aliases,
  symbol canonicalization and empty-ID paging progress; fixes and regressions
  added. History execution IDs now preserve distinct identical fills, correcting
  TS composite-key dedupe; listed in KNOWN_TS_FIXES and phase commit message.

- Phase-4 final: reviewer found no remaining blockers. Plain clippy PASS;
  all 38 venue tests PASS (including 7 history tests). Remaining work is
  harness integration, live public recording, replay/performance gates,
  repository checks, and the authorized PR/review/merge process.

### Phase 5 host/replay checkpoint (gates in progress)

- Private parity passed with signed requests, caller-ID single/batch placement,
  native amend, cancellations, account settings, net realized PnL, history and
  candle commands. Live public tape includes all public shards and two books.
- Harness audit prompted per-connection routing/subscription validation, every
  candle update and book-flush digest comparison, every-run validation, no
  future HTTP snapshot fallback and JS ArrayBuffer memory accounting. Recorder
  now captures local socket closes and explicit startup cohorts. Reconnect tape
  verification and final public CPU/memory gates remain in progress.
- Browser/Worker host audit fixed binary close/error/disposal ordering. JS host
  tests: 12 PASS. Native gzip/zlib/plain decoding tests: 3 PASS; native loop test
  PASS. Removed the unused Bitunix chrono dependency; strict bounded ISO parser
  tests and malformed UTF8 candle interval regression pass.
- Root typecheck: 44 tasks PASS; lint: 28 tasks PASS. Root tests found a preexisting
  Rust wasm test under the runtime tree (6,488 other safe-cex tests passed).
  Moved it into __tests__/rust; boundary regression and 4 wasm tests now PASS.
- Public replay found tick precision powf rounding noise, corrected by integer
  decimal scaling. Public CPU and retained/peak memory still require a final
  passing run before integration. No all-gates-passed claim yet.

- Private tape now exercises remote close/reconnect/login and distinct identical
  historical fills; 42 delivered lifecycle/message/command records PASS. Venue
  suite now 46 tests PASS; bans/fmt/venue+native+wasm clippy PASS. Root tests rerun:
  all 27 tasks PASS. Binance public PASS; expanded Binance private metadata
  comparison needs precise large-ID normalization before its final rerun.
- Corrected startup measurement by yielding before the baseline GC, so transient
  wasm-loader buffers cannot subtract from retained memory. This exposed a real
  public memory loss (Rust 2.7 MB vs TS 2.0 MB); investigating first-frame buffer
  growth, not declaring the performance gate satisfied. Preallocating table and
  string capacity and moving immutable routing metadata outside timed playback.

- Engine reductions: exact full-table frame reservation; market identity strings
  bypass the dedup index (quotes still share); ticker lookup uses market-indexed
  slots, retaining a fallback only for unknown markets. Engine 16 tests PASS,
  including duplicate/reordered markets and decoded repeated strings. Public
  socket shards hold market index ranges, not three copies of symbol strings.
- Review corrected constructor-baseline asymmetry and retained TS consumer books.
  A three-run public replay passed (CPU 785.7/760.7 ms TS/Rust, retained 2.3/2.0 MB,
  sampled peak 30.0/29.5 MB), but lifecycle/bootstrap review then required removing
  readiness before playback. That fairer timing exposes another small CPU loss;
  removing a redundant public JSON data scan is the next concrete optimization.
- Recorder excludes shutdown disposal and distinguishes per-URL startup cohorts;
  playback drives recorded opens/closes inline, waits only for the current
  subscribed topic and checks active subscriptions at completion. No V8 tiering
  flags were added. GC/ArrayBuffer deallocation must settle before retained
  measurement, as startup buffers can otherwise appear retained spuriously.

### Phase 5 local gates PASS (before landing-branch integration)

- Fresh 20-second live two-book fixture: 28,694 delivered records, 697 markets
  and tickers, two books. Three-run medians TS/Rust: main CPU 560.4/456.2 ms;
  steady CPU 321.8/274.3 ms; retained 2.3/2.1 MB; sampled peak 29.7/29.2 MB.
  Bitunix private: 42 lifecycle/message/command records PASS.
- Binance public three-run medians TS/Rust: CPU 282.6/156.6 ms, retained
  3.2/2.3 MB, sampled peak 19.0/11.5 MB; private 14 records PASS.
- Bans, venue fmt, venue/engine/native all-target clippy and wasm-target host
  clippy PASS. Venue 48, engine 16, JS host 12, native 4, crypto 29 tests PASS.
  Root typecheck 44 tasks, lint 28 tasks, tests 27 tasks PASS.
- Single-pass borrowed public envelope decoding removes the redundant raw-data
  scan while preserving arbitrary-field-order fallback, duplicate rejection,
  and private/legacy raw payloads. Reviewer found no concrete blockers.
- Harness follows Bitunix lifecycle records inline and waits for the current
  topic rather than blocking reconnects behind global startup readiness. Legacy
  Binance startup normalization remains. Both implementations include constructor
  allocations and retained consumer books. GC and backing-buffer cleanup settle
  before baseline/final measurement; no tiering flags or budget exceptions.
- Next: merge latest landing branch (currently Bybit + OKX), preserve its generic
  harness capabilities, rerun integrated gates, push and open/review PR. Merge
  only after PR CI is green; then cut the requested fresh follow-up branch.

### Landing integration checkpoint

- Merged landing at 693ad4f139 (Bybit + OKX), resolving shared files with separate
  engine/harness/host reviews. Kept canonical EnvelopeField/Path plus Bitunix
  AnyFieldEnvelope validation; kept every venue factory/alias and one canonical
  wasm test location. Added Bitunix public/private gates to Rust CI.
- Preserved upstream OKX login/topic/ping validation, venue options and TS oracle
  configuration, response headers/chunking, command-stage snapshots and required
  write consumption alongside Bitunix connection identities and timed snapshots.
- Clarified client-ID comparisons: TS stores native IDs in order.info. The
  Bitunix gate projects those into the same typed fields; this is not a claimed
  TS client-ID bug. Other ports keep their prior account comparison contract.
- Integrated root typecheck 44 tasks and lint 28 tasks PASS. First root tests hit
  an unrelated WEEX reconnect timeout; its 11 targeted tests and full 27-task
  retry PASS. Workspace Rust checks/build and integrated replays in progress.

### Integrated core checks PASS

- Full workspace bans/fmt/clippy/tests and wasm clippy/build PASS after the merge.
  Adapter tests 16 and JS host tests 12 PASS after reifying Yarn workspace links;
  the old local symlink had loaded the pre-nesting bundle.
- Bitunix integrated three-run public TS/Rust medians: CPU 429.9/333.6 ms,
  steady 260.5/198.9 ms, retained 2.5/2.2 MB, sampled peak 29.7/29.2 MB. Private
  42 records PASS. Binance public CPU 254.3/143.0 ms, retained 3.2/2.3 MB, peak
  19.0/10.9 MB; private PASS.
- Expanded subscription checks revealed the old Bybit replay stopped while its
  paced subscription queues were still draining. Both implementations now wait
  for the expected taped startup topics. Three-run Bybit public PASS: CPU
  495.4/403.6 ms, retained 3.1/1.8 MB, sampled peak 30.2/10.6 MB.
- Four-factory aggregate wasm is 1,816,352 bytes. Set CI's bundle cap to 1.8 MiB;
  CPU and memory acceptance thresholds remain unchanged. Remaining checks: Bybit
  private, OKX public/all private aliases, then PR adversarial review + green CI.

### Second landing integration checkpoint

- Bybit private PASS; the subscription oracle explicitly requires Rust's existing
  execution/wallet additions on each private connection, rather than ignoring
  subscription differences. Exact OKX arg lookup removes repeated full-market
  subscription scans while preserving row-scope validation. One-run OKX public
  PASS: CPU 503.9/413.9 ms, retained 2.4/2.3 MB, peak 31.3/16.7 MB (TS/Rust).
- Fetch found Crypto.com newly landed at e6e410146e. Commit this checkpoint before
  merging it; preserve the new response/protocol audits and worker transport
  measurement. Rerun integrated checks before publishing the Bitunix PR.

- Second integration audits preserve all five factories/aliases and both
  streaming validation paths. Updated the old malformed-item callback assertion
  for the landing branch's stricter item validation. Fixed shared adapter's
  missing Crypto.com client-ID rule to match its TS venue exactly.
- New public-worker transport, stable backing-buffer collection, signed-POST read
  classification and Crypto.com socket audits must survive harness resolution;
  integrated gate measurements below supersede the four-venue checkpoint.
- Crypto.com integration verified: full workspace bans/fmt/clippy/tests, wasm
  clippy/build, root typecheck 44/lint 28/tests 27 tasks, adapter 16 and host 12
  tests PASS. Harness helpers 12 PASS. Aggregate wasm 2,058,973 bytes.
- Harness review retained lifecycle IDs, exact writes/caller IDs, per-frame
  validation and independent oracles; adopted worker/mainStore transport,
  signed-POST read classification and stable buffer metrics. Final forced GC is
  outside both CPU windows. Timed snapshots remain ineligible until their time.
- WEEX landed at 4046db3826 while this merge ran. Integrate it before the final
  combined replay run and PR, avoiding repeat measurements on a superseded build.

### WEEX landing integration checkpoint

- Merging landing 4046db3826, preserving its upgrade-header/disposal guards,
  strict streaming JSON, WEEX protocol/oracle checks and bounded OKX startup
  subscriptions. Native reviewers own disjoint engine, harness and host merge
  scopes. Final integrated checks use the resulting six-venue artifact.
- Adapter audit also found WEEX's registered Rust venue lacked its client-ID
  helper rule; copied the existing TS venue's validation and mint contract.
- Six-venue core checks PASS: workspace bans/fmt/clippy/tests, wasm clippy/build,
  root typecheck 44/lint 28/test 27 tasks, adapter 17 and JS host 20 tests.
  Engine 31 tests and harness 19 tests PASS. Binary 2,518,643 bytes; aggregate
  cap 2.5 MiB. Performance thresholds unchanged.
- Host review additionally fixed disposed-engine calls from delayed HTTP/timer
  callbacks, retaining ordered binary decoding and fail-closed upgrade headers.
  Final combined public/private replays are next, then PR review and green CI.
- Strict component checks caught a Bybit Rust negative JS heap delta even though
  correctness/CPU/total-memory passed. Diagnosis: one concatenated canonical URL
  per replay frame flattened during comparisons, releasing harness-only ropes
  after baseline. Canonicalize/share each immutable raw URL once before either
  baseline. Three fresh Rust probes retain +796–806 KB, with all non-metric
  outputs identical. GC counts and thresholds unchanged; helper 19 tests PASS.
- Actual merged landing parent is 6cd219ce4c (WEEX plus status-only landing
  updates), already containing 4046db3826. Bitunix private 42 records PASS.

### Final integrated local gates PASS

| Public tape (3 runs) | CPU TS / Rust ms | Retained TS / Rust MB | Peak TS / Rust MB |
| --- | ---: | ---: | ---: |
| bitunix | 318.8 / 274.7 | 3.5 / 2.2 | 31.1 / 15.9 |
| binance | 197.9 / 97.5 | 4.6 / 2.4 | 34.5 / 12.4 |
| bybit | 199.3 / 139.2 | 6.5 / 2.6 | 33.6 / 10.6 |
| okx | 204.2 / 137.6 | 4.7 / 3.0 | 31.5 / 10.4 |
| cryptocom | 65.1 / 61.5 | 2.1 / 1.5 | 14.5 / 5.4 |
| weex | 234.2 / 160.9 | 5.3 / 2.7 | 20.7 / 7.2 |

- All public outputs match, with Rust main-thread CPU, retained memory and sampled
  peak below TS. Private correctness PASS: Bitunix 42, Binance 14, Bybit 18,
  Crypto.com 21, WEEX 20, and each of four OKX aliases 29 records.
- Crypto.com public-only oracle now waits for its first socket open before
  subscribing; sending during CONNECTING had induced a premature reconnect.
  New regression plus helper suite 20 tests PASS. Other gate thresholds and
  protocol checks remain unchanged.
- Core validation remains PASS: workspace bans/fmt/clippy/tests, wasm build/lint,
  root typecheck/lint/tests, adapter 17/host 20 tests; bundle 2,518,643 bytes.
  Next: push, PR into feat/rust-rewrite, adversarial reviews, then green CI and
  authorized PR merge/fresh follow-up branch.

### PR #1309 adversarial review and fixes

- Posted three independent reviews on PR #1309. Shared engine/harness review
  PASS (31 engine/20 helper tests); coverage limits are recorded on the PR.
- Fixed public empty snapshots: initialized books emit zero-level BOOK sections,
  while new/reconnected subscriptions remain unavailable until synchronized.
  Regression failed before fix; public 11 tests PASS; second public review PASS.
- Fixed three confirmed trading races: coalesce full-position TP/SL sibling
  requests by position (last requested leg wins), fence positions/balance reads
  after acknowledged settings changes, and preserve terminal rows when a cancel
  ACK arrives. These intentionally correct TS preformatted sibling writes and
  stale-setting/cancel-terminal behavior; listed in KNOWN_TS_FIXES.
- Seven new trading regressions, full venue 56 tests, fmt/clippy PASS. Independent
  reviewer reran original probes plus repeated-leg/multiple-position grouping:
  all PASS, no further findings. PNL remains verbatim net fees/funding.
- BingX landed at 022ab79de5 while PR reviews ran, introducing merge conflicts.
  Commit fixes before merging that pinned landing; preserve independent request
  oracles, all current protocols and the new BingX APIs, then rerun gates/CI.

### BingX landing integration checkpoint

- Merged pinned 022ab79de5 with disjoint engine/harness/host reviews. Preserved
  seven factories, aliases, compressed ordered socket delivery, upgrade headers,
  disposal guards, client-ID rules and every test from both engine branches.
- Engine retains constant-time ticker slots/fallbacks and unshared frame identity
  strings; adds BingX indexed update/market-ID APIs, compact collision-safe
  interning and nested/alias selectors. Engine 43 tests PASS.
- Harness retains lifecycle IDs, constructor/buffer baselines, independent
  oracles, every-run book/candle/subscription/write checks and Crypto first-open
  readiness. Adds BingX protocol/precision/order corrections, response stages
  and per-socket Bybit startup readiness; helper 34 tests PASS.
- Full workspace bans/fmt/clippy/tests and wasm clippy/build PASS. Root typecheck
  44/lint 28/test 27 tasks PASS. Adapter 19 tests PASS; WASM 2,935,585 bytes,
  aggregate budget 3 MiB. Final combined replays and updated-head CI remain.

### Reviewed seven-venue public gates PASS — PR #1309

| Public tape (3 runs) | CPU TS / Rust ms | Retained TS / Rust MB | Peak TS / Rust MB |
| --- | ---: | ---: | ---: |
| bitunix | 1013.1 / 903.2 | 3.6 / 2.2 | 31.2 / 16.0 |
| binance | 269.1 / 143.4 | 4.6 / 2.4 | 34.4 / 12.6 |
| bybit | 449.9 / 346.0 | 6.5 / 2.7 | 33.8 / 10.6 |
| okx | 308.4 / 167.1 | 4.8 / 2.9 | 31.9 / 10.5 |
| cryptocom | 78.0 / 70.3 | 2.1 / 1.5 | 14.7 / 5.6 |
| weex | 114.0 / 76.6 | 5.3 / 2.7 | 20.0 / 7.2 |
| bingx | 139.5 / 83.2 | 3.8 / 2.1 | 25.7 / 5.2 |

- All seven public outputs match with lower Rust main-thread CPU, retained
  memory and sampled peak. Workspace/root checks, 56 Bitunix, 43 engine, 34
  harness, 19 adapter and 23 host tests PASS; bundle 2,935,585 bytes / 3 MiB.
- Three original adversarial reviews, independent fix re-reviews and final
  integration review PASS; all reports are posted on PR #1309. Four actionable
  findings are fixed. Actual rebuilt WASM through FrameApplier verifies full
  to empty clearing, snapshot decoding and uninitialized-book suppression.
- Private replays passed before BingX integration; final ten-variant private
  replay results and exact-head CI are tracked on PR #1309. Landing is authorized
  only after those checks are green; then cut a fresh follow-up branch from the
  merged landing.

### Seven-venue integrated checks complete

- All seven three-run public gates and all ten private variants PASS. Rust SDK
  CI also PASS for the reviewed code commit 4cc7b297c0. Later commits only record
  documentation; their root CI is in progress.
- MEXC landed at 0018bba71a and makes the PR conflicting again. Commit this
  checkpoint, merge the pinned landing, preserve its leverage rails and replay
  clock/socket core alongside all prior controls, then rerun affected gates.
  Landing changes browser wasm optimization to `s`; keep that default first and
  verify its CPU/memory results and actual combined artifact size.

### MEXC landing integration checkpoint

- Merging pinned 0018bba71a after seven-venue checks and independent reviews pass.
  Keep optional independent leverage rails backward compatible; Bitunix retains
  its single-leverage settings by defaulting the new optional rail fields.
- Disjoint reviewers audit engine, socket hosts and the shared replay clock/tape
  integration. All prior matching, lifecycle, oracle and measurement controls
  must survive; build/gates run on landing's size-optimized browser artifact.
- Engine auto-merge retains all prior APIs/fast paths; settings reservation now
  includes optional rail columns. Regression fails before fix, all 48 engine
  tests PASS. Shared socket tape is used by production replay; 49 helper tests
  PASS including explicit lifecycle, close and URL-sharing regressions.
- Eight factories and aliases, bounded ordered compression, headers/disposal,
  MEXC command cleanup/error metadata and legacy rail decoding are retained;
  focused host/socket 19 and Workers transport 5 tests PASS.
- Workspace bans/fmt/clippy PASS. Full workspace tests, wasm lint/build and root
  checks continue; final measured artifact and combined replays are recorded on
  PR #1309. Keep the existing 3 MiB budget with landing's browser optimization.
- Eight-venue artifact: 2,705,853 bytes with landing's `s` build, below existing
  3 MiB cap. Workspace native/wasm checks, adapter 19/host 26 tests PASS.
  Actual rebuilt FrameApplier full-to-empty/snapshot probe PASS.
- New integration review found virtual timers seeing future response stages
  and legacy reconnects opening before their recorded event. Fix both using
  active clock elapsed eligibility and per-URL initial open cohorts. Both
  regressions fail before/pass after; helper suite 51 PASS; independent original
  probes and 27 focused tests PASS with no further findings.
- Root typecheck/lint PASS. First full tests hit HyroTrader five-second import
  timeouts under load; isolated 10 tests PASS and full retry is running. Bitunix
  and Binance private clock smoke PASS. Final eight-venue gates and CI are
  tracked on PR #1309 before the authorized merge/follow-up branch.

### Final replay startup correction

- Full eight-venue replay and CI reproduced a shared-clock bootstrap deadlock:
  activating virtual timers before explicit socket opens froze asynchronous
  exchange startup. Keep inline startup on real timers through bounded readiness,
  then anchor the shared virtual clock at current recorded elapsed time.
- Production Bitunix TS fixture passes with 28,694 records and 697 markets/tickers.
  All 52 helper tests PASS. Independent audit plus 28 focused tests and pending
  timer/offset probe PASS; staged-response and reconnect regressions still PASS.
- Root full tests PASS with task concurrency one (27 tasks); both safe-cex runs
  pass all 6,562 tests. This resolves load-induced HyroTrader import timeouts
  without changing test deadlines or product code. Public/private final gates
  and corrected-head CI remain tracked on PR #1309 before merge.

### Recorded HTTP completion correction

- Combined public run passed all seven prior venues, then exposed MEXC's
  deterministic intermediate depth mismatch. Requests and selected snapshot
  versions matched; replay completed an in-flight HTTP snapshot before its
  recorded completion, skipping intervening depth messages.
- Both adapters now await the shared recorded completion only during active
  public replay. Request eligibility/consumption remains at request time;
  private/startup timing stays real. No venue changes or weaker comparison.
- MEXC complete single-run gate PASS including all 144 bookEvents, helper suite
  53 PASS. Eight public medians are being repeated; private variants continue.
  Independent review and final CI/results are tracked on PR #1309.

### BloFin landing integration checkpoint

- Merged pinned a4d9e2ec84 (BloFin and MEXC parity) after it made PR #1309
  conflicting. Keep nested structure, all nine factories, new partial leverage
  columns, MEXC parity fixture, landing release profiles, and prior replay fixes.
- Disjoint engine, host and harness reviews completed. Engine 54 tests, focused
  host/codec 21 tests, MEXC TS 3 tests, combined harness 58 tests PASS. Bitunix
  explicit leverage10 defaults and absent buy/sell rails remain unchanged.
- Both adapters retain recorded public HTTP completion timing, request-time
  eligibility/consumption, real startup readiness, explicit connection lifecycle
  and strict intermediate book comparisons. BloFin adds per-connection request
  comparisons and plain ping/pong handling. Keep existing combined 3 MiB cap.
- Host review found private BloFin without passphrase waited indefinitely;
  reject immediately, matching existing passphrase venues. Add boundary proof.
  Final nine-venue artifact, gates, independent integration re-review and exact
  head CI are tracked on PR #1309 before merge and fresh follow-up branch.
- Final artifact including BloFin passphrase guard: 2,867,213 bytes. Boundary
  regression fails against the pre-guard bundle and passes after rebuild; all
  21 adapter tests and 28 host tests PASS. Root typecheck/lint/full tests PASS.
- Independent HTTP-timing review found explicit public commands waiting on
  virtual time they block, plus pending background completions after last WS
  frame. Serialize commands through a shared try/finally context; defer only
  background completions, and drain selected pending responses before return.
  Both regressions fail before/pass after, future watchdog remains unfired;
  60 helpers PASS and production MEXC all 144 bookEvents PASS.

### Nine-venue gates and CI PASS; Backpack integration

- Reviewed f965a4057f passed all nine public (three runs) and twelve private
  gates. Bitunix CPU TS/Rust358.9/309.8ms, steady196.5/171.4ms,
  retained3.6/2.3MB, sampled peak31.4/16.1MB; 28,694 public/42 private records.
  Bundle2,867,213 bytes, 60 harness/54 engine/56 Bitunix/21 adapter/28 host tests.
- Exact-head Rust SDK39305109128 and both root CI39305108864/39305092905 PASS.
  Independent reviews PASS, reports and full table posted on PR #1309.
- Backpack landed at b265a0bab0 immediately before merge and made PR conflicting.
  Integrate pinned landing, preserve shared tape refactor and all prior strict
  protocol/oracle/timing guards. Disjoint reviews own harness, host and engine.
  Keep existing 3 MiB cap; verify ten-venue artifact, gates and updated CI.
- Ten-venue merge core is resolved: production replay now delegates to the
  incoming shared Tape, retaining all prior lifecycle, matching, actual HTTP
  completion, explicit-command, terminal-drain, oracle and metric controls.
  Combined helper suite 88 PASS; engine 55 and host 31 focused tests PASS.
- Initial ten-factory WASM 3,150,942 bytes exceeded existing 3 MiB cap by 5,214.
  Apply the existing size profile (`z`) to Backpack too; rebuilt 3,128,408 bytes
  is within unchanged cap, all 22 adapter tests PASS. All ten public performance
  gates verify this optimization, alongside thirteen private variants.
- Full startup checks exposed explicit construction not notifying open-frame
  waiters; notify after registration while preserving unopened state until the
  recorded open. Regression fails before/pass after. Production Bitunix TS
  fixture passes 28,694 frames, all 89 combined helpers PASS.
- Independent review also found dropped headers on production delegation to
  shared validation; forward the fourth argument so WEEX/Backpack signatures
  stay independently checked. Keep all protocol/response controls unchanged.
- Ten-venue root typecheck/lint PASS. Full local tests passed all but two
  HyroTrader import timeouts under load; lower-worker retry also timed out on
  unrelated metadata imports. Focused verification and exact-head CI determine
  readiness; no test deadlines or unrelated venue implementation changed.

### Ten-venue public gates PASS

All ten public fixtures pass three-run correctness and strict CPU/memory gates
on the 3,128,408-byte artifact. TS/Rust medians:

| Venue | CPU ms | Retained MB | Sampled peak MB |
| --- | ---: | ---: | ---: |
| bitunix | 966.9 / 835.4 | 3.6 / 2.2 | 31.4 / 16.0 |
| binance | 304.3 / 180.3 | 4.6 / 2.6 | 34.7 / 12.4 |
| bybit | 506.2 / 352.4 | 6.6 / 2.7 | 33.1 / 10.6 |
| okx | 359.1 / 199.4 | 4.8 / 2.9 | 32.2 / 10.5 |
| cryptocom | 88.2 / 81.3 | 2.1 / 1.5 | 15.1 / 5.4 |
| weex | 127.4 / 100.5 | 5.4 / 2.7 | 20.3 / 7.2 |
| bingx | 111.8 / 73.4 | 3.9 / 2.1 | 22.7 / 5.1 |
| mexc | 491.4 / 285.5 | 10.4 / 3.5 | 39.4 / 19.7 |
| blofin | 106.4 / 80.9 | 3.5 / 2.0 | 16.6 / 5.7 |
| backpack | 221.1 / 127.4 | 3.6 / 2.3 | 18.7 / 6.4 |

Eight private venue gates pass. Backpack private differs only in outgoing
market-bracket limit fields covered by its explicit fixture `tsRequestBody`;
restore that narrow oracle during integration. Four OKX variants continue.

Both exact-head root CI jobs on f23e987a5a PASS, confirming aggregate TS
quality despite local import timeouts. Rust replay CI exposed a Backpack
initial-book race not seen locally: ready depth handlers preceded completion
of the rate-limited initial HTTP snapshot chain. Opt in Backpack to waiting
for every requested initial book callback/view before legacy playback, for
both implementations. Presence is required; initialized empty books count.

Both Backpack corrections pass: public three runs with 32 matching book
events per implementation and private one run. Other nine public and twelve
private gates remain passing. All 91 helpers PASS, including strict fixture
body aliases and delayed seed/wrong-symbol/empty-book readiness regressions.
Independent re-review and new-head CI are tracked on PR #1309 before merge.
Independent final re-review PASS: 91 helpers plus actual production callbacks
prove wrong-symbol books keep startup waiting and delayed initialized-empty
books satisfy readiness while the clock remains inactive. No open findings.

### Bitget landing integration checkpoint

The complete ten-venue gates/reviews are committed in 2ffc8ddad4. Bitget then
landed at 3a7b46407d and made PR #1309 conflicting before corrected replay CI
could run. Merge the pinned landing with disjoint engine, host and harness
reviews, retaining all prior timing/oracle/initial-seed regressions. The
landing raises the combined module budget to 3.5 MiB and optimizes engine,
dispatch and Bitget throughput; retain those profiles and verify actual size
and all per-venue CPU/memory gates. Preserve Backpack's package size profile.

Eleven-venue integration reviews PASS: 58 engine tests, 132 combined harness
helpers, 2 new Bitget SDK-helper regressions and 31 focused host tests. The
shared production Tape retains all prior controls; incoming signed preconsume
validation, typed generated-ID checks and exact product/topic routing are
integrated without a duplicate transport. Independent review prompted a fix
for untagged reconnect opens choosing an active sibling instead of the newly
constructed replacement; original reproduction now opens the replacement
exactly once. Actual Bitget TS smoke passes 36,634 frames and both books.

The initial eleven-factory artifact was 3,766,929 bytes, above landing's
3,670,016-byte cap. Extending existing package size profiles to Bitunix,
BloFin, MEXC and OKX yields 3,656,290 bytes. Preserve engine/dispatch/Bitget
throughput profiles; all eleven public CPU/memory gates verify these flags.
Full workspace Rust/wasm checks, root typecheck/lint/full27 tasks PASS. The
new artifact's 28 adapter tests and 38 host tests pass. Eleven public and
fourteen private gates plus updated-head CI remain tracked on PR #1309.

### Hyperliquid landing integration checkpoint

All exact-head CI jobs for a94796c69f PASS, including eleven-venue replay.
Local Crypto.com startup CPU remained close to its threshold; its package
throughput profile passes locally (78.8 / 70.8 ms TS/Rust), while the aggregate
module exceeded the former cap. Checkpoint 54d51cf35e committed the profile
trial before merging; no stash. The curve25519 size trial is removed because
the new landing changes CI to explicit per-venue feature builds and budgets.

Hyperliquid landed at c5bc2c4adf and made the PR conflicting. Integrate its
optional factory/features, typed recovery query and shared signing primitives,
retaining all prior strict request/signature, replay clock, seed readiness and
memory controls. Separate reviewers own engine/crypto, host/SDK and harness.
Verify the aggregate adapter, twelve single-venue bundles under 1,258,291 bytes,
twelve public and fifteen private variants, then exact-head CI before merging.

Integration checks PASS: workspace bans/fmt/Clippy/tests/wasm Clippy, combined
28 adapter and 38 host tests, root typecheck/lint. Bitunix's new unsupported
client-ID recovery query explicitly rejects before credentials/HTTP, avoiding
a panic introduced by the new query variant; independent review confirmed
and improved the no-request regression. All 57 Bitunix tests pass.

Single-venue Bitunix WASM is 696,820 bytes. Its three-run public gate passes
28,694 frames / 697 markets and tickers / 2 books: TS/Rust main CPU
1,072.9 / 938.6 ms, steady CPU 709.4 / 622.1 ms, retained 3.7 / 2.2 MB,
sampled peak 31.5 / 16.0 MB. Remaining per-venue/private gates continue.

Final merge review fixes preserve auth state on actual socket delivery for
BloFin/MEXC and route actual MEXC response matching through the typed key:
numeric economic fields cannot match strings; documented native ID aliases
remain supported. Independent original reproductions pass. All 220 harness
helpers PASS, including incoming signing/protocol/request regressions. Negative
retained components still invalidate the baseline; no metric threshold lowered.
Actual Hyperliquid TS public/private probes have zero unserved/protocol/socket/
unused-write errors. Root full 27 tasks PASS (including 458 safe-cex files,
6,622 tests). Bitunix/Binance/Crypto.com public and private gates PASS; Bitget
public rerun passes after the busy full-suite run ended, its private gate PASS.
Remaining venues and exact-head CI are required before merging PR #1309.

Exact-head CI exposed two production-only integration gaps missed by shared
helper tests: BloFin literal ping reached JSON.parse, and BingX retained
signing timestamp/signature in legacy request identity. Fix literal handling
without inventing pong responses, and route BingX through strict typed matching.
Both exact-production regressions and independent adversarial probes PASS;
222 helpers PASS. Malformed/auth/subscription errors, caller IDs, economic
field types and one-shot write consumption remain strict. Hyperliquid CI's
negative 88-byte buffer component remains under investigation, not normalized.

On 326d080514 all native checks, both root CI jobs and eleven replay lanes
PASS. Hyperliquid still rejects a negative TS buffer component (-112 bytes).
Node22 Darwin probes have valid +32 bytes and rule out fetch and temporary
DER-key exports; retain strict accounting and investigate without normalization.
Local MEXC exposed a socket-open/REST-seed race: enabling the existing symmetric
initial-book presence barrier yields both books and all 144 hashes identical
to native, zero unserved/protocol errors, and 222 helper tests PASS.
Bitrue then landed at fc18c55874 and made PR #1309 conflicting; checkpoint this
reviewable startup fix before integrating latest landing. No stash.
