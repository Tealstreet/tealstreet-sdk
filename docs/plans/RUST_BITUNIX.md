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

### Transport and Bitrue landing integration

Merge fc18c55874 (including transport8af92cea0f), retaining nested workspace,
all prior strict response/signature/body/timing/readiness controls and the MEXC
seed correction. Thirteen optional/default venue factories remain registered.
Bitunix sockets now declare Public/Private roles with stable futures/private/
ticker:start/price:start identities. SocketContext owns String feed names so
arbitrary market shards stay distinct without leaks; existing static callers
remain supported. A 301-market regression proves six independent feeds,
identity stability across reconnects and one-connect watchdog recovery hints.

Independent integration review fixed Bitrue generated-ID normalization accepting
non-string/empty/oversized/newline IDs. Malformed parent and child IDs now keep
exact wire values. All 230 helpers pass; SDK routing203 and CID2 pass. Aggregate
WASM passes32 adapter and47 host tests; root typecheck44/lint28 pass. Full Rust,
root tests and new exact-head replay CI follow the final role adaptation.

Measurement-policy audit corrected our extra component-sign restriction. The
landing metric is net retained memory: signed heap/buffer deltas combine with
WASM, while absolute counters and the net must remain nonnegative and exactly
consistent. A negative component alone cannot prove an unsettled baseline;
positive components cannot prove no setup storage was released. Restore that
landing policy, preserve signed deltas without clamping, and retain all CPU,
peak/net memory thresholds plus before-constructor sampling/real settling.
For the observed TS -88/-112 cases, the signed delta reduces TS memory and makes
Rust's comparison harder. Independent math/code review and regression cases
cover this correction; no unsupported allocation-lifetime workaround added.

Final thirteen-venue workspace bans/fmt/Clippy/tests/wasm Clippy PASS; final
aggregate32 adapter/47 host tests PASS. Bitunix58 native tests pass. Its final
single-venue module is698,289bytes; public28694frames/697markets+tickers/2books
PASS (TS/Rust CPU1136.9/947.1ms, steady811.7/646.3ms, retained3.7/2.2MB,
sampledpeak31.5/16.1MB), private42records PASS. Binance public/private PASS;
MEXC corrected startup full public/private PASS. Remaining targets continue.

391a14e7ac CI passes all native checks and twelve replay lanes, including
Hyperliquid with exact signed net retention. Bitrue public caught an audit-key
mismatch: outbound canonicalURL ended in '?' while inbound used rawURL. Use
rawURL consistently. Independent review then proved replacement sockets could
inherit URL-keyed subscriptions; audit per-frame topics and heartbeats by actual
socket identity. Parallel connections remain isolated, reconnects resubscribe,
closed private sockets do not satisfy readiness, and historical required-depth
coverage survives final local teardown. All232 helpers and exact original
reproductions PASS; actual TS public3287frames/2books/120bookEvents/780subscriptions
has zero protocol/socket/signature/unserved/unused-write errors. Preserve full
canonical TS/Rust subscription comparison. Focused re-review has no findings;
new exact-head CI remains required before merge.

On final 5a1ba0193b local targets, Bitunix/Binance/MEXC/Hyperliquid/Bitrue public
and private gates all PASS; full root27 tasks PASS. Bitrue's corrected audit
has3,287frames/2books, TS/Rust CPU111.5/94.2ms, retained4.4/2.9MB,
sampledpeak17.8/6.9MB. All phase/adversarial findings were posted on PR#1309.

### Bitget parity landing checkpoint

Before final CI completed, d172a666b1 landed Bitget inverse sizing, UTA ledger
and Classic/UTA trailing parity and made PR#1309 conflicting. Merge it while
preserving all232harness controls and explicitroles/ownedfeed identities. Still
13 publicvenue rows, now17private gate variants plus4Bitget native parity
integration cases. The new inverse TS probe passes (13frames/2markets/3commands,
zero unserved/protocol/socket errors). SDK inverse/linear compatibility review
passes19wrapper and5wire tests.

Engine review fixed exact preallocation for the new optional BALANCE collateral
tail/coin strings, with failed-before/pass-after reservation regression. Engine
61tests andBitget147tests PASS; BitunixO1indexes, railsettings, startupreservation,
emptybooks and streamingdecoder contracts are retained. Full workspace/root
verification, new aggregate build and exact-head CI continue before merge.

### WOO landing integration plan

Merge6945ee83b4 after the previous head passed Bitunix/Binance and every
replay lane except BingX steady CPU. Keep nested workspace, Bitunix socket
identities and all strict replay controls. Review harness, host/SDK and engine
compatibility independently; preserve the optional market-slippage field while
Bitunix retains its existing semantics. Re-run native/workspace/WASM/root gates,
Bitunix and Binance public/private, and native Bitget parity. WOO adds the14th
public lane and18th private variant.

BingX's previous CI steady CPU was TS41.5/Rust48.1ms. WIP8e526c20b4 changes
its venue profile from size optimization to throughput optimization. Verify the
unchanged1,258,291-byte cap and all correctness/CPU/memory thresholds before
retaining it. Exact merged-head green CI remains the merge condition.

Merge review fixed Bitunix's sole cancel/replace PlaceOrderOpts literal for the
new optional field, explicitly None as in the other existing venues. WOO's
new URL-keyed audit reset could erase parallel socket state; use actual socket
metadata and a production-path regression. All239helpers PASS. Independent
host/SDK review passes27wrapper/frame,36host/socket/wire and4adapter checks.
WOO TS public77frames/222markets/2books and private60frames/20commands have
zero unserved/unused/signature/protocol/socket errors. Local lint28tasks PASS;
full root/native/WASM verification and exact-head CI remain pending.

On760f7bb72d, CI passes13public/private venue lanes including Bitunix/BingX,
all native/workspace/WASM checks; WOO alone catches a startup REST-seed race:
TS handler readiness preceded its standalone book snapshots. Enable the
existing symmetric per-symbol initialBookSnapshots barrier for WOO, retaining
strict timestamps/hashes. Regression fails before/passes after;239helpers PASS.
Actual TS first book now472ms with46events, matching Rust's reported trace,
instead of754ms/44events. Independent review and final native replay follow.

BingX throughput optimization now passes local public/private gates and CI:
CI steady CPU TS56.2/Rust37.2ms; module827,296bytes below unchanged1,258,291.
Bitunix CI public/private PASS with699,935-byte module,28694frames/697markets/
2books; TS/Rust CPU346.3/310.7ms, steady152.7/139.6ms, retained3.6/2.1MB,
sampledpeak17.9/15.8MB. RealizedPNL remains verbatim net of fees/funding.

### Final throughput and gate checkpoint

The local size-optimized Bitunix build passed correctness/memory but narrowly
failed main CPU (TS1,181.0/Rust1,208.8ms) and steady CPU779.9/820.2ms.
Change only its release package optimization to3, preserving the full workload,
three-run medians, exact output checks and all thresholds. The rebuilt module
is774,839bytes below1,258,291. Public PASS:28,694frames,697markets/tickers,
two books; TS/Rust main CPU1,044.4/983.8ms, steady716.4/655.2ms, retained
3.7/2.2MB, sampledpeak31.5/16.1MB. Private42records PASS. Independent
performance review reports no findings; process CPU remains informational as
in the existing gate. No attempts discarded or thresholds adjusted.

WOO's seed correction passes full public/private CI on72eb26478a, including
strict book-event timestamps/hashes. Every14venue replay lane passes on that
head, along with native/workspace/WASM checks; root CI is still running. The
final throughput profile commit requires its own green exact-head CI before
merge, then a fresh branch from landedfeat/rust-rewrite. Local full root test
rerun continues; typecheck44/lint28 and all239harness helpers already PASS.

### Lighter landing integration plan

Before final-profile CI could run, landing31ec401f60 (PR1319) made#1309
conflicting. Merge it with the nested structure and fifteen optional/default
venue factories; preserve Bitunix/BingX throughput optimization, WOO's
per-socket audit and symmetric REST-seed barrier, and every prior strict
clock/request/signature/memory comparison. Review shared harness, host/SDK and
crypto/engine compatibility in independent scopes, then rerun affected checks
and exact-head CI. The new Lighter lane adds the15th public venue and19th
private variant; retain four native Bitget parity cases. Its deleted crypto
wrappers must remain provably unused; existing Fp5/Schnorr APIs remain intact.

Lighter merge review preserves all prior controls and passes256helpers. Fix
its strict transformed-form matching so duplicate/extra fields cannot disappear
into a canonical object; invalid writes return599 without consuming a tape
response. Preserve pre-consumption matched-write hooks and explicit command
ordering. Host review found unconditional client-ID checks would reject Binance
USD-M/PM because their shared rules were absent; add the exact existing TS
36/32-character rules and broker mint, with failed-before/pass-after tests.
Lighter/MEXC fail-closed guards remain intact.42SDK and36host checks PASS.
Engine61/Bitunix58 and crypto24+integration/doctests PASS; deleted crypto
wrappers have no surviving callers. Root typecheck44/lint28 PASS; preceding
full root27tasks PASS. Final workspace and exact merged-head CI continue.

### Lighter snapshot arithmetic correction

On965495d88d, all14other venue replay lanes pass, including Bitunix with its
throughput profile (WASM774,849bytes; TS/Rust main452.8/372.4ms, steady
197.1/151.3ms, retained3.6/2.1MB, peak17.9/15.8MB; private42records PASS).
Lighter's exact intermediate book hashes expose duplicate-price REST snapshot
amounts accumulated with plain Rust += instead of TS safe-math.add. Independent
fixture arithmetic reproduces both CI hashes exactly:4.3779200000000005
versus4.37792. Fix production Lighter snapshot aggregation only, preserving
TS's per-add decimal rounding and duplicate source order through stable sort.
Prices/delta replacement/nonce handling and all gate comparisons stay intact.

A captured-payload native regression fails before/passes after for both REST
and WebSocket snapshots, both sides, tiny quantities and distinct off-step
prices. All86Lighternative tests, scopedfmt/Clippy and independent source
review PASS. Native WASM compilation PASS; complete public/private replays and
exact-head CI follow this reviewable checkpoint. Full prior workspace/WASM
checks PASS; local root rerun encountered import deadlines on the busy host,
with no deadline/assertion changes. Final CI must be entirely green to merge.

### Orderly landing integration plan

All25checks pass on502f6e6337 (including both root CI runs,15public venues,
19private variants and4Bitget parity cases). Before merging, Orderly landed
atf66bdc26f9 and introduced conflicts. Merge the16thvenue while preserving
BitunixPNL/throughput and every reviewed harness correction, including Lighter
decimal snapshot arithmetic. Review harness/workflow, WASM/SDK and engine
compatibility in disjoint scopes. The new optional position-ID frame tail must
retain legacy layout by default and fully reserve strings/bytes when enabled;
deactivation and HTTP abort hooks must preserve existing host lifecycle guards.
Verify focused regressions and helpers, checkpoint, then require fresh entirely
green CI before merge. No stash, force push, master changes or lowered gates.

Orderly merge review fixes optional position-ID tail alignment and ID-string
preallocation. The63/64-row regression fails before/passes after; engine62
and Bitunix58tests PASS with legacy opt-out layout preserved. Host/SDK review
passes83SDK and43host/socket/wire checks; root typecheck44/lint28 PASS.
Harness286helpers PASS (all256prior plus Orderly protocol/production guards).
WS request correlation remains scoped by socket/symbol/generation, validated
before consumers; recorded sends retain lifecycle/channel/context metadata.
Strict timing/hash/memory/signature/write controls and Lighter arithmetic stay
intact. Final CI covers16public venues,33private variants (14Orderly broker/
network variants) and4Bitget native cases. Independent final review and full
workspace/merged-head CI continue before merge.

### Orderly private replay corrections

On1f8b2fed91 all15other venues pass and Orderly public passes. Its first
private TS run exposes ambiguous untagged synthetic heartbeats after three
public sockets share one URL. Tag fixture frames with their intended primary
socket; preserve strict routing instead of latest-socket/broadcast fallback.
Actual replay then exposes ordered writes comparing original native records
while matching uses declared TS body alternatives. Feed both queues the same
exact adjusted records. Finally equate only raw zero-byte HTTP bodies and
absent bodies, preserving distinct encoded JSON values and original signed
bytes. No venue behavior, timeouts, metric thresholds or audits weakened.

All289helpers PASS with failed-before regressions for socket provenance,
caller/type/order/single-consumption guardrails and body identity. Actual TS
Orderly mainnet private PASS:33frames,14commands,12writes,9subscriptions;
zero unserved/unused/signature/protocol/socket errors. Independent final guard
review and all14broker/network native private gates follow this checkpoint.
Full workspace bans/fmt/Clippy/tests/WASM Clippy on the integrated Rust source
PASS; exact-head CI must pass before merge.

### Bybit variants landing integration plan

The exact-head watcher detects922b5fdc89 (PR1307) landing before1eac CI can
finish. Four full Orderly broker/network private gates pass (Orderly and
Tealstreet DEX, both networks); stop the old chain before merging new harness
code. Integrate Bybit inverse/spot/demo/Hyro variants without changing Bitunix
base sizing or net PNL. Review host/SDK, harness and engine/Bitrue compatibility
in independent scopes. Preserve our exact signed-component net memory policy,
constructor baseline and strict sampled-peak gate; incoming older component
restrictions/omitted peak checks must not undo audited policy. Preserve every
Orderly289 correction and Lighter decimal snapshot fix.

New Bybit-only BBO/tick controls require Bitunix whole-batch rejection and
explicit None on cancel/replace. The optional ORDERS native-remaining column
must be fully preallocated alongside existing position-ID/collateral tails.
Review retained engine scratch buffers and AnyFieldEnvelope/object-path guards;
preserve mandatory Bitrue roles through conflicts. Verify helpers and focused
regressions, checkpoint, then require entirely green fresh CI. Coverage becomes
20public fixtures/37private variants over16venue rows, plus4Bitget cases.

Bybit integration source is verified and released. Engine69/Bitunix59/
Bitrue61tests PASS; fix conditional eighth ORDERS column reservation and
Bitunix's new-field initialization/whole-batch unsupported-control rejection.
JSON preserves AnyFieldEnvelope semantics with explicit-object-path guards,
Bitrue mandatory socket roles remain intact, and retained frame scratch buffers
pass reuse regressions. Full workspace bans/fmt/Clippy/tests/WASM Clippy PASS.
Root typecheck44/lint28,100SDK and46host/socket/wire checks PASS.

All311helpers PASS, including strict signed-component and public-peak gate
regressions. Independent review finds malformed Hyro generated IDs could alias
valid create requests; restrict original typed strings to documented ASCII
1..36characters on creation routes only. Eleven malformed transport probes
now reject without consumption; caller/cancel/recovery identities stay exact.
Replace the old invalid synthetic @generated placeholder with a valid fixture
ID, rather than relaxing validation. Actual TS Hyro private13frames/3commands
PASS with zero unserved/unused/protocol errors. Memory/requests helpers retain
our audited policy verbatim. Independent final aggregate and exact-head CI
follow this checkpoint; all20public/37private+4Bitget cases must pass to merge.

Final readiness audit tightens Hyro's public-first phase: both public and
private ready(false) calls wait for markets+tickers, while full private readiness
still requires account flags. Native already waits symmetrically. The actual
production callback regression fails before/passes after, with no timeout or
output normalization. Independent311helper aggregate plus this regression
PASS (312total); signed request/socket/generated-ID/frame controls have no
remaining findings. Checkpoint this two-file follow-up before final exact-head
CI; no further source changes planned absent a proven failure.

### Explicit venue-family selection correction

On e78dd50cba every20public fixture and36other private variants pass,
including all14Orderly and4Bybit variants. BingX TS private fails because
new Bybit code infers family from the generic subscriptionGenerations flag,
bypassing its injected typed matcher. The quantity0.0021 matches the fixture;
no SDK/native arithmetic changes are warranted. Add explicit Bybit family
metadata and isolate only family-specific matching/market/gate normalizations.
Keep generic shared-send/generation-aware subscription capture unchanged.

The exact production constructor regression fails before (599vs200), then
passes after. Wrong quantity/type/caller/extra-field attempts reject without
consumption, while original0.0021 succeeds once; two generations and topic
capture remain verified. All313helpers and8production-replay controls PASS.
Actual TS BingX private38frames/3commands PASS with zero unserved/unused/
protocol errors. No economic aliases or source-venue changes. Independent
final review and entirely green fresh CI remain required before merge.

### WOO steady CPU follow-up

At6b2d580332 Bitunix public/private and corrected BingX gates PASS.
Independent adversarial family/capability audit passes all313helpers,
including signed whole-constructor probes for WOO/Bitrue/Orderly; findings
posted on PR1309. WOO public correctness/memory PASS but steady CPU fails
18.7ms against TS18.6ms. Preserve that result, all samples and thresholds.
Review WOO's native hot path for a bounded allocation/parsing optimization;
an independent review checks harness selection and semantics. A borrowed-
string regression exposes serde's Str alias bypassing Cow specialization,
allocating every ticker symbol. Prefer explicit Cow on hot fields over the
initial reusable-buffer proposal, retaining the original derived parser and
its whole-frame validation. Assert borrowed plain/owned escaped strings.
Verify native regressions/fmt/Clippy, then build WOO once and run the unchanged
complete public/private gates. Review before committing and require fresh
entirely green exact-head CI before landing. No rerun-to-green.

Final WOO fix changes only three wire-field type spellings plus a borrowing
regression/comment. Plain event/topic/symbol strings borrow source; escaped
strings decode as owned equivalents. Original derive/Vec parser retained,
including whole-message rejection and control-event precedence. All113WOO
native tests, fmt/strict Clippy, bans and WASM Clippy PASS. First rebuilt
public three-run gate PASS:77frames/222markets/2books, TS/Rust main75.4/56.1ms,
steady34.1/25.9ms, retained2.0/1.3MB, sampledpeak14.6/4.8MB. Module835,362bytes
below unchanged1,258,291cap. Private60frames/20commands PASS. At6b all other
venue rows and both root CI jobs pass; final review/commit and fresh CI follow.

Independent final WOO review PASS/no findings. Reviewer confirms serde_derive
selects borrow_cow_str only for a syntactic Cow type, not its Str alias; the
plain/escaped regression proves both paths. No custom parser or scratch field
ships. Checkpoint before integrating newly landed KuCoin PR1312 at373cc09864.

### KuCoin landing integration plan

KuCoin PR1312 lands at373cc09864 before the WOO follow-up can enter CI.
Commit reviewed WOO fix as c3a2d1a102, fetch/read the unchanged venue brief,
then merge the new landing. Preserve nested17-venue structure, Bitunix
net-realizedPNL/base-size semantics, WOO borrowed hot fields, throughput
profiles and every existing strict replay control. Independent scopes own
harness/workflow, SDK/hosts, and native compatibility. Resolve all conflicts
without stash/rebase, audit KuCoin signed request headers and pending-read
settlement against both matcher call shapes, keep explicit Bybit family
selection and signed-component net memory/peak tests. Validate helpers,
focused SDK/host behavior and all native checks, then fresh exact-head CI
covering21public fixtures/38private variants plus4Bitget parity cases.
Post adversarial findings and require all27registered checks green before
merging into feat/rust-rewrite; cut a fresh branch from the landed result.

KuCoin integration native checks PASS:1,447workspace tests, including
engine69/Bitunix59/WOO113/KuCoin99; no native compatibility fixes needed.
SDK103 and host/socket/wire46 checks PASS. A new decimal-lot test mock
initially fails typecheck; preserve its column-key types instead of unsafe
PositionTable casting, then typecheck/focused6 PASS. Root lint28 PASS.

Independent adversarial review finds KuCoin generatedBodyId aliases any
string, allowing correctly signed malformed clientOid values to consume a
valid generated-ID ACK. Enforce existing /^[A-Za-z0-9_-]{1,40}$/ only on
create endpoints before normalization, with signed invalid/valid/one-shot
transport probes. Preserve exact caller/cancel identities. Finish helper
aggregate, actual TS private replay and independent re-review before final
WASM/public/private gates, checkpoint and exact-head CI.

First integrated KuCoin native/TS private gate PASS:32frames/17commands.
Public16,157frames/692markets/2books passes all CPU/memory metrics but all
three runs fail strict intermediate book flush counts (TS141/142 vsRust150).
Preserve the failure and every hash/time check. TS ETH startup snapshot can
remain behind its3RPS limiter when the replay clock starts; evaluate the
existing symmetric per-symbol snapshot barrier, then independently inspect
cached/stale recovery response timing. No native bug proved yet. Module
745,472bytes is below the unchanged cap. Roottypecheck44/lint28 PASS.

Adversarial checks also reproduce two further KuCoin harness gaps: sharedURL
replacement/parallel sockets borrow welcome/subscription state, and WebSocket
connectId volatility hides an extra signed HTTP parameter. Bind protocol
state to actual socket instances and keep volatile connectId normalization
on WebSocket URLs only. Validate failed-before/after signed writes and
connection-isolation probes before final aggregate/replay/CI.

KuCoin per-symbol startup snapshot barrier aligns all150flush timestamps.
Remaining early hash mismatches prove a production TS recovery publication
bug: BTC1208 equals the stale REST snapshot at1205(seq0949); ETH1461/1712
equals the stale snapshot at1232(seq0450), despite unresolved gaps and no
callback. applySnapshot mutates a previously emitted object by reference.
Native correctly retains the last valid book until contiguous recovery.
Fix TS transactionality with a failed-before/pass-after publication regression,
preserve queued gap deltas/suffix, document KNOWN_TS_FIXES with no hash waiver,
then require all intermediate timestamps/hashes to match on the unchanged gate.

Reviewed KuCoin merge checkpoint: all3harness guard fixes independently
pass actual production-constructor signed/one-shot/socket-isolation probes.
All340helpers PASS; transactional TS recovery publication regression fails
before (published amount2 silently becomes90 without callback) and passes
after, with14focused book/worker checks and package typecheck/lint PASS.
Snapshot candidates are fresh objects; contiguous live delta reuse remains.
The initial seed barrier yields150/150matching timestamps. All hashes match
except ETH1712/1968, where TS3RPS read dispatch delays recovery. Preserve
these two failures, checkpoint source before investigating causal replay
limiter epoch/request completion and native read pacing. No public gate PASS
claim, push or merge until the unchanged strict comparison passes.

### KuCoin shared request-budget correction plan

