# BloFin Rust port

Branch: `feat/rust-blofin`. Target: `feat/rust-rewrite`; never master.

## Phases

1. Public data: typed borrowed wire records, streamed instrument/ticker seeds,
   bounded subscription frames, sequenced books and books5, candles, socket
   watchdog/reconnect. Verify documented snapshots, gaps, reconnect and candles.
2. Private data: hex-then-base64 HMAC signing, authenticated private socket,
   independent account/order reconciliation, atomic cursor pagination,
   stale-snapshot guards, settings and bounded incremental execution tracking.
3. Trading: preflight entire commands, native contract sizing, caller IDs,
   normal batches, conditional/TP-SL jobs, cancel/amend and account settings.
   Writes are never retried after uncertain transport failure.
4. Queries: candle/history/fill/income/trade reads, typed JSON result shapes,
   bounded pagination and request validation.
5. Harness and gates: register wasm/worker/harness, documented private tape,
   live 20-second two-book public fixture, parity/performance gates and Binance
   regressions; repository typecheck/lint/tests. Record exact measurements.

Each phase ends in a sub-agent review, fixes, verification and a focused commit.
After all gates pass, merge origin/feat/rust-rewrite, rerun affected gates, push,
open a PR into feat/rust-rewrite, post adversarial review findings, fix/review,
merge that PR, and create a fresh follow-up branch. No rebase or force push.

## Risks reviewed before implementation

- Sizes on orders/positions/fills are native contracts; books remain native.
- Fills use differences of cumulative quantity AND traded value, retaining
  terminal rows (500-order bound) to suppress replays and regressive frames.
- Normal/TPSL/trigger snapshots must all finish through an empty page before
  replacing orders. Failed reads preserve last-known-good state.
- Every socket has ping plus inbound-traffic liveness; reconnect invalidates
  full-depth book sequences and restores all active subscriptions.
- TP/SL cancel-replace preserves the sibling leg and uses new generated IDs.
- Account-global modes and per-market settings survive position closure.

## Progress

- Read port brief, Rust spec, root and BloFin package guidance, engine and
  Binance reference. Fetched current process instructions from origin.
- Official payload reference: https://docs.blofin.com/index.html .
- No implementation or gates yet.

### Phase 1 verification

- Added public crate, bounded subscriptions, books/gap recovery, candles and
  liveness watchdog. `ArrayAt::EnvelopeField` retains API status around streamed
  data without retaining the list; validates separators, item syntax and UTF-8.
- Phase review found and corrected books5 ordering corruption (also fixed in
  TS, with whole-book gate comparison retained), duplicate candle emission,
  malformed-list acceptance and candle-window request differences.
- Rust: 8 public tests + 11 engine tests pass; venue clippy and bans pass.
- safe-cex typecheck and lint pass; focused BloFin TS regressions: 10 pass.
  Full TS suite still running; required final root checks remain in phase 5.
- Review recheck added malformed whitespace rejection, exact integer sequences,
  and authoritative replacement for thin/empty books5 sides.

### Phase 2 implementation plan

- `sign.rs`: request path+query/method/ms/unique nonce/body, HMAC hex then base64;
  private WS signs `/users/self/verifyGET...` and subscribes after login success.
- `account.rs` + borrowed `wire.rs`: independent account and order snapshot jobs;
  normal/TPSL/trigger cursor families complete only at empty pages, with
  non-advancing cursor and page caps rejecting the entire snapshot. Signed read
  retries only, bounded backoff; failed reads preserve state.
- Snapshot revisions protect orders, positions, balances and settings against
  newer socket/command updates. Settings survive closed positions. Streams map
  native contracts to base once; bounded cumulative fill tracker (500 rows)
  differences quantity and traded value to recover execution price.
- Verify signatures, login, fills/replays/regressions, setting preservation,
  stale snapshots, later-page failures, and private socket watchdog/reconnect.


### Workspace update

- Merged landing structure in `e7e0be289f`; crate is now
  `sdk-rs/venues/blofin`, shared core `sdk-rs/engine`, hosts `sdk-rs/hosts/*`.
- Removed unused `crypto::wallet_utils` (repository search found no caller);
  kept signing primitives and the shared nonce helper used by Binance.
