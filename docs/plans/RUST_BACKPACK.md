# Backpack Rust port

Branch: `feat/rust-backpack`. Spec: `RUST_REWRITE.md`; process: latest
`origin/feat/rust-rewrite:docs/plans/VENUE_PORT_BRIEF.md`.

## Phases

1. Public data: new sans-I/O venue crate, borrowed wire structs, streaming
   markets/ticker/mark seeds, on-demand depth snapshots plus buffered sequence
   bridge, bounded gap recovery, reconnect/watchdog, candle streams and reads.
   Verify sequence bridges, strict subsequent deltas, stale snapshots,
   reconnects and empty candles. Review with a sub-agent before committing.
2. Private data: ED25519 canonical signing with safe-cex-crypto, separate
   private socket, authenticated subscriptions, periodic REST reconciliation,
   global leverage/cross settings, net position flips, execution deduplication,
   and per-domain revision guards. Failed reads preserve prior state. Review.
3. Trading: sequenced jobs for single/native pair placement, brackets and
   triggers, cancel/replace amend, cancels, account-wide PATCH leverage;
   reject unsupported mode/margin changes. Preflight every caller client ID
   and the complete batch before any write; canonical uint32 decimal IDs.
   Validate cancel/replace before cancelling; never retry writes. Review.
4. Queries: streaming fills/orders/funding/public trades in engine JSON
   shapes, bounded pagination and explicit errors for unavailable queries.
   Verify official payload examples and boundary filters. Review.
5. Harness gates: wasm/SDK registration, venue harness hooks, deterministic
   private tape using public test credentials, live two-book public recording
   and compact fixture, generic replay extensions with Binance regressions.
   Run bans, fmt, clippy, tests, wasm lint/build, public/private venue gates,
   Binance public/private gates and repository checks. Review and record metrics.

## Delivery

After all gates pass, merge `origin/feat/rust-rewrite` into the lane (resolve conflicts under the overseer standing authorization,
preserving the nested workspace), rerun affected checks, push, open PR to `feat/rust-rewrite`, run
adversarial sub-agent reviews and post findings, fix and repeat, merge PR and
cut a fresh follow-up branch. Never touch master, rebase, or force-push.

## Progress

- Read current spec, port brief, root/package/venue guidance, engine/Binance
  seams and TS Backpack behavior. Verified official signing API documentation.
- Public phase verified: bans, crate fmt/clippy, 7 deterministic tests passed.
  Sub-agent review found four issues (recovery streak reset, symbol isolation,
  preserving gap evidence, malformed candles); all fixed and re-reviewed.
  Corrected conflicting older recovery documentation to match TS behavior.
- Live markets endpoint checked: current payload uses nested filters and null
  maxQuantity, supported alongside top-level increment fields.
- Private phase in progress. PATCH transport groundwork remains uncommitted until
  used there. Local corepack was absent; installed a temporary executable in
  /tmp/backpack-tools and installing frozen Yarn dependencies for later gates.

- Private phase checkpoint before overseer-requested workspace merge: first
  private suite and signing tests passed (14 total), clippy passed. Review
  found Y-vs-Q trigger size, TriggerFailed status, B-vs-b entry precedence,
  overlapping snapshots, and read-chain failure coupling. Fixes are in the
  checkpoint; expanded tests and final review remain pending. These inherited
  TS bugs must be declared in KNOWN_TS_FIXES and the private phase commit.
- Overseer requested merging nested workspace commit b32847fca6 now. Temporarily
  remove old explicit Backpack workspace registration before the structural
  merge, then move crate to sdk-rs/venues/backpack (glob member discovery).
  Re-read updated brief; audit crypto leftovers for provably unused code.

- Merged origin/feat/rust-rewrite cleanly at fa9798de88 and moved crate to
  sdk-rs/venues/backpack. Workspace discovers it without members edits.
- Expanded private suite passed before relocation: 17 tests total. Native
  PATCH transport check also passed. Post-move verification in progress.
- Crypto cleanup audit: wallet_utils module has no callsites anywhere in
  SDK, docs, CI, or tests; remove that obsolete wallet facade. Keep uuid:
  Binance trading still calls it. Other signing modules serve planned venues.

- Private review final: no remaining blockers after HTTP-only endpoint fallback
  and subscription-error reconnect fixes. Expanded suite now includes failed
  startup domain continuation, overlapping read ordering and rejected auth.