Source audit proves native omitted TS's shared xhr maxRPS3 budget: public
reads, snapshots, public/private bullet tokens, account reads and settings
use the same queue; create/cancel alone use unlimitedXHR. Remaining two ETH
flush differences expose this dispatch difference. Implement a sans-I/O
3per1000ms FIFO for those exact routes, signing only when dispatched. Keep
streamed responses, typed pending jobs, generation guards, explicit roles,
read retries, and unlimited placement/cancellation semantics. Audit timer
boundaries/reentrancy, fresh signatures and failure behavior. Add meaningful
quota/FIFO/window/sign-at-dispatch tests; adapt scripted helpers to actual
causal timers without weakening payload/state assertions. Independent plan
and implementation review, new artifact and unchanged complete gates follow.
No arbitrary sleeps, TS limiter bypass, hash/timestamp normalization or
sample removal. Existing340helpers and all native/source checks are green
at checkpoint2a3ed236f6; strict public gate remains pending this correction.

### Next landing: AsterDEX

Origin advances to d9aa597fd3(PR1322) during the KuCoin budget implementation.
Finish/review/checkpoint the queue before merging; no stash. Re-read changed
RUST_REWRITE requirements: HTTP route hints remain host-only metadata; new
optional ticker identity and per-currency balance tails preserve legacy
offsets. Integrate Aster V1/V3 while preserving netPNL/base units and all
strict harness controls. Bitunix must explicitly reject unsupported new STP
controls during whole-batch preflight. Audit optional-tail preallocation,
CancelOrderRefs backward behavior, ordinary account routing and lazy crypto
loading independently across native, SDK/hosts and harness scopes. Run fresh
checks/gates and all registered CI checks before landing.

Fresh queued KuCoin WASM748,405bytes and raw public diagnostic PASS: all
150timestamps/hashes exactly equal the corrected TS replay, with zero
unserved/unused/protocol/socket errors. Exact3RPS dispatch is causal; no
measurement or comparison adjustment. Full private gate PASS32frames/
17commands. All102KuCoin native tests and strict Clippy PASS; final helper
audit strengthens known-mode equality and preserves whole-batch zero-HTTP
assertions. Independent source/FIFO/signing/stream/generation audit finds
no issue. Final three-run public performance gate follows CPU-quiet release,
then checkpoint before Aster integration.

Final unchanged KuCoin three-run public gate PASS:16,157frames/692markets/
2books, all150intermediate timestamps/hashes exact. TS/Rust main CPU
1,068.5/646.3ms, steady709.2/398.5ms, retained7.2/2.9MB, sampledpeak35.7/18.1MB.
Module748,405/1,258,291bytes. Full private32frames/17commands PASS.
Owner102native/fmt/strictClippy/bans PASS and independent final queue/helper
review has no findings. Whole-batch zero-HTTP guards remain, cached fixture
mode equality is explicit. Checkpoint the causal3RPS correction before
fetching/merging AsterDEX; no hashes, timings, thresholds or samples waived.

Aster host/SDK conflicts resolved preserving18factories:324SDK/routing/frame
and63host/worker checks PASS, package typecheck/runtime lint and root44/28
typecheck/lint PASS. Engine76/Bitunix60 PASS; new63/64currency+orphan ticker
reservation tests fail before/passes after, preserving every prior allocation
regression and scratch buffer guard. Bitunix rejects all3STPmodes across the
whole batch before HTTP, and initializes replacement STP to None.

Harness integration preserves requests/responses/memory backbone verbatim.
Correct production validator options carry actual metadata/config credentials;
Aster protocol state uses actual socket instances, and signing time remains
continuous. Independent signed probes find unsupported extra newClientOrderId
query fields could disappear into generic aliases; keep them exact for Aster.
Initial352helpers PASS and actual TS V1/V3 private probes are running. Fresh
Aster artifact builds successfully; complete native checks, final independent
review, unchanged public/private+3parity gates and fresh28-check CI follow.

Reviewed Aster integration checkpoint: independent352helpers and signed
production constructor/socket probes PASS, all1,564native workspace tests
and bans/fmt/Clippy/WASM Clippy PASS. Full V1/V3 private gates14frames/4commands
each and3Aster parity cases PASS; module1,025,202/1,258,291bytes. Root44typecheck/
28lint,324SDK/63host checks PASS.

First public three-run gate passes books/CPU/memory but fails75OIcomparisons
(25fields x3runs). Independent raw TS store proves25nonzero vsnative50;
worker packing is correct. TS failed bulk is retried by fallback helper,
consuming recordedthirdbulk at16.753s; first25complete18.753s and nextpoll
33.753s beyond20s. Native skips that retry and reaches another25early.
Preserve TS request semantics: add repeatedbulk before fallback plus shared
public-client10/1000ms FIFO (clock/private transports unaffected). Stage
fallback values and commit only when the batch finishes as TS does. Add
quota/FIFO/stream/retry, secondbulk-success and partialbatch publication
regressions. Independent phase review, rebuilt artifact and unchanged full
gates follow. No OI erasure, reference normalization or weakened threshold.
Checkpoint merged source before this bounded native correction.

### Next landing: Kraken and ApeX Omni

Origin advances via7da56dc8a5(PR1323 Kraken futures) to ecb313026f(PR1324
ApeX Omni) while Aster's bounded OI/public-budget fix runs. Complete/review/
checkpoint Aster first, then fetch/read brief and merge without stash.
Preserve our strict all-run books/candles/outbound/peak and signed memory
backbone; incoming gates still compare only final books. Integrate actual
production typed auth/body hooks and per-socket ApeX protocol state, tighten
route-scoped generated-ID rules and commit Kraken nonce only after valid
auth. Investigate Kraken's documented depth correction against every
intermediate hash, retaining independent input-based expectations rather
than adopting weaker final-only checks. Native/hosts/harness scopes review
compatibility separately. Expected20venues/24public/42private with prior
parity cases; require all30registered CI checks green before landing.

Aster OI/public-budget correction complete:104native tests, fmt/bans/native
and WASM strictClippy PASS; independent phase review no findings. Raw native
replay exactly matches all631TS tickers,2books and130intermediate book
timestamps/hashes with zero transport/protocol errors. Unchanged3-runpublic
PASS384frames/608markets/631tickers/2books: TS/Rust main145.9/76.9ms, steady
50.1/31.8ms, retained10.6/2.2MB, sampledpeak36.6/10.2MB. Newmodule1,028,015/
1,258,291bytes. Rebuilt V1/V3private14frames/4commands each and3parity cases
PASS. No reference/timing/threshold/sample changes. Checkpoint native-only
correction before nextlanding merge.

Kraken/ApeX integration source checkpoint: all1,762native tests across151groups,
bans/fmt/strict native+WASM Clippy PASS. Host/SDK review passes216incoming
regressions+63host checks; Kraken production full-depth and sequence recovery
repair has5fail-before tests and12book/stats checks PASS. Full-depth snapshots
are no longer truncated; duplicate/stale updates are ignored, gaps request a
fresh snapshot, malformed frames preserve the last published book atomically.
No final-book reconstruction oracle remains. ActualTSpublic replay has9,641
frames/32book events and fullBTC1693/1502,ETH647/612levels.

Harness389helpers PASS. ActualTSprivate Kraken49frames/13commands/11writes
andApeX50frames/23commands PASS with zero transport/auth/protocol errors.
Review fixed actualsocket readiness, route/type-valid generated IDs, Kraken
nonce commit after validHMAC, signed-body validation before typed matching,
and complete replayOptions forwarding (ApeX omniKeySeed) to real constructors.
Root typecheck/lint and independent adversarial review are running; new
per-venue artifact/public/private gates remain required before readiness.

Final independent Kraken/ApeX review found two bounded P2 response aliases:
ApeX JSON bytes labeled as form data could normalize into a recorded form
request, and Kraken batch JSON unwrapping dropped extra outer form fields.
Both now reject before response consumption; actual production-Tape probes
verify malformed correctly signed writes leave the ACK available, then the
valid write consumes once.391helpers and6independent production constructor/
socket/credential regressions PASS; remaining review clear/diminishing returns.
Root44typecheck/28lint tasks PASS. Kraken release module885,370bytes built;
public/private artifact gates next, thenApeX.

Kraken unchanged rebuilt gates PASS: private49frames/2markets/13commands;
three-run public9,641frames/207markets/tickers/2books, all32intermediate
events/full depth exact. TS/Rust main2425.9/426.9ms, steady1832.8/277.0ms,
retained2.1/1.9MB, peak175.2/9.4MB; module885,370/1,258,291bytes.
ApeX build/gates next.

ApeX unchanged rebuilt gates PASS: private50frames/23commands; three-run
public51frames/89markets/tickers/2books. TS/Rust main95.8/49.7ms, steady
63.0/12.8ms, retained3.7/2.1MB, peak30.2/5.1MB; module1,004,067/1,258,291bytes.
Both newvenues fully pass artifact gates; checkpoint before nextlanding.

### Next landing: Paradex

Fetched origin1f9e9a5206(PR1320). Phase plan: merge preserving nestedworkspace
and all21factories; native binary ingress defaultsfalse, keeps engine frame
validation/reservation guards, accepts provenunused crypto cleanup and adds
four explicitbinary:false patterns. Hosts layer rawbinary into ourbounded
ordered decoder, preserve close/disposal andgzip/zlib text; retainBitunix in
secretfeaturecfg and add exactParadex account/CID/routing policy. Harness
layers binary frames/waitmarkers onto strictbackbone, actualsocket closed
readiness, route/type/caller-valid generatedIDs and scoped rawsignaturefields.
Plan reviews identify unsignedclient_id masking and timestamp-dropping ACK
normalization as concrete risks to resolve. No TS publicratebudget exists.
Independent phase reviews, fullnative/SDK/harness checks, public/private
artifact gates and unchanged1,258,291sizecap required. Expected21venues/
25public/43private+priorparitycases; all31registered CI checks green before
merge. PriorParadex sizeheadroom~18KB warrants directmeasurement.

Paradex merged-source size measurement: release module1,232,780bytes,
25,511bytes below unchanged1,258,291cap. Nativefocused engine78/Bitunix60/
Bitrue61/Paradex107 PASS; fullnative strictClippy PASS. Production source
keeps allprior engineguards; onlynewnative contributions are2binary ingress
regressions and4explicittextsocket flag assertions. Host Rust buildsafe;
JSboundedrawbinary and harness security/control-timing integration pending.
Latestbrief explicitly requires borrowedbinary ingress onlyon negotiated
channels and preserves existingtext/decompression; integrated accordingly.

Paradex compatibility checks PASS: fullworkspace1,873tests/158groups,
engine78/crypto27/Bitunix60/KuCoin102/WOO113/Aster104/Paradex107; bans/fmt/
native+WASMstrictClippy. SDK288/10suites and71host/socket/wire/Worker
checks PASS; root44typecheck/28lint PASS. Test-only actualfragmented
16MiB+1rawmessage rejects1009, never delivers toengine and does not reset
silence/fallback (1focusedPASS afterfullworkspacecompile). Native artifact
production source unchanged. Hostrelay exact-onceclose/typeadapter fixes
areTSonly. Borrowedbinary preservesmetadata/orderedclose/disposal andbounded
legacycompressedtext.

Harness418helpers/41focusedPASS; actualTSprivate33frames/15commands and
public12,396frames/66markets/tickers/2books/38events clear.135accepted
recordedACKtimestamps are retained as validatedwaitmarkers. Independent
review confirms raworder auth precedes scoped signature stripping/typed
matching and exactcallerIDs, then findsduplicateAuthorization last-wins
canhide aninvalidfirstvalue. Bounded duplicateheader rejection+actual
production no-consumption regression required before finalreview/gates.
RebuiltprivateartifactPASS33frames; publicperformance waits for CPUquiet.

Independent finalParadex review PASS after duplicateheaderauthfix: original
array/casevariantobject exploit rejects before consumption; validuniqueJWT
consumesonce then noACK remains.4production regressions independentlyPASS
(/tmp/bitunix-paradex-independent-focused.log). No remaining findings;
boundedreview atdiminishingreturns. Ownerfinal419helperaggregate running.

Final419/419helpers+42focusedPASS; source staged/released, no markers or
whitespaceerrors. Unchangedthree-runParadex publicartifactgate running
withlocalchecksquiet. Checkpoint reviewedmerge before gateoutcome.

Paradex unchangedthree-runpublicartifactgatePASS12,396frames/66markets/
tickers/2books/all38intermediateevents. TS/Rust main789.2/616.7ms, steady
431.4/298.8ms, retained1.9/1.6MB, peak75.4/28.6MB; module1,232,780bytes
under1,258,291cap. Private33frames/15commandsPASS; allphasereviews complete.

### Next landing: Phemex

Fetched45d7a3fb98(PR1310); briefprocess unchanged. Plan: integrate22ndfactory
withBitunixsecretcfg intact, graftUTF8scratch/actions fastpath insidecurrent
closed/connecting/bounded guards, keeprawbinary andlegacyhostcompatibility.
IncomingTSfull-depth/sequence andhedgeleg reconciliation changesretainprior
Kraken/KuCoin repairs. Harnesspreserves signedmemory/peak/settle policy and
419strictbackbone, scopesPhemex realconstructor/auth/typedmatching, replaces
URL+objectstring collisions withactualsocketidentity, activeperconnection
topics andprivatebusiness authvalidation. Bootstrap framesmustretain times/
framecount throughourstartupDelivered prelude ratherthanbeing discarded.
Nativecompatibility/unusedcrypto audit, fullchecks+targetedSDK/host/helpers,
independentadversarialreview andunchangedpublic/privateartifactgates precede
readiness. Expected22venues/26public/44private+priorparitycases, all32CIchecks
exactheadgreen beforemerge. Checkpoint priorcompletedphase beforemerge.

Phemexsource checkpoint: mergedJSONdriver preservesours selectors/objectparent
paths/AnyFieldEnvelope/fullboundederror_envelope, addsPathOmitLists and
ignoredlist validation;83engine testsPASS. Nativecompatibility reviewclear,
Bitgetindex reload/duplicates semantics andallpriorengine/venueguards kept.
22factory host+SDK released:76hostchecks,255SDK/9suites, packagetypecheck/
scopedlint PASS. Byteactionsfastpath retainsclosed/reentrant/UTF8/bounded
behavior. Root44typecheck/28lint PASS. Phemexreleaseartifact729,158bytes
(<1,258,291); actualWASM optionalstrings/bytes/actions/authsubscription
regressionPASS. Fullnativechecks+harnessintegration/replayprobes pending;
no performancegate launched while checksrun.

Phemexharness source released:441helpersPASS,3independent realconstructor/
socket/request and2bootstrap immediate/delayed-auth probesPASS. Strictglobal
writeorder nowenabled afterreviewfoundeligiblecancelcouldskipcreate; signed
wrongorder599 preservesbothACKs, correctcreate/cancel consumeeachonce. Raw
HMACbeforeconsume, typedroute/caller/economicidentity, actualsocketclosed/
unsubscribe/privateauth andoriginalbootstrap100/200/300ticks/count3 exact.
Memoryhelper+tests remainHEADverbatim; no5sdeadline/policy/peakchange.
ActualTSprivate16frames/2commands andpublic534frames/2books/126events pass
withzerotransport/autherrors. Initialrootinvocation usedwronghyphenscript
name andexitedbeforetapegeneration; correctedcolon script beforeanyreplay.
ActualbuiltprivategatePASS16frames; publicawaitsfullnativequiet.

Phemexnative fullchecks complete:1,968tests/165groups PASS; engine83,crypto27,
Bitunix60/Bitrue61/KuCoin102/WOO113/Aster104/Paradex107/Phemex86/Bitget149,
native39/WASM7. Bans/fmt/strictnative+WASMClippy PASS; sourceclean. Public
three-rungate nowrunningwithlocalCPUchecksquiet.

FirstunchangedPhemex public3rungate FAIL3identicalbookEvents comparisons:
TS127vsRust128, firstdifference1TS[784,BTCUSDT,424ff...] vsRust
[472,ETHUSDT,ffb...]. Finalbooks/CPU/memory PASS: TS/Rust main605.9/353.1ms,
steady433.2/187.8ms, retained6.2/4.8MB, peak35.4/10.7MB. Coinbaseorigin
5c0e437faf read-onlyaudited (balancecurrencyheaderreservationoddN needs+8);
HOLDnextmerge untilPhemexearlypublication causality proven/fixed/reviewed.
Nohashrewrite, timing/threshold/sample changes orrerun-to-green.

### Phemex bounded request-budget correction

Three independent source audits confirmTSparentxhr usesaxios-rate-limit
maxRPS3, unlimitedXHR distinct. Installedlimiter isFIFO first-dispatch
1000msfixedwindow, products+tickers consumeslots1/2, BTCsnapshot3, ETH4
queued; firstWSdepthboth228ms, recordedHTTPcompletions252/266ms. Native
request() dispatchesallimmediately. Everylater127bookeventrows/hashes match
exactly, isolating earlyETHpublication. NoTSproductionbug ormissinggeneric
HTTPdrain proven. No initialBookSnapshots barrier: snapshotsrequirefirstWS
frame; settingreadinesswoulddeadlock orchangetapeinit.

Plan nativevenue-only typed3/1000FIFO overeverylimitedxhr route, retaining
unlimitedcreatePUT/cancelDELETE andexactsettings/private/historysplit. Sign
atdispatch, retriesreenterbudget, stale depthrequestsdropbeforequota where
required; preserve streameditems/Pendingroles, commands andwholebatchguards.
Add quota/FIFO/freshsignatures/stale/stream/private+unlimited regressions.
Independent plan+implementation audits, fullnativechecks andrebuiltprivate/
unchanged3runpublicgates required. Nooraclehash/bookdeletion, thresholds,
sampling, readinessorfixturetiming changes. Coinbaseholduntilreviewedpass.

ActualproductionTSdispatchtrace /tmp/phemex-trace-ts.log proveswindowcarry:
first1000mstimerwall567.001; productsdispatch570.526,response578.945;
tickers592.524/592.774; clock.startwall699.047 leaves868ms. BTCsnapshot
dispatch228/completion+callback252 slot3; ETHslot4 dispatches868, recorded
response266 alreadypast, callback868. InstrumentedtracefirstETHflush1200
vsuntraced784 exposesrealstartupremainingtimer dependence, notHTTPpromise
drain. Nativebudgetusescallback-anchored hosttimer preservingstartupcarry;
no freshvirtual0window. Sourceunchangedfortrace, scoresnotreused.

NativePhemexbudget independentimplementationreview PASS (exactroutebypass,
FIFO/callbackwindow/norefund/freshsignature/stalepending/429pause semantics).
Rebuiltmodule730,910bytes, actionsregression+private16framegate PASS.
Nativefunctionaltrace534frames/errors0 confirmsnewqueueenforced: products
wall1921.161, tickers2386.551, clock.start2557.583; BTCvirtual228/252, ETH
quotareset+dispatch363. Thusrealbootstrap~637ms leaves363ms, vsTS132ms
leaving868ms; originalremainingtimer carrycauses128vs126bookevents even
withmatchingratebudgetalgorithms. No freshquotareset/hashdrop/seedwarmup
barrier orpervenueoffsetallowed. Independentread-only clockplan audit now
assessesgenericdeterministiclogicalstartup drivenactualreadiness/transport
whilecountingallconstructorCPU/memory. Preserveprotocoltimerdurations,
recordedHTTPcausality, signingcontinuity andallpriorfixtures. No clocksources
changedyet; fullnativequota+fixturetestverification stillinprogress.

Phemexnative correction COMPLETE/reviewed:93tests (86original+7directquota/
FIFO/freshsign/stream/staleread/retry/startupcallbackcarry/429pause controls),
bans/fmt/strictpackagealltargets+WASMClippy PASS. Independentfinalaudit
confirmsfixturehelpers preserveALLEmittedactions/clearedtimers/assertions,
fireearliestactualtimeronlywhenrequiredHTTP/CommandDoneabsent,100bound
fail-loud; noquotareset/refundhooks. Private16framegate/actionsPASS and
module730,910bytes. Commitnativefixseparately beforeharnessclockcorrection.

### Public startup clock correction plan

Read-onlytwo-agent audit approvespublic-only logicalstartup (privatepaced/
continuoussigningunchanged): activate afterrealsettledmemory+CPUbaselines
andbeforemeasuredconstructors. One serializedscheduler, synchronousreadiness
predicates, fullnativeHTTP/TSadapterbody ownership+explicitexternalprepare
blockers distinguishrunnableI/O fromselectedfuturerecordedresponse waits.
Preserve firstdispatchtimerduration andgenuine100mssetuplogicaltime; rebase
onlyremainingLOGICALduration atplaybacktransition, neverfreshresetquota.
Inlinelifecyclemustnotadvancepastnextundeliveredrecord; recordedresponse
times/availableAfter, everyframe/tick/count, typedauth/writes preserved.
Regressionsmustfailoldwallcarry withunequalCPUcostsametimertrace, retain
100mssetup→900msquota, rejectearlyresponse/readavailability, awaitslow
stream/asyncprepare beforewatchdog, preserveBitunixopens/reconnects/bootstrap
andprivateclock+metrics. NoN-idleturn/quiettimeoutguess, snapshotwarmup,
hashdrop, pervenueoffset, orperformancepolicychanges. Independentreview
andallhelpers+actualprobes requiredbeforeunchangedpublicgates.

Clockphase initial441+6focusedPASS butindependentproductionreview finds
three blockers: Axios transformed-error/laterretryinterceptor couldleak or
deadlockworktoken; socketForlegacyclock.wait createsunboundednestedpump
pastopenprefixceiling; timeoutsdidnotcanceldrainloop (932observercalls after
rejectionrepro). SourceWIP, fixes/regressionsrequired, no perfgatererun.
Primaryrecord.mjs27/29/31 plus109/113 provesepoch createdBEFOREconstructors.
Reviseplanaccordingly: readiness endsstartupmode withabsolute logicaltime/
deadlinesUNCHANGED, notper-implementationtranslateelapsedbackto0. Genuine
setup100ms leaves900toabsolutequota1000; nextrecord228 advances100→228.
Allpublicfixturesmustconsumeinitialrecordprefixchronologically whilewaiting
readiness, notlegacyunboundedawaitready pastfirstrecord. Privateoldpath
unchanged. Two-agentplan audit agrees; owner implementing withrecordceilings
andexplicitIOcancellation, no timings/hash/thresholdnormalization.