- Full safe-cex run reached 6,488 passes; failures were a concurrent edit/run
  race in the now-passing BloFin regression and the pre-existing production-tree
  `rust.exchange.wasm.test.ts` violating the runtime-boundary test. Rerun final
  checks on stable sources, and relocate that integration test in phase 5.

### Phase 2 review and verification

- Typed private snapshots, hex/base64 signing, authenticated socket and liveness,
  cursor pagination and snapshot revisions implemented. Cumulative fills emit
  execution quantity/price once across duplicate, regressive and terminal frames.
- Review fixes: remainder-sized inline protection, valid positive leverage
  fallback, login startup dependency, separate collateral rounding, and rejection
  of malformed global modes. Global margin is projected to flat-market settings.
- Intentional TS corrections: malformed modes cannot initialize trading; empty
  position pushes clear used/upnl. Listed in gate comments/normalization.
- Final private checks: 1 signing + 9 account + 8 public tests, clippy and bans
  pass. Review recheck reports no remaining phase blockers.

### Phase 3 implementation plan

- A preflighted `Job` holds sequenced typed write steps; successful responses
  apply effects, errors stop the job, uncertain writes are never retried.
- Native contract sizing/rounding, caller IDs honored once or rejected across
  the entire batch before writes; normal batches at most 20, TP/SL single objects,
  conditional single objects. Generated IDs carry broker attribution.
- Cancel steps preserve exact native identity and correlate every row, including
  already-gone responses; pending TP/SL sibling protection is reconstructed
  before cancellation and re-placed afterwards. Pace TPSL/trigger writes.
- Amend is bounded cancel-replace with a fully preflighted replacement; keep
  raw conditional margin/position side, trigger reference, execution price and
  attached protection. Replacement client IDs are always new.
- Account setting commands commit only after confirmed success. Isolated hedge
  leverage updates both legs; changes update positions and persistent settings.
- Verify all-or-nothing IDs, placement sizing/brackets, response-loss no-retry,
  conditional provenance, cancel correlation and sibling preservation, mode and
  leverage rejections, optimistic states and cleanup.

### Phase 3 review and verification

- Sequenced jobs preflight every write, replan queued commands against confirmed
  state, and never retry uncertain writes. Normal batches, standalone/full-position
  TP/SL, opening triggers, cancel/amend and settings are implemented.
- Review caught and fixed nullable response messages, tiny partial sizes becoming
  full-position stops, edits dropping native entries or bracket provenance,
  sibling execution/reference changes and live-position margin overwrites.
- Both isolated-hedged leverage writes are issued together and joined before
  settings commit. Lot-unit splitting preserves quantity without rounding loss.
- Final review recheck: no remaining blockers. 34 venue tests pass (16 trading,
  9 account, 8 public, 1 signing); clippy and bans pass.
- Additional intentional TS corrections: validate every cancel row and its exact
  submitted identity even under code 0; reject unsupported TIF/tiny partial
  protection instead of silently changing semantics; conserve split lot units.

### Phase 4 implementation plan

- Borrowed history payloads stream into typed result rows; commands return the
  same JSON keys as Binance's history module. Preserve per-fill trade IDs,
  native-to-base execution quantities, fee sign conventions and time units.
- Order history reports `amount`/`filled` in base units (contracts times
  `contractValue`), as open orders and fills do. This deliberately diverges from
  TS, whose history shows 1 BTC as 1000 on BTC-USDT. Rows on markets missing from
  the catalogue are dropped, as fills are, rather than reported in contracts.
  Income uses cursor paging
  and client-side time windows because BloFin ignores begin/end on fills history;
  page failures fail the read rather than silently returning partial history.
- Public trades and candle reads validate symbols/options; rare reads never
  mutate the store. Test fill identity, tiny quantities, fee signs, filtering,
  paging failures and exact JSON shapes; run a phase-end sub-agent review.

### Phase 4 review and verification

- History/fills/income/public-trade queries return typed JSON without touching
  account state. Execution quantities retain tiny fills; history orders retain
  TS's native units and milliseconds. Fill events use seconds.
- Review fixes: arbitrary-length numeric cursors, invalid timestamp handling,
  execution aliases with zero/side precedence, symbol/non-swap filtering, and
  walking past newer pages when BloFin ignores requested fill time bounds.
- Intentional TS corrections: never fabricate executions from order-total
  history, walk older fill windows instead of false empty history, and fail a
  truncated/failed income walk rather than returning partial ledger rows.