- TS corrections recorded in gate comments; harness phase will apply only
  tape-derived normalization for documented differences, never copy Rust state
  into TS as an oracle. Still need final tests/clippy result below.

- Private phase final verification: all 21 tests, clippy and bans pass.

- Trading phase verified: 31 total tests, clippy, sub-agent review and fixes.
  Jobs cover caller uint32 preflight, native pair correlation, brackets,
  cancel/replace, best-effort cancel lists, cancel-all, global PATCH leverage,
  fixed mode/margin refusals and automatic triggerBy repairs. Response guards
  preserve newer fills/new orders and protect global settings from stale GETs.
  Internal repair jobs have a separate u64 namespace; never retry a write.
- Repository typecheck passed all 44 tasks. Root lint/tests are running.
- Next: streaming rare queries, then wasm registration/tapes/performance gates.

- Query phase verified: 8 history regressions, crate clippy, final sub-agent
  review clean (39 total tests). Fills/order/funding/public trades stream typed
  rows; offset paging is capped at 50 pages and rejects repeated pages or
  malformed/error payloads. Fractional fill ranges broaden integer API bounds
  while preserving exact local filters; account-wide fills page past spot rows.
  Income supports the documented funding ledger and rejects other ledger kinds.
- Root lint passed all 28 tasks. Root tests exposed pre-existing runtime-tree
  violation rust/rust.exchange.wasm.test.ts (6,488 passed / 1 failed); move that
  test to audit-regressions during harness integration and rerun checks.
- Harness integration started: wasm and SDK registration, relocation script fix.

- Harness integration checkpoint: Backpack registered in wasm/SDK, caller IDs
  enforced in the SDK facade, default/extra recvWindow forwarded, existing wasm
  test moved out of the runtime tree. Independent Node ED25519 replay checks
  REST and private subscriptions with public deterministic test keys.
- Review strengthened replay: consume each write once, reject missing/extra
  writes, preserve exact decimals and wire types, bound readiness/commands,
  enforce Backpack write ordering and compare every repeated gate run.
- Live depth uses string update IDs; decode strings/numbers exactly as u64,
  with an above-2^53 regression. Conditional amend retains execution limit and
  original trigger source; TS source-loss correction is documented in the gate.
- Native tests and harness tests pass at this checkpoint; final gate suite is
  still running. First release wasm build is compiling the crypto dependencies.
  Live public recording awaits that build. No performance claim yet.
- Late integration review complete after clamping receive windows exactly as
  the TS auth helper does. Added actual private-header checks for fractional,
  oversized and invalid values. All 42 native tests pass. Harness re-review
  confirms no remaining blockers after the five regression tests.
- Repository typecheck passes (44 tasks); lint/tests and final wasm checks run.
- Live public recording completed: 20 seconds, 5,656 records; trimmed six-second
  fixture has 1,092 records. Recorder repaired for the landing host's async HTTP
  response bodies and corrected its diagnostic frame counters.
- Live/documented ticker payload uses o/c/v/V, while TS reads P/q. Support both
  formats in Rust; declare the tape-derived correction. Null bookTicker sides
  clear quotes. Depth deltas must not supply best quotes from changed levels;
  use authoritative bookTicker only. Review and two new regressions pass.
- MARKET normalization now projects both implementations to the spec's amount
  and price increment columns; TS-only base/quote display decimals are outside
  this frame contract. Public review confirms this matches the spec.
- All 44 native tests and five harness tests pass; bans/fmt/clippy and wasm
  clippy pass. Backpack private correctness gate PASS (21 frames, two markets,
  complete write/order/signature checks). Rebuild for public wire fixes underway.
- Root default test concurrency exposed unrelated Hyro/registry import timeouts.
  Repeat with VITEST_MAX_WORKERS=2 and serial Turbo tasks; no test changes.
- All four venue replay gates PASS on the final build. Public medians (3 runs):
  Backpack 1,086 frames/100 markets/91 tickers/two books: CPU TS 172.5 ms,
  Rust 84.8 ms (2.03x); steady CPU 95.9/44.1 ms (2.17x); retained memory
  1.7/1.6 MB (1.10x), including 1.1 MB wasm linear memory.
  Binance 871 frames/917 markets/783 tickers/three books: CPU 234.4/86.4 ms
  (2.71x); steady 139.7/35.3 ms (3.96x); memory 2.3/2.0 MB (1.15x).
  Generated private gates: Backpack 21 frames, Binance 14 frames, both PASS.