Clockcorrectedfunctional PhemexpairPASS:534frames/126bookevents each, entire
timestamp/hash arraysidentical/errors0 (/tmp/phemex-logical-{ts,rust}.json).
No normalization; absolute recorderorigin+chronologicalprefixexplainfix.
Nine focusedregressions PASS; independentintegration reviewer independently
9/9PASS1.616s andoriginalrealAxioslostconfig401 repro nowwork0/no watchdog
atlogical0. Nativefullbody/TSadapterparseownership, selectedfutureeligibility,
privateundefinedclock/continuoussigning andcostbaselines auditedunchanged.
Full450helperrun+Bitunixexplicitlifecyclefunctional smoke andsecondreview
remainbeforepublicperformancegates; source stillunstaged.

Publiclogicalstartup phase COMPLETE/reviewed:451helpersPASS7.16s,10new
regressions includingall4adversarialcounterexamples (responseinterceptor
retry/errorownership, nestedpump/prefixceiling, canceleddrain, observer-created
asyncrace). Bothindependentreviewers PASS. Synchronousreadinesspure: native
host.flush removedfrompredicate; declared250msonTickegress unchanged.
Singleoriginal15sTOTALstartupdeadline preserved acrossper-frame waits.
FinalfunctionalPhemexTS/Rust exact534frames/126events, fulltimestamps/hash
arrays/errors0; latestBitunixTSexplicitlifecycle28,694frames/150events/
697markets/errors0 completes~2.5s. Privatepaced/signingpath untouched,
beforeconstructorCPU/memory baselines andgateformula/thresholds/peak unchanged.
Three-runPhemexpublicgate nowrunning afteralllocalchecksquiet. Checkpoint
reviewedclockcorrection separatelybeforegateoutcome.

Phemexunchangedpublic3rungatePASS onreviewedclockhead d7667362c5:
534frames/891markets/324tickers/2books/all126bookevents. TS/Rust main
539.7/284.1ms, steady386.2/155.7ms, retained6.2/4.5MB, peak37.6/9.8MB;
module730,910/1,258,291bytes. Private16frames/2commands andactionsPASS.
CrossvenueBitunix+Binance rebuiltcontrols follow before nextlanding merge.
Fetched5d7589f296 nowincludesCoinbase5c0e437faf,PolymarketPR1327 andGRVTPR1321.
Threeownedread-onlyplan audits running; nextmergewillcombineallthreeafter
controls, retaininglogicalstartup451backbone andallpriornativeguards.

Bitunixrebuiltcontrol module792,113bytes; private42framesPASS. Firstpublic
controlFAILS atTS Startupframedelivery deadline (beforeRust replay), source
d766/currentNode24.21.0. Instrumentedrepro rulesoutNodeversion/topic/I/O
leaks: frame3930/socket2DELLUSDThasTopictrue,work0/holds0/pendingResponses0,
nexttimer4008, pacedsubscriptionsfinish4258logical. Newtotal15sstartup
wallbudget exhaustedwhileprogressing~5411frames×3two-idle-turn scopes,
~30kidledrains. Boundedgenericcoalescing/settlement-checkpointoptimization
underindependentreview; keepIO/newproducer invalidation, allcounterexamples,
logicaltimeline/deadline/thresholds unchanged. No gate rerununtilchanged
source verified. Coinbase/Polymarket/GRVTmergewaitingcontrols.
Combinednextmatrix25venues/29public/54private+oldBitget4/Aster3; all35CIchecks
exactheadgreen beforefinalmerge.

StartupcoalescingphaseCOMPLETE/reviewed:455helpers and14independent focused
regressionsPASS. Dense1,001recordactualHEAD uses3,001barriers; newsource
<=1,007 withALLrecords/ticks preserved. Settlementcheckpointinvalidatedby
allproducerwork/holds/timers/observer/delivery/command/genericontick; Tapeonly
fastpath requiresnoholds/IOblock/duetimer, genericadvance remainsguarded.
Queuedautoopen/ACK/pong/worker-callback SYNCinvocation explicitlyowned;
notawaitprotocoldependentPromises. Adversarialoriginalautoopenprobe now
openAt0/readyAt0/holds0. Node24BitunixTSfull28,694frames/150events/errors0
passesunderconcurrenthelperload+same15sdeadline. No protocolduration,
timeout/hash/metric/threshold edits. Source released; unchangedBitunix3run
thenrebuiltBinanceprivate/public controlsrunningsequentiallyCPUquiet.

Bitunixrebuiltcontrols onreviewedcoalescedclockPASS:42frameprivate and
unchanged3runpublic28,694frames/697markets/tickers/2books/all150events.
TS/Rust main914.3/824.5ms, steady616.4/464.2ms, retained3.8/2.4MB, peak
31.7/16.4MB; module792,113/1,258,291bytes. No protocol/hash/deadline changes.
Binancerebuiltcontrolsrunsequentiallynext; no perfgate/buildoverlap.

BinancerebuiltcontrolsPASS: private14frames/2markets andunchanged3runpublic
873frames/917markets/783tickers/2books. TS/Rust main247.4/148.8ms, steady
135.4/69.1ms, retained4.2/2.4MB, peak33.7/12.8MB; module762,710bytes.
Bitunix/Phemex/Binance controlsallgreen; checkpointbeforenextcombinedmerge.

### Combined landing: Coinbase, Polymarket, GRVT

Merge reviewedorigin5d7589f296 aftercontrols (briefre-read;processunchanged).
Plan nativepreserves83JSON/binary/scratch/identity/reservationguards; add
accountID andpredictionsections13/14/pruning inlockstepwithJS. Reserve
predictiontables plus6metadata strings/market inclUTF8bytes with0/63/64
unique/shared/longstring no-reallocproofs. New285venue+3engine+2Bybittests
expectedbeforeourproofs; fullnativechecks, preserveallpriorvenueproduction.

Currencycompatibilityplan revised fromearlierincomingheaderproposal: old
Frame.balance readslegacyrefs+4 andused/free/total/upnl, ignoresfooterbytes.
Incomingunversionednewheader+reorderedcolumns ambiguousatN2/samelength.
KEEPoldlayoutwhole; appendvalidated8-byteversionedmagic+flagsFOOTERonlyfor
newexplicitCoinbaseStoreopt-in/defaultfalse. Oldreadersdecodecorrectvalues,
newreaderrecognizesexactfooter/knownbits; absencevsSome[]collateral retained.
ReservetheOLDtablebytes+8onlywhenfooteremitted. Frozenlegacy0/1/2/63/64
bytes,crossmodeallocation,distinctcolumns, malformedfooter/sectionboundaries
proofs required. UpdateRUST_REWRITEcurrencycontract; noheaderheuristics.

Hosts/SDKunion25factories/aliases inclBitunixsecret/bytesfastpath/boundedraw
transport; CoinbasekeyJSON/mode/routingJWTalias/accountID/CFMbridge, GRVT
route-derivedsessionProxy/uint64CID/globalcancel, Polytypedauth/prediction
main+workerhydrate/clear. CoinbaseCIDrejectcontrolchars/nativeboundaries.
HarnessexplicitShared+typed profiles for3venues, actualsocket/closedauth/
topicstates, officialHTTPSpublicGETscopes, duplicateheaderrejection, nonce/
CIDoriginaltype/routescoping, GRVTsessionactivationonlyonactualHTTPdelivery.
Keep455logicalstartup+strictbooks/candles/outbound/signedmemory/peakbackbone;
rejectincomingconstructorbaseline/final-bookwaivers. Addconstructornegative
probes andallnewvariantgates; nativeupstreampacinggaps flaggedforseparate
quantification, no retimingwithoutconcretegatecausality.

Independentphase+adversarialreviews, fullnative/SDK/helpers androotchecks,
3newvenuebuild/public/privategates precedereadiness. Expected25matrixrows,
29public/54private plusBitget4/Aster3cases; require35exactheadgreenCIchecks.

Merge execution correction: the shared origin ref advanced during planning;
actual MERGE_HEAD is 74b0a7dd14, adding BreakoutProp PR1328 to the three
reviewed ports. Preserve all four incoming ports. Owners expanded their scopes
to its typed polling controls, TS wrapper and harness policy. Expected matrix
is now 26 venues / 30 public / 55 private, plus Bitget4/Aster3 and Breakout
compiled parity cases. Require at least36 exact-head green checks. Future
merges pin the reviewed SHA explicitly. Root README resolves to a single
Bitunix build example; native and host production released for Coinbase build
while independent owners finish tests and harness adaptation. No gate runs on
partially merged harness source, and no source-gate thresholds are changed.

Native/host integration review: engine88 proof tests, strict workspace native
clippy, bans/fmt, host84, focusedSDK315 and actual CoinbaseWASM10 PASS.
Root44typecheck/28lint PASS; full repository unit run still in progress.
Independent reviewer found Coinbase native accepted FEFF inside caller IDs
while ECMAScript whitespace rejected it. Whole-batch regression fails before
fix by observing a POST; native now rejects FEFF alongside controls. Full
Coinbase114 tests and strict package clippy PASS after fix. Rebuild its artifact
before replay. Root owns full workspace test/WASM-clippy completion, since the
released owner's background session did not survive turn completion; its
partial test log is not a completed gate. Protocol auth work delegated into
disjoint CB/Poly/Breakout modules while shared replay/Tape owner completes
polling and GRVT delivered-response credential activation.

Root-owned fullnative gate COMPLETE: 2,375 passed, zero failures; strict
workspaceWASM clippy PASS20.92s. PackageCoinbase114 already includes FEFF
regression. Native/host/SDK independent review PASS, wire16/16 independently
rechecked; no further findings. Fullroot test-ci safe-cex505files/7,326tests
PASS; root scheduler still completing other declared tasks. Combined26-venue
artifact/WASM bridge suite now building, separate from public CPU gates.

Combined26-venue WASM build PASS2m40s; all7 compiled SDK files /106 tests
PASS25.72s. Root shared-harness review caught private/no-clock session audit
activating credentials before body delivery; production now explicitly opts
into deferred audit independently of ReplayClock. GRVT production TS private
refresh also exposed fake WebSocket localclose missing standard close event;
owner added once-only queued event and actual refresh probe now passes all5
commands with no protocol/unserved errors. Independent fence review found
idle64 counted useful timer progress, rejecting a valid100ms poll at84ms
under1ms housekeeping; fix preserves64/8192 limits and counts actual idle.
Harness aggregate and all8 Coinbase TS private profiles remain in progress.

Root full repository quality COMPLETE: all27 test tasks PASS10m46s, alongside
44typecheck/28lint tasks. Final shared/protocol adversarial review PASS after
fixes: 59protocol/production-wrapper,40logical-startup/wrapper and14Poly checks
independently green. Fixed generic typed matcher ignoring explicit preservation
of unknown signature/clock/client-ID fields for the four new profiles: valid
HMAC cannot normalize a changed cancellation into the recorded ACK. Verified
entropy stays scoped to independently authenticated nested fields. Poly public
credential leakage regression fails before, passes after across16routes×7
private header names; actual production publicTape rejects before matching,
then clean POST/books succeeds, no unserved requests. Coinbase public signature
checks now also run on the actual public Tape path. These findings are closed;
aggregate605 passed before the added Poly case, final606 run pending. No CPU,
memory, timeout, frame/hash or peak thresholds changed. Final new-venue gates
and Bitunix/Binance rebuilt controls follow source checkpoint.

Actual TS functional integration COMPLETE: all8 Coinbase profiles each61frames/
15commands, GRVT18/5 including session refresh, Poly20/8, Breakout24/6; zero
unserved, unused writes, signature or socket protocol errors throughout.
PublicTS profiles also PASS with two books each: Coinbase201frames,GRVT4695,
Poly182,Breakout15. Breakout private has a playback clock without public
startup mode: preserve real subscription preparation in that case. Axios
relative paths now use the correct slash when joining dxsca-web/marketdata,
with production adapter regression. Final aggregate adds these controls and
fallback canonicalBody preservation; 607/608 first run isolated that fallback
assertion (typed production already preserved fields), owner correcting it
without removing the regression. Final608 and reviewed merge checkpoint next.

Combined landing integration RELEASED: final609/609 helpers PASS, all4 public
and11 private TS functional variants PASS, no background probe jobs. All merge
conflicts resolved and staged. Prior independent native/host/shared/protocol
reviews PASS; final small fallback/subscription/URL-join deltas receiving a
last bounded independent audit before gates. Coinbase rebuilt991,290bytes.
Checkpoint this reviewed source, then run unchanged three-run public gates,
all8Coinbase private gates and GRVT/Poly/Breakout private/compiled parity,
followed by rebuilt Bitunix/Binance controls. Preserve26/30/55 matrix and all
gate thresholds; only exact-head green CI permits landing PR1309.

Final three-delta independent audit PASS:10/10 targeted tests and exact/changed
signature fallback probe. Legacy normalization defaults, logical startup
observers and repeated serialized query parameters remain intact. No findings
or background jobs remain. Source review closed; start rebuilt gates now.

Reviewed merge checkpoint cb9e684e68. Coinbase public three-run gate PASS:
201frames/925markets/tickers/2books, module991,290bytes. TS/Rust main
919.0/436.2ms, steady562.1/190.5ms, retained18.2/14.1MB, peak103.7/46.2MB.
First INTX ES256 private gate stopped at independent oracle assertion
"missing Coinbase held currency oracle". Retained raw outputs show both
balances exactly {used:200,free:800,total:1000,upnl:20,currencies:{}} with
61frames, zero unserved/unused writes/protocol errors. The incidental portfolio
breakdown contains spot_positions but selected portfolio is INTX, for which
TS never publishes spot currencies. Fix only input-proven INTX exclusion;
CFM remains strict, including futures-only metadata and empty output. No gate
rerun until failed-before/passed-after oracle tests and independent audit.

INTX oracle fix implemented using only recorded breakdown.portfolio.type;
no profile, product-list or empty-output shortcut.23 focused tests and full611
helpers PASS. Actual regression failed before; CFM missing-row negative remains
strict even with zero SPOT products. After correction, the original frozen
failed TS/Rust pair matches all10 state/book/request fields, and all8 saved TS
private profiles pass the input oracle offline. Independent bounded review
pending before commit/resume. Public Coinbase path and all native source are
unchanged, so preserve its already green three-run gate and rerun private only.

Independent oracle audit PASS,23/23 tests independently rechecked. No Rust
input, balance overwrite or comparison waiver; exact TS portfolio-mode
behavior confirmed from frozen source. Commit and resume private gates now.

INTX ES256+EdDSA private gates PASS after reviewed fix14d2da5f73; CFM and
remaining profiles continue sequentially. Current source remains frozen for
the batch. Read-only landing check found pinned e391337054d5591ff2a253ed75162d7fefa20d4d:
Binance variants PR1326 plus GRVT profiling docs-only PR1331. Finish current
gates before mutating source, then merge this exact SHA (not a moving ref).

Next additive integration plan: Binance native auto-merge (no ours-only native
diff since74b), preserve entire engine/crypto/Cargo and all JSON/scratch/BCM1/
prediction reservation guards. Default USD-M and PM behavior retained; Spot
and COIN-M branches add70 tests,29→99, retaining oldnames. Host adds explicit
BinanceVariant field and Spot/inverse factories under existing Binance feature,
still26 crate features. SDK adds aliases/Spot CID rules; keep stricter Coinbase
Unicode rule and all Bitunix secret/transport lifecycle guards. New inverse
contract-size conversion must reach both worker and main FrameApplier paths,
with other inverse venues keeping factor1 and Bitunix unaffected. Preserve
old currency bytes: Binance explicit coin/currency Some tables emit no BCM1.

Harness merges Binance protocol/private tapes/live fixtures into the existing
611 strict backbone, preserving actual socket identity, typed auth/body checks,
constructor-inclusive signed memory and every book timestamp/hash. Review
production delegation/generator typing/duplicate auth before source checkpoint.
Expected CI remains26rows/36checks but now33public/58private profiles: three
new public+private Binance modes (Spot, COIN-M, PM). Rebuild Binance once for
allfour mode gates plus newBitunix control after merge, run scoped/native/root
quality checks and independent phase/adversarial reviews, post findings and
push only when local gates pass. No speculative GRVT perf code to adopt.

Current batch stopped after six Coinbase private gates passed. Dedicated
coinbasespot ES256 private functional output matches after documented input
corrections, but TS memory validation rejects net -575,120bytes: heap
26,007,384→25,432,232 (-575,152), buffers+32, WASM0. Raw artifacts retained at
out/coinbasespot-coinbasespot-spot-ES256-private-jsonl-86893; failed log copied
to /tmp/bitunix-failure-coinbasespot-es256-negative-retention.log. No retry,
clamp, negative-total waiver, retiming or baseline movement authorized here.

Forensic import-only program keeps all five loaded TS namespaces and creates
no exchange/transport/replay. Same100ms quiet/1s sampler initially reports
25.95MB heap, then23.08MB with constant backing buffers; new_space falls
2.93MB→8KB while old/code spaces stay stable. Thus a reclaimable loader young
heap can enter the preconstructor baseline. Official Node24.21 and CI22.23
V8 GC extension confirms sync collection conservatively allows native-stack
heap pointers; asynchronous major collection runs as a nonnestable task with
no conservative stack roots and resolves its Promise after collection.
Investigating awaited async fullGC at the SAME existing collection point,
with unchanged quiet/deadline/formula/thresholds. Fresh-process controls must
prove improvement before source changes or any gate retry. Sequential probes
whose sync baseline was already clean do not establish causality.

Quota-reset resume: checked cleanstatus, current a2a7498287 and PR1309 OPEN
against feat/rust-rewrite (remotehead6b2d; old WOO failure alreadyfixedlocally).
Fetched and began merge of pinned0063068241646a542ca0b936f75cb5ec6de2907c,
which supersedes planned e391 with nativeframe-server, consumer cutover and
live-smoke ports as well as Binancevariants. ActualMERGE_HEAD verified; no
stash/rebase/master. Native owner handles engine/venues/crypto/nativehost,
hostowner WASM+SDK, harnessowner scripts/workflow+collector. Preserve all
prior guards and landing structure; incoming native transport cancellation
must retain our16MiB raw/decompression bounds. LocalWASM fullbooks remain
default; bookdelta producer is native opt-in. New shared command decoder
replaces duplicate host parsing while retaining all venue command contracts.

Fresh async-FIRST collection control completed: same import-only script with
awaited asyncmajor GC baseline22,696,824→22,751,536 at30s (+54,712), backing
buffers constant; prior sync control -2,873,024. Independent review approves
bounded helper fix in principle; implement awaitcollection at SAME point,
same100ms quiet/1s totaldeadline (including rejected/hungcollectors), no
formula, negative-total guard, peak, baseline location or threshold changes.
Revalidate metrics after merged source stabilizes rather than continuing the
old failed batch across changing source. Root combined artifact and repository
quality checks now running; all final gate claims await rebuilt source.

Pinned006 integration source released after owner reviews: 2,473 workspace
native tests /198groups PASS, default engine92 separately PASS; bans/global
format/strict native-all-target and WASM clippy PASS. Combined26-venue build
PASS; host89/89 and harness737/737 PASS. Root44typecheck/28lint tasks PASS;
root tests remain running. Actual Binance USD-M/Spot/COIN-M/PM private TS
fixtures all match writes/auth/socket contracts (14/8/6/6 frames,3commands
each). Incoming consumer test encoder corrected to the frozen legacy currency
format; production BCM1 bytes unchanged. Independent native/SDK review PASS;
remaining shared harness review is in progress.

Fresh combined WASM suite384/385 PASS: Paradex realVenue consumer helper
chooses books inconsistent with recorded subscription acknowledgements; fix
the helper rather than weakening protocol validation. Original CoinbaseSpot
ES256 TS-only producer still has negative total AFTER async collector fix:
heap28,336,864→26,012,576 (-2,324,288), buffers+32, net-2,324,256. Raw proof
/tmp/bitunix-006-coinbasespot-ES256-private-ts.json. Thus helper correctness
and import-only control do not prove the actual producer fixed. Retain this
failure and investigate real loader lifetimes; no actual gate retries,
negative-total waiver, quiet/deadline changes or warmup added.

Merge checkpoint: all conflicts resolved and reviewed. Paradex cutover's
fresh failing case now PASS after actual captured-book subscriptions and
ordered validated waits; scoped typecheck/lint PASS, full44case replay pending.
Independent shared harness review67+2PASS, no new source findings. Root SDK
test-ci7349/507files PASS; duplicate test-unit/remaining repository tasks still
running. Native frame-wire1/1 and mock live-smoke9/9 PASS. Memory diagnosis
continues separately; this checkpoint does not claim final replay gates PASS.

Actual producer diagnostics distinguish collection from lifetime variance.
Exact ESBUILD_WORKER_THREADS=0 preconstructor samples: regular asyncmajor
25,965,576→23,022,472 after heap snapshot, reclaiming2,943,104 (young1.59MB,
old640KB/trusted672KB; bytecode1,786,400→1,227,768). Asyncmajor last-resort
21,161,888→20,911,904, reclaiming249,984 almost entirely young heap allocated
during the pause AFTER collection. Full last-resort plus pause BEFORE awaited
collection/immediate sample20,884,888→20,893,688 after snapshot (+8,800 from
diagnostic allocations, no residual reclaim; young2,352→1,440). Logs/snapshots
/tmp/bitunix-006-{regular,last-resort,last-resort-immediate}-residual.* retained.

Official Node24.21/22.23 V8 gc-extension.cc regular major uses
PreciseCollectAllGarbage(kNoFlags), while last-resort uses
CollectAllAvailableGarbage. The latter clears serializer/compilation-cache
roots and performs bounded root-stable collections; heap snapshots use the
same path. Snapshot's old/trusted drop is cache/root cleanup, not proof of
bytecode aging. Thus asyncmajor alone was insufficient; full collection plus
immediate postcollection sampling removes the proven initialization residue.
Implement only those sampler corrections symmetrically before construction
and after replay. Preserve all signed deltas/nonnegative-total validation,
100ms backing-buffer quiet/1s total real deadline, CPU/peak callsites, module
budgets and equality rules. Independent source audit and actual failing-profile
verification still required before restarting gate batch.

Sampler correction staged/reviewed:17focused and739aggregate helpers PASS;
pause-created-allocation regression fails prior source and passes correction.
Independent exactdiff audit PASS. No remaining sampler source finding. Cache
eviction at BOTH endpoints may affect subsequent compilation cost; fresh CPU
gates remain mandatory. Sources: Node24.21 gc-extension.cc169–194 and
heap.cc1239–1287; Node22.23 heap.cc1470–1513; V8 snapshot-generator3032–3044.
Root quality COMPLETE:44typecheck/28lint/27test tasks PASS, SDK7349tests in
507files twice. Compiled bundle385cases covered: prior384pass plus repaired
realVenue44/44 rerun; host89 and native2473 PASS. Coinbase single-venue build
PASS11.32s. Begin new final gate batch on frozen CoinbaseSpot ES256 failing
tape first, then rebuilt public/private controls. Historical failures retained.

