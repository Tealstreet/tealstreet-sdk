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