- Real SDK wasm tests: 5/5 PASS; host tests: 2/2 PASS; wasm clippy PASS.
  Root typecheck/lint PASS. Default test budget still hits pre-existing Hyro
  import timeouts; verifying the SDK suite separately with a 30-second budget.
- Merged the landing branch at e2dc51f927, retaining its __tests__ wasm-test
  location plus Backpack coverage; all five wasm tests passed after the rename.
  PR #1299 is open as a draft into feat/rust-rewrite.
- Adversarial review round 1 posted six accepted findings on the PR. Fixed:
  post-only loss on REST/WS amend, unsafe attached stop-limit protection,
  startup seed retry gaps, unsynchronized SDK auth time, concurrent cancel
  rollback, and public query results escaping replay comparison.
- Follow-up review fixed class-based transport delegation, preservation of
  post-only/TIF/reduce-only during automatic trigger repair, and cancel/cancel-all
  acknowledgement guards across REST creation-time snapshots. Guards now use
  stream event stamps; concurrent cancellation state is bounded by active jobs.
- Re-review is clear. All 49 native tests and seven harness tests pass. The
  private tape declares the exact legacy TS stop-limit request separately;
  Rust must match the correct market-protection request (tested negatively).
  Final wasm rebuild, gates and SDK clock/class-transport verification pending.
- Repository tests: default run passed 6,482 SDK tests and timed out seven tests
  in two unrelated import-heavy files. Both files pass in isolation (15/15,
  one worker, 30-second budget); other root packages passed. No product or test
  code changed for these timing failures. PR-range gitleaks is clean; the initial
  branch-push scan flags an inherited Binance test vector before this PR's base.
- Final reviewed-code verification (283a87b0d2): all four replay gates PASS.
  Backpack public median of 3: CPU TS 159.4 ms / Rust 88.7 ms (1.80x), steady
  92.4/47.3 ms (1.95x), retained memory 1.7/1.6 MB (1.09x; wasm 1.1 MB).
  Binance public: CPU 214.7/86.3 ms (2.49x), steady 126.0/31.4 ms (4.02x),
  memory 2.3/2.0 MB (1.15x; wasm 1.6 MB). Both private gates PASS.
- Final checks PASS: 49 native tests, seven harness tests, five real-WASM SDK
  tests including synchronized-clock/class-transport regression, two host tests,
  bans/fmt/native and wasm clippy/release build. Root typecheck 44 tasks and
  lint 28 tasks PASS. Review findings/remediation posted on PR #1299; all three
  sub-agent re-reviews are clear. CI completion and landing remain pending.
- Final fetch brought in Bybit PR #1298 and the green-CI landing rule. Merged
  with standing conflict authorization, preserving landing structure, both
  client-ID rules/SDK tests/WASM constructors and Bybit nested streamed envelopes.
  Shared Tape retains exact writes/auth/bounds plus canonical volatile requestId
  URLs for Bybit. Recorder retains whole request bodies and both response forms.
- SDK and harness merge re-reviews are clear; removed duplicate amend dispatch.
  Eight harness tests and engine/Backpack native suites pass after resolution.
  Rebuilding the combined three-venue wasm and rerunning all six venue gates,
  including Bybit, before waiting for green CI and merging PR #1299.
- Combined integration first failed Backpack memory by ~2% with three venues.
  Kept the gate unchanged and removed a real temporary depth allocation:
  decode sorted level vectors directly into their final representation and move
  them into Book. A pointer-identity/zero-level regression verifies ownership.
  Review is clear; Backpack public now PASS (3 runs): CPU 183.3/102.7 ms
  (1.79x), steady 101.3/56.6 ms (1.79x), memory 1.8/1.7 MB (1.04x).
  Added Backpack CI public/private gates and measured combined wasm 1,747,356
  bytes with an explicit 1.8 MiB budget. Other post-merge gates are running.
- Combined-build verification is complete: all six replay gates PASS. Backpack
  public numbers above include 1,179,648 bytes of wasm memory; exact retained
  medians are TS 1,753,696 / Rust 1,693,640 bytes. Private correctness: 21 frames.
  Binance public (3 runs): CPU 204.6/90.5 ms (2.26x), steady 119.6/36.4 ms
  (3.28x), memory 2.3/2.1 MB (1.11x); private 14 frames PASS. Bybit public
  (3 runs): CPU 412.4/299.3 ms (1.38x), steady 270.7/170.7 ms (1.59x),
  memory 3.1/2.2 MB (1.39x); private 18 frames PASS.