Final adversarial candidate90712 review found two P2 defects; fix before push.
Bitunix private UPDATE can regress cumulative filled2→1/remaining1→2, and a
stale CANCELED CLOSE after FILLED CLOSE can regress Closed→Canceled and
filled3→1. Plan: authoritative monotonic cumulative/timestamp/terminal fences
before state/native-protection mutation, preserving late advancing terminal
fills, per-execution emission and net PNL. Add deterministic fail-before
regressions for reordered UPDATE/CLOSE plus legitimate advancing terminal fill;
update documented TS correction, run venue fmt/clippy/tests, independent audit.

Recorder lost its sticky sanitizer failure flag: EngineHost can catch a later
sanitization exception as httpFailed while earlier readiness stays true, so
record.mjs exits0 despite omitting rejected input. Restore fail-closed latch,
safe diagnostics and a host-level post-readiness rejection regression. Run
focused recorder/helpers, independent audit. Neither changed path participates
in current Coinbase replays; finish these fixes before the batch reaches its
future Bitunix build. Record each finding/resolution on PR1309 after pushing.

Same adversarial review also found P1 split conservation: amount250/max100/
step0.001 emits66.667×3+50=250.001 (step1 emits67×3+50=251). TS has same
ceil-lot divisor error, so this is a documented Rust correction. Plan native
integer-tick max-sized lots plus exact remainder, each≤market max and aligned
to step; bound total resulting lots≤10,000, preserve full batch preflight and
caller-ID split rejection. Prove nonmultiple/awkward-step conservation and
boundaries with fail-before cases; independent venue audit after scopedchecks.

Recorder fix reviewed PASS: restored sticky failure covering timeline,
sanitization, serialization and write invocation, fixed safe diagnostic and
nonzero exit even when prior readiness remains true. New production-slice
tests use real EngineHost buffered+streamed paths: fail-before740PASS/1FAIL
(incorrect exit0), after741/741PASS. Includes safe successful control,
credential/serializer rejection, request cleanup, no unsafe record/diagnostic
leak and later-safe-write stickiness. Logs /tmp/bitunix-recorder-{before,final}.log.
All eight Coinbase private profiles + public PASS on reviewed sampler. Batch
launcher alone paused after final replay completed to finish native fixes;
no replay process/timer or gate policy paused/changed. Continue remaining
venues only after native correction and independent audit.

Final native fixes frozen/reviewed PASS:68Bitunix tests (8added) plus strict
all-target clippy, crate/global fmt, bans and strict WASM clippy PASS. Logs
/tmp/bitunix-final-fixes-{before,tests,clippy,fmt,bans,globalfmt,wasm-clippy}.log.
Three original account race regressions and split conservation fail before.
Presence-aware Option<Num> differentiates explicit0 from omitted dealAmount;
monotonic execution facts advance once on late UPDATE/CLOSE while timestamp,
price, sibling TP/SL metadata and Closed/Canceled terminal state remain fenced.
Tests include duplicate late fills, no-mtime sparse edits and virtual siblings.
Splits use exact integer ticks/max-sized lots/remainder, validate all resulting
min/max sizes, total≤10k and the whole batch before HTTP. Decimal0.3/0.1 and
quarter-step conservation, invalid later batch and count boundaries covered.
TS defects documented in KNOWN_TS_FIXES Bitunix branch; net realizedPNL
unchanged verbatim. Exact final independent reviewer approved latest source.
Resume paused outer batch after checkpoint; rebuild Bitunix only when reached.

Resumed batch: all Coinbase8private + public PASS, Poly/GRVT public/private
PASS. Coinbase public TS/Rust main934.8/443.9ms, steady573.1/187.4ms,
retained18.0/14.0MB, peak109.5/45.4MB; Poly51.9/46.6ms,
retained0.9/0.8MB, peak2.9/2.7MB; GRVT450.5/355.1ms,
retained1.8/1.5MB, peak6.8/4.0MB. Modules CB991403, Poly839576,
GRVT949615, Breakout638817 all below unchanged1258291byte budget.

Batch then stopped at Breakout public functionalFAIL in all3 runs. Private
PASS24records; public finalstate/books/requests and CPU/memory PASS, but
intermediatebooks TS18 first4000ms vsRust22 first0ms. TS4000hash equals
Rust2000hash, TS6000equalsRust4000, so initial snapshot omission and ongoing
one-anchor callback delay must be diagnosed separately. Rawout
sdk-rs/harness/out/breakoutprop-breakoutprop-public-jsonl-gz-66084 retained,
failedlog /tmp/bitunix-reviewed-006-breakoutprop-public.log. No hash/event/
timestamp waivers, retries, record changes or retiming. Harness owner tracing
actual TS request-selection/completion/callback/onTick; independent owner
inspecting TS initial listener semantics vs native. Bitunix and Binance gates
not reached; no final gate/PR push/merge claim.

Breakout trace disproves a clock/settlement defect: duplicate concurrent
getAccountCode GET/users makes startup6requests against5RPS, so actual TS
initial quote completes1000ms and periodic reads3000/5000. Native coalesces
account lookup, initial0/periodic2000. Independently, TS stores initialquotes
without notifying already-registered book listeners, whereas native explicitly
publishes the initial REST quote (covered by native public regression). Docs
describe synthetic REST quote books but do not promise late cached callbacks.

Choose bounded TS corrections: coalesce in-flight account lookup with safe
rejection/disposal/session-selection fences; publish fetched startup quotes to
EXISTING book listeners so first valid REST publication is delivered. Keep
late subscriptions waiting for the next poll (no new cached-listener feature),
native initial-quote contract and both original2000ms poll intervals. No
harness/timestamp/hash/fixture/clock changes. Document this explicit initial
publication improvement nearby and in known-fix comments. Add failing-before
duplicate-discovery and early-subscriber snapshots/rejection retry/disposal
controls, run package typecheck/lint/targeted+complete deterministic tests,
independent source audit, then repeat only failed Breakout gate+parity and
remaining Bitunix/Binance batch on rebuilt artifacts.

Next landing integration plan reviewed at pinned86ed5e3d2e: TxFlow1329,
server adapter1334, GRVT replay1335, Lighter variants1336. Expected union
27venuecrates/31workspacecrates/46SDKaliases/28replayrows,35public and
62private gate invocations,38CIcontexts. Incoming does not change engine,
crypto, Bitunix or native host production. Preserve our Lighter stable
duplicate-price aggregation/regression when layering15new Lighter tests
(expected103 total), plus73TxFlow tests (expectedworkspace2569).

WASM/SDK/server merge keeps all Bitunix secret/factory/transport/BCM1 guards
and adds TxFlow typed testnet/key/secret/applicationId/vault plus Lighter
Robinhood/remote-signer/provider options. Serveradapter additive real-WS
fixture requires focused native+TS checks. Root artifacts only; rebuild27
feature combined bundle and feature isolates after owners release source.

Shared harness keeps our strict backbone, fullGC+immediate sample, absolute
startup chronology/ownedI/O, constructor-inclusive cost, all book/candle/hash/
outbound fields and sticky recorder failure. Layer accepted bounded GRVT
per-socket hostname/prefix lookup caching while retaining actual-object state
keys/closed/session-delivery guards. New TxFlow typed projection must be
route-scoped after rawauth with duplicate headers/query/origin guards, caller
IDs/unknown fields retained and peractualsocket business-subscription proof.
Private virtual clock is explicit TxFlow-only; other private signing unchanged.

Incoming finalizer correctly seeks logical-end output before asyncGC but
charges structuredClone serialization and retained comparison tables inside
measurement, unlike our existing boundary. Independent plan review approves
output-artifact spooling: capture CPU/thread/steady/wall/WASM/peak FIRST at
the existing end boundary; synchronously v8.serialize full comparison output
to a temporary file in an unwound helper; retain entire live worker/engine/
store through unchanged GC; deserialize aftermeasurement and then existing
normalization; finallycleanup on every path. No comparison objects/buffers
held across sampling, subtraction/clamps, product disposal, warmup or deadline
changes. Freshly recheck sticky transport/auth/signature/socket/unserved and
unused-write failures after GC; never accept incoming late-error masking.
Tests must prove live mutation freeze, typedarray/Infinity/undefined fidelity,
usage-before-projection, cleanup errors and late integrity failures. Prior
gate results remain historical after shared source integration; final rebuilt
controls and exact38-check CI still required before merge.

Breakout correction frozen/reviewed PASS:43focused cases (13new), three
fresh fail-before baseline controls, independent13/13 and original ABA probe.
Full deterministic SDK7362tests/508files +5lint-rule tests PASS; package
typecheck/lint/whitespace PASS. Source adds identity/selector/active-request
ownership and disposal fences with own-promise cleanup, retains transparent
401 recovery, and emits initial fetched quotes only to existing/live book
listeners. Store-notification and callback reentrancy cannot publish stale
data; late/new subscribers keep next-poll behavior. No native/harness/clock/
timestamp/fixture edits. Logs /tmp/bitunix-breakout-{startup-after,startup-fail-
before-final,full-safecex-tests,safecex-types,safecex-lint,independent-review}.log.

Own git fetch nowpins02e9db0e8bbd1eaaab021cdfb7b63c7bc3d54722, which
adds GRVT-products1337 docs/tests+fixture ONLY after reviewed86ed; no GRVT
production change. Brief reread from origin Process unchanged. Merge this
exact fetched SHA after checkpoint, keeping every documented local correction.

02e9 integration native phase reviewed/released:2576 tests in208groups,
zero failures/ignored, strict native and WASM Clippy, locked workspace check,
bans/globalfmt PASS. Engine/crypto/Bitunix/native host production remain
byte-identical to reviewed local HEAD; Lighter duplicate-price aggregation
and all68Bitunix regressions preserved. Combined27-venue WASM build and
89host tests PASS. Root44typecheck/28lint/27test tasks PASS; serveradapter
real-local-WebSocket/native-session/SDK decoder integration PASS1/1.

Harness audit caught invalid chronology in incoming documented synthetic
TxFlow private generator: closeNODEsocket0 at7500, but replacement socket1
frames8800–9200 precede actual TS reconnect9500 (BaseWebSocket delay2000).
Native first reconnect8500 (delay1000) does not make the TS fixture valid.
Old socket waiting silently advanced beyond recorded event time and weaker
subscription checks hid it. Plan: verify actual per-subscription pacing on
both paths, move only synthetic replacement events after required actual
subscriptions, retain every payload/order/count, document evidence and add
strict old-chronology rejection controls. Keep production reconnect timers,
logical replay clock, thresholds and comparison rules unchanged. Independently
review finalizer/typed authentication/socket proof/SDK boundaries, then freeze
core source before compiled fixture tests and fresh feature-isolated gates.

Public TxFlow audit exposes a separate real nondeterminism: native API ticker
subscriptions iterate HashMap assets, TS reference uses metadata order, and
live recorded ACK/data order reflects that particular native HashMap seed.
Example BZ instrument65 ACK at12301 precedes current TS wire subscription
around21000; another Rust process can choose yet another order. Do not bind
adapters to captured random order or waive subscription-before-data proof.
Plan bounded production correction: queue native and TS-reference ticker
subscriptions by ascending explicit instrumentId, retain150ms pacing/bounds,
prove scrambled-metadata and reconnect ordering with fail-before regression,
independent audit, rebuild and record a NEW actual public20second/two-book
fixture. Preserve original fixture provenance/hash and failure evidence in
progress; no synthetic public timestamps/payloads, padding or hash waivers.

Private recheck exposes active-clock queued-callback ownership defect too:
advance fires reconnect9500 but jumps target10000 before queued open runs,
so actual150ms subscription pacing starts10000. Startup already owns queued
callbacks; extend exactly that synchronous callback lifetime to active clocks
and drain owned callbacks before moving to another due timer/target. Never
await callback-returned protocol promises or active HTTP work (which can need
future timers); keep original real timeout/cancellation and no late advancement.
Prove fail-before open callback sees9500 plus chained callbacks/pacer ordering,
future-timer progress and bounded failure, then independently review and run
all helpers. Synthetic private retiming alone cannot repair scheduler causality.
Original public fixture retained externally:607records/525WS/last31307ms,
gzipSHA58194322cafa0cc40943fe19c44f8fc7fdf300d3f0e26bd8d1106449bee90f19,
rawSHAd21ad5a5ed06faae3adfb7539f6e4be117dc85d2ceb6cb8ea3ed72a61af411a4.

New actual public capture exposes recorder readiness bug before replacing any
fixture: record.mjs tests loaded mask3 (balance/orders), but public readiness
is MARKETS4|TICKERS8=12 (engine wire.rs loaded_flag, store.rs status writer).
Thus recordAfterReady never starts its20second window on a public-only engine
and runs to the140second failure deadline despite all79HTTP seeds/78tickers/
both books arriving. Preserve this failed capture, fix only decoded public
loaded flags and prove actual wire-frame readiness (balance/orders alone must
not qualify). Keep20seconds after genuine public readiness and120second
startup deadline unchanged; independently review, then make a NEW capture.
Observed original process exits0 at140seconds and claimsseconds20/3103WS;
recordingReady's WS branch only checks frames/books, so it also misses the
unstarted window. Require genuinely completed readiness window for successful
recordAfterReady captures, including explicit pre-ready deadline failure test.

Integration corrections frozen and independently reviewed PASS. Native/TS
TxFlow ticker ordering: both scrambled initial/reconnect controls fail before;
74native and53TS cases, strict Clippy/fmt/package types/scoped lint PASS.
Native workspace total now2577 (prior2576 plus this regression). Active replay
callback ownership:19focused PASS, both advance/wait controls fail before;
callbacks keep actual9500 open time and150ms pacing without awaiting future
protocol promises/HTTP. Private generator retains all35frames/15commands and
payloads; replacement events11000..11500 honor actual jittered reconnect
delay[2000,3000) plus150ms paced subscriptions. No production timer changes.

Recorder public readiness uses named MARKET/TICKER wire flags; successful
recordAfterReady requires full requested duration frozen before capture stops,
not after asynchronous disk drain. Actual wire-frame and delayed-end-callback
controls plus sticky rejection checks PASS4/4. All778helpers PASS; independent
source review accepts finalizer, socket/auth guards, scheduler and recorder.

NEW live public TxFlow fixture: actual20seconds after genuine public readiness,
614records (1meta,79HTTP,2connect,2open,530WS), last30350ms; two books,
417ticker updates,78ascending unique ticker ACKs,26book updates,7contexts,
2pongs,zero gateway errors. Trim31seconds retains EVERY raw byte, including
startup; no rewritten timestamps/payloads. gzipSHA
e8ddf222be57620ea86d15fa4a2bbef1bb6c3bebd05f4d6cc32b6208c5ced642,
rawSHA43e480fbbe5363d8032e97d9152a7fbddf3ed019880b0f64dd53aebf5fe44cb9.
Capture/tmp/bitunix-02-txflow-ordered-public.jsonl and siblingrecord.log;
miswindowed original and obsolete randomized-order fixture retained externally.
Final combined27build PASS20.43s; compiled SDK suite running on this exact
artifact/new fixture. Fresh isolated release gates and exact-head CI pending.

Compiled combined SDK PASS415/12files. New fixture strict TS functional probe
then catches the same native HashMap order defect in restored BOOK interests:
NODEopen11235, first ETHbook11476, BTC11629. TS node queue sends allcontexts
atopen, BTC+150, ETH+300; captured ETH+241 precedes its own TS subscription.
Ticker ACK ordering now passes. Trace/tmp/bitunix-02-txflow-public-trace.log.
Do not weaken temporal socket audit or keep retrying recordings for a lucky
HashMap seed. Extend deterministic native-ID ordering to restored book queues
and (nativeID,interval) to restored candle queues, align TS reference, retain
all150ms pacing and active caller subscription order. Prove scrambled initial/
replacement socket queues fail before, independently review, rebuild/re-record
actual20second public fixture and rerun affected compiled checks. Current
614record capture stays preserved as causal evidence, not final acceptance.

Restored ordering correction independently reviewed/released: native76 and
TS55/6suites PASS, including actual automatic reconnect, both socket roles,
private-group ordering, exact150ms pacing and active caller FIFO controls.
Native union now2579 (full2576 plus3 new regression cases). Both native and
TS scrambled restoration controls fail before. Strict native Clippy, package
typecheck/no-ignore scoped lint/fmt PASS. Final global bans/fmt and strict
WASM Clippy PASS after these source changes. No remaining source findings.

FINAL fresh TxFlow public capture PASS20seconds after genuine readiness:
613records (1meta/79HTTP/2connect/2open/529WS), last31233ms,78orderedACKs,
413ticker/30book/6context updates,2pongs,0gatewayerrors. BTC then ETH first
book updates match ordered intents. Trim32 retains every raw byte/startup
anchor. gzipSHAe1416a092d42c0ed3e3390c4f01321772781a82c51956a99dc9cdc6277047712;
rawSHA0a083c906de37c7bac0865df89da4e3ef354be1f948103d2a3ba6a09ee05ba63.
Raw/tmp/bitunix-02-txflow-final-public.jsonl and siblingrecord.log. Strict TS
functional PASS531deliveredframes/78markets+tickers/2books/93bookEvents,
unserved0/unusedWrites0/protocolErrors[]/socketErrors[] closes both original
ordering failures. Final combinedbuild PASS21.97s; full compiled SDK rerun
PASS415/12files on this exact artifact/fixture (56.15s). Performance gates
next; all owned build/test/review jobs complete and CPU quiet.

9f05 isolated gates: TxFlow public/private PASS (531/35frames); Breakout
public/private/four compiled controls PASS; Bitunix public28694/697markets/
2books and private42frames PASS. Bitunix TS/Rust main372.8/295.8ms,
steady207.9/157.0ms, retained3.7/2.5MB, peak12.1/5.0MB, module800232bytes.
BinanceUSD-M public/private and Spot public/private PASS. Batch then stops
Binanceinverse private functionalFAIL: Rust economic amounts/PnL agree, but
raw harness rustAccount omits SDK native-count metadata (remainingNative and
contractsNative) that normalizeAccount already requires on both sides.
Failure/tmp/bitunix-final-02-binanceinverse-private.log retained; no retries.
Plan expose raw native remaining and position notional/declared market
contractSize from Rust's OWN frame for coinNativeAmounts, matching actual
SDK bridge semantics; retain existing inverseNativeAmounts branch. Prove
wrong Rust notional/remaining/multiplier still fails comparisons; no TS/tape
answer copying, rounding waivers or dropping fields. This is a boundary
projection correction, not an economic-output normalization.

Own fetch pins NEW landingf4ea9e06ab11321c0509bd75d05c0486c7820e0f,
PR1339 Hyperliquid/Kraken spot support. Brief Process unchanged. Merge after
stopped batch checkpoint; retain current strict core and every local fix.
Incoming59files, no engine/crypto/Bitunix/native host production change;
existing crates add67tests (HL32/Kraken35) for expected2646 native cases.
27venuecrates/31workspace/27WASMfeatures remain,48SDKaliases/30replayrows,
37public/64private profiles and40CIcontexts. Native branches integrate
additively; SDK keeps all typed flags, Bitunix secrets, rawbytes/BCM1/bookdelta
guards and adds two alias factories plus actual worker cutover cases.

Harness merge plan audited: preserve our KNOWN_TS_FIXES function, add spot
notes as comments/metadata without oracle repair; route both new profiles
through strict typed SharedTape responses/sends (legacy allowlists otherwise
bypass incoming hooks). Keep caller IDs/unknown fields and authenticate raw
requests before scoped transport projection. Correct incoming Kraken nonce
state mutation before HMAC validation, exact generated32hex/AddOrder-only
projection and actual delivered token activation; retain current per-socket
auth/subscription/closed proof on new profiles. Bounded negative controls
must reject forged/duplicate/unknown fields without consuming ACKs. Independent
review each scope; full root/native/compiled union checks, new spot+parent
gates and rebuilt Bitunix/Binance controls before push/exact40-check CI.

f4 phase implementation/reviews PASS: native2646/212groups/zeroignored,
strict native/WASM Clippy, locked check, bans/globalfmt PASS. Engine/crypto/
Bitunix/native host production remain unchanged. SDK164focused/250source
cutover cases, package types/lint and independent review PASS; all48aliases
and27features retained. Root44typecheck/28lint/27test tasks PASS; SDK7519
tests/519files twice. Fresh combined build PASS28.16s.

Harness795helpers and6production-path tests PASS, independent final audit
PASS. Both spots2public+2private TS probes clean: HL67public/10privateframes,
Kraken6166public/19private; all protocol/socket/unserved/unusedWrites0.
Hyperliquid actual server ACKs add l2Book defaults mantissa:null/fast:false;
identity permits ONLY these exact defaults plus absent==nSigFigs:null. One
positive/six negative controls preserve wrong types/unknown fields/precision/
coin distinctions. No output repair or fixture changes. Kraken nonce state
commits after valid HMAC; token state activates at successful delivery;
new profiles use typed SharedTape requests/sends/subscriptions and actual
socket ownership. Original newline-ID validation already rejects that input;
do NOT claim it as a new bug fix. Route-scoped generated-ID projection and
caller/unknown identity preservation are independently proved.

Binanceinverse native/TS functional both6frames clean; native-only metadata
remaining[5,0],contracts7 from700/100. Mutation controls detect any incorrect
native remaining/notional/multiplier, retaining existing economic comparisons.
Final compiled run426PASS/1FAIL of427: stale realVenue KrakenSpot global
initializeReplay call after per-Tape audit migration. One-line helper repair
keeps futures Kraken hook and lets spot own its audit; types/lint/250source
controls PASS, independent review PASS. Affected actual48-product compiled
startup/late-frame matrix rerun PASS48/48 on unchanged artifact (32.95s);
other379 compiled cases already passed. All owned jobs finished. Merge this
reviewed phase, then fresh feature-isolated new spot/parent gates, Bitunix/
Binance four-mode controls and unfinished GRVT/Lighter/Coinbase controls.