- 43 venue tests pass, including 9 query regressions. History clippy passed after
  the style fix; final all-target checks will repeat on stable integrated code.

### Phase 5 implementation plan

- Register wasm and SDK venue; pass passphrase through engine options. Add
  documented private tape, write-body request comparison and fill-price parity
  as generic optional harness capabilities; preserve Binance defaults.
- Build wasm, run private correctness gate, record 20s BTC/ETH public tape and
  trim fixture, run public performance/correctness gate and Binance regressions.
- Fix workspace-path integration issues and move the existing Rust wasm test
  out of production runtime code to satisfy the package boundary invariant;
  the landing branch's `src/__tests__` location wins during integration.
- Run root typecheck/lint/tests plus required Rust checks. Record exact results
  and phase review; merge landing branch, rerun affected checks, then PR reviews
  and merge only after every required gate is green.

### Phase 5 review checkpoint

- Registered BloFin in wasm and SDK, including passphrase forwarding. Added
  replay body matching, exact caller-ID preservation (including broker-prefix
  IDs), one-shot recorded mutations, missing-write detection, logical private
  subscriptions, and per-execution fill-price comparisons.
- Review found terminal-row resurrection in amend/protection merging after
  retaining terminal statuses. All planners now reject/skip terminal targets and
  siblings; late partial frames cannot reopen terminal rows. A regression covers
  terminal amendment, repeated cancellation, and new protection after cancellation.
- Paired TS fix: cancel-all filters open rows instead of resending retained
  terminal orders. Other defensive TS divergences are listed in gate.mjs.
- Fixed live recorder handling of streamed transport bodies. Recorded 20 seconds
  of BTC/ETH public traffic (2,922 records); public performance gate is running.
- 44 BloFin tests and clippy pass after review fixes. Root typecheck (44 tasks)
  and lint (28 tasks) pass; root tests and stable wasm/gates remain in progress.

### Phase 5 integrated verification

- Merged landing through `08662b3282`; resolved the duplicate wasm-test rename
  in favor of landing's `src/__tests__/rust.exchange.wasm.test.ts`.
- Release wasm rebuild, wasm-target clippy, 44 venue tests, 11 engine tests,
  venue fmt/clippy and bans pass. SDK wasm integration tests: 4 pass.
- Isolated gate output directories prevent concurrent runs overwriting results.
- Stable BloFin public gate: 2,764 frames, 512 markets/tickers, 2 books, exact
  parity. TS/Rust main CPU 126.6/125.2ms, steady CPU 82.4/74.5ms, retained
  memory 1.6/1.5MB (Rust linear memory 0.9MB). PASS over three runs.
- Stable private gate: 26 frames, exact account/command/fill/write/subscription
  parity, zero unmatched or unused mutations. PASS. Private tapes gate correctness.
- Root test run: 6,486 passed, four unrelated HyroTrader import timeouts at the
  default five-second limit. Isolated HyroTrader recheck with 30-second timeout:
  all 10 passed. Full root retry is running; no unrelated production edits.

- Isolated Binance public gate: 871 frames, 917 markets, 783 tickers, 3 books;
  TS/Rust CPU 212.4/92.4ms, steady 118.7/36.7ms, retained 2.3/2.0MB. PASS.
  Binance private gate: 14 frames, exact parity, PASS.
- Every Rust/wasm/replay gate required by VENUE_PORT_BRIEF passes on the stable
  build. Phase-end adversarial reviews rechecked fixes and report no blockers.
  Proceeding to PR review while the additional full JS timeout-adjusted run finishes.

### PR adversarial review rounds 1–2

- PR #1300 targets `feat/rust-rewrite`. Three local reviewers' findings are
  posted as PR comments. Fixes include regressive fill/store highwater guards,
  terminal tombstones across WS/REST reconciliation and confirmed cancellations,
  price-only virtual protection preserving entry remainder, and homogeneous
  placement batches. Cancel-replace now verifies authoritative final native
  canceled-order details and aborts on unknown terminal state or new executions.
- SDK fills normalize seconds/milliseconds; flat-symbol leverage uses a signed
  typed query; account-global margin changes allow no symbol. SDK caller-ID
  helper declares BloFin's real rule. Replay covers fill/income windows and
  leverage query results, and preserves omitted amendment fields.