- Latest checks PASS: Backpack 49 tests, engine 11 tests, harness eight tests,
  real-WASM SDK seven tests, native/WASM clippy, bans, format and release build.
  Root typecheck (44 tasks) and lint (28 tasks) PASS. CI on the previous PR head
  passed; pushing the merged/optimized head and waiting for its CI before landing.
- Three-venue PR CI passed every Rust job and all six replay gates (Backpack
  public CI CPU 185.0/90.0 ms, retained memory 1.7/1.5 MB). While waiting for
  repository tests, OKX landed and made the PR conflict. Merged landing commit
  693ad4f139 with authorized conflict resolution, preserving all four venue
  constructors, client-ID rules, clock/transport handling, and SDK coverage.
- Separate sub-agent merge reviews retained OKX's strict body/staged-read,
  login/subscription/ping checks in the shared Tape with Backpack's exact
  writes/auth validation. Eleven harness tests pass; all 17 real-WASM SDK tests
  and both host tests pass. Backpack 49 tests, engine 15 tests, root typecheck
  and lint, workspace native/WASM clippy, bans and format pass.
- Four-venue release WASM is 1,791,671 bytes after retaining landing's name/
  producer-section stripping, within the unchanged 1.8 MiB budget. Backpack
  public/private gates pass; all eleven replay gates (including OKX's four
  private variants) are running before the next push and green-CI wait.
- Four-venue OKX public replay initially missed memory by 2,208 bytes. Fixed a
  real host allocation: frame readers no longer reserve unused string-cache
  slots before any string is decoded. Two regressions cover numeric-only
  readers and sparse/empty/multibyte cached decoding; independent review clear.
  All four host tests and 17 SDK WASM tests pass. Gates remain unchanged.
- All eleven integrated gates now PASS: public medians of three runs are
  Backpack CPU 150.9/73.6 ms (2.05x), steady 81.6/31.5 ms (2.59x), memory
  1.8/1.7 MB (1.03x); Binance CPU 169.9/75.7 ms (2.24x), steady 88.3/25.9 ms
  (3.40x), memory 2.3/2.0 MB (1.13x); Bybit CPU 268.1/175.2 ms (1.53x),
  steady 188.2/104.0 ms (1.81x), memory 3.1/2.4 MB (1.32x); OKX CPU
  908.8/832.2 ms (1.09x), steady 730.3/618.5 ms (1.18x), memory 2.6/2.5 MB
  (1.01x). Private gates PASS: Backpack 21, Binance 14, Bybit 18, and each of
  OKX/OKX EU/OKX demo/OKX EU demo 29 frames. PR-range secret scan clean.
- Three-venue head 666993c597 finished fully green repository/Rust CI. Pushing
  the reviewed four-venue head, waiting for its CI, then merging PR #1299 into
  feat/rust-rewrite and creating a fresh follow-up branch as authorized.
- Four-venue head 8dfb2ddf63 completed every repository and Rust CI job green.
  CI Backpack public CPU 139.7/58.6 ms (2.38x), steady 68.9/22.3 ms (3.09x),
  memory 1.7/1.5 MB (1.11x); all eleven CI replay gates pass. Before landing,
  Crypto.com PR #1303 merged and caused harness conflicts. Integrated target
  c63c3e5670 with standing conflict authorization and retained all five venues.
- SDK/crypto review confirms unchanged Backpack ED25519, IDs, clock, bound
  transport and windows. Shared Tape retains strict Backpack/OKX protocols and
  actively reuses Crypto.com's response correlation/exact JSON helpers. Incoming
  memory measurement counts retained frame ArrayBuffers and excludes GC from
  CPU timing; retained these accuracy improvements without weakening thresholds.
  All 22 harness tests pass, including integrated POST-read/write/signature and
  exact-integer regressions. Backpack 49 and engine 17 native tests, workspace
  native clippy, bans and format pass. Five-venue build/gates are pending.
- Actual replay caught a missing `stable` helper import left by conflict
  resolution. Exported the existing helper and imported it in the gate;
  subsequent no-undef review of gate/replay/tape is clean. No gate logic changed.