40f204 isolated batch stops on parent Hyperliquid public TS StartupFrameDelivery
timeout; private26frames PASS, module1042985bytes valid. Failedlog
/tmp/bitunix-f4-hyperliquid-public.log retained; no retries or source changes.
Exact trace/tmp/bitunix-hyperliq-dispatch-trace.log proves missing native rate
budget: TS meta dispatch0/complete573, fees1000/complete2244, perpDexs2000;
next HIP3 request must wait3000. Captured native HIP3 replies952..2146 and
OPEN2351 precede TS socket construction. AtOPEN, TS sockets=[],holds0,
work0,startupBlockedfalse, next protocoltimer3000. No generic ownership leak;
strict prefix ceiling correctly refuses future work before recorded open.

Native audit/plan next: inspect exact TS per-endpoint weights/FIFO/dispatch
and public/private/spot scopes, native info/exchange callsites and existing
budget patterns. Correct missing production pacing as a typed state machine,
including dispatch-time auth and stale/expired queued-job fences; no JS
emulation, TS limiter shortcut or harness timeline/threshold change. Add
fail-before pacing and queued-job controls, preserve all90HL cases/spotmode,
independent audit, rebuild and NEW actual live public recording after corrected
native pacing. Preserve obsolete capture as evidence; never retime/pad it.
Final release gates/PR push/CI/merge remain outstanding.

Budget phase plan audited against actual Axios callsites: new HLhttp.rs owns
xhr3/1000ms admission plus one weighted FIFO50ms/unit for /info and /exchange.
infoXhr/unlimited bypass ONLY first layer; direct maxBuilderFee fetch bypasses
BOTH. Actual spot API imports the same weighted limiter (old spot.rateLimiter
is unused). Use Engine-owned state consistent with sans-I/O architecture;
no static runtime or cross-engine host policy rewrite in this correction.
Weights: userRole60; mids/book/orderStatus/clearinghouse/spotState/exchangeStatus2;
other info20 plus only documented top-level explicit limit extras; action
1+floor(batch/40). Nested candle req and response row counts do not add cost.

Replace incompatible account1000/min burst/row topups and trading334ms-only
pacing. Preserve typed Pending/ArrayAt/stream ownership and every existing
state/retry/late-fill/ACK fence. Revalidate queued active/amend ownership and
sign fresh only at real dispatch; expired/stale jobs cannot send after slot
wait, and unknown writes never retry. Startup independently enqueues core
meta→fees→perpDexs in TS order, waits all settled, then serialHIP3; cache fees
for final catalog and remove old late fee/duplicate discovery requests.
Spot retains its own one-meta bootstrap. Timer cooldown survives temporarily
empty queues/startup→playback. Files lib/account/trading/history plushttp.rs;
tests8–12focused controls and all90existingbehavioralassertions. Test drivers
may advance earliest live protocol timers with bounded action retention, not
drop requests or suppress assertions. Independent TS contract audit before
implementation and independent final native review required; then rebuild/
record/gates on corrected production. No fixture retiming or deadline changes.

Independent TS contract review PASS; native implementation authorized. Stage
the complete initial catalog until bootstrap settles, matching TS aggregate
publication; retain any already valid catalog on refresh/retry. New initial
partial-catalog readiness must not precede public connection by the newly
paced bootstrap. Queued writes hold unsigned intent/command ownership and
re-prepare against current state only at dispatch, before optimistic effects
and fresh nonce/signature. Preserve actual active request/filled/terminal/
protection checks through the added queue delay. TS manual order nonce can
be preselected; native dispatch-fresh signing is an explicit improvement,
not a claim that TS chooses its nonce at the same time.

Before any new performance result, choose existing after-ready recording
policy for parentHyperliquid too (already used by TxFlow and both new spots):
NEW actual recording gets20seconds of live public traffic after genuine full
catalog readiness, retaining every startup HTTP/lifecycle record as well.
Correct bootstrap is naturally about13seconds; recording20total would capture
only about7seconds of public traffic. This is a declared acquisition policy,
not fixture padding or a measured-window/warmup change: entire constructor+
startup still enters every replay cost. Original failed fixture untouched
until replacement; no future bytes or timestamp synthesis, unchanged120second
startup cap/20second duration and original gate rules.

Budget implementation checkpoint: partial-catalog regression fails original
native code at loaded.markets after core metadata alone; proof
/tmp/bitunix-hl-budget-before.log. New typed dispatcher/bootstrap code passes
initial Cargo check; full behavioral/queue controls still in progress. Early
test-driver review caught legacy fixed host timestamps rewinding engine time
after new causal timer advancement. Keep raw exchange timestamps unchanged,
but migrate scripted host events to monotonic logical time; fire overdue
timers at current time with deterministic equal-deadline registration order.
Never pass old assertions by manufacturing backwards host time. New budget/
nonce/dispatch controls must use strict raw Engine chronology independently.
Original invalid parent fixture backed up outside repo:298records/284WS,
gzipSHA1ae037ecae4713647f9087962cfda6d5f6974c18c156a6c4f92daa853636fdfc,
rawSHA67ab6d47f99fd53e3fc9e09ef9a41ae91db04f95e9f2b0c39ef6e6099c1e59df.

Resume checkpoint before required landing merge: fetched5e92f93a8a (PR1341 product semantics). Native HL budget work is paused and checkpointed as WIP, not gate-ready: production check and four raw dispatcher controls pass; remaining legacy scripted timer assumptions and private bootstrap are unfinished. Independent review found private known-identity startup must enqueue Mode/coreState before Meta/Fees/Dexs, with Role gating when identity is absent. Extend the phase minimally to cache early core state until full catalog publication, retaining revision/stale-read guards and spot startup. No artifacts or performance gates until source freeze, complete behavioral tests, and independent final audit. Merge landing structure and product additions while retaining strict harness/auth/output and all local corrections; then verify native union, frame/adapter extensions, Bybit/OKX new profiles and existing Bitunix/Binance controls.

Landing5e92 merge union: retain Bitunixfeature and incomingcold-subscriptions; new semantic metadata/Greek sections extend the existing frame with no layout replacement. Store resolution retains tickerindex rebuild plus semanticclear, all writers and complete reservation (including9th optional order column). Independent source audit, focused1/2/63/64-row union capacity/tails control, all99engine tests and all-feature strictClippy PASS. Bitunix68tests and strictClippy PASS after adding absent impliedVolatility to its reconstructed native order. Root44typecheck/28lint/27test tasks PASS; SDK7533tests/520files twice. First expanded800helper run799PASS/1FAIL: incoming test mocked obsolete socket.topics; strict runtime uses subscriptionArgs. Harness audit additionally finds incomingOKX profiles bypass strict typed SharedTape in legacy replay overrides; correct selectors/raw duplicate-header guards and scope generated IDs to creation routes, with actual constructor controls, before final release. HLbudget/private bootstrap remains uncommitted follow-up to its explicit WIP checkpoint; final all-venue gates and CI remain outstanding.

Hyperliquid budget phase complete:103native tests (all original90 retained,13new controls), strict scoped Clippy and independent final source review PASS. Typed xhr3/1000 admission and50ms/weight FIFO cover actual perp/spot clients; direct builder bypassesboth. Constructor graph matches TS: role gate ifneeded, earlymode/core+builder, then Meta/Fees/Dex barrier and serialHIP3, one atomic complete catalog with cachedfee/defaults. Early core state remains typed and fenced by revision/time/settings. Queued writes retain unsigned intent, revalidate amend/source/fill/terminal ownership, and sign fresh at actual dispatch. NewrawEngine controls cover exact0/1000/1100/3000 dispatches, idle/no-refund/bypass/stream envelope, atomiccatalog/failedfees, stalequeuedamend, dispatchnonce, rolefailure/timeout/late response andstalequeuedaccountscopecleanup. Review caught and fixed8s builder generation-owned timeout/late-response/no-duplicate fences, plus firstopen no duplicate core while actualreplacement socket refreshes. Rawtests open actual physicalsocket before messages. Testdriver hosttime ismonotonic with stableTimerID ties and everyactionretained. Root fullworkspace checks inprogress; no new artifact/capture/gate yet. Prior host89test attempt used stale HL-only artifact and returned87PASS/2unsupported-venue failures; rerun only after freshcombined build. Public capture policy remains20s after real readiness with entire constructor/startup included.