- Independent hedge leverage now travels durably in optional appended SETTINGS
  columns with legacy scalar fallback; tests cover later worker/main frames.
- Paired TS correction: preserve filled/remaining highwater and opt into store
  terminal tombstones for final BloFin WS events. Existing 37 TS private/order
  regressions pass. The expanded private tape (29 frames) passes exact parity.
- Full JS suite: all 6,490 tests passed with a 30-second import timeout allowance;
  default root failures were unrelated HyroTrader import timeouts. Both root
  typecheck/lint passed after landing merge. Integrated native suite passes:
  67 BloFin +12 engine +29 Binance tests, plus4 JS host/wire tests.
- CI old head passed native fmt/clippy/tests, wasm and JS checks; replay stopped
  at wasm size1,308,570 bytes above the existing1.2MiB budget. Keeping the budget
  and stripping release-only wasm name/producers metadata, then rerunning
  correctness/performance gates. Speed optimization remains unchanged.
  Initial push secret scan included landing history; PR-scoped scan passed.

- Final cross-review recheck reports no remaining material code blockers. Native
  tests: engine12, BloFin67, Binance29; all-target clippy, wasm clippy, fmt and
  bans pass. Optional side columns retain old scalar offsets and decode old
  frames, including worker-to-main transfer. SDK typecheck/lint pass.
- Secret-scan diagnosis: the initial branch-push range included a pre-existing
  Binance signing fixture in landing commit `879fdc7461`; the PR's own diff scan
  passed. No credentials were introduced by this port.

### Landing integration after review

- Fetched/merged `20dd66dddb`, including the completed Bybit port and explicit
  green-CI landing rule. Resolved shared engine/wasm/SDK/harness conflicts as
  a union: nested `result.list` streaming and BloFin envelope validation share
  one splitter; both venue rules/registrations/private tapes survive.
- Bybit settings initialize optional side leverage with scalar fallback; the
  new BloFin leverage query is explicitly unsupported in Bybit's query dispatcher.
- Added merged streaming regressions: large nested arrays keep error codes and
  cursors, while malformed/truncated/oversized envelopes fail. Native validation:
  engine15 +BloFin67 +Bybit43 pass (125); Binance recheck is running.
- Pre-merge stable release was1,132,561 bytes after stripping names/producers,
  below the original1.2MiB budget. Landing already raises its shared three-venue
  budget to1.4MiB; this lane does not increase it. Post-merge wasm build, SDK
  tests and all three venues' replay regressions will be recorded next.

- Three-venue release initially exceeded landing's existing1.4MiB budget by
  37,455 bytes. Optimizing only wasm dispatch/serialization glue for size (`z`)
  reduces it to1,461,186 bytes, below1,468,006. Engine and venues remain O3.
  The budget is unchanged. Final SDK/replay checks run on this exact artifact.

### Final local gates on the three-venue artifact

All required brief gates pass after landing integration on `487fd434f5`:

| Public fixture | Frames | TS/Rust CPU ms | TS/Rust steady ms | TS/Rust retained MB |
| --- | ---: | ---: | ---: | ---: |
| BloFin | 2,764 | 140.791 /132.057 | 93.100 /74.126 | 1.649 /1.595 |
| Binance | 871 | 209.026 /87.669 | 114.226 /38.241 | 2.314 /2.037 |
| Bybit | 8,742 | 388.614 /310.030 | 263.612 /188.444 | 3.082 /2.220 |

- All public outputs match exactly; medians use three runs. Private gates PASS:
  BloFin29 frames (52 records), Binance14, Bybit18; no unmatched/unused writes.
- BloFin executions remain exactly0.0001@100,0.0002@130,0.0002@170; duplicated,
  regressive and terminal replays emit no extra execution.
- Native154 tests pass (engine15, BloFin67, Binance29, Bybit43); SDK wasm7 pass;
  JS host/wire4 pass. Venue and wasm clippy, all fmt and bans pass.
- Root typecheck44 tasks and lint28 tasks pass after merge. The full JS suite
  passed6,490 tests with30-second import allowance;37 focused BloFin regressions
  pass after the paired TS change. Existing default timeout failures are unrelated.
- Release wasm1,461,186 bytes below unchanged landing budget1,468,006. Final
  cross-review found no remaining material blockers; both review rounds and fixes
  are posted on PR1300. Waiting for green CI, then merging and cutting follow-up.