- All thirteen five-venue gates PASS (32c26cbabf + a70bf1f219): public medians
  of three runs, TS/Rust CPU, steady CPU, retained memory:
  Backpack 191.8/110.5 ms (1.74x), 102.1/54.5 ms (1.87x), 3.0/1.9 MB (1.58x);
  Binance 242.1/104.7 ms (2.31x), 137.2/42.3 ms (3.24x), 4.3/2.5 MB (1.75x);
  Bybit 502.7/343.5 ms (1.46x), 345.0/205.9 ms (1.68x), 6.0/2.7 MB (2.25x);
  OKX 1554.8/1236.9 ms (1.26x), 1254.3/952.4 ms (1.32x), 4.1/2.7 MB (1.55x);
  Crypto.com 185.7/132.3 ms (1.40x), 106.0/67.3 ms (1.57x), 1.8/1.4 MB (1.27x).
  Private correctness: Backpack 21, Binance 14, Bybit 18, Crypto.com 21, and
  each of four OKX variants 29 frames, all PASS. New memory numbers include
  retained backing buffers and TS worker egress, per landing's revised method.
- Five-venue WASM 2,034,746 bytes, within a measured 2 MiB CI budget. All 17
  SDK WASM tests, four host tests, 22 harness tests, Backpack 49/engine 17 native
  tests, workspace native/WASM clippy, bans/fmt and root typecheck/lint pass.
  All merge reviews are clear. Pushing the final five-venue head and waiting
  for its green CI before merging PR #1299 and cutting the follow-up branch.
- All thirteen five-venue gates passed, but WEEX PR #1301 landed before push
  and introduced conflicts. Integrated target 4046db3826, retaining all six
  constructors/client-ID rules and WEEX's authenticated WebSocket upgrade
  headers, ticker selection, bounded HTTP and stricter streamed envelopes.
- Independent merge reviews preserve Backpack exact bodies/IDs/signatures,
  ordered single-use writes and tape-derived corrections, Crypto.com audit
  correlation, and OKX auth/subscriptions. Fixed a future-only timed response
  being served early; all 32 harness tests now pass, with static no-undef checks.
- Host review found delayed callbacks evaluated engine calls before checking
  disposal. Added pre-call guards and five regressions covering delayed chunks,
  completion/error, signed read/write rejection and queued timers; re-review
  clear. All 18 real-WASM SDK tests and 14 host tests pass.
- Six-venue WASM is 2,498,285 bytes within a measured 2.5 MiB aggregate budget.
  Backpack 49 and engine 22 tests, workspace native/WASM clippy, bans/fmt,
  root typecheck (44) and lint (28) pass. All fifteen replay gates are running
  before the next push and green-CI wait. No gate thresholds were weakened.
- All fifteen six-venue gates PASS (6a684ef4d1), public medians of three:
  Backpack CPU 111.0/53.9 ms (2.06x), steady 56.8/22.3 ms (2.55x), memory
  3.0/1.9 MB (1.56x); Binance 185.5/81.2 ms (2.28x), steady 99.0/27.0 ms
  (3.66x), memory 4.3/2.5 MB (1.74x); Bybit 469.5/332.2 ms (1.41x), steady
  314.1/219.4 ms (1.43x), memory 6.0/2.7 MB (2.25x); OKX 1210.5/1148.8 ms
  (1.05x), steady 959.3/904.9 ms (1.06x), memory 4.1/2.6 MB (1.59x);
  Crypto.com 177.4/137.1 ms (1.29x), steady 115.6/74.3 ms (1.56x), memory
  1.8/1.4 MB (1.26x); WEEX 145.7/80.5 ms (1.81x), steady 74.9/30.2 ms
  (2.48x), memory 5.1/2.7 MB (1.89x). Private correctness: Backpack 21,
  Binance 14, Bybit 18, Crypto.com 21, WEEX 20, four OKX variants 29 each.
  All reviews/checks above pass; pushing this final head for CI and landing.
- Six-venue head 5cfd928d5c passed every Rust CI job/all fifteen replay gates
  and push repository CI. While the PR repository job was still running,
  BingX PR #1305 landed and caused conflicts. Integrated target 9ddec5d0a4,
  preserving all seven constructors/client-ID rules and Backpack behavior.
- Reviews confirm compact string interning/indexed tickers, metadata envelopes
  and binary socket handling preserve Backpack semantics. Kept sparse string
  caches, bound class transport and all pre-engine-call disposal guards. Added
  a delayed binary Blob/disposal regression. Re-review found decompression used
  replacement UTF-8 decoding; switched to fatal decoding with gzip/deflate
  malformed-ID/symbol regressions (no mutated frame delivered). Review clear.