Final boundary phase PASS: OKX4profiles now use strict production SharedTape typedrequests/responses/send/fullsubscriptions. Eightactualconstructor probes failedbefore andpassafter. GeneratedclOrdId/algoClOrdId projection is boundedASCII and POSTcreation/nativefieldonly; caller IDs, cancel/amend/lookup targets, typed economics andunknownbody/queryfields remain exact. Unique raw headers, exactofficialorigins/userinfo/fragment(includingbare#)/portguards reject BEFORE matching. Anyknowncredentialheader requiresfullHMAC/account/freshness proof evenonpublicroutes; Authorization/unknownOK-ACCESS reject. ActualTSAPIaudit confirms honestunsignedpublicreads andfullysignedmarket/publicreads retained. Independent originalcounterexamples nowrejectwithoutconsumingACK/readresponse. Focused52 andall808helpers PASS, final independentaudit PASS. Staleincomingmock usesactualsubscriptionArgs withoutlosingassertions. Existingfinalizer guards, callableKNOWN_TS_FIXES andallnewsemantic/option/Greek projections retained. Additionaltest-only actualHLcancelbatch39→40 control passes exact50/100ms nextwrite reservation despiteimmediateACK; nativeHLproof total104 (previousfull103 plusnew1), independentreviewPASS, productionunchanged. Rootglobalchecks continue; freshcombinedartifact isrequired beforecompiled/host checks andnewlivepubliccapture.

Freshcombined27 build PASS122s; globalbans/fmt/check/nativeClippy/strictworkspace tests2709 across215groups/zeroignored andworkspaceWASMClippy PASS. Addedbatchboundary1PASS bringsnativeverifiedunion2710. Freshcombined host89 andactualsemantic/CLI integrations16PASS. ExpandedSDKcompiled427run426PASS/1FAIL: testcalls5sconnecthelper whilecorrectHLcatalogneeds12sREST-first bootstrap. Fixonlytestsequencing: awaitactualHLworker.start success beforeframe delivery; original20stest and5shelper unchanged, allnativefreshpublicframe/lateconsumer assertionsretained. IndependentreviewPASS, affectedall48-product compiledmatrix PASS45.26s; other379compiledcases alreadygreen. SDKtypes/lint PASSaftertest-onlychange.

NewactualHLpubliccapture afterdeclaredpolicy:333records(meta1/http13/connect1/open1/ws317),last32455ms; twentysecondsafterfullcatalogreadiness,2books. Allactualstartup retained; trim33seconds decompression isBYTEIDENTICAL to/tmp/bitunix-5e-hyperliquid-public.jsonl. gzipSHA2569fb5bf567759cf33ead422ad7f9ed5619162ed5d20e600808e4934b3ac8da20a; rawSHA256e3cd17866dc7be653be95d42c823e0ae81dfd71131c2e2c6cfbcb0d06d3e3b40. Independentprovenance/causal auditPASS: core241,fees1165(legit422default),DEX2177,HIP3receipts3233..12231,CONNECT12232,OPEN12408,firstWS12483. Literal0/1000/2000/3000..12000 weighted dispatchfitsallprefixes. Originalinvalidfixture backupretained. StrictTS/Rustcombinedfunctionalprobes both318frames,0unserved/unusedWrites/protocol/socket/signatureerrors. Finalfeature-isolated performance/correctness gates nowstart withotherownedjobsstopped; actualPRpush/adversarialcomments/exactheadCI/merge stillpending. No gateclock/deadline/warmup/threshold/hash changes.

Finalselectedbatch57c1 stopsatHyropublic CPU FAIL(mainTS20.0/Rust22.0ms;steady3.5/3.3,retained/peakPASS),unchanged160framefixture. IsolatedBybitmodule1104090bytes; parent/inverse/spot/demo public+private andHyroprivate PASS beforestop; laterOKX/Paradex/Breakout/HL/Kraken/Bitunix/Binance/GRVT/Lighter/Coinbase steps NOTRUN. No unchangedretry. Profile exactfailedcandidate withdiagnosticCPU sampler (codeidenticalnamedWASMcopy ifneeded, preserve/restoreoriginalassets) beforeproductionchange. Audit hypothesis: selectedBybit missesexistingcold-subscriptions feature (enabledBreakout andthereforecombinedSDK already); genericoption-aware commanddecoder coldcompile maydominate tinyHyrobootstrap. Existingreviewedfastsubscription parser preservesfullvalidation andcommand clock/dispatch/flush semantics. Independentplan+implementation review, meaningfulexistingdecoder/query controls andaffectedgates required; no clock/deadline/tape/oracle/hash/budget/warmup adjustment.

ExactHyrodiagnosticprofile confirms coldcommand target: namedWASMcopy has identicalALLnoncustom sections tofailedrelease (originalSHAe662bb616a14cabb73cff29afb9b98a4d5623e9063e4382c4755b8f1d08ccff6); originalassetsrestoredandSHAverified. CPUprofile/tmp/bitunix-5e-hyro-rust.cpuprofile samplesWasmEngine::command2.000msself/2.916msinclusive, commandwrapper3.083msinclusive andgenericCommandIn::deserialize0.333msself. Diagnosticmetricsarenotgateevidence. InitialprofilerbeforeExit hook producednoprofilebecauseexistingreplayexplicitlyexits; corrected diagnostic-only exithook flushesprofile, noreplay/source/gate modifications. IndependentplanreviewPASSconditionalonthisprofile. Minimalproductionfix activatesexistingcold-subscriptions forisolatedBybitfeature; combinedalreadyusedsamepathviaBreakout andall427compiled/16semantic integrationspassed. Otherisolatedvenues/nativehosts unchanged; option/trading/querycommands retaincompleteparser/dispatch/clock/flush. RecheckisolatedhostvalidationandnativeWASMClippy,sizeandallBybitfamilygates; no thresholds/deadlines/fixture/window/hash changes.

IsolatedBybitcoldpath7host boundarytests andstrictWASMClippy PASS; independentimplementation reviewPASS(exactonefeatureedge, allothercommandparser/clock/dispatch semantics retained). Native/engine/sourceunchanged. Rebuildselectedsize andfresh10Bybitfamily correctness/performancegates,thencontinueunexecutedbatch; preserveoriginalCPUfailurelogs.

ColdBybitphasegates PASSall10familyprofiles; module1127141<1258291. NewHyropublicmainTS19.7/Rust19.2,steady3.5/3.0,retained0.8/0.7MB,peak2.3/1.4MB. All4OKXprivate+2publicprofiles PASS; module726597. Paradexprivate/public PASS,module1246974. BatchthenstopsBreakoutpublicFAIL mainTS15.3/Rust15.3(rounding),steady5.3/3.6andmemoryPASS; privatePASS,module664705. LaterstepsNOTRUN. Preservealloriginalfailedlogs,no unchangedgate retry.

ExactBreakoutdiagnosticnamedmoduleallnoncustomsectionsBYTEIDENTICAL (originalSHAe71cb7096ef03ed8c4409d8805c0945e626506df9af6e1c665a035b60eb23b0b,namedSHAa58bb6583b51df13ba98be4ccc50d0cd95a60c46f92b695d20b735455810c020), originalnodeassetsrestoredandSHAverified. CPUprofile/tmp/bitunix-5e-breakout-rust.cpuprofile samplesSession::select_account0.292msinclusive andtimeout_ms0.208msself; no sampledRfc3339 targetsoleavealltimestampcodealone. Boundednative-onlystartupallocationplan: replace3cardinality/filterVecs withiterators preservingfullyparsedJSON,exactdomain/status/account/subaccount/ambiguity/errorprecedence andmutateonlysuccess; parseexact3nonemptyASCIIdigit timeoutcomponents withnofourth viaiterator,sameu64/ranges/positiveyearbound,withoutVec. Nativeownerimplementation plusindependentplan/finalreview; all106oldcases retained andstrictchecks/compiled4parity/freshunchangedpublic/privategate required. NoTSslowdown, warmup/deadline/hash/oracle/tape/budget changes.

Breakoutnativeallocationphase sourcefreeze/reviewPASS: onlysession.rs changes; all108tests(original106+2contractguards) andstrictall-targetClippy/fmt/diffcheck PASS. Independent exact-sourceaudit confirmsfullparse/errorprecedence/uniquecardinality/singleaccount behavior/ranges andsuccessfulmutation unchanged. NoTS/timestamp/protocol/harness/tape changes. Verifiednativeunion now2712; freshunchangedBreakoutpublic/private+compiled4parity andremainingunexecutedbatch next. PreviousBybit/OKX/Paradex isolatedproduction unaffected, retainpassinggates; no unchangedreruns.


Resume after usage reset: PR #1309 remains open against feat/rust-rewrite.
The worktree was clean at dc2ca1fe66; fetched landing df2d6a4a03 adds the
parity sweep. Required merge is in progress. Resolve conflicts by retaining
both queued native HTTP budgets and secondary read credentials/primary lookup
routing; retain strict typed harness matching and incoming OKX product fixtures.
The relay decoder keeps bindSocketMessages, its close/dispose fences and binary
protocol handling, alongside incoming egress policy. Independent native and
harness resolution reviews run before committing the merge. No stash, rebase,
force push, master edits or upstream access.

Breakout gates after the allocation fix pass public/private and four compiled
parity controls. Hyperliquid parent public/private and spot private pass, but
spot public fails main CPU (TS 46.4/Rust 47.6 ms) and retained memory (rounded
2.0/2.0 MB, ratio 0.98). Preserve this failure; do not retry unchanged.
An identical-code named WASM diagnostic profile attributes 3.375 ms inclusive
to the untagged numeric decoder through ContentDeserializer, including failed
serde alternatives. The original release assets were restored and SHA verified.

The audited bounded correction replaces only that numeric helper with a typed
serde visitor, preserving trimmed numeric strings, finite checks, optional/null
behavior, signed zero and integer conversion. After complete spot context
validation, reserve exact output capacity using the smaller universe/context
length as a hint, without truncation or schema changes. Keep all 104 native
controls, add numeric and late-invalid-envelope contract coverage, run strict
checks and an independent source review, then rebuild and run all four parent/
spot gates. No fixture, oracle, deadline, warmup, threshold or hash changes.
After merge and correction, refresh affected global checks and actual compiled
consumers; run the remaining venue and Binance/Bitunix control gates, push,
post adversarial review findings, fix them, and merge only the reviewed head
with green CI. CI now includes two additional OKX public fixture invocations.


Parity-sweep merge committed as 8202946de8. Independent harness and SDK
transport union reviews pass. Final harness suite passes 811 tests after
removing two duplicated conflict-overlap test copies, keeping their originals.
KuCoin and Phemex retain quotas with secondary credentials selected and signed
at actual dispatch; definitive Phemex lookup retains primary identity and route.
All 210 scoped tests and strict Clippy pass. The six initial Phemex query-helper
failures are retained in /tmp/bitunix-df2-kucoin-phemex-tests.log; repaired helpers
advance registered timers monotonically without relaxing result assertions.
Root typecheck 44/44, lint 28/28 and test 27/27 tasks pass, including 7,707 SDK
tests in each target. Global bans, formatting, locked check and native Clippy
pass; full native tests and WASM Clippy are still running.

Hyperliquid numeric allocation correction has independent final source review
PASS. All 104 existing tests plus six contract controls pass (110 total).
Direct and flattened numeric parsing preserve finite validation, escaped and
trimmed strings, optional null/defaults, integer rounding and signed zero.
A late invalid context prevents publishing any market/ticker or opening a
socket until a valid retry. Spot output reservation occurs only after complete
context parsing and changes capacity, never selection or output rows. No
fixture, oracle, gate or timing policy changed. Rebuild and fresh parent/spot
gates are required next; the previous failed replay remains evidence.


Merged-candidate quality checks all pass: global bans, format, locked check,
strict native and WASM workspace Clippy; 2,826 native tests across 218 groups,
zero failures and zero ignored. Independent native merge and OKX product/frame
union reviews pass. Hyperliquid scoped strict checks also pass.

Fresh Hyperliquid release gates pass all four parent/spot public/private paths
and three compiled spot parity controls. Module 1,061,457 bytes is within the
unchanged 1,258,291-byte ceiling. Spot remains 67 frames, 302 markets/tickers,
two books: TS/Rust main CPU 56.6/53.2 ms, steady 21.3/15.9 ms, retained
2.0/1.9 MB, sampled peak 5.4/3.1 MB; WASM linear memory 1.1 MB. Parent:
318 frames, 340 markets/tickers, two books; main 144.6/81.2 ms, steady
49.8/29.9 ms, retained 6.8/2.1 MB, peak 15.7/3.1 MB. Logs are retained under
/tmp/bitunix-df2-final-*. No fixture, threshold or measurement policy changes.
Remaining selected integration/control gates continue sequentially from Kraken.
Final compiled consumers, push/reviews, exact-head CI and merge remain pending.


Fresh merged-source Bitunix gates PASS. Selected release module is 806,839 bytes
against 1,258,291. Live public fixture remains 28,694 frames, 697 markets and
tickers, two books: TS/Rust main CPU 344.3/276.0 ms, steady 189.5/143.7 ms,
retained 3.7/2.5 MB and sampled peak 12.1/5.0 MB. Private documented tape:
42 frames, two markets/tickers, correctness PASS. Net realizedPNL remains
verbatim. Kraken futures/spot public and private plus five compiled spot
controls also PASS; selected Kraken module 1,057,043 bytes. The remaining
Binance four-mode and integration controls continue; final CI is still pending.


Selected replay batch stopped at portfolio-margin private correctness. USD-M,
Spot and COIN-M public/private gates passed first. PM account/command outputs
match, but exact outbound audit catches TS sending three writes versus four
native writes: missing TS conditional cancel-all. Failure retained in
/tmp/bitunix-df2-final-binancepm-private.log and out/binancepm-binancepm-private-
jsonl-81998. No unchanged retry or write-comparison waiver.

Independent production and harness investigations agree this predates the
landing merge. TS PM calls only regular cancel-all then marks every symbol row
canceled, including live conditionals. Native already cancels both, with an
existing regression. The private fixture declares the conditional ACK native-
only, but the strict Binance outbound comparison correctly still rejects the
missing write. Correct production TS to attempt both families despite one
failure and mark only acknowledged families; add success/partial-failure
controls and the existing endpoint constant. Make that private ACK required
for both implementations. Record this genuine paired TS bug correction in the
gate and commit. No output normalization, gate change or removal of native
cancellation. Independent review and fresh PM private/public controls precede
resuming the remaining batch; other unaffected Binance modes retain passes.


PM correction source freeze and independent final review PASS. Four new controls
fail before the TS correction, and all seven cancellation controls pass after
(existing three USD-M controls retained). Package typecheck, production and
explicit regression-test lint, harness syntax and diff checks pass. TS now
attempts both DELETE families, marks only successful captured IDs, preserves
closed/canceled racing rows and later IDs, and propagates errors. The common
conditional ACK is required for both implementations. Native production and
strict output/write checks are unchanged. Earlier PM diagnostics confirm the
latent omission escaped because both outbound arrays were empty; the stricter
current arrays expose the three-versus-four write difference. This is a genuine
paired TS bug fix, documented in the gate. Fresh PM controls and remaining
integration gates resume on this reviewed source.


PM paired-correction replay now matches both required writes, but exposes a
second latent native defect: cancel-all turns a fully filled event-order from
Closed into Canceled. The TS correction correctly preserves Closed. Failure
retained in /tmp/bitunix-df2-pmfix-binancepm-private.log. Keep status/economic
comparison exact; no unchanged retry.

Independent plan review approves reusing the existing native Binance
mark_canceled_preserving_closed helper in both proof-less cancellation success
and UNKNOWN_ORDER paths, then deleting the unused unconditional helper. Spot
and COIN-M already use this guard. Add command/ACK regressions covering PM and
USD-M preclosed and racing full fills, success and unknown-order responses,
while still canceling open targets. Run scoped native tests/strict checks and
independent final review. Rebuild and rerun all four Binance public/private
modes before resuming the remaining batch; no fixture or gate changes.


Native cancellation guard correction is frozen and independently reviewed PASS.
All 115 Binance tests pass (111 existing plus four raw PM/USD-M success and
UNKNOWN_ORDER controls); strict all-target Clippy, scoped formatting and diff
checks pass. Four new controls fail before the correction and pass after,
covering a preclosed order and a full fill racing the actual cancel-all ACK,
while open siblings still cancel. Reuse the existing helper; delete the unused
unconditional helper. No proof-based ACK validation or amendment failure logic
changes. Verified native union is now 2,830 tests. Source root typecheck 44/44
and lint 28/28 pass again after the paired TS fix; full root tests are running.
Rebuild all four Binance profiles and rerun public/private gates next, then
resume unexecuted integration checks. No output, fixture or gate changes in
this native correction.


Native guard replay correctly exposes the paired USD-M TS version of the same
status bug: /tmp/bitunix-df2-terminalfix-binance-private.log retains fully filled
ETH order 1001 as Closed in Rust, but TS symbol cancellation changes it to
Canceled. No other output differs. The stricter comparison stays unchanged.
The PM and USD-M native guards have four fail-before controls; TS PM already
has the proper captured-ID/current-status guard. Independent plan review
approves applying it to USD-M's existing two-family allSettled cancellation,
retaining partial-failure/error order and adding preclosed/racing fill/later-ID
source coverage. Native production, fixtures and gate logic stay unchanged.
After source tests/types/lint and independent final review, resume rebuilt
four-mode Binance gates and all remaining selected checks. Root source checks
just passed 44/28/27 tasks and 7,711 SDK tests per target before this final
paired USD-M correction; refresh affected validation before final landing.


Paired USD-M source correction is frozen and independently reviewed PASS.
Two new USD-M preclosed/racing-fill controls fail before; all 11 cancellation
controls pass after (all original seven retained plus four shared USD-M/PM
cases). Package typecheck, explicit production/test lint with no-ignore,
harness syntax and diff checks pass. The source captures native family IDs,
then guards current Closed/Canceled status at ACK time, retaining quantities,
both requests, family-specific failure behavior and later placements. Gate
delta is documentation only. Native production and tapes are unchanged.
Resume all four Binance public/private controls on the reviewed candidate.


Fresh rebuilt Binance four-mode union passes all eight public/private gates on
7892830da5. Selected module is 923,949 bytes. PM private now requires and emits
both cancellation families in TS and Rust, preserves fully filled Closed
status, and compares all account economics/commands/outbound writes exactly.
USD-M private likewise retains the filled ETH order, with strict output/write
comparison unchanged. No fixture retiming or normalization. Logs are under
/tmp/bitunix-df2-usdmfix-*. Remaining GRVT and other integration gates continue.


GRVT public/private and Lighter parent public/mainnet/testnet private gates pass.
Robinhood public passes, but Robinhood private stops on raw signed transaction
bytes: ten writes each, identical outputs and zero crypto/protocol errors, with
fresh ExpiredAt differing by 293 ms and therefore different Sig. Preserve
/tmp/bitunix-df2-usdmfix-lighter-robinhood-private.log; no unchanged retry.

Independent dataflow reviews identify a replay-only identity duplication:
SharedTape validates crypto and matches/records typed request intent, but the
production outbound override computes another key. Its allowlist includes
Lighter and omits Robinhood, falling back to raw form strings there. Approved
bounded generic correction reuses the actual just-recorded SharedTape write
identity after validation, with identical write classification. No new fields
are normalized. Controls must exercise the actual production override, custom
contracts/comparison bodies, signature rejection and unmatched/missing ACKs.

A separate real-verifier probe finds unsupported query fields named signature,
timestamp or clientOrderId can disappear under generic URL filtering. Make the
Lighter profile preserve these unknown fields explicitly; only independently
verified auth may be projected. Add real Go-WASM positive differing-expiry
controls for both deployments, freshly re-signed economic/target/nonce/unknown
mutations, invalid credentials/signatures/expiry, duplicate forms and single-use
ACK assertions. Native source, tapes, thresholds, deadlines and metrics are
unchanged. Independent final review and helper suite precede fresh Lighter,
Bitunix/Binance private controls and the remaining batch. Public write-free
behavior is unchanged; final CI still covers every declared profile.


Shared write-identity correction frozen and independently reviewed PASS.
Generic production outbound captures the one newly recorded SharedTape write
key after actual-request validation/matching, uses the shared write classifier,
and rejects missing/multiple per-call identities. Invalid signatures append
neither array; unmatched 599 attempts remain visible and keep required ACKs
unconsumed. Bitget/Hyperliquid custom contracts and comparison bodies remain
selected upstream; POSTed reads still append no write. No new normalization.

Lighter now preserves unsupported query names, including generic clock,
signature and caller-ID-shaped fields; only verified auth is projected. Real
Go-WASM controls sign both deployment domains independently, accept legitimate
293-ms expiry differences, and reject freshly re-signed economic/target/nonce/
unknown-field changes, invalid account/key/signature/expiry, duplicate forms
and repeated ACK use. Four fail-before cases are retained. Focused 26/26 and
all 818 harness helpers pass, with syntax/diff checks. Independent reviewer
re-probes all seven prior unknown query counterexamples: each returns 599 with
ACK retained; honest original returns 200 once. Root typecheck 44/44 and lint
28/28 pass again. No fixture, metric, threshold, deadline or economic output
change. Resume actual related replay controls with owned jobs quiet.


The fresh Lighter public gate catches a scope regression in the new strict
query policy: top-level preserveQueryFields also reaches WebSocket subscription
URL identity, making TS's generated transport requestId differ from Rust's
URL without that token. All output economics, CPU and memory pass; only this
subscription identity fails in all three runs. Preserve the failure under
/tmp/bitunix-df2-identityfix-lighter-public.log; no unchanged retry.

Narrow the HTTP policy into the existing requestOptions hook, which HTTP
requestKey applies but socket URL canonicalization does not. Preserve all new
HTTP unknown-query rejection controls, verify the actual Base WS transport
requestId generation, and add socket token/unrelated-field controls. This
restores existing socket behavior without a new projection or fixture change.
Independent review and all helper controls precede the fresh related batch.


HTTP/socket scope correction frozen and independently reviewed PASS. The same
strict HTTP options now live only in requestOptions, preserving every signed
HTTP counterexample. Actual LighterPublicWebsocket.connectAndSubscribe source
(line 170) adds the nanoid transport requestId; the new test executes its real
URL construction. Both profiles preserve existing no-token/token-A/token-B
socket identity while readonly, account index, unrelated fields and paths stay
distinct. Independent re-probes confirm all seven unsupported HTTP queries
still reject with ACK retained. Focused 27/27 and aggregate 819/819 helpers,
syntax and diff checks pass; fail-before socket proof retained. No new URL
projection, fixture or measurement change. Owned jobs are quiet; restart the
related release batch on this reviewed two-file correction.


Fresh shared-identity candidate passes both Lighter public profiles and all
four mainnet/testnet private profiles. Lighter module is 996,888 bytes. Bitunix
private and public controls also PASS on the current harness; selected module
remains 806,839 bytes. Latest Bitunix public result supersedes earlier timing
receipts: 28,694 frames, 697 markets/tickers, two books; TS/Rust main CPU
1,330.4/1,252.6 ms, steady 757.8/719.4 ms, retained 3.7/2.6 MB, sampled peak
12.1/5.1 MB. No unchanged retry or measurement-policy change. Binance controls
and remaining venue checks continue under /tmp/bitunix-df2-identityscope-*.


KuCoin startup causal proof is complete. Actual unchanged TS diagnostic dispatches
contracts 0→334, tickers 334→495, first bullet 495→628. The first recorded OPEN
at 952 requires the second token, whose real shared quota slot is at 1000.
At the blocked prefix there are no held callbacks, work, pending responses,
unserved requests or invalid signatures; only the first token's socket exists.
The scheduler correctly refuses to fire the 1000 timer past OPEN 952. Exact
trace is /tmp/bitunix-kucoin-causal-ts.log. Generic write identity is excluded:
bullet POST is a read and appends no write/outbound entry in a production probe.
The old fixture predates the native quota correction and is obsolete evidence.

Preserve its gzip/raw copies and hashes outside the repository, then acquire a
NEW actual native KuCoin public recording using the current corrected budget,
standard 20-second acquisition and the same BTCUSDT/ETHUSDT books. Keep every
real startup record; no retiming, padding, after-ready policy or duration change.
Independent causal/provenance review and strict TS/Rust functional replay must
validate the new capture before its performance gate. Native production,
clock, deadlines, matching, hashes and thresholds remain unchanged. Related
Bitunix/Binance controls are already green under the latest shared identity.


Resumed checkpoint: clean branch 2d937ade40, PR #1309 remains open into
feat/rust-rewrite at old remote head 6b2d580332. Required fetch now advances
landing to 1a10f714ec, adding the KuCoin Spot product, delegated Paradex signing,
engine feature isolation and read-only audit findings. Merge in progress:
preserve the nested structure, Bitunix registration/net realized PnL, paced
KuCoin futures HTTP and all strict replay/lifecycle/identity protections while
adding the incoming product and signing interfaces. Independent reviewers own
disjoint conflict scopes (harness, hosts, KuCoin); root owns the merge and gates.

The new actual standard twenty-second KuCoin recording completed successfully
with 10,031 frames, 73 store updates and two books. Preserve its raw bytes in
/tmp/bitunix-df2-kucoin-public.jsonl for causal/provenance review. Landing also
contains a new KuCoin futures fixture; audit both captures before selecting
current evidence. Do not retime or pad either capture or alter measurement
policy. Rebuild and verify the merged source before replay; run full source
and native/host/consumer checks plus affected new product/signing controls.
Earlier passing controls remain historical evidence, not claims about newly
changed code. Finish remaining OKX gates and refresh Bitunix/Binance controls
on the merged candidate, then push/review/green CI/merge/fresh branch.


Merge conflict audit is complete for hosts, paced KuCoin futures and the strict
harness. All fifty source-only host tests and focused ninety-seven harness
controls pass; full merged checks are running. Preserve both incoming delegated
signing and our bounded binary/lifecycle/validated request identity protections.
KuCoin Spot is a distinct product; incoming no-auth-on-public futures guard is
combined with dispatch-time secondary credential selection and FIFO pacing.

Fresh capture provenance audit passes. Raw SHA-256 is
2a649f46a1a587fd1c9830eec72fe35b3201c15da5979270ae07002eaff38632.
The standard twenty-second recorder exits at 20,168 ms because the last actual
transport callbacks settle after its stop request; trimming at twenty seconds
would discard thirty-five real messages. rust:trim-tape --seconds 21 retains
all 10,045 original rows byte-for-byte (10,031 WS, seven HTTP, three connect,
three open and metadata); no acquisition extension, padding or retiming. Gzip
SHA-256 is 13fda57afbc3748079de63cf0261788807903faa21f0767bef88306143c1f086.
Bullet receipts at 1491/1549/1551 precede their connects and opens, with quota
slots available after markets 948 and tickers 1348. The incoming 6830bc02
fixture still has three tokens before the third shared quota slot at 1000;
retain the independently audited paced capture instead. Strict functional
replay must precede its performance gate after the merged build.

New merge audit finds the incoming Spot harness erases arbitrary REST connectId
and overbroad generated clientOid values and keys socket audit by URL. Bounded
follow-up: preserve unsupported HTTP fields, recognize only source-proven
generated IDs, bind audit state to actual sockets, add fail-before controls,
and independently review before actual replay. No economic projection or gate
threshold changes are authorized.


Merged Clippy catches a stale test-only helper name: incoming feature isolation
renames prediction_store to market_position_store, while the retained semantic
frame-capacity regression still references the old name. Update that single
call, preserving every fixture and assertion. Production workspace check passed
because this branch is test-only; all-target checks correctly reject it.


Test scheduling correction is independently reviewed PASS. Two unit tests had
assumed setup reached the intended wait before their short wall deadline; the
full suite correctly timed out earlier under contention. Test-only setup now
enters actual Tape.play open delivery at logical 50 before its unchanged 30 ms
deadline and asserts no nested clock pump. The collector case explicitly proves
entry with an immediate fixture pause/fixed supplied sampling clock while its
original real 30 ms timer stays live; resource cleanup and suppression of late
sampling remain required. Production clock/memory code is untouched. Delayed
originals reproduce both exact failures, revised fixtures pass the same 60 ms
delays, and restoring the forbidden nested pump still fails. All 36 scoped
controls and the subsequent 833-helper aggregate pass. Proof logs are
/tmp/bitunix-harness-timing-{before,stress-after,nested-mutation,after}.log.

Merged source checks pass: 44 typecheck tasks, 28 lint tasks and 27 repository
test tasks; SDK 7,792 tests in 526 files pass in each source target. The direct
SDK typecheck also passes after harness changes, bypassing Turbo's cache for
imports outside the package. Strict all-target native Clippy passes after the
stale test helper correction. Full native tests and WASM lint continue before
release replay; no new performance result is claimed yet.


KuCoin Spot integration guard phase frozen and independently reviewed PASS.
Incoming HTTP connectId/body-ID normalization is narrowed to source-proven
creation entropy: only generated alphanumeric clientOid (1..21 characters) on
POST /api/v1/hf/orders may vary; caller IDs, alteration/cancel targets, unknown
query/body fields, generic clock/signature names and financial wire types stay
exact. Socket connectId remains transport-only and protocol state belongs to
each physical socket, including parallel peers, close/replacement and unsubscribe.

Raw auth now reuses uniqueRequestHeaders, rejects exact-origin aliases with
userinfo or fragments, and returns false on duplicate headers so sticky failed
attempt auditing remains visible if the venue catches the failure. All four
generic body names are explicitly preserved: KuCoin authenticates in headers.
Fail-before production-constructor controls reproduce query/socket ownership,
four header/URL aliases and four signed unsupported body fields. Independent
review re-signs eighteen negative requests, each rejected with unserved=1 and
the ACK retained before the honest original succeeds exactly once; actual
socket sibling/replacement/readiness checks also pass. All 61 focused controls
and final aggregate 837/837 helpers pass, with syntax/diff checks. No fixture,
output repair, economic projection, deadline or threshold change.

Fresh paced KuCoin public capture now completes strict production TS functional
replay (692 futures markets/tickers, both requested books), eliminating the
old impossible startup prefix without relaxing the scheduler. Log/result are
/tmp/bitunix-1a10-kucoin-causal-functional-ts.{log,json}. This diagnostic runs
during native compilation; its timings are not performance-gate evidence.
A rebuilt Rust functional replay and quiet sequential gates remain required.


First merged feature-isolated KuCoin release build catches E0609: incoming
prediction feature isolation removes two Store fields in ordinary builds, but
the retained table-reservation union still references them unconditionally.
Preserve the failed log /tmp/bitunix-1a10-kucoin-initial-build.log. Gate the
prediction market/holding reservation branches with the same prediction feature
as their fields and frame writers; no counts, strings or enabled-path behavior
change. Independently review the two attributes and check disabled, product
and prediction engine feature builds before the release retry. Existing full
workspace checks unify prediction and therefore could not catch this case.


Independent feature audit also catches the retained prediction reservation
unit test calling gated setters without enabling prediction. Disabled-feature
all-target Clippy reproduces E0599 in the unchanged test. Add the same feature
guard to that test, preserving its entire body; explicitly run the prediction
regression with the feature enabled. This keeps the assertion in its supported
configuration and makes ordinary engine tests compile. No test is deleted.


Prediction feature correction is independently reviewed PASS: exactly three
attributes, no arithmetic/string/test-body changes. All-target engine Clippy
passes disabled, product-semantics and prediction configurations. Explicit
prediction suite passes 107 tests, including the full reservation regression.
Merged isolated KuCoin release now builds at 778,411 bytes; strict Rust and TS
functional replay each complete 10,034 lifecycle/data frames with both books,
zero unmatched requests, signature/protocol/socket errors and unused writes.
These functional runs occur during compilation and are not timing evidence.
Quiet performance gates still follow the remaining native/WASM checks.


Merged native quality is complete: 2,910 workspace tests in 221 groups, zero
failures/ignored tests; strict all-target native Clippy and whole-workspace
WASM Clippy pass. Feature-isolated engine checks also pass as above. Fresh
combined release consumers pass 440 compiled SDK cases in fourteen files, all
95 host tests, sixteen actual product/CLI checks, the cross-language frame-wire
integration and native WebSocket session-adapter integration. Full scripts/logs
use /tmp/bitunix-1a10-{native-tail,consumers}.sh and matching log prefixes.

The scripted lifecycle suite passes eight safety cases but its successful fake
lifecycle tape fails strict replay: native observes the BTCUSDT book while TS
does not (all book requirements remain enforced). Preserve the original
/tmp/bitunix-1a10-live-smoke.log and exact recapture/failure under
/tmp/bitunix-1a10-smoke-fail-before.*. Bounded diagnostic exports the existing
opt-in fake tape before asserting gate success so failure evidence survives;
then trace real TS socket/listener/depth delivery. No synthetic padding, message
retiming, deadline increase or book-check waiver. Independent causal review and
all nine scripted lifecycle cases must pass before accepting the consumer gate.

Both strict KuCoin functional outputs also match raw: all 692 markets/tickers,
two final books, 136 intermediate book events, five request keys and 698
subscription keys. Owned compilation is now complete. Resolve the lifecycle
replay defect, then run the quiet sequential /tmp/bitunix-1a10-gates.sh batch
(KuCoin/Spot, Bitunix, four Binance modes, four OKX public/private profiles,
Paradex and Bitrue), preserving any failed result before bounded corrections.
No source is pushed yet; remote PR head is still 6b2d580332. Reviewable commits
now end at ce578f6962. All-gate claims, PR update/reviews/green CI/merge and the
fresh follow-up branch remain outstanding.


Lifecycle book causal trace confirms a replay delivery bug, not invalid depth
sequence or missing live data. On the exact untouched fake capture, buffered
non-I/O instrumentation observes listenOrderBook called while disconnected,
then depth at t=75 delivered with zero parser handlers; the actual depth
subscription follows later and no snapshot read occurs. Verbose console tracing
changes setup timing and hides the race, so it is explicitly not accepted as
proof. Buffered trace: /tmp/bitunix-1a10-smoke-buffered-trace.log.

Bounded fix plan: register only USD-M depthUpdate's exact native symbol-derived
@depth stream through the existing venue.streamOf/socketFor path. It already
waits for that physical socket's actual subscription and wakes on send, under
the original deadline. Preserve capture messages/timestamps, readiness, book
comparison and core clock code. Verify both implementations' sent topic shape;
add delayed, never-subscribed, wrong-symbol/socket and generation controls.
Independent reviewer must validate the same capture and no background waiters,
then all nine scripted lifecycle tests and affected helper controls must pass.
Other Binance products are unchanged until evidence requires otherwise.


Depth routing phase is frozen and independently reviewed PASS. Only USD-M
depthUpdate maps to its exact lowercase symbol@depth stream through existing
socketFor; wrong symbol/physical socket, absent/unsubscribed and closed-generation
subscriptions cannot release a frame. Exact target subscription releases the
original bytes/t=75 with no waiter leak. Other Binance products stay unchanged.
The hook returns undefined on unrelated/malformed text, leaving the actual
protocol handler in control; five raw literal/malformed/non-depth shapes are
independently verified byte-for-byte. Positive unit setup proves waiter entry
synchronously, preserving its original deadline and all routing assertions.
All 25 protocol controls and final 841/841 helpers pass; three controls fail
before, direct SDK typecheck and source/diff checks pass.

The owner completed all nine scripted lifecycle controls after the routing fix
(/tmp/bitunix-1a10-smoke-routing-after.log). The root's subsequent full helper/
lifecycle verification passes 841 helpers and eight lifecycle safety cases but
the successful lifecycle case stops earlier, during TS baseline memory
settlement before exchange construction (replay.mjs:757), with the unchanged
1000 ms budget. Preserve /tmp/bitunix-1a10-depth-final-live-smoke.log. No output
or performance result from that failed attempt is accepted. The raw-message
follow-up changes only malformed input, not this successful JSON path; do not
hide this distinct pre-start failure or rerun unchanged to select green.
Read-only bounded GC/counter diagnostics must classify it before performance
work; production memory policy, deadlines, warmup and metrics stay unchanged.
Exact-head CI must still validate all lifecycle cases before merge.


Read-only preconstructor diagnostic completed without running a replay/gate.
All three arrayBuffers samples are exactly 2,377,281 bytes; settlement finishes
at 965.18 ms. Requested 10 ms pauses take 10.52/170.23/112.53 ms and full GCs
take 22.74/304.65/343.95 ms. The observed pressure is scheduling/GC wall latency,
not moving backing counters; the original failed run lacks samples, so its exact
cause remains unproven. Preserve /tmp/bitunix-1a10-baseline-diag.json. There is
no evidence warranting a production sampler, deadline, warmup or metric change.

Final verification runtime is standardized explicitly to CI's pinned Node 22
(.github/workflows/ci.yml:23 and rust.yml:214), installed v22.23.3. Previous
source/compiled checks and failed baseline used v24.21.0 and stay recorded. Run
a distinct supported-runtime compatibility check on the final combined artifact
(SDK, host/product/wire/server and all nine scripted lifecycle cases), then
all remaining sequential release replays under Node 22. No deadline/threshold
changes or hidden replacement of failed Node 24 evidence. Exact-head CI remains
the required final lifecycle and complete profile validation before merge.


CI-runtime compatibility phase passes at final reviewed source: Node v22.23.3
completes all 440 compiled SDK cases, 95 host tests, sixteen product/CLI checks,
frame-wire and native WebSocket session-adapter integrations, and all nine
scripted lifecycle cases. The same combined WASM artifact is used; no Rust or
memory-policy change. Latest successful lifecycle log is
/tmp/bitunix-node22-live-smoke.log (nine pass, zero failed/skipped); the failed
Node 24 preconstructor attempt remains explicitly preserved above. No output or
timing is cherry-picked from that failure. Runtime switch was declared before
verification and matches both CI workflows' checked-in Node 22 pin.

Begin quiet sequential /tmp/bitunix-node22-gates.sh with all owned compilation,
helper, consumer and diagnostic jobs finished. Rebuild each selected module and
check the original 1,258,291-byte limit, public three-run CPU/memory/correctness
and private correctness, starting with the causally audited fresh KuCoin tape.
Keep every result and diagnose any failure before a bounded correction.


Node 22 replay batch passes KuCoin public/private and Spot private. KuCoin
public: 10,034 frames, 692 markets/tickers, two books; TS/Rust main CPU
743.6/455.3 ms, steady 434.6/263.4, retained 6.9/2.7 MB, sampled peak 23.5/7.4;
module 778,411 bytes. Spot module is 680,912 bytes. Spot public stops before
TS constructor/measurement at the baseline settlement guard; no correctness
comparison or performance result is produced. Keep
/tmp/bitunix-node22-kucoinspot-public.log and the stopped batch; do not claim
Spot public passes or discard this attempt. No later batch steps ran.

Same-path Node 22 diagnostic (no constructor/replay) keeps arrayBuffers exactly
3,784,040 for three samples and settles at 840.8 ms, using only 173.4 ms process
CPU/48.1 ms main CPU. GCs take 376.5/226.1/187.3 ms wall versus
64.0/53.0/51.5 ms process CPU. Host aggregate CPU was 775 percent, with 32 GiB
RAM and 2.7 GiB swap in use; own process nice is zero, and thread timeshare
flags do not establish background QoS. No scheduling/priority change is made.
Original failure lacks samples, so its exact cause remains unproven; current
observations show substantial off-CPU delay, not moving backing counters.
Evidence: /tmp/bitunix-node22-spot-baseline.json.

Independent process/evidence review accepts a single new full gate only after
a documented runner-state change: wait until aggregate CPU is below half of
reported hardware core capacity in two readings at least fifteen seconds
apart. Preserve all logs and diagnostics; do not construct/warm the producer
during qualification, change baseline placement/GC/deadlines/thresholds, or
select favorable samples. Then run one fresh full three-run Spot public gate
under the same Node 22, artifact and tape. A completed correctness/performance
failure must never be retried unchanged; stop on another incomplete attempt
and diagnose it. Static review/docs may proceed while performance is paused.


Resume checkpoint: worktree clean at e66b0432d9, PR1309 open on remote
6b2d580332; its old WOO CI failure is already corrected locally. User explicitly
requires another landing merge. Fetch now advances landing to fc809a9d85
(consumer blockers/public-feed integration). Merge is in progress with ten
conflicted files. Resolve disjoint venue/SDK and harness/host areas with native
reviewers, preserving incoming public-feed ownership and our reviewed pacing,
clock, identity, allocation and lifetime corrections. Store retains its existing
market-index ticker lookup plus the incoming revision counter; no redundant
sorted index is kept. Keep the nested workspace and all incoming consumer
startup/loader checks. Inspect every resolution and run fresh native, helper,
source and compiled consumer checks before release replay.

This new required source merge supersedes the prior frozen artifact for final
validation. Preserve the earlier KuCoin/Spot private receipts and incomplete
Spot public baseline attempt as checkpoint evidence. Do not execute the old
Spot continuation against its stale artifact. Rebuild required modules after
merge verification, then run fresh gates only on objectively qualified host
load. The previous two-reading qualification policy, thresholds, tapes, clock,
GC and deadline rules remain unchanged. Latest aggregate CPU696.2 percent at
11:47:46 UTC on ten cores does not qualify for the below500 percent policy.


Landing merge resolutions are independently audited and frozen. Hyperliquid
keeps the weighted quota/atomic bootstrap/fresh signing and adds incoming
shared-feed policy/revaluation; new policy tests use the causal TestEngine
and actual paced socket/ping/reconnect times instead of immediate bootstrap.
Lighter retains snapshot safe math and incoming position revaluation; TxFlow
retains deterministic intent sorting under direct-feed guards, with private
node subscriptions unaffected. SDK keeps strict CID/options/Bitunix registration
and all incoming consumer lifecycle wiring. Store revision/index union passes
independent review including duplicates, orphan promotion and catalog changes.
WASM retains Bitunix plus incoming feed policy/commands. Tape retains its entire
reviewed clock/identity/lifecycle implementation and only adds default URL-key
memoization; arbitrary custom/mutable canonicalizers remain uncached. Incoming
cache tests are adapted to live physical-socket APIs and preserve caller/rule/
closure coverage without applying HTTP volatility to socket identities.

Initial merged helpers843/844 exposes a test-only generatedQueryId assumption:
that hook belongs to HTTP and the strict socket canonicalizer correctly retains
the query. The custom-policy test now explicitly supplies its fixture-owned
rule without changing production normalization. Initial strict native Clippy
finds two borrow errors in newly adapted Hyperliquid helper calls; copy the
logical now before mutably dispatching, with all assertions retained. Preserve
/tmp/bitunix-fc809-helpers.log and native-clippy.log; fresh suffixed checks run
on the corrected frozen source. Root typecheck44/44 and lint28/28 already PASS.


Postmerge native strict Clippy PASS; repository checks44typecheck/28lint/
27testtasks PASS (SDK7793cases/526files in each runner). Independent merge
engine/host and venue/helper audits pass, except one bounded P2 in the incoming
URL memoization: JSON query values call mutable canonicalBody hooks/rules, and
rollingListWindows can change, so cached identities can become stale. Fix only
cache admission: bypass decoded JSON object/array query values and active
rolling-window policy, leaving production canonicalization, physical socket
selection and common URL cache intact. Four new mutation controls exercise
generatedBodyId, ignoredBodyFields, requestBody and rolling transitions; all
four fail on the committed pre-fix source and207tape/memory controls pass after.
Fail-before proof uses isolated temporary modules, now deleted; source was not
swapped under concurrent consumers. Logs /tmp/bitunix-fc809-cache-fail-before.log
and cache-memory-after.log retained. Full848helper rerun is in progress.

The second aggregate's unrelated linear-memory accounting unit fails because
process-wide arrayBuffers drops16,787,840bytes during construction, including
the previous test's discarded16MiB buffer. Preserve frozen-helpers predecessor
/tmp/bitunix-fc809-final-helpers.log. That actual runtime accounting proof now
runs in a fresh process with GC before baseline and after retained16-page
WASM allocation, sampling before serialization and reading memory after GC.
Both original exact zero-arrayBuffers-delta and retainedMemory arithmetic
assertions remain, plus exact1,048,576linear bytes. Independent plan and diff
audits PASS;17focused and207combined controls PASS. Production sampler, signed
deltas, deadlines and all gate policies are unchanged. An initial yarn exec
invocation without the argument separator executed no controls (empty log) and
is explicitly not accepted; the actual suffixed log records17 executed PASS.


Full848/848 helpers now PASS with all four proven cache regressions; separate
independent implementation audit PASS for cache admission and isolated runtime
memory proof. Commit this bounded harness phase. Full native runtime reaches
one newly adapted Hyperliquid cold-account test failure: completing the atomic
catalog invokes account_start's deliberate default account initialization, so
its initial !loaded.balance assertion is no longer a cold fixture. Production
behavior is unchanged; adapt the unit's public-catalog/private-cold boundary
without removing the private-readiness or accepted-provider-price assertions.
Initial full native log is retained; a fresh full workspace run must finish
before release gates. Combined release build is already complete; compiled
consumer suite is running on that fresh artifact.


Cold-account regression correction is test-only and independently reviewed PASS.
Leave real fee/dex discovery unanswered and assert the atomic native catalog is
still unpublished; seed an explicit synthetic typed public catalog through
store_mut to exercise the shared-price hook before any private snapshot. Both
!loaded.balance assertions and actual provider-price110 assertion remain.
Removing the on_public_feed loaded-balance guard would call live_balance for
the valid private identity and fail final readiness, so the regression retains
its intended protection. Production account_start and default initialization
are untouched. The fresh workspace run passes all three Hyperliquid public-feed
controls and thirty spot controls; full workspace tail/WASM check finishing.

Fresh combined validation COMPLETE on Node22/Bun1.4.3:452compiledSDKcases/
16files,97hostcontrols,16productchecks, framewire/native-session adapter,
11serverconsumer,5MCPconsumer,2CLIconsumer,2mobileloader, MCPbuild/isolated
packagedsmoke and all9scriptedlifecycle cases PASS. Logs are under
/tmp/bitunix-fc809-*.log. Old failed attempts remain preserved. Fresh module
gates and exact reviewed-head CI remain mandatory before landing.


Final native frozen batch COMPLETE:2,937tests across227groups, zero failures
or ignored; strict native/WASMClippy, globalfmt and bans PASS. All source and
combined consumer receipts above remain current. Fresh sequential release
batch /tmp/bitunix-fc809-gates.sh passes Bitunix, KuCoin and KuCoinSpot private/
public plus all eight Binance USD-M/Spot/COIN-M/PM gates. Seven rebuilt-module
checks and26selected replay gates are planned in total; no source mutation or
owned background test/build accompanies measurement. Each module waits for
two below500percent host readings at least15seconds apart after its build.

Bitunix module852,298bytes under1,258,291; public28,694frames/697markets/
697tickers/twobooks PASS. TS/Rust main435.3/394.5ms, steady226.8/209.7,
process627.6/610.7, retained3.6/2.4MB, peak11.8/4.8, linear0.9MB. Private
42frames correctnessPASS. KuCoin module818,900bytes; public10,034frames/
692markets/tickers/twobooks, main577.5/356.9, retained6.9/2.7, peak23.4/7.4
PASS. Spot module721,073bytes; public26,427frames/968markets/tickers/twobooks,
main511.4/413.0, retained5.1/2.3, peak13.0/4.9 PASS. All their private gates
alsoPASS. The earlier incomplete Spot baseline is retained, not selected as
passing evidence; this distinct final source/artifact attempt followed recorded
host qualification300.0/459.1percent15seconds apart and uses unchanged policy.

Binance rebuilt module968,787bytes; all four public/private modesPASS. Base
public873frames/917markets/783tickers/threebooks main238.2/129.9, retained
4.5/2.5, peak21.1/5.1. Spot7045frames/3729markets/tickers/twobooks main
780.7/533.3, retained13.7/8.8, peak75.7/27.1. Inverse andPM receipts remain
in their full logs. OKXmodule798,235bytes buildsPASS, but host CPU rises
673.1→704.3→838.6percent before its qualification; no OKXmeasurement has
started. Preserve the continuing wait. Remaining planned OKX/Paradex/Bitrue
replays and exact-head CI still must finish before landing.


The final batch STOPS at its first completed performance failure, OKXEU public:
37,817frames/755markets/750tickers/twobooks equality and memory pass, but TS/
Rust main704.2/774.6ms and steady452.1/506.8 fail CPU. Process946.6/1039.3,
retained8.7/6.1MB, peak25.1/14.7, linear1.8MB. Preserve full
/tmp/bitunix-fc809-okxeu-public.log and stopped batch. No xperp, Paradex or
Bitrue steps ran. All four OKX private profiles and base/inverse public passed
before this failure; do not replace or silently repeat any completed result.

Bounded performance plan: OKX's native public decoder still uses the global
size-optimized z profile, despite its isolated module using only798,235bytes
of1,258,291budget. Spend this known standalone headroom on throughput using
an isolated --venueokx opt-level3 crate override, exactly the existing Bitrue
build pattern. Keep combined size profile and every Rust/data/formula/parser/
clock/memory/deadline/equality policy unchanged. Independent plan audit, then
rebuild and enforce unchanged module budget. Recheck failing OKXEU public first
on the newly optimized artifact after host qualification, with all samples
retained. If it passes, check the other seven OKXpublic/privatecontrols and
continue remaining builds. No unchanged rerun-to-green or threshold change.


Independent plan and implementation audits PASS for the three-line standalone
OKX override. Shellsyntax and rebuild PASS; corrected module861,613bytes,
SHA256 b7345a8d64fbcafe565c0280c1d3e26d73f5aaa830a07bd8229d2e40bc718484,
under the unchanged budget. Failed z artifact798,235bytes/hash
9ae65cbaff19323e5972074f83b77d7f8af8e5adf5ff34000746d1b76e8d20d0 and all
six raw failure samples are retained by hash/results. The new candidate has
not begun performance measurement: host load repeatedly breaks the same
below500percent/two-reading qualification. No CPU win is claimed. Running
/tmp/bitunix-fc809-okx3-gates.sh waits for qualification, then failed EUfirst,
othersevenOKXcontrols, Paradex andBitrue; stop onanyfailure. Commit this built,
reviewed compiler candidate as a checkpoint, with gate phase explicitly open.
Use the wait for read-only prepublication adversarial review across private,
public/hosts and sharedharness; verify actual PR head again after publication.


Prepublication adversarial reviews find two bounded P2s before push: raw
Bitunix auth accepts a correctly re-signed nonnumeric timestamp, and sparse
cumulative order events emit a defaultSell/zero-price fill despite preserving
known parent metadata in store. Public/hosts and remaining sharedharness scopes
pass. Fix both with production-path regression proofs; realizedPNL remains net
of fees/funding verbatim. Timestamp validation accepts decimal safe integer
milliseconds as actual numericTS/stringRust values, without inventing nonce
formats or freshness windows. Invalid HTTP must stop before ACK consumption;
invalid WS/cardinality cannot set authenticated and failures stay sticky.
Private fill fix resolves omitted side/price from authoritative parent, retains
explicit price/cumulative average semantics, and defers unresolved fill/watermark
rather than inventing metadata; sparse unknownCREATE must not fabricate side.
New3auth/fivefill regression controls need fail-before, affected native/helper
checks, independent review and fresh Bitunix gates before publication.

Optimized OKXEU public first3run gate PASS: TS/Rust main662.0/546.4ms,
steady441.7/350.6, retained8.7/6.1MB, peak24.9/14.8. All four private controls
alsoPASS on opt3module861,613bytes. Then basepublic stops before its TS
constructor at the unchanged baseline settlement guard (replay.mjs:757).
Preserve /tmp/bitunix-fc809-okx3-okx-public.log and stopped batch; no output or
performance result from this incomplete attempt is accepted. No inverse/xperp/
Paradex/Bitrue steps ran. Original completedEU CPUfailure remains preserved.

All performance/build jobs are now stopped while auth/fill corrections run.
After these are verified, strengthen external qualification to TWO below500
readings15sapart BEFORE EACH remaining profile, not just after each module's
build; long multi-profile batches can outlive a quiet host interval. This is a
runner-state change outside replay; no sampler/clock/deadline/threshold/warmup
change. One fresh basepublic attempt follows qualification; stop/diagnose any
new failure. Already completed opt3EU/private passes are retained, not rerun.


Both new findings now have fail-before evidence. All three production auth
controls fail on prior replay source and pass after validation; full851helpers
PASS. Independent auth implementation audit PASS: actual integer numeric/string
clocks stay valid, malformed re-signed values and login cardinality fail before
ACK/requests/writes/outbound/authentication, errors remain in finalizer. No
nonce/freshness policy or valid request normalization changes.

All five sparse-fill controls fail on prior native source (13oldaccountcontrols
stillpass): wrongSell/zero-price metadata, wrongcumulativecost, lostunknown
watermark and fabricatedCREATEside are reproduced. The temporary prior-source
probe restores final account.rs byte-for-byte in finally; restoration verified.
Fresh globalnative/bans/fmt/strictClippy/WASM and rebuilt452compiledSDK controls
are now running on the fixed source. Other production TS/host packages remain
unchanged from their passing source/consumer checkpoints. Preserve
/tmp/bitunix-final-auth-{fail-before,after,helpers}.log and
/tmp/bitunix-final-sparse-fail-before.log. Independent finalfillaudit is pending.


Auth correction phase COMPLETE and independently reviewed:3productioncontrols
fail before/pass after;851fullhelpers PASS. Commit only that bounded harness
correction while private recovery review continues.

Eight private regressions now pass with all76Bitunixnativecontrols. Three
follow-up controls fail on the first five-control candidate (18oldcontrols
pass): knownsame-cumulativeUPDATE recovery, olderCLOSE recovery and knownsparse
CREATE identity/quantity preservation. Source restoration verified. Independent
review confirms those admission fixes but identifies a further real nonterminal
reconciliation overlap: after an unresolved sparseUPDATE, a newly dispatched
Orders read can advance the REST baseline before complete WS metadata arrives.
The corresponding terminalCLOSEcase is already fenced by terminal IDs and
read generations. Add explicit bounded unresolved-execution handling and an
actual debounce/request/response recovery regression, preserving ordinary
startup/REST baseline behavior. No active build/performance jobs during edits.

Initial globalfmt failed on first unformatted candidate and stopped its batch;
no nativeClippy/tests result was claimed from it. The worker accidentally
reused /tmp/bitunix-review-final-fmt.log for successful rustfmt and overwrote
that initial diff. Its observed failure remains in tool output; the saved
original source reproduces nonzero rustfmt in the distinct
/tmp/bitunix-initial-fmt-reproduction.log. Do not accept the empty overwritten
file as a globalfmt pass. Future full checks use new publication-prefixed logs.
Fresh combined452compiledSDKcases passed the first private candidate; final
recovery changes still require rebuilt compiled validation before publication.


Private recovery phase COMPLETE and independently reviewed PASS. Eleven new
native controls preserve all originals: five source-metadata cases fail on
prior source; three terminal/CREATE controls fail on the first candidate;
two additional causal REST/partial-recovery controls fail before pending-state
tracking, with ordinary REST baselines protected by a passing control. The last
fail-before run is22pass/2fail; final source is restored byte-for-byte afterward.
Highest pending cumulative targets skip only unresolved REST baselines, survive
partial recovery and clear once complete; cleanup reuses filled/terminal
retention, without new timers or indefinite unknown-ID state. Parent identity,
terminal status/timestamp, finite prices, cost deltas and netPNL remain correct.

Final publication validation PASS:2,948native tests/227groups/zero failed or
ignored; globalfmt, bans, strict native and WASMClippy; fresh combined release
build and452compiledSDKcases/16files. The851helper pass remains current after
the committed auth change (private follow-ups touch Rust only). Source/root and
other consumer proofs above remain unchanged. Logs use distinct
/tmp/bitunix-publication-* names; all completed CPU failures and incomplete
baseline attempts are retained separately. No output/metric/gate-policy waiver.

Commit private phase, then quiet /tmp/bitunix-publication-gates.sh: refresh
Bitunix private/public on final source, rebuild the already-reviewed OKXopt3
artifact and evaluate only its pending base/inverse/EUxperp public profiles,
then Paradex/Bitrue private/public. Each gate independently requires the same
two below500percent readings15seconds apart immediately before its launch.
Stop on any failure. No passed opt3EU/private controls are repeated merely to
select different samples. Exact-published-head adversarial reviews, all green
CI, merge and fresh branch remain mandatory after local gates complete.


## Resumed landing integration: 702a62050c

Required status/log/PR check found clean a0949b3d57, open PR1309 still at
6b2d58033228ab28ad6f468ad969bd72b7a4fca5. Fetch finds21newlandingcommits.
Stop only our publication batch and its qualifier while waiting for host capacity;
final private gate passed, no final public attempt started. Preserve its logs.
Merge the landing branch before continuing, retaining nested workspace structure.

Resolve five conflicts with independent bounded reviewers: preserve BloFin
pre-start passphrase guard plus both direct/proxy broker cases; preserve callable
strict gate normalizers/input oracles and all local KuCoin identity/socket tests,
add incoming cancellation/TIF notes and unchanged production-method control.
Replay keeps active-clock/work ownership, full memory/constructor accounting and
all authentication guards; signing clocks now begin after module setup before
both constructors, never rebase at readiness. Independent clock audit PASS;
actual incoming slow-module/delayed-readiness Aster production regression PASS.

KuCoin public broker headers must join our queue at actual dispatch: futures
configured public reads use primary signing, private GETs use secondary, unsigned
public/Spot requests retain channel attribution. Do not select/sign before queue
admission. Refresh global native checks, helpers, source checks and rebuilt
compiled/consumer proofs after resolution; remaining replay batch uses new log
names and per-profile objective quiet-host qualification. No completed performance
failure is erased, retried unchanged, or counted as a pass.

Incoming unknown-cancel audit incorrectly says Bitunix is not registered. Reading
its actual implementation exposes a real counterpart: absent-store IDs silently
continue in plan_cancel. Add a causal authenticated native regression for unknown
alone and known+unknown, prove failure, then reject the entire plan before writes.
Keep empty-list no-op behavior. Independently audit that follow-up after merge.

Merge resolutions independently audited PASS. Focused checks PASS:79 harness
merge/auth/identity tests, incoming slow-setup Aster signer regression, and5
KuCoin broker-attribution/pacing controls across futures/Spot/public-only paths.
New queued-public control exercises the actual3-request timer and independently
checks primary HMAC/partner timestamps at delayed dispatch. Other full validations
remain pending. The separate Bitunix unknown-cancel regression proves failure on
prior production: named-error assertion sees CommandDone Ok([]) for an unknown
ID; preserve /tmp/bitunix-702-cancel-fail-before.log. Its test/source fix is kept
outside the landing merge commit and follows immediately afterward.

Bitunix absent-ID follow-up COMPLETE: fail-before sees false Ok([]); the bounded
plan now errors with the ID before any job dispatch, including mixed batches.
All81Bitunix tests PASS; independent implementation audit PASS. Empty lists still
succeed without writes; native deduplication, sibling protection and ACK handling
are unchanged. This is native command admission (TS accepts full Order objects),
so no TS result or oracle normalization is changed. Incoming cancellation audit
now lists the actual27th Bitunix crate separately from WEEX.

Refresh scripts/logs use /tmp/bitunix-702-* without overwriting earlier evidence.
Native/source/compiled+consumer batches run on the resolved frozen source; no
performance job overlaps them. Then run the remaining final Bitunix, pending
OKX public and Paradex/Bitrue gates; add refreshed KuCoin futures/Spot and base
Binance private/public because the required landing changed their production
cancel/broker behavior. Already-passed opt3OKXEU/private results are retained,
not repeated to select samples. Exact-head CI refreshes all31rows before merge.

The first refreshed native Clippy run catches3new incoming KuCoin test calls to
removed Harness.has; no native-test result claimed from that stopped batch.
Adapt only those assertions to inspect actual emitted HTTP with the original
all-method URL predicate; retain quota timers, exact response sequence, methods,
continuation and CommandDone checks. No helper stub or assertion weakening.
The corrected full native batch uses /tmp/bitunix-702-native-fixed-* logs.

Refreshed852helpers and root44typecheck/28lint/27testtasks PASS. SDK source
suite now7,803cases/528files in each runner. Compiled consumers/native checks
remain running, and all replay jobs remain stopped until they finish. Static
review deduped37finding drafts against45currentPRcomments/zero reviews/threads;
clarify historical receipts and diagnostic claims before posting, retain
separate metadata/terminal/REST causes without repeating the same finding.

702 integration validation COMPLETE:2,968native tests/232groups/zero failed or
ignored,81Bitunix tests, globalfmt/bans/strict native and WASMClippy PASS.
Independent audit confirms the3KuCoin helper replacements exactly inline the
incoming predicate without weakening any assertion. Refreshed852helpers,
44typecheck/28lint/27testtasks,7,803SDKsourcecases/528files per runner,
453freshcompiledSDKcases/16files,97hosttests,16productchecks,wire/nativeadapter,
20server/MCP/CLI/mobileloadercases, packagedMCPbuild+isolatedsmoke and all9
scriptedlifecyclecases PASS. No live private account was contacted.

All root test/build jobs have finished. Start /tmp/bitunix-702-gates.sh as the
only root performance/build batch. Each profile waits independently for2hostCPU
readings below50percent aggregatehardwarecapacity,15sapart; no constructors or
replay samples run while waiting. All original limits/equality/tapes/timers stay
unchanged. Preserve stopped publication batch (no finalpublic attempt started).

Final Bitunix gates PASS on3d975c0d78 production: standalone856,206bytes
(SHA256be1a4ce3917d738b42fee0b9945bb1d57be547ee8189369e512c3e9bcf8b5cd3)
under1,258,291. Private42frames/2markets/tickers passes exact correctness;
CPU/memory informational. Public3run exact output28,694frames/697markets/
tickers/2books PASS: TS/Rust main505.9/425.5ms, steady280.1/198.0,
process696.7/652.3, retained3.6/2.4MB, peak11.6/4.8, WASMlinear0.9.
Preserve /tmp/bitunix-702-bitunix-{private,public}.log and complete raw samples
sdk-rs/harness/out/bitunix-bitunix-public-jsonl-gz-30916. Host qualified before
each profile. Remaining regression gates and actual published-head PR review/CI
are still pending; continue, do not report complete from these Bitunix receipts.

Final batch14/15PASS: bothBitunix, KuCoin futures/Spot, baseBinance, all3pending
OKXpublicprofiles and Paradexprivate/public; Bitrueprivate alsoPASS. Bitruepublic
stops on firstTS at unchanged1s backing-buffer settlement guard, noJSON/result.
Preserve /tmp/bitunix-702-bitrue-public.log and empty rawdirectory83187. The
asynchronous timer stack cannot establish baseline vs final-sample phase, so
claim neither; there is no accepted equality/performance result from this attempt.

Read-only diagnostic uses a loader hook to run ONLY the unchanged preconstructor
baseline and throw before either constructor:137.2ms wall,218.8msCPU, host140%
aggregate. Source and policy untouched; no producer or performance gate runs.
Diagnostic /tmp/bitunix-702-bitrue-baseline-diagnostic.log is not a gate pass.
Original qualification readings228.8/293.9% allowed the attempt under500%limit.
Propose a stricter runner state for ONE final Bitruepublic attempt: two readings
below quartercapacity250%,15sapart. All gates,1sdeadline,100msquietwindow,
fixtures/equality/metrics and warmup policy remain unchanged; no other profiles
repeat. Independent review of that bounded infrastructure action pending.

Independent infrastructure plan audit PASS: original sampling phase/cause stay
unknown; the diagnostic proves only an isolated baseline can complete. Quarter
capacity is a material admission change (293.9% would fail it). Predeclare exactly
ONE full3run gate after2readings<250%15sapart, sameNode/source/module/tape/limits.
Stop on any completed failure or repeated incomplete attempt; do not keep
adjusting runner admission/retrying to select green. No sampler repair claimed.

## Publication checkpoint and remaining CI gate

Required venue-brief pre-push gates COMPLETE: Bitunix crate/global native checks,
WASM Clippy/build, Bitunix private/public, and Binance private/public all pass.
Fourteen of fifteen final local profiles pass. The additional Bitrue public
profile is incomplete at the unchanged memory-settlement deadline, with unknown
phase/cause; no result is accepted. Independent process audit confirms the brief's
Gates list requires selected venue plus Binance locally, while the other matrix
profiles remain mandatory exact-head CI verification. No Bitrue performance gate
is waived or declared passing.

After26minutes without runner qualification, cancel ONLY our waiting shell and
qualifier before any second Bitrue producer starts. No quarter-qualified replay
or new samples ran. Delegate this additional profile to the required clean CI
runner, retaining the original failure and isolated diagnostic. Publish the
verified candidate, run actual published-head adversarial reviews, post findings,
and require ALL41 exact-head checks SUCCESS including the full Bitrue private/
public job before merge. Red or omitted Bitrue steps block. Do not claim complete
from local receipts. This replaces the unadmitted local runner plan, not a gate
threshold, fixture, output or sampling policy.

All14 completed local receipts and each raw TS/Rust sample verified. Public
results below show TS / Rust values; every public row uses3runs, private uses1.

| Profile | Frames | Main CPU ms | Retained MB | Peak MB |
| --- | ---: | ---: | ---: | ---: |
| Bitunix | 28,694 | 505.9 / 425.5 | 3.6 / 2.4 | 11.6 / 4.8 |
| KuCoin | 10,034 | 379.5 / 201.3 | 6.9 / 2.7 | 23.2 / 7.3 |
| KuCoin Spot | 26,427 | 389.1 / 321.9 | 5.0 / 2.3 | 13.4 / 4.9 |
| Binance | 873 | 253.1 / 146.2 | 4.5 / 2.5 | 21.1 / 5.1 |
| OKX | 22,663 | 385.5 / 281.0 | 6.4 / 4.8 | 16.0 / 12.8 |
| OKX inverse | 41,499 | 573.1 / 459.9 | 6.4 / 4.6 | 22.2 / 12.8 |
| OKX EU XPERP | 29,268 | 1043.1 / 950.5 | 11.2 / 8.7 | 28.6 / 18.4 |
| Paradex | 15,220 | 417.5 / 393.7 | 1.9 / 1.6 | 27.8 / 21.3 |

Six private profiles pass: Bitunix, KuCoin futures/Spot, Binance, Paradex, Bitrue.
Module byte counts: Bitunix856206, KuCoin819125, KuCoinSpot721043,
Binance969416, OKX861779, Paradex1224575, Bitrue850131. Each under1258291.
All source/native/compiled/consumer checks above remain current. Actual PR review
and green CI are still required; no live private credentials were used.

## Published review and required c44f22bd8e conflict integration

Publishedfcc1156ea7 receives3independent adversarial PASS verdicts after matching
actualPRhead/title/body/diff: private/trading/history, public/hosts/SDK, and shared
harness/integrity/performance accounting. All37prior findings/resolutions are now
posted; round1PASS summary also posted. No review claimsCIgreen or waivesBitrue.

GitHub reportsCONFLICTING, preventing fullCI. Fetching finds60landingcommits to
c44f22bd8e from wire-fixes, leverage-side and Binance/Bybit audit PRs. Merge now:
resolve8files in3disjoint reviewed groups (coreStore/wire/JS reader; Binance
TS/PM/native trading; gate/replay). Preserve all incoming revision1wire identity,
midpoints, display names, side rails, recovery ownership and native cancellation
proofs, while retaining this lane's ticker indices, scoped cancellation fences,
strict oracles/auth/physical sockets/clock/CPU/memory and finalizer controls.

Bitunix is coupled-leverage/global-mode like its TS adapter. Adapt only the new
sharedCommand optional fields (`side: _`, `symbol: _`) as the incoming coupled
venues do; retain existing leverage/global-mode behavior and addNone to legacy
native fixtures. Root owns this3file adaptation. Re-read the revisedwire and
leverage spec before verification. Then independent union audits, refresh affected
native/source/compiled proofs plus mandatory Bitunix/Binance gates, push resolved
head, freshactualPRreviews and ALL41greenCI. Do not chase anothermovingbase unless
GitHub reports a new conflict. Bitrueextra public remains mandatoryCIverification.

All c44 conflict unions frozen and independently audited PASS. Core preserves
revision1 layouts/reservation widths, ownedidentity, symbol-keyedsettings and
our exactlegacycurrency/BCM1 bytes/O(1)indices. Binance native/USD-M equal the
incoming stronger proof implementation; PM keeps both-family Promise.allSettled
and successful-family-only commits. Coupled Bitunix command adaptation and SDK
factory/CID/credential/recovery union independently PASS. Harness keeps every
strict guard, forwards new optional command args and projects stored native
identity/per-leg triggers through existing comparison fields.

Review catches legacy fill identity still resolving after catalog replacement.
Capture the current legacy symbol at delivery as well as modern stored symbols;
only absent catalog permits late fallback. Production-source regression fails
before this correction and passes afterward; source restored byte-for-byte after
the temporary probe. 76focused replay/polling controls PASS; independent follow-up
review PASS. Preserve /tmp/bitunix-c44-legacy-fill-fail-before.log.

First855helper run has854pass/1flakyfixture: the idle-fence test's100ms timer
legitimately spends1ms before conversion from real startup time. Make this exact
logical100ms unit start its own active clock before scheduling; exact100assertion
stays. Production timers/deadlines/clock modules unchanged. Fresh855helpers,
44typecheck/28lint PASS; root source tests still running. Distinct corrected
logs /tmp/bitunix-c44-fixed-* retain the first failed aggregate.

Fresh combined release+454compiledSDKcases PASS. Initial engine suite stops at
120pass/1oldfixtureexpecting deltas after market replacement; incoming contract
requires a full replacement. Updated fixture requires exact3identities/nativeIDs,
marketrefs/values in the serialized full table, THEN exact indexed delta after
drain, retaining all delist/relist lookups. Focused114enginecontrols PASS. Initial
host109pass/1onlybalanceerror-vocabularyfixture mismatch: retain exact RangeError
'Malformed balance section' for its bounds helper; all other readers keep their
truncated predicate. All110hostcontrols PASS. No production change for either.

Re-run full native checks with updated test source and finish remaining consumer
checks using distinct native-fixed/consumers-tail logs. Existing454compiled proof
stays current (test-only follow-ups). Then mandatory Bitunix/baseBinance private/
public4gates before resolved-head push. All other modes/venues, including every
Binance mode and Bitrue public, remain strict exact-head CI requirements; do not
repeat the extra15localprofile batch. No sampling/output/performance waiver.

The remaining source failure is one incoming PM race fixture expecting a single
symbol DELETE. Our preserved paired implementation correctly sends regular and
conditional DELETEs; update exact expectedcount to2 for both USD-M/PM. Both replies
stay pending until the later order is inserted; originalCanceled/laterOpen checks
unchanged. All19focused cancellation tests PASS and independent test audit PASS.

Consumer-tail's firstbook wire control PASS, second wire-contract example hits
spawnSync cargo ETIMEDOUT at unchanged120s while sharing the target/build lock
with the full native rebuild. Preserve that failed tail log; no consumer result
claimed for the second example. Root scheduled both Cargo users concurrently.
Resume the wire/nativeconsumer tail serially AFTER the native batch finishes;
keep timeout120s and every wire assertion unchanged. Product checks alreadyPASS,
454compiled SDK and110host proofs remain current. Source-last batch refreshes
root44/28/27 after the PM test-only correction. No performance job is running.

Source-last is fully green: 44typecheck/28lint/27test tasks, 7,885SDKsource
cases in each runner. Native-fixed passes strictnativeClippy/bans/fmt and
2,584tests across177groups beforePhemex; its sole failure is the incoming
Phemex stale-read fixture assuming two requests dispatch immediately despite
the retained real3RPS account budget. Test-only correction waits on actual
emitted timers until both GETs dispatch, requires exactUSDT/USDC identities,
and preserves all stale-response/mode/margin/leverage assertions. Independent
review PASS; production pacing and settings/account authority unchanged.

Root now runs consumer-serial first (framewire and nativefixture PASS), then
the completePhemex crate and the remaining Polymarket/TxFlow/Weex/Woo/WASM
packages, allworkspace docs and WASMClippy. No concurrentCargo consumers.
Then run only the four mandatory Bitunix/Binance gates on current source;
allother31rowmatrix proofs remain exact-head CI prerequisites. Wind-down
instruction: finish thisPR, merge onlygreen, write finalhandoff, stop; noqueued
follow-up work.

Consumer-serial passes framewire, nativefixture, serveradapter, server/MCP/CLI/
ReactNative loaders and packagedMCPbuild/smoke. Lifecycle has8PASS/1incomplete:
TS backing-buffer baseline at replay.mjs:765 fails unchangedsettlement before
constructor/cpuStart, so no measured replay result. Preserve originallog
/tmp/bitunix-c44-consumers-serial-live-smoke.log. Plan one fresh unchanged
lifecycle run only AFTERnative stops and existinghostqualification succeeds;
use the existing capture-output variable to retain its private tape even if
itfails. No completed performancefailure is retried; no sampler/deadline/
quietwindow/cap/oracle changes. Independent review of this admission pending.

Allremaining native checks PASS: 486cases across29groups plus2,584pre-Phemex
cases gives3,070distinctnative cases/206groups, zeroignored. Allworkspace
docs, globalfmt, strictPhemexalltargetClippy and strictWASMClippy PASS; earlier
allworkspacenativeClippy/bans remain current (productionunchanged since).
Consumer loader totals20 (server11/MCP5/CLI2/RN2), packagedMCP smoke PASS.
Independent admission audit approves exactlyone qualifiedlifecycle attempt:
originalstackpoints to baseline765 and emptyoutputdirectory, not a measured
performance outcome. Native jobs are stopped beforequalification/replay.

## Overseer load-rule override: publish for strict CI verification

2026-10-20T15:02UTC overseer directs focusedlocal checks only with
CARGO_BUILD_JOBS=4; no localWASMrelease/timing/size/sweep jobs. Push and let
CI run these gates; merge onlygreen. This supersedes the earlier localfourgate
beforepush plan. Stop only our waiting shell51203/qualifier51217 before ANY
freshlifecycle replay was admitted (69419 exits143; noqualifiedreplaylog).
Latest aggregateCPU700.7>500; its qualification log preserves everyreading.
No local releasegate script was started and no new performance sample exists.

Publish current reviewed/source-verified checkpoint and explicitly label all
pre-c44 performance receipts historical. Actualheadround2 source reviews,
all41exactheadCIchecks including mandatoryBitunix/Binance gates, complete
Bitrue and lifecycle remain prerequisites to merge. No deadline, sampler,
CPU/memory/module cap, inputfixture or equality/oracle change. No completed
performancefailure is retried unchanged. Then winddown, nofollowupwork.

## Required second landing merge: 3686c4ffa1

Publishedfea43c36b7 GitHubreportsCONFLICTING. Fetch+merge newlanding3686c4ffa1
(socket supervisor, Binance/Bybit auditfollowup, Paradex/emitter fixes).
Resolveonly3conflicts gate.mjs/gate.test.mjs/kraken-protocol.mjs: retain our
strictcallable adapters/rawauth/memory/identity controls plus incoming duplicate
JSON-key checks, independent Coinbase midpoint proof, live-smoke proof and
current Bybit owner-selected Partial replacement semantics. No stale amend-
in-place divergence note. Auto-merged incoming private/trading implementation
byte-identical to landing, Bitunix economic/fill/cancel guards unchanged;
independent privateunion audit PASS.

Under newloadrule rootfocused engine+Bitunix checks PASS:127engine+90Bitunix
nativecases (217total), touchedcrate format/strictalltargetClippy/bans;75TS
cases across6touched/integration files PASS. CARGO_BUILD_JOBS=4. No local
WASMrelease, timinggate, size matrix or replay started. Currenthead release
gates remain delegatedto all41strictCIcontexts beforemerge.

Secondmerge unions independently PASS: public/core/SDK/feed/supervisor;
private/trading/netPNL/fill/cancel; and crossaudit3conflictedharnessfiles.
Focusedgate/Kraken/uniqueJSON/CoinbaseMidpoint controls138/138PASS, zero skipped.
Coinbase-family exactrequests comparison is restored after the existing bounded
fixCoinbase proof; newnegativeCLI test retains unknown-request rejection.
Midpoint comparator uses actualtape, only proven fullrow cosmetic ticker cases
may differ; books/auth/account/request/metric checks remain independent.
Kraken duplicate validator is shared while exactBigInt nonce/signature/activation
fences stay intact. Commit and push resolvedmerge now, actualPRround2reviews
thenall41strictCIgreen beforemerge. No localrelease/timing/sweep executed.

## Published round2 / c34 CI repairs

Actualpublished c34e4153be isMERGEABLE. Public/host andprivate/trading/query
adversarial reviews PASS; harness review findsP2: Coinbase's strictmidpoint
proof requires isSpot/isInverse/isExpiring omittedby sharedmarketprojection.
Production-source regression fails before (marketidentity/contractunits differ).
Retain these flags fromactualTS/native markets forCoinbase family, same as
Bybit; unchangedproof rejects eachflippedflag forbothaliases.170focused
replay/gate/midpoint casesPASS after, independentreviewPASS.

CI nativeClippy/testfailto compilePhemexsockettest: incomingbyvaluehelper
expectsrawEngine while ourfixture usesits causal-timerwrapper. Importexisting
crate::support::Engine inhelper; everyflap/backoffassertion unchanged.111Phemex
tests plusstrictalltargetClippy/fmtPASS, independentauditPASS.

CIhelpers hasoneflakyunit: memory.test's isolatedNodechild canreclaim36foreign
ArrayBufferbytes, invalidating assertedzero. Preserveactualsignedobserveddelta
exactly; addfrozen countercontrols forsingleWASMaddition, ordinary4096frame
andnegative36 reclaim.17memorycasesPASS. Productionmemory.mjs formula,
signedcountervalidation, quiet100ms/deadline1s/sampler unchanged. Thisunit
provesexactaccounting, not independentrealV8exclusion ofWASMfromarrayBuffers;
removed empiricalzeroassumption isnot claimedasproof. IndependentauditPASS.

BothrootCIjobs failonly staleMCPconfignegative expectingBitunixRustunavailable.
Replace withpositiveBitunixRustfutures/default-unsetcase; retainallothernegative
validationcases andproductionguards.17focusedMCPconfigcasesPASS.

CurrentCIalso hascompleted HyperliquidSpot retained-memoryFAIL (rounded1.9MB/
1.9MB, CPU/correctnessPASS), MEXCRustlogicalstartup timeout (incomplete), plus
same memoryunit failure inOrderly/KuCoin/KrakenSpot. Preserve c34logs; no
unchanged completedperformancefailure retry. Investigate soundHLIretained/
monotoniclinearpeak reduction andMEXCcausalstartup before nextpush. No local
WASMrelease/replay/timing/size job underoverseerloadrule. CurrentCI_NOTgreen.

HLI119focusednativecases/strictalltargetClippy/fmtPASS for exactcount context
scratch allocation (one numericdecode; fulljson.end validation beforeoutputs).
Oldallocation fail-before showscapacity4 vs1, not a printed302→512pair; preserve
that distinction. Independent allocation/join/malformed/atomicity reviewPASS.
ActualCIretainedmemory/CPU outcome stillpending; doNOTclaim a page saving.

MEXCsecondaudit provesrecording isrecoverable:7,965BTCupdates ononesocket are
contiguous43065..51029. FirstREST1206anchor43035 cannotbridge, but secondREST
1823anchor43125 has exactsuccessor43126 at1602 beforeitsresponse. Both TS/native
currentlydropallvalidbackoffupdates, discarding requiredsame-interesthistory.
Plan pairedproductionrepair preserving bounded2000delta buffer throughretry
while invalidating oldrequest/visibleversion. Strict+1 and atomicfullcandidate
validation stay; actualsocket/interest replacement, malformedinput andoverflow
hardreset buffer. Focusedtestsfirst; no retiming/fixture/readiness/oracle changes.

MERGE HOLD (overseer): doNOTmergePR1309 oranyPR into feat/rust-rewrite until
securityPR1351lands andoverseer explicitlylifts hold. Keepfixing/pushing/CI.
No mergewatcher isactive; all41green andactualheadreviews remainnecessary
but do not override thishold. Finalwinddown stillrequiredafterauthorizedmerge.

MEXCtest-first checkpoint:2newnativecontrols FAILasexpected (10existingPASS);
2newTScontrols FAILasexpected (19existingPASS). Theyprove ordinarysame-socket
lagginganchor recovery and genuinegap staysunpublished beforecompletechain.
Production edit nowauthorized underreviewed boundedatomic plan.
Gate failureonly rawper-runmetric diagnostics addedafterallmeasurements/
comparisons for actionableCIbytes;20focusedCLIcasesPASS, independentauditPASS.
No sampling/threshold/acceptance/outputsemantic change.

REVERT GUARD (overseer mandatory): latestfetched3a6737115e knownsilentlyreverted
PR1354 viaPR1361; restorelane isfixing. DoNOTapplythatknownbadlandingstateyet.
Keep current1354proofs, finishMEXCcausalrepair, thenfetch/mergerestoredlanding.
BeforeeveryPRmerge, withlandingmerged, run exact `git diff --stat
origin/feat/rust-rewrite HEAD` and auditeveryfileagainstintendedPRscope; any
unexpecteddeletion/revert blocksmerge. Neverresolvewholelandingconflicttoours
or droplanding-addedfiles. Premerge532diff290files includesunmergednewlanding
changes and CANNOTpassfinalguard; notclaimedreviewed/final. MERGEHOLDcontinues.