- Added BloFin public/private replay steps beside Binance/Bybit in CI, retaining
  the existing shared size budget. Verifying the same BloFin gates on CI's Node22
  locally before the final CI run; no runtime changes in this checkpoint.

- CI-runtime compatibility checks PASS on Node22: BloFin public225.888/162.239ms
  CPU,158.040/95.482ms steady,1.520/1.384MB retained (TS/Rust, three medians).
  Private29-frame correctness passes too. The earlier public table uses Node24.

### OKX landing integration

- Merged landing `693ad4f139` after OKX's PR1302 landed. Preserved all four
  registrations, OKX region/demo aliases and client-ID rules, boxed wasm dispatch,
  typed metadata modes, strict BloFin/Bybit validation, and generic replay staging,
  auth/subscriptions, timeouts and body matching. CI retains all venue gates.
- The merged engine passes222 native tests: engine19/BloFin67/Binance29/
  Bybit43/OKX64. SDK wasm17 tests, root typecheck44 tasks and lint28 tasks pass.
- Four-venue release initially exceeded budget. Size optimization on venue crates
  and serde_json, retaining O3 for shared engine/crypto/dependencies, reduces the
  release to1,446,776 bytes against unchanged1,468,006. Public CPU/memory gates
  verify the chosen release profile; no budget or correctness gate is weakened.
- Integration replay caught a lost BloFin-specific instruments cache-bust filter;
  restored only that known empty random query on its configured path. Retained
  all meaningful query values and caller IDs. Final four-venue gates are running.

### Final four-venue gates

All brief gates PASS on the four-venue artifact from `f698c02126`, using Node22
and three-run public medians:

| Public fixture | Frames | TS/Rust CPU ms | TS/Rust steady ms | TS/Rust retained MB |
| --- | ---: | ---: | ---: | ---: |
| BloFin | 2,764 | 184.932 /179.409 | 134.210 /103.233 | 1.526896 /1.399720 |
| Binance | 871 | 180.825 /103.744 | 101.247 /47.654 | 2.188504 /1.859880 |
| Bybit | 8,742 | 455.663 /415.780 | 300.881 /248.049 | 3.201512 /2.291096 |
| OKX | 8,902 | 569.781 /524.418 | 439.808 /386.700 | 2.453216 /2.410456 |

- Exact public Store/book outputs match. Private correctness PASS: BloFin29,
  Binance14, Bybit18, and29 each for OKX/OKX EU/demo/EU-demo. Every recorded
  write is matched once. BloFin deltas remain0.0001@100,0.0002@130,0.0002@170;
  duplicates, regressions and terminal replays produce no additional executions.
- Release wasm1,446,776 bytes, below unchanged1,468,006-byte budget. Shared
  engine remains O3; host, venue crates and serde_json use measured size profiles.
- Native222 tests, SDK wasm17 and JS host/wire4 pass; venue/wasm clippy, fmt,
  bans, root typecheck44 tasks and lint28 tasks pass. Earlier full JS6490 and
  focused BloFin37 pass; PR CI verifies the integrated repository again.
- Final three independent integration reviews found no remaining concrete
  blockers. Shared splitter review verified legacy/nested/envelope modes;
  harness review verified exact caller IDs, cache-bust filtering, staging and
  single-use writes; trading/SDK review verified all four dispatches and durable
  asymmetric leverage settings. Findings are posted on PR1300.
- PR1300 is mergeable into `feat/rust-rewrite`. Awaiting green CI before merge
  and creation of a fresh follow-up branch; no pending code changes identified.

### CI performance correction and Crypto.com landing

- CI passed native tests/fmt/clippy, wasm, root repository checks and secrets,
  but BloFin total CPU failed130.8/154.3ms (TS/Rust), despite steady91.0/72.7ms,
  exact correctness and lower retained memory. This supersedes the local-only
  all-gates claim above; landing still requires green final CI.
- Review identified quadratic subscription sizing:1052 topics caused48,808 topic
  serializations and2,242,126 candidate bytes for48,050 emitted bytes. Count each
  topic once before serializing final bounded slices, preserving frame ordering
  and limits. Existing400-topic boundary regression and independent review cover
  the builder. Module loading precedes the CPU measurement.