- All 20 SDK WASM tests, 20 host tests, 49 harness tests, Backpack 49/engine 34
  native tests, workspace native/WASM clippy, bans/fmt, typecheck44/lint28 pass.
  Shared helpers retain exact ordered single-use Backpack requests/auth/oracle,
  independent account-presence checks, generation/subscription barriers and all
  other venue audits. Only public fixtures gate CPU/memory as documented; all
  three public performance thresholds remain strict. Private metrics stay
  informational, with correctness/request/write/protocol checks unchanged.
- Seven-venue WASM is 2,924,643 bytes within a measured 3 MiB aggregate budget.
  All seventeen replay gates are running before the next push/green-CI wait.
- Required brief replay gates all PASS on the seven-venue build: Backpack
  public CPU 112.0/48.5 ms (2.31x), steady 56.1/19.3 ms (2.91x), retained
  memory 3.2/1.7 MB (1.87x); private 21 frames PASS. Binance public CPU
  236.3/95.0 ms (2.49x), steady 143.3/35.8 ms (4.00x), retained memory
  4.2/2.4 MB (1.76x); private 14 frames PASS. Pushing the reviewed candidate
  for CI while auxiliary venue replays finish locally; all seventeen CI gates
  must pass before landing. The previous six-venue head completed all CI green.
- Seven-venue head 6b53912de3 completed all repository and Rust CI jobs green,
  including all seventeen replay gates. CI Backpack public CPU 124.8/48.6 ms
  (2.57x), steady 46.7/19.4 ms (2.41x), retained memory 3.1/1.5 MB (2.04x).
  Before landing, MEXC PR #1304 introduced another target conflict. Integrated
  target 63d521a860, preserving all eight venue constructors and Backpack's
  signing, client IDs, quantities and account settings. The target's production
  wasm size optimization is retained consistently for tests and replay builds.
- Independent integration reviews confirm optional side-leverage fields remain
  compatible with Backpack's global leverage; MEXC body normalization is scoped
  to MEXC, while Backpack keeps exact body types and ordered single-use writes.
  Shared replay clock uses recorded time for public timers and real bounded
  deadlines. Review found ordinal routing could bypass Backpack's subscription
  check on new recordings; require both ordinal and the actual stream topic.
  Full eight-venue build, nineteen replays and current-head CI remain pending.
- A second adversarial finding exposed public commands waiting forever on the
  newly virtualized timers. Added bounded timer pumping during those commands,
  monotonic replay time, and delayed-command/real-timeout regressions. Ordinal
  and clock fixes re-reviewed clear; all 65 harness tests pass. Backpack 49 and
  engine 38 native tests pass, as do workspace native clippy, bans and format.
- Eight-venue production WASM built successfully: 2,702,020 bytes with the
  landing branch's size optimization, below the unchanged 3 MiB aggregate CI
  budget. Workspace WASM clippy, root typecheck44/lint28 also pass. Final SDK
  tests and nineteen replay gates now run against this exact artifact.
- The first eight-venue live gate exposed a third clock integration case:
  Backpack schedules ticker subscriptions after two seconds, so socket waits
  also need to pump active public timers. Added bounded socket-wait pumping
  without bypassing ordinal/topic checks, plus unrelated-topic then delayed
  required-topic coverage. All 66 harness tests and independent re-review pass;
  restarting the replay gates. Production wasm is unchanged.
- Required brief gates all PASS on the final eight-venue artifact: Backpack
  public CPU 200.0/113.3 ms (1.77x), steady 112.9/58.0 ms (1.95x), memory
  3.3/1.7 MB (1.93x), private 21 frames; Binance public CPU 256.2/128.3 ms
  (2.00x), steady 154.1/53.9 ms (2.86x), memory 4.3/2.4 MB (1.82x), private
  14 frames. SDK20/host23, harness66, Backpack49/engine38, both workspace
  clippy targets, bans/fmt, typecheck44/lint28 and PR-range secrets scan pass.
  Pushing this reviewed candidate for CI while auxiliary venue replays finish;
  all nineteen CI gates and repository jobs must be green before landing.
