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