- Merged Crypto.com landing `c63c3e5670`, retaining all five registrations,
  stricter body/signature/subscription audit, one-shot write queues, staged GETs,
  backing-buffer memory accounting and BloFin exact IDs/cache-bust filtering.
  Crypto.com explicitly rejects the newly added unsupported leverage query.
- Landing's existing shared budget is now1.8MiB (1,887,436bytes). Restore BloFin
  and serde_json O3 to address startup CPU; other venue/host size overrides stay.
  Final five-venue correctness/performance/size checks are running. Shared-engine
  merge review passes21 tests and clippy; host/SDK integration review has no
  concrete blockers. No master, rebase, force push, stash or gate relaxation.

- Corrected a shared baseline race: three equal immediate-only samples could
  precede delayed56-byte loader-buffer cleanup. Require100ms exact stability
  across10ms timer turns/GC, bounded1s. Signed deltas and negative checks stay.
  Independent review and11 harness tests pass. BloFin public189.063/150.870ms,
  steady120.928/91.103ms, retained2.828152/1.661880MB passes; private29 passes.
- Five-venue build1,817,179bytes fits. Binance/Bybit/Crypto.com public+private
  pass. OKX exact correctness/memory pass but size profile fails CPU1151.3/
  1235.1ms and steady891.0/911.5ms. Restore OKX O3; evaluate Crypto.com size
  profile under the same unchanged aggregate budget. All final gates rerun on
  that artifact before landing. Native266/SDK17/host4/TC44/lint28/clippy pass.

### Five-venue speed profile verification

- Restore OKX O3, retain Crypto.com/Binance/Bybit/host size profiles. Final
  five-venue artifact1,841,595bytes fits the inherited1,887,436-byte budget.
  All five public gates pass on explicit Node22; this supersedes the prior
  mixed-runtime baseline-fix sample. SDK17 and first four private gates pass.

| Public fixture | Frames | TS/Rust CPU ms | TS/Rust steady ms | TS/Rust retained MB |
| --- | ---: | ---: | ---: | ---: |
| BloFin | 2,764 | 202.493 /161.006 | 128.535 /101.276 | 2.729528 /1.504672 |
| Binance | 871 | 238.248 /123.732 | 139.849 /59.735 | 4.129976 /2.243296 |
| Bybit | 8,742 | 495.644 /404.329 | 302.369 /255.417 | 6.042144 /2.532560 |
| Crypto.com | 2,177 | 166.659 /129.852 | 107.622 /65.134 | 1.684616 /1.185992 |
| OKX | 8,902 | 631.662 /553.241 | 484.224 /412.866 | 3.959448 /2.504864 |

- WEEX landed concurrently at `6cd219ce4c`, including shared transport/parser
  changes and its existing2.25MiB budget. Integrate and review before final CI;
  the remaining old-artifact aliases are finishing, then affected gates rerun.

### WEEX landing integration

- All13 five-venue replay gates completed successfully, including all four OKX
  private variants. Native266/SDK17/host4/harness11 and root checks pass on the
  preceding artifact. The public table above uses explicit Node22.
- Merge landing `6cd219ce4c`: retain six boxed venue registrations, WEEX upgrade
  headers/capability checks, ticker filtering and strict socket/request audits.
  BloFin ignores SetTickerSymbols (it subscribes its full set), preventing a
  spurious unsupported-command0 notice. WEEX rejects unsupported leverage reads
  even without credentials; settings literals retain legacy scalar fallback.
- Adopt landing's deliberate always-metadata streamed-response contract and
 1MiB cap, keeping every prior BloFin/Bybit large/nested/strict regression and
  explicit UTF8 failure handling. Engine26 tests, clippy and independent merge
  review pass. Preserve the corrected exact stable memory baseline.
- Landing budget is now2.25MiB (2,359,296bytes); this lane does not raise it.
  Six-venue build, root checks and final exact-artifact replays are running.

### Final six-venue local gate proof

Every brief gate and all shared-venue regressions PASS on the exact release
artifact from `3aa26dc83a`, using Node22.23.3 and three public medians:

| Public fixture | Frames | TS/Rust CPU ms | TS/Rust steady ms | TS/Rust retained MB |
| --- | ---: | ---: | ---: | ---: |
| BloFin | 2,764 | 113.410 /73.762 | 77.183 /42.699 | 2.716136 /1.496640 |
| Binance | 871 | 166.376 /79.315 | 89.733 /27.468 | 4.097984 /2.235792 |
| Bybit | 8,742 | 186.155 /142.025 | 116.815 /71.096 | 6.044384 /2.505752 |
| Crypto.com | 2,177 | 74.899 /70.281 | 42.587 /28.184 | 1.680472 /1.184744 |
| OKX | 8,902 | 561.891 /497.629 | 421.979 /366.506 | 3.947024 /2.427024 |
| WEEX | 2,028 | 116.294 /75.956 | 56.596 /32.909 | 4.934416 /2.421168 |

- All15 replays PASS: six public fixtures and nine private variants. Public
  Store/book output matches exactly; private request/auth/subscription/account/
  query/execution checks pass with no unmatched or unused writes. Private frame
  counts: BloFin29, Binance14, Bybit18, Crypto.com21, WEEX20, and29 each for
  OKX/OKX EU/demo/EU-demo. BloFin deltas retain prices100/130/170 exactly;
  duplicates, regressions and terminal replays emit no additional executions.
- Release wasm2,297,675bytes against inherited2,359,296-byte budget. Engine,
  BloFin, OKX and serde_json retain O3; host/Binance/Bybit/Crypto.com size profiles
  are measured by their passing public CPU/memory gates. No threshold changes.
- Native348 tests (engine26/BloFin67/Binance29/Bybit43/Crypto.com42/OKX65/WEEX76),
  SDK wasm18, JS host9, harness16 pass. Native/wasm clippy, fmt and bans pass;
  root typecheck44 and lint28 tasks pass. Earlier full JS6490 and focused
  BloFin37 passed; latest PR CI checks the complete integrated repository.
- Final three independent reviews report no remaining concrete blockers in
  streaming parser, semantic replay/memory accounting, or SDK/header/trading
  boundaries. Detailed findings/fixes are posted to PR1300. Await green final
  CI, then merge into feat/rust-rewrite and create a fresh follow-up branch.

### BingX landing integration

- Six-venue PR CI Rust checks and all15 replay gates passed. CI BloFin public:
  total CPU82.1/59.2ms, steady47.8/25.9ms, retained2.7/1.5MB (TS/Rust);
  CI artifact2,297,685bytes. The push repository run passed while its duplicate
  PR run was finishing. BingX landed before merge and introduced conflicts.
- Merging landing `022ab79de5`. Preserve all seven venue registrations and
  BingX strict request matching, streamed aliases, ticker indexing/reservation,
  intern collision handling and lazy host decoding. Independent engine review
  passes38 tests/clippy; host/SDK review passes12 JS host tests with all
  BloFin CID/passphrase/range/leverage/global-margin hooks and side columns.
- BingX rejects unsupported leverage reads even without credentials; settings
  literals retain scalar fallback. Harness review preserves incoming request
  canonicalization/staging/audits plus BloFin exact caller IDs, cache-bust
  exception, write consumption and settled memory accounting.
- Inherited aggregate budget is now2.5MiB (2,621,440bytes). Seven-venue
  build/native/root/harness validation and exact-artifact replay gates follow.

- Seven-venue native452 and harness33 tests pass; host12, root TC44/lint28,
  venue/wasm clippy, fmt and bans pass. Combined initial artifact2,720,924bytes
  exceeds inherited2,621,440. Evaluate size profiles on WEEX/BingX with shared
  engine/BloFin/OKX/serde_json O3 unchanged, then require all public CPU/memory
  gates and private parity on that exact artifact. No budget increase.

### Seven-venue gate checkpoint

All required venue-port brief gates PASS after `10789e0d54`: bans, fmt, native
venue tests/clippy, wasm clippy/build, BloFin public/private and Binance public/
private regressions. All seven public fixtures also PASS on the final artifact
using Node22.23.3 and three-run medians:

| Public fixture | Frames | TS/Rust CPU ms | TS/Rust steady ms | TS/Rust retained MB |
| --- | ---: | ---: | ---: | ---: |
| BloFin | 2,764 | 209.004 /197.145 | 134.620 /123.186 | 3.231000 /1.942176 |
| Binance | 871 | 249.966 /125.936 | 145.719 /56.455 | 4.152240 /2.250544 |
| Bybit | 8,742 | 502.958 /384.792 | 303.414 /204.795 | 6.238064 /2.465232 |
| Crypto.com | 2,177 | 182.109 /145.028 | 125.391 /80.878 | 1.885088 /1.441040 |
| OKX | 8,902 | 1163.972 /1009.309 | 889.206 /739.757 | 5.117976 /3.489096 |
| WEEX | 2,028 | 233.324 /166.047 | 137.541 /83.753 | 5.052968 /2.189176 |
| BingX | 188 | 151.829 /112.052 | 71.761 /55.437 | 3.532088 /2.063640 |