- Eight-venue head 46c5a726ad completed every repository and Rust CI job green,
  including all nineteen replays. CI Backpack CPU 143.6/58.0 ms (2.48x), steady
  67.3/22.5 ms (2.99x), retained memory 3.2/1.5 MB (2.09x). BloFin PR #1300
  and MEXC parity PR #1311 landed before the final repository job completed,
  causing new conflicts; the guarded landing command correctly refused them.
- Integrated target a4d9e2ec84, preserving all nine venues and Backpack behavior.
  Reviews confirm scalar leverage/quantity compatibility and keep sparse string
  caches plus side-column regressions. Shared memory settling is now a tested
  helper with real-time pauses under recorded public clocks. Nine-venue wasm
  measures 2,863,225 bytes within the unchanged 3 MiB budget. Backpack49/engine44,
  bans/fmt, both workspace clippy targets, typecheck44/lint28 pass. Harness
  matching/oracles, final SDK/host tests and twenty-one replays remain pending.
- Harness review restored `effectiveAt` support while integrating BloFin's
  opt-in typed matcher/default trigger references and native WS comparisons.
  Avoided copying BloFin margin values from Rust into the oracle; only the
  independently defined empty-position aggregate correction remains. All
  73 harness tests, undefined-name checks and independent re-review pass.
  SDK21/host25 pass on the nine-venue artifact. Twenty-one replays are running;
  Backpack public already passes. No public performance threshold changed.
- Required brief gates all PASS on the nine-venue artifact: Backpack CPU
  199.0/119.4 ms (1.67x), steady 116.8/69.4 ms (1.68x), memory 3.3/1.8 MB
  (1.86x), private21; Binance CPU 247.6/114.0 ms (2.17x), steady 142.4/50.4 ms
  (2.82x), memory 4.3/2.4 MB (1.77x), private14. Secret scan clean. Pushing the
  reviewed candidate while auxiliary replays finish; all twenty-one CI gates
  and repository jobs must pass before merging PR #1299 into feat/rust-rewrite.

### Port-review lows plan (Codex, 2026-10-21)

1. Make REST order quantity nullable/missing with strict present-value decoding.
   Permit omission only when positive triggerPrice and triggerQuantity prove size;
   reject blank/malformed numbers and unproven ordinary orders. This deliberately
   tightens the prior missing ordinary quantity default of zero.
2. Share that admission rule across account/history/trigger repair/replacement
   ACK mapping. Preserve present-quantity precedence and the separate WS q/Y
   schema, child/trigger identity, cancellation provenance and terminal guards.
3. Validate before staging a repair cancellation or removing the old order on
   replacement. Reproduce null-trigger failure; test valid mixed reads, malformed
   fallback/no-write, history and replacement already filled before ACK.
4. Focused Backpack/Binance tests, touched fmt/strict Clippy/bans, independent
   review, audit/spec/caller notes, exact-head CI and separate final merge guard.

- Reproduced explicit-null trigger quantity blocking the entire open-order read
  before the existing native triggerQuantity fallback could run. REST quantity
  now decodes as optional with strict present numbers; blank, bad, boolean and
  nonfinite values still reject. Absence/null needs positive triggerPrice and
  triggerQuantity with executedQuantity in [0, amount]. The shared guard applies
  to account mapping, order history, trigger repair and replacement ACK mapping.
- Missing ordinary quantity deliberately tightens prior default0 admission to
  explicit rejection. Present quantity precedence is unchanged: account selects
  trigger quantity only with a trigger price; history/repair retain their existing
  trigger-field precedence. WS q/Y remains separate and unchanged. No native
  identity or quantity is guessed from cached state.
- Invalid reads preserve known orders and cannot queue a repair cancellation.
  Replacement ACK validation precedes old identity removal; a replacement already
  filled on WS is never reopened by its later ACK. Limit price, TIF, post-only,
  reduce-only and default-trigger repair contracts stay intact.
- Primary field meanings: https://docs.backpack.exchange/ (triggerPrice and
  triggerQuantity describe conditional trigger size). This guarded tolerance is
  not a claim of live validation for every nullable response schema.
- Validation:343 focused Backpack/Binance tests pass, touched formatting, strict
  all-target Clippy and bans pass. Independent read-only review found no blocker.
  Valid mixed null/missing reads, history and terminal-before-ACK repairs are
  covered along with unproven/no-write and malformed replacement identity cases.
  Local disk exhaustion was resolved by cleaning9.6GiB of generated artifacts
  for already-merged venues in this worktree; source files were not removed.