- Final release2,580,836bytes below inherited2,621,440. WEEX/BingX compact
  profiles pass CPU/memory; restore Crypto.com O3 after compact profile failed
  total CPU98.6/98.8ms (TS/Rust). Engine/BloFin/OKX/Crypto.com/serde_json stay O3;
  host/Binance/Bybit/WEEX/BingX use measured size profiles. No budget increase.
- Native engine/venue452, SDK wasm20, JS host12 and harness33 tests pass.
  Root typecheck44/lint28, native/wasm clippy, fmt and bans pass. BloFin29 and
  Binance14 private correctness pass; additional shared private variants finish
  alongside final PR CI. All checks must pass before merge.
- Three independent final cross-reviews have no concrete blockers: exact caller
  IDs/default refs/staging/unused writes; per-side leverage/wire fallback and SDK
  hooks; retained order/fill identities across store/index/interner changes.
  Plan/proof committed; final CI and remaining private results go on PR1300.

- All17 seven-venue replays completed PASS on854c600708: seven public fixtures
  and ten private variants, zero mismatched or unused writes. Private frames:
  BloFin29/Binance14/BingX38/Bybit18/Crypto.com21/WEEX20/OKX variants29 each.
  MEXC landed at63d521a860 during validation, making PR1300 conflict again.
  Integrate its shared quantity-unit/parser/transport changes before final CI.

### MEXC landing integration

- Merging63d521a860 after all17 seven-venue local gates passed. Preserve
  eight registrations, MEXC-only derived native order extras, stop provenance,
  strict client-ID decoding and optional independent SETTINGS columns. BloFin
  base quantities remain unchanged. Host review passes15 JS +3 frame-unit tests;
  engine43 tests/clippy and harness45 tests/microchecks pass.
- Adopt canonical landing Command::FetchLeverage for both MEXC and BloFin;
  remove this lane's unmerged duplicate Query variant and six unrelated venue
  rejection arms. Migrate BloFin's eight leverage regressions to the canonical
  command; SDK JSON operation stays identical. This reduces future merge scope.
- Adopt shared ordinal tape/virtual public replay clock and preserve all prior
  exact caller IDs, semantic payloads, signatures, subscription/write checks,
  full execution prices and isolated outputs. Memory settling uses real timer
  turns even when replay timers are virtual; signed accounting is unchanged.
- Landing builds wasm with opt-levels while native release remains3. Preserve
  that wasm-only setting and remeasure the aggregate artifact, all public CPU/
  memory gates and private parity under inherited2,621,440-byte budget. Native,
  SDK, root typecheck/lint and final exact-artifact replay verification run next.

### Eight-venue required gate proof

Every required venue-port brief gate PASS on artifact from6d5104926e:
BloFin/Binance public and private, native tests/clippy/fmt, wasm clippy/build,
and bans. Public medians use Node22.23.3 and three runs (shared tape now counts
recorded socket close events too):

| Public fixture | Frames | TS/Rust CPU ms | TS/Rust steady ms | TS/Rust retained MB |
| --- | ---: | ---: | ---: | ---: |
| blofin | 2,765 | 214.553 /173.292 | 129.342 /99.369 | 3.285168 /1.889536 |
| binance | 873 | 214.725 /107.070 | 115.620 /45.532 | 4.171552 /2.185384 |

- Release2,579,044bytes below inherited2,621,440. Native engine/venue508,
  SDK wasm20, JS host15, frame-unit3, harness45 tests pass; root typecheck44/
  lint28, native/wasm clippy, fmt/bans pass. Eight canonical leverage command
  tests pass, with caller IDs, independent side settings and base units intact.
- BloFin29 and Binance14 private frames match exactly. Remaining shared public/
  private variants run alongside final CI. Three independent cross-reviews find
  no concrete blockers in virtual-tape/real-baseline handling, model/SDK units
  and rails, or account/trading identity/protection/fill guards. All final PR
  checks must be green before merge; findings and final results go on PR1300.
