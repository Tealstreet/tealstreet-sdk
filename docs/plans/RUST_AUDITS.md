# Rust rewrite audits

Read-only Opus audits of `feat/rust-rewrite` at `df2d6a4a03` (after #1340).
Findings land as fixes through lanes; strike an item when its PR merges. Line
numbers are as of that commit.

**Judging rule.** Rust need not match TS 1:1. Lifecycle (startup order, dispose,
cleanup, retries, internal state) may differ freely. A divergence matters only
by its production impact: wrong orders, positions, balances or fills; money or
attribution lost; data silently missing or stale; a failure the user cannot see;
or a downstream caller that breaks. Fix to that bar, not to TS's shape.

## Owner decisions (gate the cutover)

1. **Engine delivery to tealstreet-next.** The Copybara glob has no `sdk-rs/**`,
   tealstreet-next already owns a different `sdk-rs/`, and `@tealstreet/safe-cex-wasm`
   builds a gitignored `pkg/`. Mirror under a distinct path plus a wasm build in
   tealstreet-next CI, or publish a prebuilt package.
2. **Keyless hosted signing.** exchange-server asks the browser to sign through
   TS axios interceptors (`base.ts:2074`); Rust refuses remote signers
   (`rust.exchange.ts:397,423`) and the native frame server assumes server-held
   credentials. Engine sign-request actions with browser signers, or server keys.
3. **React Native.** `registry.native.ts` maps 9 names to the wasm engine;
   Hermes has no `WebAssembly`. Revert those to TS for mobile, or a native/JSI host.
4. **Rollback.** `engine` accepts only `'rust'`; unsetting it restores registry
   defaults, which are already Rust for some venues. [Recommendation and costs](RUST_ROLLBACK.md):
   one explicit `'typescript' | 'rust'` construction resolver, coherent per-venue
   application/hosted policy, tested TS reachability and shadow/canary/soak before
   deletion. This is a decision aid; the switch and caller policy are not implemented.

Settled: Backpack spot retired; Kraken market+TP/SL rejects with a clear error
(futures and spot; spot was placing the entry unprotected until verification);
Bybit Partial TP/SL keeps TS semantics (sensitive, no change). Standing licence:
obvious improvements (clearer errors, better messages) need no decision.

Ordinary registry defaults already select Rust for `coinbase`, `coinbasespot`,
the six Orderly brokers and `txflow`. Coinbase/Orderly retain production TS
hosted-signing delegates; they are not a generic ordinary-account rollback path.
TxFlow has no production TS fallback. The branch cannot reach master until 1–3
are settled for them; rollback availability remains explicit in decision 4.

## Shared financial snapshot admission (Binance and Bitunix)

The engine now owns an issuer-bound, checked order-snapshot write fence and one
admission boundary before BOTH native cache application and Store publication.
Binance and Bitunix replace their local order-write revision copies with linear
Store tokens; all existing invalidation boundaries and native lagging-server,
terminal, incarnation and client-ownership protections remain. Bitunix keeps
row-selective WS healthy-peer progress and validates both initial endpoint
tokens before regular then TP-SL native application. Snapshot publication does
not invalidate sibling reads or metadata domains. The existing StaleGuard
progress escape never overrides this write fence.

This is standardization of native behavior, not a new original-RED fill fix.
Permanent exhaustion refuses reads before HTTP and settles owned preflight;
accepted native writes remain possible. Native compatibility and new exhaustion
controls are recorded in [the contract](RUST_FINANCIAL_SNAPSHOT_FRESHNESS.md).
Only these two financial references are migrated. Metadata reuses the primitive
in its own follow-up; RequestContext's initial late-response codec remains
read-only. Safe late ACK patches to both Store and native terminal/filled tables
must explicitly invalidate this fence before future snapshot application; they
are not claimed implemented here. Broader Store mutation revisions protect a
different boundary and must not replace selective snapshot-write admission.

## Error handling (scoping)

Owner flags error handling as one of safe-cex's weakest points. Scope a
standard while the port is open: one error taxonomy across engine, bridge and
TS callers (rejected by venue / invalid input / auth / rate limit / network /
unsupported), venue code and message preserved, every failure visible to the
user with its cause. Design pass first (Opus), then lanes apply it per layer.
The client contract outside safe-cex (error shape, logger, exchange interface)
may change when there is a good argument; update tealstreet-next callers with it.
Audit results and the standards: `docs/plans/RUST_QUALITY.md`.

## Cutover

Re-verified on landing `bba1f43126cd`; exact source, regression and caller
boundaries: [Cutover tail audit](RUST_CUTOVER_TAIL_AUDIT.md). Source proof is
not a deployed-runtime or npm-publication claim.

- ~~Main-thread writes: web places orders on the main-thread exchange
  (`apps/web/src/order/sendOrder.ts:24`); a main-thread `RustEngineExchange` has
  no engine (`rust.exchange.ts:701`). Route commands to the worker engine.~~
  Fixed: a bridged main-thread instance forwards its trading methods to the worker engine (B4).
- ~~No `socketHealth`, `reconnect` or `recoverAfterOffline` on Rust;
  CLI stale-stream gate fails open.~~ Fixed by #1450 (`5ff7ebde6c1`): native
  channel handles feed store health and recovery. Fresh 7 SDK socket and 3 CLI
  regressions pass, including refusal after initialized private-stream silence.
  Missing/uninitialized evidence still intentionally fails open; risk-reducing
  operations remain available.
- ~~Missing `fetchHistoricalPositions`, `fetchClosedPnl`, Bitrue
  `updateSymbolMetadata` and Hyperliquid `enableSpotMarkets`.~~ Implemented
  through native capability policies and #1445/#1447/#1453/#1501; preserve
  supported-product limits and explicit unsupported-axis refusals (B8).
  ~~Backpack REST `fetchOrderBook` parity~~ now uses an unsigned request-owned
  snapshot query, native price IDs/order/zero totals and a standalone JSON decoder;
  malformed snapshots/refusals reject without mutating subscription books.
  ~~Crypto.com REST `fetchOrderBook` parity~~ now uses unsigned live/UAT GET
  with fixed depth50 and a request-owned typed first-book snapshot; raw IDs,
  order and zero totals survive without touching streaming books.
  ~~GRVT REST `fetchOrderBook` parity~~ now uses a request-owned unsigned MDG
  lite query with native instrument/depth fields, full/lite typed mapping and
  cumulative base totals; public errors never refresh or replace account auth.
  ~~Paradex REST `fetchOrderBook` parity~~ now uses an unsigned request-owned
  depth query, native side order and per-level quote totals; live SBE books,
  sequence state and existing private JWT remain independent.
  ~~WEEX REST `fetchOrderBook` parity~~ now uses an independent unsigned query,
  native cumulative base amounts and exact version strings; shared rate pause
  admission survives while live book generations and account metadata stay separate.
  ~~Hyperliquid `scheduleCancel`~~ now has native signed arm/clear, safe UTC-ms
  admission/dispatch checks, synchronized clock, vault/network signing vectors,
  append-only capability and worker/host uncertain-write ownership.
  [Regression evidence and explicit lifecycle boundaries](RUST_HYPERLIQUID_SCHEDULE_CANCEL.md).
  ~~Typed GRVT builder rejection~~ now preserves definitive code/reason/caps
  through the engine, host and worker/hosted command replies; failed checks stay
  nonmodal. [Regressions and required NX migration](RUST_GRVT_BUILDER_ERROR.md).
  ~~WEEX metadata refresh~~ now uses the shared `onReadAndMetadata` policy;
  [regressions](RUST_WEEX_METADATA_REFRESH.md) cover cached flat symbols,
  repeated reads, malformed-response retention and public-only isolation.
  REST snapshot queries and the signed dead-man control remain independent of streaming books.
- ~~`instanceof BlofinExchange` as an active downstream cutover blocker.~~
  Pinned NX read-only audit finds the class check in an unused helper, with no
  production import/render; active generic helpers have no guard. It remains
  source/deletion cleanup, not a proved active user-feature failure.
- ~~Deletion inventory misses downstream deep imports.~~ The inventory now
  records the pinned NX callers and SDK/reference dependencies. **Extraction
  and caller migration remain open:** Hyperliquid constants, Bitget family
  helpers, Aster types/validators and base/registry contracts have downstream
  consumers. Include their exact callers in
  [the inventory](RUST_CUTOVER_CALLERS.md); extraction/caller migration is not
  completed by listing them. No production NX direct Bybit inverse/spot class
  import outside SDK runtime was found, but internal registries and fixture
  scripts still use those classes.
- ~~exchange-mcp release ships without engine WASM.~~ Current package/tsup,
  artifact-relative loader, exact release pack check and isolated smoke include
  the engine CJS/WASM (#1342). Green #1666 CI exercises the packaged consumer.
  A published npm version or deployed NX binary is separate evidence.
- ~~Combined WASM has no budget; combined versus family layout undecided.~~
  #1393 selects families for browser/Workers and combined for Node/MCP. CI
  enforces 1,343,488 B family / 14,155,776 B combined ceilings; #1644/#1656/
  #1661 recover size without raising them. ~~Actual browser cold-start
  measurement is missing.~~ [The Chrome worker measurement](RUST_BROWSER_COLD_START.md)
  loads the production family loader for all 28 families plus a combined-web
  diagnostic, with three fresh process/profile samples each. Separate CI runs
  on landing pushes and manual dispatch only; missing assets/errors/incomplete
  samples fail, with no timing threshold pending several landing baselines.
  Loopback module readiness is not device/network or account-startup evidence;
  existing fresh-process metrics still describe Node load separately.
- Replay gates still execute TS venues and frozen references. ~~Drop
  `__tests__/reference`.~~ **Owner decision:** keep executable TS references as
  the CPU/memory oracle through cutover; no venue or reference deletion until
  after cutover. Golden output/retirement work is deferred, not implemented;
  before any later deletion, preserve an independent correctness oracle and
  performance baseline in a reviewed change. Do not turn TS-vs-Rust into
  Rust-vs-itself.

## Trading correctness

High:
- ~~Cumulative-fill correctness on landing (Hyro and Bitget Classic): an unpriced
  Hyro report consumes its cursor and loses the corrected fill; a missing quote
  with bootstrap average0 invents a known zero value and overprices a later delta;
  Bitget emits negative-price fills, loses cumulative-only corrected delivery
  after display advances, and rewinds progress on late direct executions so a
  later cumulative catch-up emits a duplicate.~~ Shared `engine::execution`
  validates typed quantity/price evidence and derived arithmetic before delivery
  progress or native dedup commit. Five separate native original-RED/core-GREEN
  controls cover these real financial defects; a negative-quote compatibility
  control preserves Hyro recovery. Bootstrap observations remain distinct from
  delivered progress, zero average means unavailable, explicit quote0 stays
  distinct, and late direct executions deliver without rewinding their anchor.
  [Scope, evidence and remaining fan-out](RUST_CUMULATIVE_FILLS.md). This first
  core now also supports observed suffixes and bounded priced intervals with
  WOO/Bitrue consumers. Checked average/direct-suffix quote binding and public
  execution rounding now also serve Bitunix and Coinbase(+spot); compatibility
  controls preserve historical quotes, equal-endpoint corrections and native
  Spot/INTX/CFM quantity units. BloFin also uses checked prefix admission,
  capped-decimal arithmetic and scaled public-output preparation before live
  progress commits. Kraken Spot now prepares direct/cumulative trade delivery
  through the same helper before row state commits, preserving native IDs,
  explicit last quantities and independent lifecycle observations. BingX,
  AsterDEX, Orderly and the KuCoin Spot
  fallback still need adoption before
  row reporting and snapshot freshness. ApeX cumulative events still never
  emit public fills; synthetic history remains distinct. No live capture claim.
- ~~WOO advances its delivered cumulative anchor on a zero/negative last
  quantity, losing the same-ID cumulative-only correction. Bitrue replaces a
  trusted REST quote anchor with an overflowing cumulative-average product,
  suppressing the next valid VWAP suffix.~~ The shared observed-suffix and
  priced-interval paths validate before delivered/quote state commits. Two WOO
  native original-RED/core-GREEN controls recover .001@59000 without moving
  the position twice; a Bitrue original-RED/core-GREEN control preserves the
  REST30/quote3300 anchor and delivers the valid10@130 suffix. These are real
  financial defects on the previous landing, within the helper contract;
  A fourth original-RED/core-GREEN control prevents WOO last-only unpriced
  reports and their same-ID correction from doubling displayed quantity (.007
  instead of .006). Deliberately, inferred last-only observations now wait for
  a valid financial receipt; explicit cumulative observations stay independent.
  A fifth original-RED/core-GREEN control recovers a missing Bitrue delivered
  quote from a valid same-quantity correction, then delivers10@120 once.
  Shared enrichment fills missing evidence at the exact endpoint only; it
  cannot replace a known quote, move quantity or replay a Fill. Five controls
  cover four defect classes. Complete exact-head CI/raw-artifact proof remains
  required before merge.
- ~~Binance COIN-M treats `EXPIRED` as terminal, dropping a triggered stop's fill
  (`binance/src/account.rs:1186`).~~ Fixed: late native executions emit independently of lifecycle terminal state.
- ~~Binance USD-M amend re-places the full original size when the order filled
  during the cancel ("unknown order" counted as cancel) (`binance/src/trading.rs:1131,536,1463`).~~
  Fixed: replacement waits for cancellation proof and subtracts racing fills; unknown cancellation cannot replace.
- ~~Rejected private login leaves a silent socket: bitget `account.rs:1128`,
  blofin `account.rs:1125`, mexc `account.rs:308`, woo `account.rs:1482`,
  cryptocom `lib.rs:390`, apexomni `account.rs:1130`. Close and back off; add an auth deadline.~~
  Fixed in `feat/rust-socket-supervisor`: rejected login retires the socket,
  preserves account state and schedules jittered retry; an independent auth
  deadline expires despite pongs. Native regressions cover all six, including
  ApeX's required immediate subscriptions and validated ACK-less wallet data.
- ~~Binance USD-M emits fills from non-TRADE events and does not dedupe on trade id
  (`binance/src/account.rs:1169`).~~ Fixed: TRADE-only execution-ID dedupe, including PM/COIN-M.
  Verification found liquidation (`x=CALCULATED`) fills dropped; they emit again.

Medium:
- ~~Phemex cancel-all fails while any place is in flight (`phemex/src/trading.rs:806`).~~ Pending placements are fenced before resolving native cancellation targets. See [verification](RUST_ORDER_RACES.md).
  Verification found the uncertain-placement fence never cleared on Phemex (cancel-all
  failed for the session). Owner decision: Phemex cancel-all is never refused for an
  uncertain placement; it cancels every known order at once, and after the lost write's
  signed expiry an `activeList` read cancels any late-landed order from it.
- ~~Cancel-all misses just-placed orders: okx, asterdex, breakoutprop, bitget, blofin, bitrue.~~ Retained acknowledgement routes, pending-placement fences and SDK attempted-symbol scopes cover the gap; Aster/Breakout acknowledgement caches already existed and were retained. See [verification](RUST_ORDER_RACES.md).
- ~~Backpack amend replaces full size after partial fill (`backpack/src/trading.rs:411`).~~ Replacement subtracts prior and cancel-racing executions and requires definitive cancellation. See [verification](RUST_ORDER_RACES.md).
- ~~No per-order lock between amend and cancel in most venues.~~ Shared scoped admission covers concurrent/preplanned cancel-replace adapters; existing serial-intent or explicit-lock paths remain. Binance/Bybit findings belong to their lane. See [verification](RUST_ORDER_RACES.md).
- ~~Stale snapshot reopens canceled orders: Binance USD-M/PM (`trading.rs:1463`),
  Bybit Demo/Hyro (`account.rs:1824`).~~ Fixed: write/event revision fences plus native terminal retention cover in-flight and replica-stale reads.
- ~~REST snapshot swallows stream fills: bitrue `account.rs:1291`, bingx `account.rs:726`.~~ Independent execution axes also cover Aster/WOO fallback; startup baselines do not let later REST consume stream fills. See [verification](RUST_ORDER_RACES.md).
- ~~Binance Spot quantified cancel acknowledgement can advance the execution
  cursor before its matching TRADE, losing the execution notification.~~
- ~~Binance PM strategy/regular client-ID reuse on one symbol can let an old
  strategy terminal replay cancel the newer regular row.~~ Both fixed by the
  Binance/Bybit follow-up (#1354; [follow-up](RUST_AUDIT_BINANCE_BYBIT_FOLLOWUP.md)).
- ~~BloFin replays pre-session fills (`blofin/src/account.rs:1231`).~~ Session cutoff and one-time complete REST baseline preserve live continuation without historical replay. See [verification](RUST_ORDER_RACES.md).
- ~~Stale books stay visible during resync (`engine.rs:1147`); woo ignores `prevTs`, kraken spot ignores checksum, asterdex/lighter keep books while waiting.~~ Explicit invalidation and recovery snapshot clears, WOO continuity and Kraken Spot CRC. Binance/Bybit iterator-contract follow-up was sent to their lane. See [verification](RUST_ORDER_RACES.md).
- ~~Bybit inverse amend writes `remaining` in contracts (`bybit/src/trading.rs:1934`).~~ Dropped after reverification: cited local-ACK branch runs only for Demo/Hyro, which cannot enable inverse. Regression preserves coin `remaining` and native `remainingNative`.
- ~~Write timeout discards optimistic state instead of refreshing: orderly, okx.~~ Unknown order outcomes preserve pending state and force fresh reads; includes HTTP408/5xx and OKX50004. Config-specific confirmation is not claimed. See [verification](RUST_ORDER_RACES.md).

Low: ~~no reconnect backoff (asterdex, bitrue, bingx, kucoin, cryptocom, phemex,
coinbase public, grvt, paradex, polymarket)~~ — fixed for the requested feeds
by the shared supervisor, including fresh KuCoin token POSTs. Reverification
dropped the literal public no-backoff claim for AsterDEX, Bitrue, BingX and
Paradex: they already had exponential delays, but reset at open; their fixed
private recovery, missing jitter and repeated-flap reset are now covered.
~~KuCoin client-id collisions~~;
~~`uuid().unwrap_or_default()` in Binance~~ (entropy failure now propagates); ~~Backpack empty positions on any 404~~;
~~OKX `partially_effective` final~~; ~~Lighter late-fill drop~~; ~~Bybit unused `exec_id`~~ (bounded execution-ID dedupe survives reconnects).



Scenario tests, independent phase reviews and acceptance evidence for Binance/Bybit
closures: [`RUST_AUDIT_BINANCE_BYBIT.md`](RUST_AUDIT_BINANCE_BYBIT.md).
Execution and Bybit terminal retention are bounded; private verification remains scripted.


Engine root causes: ~~`Fill` has no execution id~~ (optional execution identity
now crosses FILLS revision 2; Hyperliquid publishes `tid`). The store replaces
lists with no freshness fence, so venue-specific dedupe and terminal fencing
remain necessary.

## Wire contract

Repair lane: `feat/rust-wire-fixes`, based on `bd94abb524`. Historical repair record: #1345 (`c44f22bd8e6`) landed the wire repairs
below; the 2026-10-21 sweep reconfirmed them against `687fe0c1e08`. Ticker
orphan identities remain intact. The completed phases were:

1. Preserve wire information: explicit per-section revisions, display symbols,
   orphan identities, order extras, midpoint snapshots/deltas, and owned ticker
   state. Keep Rust, JS declarations, decoder and FrameApplier synchronized.
2. Preserve market identity: symbol-keyed settings and no market-only flush
   while MARKETS is pending; regression tests cover reorder/removal and queued
   books/candles/fills. Route SETTINGS and rare-query leverage writes through
   the sole FrameApplier owner.
3. Recover apply failures: retain independent account sections, stage identity
   changes, request local/worker snapshots, and test actual frame-to-chaser fills.
4. Run scoped native/JS/SDK checks, combined WASM and Binance plus affected-field
   venue replay gates; create the PR into `feat/rust-rewrite`, post adversarial
   reviews, fix findings, merge only on green CI, and cut a follow-up branch.

Each phase requires an independent sub-agent review and a stable commit. Every
fixed finding gets a reproducing regression. No TS venue deletion or replay
threshold relaxation. Strike findings with the fix PR and its evidence.

- ~~FILLS lose `orderId`, so chaser fill tracking never fires on Rust
  (`frameApplier.ts:469`, `ChaserOrderManager.ts:783`).~~
- ~~`Market.pretty_symbol` never encoded (`store.rs:827`).~~
- ~~Rows on markets missing from the table get symbol `''` (`store.rs:704`,
  `frameApplier.ts:337`).~~
- ~~Settings keyed by market index survive a market-table change (`store.rs:144`).~~
- ~~One throw in `FrameApplier.apply` drops the frame with no resync (`frameApplier.ts:41`).~~
- ~~Never encoded: `conditional_order_price`, bracket trigger types,
  `remaining_notional`, `Ticker.mid_price`.~~
- Low: ~~two `fetchedLeverage` writers~~; ~~`FORMAT` never bumped and reserved u16 unused~~;
  ~~fills precede orders by up to 250 ms~~ (a book-cadence flush carrying fills
  writes the pending store sections with them); ~~`TickerState` views detach on transfer~~.

Repair evidence: actual Rust-writer → JS decoder → FrameApplier regression,
actual FILLS frame → GRVT chaser partial/completion regression, native layout
and market-reorder/queue tests, decoder legacy/revision/bounds/transfer tests,
worker midpoint bridge tests, and local/worker recovery RPC regressions.
FORMAT remains 1 because the envelope is unchanged; section revisions identify
new fixed tails. Both readers check FORMAT; unknown enum values stay absent.
Independent phase reviews found missing STRINGS acceptance, observer exceptions
masking recovery, worker midpoint loss and orphan native-quantity NaNs; all fixed
with regressions. Execution/order flush cadence was subsequently repaired by
#1527 (`8c1e7790217`): a flush with pending fills carries their orders in the
same frame; native cadence regressions retain ordinary book-only behavior.

Phase progress: phases 1–3 and independent reviews complete, committed in
`fb5537d662`. Integrated landing PR #1338 (`1b8592dbd2`) in `a35660f9f3`,
retaining both test additions and adapting renamed helpers/revised Greek tails;
the landed KuCoin Spot leverage path also uses the sole applier owner.
Integrated checks pass: 95 all-feature/89 default engine tests, 25 decoder tests,
187 scoped SDK/worker tests, typecheck/lint, native/WASM Clippy and 586 harness
tests. Combined release WASM built successfully. All 12 public/private gates pass
for Binance, Hyperliquid, OKX, GRVT, KuCoin futures and KuCoin spot.
Public main/steady CPU in ms (TS → Rust): Binance 296.1/166.6 → 158.1/82.4;
Hyperliquid 387.2/44.9 → 81.5/33.8; OKX 1589.2/1188.4 → 1290.8/936.8;
GRVT 171.3/97.4 → 153.7/84.1; KuCoin 408.9/247.4 → 202.6/108.5;
KuCoin spot 525.5/333.2 → 464.3/301.3. All retained-memory comparisons pass.
The first OKX attempt matched output but missed steady CPU by 0.4%; a
same-machine decoder control profile matched output/main CPU, followed by one
full three-run retry with unchanged fixture and thresholds (passing above).
The PR records both attempts and distinguishes profiling from gate evidence.

Phase-4 review additionally found query-only leverage filtered from main, stale
late snapshots, market maxima masquerading as account leverage, and
identity/book baselines surviving rejected frames. Repairs preserve query rails
in coherent snapshot bundles, return only confirmed account rails or null, and
require fresh baselines after errors; each has reproducing SDK regressions.
These are production data-loss/staleness/attribution fixes, not lifecycle parity.
Final phase review passes. Follow-up checks: 54 recovery/worker/contract tests,
12 book tests (two fail with the baseline invalidation removed), 83 JS host/reader
tests, 7 actual-writer cross-language tests, typecheck, production lint and bans.
PR #1345 adversarial round 1 found two additional P2 defects: required identity
references accepted NONE, and local recovery erased query-only leverage. Both
are repaired and independently re-reviewed: 27 decoder tests and 37 recovery
tests pass, with three failing-before controls for each defect, plus typecheck
and production lint. Required references reject NONE while optional values and
empty strings retain their contracts. Local/RPC snapshots share coherent capture.
CI exposed stale tests for the repaired contracts: BreakoutProp synthetic
candles expected a market-only startup flush, and two WASM consumer fixtures
omitted FILLS.orderId. Corrected tests preserve their original data assertions;
36 BreakoutProp public tests and 262 WASM consumer tests pass. The initial CI
also hit Paradex size and two small CPU comparisons; fixed-budget integration
and final green-CI verification remain in progress.
Integrated landing PR #1342 (`fc809a9d85`), retaining its public-feed ownership,
provider callbacks before transfers, lazy text codecs and fixed-budget Paradex
profile alongside all wire repairs. The integration revealed shared public feeds
omitting midpoint; explicit numeric/null/omitted updates now preserve/change/
clear midpoint through the native cached feed and emitted frames. A JSON control
against the incoming DTO failed at runtime; the repaired native target passes
9 tests. Two binary wrapper regressions cover observer/provider exceptions and
catalog validation failures without losing original fills or frame forwarding.
The measured settings-helper size experiment grew WASM by 34 bytes and was
removed; no speculative engine optimization or budget increase is retained.
Integrated checks pass: 95 all-feature engine unit tests plus 9 shared-feed
integration tests, native format/Clippy/bans, 62 SDK tests and SDK typecheck/lint.
Real rebuilt-WASM midpoint/provider regression and all 12 shared-feed tests pass.
CI identified three more fill-layout fixture assumptions (OKX/Phemex/WEEX).
Their repairs preserve execution prices/amounts, ordering and deduplication, and
consume pending MARKETS before expecting a market-only flush. Independent
failing-before controls and all 84 affected native tests pass; review passes.
Repeated Hyperliquid Spot CI startup misses prompted a bounded fixture profile.
Owned TickerState snapshots now copy numeric storage once and keep validated
owned identity bytes for lazy decoding. Three alternating paired replays match
all output; eager/owned median main CPU is 42.634/40.213 ms and steady CPU
13.063/10.739 ms. This does not explain all startup variance. Transfer, required
reference and lazy-decode regressions pass (29 reader cases); independent review
passes. The canonical three-run Hyperliquid Spot gate passes: TS/Rust main
46.3/41.6 ms, steady 17.1/11.8 ms, retained 18.9/6.9 MB. No budget, fixture or
shared engine hot path changed. Host JS tests pass 89/89 and actual-writer
cross-language tests 7/7. All 12 rebuilt affected-venue gates pass at `560b969297`. Final public main/
steady CPU ms (TS → Rust): Binance 189.7/100.8 → 89.2/35.1; Hyperliquid
442.3/32.2 → 60.0/19.9; OKX 2155.2/1668.3 → 1905.5/1461.2; GRVT
302.8/195.5 → 110.1/56.1; KuCoin 424.9/263.6 → 220.3/121.7; KuCoin spot
571.4/350.8 → 501.4/322.6. Every retained-memory comparison and all six private
correctness gates pass. Fresh Bitrue CI passes after the prior registry HTTP 502.
CI still requires confirmation: Hyperliquid Spot startup missed by 0.7 ms and
Paradex by 15.6 ms (both steady/memory/output pass); the prior identical native
Paradex integration passed 317.5/303.7 ms TS/Rust. A bounded unchanged-production-code
retry accompanies the fixture checkpoint. Native CI additionally identified two
Paradex candle tests assuming an early market-only startup frame; repaired
fixtures consume full MARKETS and retain candle quantity/rewind assertions.
Both original failures reproduce with a passing control; all 32 public tests and
formatting pass, and independent review passes. Full workspace native tests run
with no-fail-fast to discover any remaining stale fixtures in one pass. Refreshed
SDK recovery/contract/worker tests also pass 62/62.
Full workspace verification at `164baa955c` passes 2,802 tests across 218 test
results, with zero failures; both general CI runs are green. Native CI and
Paradex pass. The bounded CPU retry still misses Hyperliquid Spot startup by
0.4 ms and Hyro by 0.1 ms, with identical output and passing steady/memory.
Profiling identifies repeated market-index hashing inside the symbol-settings
sort introduced by this repair. Each row now captures its current index once,
then sorts and encodes that same value; indices are never cached across flushes.
Independent review confirms identical known/orphan/empty-symbol order and rails.
A first paired timing was inconclusive. A balanced three-variant fixture
comparison then measures original/cached-index main medians 43.387/38.418 ms,
startup 31.526/27.951 ms, steady 10.736/10.950 ms and identical complete output.
WASM shrinks 1,128,738 → 1,103,477 bytes. Canonical standalone Hyperliquid Spot
passes main TS/Rust 39.3/38.7 ms, steady 12.3/11.2 ms, retained 18.9/6.1 MB.
The constant ticker-loop experiment showed no benefit (+80 bytes), and a
separate serializer boundary added no clear benefit; both were removed. Only
the cached per-flush SETTINGS index is retained. Engine tests pass 95 unit +
9 feed integration, cross-language writer tests 7/7, formatting and Clippy pass.
The final combined rebuild, replays and exact-head CI follow.


Wire lane final allocation follow-up (`feat/rust-wire-fixes`): all twelve final
combined-WASM gates pass at `ebe329766f`; both general CI runs and 34 of 35 Rust
jobs pass. The declared unchanged-head retry passes Paradex but Hyperliquid Spot
misses startup by 0.4 ms (TS 64.6/Rust 65.0; steady 17.9/15.4 passes). Standalone
Paradex host opt2 exceeded the fixed size budget; host s fit but measured worse
CPU. Both experiments were removed, so build profiles remain unchanged.
Review found the ticker reserve still assumed 92 bytes/row after revision 1 grew
to 108. A full-startup allocation regression reproduces capacity 62,348 for 32,152
encoded bytes; the corrected estimate uses 33,222 for identical bytes. It checks
all 128 market refs, midpoints and exact identity tails after string packing.
Independent review passes. Engine lib tests pass 90 default/96 all-features;
Clippy, formatting and seven cross-language writer regressions pass. Alternating
three-pair standalone replays match all output: main median 43.848 → 40.483 ms,
steady 14.503 → 12.333 ms; retained memory is effectively unchanged. Canonical
Hyperliquid Spot gate passes: TS/Rust main 56.1/49.8 ms, steady 20.9/16.4 ms,
retained 18.9/6.1 MB. Exact-head CI and final combined verification follow.

Wire integration checkpoint: the rebuilt `2ebdda7c1c` source passes all twelve
affected-venue local gates and 2,820 native CI test executions. Both general CI
runs and 34/35 Rust jobs pass; the bounded Bybit retry still misses Hyro steady
CPU (TS/Rust 3.4/3.9 ms, main 26.2/22.8 ms passes). Its canonical standalone
control passes main 22.560/19.865 and steady 4.177/3.142 ms, whereas combined
main 33.5/34.3 and steady 10.5/11.6 ms fail. Same-combined-WASM reader controls
match every nonmetric output: current reader beats the landed reader in all
three pairs, with variable steady medians 4.286/6.846 ms. Steady-window profiling
does not support another wire change; no speculative hot-path edit is retained.
Merged landing `5f84b3e21d` in `6c8823dbcf`, including its Bybit numeric ticker
decoder, broker/cancellation and side-aware leverage repairs. Independent engine
integration review passes. Wrapper review caught overlapping leverage flushes;
the existing regression fails before removing the redundant branch flush and
passes afterwards (58 recovery/contract/leverage tests). Incoming broker/scope
tests pass 19/19; integrated engine lib tests pass 99/99, SDK typecheck and scoped
lint pass. Sole leverage ownership and coherent snapshots remain intact.
~~Fresh combined gates and exact-head CI are required before merging PR #1345.~~
PR #1345 merged.

## Security

Addressed by `feat/rust-security-fixes`; regression and gate receipts are in
[`RUST_SECURITY_FIXES.md`](RUST_SECURITY_FIXES.md). These findings were reverified
against current code before fixing. Lifecycle differences alone are not defects.

- ~~Workers Durable Object has no client auth and serves the first client's account
  to all (`hosts/wasm/js/workers.mjs:189`); start race leaks running engines (`:206`).~~
  **M1/M2:** deployment-owned per-account token checked before upgrade;
  conflicting starts rejected, one startup in flight, membership/generation
  rechecked after loading, malformed JSON handled. Options/start failures are
  generic; actionable venue command failures remain visible. Network clients
  cannot inject trusted-provider public feed updates into account valuation.
- ~~Native transport errors include signed URLs (`hosts/native/src/transport.rs:390,404`).~~
  **M3:** strip URLs on connection/send errors. Current reqwest body-stream
  errors already have no URL; that path has a non-disclosure regression rather
  than a claimed defect fix. New delegated signer callback errors
  are generic instead of echoing arbitrary credential-bearing exception text.
  Unstructured custom JS HTTP/body/socket exceptions are sanitized at origin;
  structured venue responses retain actionable errors. Native direct-proxy and
  HOW HTTP authorization-provider failures also use fixed diagnostics.
- ~~Custom credential headers follow cross-host redirects (`transport.rs:101`, `host.mjs:352`).~~
  **M4:** redirects disabled in native HTTP, host fetch and Workers HTTP/upgrade.
- ~~live-smoke: no notional cap, no post-only, testnet reads mainnet env names.~~
  **M5:** fixed $100 quote-notional cap before any write, post-only on supported
  adapters, mode-specific credential variables without fallback. Paired TS USD-M
  fix preserves GTX through REST/NEW mapping and cancel-replace; no relaxed oracle.
- ~~Per-instance nonces collide across engines on one key: hyperliquid, txflow, kraken.~~
  **M6:** atomic per-signer monotonic reservation across native engine objects or
  one Wasm module memory, surviving engine recreation and clock regression.
  Hyperliquid and TxFlow share their underlying signer domain. Separate
  processes/independent Wasm memories still require deployment coordination.
- ~~Low: `setTickInterval` unclamped; unbounded gunzip; credential `Debug`;
  partial zeroization; Paradex 24 h auth signature; Lighter non-hedged Schnorr
  nonce; denylist tape redaction; `set-cookie` in fixtures; broad gitleaks test exemption.~~
  Polling is bounded to 1000–120000 ms with explicit pause; decompression is
  capped at 16 MiB. Credential options/signing diagnostics redact Debug;
  Polymarket credentials cannot Serialize. Internal owned options and stored
  credentials scrub on drop, including Kraken Spot, KuCoin/Spot and Orderly. Paradex auth
  expires in 300 seconds; shorter supported caller receive windows are honored
  while existing TS-compatible defaults remain. Lighter production Schnorr
  nonces hedge key/message/entropy; independent explicit-nonce vectors remain.
  Tape fields/headers are allowlisted, unknown nested payloads scrubbed,
  Set-Cookie removed from persisted fixtures/recordings, and scanner exemptions
  require exact reviewed public bytes AND exact paths.
  Owned-carrier erasure does not claim to erase caller-owned/borrowed JS strings,
  partially failed serde allocations or upstream cryptography/allocator copies.
- Clean: no committed secrets; native frame server auth and isolation; TLS; RNG.

## Architecture

Re-verified on `b6cb612bc7c`; item-by-item source/merge evidence and remaining
dispositions are in [the architecture and gate ledger](RUST_ARCHITECTURE_GATE_AUDIT.md).

- ~~Numeric decoders duplicated 40× across 19 venues, already divergent → `engine::de`.~~
  Shared strict/lenient numeric helpers landed in #1358 (`722e878afb8`);
  preserve each venue's chosen malformed/blank policy (RUST_QUALITY helper 1).
- ~~Socket supervision copied per venue, no jitter, two without backoff →
  `engine::socket::Supervisor`.~~ Shared supervision landed through #1350,
  #1385, #1406, #1423 and #1433; #1512 shares socket specs. Connection-specific
  protocol/authentication behavior remains explicit.
- ~~Venue registry lives only in the wasm host (`hosts/wasm/src/lib.rs:592`) →
  `safe-cex-venues` crate shared by both hosts.~~ Done: `sdk-rs/registry` is shared
  by the hosts (#1467/#1469/#1476), with uniform TS options in #1519 and
  runtime host-clock policy in #1520 (RUST_QUALITY helper 6).
- Bybit fields and venue-named features in the engine (`model.rs:411`, `engine/Cargo.toml:10`).
  **Still present:** `BybitBbo`, BBO/tick-tolerance placement fields and four
  venue-named command features. This is coupling/cleanup debt, not evidence of
  incorrect routing. **Owner decision:** park pure cleanup until after cutover;
  any later change must preserve existing JSON and trading rules.
- ~~Client ids use OS randomness in the deterministic core (`crypto/src/uuid.rs`).~~
  Host-owned identity and separate signing entropy now enter through `Engine`/`Cx`;
  production hosts bind OS/WebCrypto and replay hosts inject tape-derived identity.
  Missing providers fail construction; CI bans ambient core/venue draws (#1624,
  `0cd8f2b1657`). Signing remains cryptographic unless explicitly injected.
- ~~~30 URL encoders in three dialects → `engine::url`.~~ Shared encoders and
  query dialects landed in #1368 (`e4cdde22cd3`); signing-specific spellings remain
  explicit (RUST_QUALITY helper 2).
- ~~`RangeWalk` used by 6 of 26 venues~~ (shared `CursorWalk`/`windows`/`OffsetWalk`
  now serve the documented batches; retained paging exceptions have reasons in
  [RUST_HISTORY_PAGING.md](RUST_HISTORY_PAGING.md)). Book storage is shared;
  venue-specific snapshot/sequence/checksum protocols still require distinct
  stitching policies. Their count alone does not establish a production defect.
- Read-key routing remains repeated in venue signing/request paths. The shared
  `ReadCredentials` type does not select credentials. Preserve GET-only behavior,
  private/write boundaries, passphrase inheritance and Bitget's degraded-balance
  primary-key exception when sharing it. ~~Copied per-venue test probes~~ were
  replaced by `engine::test_support::ProbeVenue` in #1427 (`1435005aea3`);
  13 secondary-test files retain venue-specific assertions. This re-audit
  established no wrong-key or missing-read-key production defect. ~~Owner
  decision: park selection deduplication until after cutover.~~ Superseded by
  the 2026-10-22 owner choke-point decision: selection moves to the engine per
  [RUST_CREDENTIAL_SELECTION.md](RUST_CREDENTIAL_SELECTION.md).
- ~~`docs/plans` has 47 files; collapse the evidence documents; roster stale.~~
  **Owner decision:** retain the documents and use the single
  [topic/status/read-trigger/file index](README.md). The old in-progress port
  labels are refreshed in [the roster](RUST_REWRITE.md#8-port-inventory-and-historical-receipts).
- ~~Workspace manifest, `rust:*` scripts and `.cargo/config.toml` comment cleanup.~~
  Re-check found 68 Rust scripts with all 61 referenced source paths/patterns resolving;
  the 128 KiB stack comment matches `131072`. Workspace profiles/layout are
  documented in [the artifact audit](RUST_WASM_ARTIFACT_SIZE.md). No remaining
  broken-path/comment defect was demonstrated; feature-name debt remains above.

## Gate honesty

Gate implementation and measurements are tracked in
[RUST_GATE_HONESTY.md](RUST_GATE_HONESTY.md). Strict replay failures remain FAIL;
CI can accept only the exact venue, tape hash and diagnostic fields in the
single [`known-gate-failures.json`](../../sdk-rs/harness/known-gate-failures.json)
list. An accepted known failure is not a parity pass. Baseline/workload and
candidate/fixer integrity failures cannot be exempted.

Implemented findings (source and merged-PR receipts in
[the re-audit](RUST_ARCHITECTURE_GATE_AUDIT.md); primarily #1346, `a22e6b6ac1e`):

- ~~Circular Binance/Bybit margin expectations.~~ Expectations come from taped native position/configuration
  fields. Isolated TS fixers cannot receive candidate output; AST and mutation
  controls reject circular fixes.
- ~~Comparison omissions, duplicate masking and final-only coverage.~~ Compare TP/SL and each leg's trigger source, conditional execution price,
  parent IDs, open/flat-position realized PnL and fill
  price/fee/time/execution identity. Client IDs and fill prices default on; any
  exclusion requires an explicit venue/field/reason. Duplicate public symbols
  fail before filtering or map construction. Compare production receiver state
  at startup, tape quarters, commands and terminal checkpoints, before fixers.
  Validate each runner's boundaries against the tape plan independently, with
  required account/book sections; matching omissions cannot establish coverage.
- ~~Unbounded fixer mutations.~~ Per-venue leaf permissions and structural rules constrain every fixer;
  overwritten-field counts are reported. No fixer rewrites candidate requests.
- ~~Three-run/partial-load performance evidence.~~ At least seven fresh
  interleaved performance pairs alternate the first runner. Main/steady CPU
  objectives below 100 ms TS use nine pairs and allow at most max(1 ms, 5%)
  above TS (#1461, `dfbc0a94bc2`); larger windows, process CPU and retained
  memory require 10% improvement. CI requests nine public pairs. The small-window
  exception cannot detect a regression that remains within its allowance.
  Implementation loading and WASM compilation count in total CPU. Audit
  callbacks, checkpoint reconstruction and baseline GC are outside performance
  intervals. Retention subtracts the idle-loaded module baseline and counts
  WASM growth once. Each performance run must reproduce its own audited final
  output and transport receipt. The gate does not select favorable pairs;
  separate CI retries remain separate observations, not erased failures.
- ~~Missing combined budget / undecided artifact layout.~~ One budget source
  now enforces 14,155,776 B combined and 1,343,488 B family ceilings
  (#1528/#1535); combined web/Node are checked before standalone overwrite.
  #1393 selects families for browser/Workers; explicit combined dispatches are
  also supported. No budget change is made by this audit.
- ~~Bitget USDC has no independent public/private CI case.~~ Public/private
  cases have explicitly authored synthetic
  provenance. A private query timeout remains incomplete evidence, not a pass.

Remaining evidence limitations:

- Private CI tapes are documented/synthetic, not live-private certification.
  Live-smoke tooling and scripted lifecycle regressions exist; no all-venue
  approved live-capture corpus was found. The tape-time sweep now makes the
  effective private clock policy deterministic for all 50 catalog aliases
  (49 explicit flags plus polling BreakoutProp). Merged per-family proofs in
  [HARNESS_TAPE_TIME.md](HARNESS_TAPE_TIME.md) compare complete nonmetric outputs,
  including IDs/requests/receipts/checkpoints, over five pairs. Determinism does
  not close private parity or live-evidence gaps; declared differences stay FAIL.
  Several TS handlers emit execution identity or timestamps, now preserved by
  the comparison. Rust now preserves Hyperliquid execution IDs; other venues'
  execution IDs and live-fill timestamps remain unreported where not supplied.
  Other TS handlers lack them too. Null/null equality cannot establish those capabilities. Missing wire fields
  remain missing, never reconstructed into candidate output from the tape.
- Some tape-derived TS corrections still rebuild sections. Their declared leaf
  fields and overwrite counts are visible; raw checkpoints do not use them.
- Paired TS fixes mean the oracle is not unchanged master TS. **Owner decision:**
  retain executable TS references as the CPU/memory oracle through cutover;
  delete neither venues nor references before then. Before later retirement,
  preserve independently reviewed correctness/performance evidence and declare
  deltas; Rust-versus-itself is not a replacement oracle. No frozen-output or
  independent-oracle completion is claimed by the tape-time sweep.
- Existing public recordings were driven by Rust; TS-only subscriptions can
  still be absent. The [opt-in public TS recorder](HARNESS_TS_PUBLIC_RECORDER.md)
  adds offline-tested capture support for audited Binance USD-M production TS,
  observing actual outgoing subscriptions and raw adapter response bytes. It
  does not replace the Rust recorder or any existing fixture. Unsupported
  reference/custom/session paths fail explicitly. Actual TS-origin recaptures
  for this and other venues still need owner-approved live recording; fake
  transport tests prove mechanics, not captured venue coverage. HyroTrader,
  BreakoutProp and Bitget USDC public fixtures retain synthetic provenance.
- ~~No actual browser cold-start observations.~~ #1688 (`b6cb612bc7c`) adds
  actual Chrome worker observations for 28 families plus a combined diagnostic,
  with three fresh processes/profiles each. It remains measurement-only with
  no threshold; a proposal requires at least three completed landing runs.
  The replay harness separately measures implementation loading/runtime plus replay routing and
  minimal receipt recording. Common harness imports precede those intervals;
  this is not a browser cold-start or pure venue-handler benchmark.
- Routine CI replays use the selected standalone family build. An explicit
  combined dispatch can measure that build, but standalone results do not
  establish combined runtime performance. Browser module readiness has its own
  provenance and limits; it is not a replay CPU/memory or account-startup gate.

Current source requires 32 suites / 110 exact venue/tape/books cases. The
mandatory verifier rejects missing/duplicate/substituted cases and protected
integrity failures. A green CI result may include exact declared FAILs; unused
declarations and missing native fill metadata must remain visible limitations.
The harness lane owns the remaining validator/fixture/declaration work; see the
re-audit ledger for the Hyperliquid and Orderly caveats. Final sweep #1694
(`e87107621f6`) pins all 50 effective clock policies and removes exactly 20 stale
WEEX fee leaves; ten stable raw outputs and 112 exact remaining leaves establish
repeatability/declaration scope, not private parity.

Before cutover: live-smoke private captures on every venue (mainnet minimum size
for coinbase, backpack, mexc with approval), a 3–7 day read-only production
shadow diffing full store snapshots, public fixtures re-recorded with TS, and a
per-venue canary holding the formerly circular-evidence venues last.

## Port reviews (Opus, per venue vs TS)

### Bitunix

- ~~Bitunix's query page-cap ownership omission~~ now carries the actual request
  command before partial completion, as do duplicate/nonadvancing-page markers.
  Failed pages reject rather than masquerading as complete empty/partial history.

Originally reviewed on `feat/rust-bitunix` at `846f285d13`; #1309 merged as
`3acf9e46392`. Paths below are relative to `sdk-rs/venues/bitunix/src`.
Remaining resilience and completeness findings are independent of request parity.
- High: ~~app stop payloads are rejected. `trading.rs:265` refuses any order with
  `trigger_price`, including StopLoss/TakeProfit sent with `triggerPrice == price`
  (TS places them from `price`, `bitunix.exchange.ts` `formatTPSLOrder`), so a
  dragged SL/TP leaves the position unprotected. Accept protective kinds whose
  trigger equals the price (as Orderly does).~~ (accepted via
  `redundant_protective_trigger`; distinct triggers still reject)
- High: ~~the futures socket (books, candles) is tagged `SocketChannel::Public`
  (`lib.rs:127`), so the host applies the public route; its fallback is the public
  proxy, not `bitunixFutures` (`routingTable.ts` `business`). After two silent
  connects or a watchdog recover the socket sticks to an endpoint that ignores
  depth/kline subscriptions while still answering pings: books and live candles go
  dead with no log. Tag it `SocketChannel::Business`.~~ (tagged Business)
- Medium: ~~reconnects have no backoff; refused connections loop at network
  speed across futures, private and every ticker shard~~. Each stable feed now
  retains a shared socket Supervisor across epochs, with capped jittered retries,
  connection deadlines and real-traffic stability reset. Retired epochs and their
  queue/login timers cannot affect replacements; routing roles/feed/recover are
  preserved. Opened private sessions retain first-pong silence compatibility.
- Medium: ~~one unparsable catalog/ticker row fails the whole list and retries
  forever; a ticker failure also blocks private startup~~. Typed public row
  failures are skipped with coded warnings while valid peers commit; empty or
  wholly unusable seeds retry without establishing readiness. Private startup
  completes from the usable catalogue while tickers remain pending, and ticker
  recovery never resets private settings/state. Shared capped discovery backoff
  preserves native/HTTP rate-limit floors and degradation/recovery; malformed
  streams/envelopes remain failures with sanitized public diagnostics.
- Medium: ~~one unmappable position row holds all financial peers and absence
  forever~~. Rejected native symbols retain their cached hedge legs, settings and
  REST routing IDs; valid peers and explicitly closed zero rows advance after a
  complete envelope. Unknown identities hold absence, known duplicate conflicts
  quarantine their whole symbol, and partial mode mixes remain atomic. Partial
  initial reads cannot establish readiness or prove trading prerequisites.
- Medium: ~~one malformed regular/TP-SL order row holds every healthy order peer~~.
  Request-local native raw-ID coverage retains rejected cached parents, attached
  brackets or both standalone siblings together with routing metadata. Valid peers
  and safely absent raw families advance; admitted parents remove old omitted
  brackets despite an unknown neighbor. Unknown identities hold absence only in
  their endpoint scope, and only admitted raw groups advance fill baselines.
  Initial readiness requires both current serials without rejected rows, while
  command-owned partial refreshes cannot prove a trading prerequisite.
- Medium: ~~WS position-ID notifications invalidate in-flight financial snapshots
  and postpone reconciliation indefinitely under steady activity~~. Ordinary
  positions now retain their in-flight serial for a bounded read interval and
  survive WS-only generation changes, with separate write-ACK revision fencing.
  Command-owned preflight remains strict; notifications coalesce one follow-up
  without moving deadlines, and lost reads recover without admitting old replies.
- Medium: ~~Order/TP-SL WS generation fences the entire order snapshot and
  starves healthy REST peers~~. Ordinary reads use current endpoint serials and
  command-mutation revisions, retaining newer WS raw families, native metadata and
  terminal absence while healthy REST-only peers advance. Malformed known/unknown
  WS coverage quarantines only its family/endpoint absence; bounded overflow holds
  cached data and cannot establish initial readiness. Both complete initial halves
  retain overlap until atomic publication. Owned preflight remains generation
  strict. Independent endpoint tokens retain the first repair deadline, and write
  response/cleanup fences include partial-cancel ACK effects before error exits.
- Medium: ~~history failures return success with empty or partial rows~~.
  Rust now rejects failed first/later pages (transport, HTTP/native, malformed
  rows/envelopes), so a failed read cannot establish complete history. Full
  duplicate/nonadvancing pages and caps retain validated rows only with an owned
  `history.incomplete` marker before completion. Empty end pages and proved
  requested-start crossings remain complete. TS historical fills warn and return
  empty rows; orders toast and return empty rows, and income toasts and returns
  collected rows. Those fallbacks do not establish completeness or justify a
  clean native success on a failed page.
- Medium: ~~one failed `batch_order` chunk drops IDs already placed by earlier
  chunks~~. #1589 (`ef362708305`) preserves acknowledged IDs and effective legs
  in one classified failure, with Unknown sticky across later refusals. Native
  partial/malformed batch regressions cover this; remaining chunks follow the
  existing failure policy rather than blindly resending accepted writes.
- Medium: ~~balance notifications invalidate every overlapping balance REST read
  and postpone repair; reconnect/debounce refreshes supersede slow reads~~.
  Ordinary balance keeps its current serial until completion or the existing
  ten-second lost-read bound, with issue time recorded at dispatch. WS-only
  notifications retain the first repair deadline and no longer fence financial
  progress; accepted position-mode ACKs retain a separate write revision. Late
  old callbacks cannot release newer reads, and synchronous signing failure
  releases before ordinary repair. Financial values and500ms polling are unchanged.
- Medium: ~~account-read failures (auth, IP whitelist) are logged at Warning and
  retried at ~1 Hz~~. Current signed account/settings reads now report selected
  credential/scope evidence from strict native refusals and validated success
  shapes; merged order endpoints and per-symbol settings retain other refusals.
  Reads back off from1s to30s, with a shared60s minimum rate pause and bounded
  Retry-After. Every issuer obeys the pause; blocked owned preflight fails once
  NotSent and never replays a write. Sanitized coded diagnostics preserve native
  and rate metadata. Regression coverage includes malformed lists/rows, stale
  callbacks,40 settings symbols, both shared-expiry orders and owned no-write.
- Medium: ~~per-symbol settings reads starve under any position notification,
  hiding leverage/margin updates and scoped refusal/backoff evidence~~. Settings
  attempts now retain their symbol serial and accepted settings-write revision;
  WS-only routing-ID notifications cannot supersede them. Accepted leverage,
  margin and position-mode ACKs still fence old metadata and auth evidence;
  refused/unproved writes do not. Current failures retain ordinary bounded
  retries, and grouped per-symbol credential refusals remain independent.
- Medium (unverified venue behaviour): leverage serialized as `30.0`
  (`trading.rs:344`, TS sends an integer); replacement after a virtual-leg cancel
  requires order detail to be final immediately (`trading.rs:1437`); ~~a login
  rejection without `data` is ignored~~. Missing/null/malformed login results now
  emit a sanitized malformed-stream warning and coalesced retry on the originating
  socket; explicit false retains its existing rejection diagnostic. Success and
  retirement cancel pending retries, and old timers cannot sign on replacements.
- Low: ~~no clock sync~~ (Bitunix requests synchronized SDK host time before
  startup; authenticated REST and private login use that time); signature-rejection
  (10007) resend remains separate (`sign.rs:73`); rejected
  login retries every 2 s on the same socket (`account.rs:766`); private watchdog arms
  only on `op:"pong"` (`lib.rs:394`); ~~`tealstreetWsRoute` not honoured~~ (engine hosts now select each declared
  Bitunix proxy on the first connection when explicitly requested; false/unset
  retain two silent direct attempts and existing fallback recovery); ~~book kept
  while disconnected~~ (futures retirement immediately invalidates subscribed
  books and publishes consumer clearing before retry; current interests replay
  once on replacement); ~~secondary read keys dropped~~ (validated native options
  now reach the GET signer, with api-key secondary egress in SDK/native plans;
  mutations/private login remain primary and public discovery stays unsigned); `data: null` envelopes treated as
  failure (`wire.rs:286`); TP/SL without `orderId` in the reply fails the command;
  caller `clientOrderId` with inline TP/SL rejected; pre-session fill can be emitted
  as the cumulative amount before the first orders snapshot (`account.rs:989`); TP/SL
  side defaults to Sell until the position id is known (`account.rs:598`).
- Shared by both: amending a standalone TP/SL or placing a full-position one can
  borrow a sibling leg from another TP/SL order by `positionId`
  (`trading.rs:554,1006`); cancelling one leg of a combined position TP/SL cancels both.
- Confirm (TS bugs Rust fixed): exact-tick lot splitting within min/max; market
  orders with inline TP/SL go to `place_order`; TP+SL on a full position merged into
  one payload; caller ids on TP/SL rejected rather than dropped; stop-limit no longer
  sent as a resting limit; recreate after cancel/amend uses remaining size and keeps
  TIF; cancel errors checked; stale snapshots cannot reopen terminal orders and
  failed order reads no longer wipe orders; historical fills dedupe by `tradeId`
  (live cumulative events use execution watermarks, without a fabricated ID); history
  respects from/to; funding time from `nft`; decimal depth precision in topics;
  unsubscribes actually sent.

### OKX

- ~~OKX's three request-associated cap/cursor ownership omissions~~ now carry
  the actual closed-position or history query command before retained rows
  complete. Native regressions cover exact Warn ownership/order at Error-only
  logging; EEA composite-source failures remain failures.

- ~~High: blank `bidPx`/`askPx`/volume or any failed instruments/tickers read at
  startup halts the venue with no retry (`okx/src/lib.rs:841,995`); TS treats
  blanks as 0 and continues. Parse blanks as 0, retry with backoff.~~ Fixed:
  blank or malformed seed quotes start at 0 (row kept for live updates); a failed
  or unusable catalog/ticker read retries the whole product chain with backoff.
  Verification: JSON `null` quotes now behave like blanks (they dropped the row),
  live nulls keep the last good value, and an all-undecodable ticker read retries.
- ~~Medium: one bad instrument row aborts the catalogue (`lib.rs:738`); leverage
  for flat symbols missing (no `leverage-info`); account config (posMode, acctLv)
  read once, so a mode change in the OKX app makes every order reject.~~ Fixed:
  bad rows are skipped and counted in a warning; `fetchLeverage` reads
  `leverage-info` (it previously never answered); config is re-read every 30 s
  and a changed mode/level reconciles positions.
- Low: ~~`mmp`/`elp` order types get an empty id; conditional `price: 0`
  rejected~~ (regular types keep their `ordId`; `price: 0` sends `orderPx: -1`,
  market once triggered, as TS); income limit truncates (kept: Rust returns the
  newest `limit` whole bills where TS returns up to a page more, and the EEA
  SWAP+FUTURES merge relies on it).
- Confirm (TS bugs Rust fixed): OCO leg edit deleting its pair, triggered algos
  staying open, X-Perp history SWAP hardcode, market-order amend placing a new
  order, duplicate algo cancels. ~~Protective conditional orders omit
  `reduceOnly` and invert hedge `posSide` incorrectly~~: #1302 (`2b37754970c`)
  closes the correct leg for SL/TP. Generic Conditional intentionally opens a
  position under the documented conditional-order contract.

### Hyperliquid

- ~~User fee tiers use the API signer address instead of the trading account.~~
  Native fee reads use the vault or applicationId account, after successful
  identity discovery when needed. Agent, master and vault addresses are tested
  separately; failed/timed-out discovery cannot emit a premature fee query.
  [Native regression scope and official contract](RUST_HYPERLIQUID_FEE_SUBJECT.md).
  The analogous TS production defect remains an owner-managed follow-up; TS is
  unchanged by this native repair.

- ~~Hyperliquid's rolling order-retention ownership omission~~ now carries the
  actual query command before completion. A native 2,000-row regression checks
  retained financial rows, Warn ownership/order and Error-only delivery.

- Owned-history follow-up: ~~later actual page/stage failure discards accepted
  history, and inclusive stalls/caps reject the entire accumulated read~~.
  Existing CursorHistory now owns staged admission, completion, page budget,
  stage restart and rolling-retention gaps. Actual first/zero-eligible failure
  rejects typed; later loss retains only admitted output with one owned gap.
  Native inclusive progress, output mapping/limits and lookup/spot behavior
  remain. Cross-family fan-out, managed completion and B11 runtime proof stay open.

Signing hash, order payload and price/size rounding match TS; no new highs.
- ~~Medium: HIP-3 leverage change sends cross where TS sends isolated
  (`hyperliquid/src/trading.rs:758`); read `onlyIsolated`/`marginMode`. HIP-3
  margin-type change with no position fails (default leverage 20 > market max,
  `trading.rs:759`). Spot open orders never reconciled and can go stale
  (`account.rs:426,1604`). Edit aborts when the source order already filled
  (TS places a second order) — confirm fail-closed is intended.~~ Fixed:
  `onlyIsolated`/`marginMode` seed isolated settings with leverage capped at the
  market maximum, and cross on an isolated-only asset is refused before signing;
  standalone spot reads `frontendOpenOrders` at startup, on every refresh and
  every 10 s. Edit fail-closed is kept: placing a replacement for a filled source
  doubles exposure, which is worse than a visible edit error.
- Low/confirm: spot testnet signing and spot size rounding (TS bugs Rust fixed);
  ~~spot vault signs as vault (TS as master)~~: #1306 (`5c47683969f`) signs
  with the configured signing key and binds the vault address in the action hash
  and payload, rather than copying TS spot's omitted vault routing; ~~more terminal statuses~~
  (Rust treats every `*Canceled`/`*Rejected` as terminal, a superset of TS);
  ~~`stopLoss: 0` rejects whole order~~ (0 means no bracket on every market, as
  TS); ~~signed fees~~ (deliberate: TS `abs` mislabels rebates); ~~unified
  balance spot slices not populated~~ (spot holdings publish as `coins`;
  currencies stay unset because TS's `currencies.USDC` is a synthetic sizing
  slice, not the raw spot row); ~~fills lack execution id, realized PnL and fee~~
  (FILLS revision 2 carries `tid`; #1430 financials retain live venue conventions).


Known nonce limits (owner decision: document only; no production change):
`engine/src/nonce.rs` `reserve` keeps signer-domain history and allocates
`max(floor(now), previous + 1)`. The `hyperliquid-agent` domain keys that history
by the signer's public-address digest (`NonceKey::ethereum_address`); Hyperliquid and TxFlow writes reserve through
`venues/hyperliquid/src/sign.rs` `reserve_agent_nonce`. Engine recreation and backward clock
resynchronization within the same native process or Wasm memory preserve that
history. The synchronized SDK clock is passed at
`packages/safe-cex/src/exchanges/rust/rust.exchange.ts:892,917`; it can move
backward (`packages/safe-cex/src/utils/virtualClock.ts:98–110,151`).

- Fresh process, worker or Wasm-memory restarts lose the history, as documented
  in the `sdk-rs/engine/src/nonce.rs` header. A same-ms restart after the previous counter
  ran ahead of wall time can reuse a nonce. Deployment boundaries must coordinate
  signers if they need continuity across those restarts; TS Date.now-based
  signing has the same restart limitation.
- The allocator checks finite/nonnegative time and the safe-integer ceiling,
  rather than the venue's T−2-day/T+1-day freshness window
  (`sdk-rs/engine/src/nonce.rs` `floor` and the `HYPERLIQUID_AGENT` limit). A >1-day forward clock excursion, or
  sustained >1000 writes/s for one signer, can leave its counter ahead of the
  allowed future window even after clock correction. These are known limits,
  not regressions from #1693; synchronized time reduces ordinary skew rejections.

### Binance family
- ~~The known USD-M amend double exposure also hits PM, where TS throws on -2011
  and Rust re-places (`binance/src/trading.rs:530`): a regression.~~
  Fixed with the USD-M/PM cancellation-proof and racing-fill regression matrix.
- Medium: ~~PM upserts `fs:"CM"` rows as phantom positions (`account.rs:1381`)~~;
  ~~REST candles with `from`+`limit` return the oldest N instead of newest, no
  1500 cap (`lib.rs:1439`)~~; ~~COIN-M strict batch discards placed ids on a partial
  reject (`trading.rs:397,700`)~~; ~~PM `close: true` with an amount closes the whole
  position (`trading.rs:1087`)~~; ~~COIN-M unpriced coin excluded from totals
  (TS counts 1:1 — confirm)~~ (deliberate: an unpriced coin waits for a price,
  then counts $0 with its native amount kept, where TS adds a fabricated $1 per
  coin; `inverse_unpriced_collateral_keeps_its_native_amount_without_inventing_usd`); ~~CancelOrders with unknown ids reports success
  without sending (`trading.rs:224`)~~ (explicit errors; see
  [cancel audit](RUST_UNKNOWN_CANCEL.md)); ~~history maps `EXPIRED_IN_MATCH` to filled
  (`history.rs:687`)~~; ~~spot/COIN-M history returns plain `[]` instead of
  unsupported~~; ~~USD-M journal (`fetchHistoricalPositions`) not wired in the host~~ (folds
  per-symbol fills as TS).
- Low: ~~fixed `recvWindow`~~; ~~Binance UI STOP orders amend to stop-market~~
  (native `STOP`/`TAKE_PROFIT`/`*_LIMIT` stops refuse amend as `unsupported`
  before any write: re-placing them as `STOP_MARKET` at the old limit price would
  trigger at a different price).
- Known risks, kept by decision (overseer, 2026-10-21):
  - closePosition conflict cancel in batches: `trading.rs` cancels an existing
    full-position stop for every `PlaceOrders`, batches included, while TS only
    does so for single `placeOrder` (its batches hit -4130 and keep the old stop).
    Kept: replacing the stop is what the caller asked for, and parity would change
    which orders are sent.
  - PM -2011 plain cancel marks canceled: outside an amendment, a PM -2011 is
    treated as already gone and silent, where TS shows the error and keeps the
    row. A cancel on the wrong endpoint or with stale ownership could hide a live
    order locally until the next snapshot. Kept: changing cancel reconciliation is
    RISKY without live -2011 evidence; snapshot reconciliation bounds the window.
- Follow-up fixes and scenario evidence: [Binance/Bybit follow-up](RUST_AUDIT_BINANCE_BYBIT_FOLLOWUP.md).
  Native unsupported histories reject; SDK methods expose matching branded Base
  fallbacks. Existing unknown-ID mixed-batch cancellation preflight was reverified.
  PM canonical ownership and Spot quantified-ACK delivery are also covered there.
- Confirm (TS bugs Rust fixed): fill price `L` not `ap`; spot fill price on
  market orders; spot cancel uses `C`; amend keeps TIF; PM conditional client id
  `newClientStrategyId`; PM TRIGGERED closes; PM cancel-all both endpoints;
  COIN-M rejects caller client ids; spot retired streams; depth snapshot retry.
- Verify live (unconfirmed, no code change): a COIN-M triggered stop that keeps its
  `orderId` after `EXPIRED` and then rests on the book might leave the row Canceled
  and block later order snapshots; several `x=CALCULATED` liquidation partials with
  `t=0` on one order would dedupe to the first fill (`account.rs` execution dedupe).
  Both need a captured live packet before changing behaviour.

### Bybit family

- ~~Bybit's closed-PnL and unbounded-order-history cap ownership omissions~~ now
  carry the actual request command before retained rows complete. Both native
  regressions check exact ownership and order at Error-only logging, preserving
  row counts and existing window/cap policy.

- ~~Medium: HTTP-200 discovery `retCode:10006` loses reset-timestamp headers
  and retries before the limit clears.~~ Fixed: typed rate-limit envelopes keep
  their headers; reset and Retry-After hints extend all waiting category retries
  without changing attempt counts, bounded by the shared one-hour cap. HTTP403
  keeps its ten-minute floor; success headers do not manufacture a pause.
- ~~High: Brazil inverse/option sockets routed to the linear host (engine
  transport keys routes by channel, `packages/safe-cex/src/transport/engine.ts:552`,
  `bybit/src/lib.rs:407`).~~ Fixed: browser and native hosts pick a product route
  by native URL (`publicWsRoutes`): inverse/spot use the `bybitinverse`/`bybitspot`
  hosts as TS does; options stay direct (no option proxy is deployed).
- ~~High: per-coin balances never set (`account.rs:662`).~~ Fixed: `Balance.coins`
  (equity, else wallet balance; USD value, else amount) from every accepted wallet
  read, merged from stream pushes with explicit zeros; failed reads keep holdings.
- ~~High: fills history includes funding/settlement rows (`history.rs:564`).~~
  Fixed: Funding/Settle/Settlement/Delivery/SessionSettlePnl (any case) and rows
  without a valid side, price, quantity or time are excluded from REST fills.
- ~~High: income dedupe drops hedge-mode funding legs (`history.rs:131`).~~ Fixed:
  income ids keep TS's shape; distinct native rows that would share one (same-
  instant hedge legs) get their native id, side and signed size appended, and
  exact page repeats still collapse.
- ~~Medium: option failures stall linear startup (`lib.rs:857`).~~ Fixed: a failed
  option catalog or seed lets the other products publish (markets, tickers,
  sockets, account); options join later by appending (stable indexes, live rows kept).
- ~~Medium: option catalog `baseCoin=All` unverified.~~ Dropped: the instruments
  documentation explicitly permits `baseCoin=All` for `category=option`.
- ~~Medium: live candles lack trade aggregation and seconds intervals.~~ Fixed:
  `publicTrade` extends the native kline bar (every trade in a packet, in order;
  Monday weeks, UTC months), seconds intervals are built from trades alone; Hyro
  stays native-kline-only.
- ~~Medium: option WS ticker field names likely wrong.~~ Fixed: documented option
  `bidPrice`/`askPrice`/`change24h` (REST option rows: `change24h`) now update quotes.
- ~~Medium: no UNIFIED retry on the wallet call.~~ Fixed: a CONTRACT read refused
  with `accountType only support UNIFIED` switches the account to UTA and retries
  once; other errors never change the account type.
- ~~Medium: demo reduce-only fill check missing.~~ Fixed: a locally published Demo
  reduce-only limit is checked against its executions at 1 s, 3 s and 8.25 s; a
  complete cursor walk (20 pages, as TS) proving a full fill retires it (terminal
  fence); otherwise, or on failure, orders are re-read after the grace. Partial,
  failed or capped walks never prove a fill.
- ~~Medium: Hyro tears down on one bad poll (`account.rs:1838`).~~ Fixed: a periodic
  read failing in transport or at the venue keeps the ready stream and last-good
  state and retries next tick; malformed content and initial snapshots still
  reconnect.
- ~~Medium: order/income history caps.~~ Fixed for orders: an omitted limit reads
  every page up to TS's 20 × 50 per window (then warns and keeps the rows, as TS)
  instead of truncating to 50; explicit
  limits still cap after every category. Income caps match TS (unchanged).
- ~~Medium: Partial TP/SL amend semantics — decided: match TS (flip SL/TP on drag),
  do not amend in place~~ ([owner-selected cancel/recreate regressions](RUST_AUDIT_BINANCE_BYBIT_FOLLOWUP.md)).
- ~~Medium: spot create params (`marketUnit`).~~ Kept: `baseCoin` for market orders
  is a documented TS correction in `harness/gate.mjs` `KNOWN_TS_FIXES`.
- ~~Medium: one unmappable order invalidates every later snapshot (`account.rs:1569`).~~
  Fixed: ordinary REST/WS reads admit valid peers while rejected keyed rows keep
  their stored copies; unidentified rows hold absence without freezing healthy
  peers (#1620, `4592d985e8a`). Hyro retains its strict atomic handshake.
- Low: ~~OHLCV from+limit~~ (a limit asks for the newest bars up to `to`, as
  TS); ~~no market refresh~~ (explicit atomic native catalogue refresh); ~~Hyro market filter;
  subscription breadth~~ (decided: USDT perpetuals only, Hyro's own rule; foreign
  categories are dropped by row, bandwidth only); split-lot error handling (RISKY:
  one unformattable lot rejects the batch before writes, where TS skips
  that order); ~~classic upnl; inverse reference price~~ (Rust is the
  same or better: live upnl on every account type, row mark before tickers);
  ~~demo key hint~~ (bounded signed opposite-host probe on startup10003;
  only confirmed built-in linear live/demo mismatches add guidance);
  ~~recvWindow~~ (decided: capped at 20 s); rate limiter (RISKY: send timing);
  ~~Hyro early pings~~ (stale: watch wakes do not advance the 10 s ping deadline;
  active-cleared/consumed timer regression confirms supervision).
- Confirm (TS bugs Rust fixed): spot amount precision, option books socket, spot
  cancel-all groups, spot trailing/full TP/SL endpoint, inverse position-mode
  category, stop with only price, category-filtered private topics,
  PartiallyFilledCanceled, inverse units, failed position read zeroing, create
  network errors, live candle volume units.

- ~~A leverage ACK can be overwritten by an older in-flight positions read~~:
  acknowledged writes (including native not-modified ACKs) advance the existing
  positions revision and coalesce fresh read metadata. Regression proves the
  20x→10x stale overwrite, fresh repair and refused-write admission; no writes
  are resent.

### BloFin

- High: ~~a finite cumulative quote can overflow its incremental price division,
  emit an infinite-price Fill and consume the corrected same-quantity suffix.~~
  Shared engine prefix/delivery preparation now validates division, contract
  scaling and public rounding before live progress commits. Two separate native
  original-RED/core-GREEN controls prove rejection of `(inf, 0.0005)` and recovery
  of the corrected `.0005 @298` receipt once. Both failures were reproduced on
  unchanged actual #1729 source before the fix; this is a real financial defect,
  covered by the owner's same-helper-contract approval. Native historical floors,
  missing-quote observation recovery, startup census/buffering and 500-prefix
  retention stay separate; no public Fill is synthesized from historical data.

- ~~BloFin's closed-position page-cap ownership omission~~ now emits the actual
  request command before retained rows complete. Its native cap regression
  checks exact ownership and ordering even at Error-only log level.

- High: ~~a failed or malformed startup ticker read blocks the private side
  without retry~~. #1356 (`d65119bea35`) starts private reads independently
  after usable markets and retries ticker discovery; #1615 (`c63acfc19c2`)
  adds shared degradation/recovery reporting on that retry schedule.
  ~~`brokerId` never sent,
  losing broker attribution (`trading.rs:126`, TS `blofin.exchange.ts:2159`)~~
  ([attribution audit](RUST_BROKER_ATTRIBUTION.md));
  ~~per-symbol margin mode stays stale after a position closes, so orders go out
  in the wrong margin mode (`trading.rs:271`, `account.rs:614`).~~
- Medium: ~~`null` in order/position/ticker fields rejects whole snapshots/frames
  (`wire.rs`; verify against one live frame)~~ (documented optional protection
  nulls and row-local WS recovery covered; captured public tape has no nulls;
  invalid required private REST quantities still preserve the snapshot);
  ~~WS login timestamp serialized as float (`account.rs:284`)~~;
  ~~one odd instrument row aborts startup~~ (#1356, `d65119bea35`: mixed valid
  and malformed catalogue rows retain valid markets and retry repair);
  ~~cancel of an id the store lacks resolves OK with no request
  (`trading.rs:671`)~~ (explicit error).
- Low: ~~strict leverage fetch~~ (no user impact: the bridge turns a refused
  read into `null` plus a warning, as TS returns `null`); ~~trigger TP/SL not
  tick-rounded; missing iso/cross warnings; 102143 not translated; history
  shape~~ (order history now reports base units like open orders and fills, a
  deliberate divergence from TS; `BLOFIN_RUST_PORT.md`).
- Confirm (TS bugs Rust fixed): IOC/FOK rejected; TP/SL trigger type honored;
  zero-size partial TP/SL rejected; amend checks order-detail (endpoint
  unexercised live); per-row cancel errors; empty snapshot commits; income
  filters; boolean reduceOnly.

### Cross-venue patterns from the port reviews
- ~~OKX/BloFin startup has no retry and needs a shared policy~~. #1356
  (`d65119bea35`) supplies row-local public discovery and bounded per-read
  backoff; #1516 (`1f93bf93358`) preserves retry-after bounds. #1615
  (`c63acfc19c2`) adds shared degradation/recovery reporting for OKX, BloFin
  and Bitget on that schedule; see [the policy](RUST_STARTUP_RETRY.md).
  This does not authorize accepting malformed private financial envelopes.
- ~~Broker attribution: BloFin drops `brokerId`; check every venue's broker/referral
  tag against TS.~~ All registered names audited; payload/partner/header fixes
  and preserved gates are tabled in [the attribution audit](RUST_BROKER_ATTRIBUTION.md).
- ~~Cancel of ids not in the store silently succeeds (Binance, BloFin).~~ Eight
  families now error; KuCoin sends uncached IDs. [Every-family audit](RUST_UNKNOWN_CANCEL.md).

### Bitget family

- High: ~~hedge-mode isolated leverage always goes to the long leg — the adapter
  drops `side` (`rust.exchange.ts:822`), `SetLeverage` has no side
  (`engine/src/command.rs:85`), Bitget hard-codes `long` (`trading.rs:548`).
  Affects every venue with `hasIndependentHedgedLeverageInIsolatedMargin`.~~
  Fixed by side-aware commands and selected-leg settings for the five supported
  families; BreakoutProp has no implemented TS leverage setter.
- Medium: ~~Classic fills/order history fail outright on liquidation/ADL/reduce
  fills (`history.rs:984,867`).~~ Forced-close suffixes and absent profit now map.
  ~~One bad market row or transient error halts startup without retry~~
  (#1356, `d65119bea35`; catalogue retry and independent private-start tests).
  #1615 (`c63acfc19c2`) exposes prolonged public startup degradation/recovery.
  ~~One unmappable REST row prevents valid-peer position/order refresh~~
  (#1459, `f903d7a9436`): identity-preserving coverage retains rejected cached
  rows and protective children while peers commit; unidentified rows protect
  absence removal. Mixed-row and missing-identity regressions cover the boundary.
- Low: ~~zero-lot order rejects the whole batch; commands before the account probe
  rejected; sized stop silently becomes full-position; WS-triggered Classic
  refresh dropped while another is in flight; `"0"` leverage limits make `clamp`
  panic (`trading.rs:547`).~~ Fixed with row filtering, bounded probe waiting,
  warning, rescheduling and safe limits. UTA strategy ids look virtual (unchanged).
  ~~UTA one-way fills report side `both`.~~ Fill history retains the execution leg.
- Confirm (TS bugs Rust fixed): Classic limit move dropping attached TP/SL; UTA
  attached TP/SL edit cancelling the parent; hedge plan side flip; close orders
  side inversion (verify live); USDC commission asset.

### Engine and TS bridge

Reverified at `687fe0c1e08`: the command, account-state and wire defects below
are repaired. Keep cache mutation and caller-owned lifecycle distinct from
native authoritative state; neither observation alone demonstrates a defect.

- High: ~~cancel sends ids only and venues drop ids not yet in their store, so a
  chaser stop during placement leaves the order live (`rust.exchange.ts:789`,
  `bybit/src/trading.rs:422`)~~ (unknown ids fail explicitly, RUST_UNKNOWN_CANCEL;
  a cancel or amend naming an acknowledged placement waits in the engine until
  the store has it, `engine/src/placement_acks.rs`); ~~`setLeverage` drops side (see Bitget)~~; ~~a wasm
  panic freezes the account silently — no containment in `host.mjs:131,175`~~
  (a trap disposes the host, settles pending commands with unknown write
  outcomes and rejects with `ENGINE_DEAD`, `kind: 'internal'`);
  ~~`fetchMarkets`/`fetchTickers` on an unstarted instance return `[]` and
  `PublicExchangeFactory` caches it (`rust.exchange.ts:856`)~~ (reads start the
  engine and wait for native data, #1375; the factory throws on an empty list;
  the engine store never applies an empty catalogue and warns instead); book/candle
  ~~subscriptions made before the engine starts are lost (`rust.exchange.ts:1000,1022`)~~
  (demand is retained and replayed once markets load; `startupDemand.wasm.test.ts`).
- Medium: ~~`fetchLeverageForSymbol` returns market max for mexc, bingx, weex
  (`rust.exchange.ts:945`) and frame mode strips `fetchedLeverage`~~ (BingX uses
  fresh reads plus selected metadata refresh via `onReadAndMetadata`; WEEX keeps
  fresh `onRead` rails; MEXC reads a symbol its metadata refresh
  has not covered yet); ~~command rejections don't emit the error toast~~
  (#1375 `b6c4f006a69`, #1518 `8b115cefd73`: one-error command boundary,
  `rust.command-errors.test.ts`); ~~`privateOnly`/`externalPublicData` ignored~~
  (publicFeed policy; in-process source served, #1441);
  ~~`getAccount` missing for ~18 venues~~ (#1430 `3002a226469`: every registered
  alias has native identity support or an explicit unsupported capability,
  `rust.account-identity.test.ts`); ~~dispose mid-command reports a placed order
  as failed~~ (#1375/#1518: interrupted writes retain unknown outcome and native
  identity receipts); ~~base `cancelAllOrders`/`nuke` read the lagged TS store~~
  (#1392 `48aeec37837`: native state capture, `rust.account-state.test.ts`);
  ~~fills lack realizedPnl/fee~~ (#1430: optional financial tails preserve zero,
  `rust.fill-financials.test.ts`); ~~`fetchOrders`/`fetchPositions` don't reach
  the engine~~ (#1392: native capture through the sole applier). `removeOrders`
  remains the void cache-mutation API; an authoritative native delete could
  hide a live order. `validateAccount` cleans its listeners/deadline and leaves
  the started instance with its caller, whose lifecycle owns disposal. No
  mirrored production caller establishes a defect in either remaining contract.
- Low: ~~Asterdex unsupported interval mapped to 1h breaks candle matching~~
  (typed `Interval` refuses it);
  ~~`num::adjust` with step ≤ 0~~ (a market without two positive finite steps is
  a malformed catalogue row on every venue; the guard stays as defence in depth);
  ~~no command deadline, `execute` drops the batch tail
  on an unknown action~~ (wasm host: 120 s deadline with unknown write outcome,
  `hostcommand.test.mjs`; batch tail kept, "unknown actions preserve batch tail
  completions"; the undeployed native host has no command deadline); candles omit optional `lastSide` (no mirrored runtime consumer found;
  do not infer trade direction from OHLC); ~~orders lack generic `triggerType`~~
  (#1430 `3002a226469`, decoded-order regression in
  `rust.fill-financials.test.ts`).

### Kraken
Futures matches TS on nearly every input; no high regression.
- ~~Spot cumulative-only trades with zero/negative prices commit displayed
  progress without a Fill, losing same-quantity corrections; an invalid final
  trade can also poison native time, terminal status and order provenance.~~
  Existing `engine::execution::Cursor::prepare` validates direct/last evidence
  before any row/fence/dedup effects. Four original-source assertion-RED controls
  cover zero and negative prices independently, retained final-row state and
  same-ID correction within a batch; decoded correction delivery is .5@100.
  Healthy peers and accepted-row ordering remain independent. Zero/no-growth,
  snapshots and separate lifecycle observations retain native policy; no
  independent delivery ledger or recovery after a valid terminal lifecycle is
  promised. Full native engine/Kraken/Binance verification passes (844 tests).
  [Scope and remaining cumulative fan-out](RUST_CUMULATIVE_FILLS.md).
- ~~Medium: isolated positions flip to cross when a WS row omits
  `maxFixedLeverage` (`kraken/src/account.rs:1056,1132`; confirm live);
  `more_candles: true` fails the whole candle fetch, empty chart (`lib.rs:727`);
  spot `EGeneral:Permission denied` on an order deactivates the account
  (`spot/account.rs:513`).~~ Fixed: a stream row without the field keeps the
  REST-proven margin mode and leverage (as TS's cached preference does); a capped
  candle page (Kraken serves 2000 bars from `from`) pages forward to `to`, bounded; permission denied on a spot write fails only that
  command, while on a read it still deactivates.
- ~~Decided: market entry with attached TP/SL is rejected (Rust behaviour);
  the error must say the TP/SL legs are unsupported on market entry.~~ Done: the
  rejection names the unsupported take-profit/stop-loss legs before any write.
- Low: ~~income over 500 rows errors (TS partial)~~ (known limit: the newest
  page returns with a `history.incomplete` warning, as TS; `account-log` honours
  only `since`, so a complete walk awaits the cursor probe in
  [RUST_LIVE_GATE.md](RUST_LIVE_GATE.md)); ~~one bad row rejects
  catalogue/tickers or open orders~~ (public startup isolates rows; #1524,
  `b69285f9909`, retains rejected cached orders while REST/WS peers commit,
  covered by both mixed-order regressions); missing
  funding field set to 0 (decided: Kraken omits a zero rate; b778fa3); ~~raw error
  text~~ (non-2xx failures carry the body's error code); spot 3m candles
  (decided: typed `Interval` refuses rather than serving 1m bars as 3m).
- Confirm (TS bugs Rust fixed): index trigger signal; full_fill not canceled;
  live % change/volume/OI; stop amend racing a fill; spot fills over-reported
  and mislabelled XBT/BTC.

### Orderly and TxFlow (already default to Rust)
TxFlow loses nothing (master TS was a placeholder).
- ~~High (Orderly WS): one malformed position/execution row starves later healthy
  rows; terminal algo effects can cancel cached siblings before a bad child is
  rejected; an invalid execution price advances the cumulative watermark and
  loses a later corrected fill.~~ Complete raw-container validation and native
  pure preparation now precede shared engine RowAdmission and all terminal/fill
  commits. Known rejected rows retain state, unknown identity never grants
  absence, healthy neighbors progress and each bad native row reports once.
  Regressions cover root-only symbol and inherited-parent binding, sparse/root
  controls, sequence fences, cumulative correction and structural inertness.
  This introduces no caller API change. Shared cumulative execution preparation
  across eleven public-Fill crates plus ApeX synthetic-history progress is
  separately assigned to the next engine cumulative-fill choke point before the
  reporting/fan-out and snapshot-token phases. ApeX cumulative reports do not
  emit public Fill. This Orderly repair does not claim the other paths are fixed.
- ~~High (Orderly, all six brokers): standalone SL/TP rejected because the app always sends `triggerPrice: price` (`orderly/src/trading.rs:362`); master TS ignored `triggerPrice`. Chart-drag stops, stop form and hotkeys all fail.~~ Matching protective prices are accepted and covered by app-shaped regressions.
- ~~Medium: cancel-all only loops symbols in the store instead of the unscoped DELETE (`rust.exchange.ts` has no Orderly override; engine supports `symbol: ''`); SL/TP blocked when a trailing stop or quantity TP_SL exists on that side (`trading.rs:254`).~~ All broker aliases cancel globally; independent trailing/quantity protections are preserved, and mixed-parent contradictions fail before cancellation. See [verification](RUST_ORDER_RACES.md).
- Low: ~~one bad market row or unknown order row fails closed~~ (stale:
  discovery skips malformed rows; order snapshot coverage retains rejected
  roots and commits valid peers); ~~one malformed margin-mode row freezes the
  defaults map~~ (accepted peers commit; rejected symbols retain old defaults,
  unidentified rejections protect the old scope, and structural failures retain
  the whole map); ~~trailing-stop display price~~ (maps native
  extreme/callback value or rate using order side, for REST and WS);
  ~~cancels skipped for unknown/finished ids~~ (unknown IDs reject explicitly;
  finished IDs are an intentional no-op. **RISKY** to infer normal/algo endpoint,
  symbol or sibling repair from an uncached ID; keep reconciled-order preflight);
  ~~candle `from`/`to` filtering a latest-N fetch~~ (bounded reads use native
  `/v1/tv/kline_history`, validate parallel arrays and filter exact bounds).

### KuCoin
- High: ~~cancel-symbol also deletes the symbol's TP/SL stops
  (`kucoin/src/trading.rs:515`; TS only `/api/v3/orders`)~~ (stop-book deletion
  now only on account-wide cancel); ~~one failed or
  malformed markets/tickers read stops the venue~~ (#1356, `d65119bea35`:
  bounded discovery retry, mixed-row admission and independent private start), ~~and futures
  private REST waits for the private socket `welcome` (`lib.rs:767,954,1026`)~~
  (reconciliation starts at launch).
- Medium: ~~one unknown id rejects the whole cancel batch (`trading.rs:505,1034`)~~
  (uncached IDs sent directly; later cancellations continue and failures surface);
  ~~one malformed REST row freezes reconciliation (`account.rs:652`)~~ (skipped
  with the store's copy kept; still counted in the page census; verification fixed
  attached `__stop_loss`/`__take_profit` legs and positions being dropped behind it);
  ~~caller client id on attached TP/SL rejected (`trading.rs:120`)~~ (no change:
  the client-ID contract honours caller IDs only on plain Limit/Market).
- Low: leverage sent as `10.0` (RISKY, `num::whole`); ~~fill without `matchPrice`
  dropped~~ (fills at the order price, as TS); spot polls 5 s vs 500 ms (kept:
  10× the REST load needs live rate-limit evidence); ~~per-symbol cancel-all~~
  (stale: symbol cancels use `?symbol=` and cancel-all is one native call, as
  TS); ~~partial balance rows ignored~~ (missing `orderMargin`/`unrealisedPNL` read
  as zero; equity and free stay required).
- Confirm (TS bugs Rust fixed): stop side read-back, amend overfill, amend
  keeping trigger type and post-only, standalone stop client id, open-order
  paging, spot partial snapshot, swallowed cancel errors.

### Cross-venue: app stop payloads
The app sends stops with `triggerPrice` equal to `price`. Orderly rejected every
such stop. Many venues test `trigger_price.is_some()` in placement (apexomni,
backpack, binance, bingx, bitget, bitrue, blofin, coinbase, breakoutprop, bybit,
…); each must accept the app's exact payloads.

### Phemex

- High: ~~`SetPositionMode` has no symbol (`engine/src/command.rs:95`), so Phemex
  loops PUTs over every market and can half-switch (`phemex/src/trading.rs:858`).~~
  Phemex now requires one market and sends one PUT; WEEX preserves optional scope;
  ~~one unknown order status/type drops the whole private frame incl. positions
  and balances (`account_wire.rs:298`, `account.rs:601`)~~ (that row is skipped,
  its executions still emit, the store keeps its copy; verification: documented
  `Deactivated`/`Expired` rows now cancel instead of leaving the order Open).
- Medium: ~~one bad public row stops startup~~ (stale: malformed product/ticker
  rows are skipped while valid peers publish); ~~one bad position row freezes a
  currency slice (`account.rs:286`)~~ (REST and stream rows skip, keeping the
  store copy; the slice balance still commits); no fallback read when a symbol's mode or
  leverage context is uncached (`trading.rs:229,713`) — re-verified as low
  impact: `accountPositions` (size-0 rows included) reconciles every 2 s, so the
  gap is the first cycle; TS also refuses a symbol that read omits.
- Low: cancel batch refused on one uncached id (intentional preflight: native
  cancel needs symbol and position side; **RISKY** to infer routing or silently
  change batch execution); ~~index triggers refused~~ (documented ByIndexPrice
  is encoded for standalone and attached SL/TP, and preserved on readback);
  ~~daily candle `to`~~ (Rust correctly honors the native cutoff; TS shifts to
  the previous day and drops valid bars, now pinned by regression). No extra
  1 s REST resync after terminal events is an accepted lifecycle difference:
  WS publishes positions/balance immediately, local trade ACKs refresh REST,
  and external terminal-only events use the policy-controlled 2 s poll. Extra
  debounce reads would increase REST load and bypass slow/paused polling.
- Confirm (TS bugs Rust fixed): filled from cumQty with exec dedupe; WS-closed
  positions removed; amend re-places remainder; hedged margin per side; split
  stops at first failure.

### MEXC
- High: ~~live tickers only for the first 100 markets and `SetTickerSymbols`
  ignored (`mexc/src/lib.rs:416`), so positions outside them keep startup prices
  forever; one open order on an unloaded market blocks the order store for good
  (`account.rs:618,850`).~~ Sparse all-contract prices plus explicit detailed
  interest keep selected and non-default symbols live; identifiable unmappable
  rows are skipped/logged while malformed identities retain prior snapshots.
- Medium: ~~full-position TP/SL `vol` not snapped to step (`trading.rs:342`); every
  single order uses the batch path paced 1.5 s (`trading.rs:451`)~~;
  startup halts on ~~tickers/markets/position-mode/plan-list~~ failures
  (tickers/markets retry through shared discovery backoff since #1342). Reverified: mode and plan-list failures
  retry every 2 s and a failed plan read publishes no normal-only list; a refused
  key proven by a parsed native credential refusal fails startup instead of
  retrying (#1494, `5edb5f52dc7`; #1487, `1fb3c64d948`). Bare HTTP 401/403
  can be a gateway/WAF response and is not credential-rejection evidence.
- Low: ~~every plan order marked reduce-only~~ (explicit native flag or closing
  side; dedicated position stops stay reduce-only); ~~`fetchOrderBook` refused
  by the adapter~~ (independent public depth snapshot with captured contract
  scaling, cumulative base totals and exact version string).
  **RISKY — retained:** signed fill fee. Taking absolute value without verified
  debit/rebate conventions can turn rebates into charges; keep native sign.

### Cross-venue: rows and subscriptions
- ~~Blanket whole-snapshot starvation in Bitget REST, Phemex, MEXC, KuCoin,
  Kraken and Orderly~~: current keyed retention/coverage regressions and landed
  row repairs (#1459 `f903d7a9436`, #1493 `67a65e90e23`, #1517 `5c7913062ed`,
  #1524 `b69285f9909`, #1606 `363139a6cc7`) admit valid peers while retaining
  rejected cached identities. ~~Ordinary Bybit REST and structurally malformed
  WS order/position rows can starve valid peers~~: bounded parent/symbol coverage
  retains rejected exposure/protection/Greeks while healthy peers commit; unknown
  identity holds absence, and ambiguous mode changes retain a coherent symbol.
  Rejected WS identities protect pending REST joins without globally starving
  unrelated rows, with coalesced read repair. Regression coverage includes both
  structural/numeric failures, missing IDs, duplicate keys, option update/close,
  mode coherence and repeated WS rejects during REST walks.
  ~~Bitget structural WS order/position array rejection~~: valid Classic/UTA
  peers advance with identity/scope retention and coalesced read repair. Native
  rejection observation times remain separate from financial timestamps; earlier
  clean snapshots still fence older reads, while fresh REST-only peers can heal.
  Partial/mixed account modes cannot corrupt global routing or manufacture initial
  readiness. Plans/trailing metadata retain native cancel routes through the
  existing revision/terminal fence. Regressions cover each boundary.
  Preserve actual revision/mode fences, strict financial execution coverage and
  Hyro's atomic handshake.
- ~~MEXC ignores `SetTickerSymbols`~~ (repaired; dynamic public-demand tests).
  Other no-op arms are not proof of missing data: aggregate/all-market feeds
  already satisfy subsets. Require a demonstrated missing requested feed.
- ~~History completeness: fourteen query-associated ownership omissions in
  Bitunix, BloFin, Bybit, Hyperliquid, OKX, Paradex and WEEX left partial history
  unflagged and worker owners unwarned.~~ #1627–#1633 attach the actual request
  before completion, preserving Warn semantics and #1593's client isolation.
  Bitunix's #1633 first/later failure rejection replaced silent truncated success
  (actual merge receipts in [the sweep](RUST_PORT_WIRE_SWEEP.md)); the owned-history
  adoption below supersedes its later-page policy.
  The owned-history core now binds BloFin closed-position completion and cursor
  paging in `engine::history`; its venue no longer emits this marker directly.
  Bybit and OKX closed-position reads now share its staged/accepted cursor owner,
  including per-window reset and seeded bounds. Bybit general fills/orders/income/
  public trades now share accepted-only contextual finalization and strict policy
  rejection: first failures reject typed, later actual-page failures recover only
  accepted rows with an owned marker; missing result/list never proves empty
  complete coverage. Binance general histories now use shared accepted-page
  storage too, with engine-owned range selection/completeness for income. Whole
  array parse failures cannot leak prefixes or empty complete results; strict
  signer refusal/timeout keeps typed rejection even after accepted rows.
  OKX general history and other query-marker sites still need
  fan-out and enforcement; these repairs do not claim a universal choke point.
  BloFin general histories now share this owner too: actual later-page failures
  retain only accepted rows with command-owned incompleteness; native paging
  and header-preparation refusals still reject strictly. Successful cursor/row
  mapping and final limits remain.
  Bitunix fills/orders/income now reuse CursorHistory too: first actual-page
  failures reject typed; later failures retain accepted pages with one owned
  marker, excluding failed-page prefixes. Signing preparation remains strict
  notSent even after accepted rows. Native time/offset paging, raw progress,
  mapping, 50/100 page caps and final limits remain; no engine API extension.
  Six background account-coverage markers now use the shared BackgroundCoverage
  reporter (#1735), with account/background scope and no command. They never
  fabricate request ownership or flag an unrelated query.

### BingX
Normal inputs send the same requests as TS; the risk is resilience.
- High: ~~a zero/negative selected execution price emits an invalid Fill, consumes
  the native trade ID and cumulative progress, suppresses corrected evidence,
  and can close/advance a stored partial order.~~ Four original-source native
  assertion regressions reproduce these real financial defects. Shared engine
  `ObservedProgress::prepare_capped` validates the chosen capped suffix before
  revisions, dedup, cumulative/terminal commits or Fill delivery. The BingX
  adapter preserves native price fallback, explicit/heuristic identity rules,
  blind-gap coverage and quiet zero-growth identity bookkeeping. Existing
  authoritative Last semantics are unchanged. Remaining cumulative consumers
  still need fan-out; [scope/evidence](RUST_CUMULATIVE_FILLS.md).
- High: ~~one bad market/ticker row or failed funding read keeps the private side
  from ever starting (`bingx/src/lib.rs:529,639`); one bad position/order row
  freezes that list (`account.rs:491,557`).~~ Bad rows skipped (placeholder
  tickers, stored copies kept); funding retried separately.
- Medium: ~~`TRIGGER_MARKET` stops (attached brackets) can't be moved
  (`trading.rs:842`); no one-shot resend on signature error 100001;~~ multi-order
  cancel sends nothing if any id is unknown (`trading.rs:780`) — intended per
  [the unknown-cancel audit](RUST_UNKNOWN_CANCEL.md) (error before any write).
- Low: ~~hedge leverage sets both sides~~ (requires a side and writes only that
  native rail); ~~per-symbol leverage/margin not read~~ (explicit metadata reads
  stage current metadata atomically; #1604 (`0d4f17a1bdd`)
  fences stale publication while returning newer cached rails with independent
  validated-read fallback, including pending writes); ~~bracket `workingType`~~
  (per-leg trigger precedence already correct); ~~WS message with one bad field
  dropped~~ for ACCOUNT_UPDATE positions (row isolation, coded warning and REST
  repair). Orders/fills/config retain strict decoding when identity is uncertain.
  **RISKY — retained:** `cancelRestrictions=ONLY_NEW` guards cancel-replace
  against partial-fill races (quantity-only amend has no such restriction);
  trigger moves require a known source rather than guessing; malformed financial
  history rejects instead of silently returning a plausible incomplete ledger.

### WEEX

Owned-list follow-on: ~~a later fill-window/algo failure discards already valid
history~~ now returns accepted rows with shared command-owned incompleteness.
Missing fill identities and explicit algo hasMore facts are admitted only after
whole-response validation; the engine publishes coverage once before success.
First/zero-accepted failure rejects typed, preparation remains strict notSent,
and native mapping, sequencing, pause and dedupe are preserved. This completes
WEEX general-history owner adoption, not all-family or B11/deployment closure.

- ~~WEEX's five query-associated omissions~~ now flow through the shared income
  owner. First/zero-accepted failures reject typed without a coverage marker;
  later failures immediately retain only accepted pages with one command-owned
  marker before success. Valid opaque-cursor gaps and lifetime/chunk budgets
  retain the first coverage cause until final success. The old failed-chunk
  continuation policy is superseded. Native-backed host/worker fixture proof now
  uses accepted-page/later-failure success; fills/orders remain a later slice.

- High: ~~one `null` numeric field in a public row stops the venue, private start
  gated on tickers (`weex/src/wire.rs:73`, `lib.rs:671,727`); one unparsable
  order/position row freezes the list, no `planType` fallback (`account.rs:335`).~~
  Bad public and account rows are skipped (stored copies kept, exact hedge ids);
  ticker failure no longer blocks private start; `planType` fallback added.
- Medium: ~~failed config read backs off all polling to ~60 s (`account.rs:518`);~~
  ~~balance stops updating during active trading (revision discard,
  `account.rs:408`)~~ (balance reads commit; the next cycle corrects a raced read); ~~accepted conditional reported failed if
  the venue echoes `clientAlgoId` (`trading.rs:1303`; verify live); imported limit
  with TP/SL can't be amended (`trading.rs:343`); missing passphrase looks like a
  valid empty account (`lib.rs:148`).~~ Settings-only backoff, algo id echoes
  accepted, imported TP/SL amend keeps row working types, partial credentials
  deactivate visibly.
- Low: ~~isolated leverage sets both sides~~ (selected isolated rail changes;
  required opposite payload rail is preserved, including flat symbols; cross
  leverage is coupled); ~~fetchLeverage returns max~~ (bridge returns cached
  native buy/sell rails, not market limits).
  **RISKY — retained:** index-source placement needs a verified native wire
  contract (TS substitutes mark price for IndexPrice, which changes semantics).
  Fills accept `id`/`tradeId`/`fillId`; an absent identity field rejects the query.
  ~~An empty identity silently drops a fill and reports complete history.~~
  Empty identities remain skipped without invention, but each affected query now
  emits one coded Warn `row.skipped` naming `id` and an owned `history.incomplete`
  before completion. Valid peers and financial values remain intact; the warning
  budget spans windows and resets per query. Native alias/window/isolation and
  Error-only tests plus a native-verified host/worker fixture cover this boundary.
  Income with neither
  `asset` nor its `coin` alias rejects the affected page (range walks continue
  with an explicit incomplete marker); assuming USDT without accounting evidence
  can mislabel value.

### Coinbase (already defaults to Rust)
- Owned-history follow-up: Advanced Trade fills/orders/public trades now share
  the engine CursorHistory owner, including CFM-to-Spot reads. Later actual
  page failures retain only admitted pages with command-owned incomplete coverage;
  zero-admitted failures reject typed, while native continuation/proof and
  queued signing refusals remain strict. Native mapping, requests and account
  state are unchanged; Starbase and NX B11 coverage are not closed. See
  [RUST_OWNED_HISTORY.md](RUST_OWNED_HISTORY.md).
- High: ~~one unmappable open order (e.g. a bracket or TWAP placed on Coinbase's
  site) rejects every snapshot and blocks all trading incl. cancel
  (`coinbase/src/account.rs:336,1577,1842`, `trading.rs:1028`); a failed
  placement leaves a reservation that breaks cancel-symbol for the session
  (`trading.rs:1109,1283,1164`); CLI market orders with GTC rejected
  (`trading.rs:264`, `cli-core/.../BaseBuySellCommand.ts:368`)~~ (rows skip with
  store copies and stay cancellable; 4xx releases, ambiguous outcomes release
  after 30 s plus later order/position reads; GTC market sends IOC); browser-side
  `signWsAuth`/HTTP signing lost (decision 2).
- Medium: ~~reduce-only capacity counts protective stops against TPs and spot
  reduce-only always rejected (`trading.rs:654,676`); portfolio probe stalls on
  404/other non-2xx (`account.rs:1322,1687`); one bad CFM position row rejects
  the snapshot (`account.rs:1246`)~~ (stops and other closes reserve separately;
  spot reduce-only is plain; probe advances unless 408/429/5xx; bad position
  rows skip keeping the store copy, flat side-less rows clear).
- Low: ~~spot `total` source differs~~ (native `total_balance_crypto` includes
  held funds; TS incorrectly used available-to-trade as total); ~~OHLCV honours
  `from`~~ (correct: explicit caller lower bounds reach the native start field;
  TS's defaulted limit made its `from` branch unreachable); ~~placement errors
  lose Coinbase's message~~.
  **RISKY — retained:** CFM summary `upnl`/equity aggregation. Native uses the
  summary unrealized-PnL field while TS returns zero. Changing that value needs
  verified CFM/spot portfolio accounting and downstream equity-consumer evidence;
  neither suppressing real PnL nor silently double-counting it is acceptable.

### Lighter

Signing, integer scaling, tx encoding and nonces match TS.

Owned-history follow-up: ~~later page failure discards validated earlier history~~
now retains accepted pages with one command-owned incomplete marker through the
shared engine owner, including trade-to-funding income. Failed prefixes remain
excluded; first/zero-accepted failures reject typed. Native cursor/saturation
proof and synchronous auth preparation stay strict; account reconciliation,
signing arithmetic and the remaining cross-family history fan-out are unchanged.


- High: ~~position `initial_margin_fraction` ("20.00" = 5x) read in basis points
  (`lighter/src/account.rs:424,877`), so leverage reads as the market max and a
  margin-mode change signs max leverage (`trading.rs:567`).~~ Position-only
  ratio/percentage/tick normalization now has REST/WS and 5x signing regressions.
- Medium: ~~windowed fill history pages the whole account history then fails
  (`history.rs:404,744`)~~ (newest-first paging stops past `from`); ~~one bad
  market/account row stalls startup or sync~~ (#1356, `d65119bea35`; #1459,
  `f903d7a9436`; rejected stored copies survive while public/private peers commit);
  ~~account tier not exposed and `changeAccountTier` missing~~ (shared
  `Command::Venue` action; Lighter implements fetch/set tier).
- Low: ~~thinner error mapping (rate-limit guidance, 21613 refresh); reduce-only
  re-check races a fixed 2 s timer~~. Documented HTTP405/429 failures include
  neutral guidance and retain native diagnostics/outcome; explicit 21613
  placement rejection refreshes account plus orders without mode guessing or
  resubmission. Missing reduce-only exposure waits for a matching account-read
  completion (10 s deadline), including target size/absence evidence. Timed-out
  reads cannot release later queued orders. ~~Shared with TS: amend re-places the full
  original size after a partial fill.~~ Rust now re-places the observed remaining
  size for price-only amendments; explicit replacement quantities are preserved.

### WOO
- High: ~~zero/negative last quantity consumes delivered cumulative progress
  before a valid same-ID correction.~~ Shared observed-suffix preparation
  retains correction eligibility, while valid display observations remain
  separate; two native original-RED/core-GREEN controls cover the lost fill.
  A third prevents last-only unpriced/corrected receipts from doubling display
  quantity; inferred observation now requires the valid financial receipt.
- Medium: ~~algo child rows don't inherit the parent's side/status/reduceOnly, so
  fills can be lost (`woo/src/account.rs:700`; verify one live frame); one bad
  position row freezes positions (`account.rs:749,949`); one unmappable
  fill/order-history/instrument/ticker row fails the read or startup.~~
  Children inherit missing parent fields (snapshot and stream); bad rows are
  skipped keeping stored copies; hidden exposure on unknown markets warns once.
- Low: ~~SL trigger falls back to `triggerType`~~ (standalone precedence matches
  TS); ~~algo reduceOnly default~~ (native default is false; children inherit
  parent fields); ~~leverage readback can report a false error~~ (exhausted reads
  now explain accepted write/unconfirmed setting, retain Unknown outcome and
  never replay the write); ~~isolated warning~~ (one coded warning per symbol,
  exposure remains visible); ~~stop without reference price rejects the snapshot~~
  (row isolation and re-read after ticker seed already preserve valid peers).
  **RISKY — retained:** changing attached-bracket generic trigger fallback
  without verified per-leg semantics, or guessing SL/TP for a STOP without
  a reliable reference. Readback confirmation remains required.

### AsterDEX
No open confirmed highs.
- High: ~~negative cumulative execution observations consumed bounded ledger
  slots as zero-valued entries, evicting a valid fill anchor and allowing its
  identical replay to deliver the fill again after10,000 distinct malformed
  observations.~~ Existing engine `ObservedProgress` now validates and caps
  before observation admission; typed native commit keeps the10,000-entry
  bound and accepted quiet observations. One original native assertion RED
  proves the duplicate `(102,0.5)`; ten original compatibility controls and
  all11 fixed controls pass, including initial REST seed, later-read isolation
  and authoritative direct-ID independence. No new engine policy or raw-ID change.
- Medium: ~~failed position-mode read silently leaves one-way, never retried, and
  orders can go before it lands (`asterdex/src/account.rs:566`); order events
  with unmapped types (e.g. `LIQUIDATION`) dropped with their fills
  (`account.rs:817`); one malformed row halts startup (`lib.rs:849,781`); one bad
  row discards open-orders/positions snapshots (`account.rs:402,415`); hedge
  amend of a closing order may open the opposite side (shared with TS; reuse the
  row's `positionSide`).~~ Mode read retries and holds placement until known;
  unmapped order types keep their fills; bad rows are skipped keeping stored
  copies; hedge amend reuses the recorded `positionSide` (TS bug fixed).
- Low: ~~no credential check at start~~ (missing local credentials intentionally
  select public mode; formed credentials are checked by signed account startup);
  ~~ACCOUNT_UPDATE strict on `ep`/`cr`~~ (`cr` already defaults to zero; absent
  `ep` is now accepted only with an explicitly valid zero amount);
  ~~fills refuse >7 day windows~~ (bounded symbol reads page seven-day windows,
  deduplicate execution IDs and normalize cumulative PnL once across the range);
  ~~listen-key keepalive omits `listenKey` and tears down on failure~~ (official
  V1/V3 PUT has no business parameters; three transient retries precede an
  epoch-guarded reconnect). Equal-time saturated fills fail visibly.
  RISKY retained: skipping malformed financial fills can shift every later
  cumulative-PnL delta; partial balance rows lack required wallet/free provenance
  in TS too. Do not invent zero balances or silently return partial execution data.

### Crypto.com
Mostly matches TS.
- Medium: ~~per-coin balances (`position_balances`) dropped
  (`cryptocom/src/account.rs:232`)~~ — published with the aggregate (REST and
  `user.balance`). ~~Caller client id refused
  on stop/TP orders (`trading.rs:76`).~~ Matches TS and the client-order-id
  contract (`rust.exchange.ts` rejects it before the engine); a change is a
  contract decision, not a port bug.
- Low: ~~one invalid order fails the whole batch; null string fields fail reads;
  one bad instrument row stops startup.~~ Bad rows in reads and pushed frames
  are skipped with one summary warning, keeping stored copies; startup stops
  visibly only when no row maps. Placement still validates the batch before writes.

### Bitrue
- High: ~~a finite average whose cumulative product overflows overwrites a
  trusted REST quote anchor and loses a later valid execution.~~ Shared checked
  quote preparation keeps the trusted anchor; native original-RED/core-GREEN
  control proves the10@130 suffix, with interval dedup/floor/cap preserved.
  A second control repairs missing delivered quote evidence from a matching
  correction and proves the next10@120 suffix, without duplicate delivery.
- High: ~~orders refused on symbols with no account leverage data, and a fresh
  account can never trade (`bitrue/src/trading.rs:363`, `account.rs:948`);
  default margin to Cross and take leverage from a successful change, never
  the market max. A cancel batch stops at its first failure and leaves the rest
  live (`trading.rs:866,884`).~~ Confirmed explicit leverage survives flat reads;
  unknown margin defaults Cross. Cancels continue and report per-ID failures,
  including child fallback after a missing or failed parent.
- Medium: ~~orders marked loaded only after every market's open-orders read
  succeeds, one bad symbol blocks forever (`account.rs:382,487`); per-coin
  balances dropped; max leverage stuck at 100 with silent clamping (no
  leverage bracket read, `lib.rs:758`).~~ Symbol failures retain state and retry
  without blocking the usable initial sweep; both collateral balances and
  authenticated native bracket caps are exposed without silent leverage clamps.
- Low: ~~one bad contract row blocks startup~~ (public discovery isolates rows);
  ~~stream stops misclassified~~ (private conditionOrder/triggerType now precede
  execution MARKET/LIMIT, retaining authoritative cached protection);
  ~~snapshot starvation under load (revision discard, `account.rs:581,1310`)~~;
  ~~malformed order snapshot never sets loaded~~ (each symbol's first attempt
  finishes the initial sweep; failed symbols retain state and retry independently).
  RISKY retained: malformed account snapshots do not mark balance/positions loaded;
  only admitted authoritative financial data can establish complete account state.

- Confirm (TS bugs Rust fixed): TS prices a position at the all-zero placeholder
  ticker before the first quote (private tape: -12000 on a 0.2 BTC long whose
  real quote equals its 60000 entry); Rust keeps 0 until a real price arrives.

### Cross-venue: per-coin balances and snapshot starvation
- Per-coin balances missing on ~~Bybit~~ (#1379), ~~Crypto.com~~, ~~Bitrue~~;
  ~~check every venue~~: GRVT, MEXC and Phemex also dropped the per-currency
  balances TS publishes and now set them; ApeX and Backpack publish none in TS.
- ~~Revision-discarded snapshots starve under active trading on WEEX and Bitrue;
  check every venue that fences REST snapshots by stream revision.~~ Fixed with
  `engine::fence::StaleGuard` (commit after two consecutive discards, then re-read)
  on Bitrue, Backpack, BingX (balance, position mode), Kraken Spot, KuCoin Spot,
  Orderly (balance only), Polymarket and WOO; WEEX balance reads no longer fence.
  - ~~Bybit Hyro: a ready session's periodic snapshot is discarded on every order/position
    frame and three discards reconnect, so steady frames loop forever.~~ Fixed: overtaken
    reads re-read (a ready session commits the unfenced wallet meanwhile); the third is
    admitted with a warning instead of reconnecting, in initial recovery too, and the
    next tick reconciles again. Admission uses `engine::fence::StaleGuard`.

### ApeX Omni
Signing and payloads match; no confirmed highs.
- Medium: ~~cancelled ids hidden for good (terminal map, `apexomni/src/account.rs:150`;
  cancel-all marks untriggered stops terminal, `trading.rs:928`); one bad
  open-order row leaves the account side dead incl. signing state
  (`account.rs:975`); null fields drop whole private frames incl. fills
  (`private_wire.rs:205`); one odd `/v3/symbols` row blocks private start; a bad
  omni key seed kills read-only monitoring (`account.rs:504`).~~ Cancels fence by
  version with a 5 s hide (kept orders reappear with a warning); bad rows are
  skipped per row keeping stored copies and fills; a bad seed disables only placement.
- Low: ~~float remaining size~~ (operand-precision subtraction for mapped
  orders and default-size amendments after late fills); ~~zero-price candle
  rejects batch~~ (coded row skip retains sorted valid peers; malformed time,
  envelopes and nonempty unusable results still fail). RISKY retained: caller
  IDs on stops/brackets/splits violate the current one-native-order identity
  contract without per-leg proofs; skipping explicit leverage writes from cached
  equality can silently leave venue settings wrong. Native private socket routing
  is host-owned via SocketChannel::Private/proxyConditional (static concern stale);
  a live proxy lifecycle remains unverified.

- Confirm (TS bugs Rust fixed): TS values positions at an all-zero placeholder
  ticker on startup, showing a fabricated loss (private tape: balance 1300 / upnl
  -300 while the real quote equals the 60000 entry and native PnL is 0) until its
  next balance fetch; Rust uses reported PnL until a real price arrives.

### Paradex

- ~~Paradex's bounded history-cap ownership omission~~ now carries the actual
  request command before retained rows complete. Its native regression covers
  exact Warn ownership/order at Error-only logging; unbounded caps and other
  stalls still fail.

StarkNet typed data, JWT, payloads and SBE decoding match.
- High: ~~one blank ticker field halts the venue including its private side~~
  (#1356, `d65119bea35`: nullable summary/default regressions and independent
  private startup); ~~a failed TP/SL child logs at
  Warning so no toast — user believes they are protected (`trading.rs:855`)~~
  — child failures now produce Error notices while retaining accepted entry IDs;
  ~~blank leverage/liquidation fields prevent REST positions from committing~~
  (#1356, `d65119bea35`): blank/null optional position values have explicit
  REST/WS defaults; required financial values remain strict, with regressions.
- Medium: ~~multi-order placement stops at first failure and loses placed ids
  (`trading.rs:458,699,867`)~~ — independent operations continue, verified partial
  native IDs are returned in caller order and failures identify rejected clients;
  ~~venue rejection reasons dropped (`lib.rs:231`)~~ — structured error/message
  details retained with bounded text and no response dump;
  ~~leverage serialized `20.0` (`trading.rs:542`; verify live)~~ — JSON integer,
  per the documented request schema, matching confirmed state;
  ~~one malformed SBE frame recycles the public socket (`public.rs:388`)~~ — malformed frames
  now drop without clearing books/socket or refreshing liveness; subsequent valid
  sequence gaps still recover only the affected book (native regressions).
- Low: ~~one unmappable order row discards refresh~~ (row isolation keeps its
  stored copy while valid peers commit); ~~leverage-0 config row~~ (only positive
  valid configuration leverage is admitted; bad rows preserve cached settings);
  ~~stale uPnL after WS close~~ (closing removes the leg and recomputes aggregate
  UPnL). Existing regressions cover row isolation, invalid configs and REST/WS
  closure; zero-config behavior is source-verified, not a dedicated test claim.
  RISKY retained: explicit zero TP/SL rejects before entry dispatch rather than
  silently dropping requested protection; malformed financial-history rows fail
  visibly rather than returning a plausible incomplete ledger.
  ~~README says remote signing rejected~~ — corrected to the landed payload
  signing contract exercised by the delegated-signing regressions.

### Cross-venue: silent partial failures
- Child-leg failures (TP/SL after an entry) must surface as errors on every
  venue (~~Paradex logs at Warning~~; now Error with a native regression;
  verification: a rejected TP no longer drops the queued SL, and wrong-side
  brackets reject before any write instead of placing a bare entry).
- Batches that fail midway must return placed ids with the error (~~Paradex~~,
  ~~Binance COIN-M~~, ~~Bitrue cancels~~, ~~BingX sequential placements~~ repaired;
  BingX preserves acknowledged IDs in typed and legacy partial-error details
  after a later rejection, transport loss or malformed acknowledgement).
- ~~Paradex leverage serialized as a float~~ (repaired). KuCoin's explicit
  leverage setter already serializes a string (`65c110dca78`); its optional
  placement leverage is numeric. No current endpoint evidence proves `20.0`
  is rejected. Do not round/cast fractional values or claim a blanket integer
  serialization defect without a native field contract.

### GRVT
Signing and integer scaling exact; no highs.
- Medium: ~~builder check fails once → every order rejected for the session, and
  a matching row without `mfr` treated as unknown (`grvt/src/account.rs:1531,1011`,
  `trading.rs:777`); one unknown-instrument row hides all positions or orders
  (`account.rs:516,422,970`); GRVT's rejection message dropped and misleading
  "reconcile" advice added on definitive rejects (`wire.rs:180`, `trading.rs:1348`).~~
  Builder check retried (TS budget) with orders held FIFO; rows skipped keeping
  stored copies; venue code/message surfaced, reconcile advice only when unknown.
- Low: ~~order history fails on one bad row; signer mismatch blocks trading;
  1008 loses the proxy-whitelist message~~ (bad history rows skip with a
  warning, a mismatched configured signer warns at startup as TS does, 1008 keeps
  TS's whitelist guidance); amend may cancel without replacing if cancels are
  asynchronous (verify live).

### Polymarket (development-only)
CLOB V2 signing and amounts match the official client.
- Medium: ~~one `null` in a Gamma market row breaks the venue with no retry
  (`polymarket/src/wire.rs:33`, `lib.rs:688,832`).~~ Nulls tolerated, bad rows
  skipped, market/midpoint reads retry (≤30 s).
- Low: cancel-all reports failure after a successful cancel (kept: the test
  deliberately requires proof for each cached order); ~~one bad position
  row freezes positions~~; ~~REST auth failures are invisible~~ (#1558,
  `e757d8d7990`: coded retried failures emit Error initially and after a quiet
  spell, Warn on repeats). Typed credential feedback is outside the current
  Polymarket classifier scope; user-socket refusal envelopes still need evidence.

### Registration and options (all RUST_VENUES aliases)
The current registration inventory contains 49 aliases, checked by
`rust.account-identity.test.ts`. Alias→product mapping, credentials, testnet,
routing and broker/builder attribution have been reviewed (~~BloFin `brokerId` aside~~; repaired
with its production/proxy gate).
- Medium: ~~Coinbase accounts with a saved read-only key fail to start — no
  `secondaryReadHeaders` entry or `read_credentials` (`transport/engine.ts:76,116`,
  `hosts/wasm/src/lib.rs:656`); Coinbase is Rust-only.~~ #1487 (`1fb3c64d948`)
  intentionally signs Coinbase with the primary key only; saved secondary
  credentials are ignored with a warning, without a dual-key portfolio probe.
- Low/medium: ~~polling controls ignored: `extra.tickInterval` (including
  `Infinity`), server slowdown and credential-rejection backoff~~ (#1392
  `48aeec37837`: native polling policy before startup, pause/resume and multiplier
  regressions in `rust.polling-policy.test.ts`; #1487 `1fb3c64d948` and #1611
  `a07b6b35c87`: typed credential/scope feedback and matched acceptance reset,
  `rust.polling-native-auth.test.ts`). This does not claim universal native
  classification for every arbitrary venue failure.

### Backpack
Signing, order payloads (incl. app stop shapes), amend, cancel and positions match.
- Medium: ~~spot markets dropped — 88 SPOT pairs vanish and open spot orders
  can't be seen or cancelled (`backpack/src/lib.rs:543`); decided: retire
  Backpack spot (remove any spot surface, no port).~~ No spot surface remains in
  the Rust crate. ~~A mark-price failure or one bad ticker row
  blocks startup (`lib.rs:211,787`).~~ Bad rows skipped; startup proceeds on last
  price until marks load.
- Low: ~~one bad fill row fails fill history (`history.rs:312`)~~;
  ~~open-order rows with `"quantity": null`~~ now admit a missing/null base
  quantity only when positive native triggerPrice/triggerQuantity and bounded
  execution prove its amount. Account/history/repair share that guard. Ordinary
  missing quantity now rejects instead of defaulting to zero; malformed fallback
  retains prior state and cannot start cancellation. No live-schema claim is made.

### BreakoutProp (disabled in production)
App stop payloads accepted; amend stricter than TS.
- High if re-enabled: ~~one unexpected portfolio row (XBTUSD lot, terminal-placed
  order type, multi-leg order) rejects the whole snapshot
  (`breakoutprop/src/account.rs:159,226,261`, `lib.rs:920`)~~ (such rows are
  skipped and logged; off-catalogue position margin still counts).


## Paradex owned-history fan-out

General histories and client-ID lookup now use the shared CursorHistory owner,
including the native empty-fills fallback. Actual later-page failure recovers
accepted mapped rows with owned incompleteness; a first failure rejects typed.
Lookup proof, repeated cursors, unbounded cap refusals and queued preparation
failures stay strict. Native request/codec/ledger behavior is pinned; no account
or signing implementation changes. This closes Paradex's local completion/
paging ownership copy, not all-family fan-out or B11 downstream coverage.
[Implementation and limits](RUST_OWNED_HISTORY.md).

Phemex income owned-history follow-up: ~~later page/currency errors discard all
accepted ledger rows and saturation rejects before the next currency~~. The
shared range owner now retains eligible admitted rows and one command-owned gap;
first actual page failures reject typed and failed-page prefixes stay excluded.
Shared filtered admission and checked range-stage/native-bound transitions retain
coverage before output filtering. The strict output cap and single-page Phemex
history/lookup policies remain; other venue adoption is still open.

## KuCoin Spot cumulative fallback — shared execution adoption

~~Without `matchSize`, the Spot match reducer emitted all of `filledSize` as a
new Fill, replaying the already trusted cumulative prefix.~~ Native regressions
prove the defect against actual `2cf8c4c91dd`: startup filled0.5 followed by
filledSize0.75 emitted0.75 instead of0.25. Existing engine `Cursor` now prepares
the binary suffix before native identity admission; positive unique delivery
commits its anchor. Authoritative `matchSize` remains exact and independent of
stale/terminal display fences. Raw native identity keys and optional bounds stay.

Valid raw REST prefixes seed previously unseen IDs only after current nonlookup
snapshot admission, including healthy peers of rejected rows. Seeds never come
from retained rejected/cached replacement rows, failed/stale reads or lookups.
The independent20,000-order FIFO retains anchors through terminal removal,
REST absence and reconnect; later reads do not overwrite live execution progress.
Eviction ends that bounded guarantee, even for active orders. A last fill with
no cumulative endpoint does not invent a prefix; subsequent fallback cannot
prove how that unanchored last overlaps, an explicit remaining evidence limit.
This fixes one financial defect with multiple native RED controls, not a claim
of new live captures or complete all-family fill-helper adoption. Orderly and
the shared row-reporting extension remain next.


## GRVT owned-history fan-out

~~GRVT general histories own accepted/staged output and numeric finalization
alongside a separate shared cursor walk.~~ They now adopt CursorHistory for page
admission/completion and the existing first/later actual-failure contract. A
reusable engine staged-page discard preserves admitted rows/cursor/budget across
existing retries and session refresh. Order-filtered fill totals, setup/session
refusals and lookup proofs remain strict; native identity ledgers/units/request
sequences are unchanged. Other adapters, managed completion and NX B11 coverage
remain open. [Scope](RUST_OWNED_HISTORY.md).


## Crypto.com owned-history fan-out

~~Crypto.com history output retention and numeric completion are venue-local
alongside nanosecond continuation state.~~ Existing CursorHistory now owns page
admission and command-owned finalization, reusing the shared staged-page reset.
First/zero-accepted actual failure rejects typed; later actual loss retains prior
accepted rows only. Exact cursor proof refusals stay strict. Native time bounds,
identity rollback, mixed retry counter, pacing, public duplicates and ranged
income limits remain. Other adapters and B11 runtime coverage stay open.
[Scope](RUST_OWNED_HISTORY.md).

## Orderly observed-average quote retention

~~High: a uniquely delivered whole-growth direct Fill with no cumulative average
(or average zero) stores a fabricated zero quote. A later average-priced suffix
can be mispriced: cumulative1/last1/direct100 then cumulative2/average100 emits
the second1@200 rather than1@100.~~ Original native assertions prove missing-
average, zero-average and corrected-root-child instances of this one financial
defect. Shared engine `prepare_observed_average` keeps unknown quote explicit and
lets an admitted direct Fill establish only its exact whole new interval, from
an already known prefix or the empty prefix. Inferred overflow is best-effort;
supplied average-product overflow still rejects before effects.

The Orderly adapter retains one native totals map with terminal-FIFO eviction,
native identity precedence,
observation-before-dedup, quiet forward observations, binary quantities, REST
non-seeding, sparse freshness gates and late unique delivery. Missing cumulative
still resolves to zero and never invents a prefix. Equal/regressed endpoints do
not replace quote. A blind gap cannot infer a whole historical quote; the native
arbitrary-last quote-delta quotient and its zero fallback remain deliberately
unchanged. `Report::Last` remains strict. Native/root/helper controls are evidence
for this repair, not live capture, new caller deployment or reporting closure.


## Persistent row-reporting core: first consumers

Repeat suppression and re-arm are cross-cutting engine policy. The first
reporting slice moves Coinbase, Crypto.com and WOO diagnostic memory into
`coverage::RowReports`: Coinbase preserves one non-Error-to-Error upgrade and
namespace re-arm; Crypto.com preserves separate bounded Once memories for orders
and positions, including decoded-key forget before market lookup; WOO preserves
unbounded last-message comparison for fixed report kinds. Bounded memory still
clears at4096 before even a duplicate lookup. The limit is independent of
`RowAdmission` protection. Suppression does not increment health; an admitted
report passes the existing coded boundary exactly once even if host delivery is
filtered.

This is a compatibility/architecture change, not a newly proven financial or
production bug. Original native account controls pass before migration, covering
Coinbase upgrade/re-arm, Crypto.com Warn-to-missing-copy suppression and decoded
row re-arm within a partial response, and WOO A→B→A/clean-reset summaries. New
engine controls pin repeat policies, namespace isolation/empty identities,
borrowed forget, bounded duplicate-key reset, unbounded retention and filtered
health. Existing financial reducers, admission coverage and snapshot fences are
unchanged. Grouped collectors (Crypto.com every rejection, WOO unique known IDs
plus anonymous rows), Bybit deferred reporting, and remaining persistent copies
remain open in [the reporting plan](RUST_ROW_ADMISSION.md). No all-venue closure
or new live capture/parity claim is made by this first slice.


## Grouped and deferred row-reporting engine path

Phase2 moves duplicate-inclusive counts/first labels and ordered unique-ID plus
anonymous counts into explicit engine `RowCount`/`RowKeys` collectors. Crypto.com
keeps its exact summary formatting, including anonymous versus empty first labels;
WOO keeps unique known IDs, each anonymous row, original retention/page progress
and ChangedMessage/empty-group re-arm. Bybit keeps deferred unbounded Once memory
at the accepted joined-order boundary, including admitted partial joins. Failed,
noncurrent or incomplete whole joins do not consume it; clean/mapped rows do not
re-arm it, and cached-row restoration remains outside report suppression. Both
real Bybit constructors select that explicit policy and product swaps preserve it.

Architecture gate YES: these are cross-cutting grouping/repeat policies behind
engine coverage, with native identity/formatting/admission timing retained. The
233 selected original native tests pass before migration; these are compatibility
controls, not defect RED evidence. New core controls cover exact first labels,
summary health/filtering, empty text versus absent summary and key-only re-arm
at capacity. Financial reducers, signing, clocks and snapshot fences stay native.
Seventeen bounded persistent source files remain for subsequent fan-out; no
all-venue closure, live capture or stronger replay parity claim is made here.


### Row reporting: BloFin and Kraken consumer migration (2026-10-23)

Moved BloFin position/order, Kraken futures row/unlisted-symbol and Kraken Spot
row diagnostic memories onto existing engine RowReports. Architecture gate YES:
this is consumer fan-out of cross-cutting policy, not another venue policy copy
or new engine API. The futures real constructor explicitly chooses bounded
ErrorUpgrade plus separate unbounded Once; BloFin and Spot use bounded Once.
Native messages, severities, keys, empty identities, 4096 clear-before-lookup,
re-arm sites and pre-admission reporting remain unchanged. The final 126 original
native account controls (55 BloFin,39 futures,32 Spot), including eleven new
compatibility controls, pass against unchanged actual #1775 production. Exact
source/test/log hashes are bound in
`/tmp/codex-row-blofin-kraken-original-final-receipt.json` (SHA256
`aa9137471325c9cb881aaa2cf4bf16f060e482db5ad4943038a5a37521489bc6`).
An initial Spot test expected REST text for WS execution diagnostics; only that
fixture expectation was corrected before the final original-GREEN run. No
production defect/RED, financial repair or all-venue closure claim. Other bounded
and unbounded reporter consumers remain open; see RUST_ROW_ADMISSION.md.

Migrated engine/BloFin/Kraken/Binance focused suites pass1253 native tests across
68 groups; strict all-target Clippy, scoped fmt, bans, root44 typecheck/28 lint
(root tasks cached) pass. Both independent source/caller reviews are CLEAN.
Actual #1781 `ff6870f921a79cedd108017903b49f86ce924d14` is genuinely integrated
without conflicts, preserving both incoming files. Full green CI and current-head
artifact proof remain pending.

## Kraken futures owned-history fan-out

~~Kraken futures histories retain rows, cursor sets, page counters and terminal
ledger completeness emission in the venue.~~ Fills, closed orders, ledger and
public trades now adopt the existing engine CursorHistory/Completion owner.
Actual first/zero-eligible failure rejects typed; later actual failure retains
only admitted pages with command-owned incompleteness. Native strict cursor/
continuation-proof/budget refusals remain strict; terminal order folding sees
only admitted rows. Native request/identity/units/filter/signing and ledger
coverage facts remain unchanged. Other adapters, managed completion and B11
coverage/runtime proof stay open. [Scope](RUST_OWNED_HISTORY.md).

KuCoin owned-history follow-on: futures history/funding and spot fills
adopt the existing engine query owner. Initial page failures become typed
Internal rejects; later futures/funding failures return only validated eligible
rows with command-owned incompleteness. Native continuation/census/safety proofs
remain strict. Signing, disabled spot endpoints and B11/NX coverage are outside
this slice; see RUST_OWNED_HISTORY.md and RUST_CUTOVER_CALLERS.md.

## Bitrue single-page owned fills

~~Bitrue fills keep native output storage and terminal completion beside streamed
response validation.~~ Existing engine HistoryRows/Completion now owns staging,
page admission, retry-page reset and typed terminal delivery. All native proof
checks precede admission; a saturated filtered/duplicate page remains a strict
refusal. No fictitious pagination, partial-success case or new venue marker is
introduced. Public trades, unsupported histories and client-ID lookup stay
separate; B11 caller/runtime coverage remains open. [Scope](RUST_OWNED_HISTORY.md).

## Backpack owned-history fan-out

~~Backpack retains accepted history rows and terminal paging decisions in its
venue continuation, discarding earlier pages on later failure/repeat/cap.~~
CursorHistory now owns page admission, repeat/cap coverage and completion, with
the existing offset helper subordinate to owner-authorized continuation. Failed
page prefixes remain staged; admitted malformed-fill skips supply owned coverage
without changing native row isolation. Raw request displacement, native mapping,
eager non-fill diagnostics and strict setup/admission behavior remain. This is
shared-owner adoption, not universal history or NX B11 coverage closure.

## ApeX bounded execution completion

~~ApeX fills exhaust all native/display REST alternatives into apparent complete
WS-cache or empty success, even when every REST attempt actually failed.~~
Existing engine HistoryRows/Completion now owns terminal admission and delivery;
all-actual-failure exhaustion rejects typed Internal. Validated successful empty/
nonempty paths retain native/cache selection; bounded native upsert remains an
endpoint normalization fact. No invented pager, partial recovery or cache-backed
coverage proof is added. Setup/refusal provenance and B11 runtime coverage stay
separate. [Scope and controls](RUST_OWNED_HISTORY.md).


### Row reporting: AsterDEX, GRVT and TxFlow consumers (2026-10-23)

Replaced three account-owned bounded suppression sets with existing engine
RowReports Once, including GRVT's shared coin/order/position namespaces.
Architecture gate YES: consumer fan-out through the existing cross-cutting
reporting path; no new venue policy or engine API. Exact native notice text,
severity, empty/composite identities, clear-before-lookup capacity and re-arm
ordering are preserved. Aster/TxFlow mapping still forgets a mapped ID before
full-read admission, zero quantity does not gain forgetting, and TxFlow WS has
no REST-only per-key effect. GRVT duplicate coins rearm from unfiltered totals;
position/order identities keep native rejection filters, including unknown
identity preventing absence proof. Per-row timestamp rejection still follows
current-response rearming.

Final original125 account tests, including nine new compatibility controls,
pass on unchanged actual1782 production. The source/test/log receipt is
`/tmp/codex-row-aster-grvt-txflow-original-final125-receipt.json`, SHA256
`0fa028da226ab076553c0d607657ba113482547141490dd1550817ac38fb8602`.
This is standardization without a product defect RED or financial repair.
Other persistent reporter consumers remain open; no all-venue closure or new
private parity/provenance claim. See RUST_ROW_ADMISSION.md.

Migrated focused engine/AsterDEX/GRVT/TxFlow/Binance suites pass1294 native tests
across66 result groups; strict all-target Clippy, scoped fmt, bans (13 controls)
and root44 typecheck/28 lint pass. Both independent source/test/caller reviews
are CLEAN. Fresh full CI, current artifact proof and separate guard/merge remain
required; no all-venue reporter-completion claim.

## BloFin public request-context adoption

BloFin instruments, ticker seeds and candle queries now declare unsigned Read
semantics through the existing managed transport lifecycle. Context-bearing
pending reads replace opaque public HTTP and numeric candle terminal callbacks;
discovery retires its logical handle on terminal success/failure before native
startup callbacks can enqueue another read. Native catalogue, ticker and candle
mapping remain intact. No private signing, financial admission, history owner or
trading-outcome policy is copied into the venue.

One original numeric candle failure site remains for refusal before Context
issuance; it is a shared command-entry blocker, assigned to `codex-uufub4`, not
an exemption. The exact BloFin inventory shrinks 28 to 24; every other path map
remains unchanged. Context-bearing history completion and multi-request admission
remain separate shared dependencies. [Inventory and scope](RUST_BLOFIN_REQUEST_CONTEXT.md).

## Bitrue FIFO read authority adoption

Public candle/trade completion uses the canonical shared command-entry/retention
seam and generation-bearing Context terminals. The native adapter checks current
Context authority before send and response decoding, while timeout/disconnect
cleanup retains its typed completion authority after expiry. It creates no venue
registry, synthetic HTTP attempt, retry policy or group authority.

The exact Bitrue inventory shrinks 53 to 42 occurrences, with lib.rs 22 to 11:
ten numeric WS terminal calls and one conservative timer-correlation macro retire
with the owned-read representation. The HTTP dispatcher and ten pending.request
fields remain; account, private history, signing and coalesced FetchLeverage are
outside this area. No other venue inventory entries change.

## MEXC public candle lifecycle adoption

The single `FetchCandles` path now consumes shared Context admission, managed
public HTTP release and Context completion. Original-source RED controls cover
unprepared dispatch, parked/released expiry, stale owner delivery and HTTP429
side-effect authority. Native mapping/decoding/status/pause policy is retained.
This closes only this command's lifecycle sites; startup, history, account,
signing and background subscription repair remain separately inventoried.

## KuCoin Spot discovery authority proof

The unsigned catalogue/ticker seeds retain their original read Context through
stream staging and publication, retire it on every terminal callback, and use the
shared static-header preparation seam. No private credential, retry, row codec or
error-classification policy is duplicated. Exact source inventory remains 84
occurrences: the original shared streamed helper still serves unmigrated candles,
so this area justifies no ratchet prune or zero-legacy claim. The complete venue
inventory and remaining prerequisites are committed in PR #1819.

## MEXC candle initial rejection ownership

Closed: the candle adapter's pre-Context numeric refusal no longer assigns
NotSent to an unissued read. Genuine engine command-entry admission publishes
initial rejection once and denies numeric fallback. Original-source RED controls
cover nonfinite time, capacity, valid ID reuse and preservation of an existing
parked owner. Other MEXC command families remain separately inventoried.

### MEXC/Phemex/Polymarket bounded reporter ownership (2026-10-23)

These three account consumers use existing engine `RowReports<String>` bounded
Once instead of local 4096-entry sets. This is a compatibility migration, not a
new financial defect finding: native messages, severity, labels and rearm sites
remain authoritative. MEXC keeps its rejection-filtered mapped IDs and shared
coin/position/order memory. Phemex keeps currency-specific clean rearm and
leg-ID filtering before symbol-key rearm inside admitted reads. Polymarket keeps
validated zero/priced holding forgetting before complete-response admission;
its separate permanent unlisted-asset memory remains unchanged.

Nine new original-GREEN native controls pass with all116 account tests on
unchanged actual #1780 production (40 MEXC,40 Phemex,36 Polymarket). The sealed
source/test/log receipt is
`60396c532d8eb3d220bd3ba74d73578a67d92ba7fd2ef0ae6ee67a2864abba1c`.
The controls pin namespace/currency isolation, rejected duplicate versus named
partial recovery, stale-slice no-rearm, and early valid holding recovery despite
later duplicate invalidation. No product-RED, all-venue closure or private parity
claim. Remaining queued integrations, current-head CI and authenticated complete
raw/ordered-gate proof remain required before merge.


## Lighter candle request ownership

Command candle reads and subscription REST fallbacks use the existing managed
HTTP lifecycle. Commands use Context finalizers and the exact read-delivery
validator; background reads use engine delivery admission plus native
subscription generation/revision fences against stale candle publication.
Typed preparation failures retain engine-provided metadata; terminal background
paths retire their handle before freshness exits and release only their own
in-flight generation. No venue admission, retry or outcome algorithm is added.

The four removed lexical occurrences shrink Lighter51->47 and its lib10->6.
One existing numeric failure remains solely when Context issuance itself refuses
before returning authority; its removal belongs to the shared pre-authority
failure seam. All other venue maps remain exact. This closes this candle area
only, not the remaining public/private/history/signing inventory or master gate.

## Paradex independent startup authority

~~Paradex public catalog and ticker seeds bypass managed HTTP authority and can
remain pending without an engine-owned original deadline.~~ Independent background
Read Contexts now own preparation, release and each 20-second attempt. Native
staging commits only on successful validated delivery; refusal, expiry and
transport failure retire authority before the existing startup Backoff runs.
Candle fallback, authenticated/account/history and joined startup work remain in
their previously committed inventory; this closes only public_seed's send site.

## Hyperliquid perps candle Context authority

Perps candles use actual shared command admission, unsigned static-header ingress,
exact read delivery validation and Context terminals. Native row parsing, body
geometry and the existing single HTTP status codec remain. Spot candles preserve
their legacy protocol branch; no signer, retry, deadline or classification core
is implemented in the adapter. All 52 inventory occurrences (41 fingerprints)
remain because spot and other paths still use every shared lexical site. This
runtime area makes no inventory deletion or all-family closure claim.

## BloFin candle entry fallback retirement

The final numeric candle refusal recorded in the earlier BloFin public-read
audit is removed. Initial admission uses the landed engine command-entry seam;
shared rejection owns the pre-Context terminal. Candle payloads no longer carry
a numeric command ID, and physical callbacks complete through their Managed
Context. The remaining discovery error fallback accepts only Markets/Tickers.
No venue admission policy or finalizer is introduced. The exact own inventory
shrinks 24 to23 occurrences (20 to19 hash rows, five to four files); account,
closed/private history and trading remain tracked with their original owners.


## TxFlow indexed history ownership

TxFlow now adopts existing CursorHistory for native identity observations, staged
page admission, accepted rows and completion. The native 2000-step/200000-cap
indexed sequence maps to shared cursor tokens with 101 pages per income stage.
Empty/no-new continuation and cap proof refusals remain strict; actual later
failures recover only prior accepted eligible output with one owner warning.
Income stage restart retains the same output owner and identities. Native
info payload, archived instrument mapping, financial codecs, full pagination
before sorting/truncation and Store isolation remain unchanged. Exact history
legacy inventory prunes eleven to six sites, zero additions or checker changes.


## Orderly accepted-page history ownership

Orderly history adopts the existing engine CursorHistory owner without a new
engine API or venue-local completion implementation. The single cursor walk
retains the total fifty-page regular/algo budget; native metadata census and
per-stage limit remain venue facts. Actual later read failures recover only
accepted eligible output with the owned warning, while native proof and signing
refusals remain strict. Failed streamed prefixes and filtered rows never authorize
recovery. Request builders, signing, timers, account reducers and Store are
unchanged. Exact request inventory shrinks from thirteen to nine entries by
removing four numeric finalizers, with no additions or checker changes.


## MEXC accepted-page history ownership

MEXC history adopts existing CursorHistory rather than owning a result vector,
identity set and numeric finalization. Window transitions retain the same output
owner and identity ledger while resetting the existing native per-window page
budget. Only validated pages authorize partial recovery; failed prefixes and
filtered rows do not. The cap stays strict. Request builder, native row mapping,
account state and Store publication policy are unchanged. Exact request legacy
inventory prunes nine entries to six, no additions or checker/bootstrap changes.
Kraken Spot, BreakoutProp and Hyperliquid Spot remain explicitly classified as
non-pageable inherited stubs, with no new native history capability.

## Phemex bounded non-income owner

~~Phemex fills/orders/public trades store output and finalize through venue-local
Vec and numeric completion branches.~~ Existing HistoryRows/Completion now owns
staging, one-response admission and terminal delivery. No result is accepted
until complete response validation, so failure rejects Internal rather than
publishing a staged prefix, partial result or incomplete marker. Native limits,
filters, dedupe, sort/shape, shared rate pause and lookup/income policy remain.
This is bounded owner adoption, not a new pager or managed completion seam.
[Scope and original controls](RUST_OWNED_HISTORY.md).

## WOO shared history completion and global paging owner

~~WOO retains accepted history rows, seen identities, physical-page budget and
completion in the venue continuation.~~ Existing CursorHistory/Completion now
owns those duties across all four implemented history axes, including native
fallback transitions and delayed ledger continuations. Existing RangeWalk stays
the subordinate engine v3 geometry selector, preserving its span/direction,
identity-selection and strict truncation facts. No second completion authority,
shared API extension, venue marker emitter or synthetic REST capability is added.
Native current-page facts remain local until complete validation; failed prefixes
do not enter shared dedupe or output. Existing mixed-row diagnostics keep their
timing and also supply the owner's missing-coverage fact. All-native strict
refusals and consuming legacy signing setup remain separate.

## Non-pageable history coverage ownership

~~Kraken Spot, BreakoutProp and Hyperliquid Spot locally return [] for unsupported
historical lists without declaring missing coverage.~~ Existing
HistoryRows/Completion now owns these terminal results and emits an explicit
command-scoped incomplete warning before the unchanged array payload. No row is
staged or accepted and no native endpoint is fabricated. Public trades reject
through Completion with typed pre-I/O Unsupported/NotSent, while lookup's
nondefinitive contract remains separate. Hyperliquid perpetual history and all
native trading/signing/account paths retain their existing behavior.


## Paradex independent book request authority

Public REST book queries use existing engine managed release and Context
completion with exact current read-delivery validation. Native snapshot parsing,
public errors and bounded rate hints remain the existing request-owned codec;
no status ladder, auth/retry policy or outcome aggregator is added. Typed
preparation errors pass intact, and original deadlines/retirement fence stale
results and reused command IDs. Live SBE/JSON books and held account/trading
source are unchanged.

The exact inventory prune is Paradex62->57: lib3->0 and depth_query3->1, with
every other map unchanged. The remaining numeric refusal in depth_query exists
only if initial authority issuance fails before returning a Context; its shared
command-entry prerequisite remains explicit. This closes one independent read
area, not the remaining private/history/candle/JWT or whole-venue inventory.

## Lighter candle pre-authority refusal closed

~~The candle issuance-refusal site requires a shared pre-authority failure seam.~~
Actual shared command-entry authority now owns the genuine initial FetchCandles
callback and its unissued Read denial. Lighter consumes it directly and returns
on rejected/revoked admission without a numeric terminal. The one removed legacy
fingerprint shrinks Lighter47→46 and lib6→5; all other inventory entries remain.
This closes only the candle refusal and introduces no venue capacity, retry or
outcome rule. Native background polling continues using its independent Context.

## KuCoin Spot candle lifecycle adoption

Unsigned Spot candles now retain their original command Context through shared
entry admission, static-header preparation, exact read delivery and Context
completion. Typed failures and correlated companions use the shared engine
authority; discovery background retirement remains separate. The sole legacy
public streamed helper and three numeric candle terminal/preflight expressions
retire. Exact KuCoin inventory shrinks84 to80 occurrences and74 to70 hash rows;
Spot mod shrinks20 to16. The shared Account failure hash stays at count2.
No other venue prune or exemption is introduced. Private/HMAC, account/history,
trading, bullet sessions and futures retain the committed full-map prerequisites.

## MEXC initial Plan credential refusal audit closure

~~The initial plan-list read retries a parsed terminal credential refusal after
MODE succeeds.~~ Re-verified on actual #1786: the old general statement that a
refused key fails startup covered MODE but missed the streamed Plan path. Both
normal Plan envelopes and complete error envelopes without data now reuse the
existing native refusal helper and engine StartupFailed authority. Terminal
cleanup discards the initial staged join and prevents late sibling pagination,
private-login refresh, account poll and asset refresh from resuming account HTTP.
The existing transient public/private startup policies and post-startup behavior
remain. This closure does not migrate signing, request ownership or classification.
Architecture gate: No; existing native facts and shared startup publication are
reused. Account/history ownership remains separate from #1793.

## Phemex unsigned candle lifecycle authority

~~An unsigned candle command bypasses prepared request ownership and can publish
an expired result through numeric completion.~~ When both constructed credential
containers are absent, the native adapter now consumes genuine engine command
entry, managed HTTP and a Context-bearing continuation. Shared lifecycle fences
route waiting, release and response delivery before result or pause effects;
setup/network/deadline failures retain typed causes. Existing native codec and
error handling remain shared with the signed branch. Optional primary/secondary
signing stays on its existing path pending the signer/selection migration.
No legacy-site removal is claimed: every old site still serves credentialed
candles or another family, and incoming inventory remains exact. Architecture
No: shared engine custody and lifecycle are consumed without another executor,
classifier, retry timer or credential-selection policy. Claimed history files
from #1790/#1795/#1796/#1797 remain excluded.

## MEXC standalone book lifecycle adoption

~~An unsigned book command bypasses prepared custody, so an expired response can
publish success or apply a depth/account pause through numeric completion.~~
The native adapter now borrows genuine command entry and carries its Context
with captured scaling/limit through managed HTTP and typed completion. Shared
lifecycle fences prevent stale decode/pause effects before native callbacks.
The native snapshot decoder, totals-before-truncation, status/code510 codec and
live cooldown remain unchanged. Exact pruning removes8 occurrences/7 entries;
there is no signer migration, local timer/classifier, retry executor or numeric
Context lookup. Architecture No: existing shared custody and lifecycle are
consumed. The separate reviewed startup fix is retained as a stack dependency.

### Snapshot-token fan-out: Backpack and Paradex

At actual848924, only Binance and Bitunix used the engine's financial snapshot
admission token. The complete27-crate inventory (including nested Spot adapters)
is in RUST_FINANCIAL_SNAPSHOT_FRESHNESS. Backpack and Paradex now use that same
Store boundary for whole-read admission, native caches and row replacement;
per-ID timestamps, terminal state, amend evidence and other domain clocks remain.
This is engine-consumer fan-out, not another per-venue freshness counter.

Two Backpack defects were independently reproduced on unchanged production:

- A stale streamed order read, or a failed current streamed read, could erase an
  accepted unpriced trigger's pending relabel bookkeeping before final snapshot
  admission. When the ticker seed arrived, the retained take-profit stayed
  classified as StopLoss. Mapping-time insert/remove decisions are now staged
  and applied only after token admission; the two independent controls fail on
  original code and pass after adoption.
- A fully accepted native Pair ACK did not advance the order-read fence. An older
  empty read could remove a retained order, or an older nonempty read could
  inject a row into an empty Store after that ACK. The accepted, fully validated
  identity set now invalidates shared snapshot authority. Both original-RED
  controls pass after adoption; a malformed-identity negative retains its old
  behavior and command correlation/result shape.

Four native assertion REDs cover these two classes. Exact final fixture/source/
log bindings are sealed in receipt
`95ebc65eecbd8e231407715231058a93bd6662737d124f3e2eeae3ead5b2f2af`.
Paradex's native amend-ACK compatibility control passes original code; its two
Store-invalidation/exhaustion controls exercise the newly adopted API and are
not claimed as production defects. Capture exhaustion admits no unfenced read
and preserves native account-cycle progress. Final current-head CI, actual queued
integration and full authenticated raw proof remain separate merge gates.

### Lighter cached reads: remove legacy numeric completion

The cached FetchLeverage and FetchAccountTier branches consume shared initial
Read admission and Context terminals. This closes their capacity/invalid-clock
admission bypass without fabricating HTTP or financial evidence. Rejected and
revoked entries return without numeric fallback; cache data and native errors
remain unchanged after admission. Exactly three lexical sites are removed:
Lighter46→43, lib5→3, limits6→5. SetAccountTier's separate identical numeric fail
fingerprint remains counted. No held trading or engine source is changed.

## WOO family ticker decoding compiler policy

The WOO family retained shared speed defaults, while its explicit venue package
profile still used opt-level z. A names-preserved profile identified generated
Tick deserialization as the main sampled ingress cost. The existing family build
mechanism now overrides only the WOO venue package to opt-level3; no second
parser, Store lookup, numeric codec or engine policy is introduced. Original
regression evidence is the reported landing CPU miss at28.838ms versus27.68ms.
Interleaved unchanged-source artifacts show a local median23.105→18.957ms;
exact-head CI's fixed public/private, CPU, memory and size gates remain acceptance
requirements. [Scope, raw evidence and limitations](RUST_WOO_PUBLIC_CPU.md).

## SDK master Hyperliquid/Lighter parity

~~Public or unresolved Hyperliquid startup asks userFees with an absent subject,
and an empty vault masks a configured trading account.~~ The existing identity
selector now follows master's nonempty vault/applicationId precedence; public or
unresolved fees retain metadata defaults without HTTP. Actual agent discovery
still establishes identity before private reads.

~~Robinhood placements omit the newly configured integrator47296, and the
instance client-order-index allocator wraps after its positive48-bit maximum.~~
Robinhood uses its configured native builder and existing approval preflights.
Each existing per-leg mint reservation considers the current candidate and
retains monotonic state; exhaustion refuses without wrap. No transaction nonce,
managed ownership, retry or outcome policy is introduced. No legacy inventory
site changes; all incoming maps and the #1869 TS reference/harness remain exact.

## Native mode-preflight redo liveness

Repeated external mode changes can make an adapter redo its prerequisite mode
reads indefinitely. Each request remains bounded by its declared read policy,
but the final write-plan root starts only when the native write steps are fixed.
No arbitrary whole-preflight or 120-second product cap is added. A finite redo
limit would change caller-visible liveness policy and needs a separate decision.

## Shared ticker lookup CPU headroom

A named WASM profile identifies indirect symbol comparisons in Store::ticker_index
as a substantial public-data cost. The shared Store now uses the existing
HashMap<Box<str>,u32> index representation for borrowed symbol lookup, rebuilding
in input order and releasing obsolete keys/capacity on replacement. This applies
to every venue using symbol lookup; WOO's cached-index WS reducer remains outside
that hot path. No authority policy, codec, venue-specific shortcut or legacy-site
exemption changes. Exact paired CI performance and family memory/size gates remain
separate evidence from diagnostic profiles.

## Draft socket lifecycle architecture adoption

The shared Supervisor alone did not own current epochs, auth acceptance,
subscription coverage or invalidation. The draft Engine socket lifecycle now
owns those boundaries for OKX and rejects late ingress before health
or native mutation. Review found and corrected inactive retry-stop ownership,
unexpected-close accounting, per-event health reset, silent callback errors and
cleanup loss on queue exhaustion. Safe-integer physical IDs use the landed shared allocator; full family adoption and deployed cutover
proof remain open. No allocator change is included in this socket draft.

The option-1 variant leaves Bitunix incoming-exact on the legacy path; the
temporary socket inventory records that deferred family. No source or gate
claim is inherited from the historical two-family experiment.

### Lighter depth snapshot Context ownership

The independent public REST depth sender consumes an engine Background Read
Context and retires it on every success/failure, including stale subscription or
WS-revision returns. Original deadline/release authority fences publication;
matching-generation native cleanup cannot release a replacement's latch.
Exactly one sender is removed (Lighter43→42, lib3→2). Ticker adoption is deferred
because #1877 holds its interfering candle timer fixture; no timing/order or
source-ownership workaround is introduced. No held signing/trading/test edits.

## KuCoin futures local terminal provenance

Sixteen synchronous numeric history/funding terminals adopt shared initial Read
admission and Context finalization. The exact ratchet shrinks KuCoin80->64
occurrences and70->58 rows, nine files retained; history13->5 and funding9->1.
Six conservative embedded macro fingerprints remain counted unchanged. Legacy
signed-streaming, numeric Completion and multipage scheduling remain held for
their shared signer/history/Plan prerequisites; no zero-legacy claim is made.
No venue owner registry, finalizer wrapper or credential/routing substitute is
introduced. Three defensive guards are migrated without an unreachable native
original-RED claim; thirteen dispatcher-reachable exits supply the production
acceptance coverage.
## Bitunix local history authority adoption

Five synchronous history terminal expressions now consume genuine command-entry
Read authority and complete through the shared Context APIs. The exact four
history.rs scanner rows retire: Bitunix23 to18 occurrences,20 to16 hash rows,
four tothree files. Account3, lib7 and trading8 occurrences remain, including
four conservative trading macro matches. This does not close private history's
legacy Completion/pager or the blocked physical dispatchers; #1771 source claims,
signer and shared multi-request prerequisites remain separate.

## Bitrue local history terminal provenance

Five synchronous numeric refusals consume shared initial Read admission and
Context completion directly. Bitrue inventory shrinks42->37 occurrences,
27->22 rows and five->four files; every other incoming hash/count stays exact.
The still-legacy signed fill/HistoryRows/Completion path and account lookup stay
held for their shared prerequisites, even where the scanner no longer lists the
history source. No private read, group or whole-venue zero claim is introduced.

## Bitrue local leverage terminal provenance

Four synchronous account-command refusals adopt shared Read admission and Context
failure. This independent slice prunes only two count-two account hashes:
Bitrue42->38 occurrences,27->25 rows, five files retained. The separate history
slice's prune composes independently. Native successful leverage joins, refresh,
projection, account replies and signed bracket requests remain unchanged; no
managed private-read, execution-group or whole-venue zero claim is introduced.

## Phemex synchronous history/income Context ownership

~~Eleven synchronous local history/income exits publish numeric terminals without
command-entry Context admission.~~ Local branches now borrow genuine shared
admission and immediately use its Context terminal, before active-set mutation.
Original-source controls show full-capacity local errors lack Internal typing,
unknown-market lookup succeeds despite full capacity, and invalid-clock errors
lack InvalidInput typing. Finite native payloads, refusal order and outstanding
requests are preserved. Exactly eleven Phemex occurrences/entries are removed;
managed network/history and signer adoption remain deferred. Architecture gate:
No lifecycle copy; the adapter consumes shared admission and finalization.

## MEXC local read refusal custody

Eight immediate native refusal sites now consume shared genuine command entry
and settle through fail_context: five history admission sites and three leverage
admission sites. They can no longer bypass the shared command-owner bound or
nonfinite-clock rejection. Native validation precedence, full finite VenueError
fields and all successful transport/paging/reducer paths are preserved. This
closes exactly eight request-legacy fingerprints/occurrences, 29/29 to 21/21;
private signing, account callbacks, discovery and valid history transport remain
separately inventoried. The architecture gate is No: only native local error
projection changes, with no venue-owned lifecycle, timer, credential selection,
retry, classifier or owner registry. CI/replay/size proof remains a separate gate.

### Paradex public-trades Read history bridge adoption

Unsigned single-page public trades now use a dedicated Context-bearing history
owner, separate from legacy private history. Shared exact root/leg/Attempt
admission gates staging, identities, page advancement and terminal publication.
Strict failures discard prefixes; stale/reused owners produce no result/marker.
Only the obsolete public sender is removed (Paradex56→55, history14→13); all
private history counts remain. No signing, session, Plan or engine source edit.

## Hyperliquid local lookup custody

The cached-order match and invalid-client-ID/missing-identity returns now consume
shared genuine Read entry and exact Context terminal authority. The original
numeric returns passed through invalid clocks and exhausted Context capacity;
six original admission REDs cover those branches, with native payload/precedence,
owner isolation, synchronous retirement and unchanged fallthrough controls.
Only the two exact local completion fingerprints are pruned (Hyperliquid50->48,
history13->11). Private history/signing and spot REST capabilities stay native.
Architecture gate: No; existing engine admission and completion choke points.

## KuCoin local Read ownership

Eight local terminal expressions use genuine initial Read command admission.
The original root is validated immediately before cached-result publication or
Spot leverage projection. No HTTP attempt is fabricated and no owner is acquired
on private/pager fallthrough. Existing credential precedence, successful setter
no-ops and accountMarginType capability are preserved. The exact ratchet removes
eight occurrences/seven rows; remaining signing/history/Session sites remain held.

### Lighter ticker seed Background custody

The exchangeStats physical read now carries a mandatory managed Background Read
Context. Shared ownership gates staging/publication and retires on success,
strict stream failure, preparation/network denial and original deadline expiry.
Native partial-row acceptance and per-symbol WS revision dominance are retained.
Six original legacy-dispatch assertion REDs precede the move; controls cover four
deployments/six envelopes, early response/stream fencing, malformed prefixes,
route/transport/expiry recovery, capacity peer isolation and300 retirements.
Only2bbebb…:1 is pruned (Lighter42→41/lib2→1); catalog remains counted.
Architecture No: existing shared Context/HTTP authority and startup Backoff.



## BloFin PublicTrades history authority

The PublicTrades owner now adopts shared ReadCompletion/ReadCursorHistory,
managed HTTP delivery and borrowed page admission. Exact physical delivery is
checked before row mutation or page acceptance; typed shared terminals own both
preflight and asynchronous cleanup. The adapter supplies only its native wire
codec and result shaping. Independent source/adversarial, caller and full-map
reviews answer the architecture gate No.

The legacy inventory remains 24 BloFin occurrences: the shared private-history
dispatcher and conservative query macro matches remain inventoried. This slice
adds no exemption and makes no zero-legacy claim. Signed/private history, trading
and the existing candle entry refusal remain separately tracked.

## BloFin PublicTrades current inventory scope

The actual-#1848 PublicTrades integration preserves all incoming engine and
venue code outside its declared slice. The earlier 24-occurrence snapshot above
predates #1867; current BloFin is 23 occurrences /19 rows /four files, unchanged
by PublicTrades adoption. Its private dispatcher and conservative history macro
sites remain explicit debt; neither the actual bridge nor the capacity assertion
update claims a ratchet removal or private signer/Plan readiness.

## WOO local history admission

The three synchronous pre-I/O history refusal sites use genuine engine Read
entry and Context completion. The separately committed original tests reproduce
three typed-admission assertion failures and four compatibility passes on native
source; the narrow cutover passes all seven assertions unchanged. Exact inventory
shrink is three fingerprints/three occurrences, WOO34/37 to31/34. HTTP pagination,
private signing and socket ownership are separate conversion targets.

### Paradex local history terminal custody

Plan `987e5ea81e2`; original test-only checkpoint `45c743903b8`. With production
and inventory byte-identical to actual `c473342d483`, the new target compiles
and has three native/control passes plus five intended assertion REDs: four
capacity cases and invalid-clock admission. The adapter adopts genuine Read
command entry plus context done/fail in only those four synchronous branches.
The same eight tests pass after source adoption, without assertion changes.

Native controls retain public-only precedence, exact JSON and error metadata,
Unicode client-ID bounds, all three history range validators, pre-auth no-symbol
Fills, PublicTrades bypass, real256-peer isolation and recovery, duplicate-owner
protection, and300 fixed-clock completions followed by full capacity refill.
Inventory removes four single-occurrence hashes only in Paradex `history.rs`:
55->51 total,13->9 in that file. Other paths and same-hash trading siblings stay
intact. The rejected settings proposal remains held on genuine keyless Mutation
no-op terminal authority; no signing, credential or outcome workaround was added.
Broader native gates, independent implementation review and CI are separate.

Local history verification on reviewed integration `56dacc36562` genuinely
includes actual landing `23d0d490009` (#1892 background executions and #1817
credential selection). All513 Paradex/Binance tests pass across42 targets,
none ignored, including the same eight original local-history controls.
Strict all-target/all-feature Paradex Clippy, scoped formatting, bans, the
request-context scanner and all15 ratchet tests pass. Source/adversarial,
caller and full-map reviews are CLEAN on this actual integration; architecture
No. The seven-path diff preserves every unowned incoming blob and all foreign
manifest entries, with only the four planned Paradex history occurrences pruned.
CI and an authorized train slot remain separate gates.


## Subscription handoff custody boundary

The shared correction adds bounded native Pending custody and a read-only
arming tail after the existing final drain dispatch/fence/retirement checks.
Native callbacks run only at later timer ingress; they cannot retroactively
remove prerequisite frames from an already returned batch. Required canonical
interests bind work to committed revisions, and only committed changes cancel
or rebase existing work. Refused replacements do not revoke old committed work.

The uufub4 API/drain review of 4583d233556 plus 8739ef44fcf cleared this timer-boundary
design with explicit committed-change, subsequent-ingress and timer-bookkeeping
controls. The runtime source review is separate. HTTP lifecycle, signing
release/gates, Plan flushing, financial reducers and request recipes retain
their existing ownership. No legacy-inventory or performance-budget changes
are part of this shared correction; WOO consumes the seam in its own PR.

The independent ih35js runtime review clears the handoff-owned d9fd54ff531 source
(exact at 3bebb12dc9f), excluding build/CI claims. Uufub4 identified early-fire
liveness before final clearance: both hosts consume one-shot timers, so retaining
armed custody without a returned rearm can strand work. The narrow correction
re-emits the existing TimerId at its immutable armed_at, with no early callback
or new authority. Original assertion RED and one-use host/engine controls bind
the correction; final runtime clearance is tracked separately from API review.

Final bounded runtime/drain clearance is recorded at 08fca8e6ec4 from uufub4,
including independent archived host 4/4 execution and inspection of the early
rearm correction. Ih35js independently clears that correction and its RED/control
coverage at the same source pin without a build-execution claim. Actual #1932
expiry cleanup is genuinely merged without conflicts at 856daa0a1c2; its incoming
production bodies are retained verbatim.

The #1932 owner confirms `http_plan_expiring` is an expiry cleanup/seal barrier,
not a callback loan. Socket continuation readiness gates on its absence. A
synthetic matching wake during cleanup consumes only its timer, unarms retained
work and leaves the barrier intact; a fresh handoff arm resumes after cleanup.
No expiry flush, lifecycle, seal or poison invariant is rewritten. Two cases in
the original-RED composition control cover both initial arming and prior wakeup.


### Bitunix public REST Read authority

- [x] Public markets/tickers/candles dispatch and their selected terminals use
  genuine shared Read authority and managed static-header HTTP; exact8occ7row
  prune18->10. No early/unclaimed/expired publication or numeric fallback.
- [x] Shared BITUNIX table/read classifier preserves typed engine failures,
  safe diagnostics and native rate floors. Native successful candle codec and
  wire behavior remain compatible. Private P0#33 and signer conversion remain.

### WEEX local history admission custody (2026-10-24)

Original test-only checkpoint `d5746d2d490` on actual `a828ede7ebb` compiles and
runs eleven dedicated controls: three native controls pass; seven independent
capacity assertions and the invalid-clock assertion fail at native legacy
publication. The forwarding fixture retains256 genuine engine Read command
roots and delegates WEEX Query unchanged; it creates no HTTP or fake Context.
Source/adversarial, caller and full-map reviews cleared the plan and REDs.
The seven inline terminals now use shared initial Read admission and checked
Context completion. The unchanged eleven controls pass, including native
credential-presence/validation order, exact error metadata, equal/one-sided/
safe-integer bounds, budget boundaries, remote signed/public wire controls at
full capacity, peer isolation, recovery and300 fixed-clock completions/refill.
Exact own inventory prune is WEEX51->44/history10->3 occurrences; all remote
macro records, foreign maps and held trading/dispatch regions are preserved.

Scoped WEEX/Binance native verification passes506tests,0failed/ignored;
strict WEEX all-target/all-feature Clippy, scoped formatting, source scan and
all15request-context checker tests pass. Implementation review/CI remain separate.

### WEEX account identity Read entry (2026-10-24)

Committed plan `af65a1577b2` and original test-only checkpoint `a55a561b921`
received independent source/adversarial, caller and full-map CLEAN reviews.
Test-only refinement `902335f2b6f` removes a Clippy identity-map expression while
retaining exact header equality; all three reviews cleared that bounded change.
On unchanged actual `a828ede7ebb`, nine tests compile/run:3native controls PASS,
6intended assertion REDs (four local capacity bypasses, remote numeric admission
fallback and invalid clock). A fixture initially drained Prepare before the
shared release helper; this was fixed before freezing and is not a RED.
The unchanged frozen nine controls now PASS after direct use of genuine initial
Read entry and existing signed-managed dispatch in identity.rs. Checked native
companion diagnostics and exact signer/primary namespace survive. The orphan
legacy success arm and numeric failure helper are removed, pruning exactly the
two identity singletons; no held helper, dispatch, account or trading code moves.

Pre-integration native504WEEX/Binance tests and strict WEEX Clippy/fmt pass.
Whole-map provenance is blocked by newer Aster inventory shrink in origin;
actual integration and fresh gates follow, with no foreign manifest edits.

Actual integration e336f67b7de genuinely includes15bd7821b52; all unowned incoming
blobs and foreign prunes are preserved. Fresh504native tests, strict Clippy/fmt,
full bans and both request-context checks (15tests) PASS. Three independent
implementation reviews are CLEAN; architecture No. Exact own identity2F/2O
prune is verified; the parallel #1951 history prune is not credited here.

### Binance identity terminal provenance

The local identity error helper reuses existing Read entry and typed identity
error completion. Its unused numeric success arm and numeric command argument
are removed. Successful credential selection, HTTP preparation and response
handling remain exact to actual `15bd7821b52`; no shared API, session, signing,
history, account reducer or Store changes are involved. The listen-key owner
confirmed the identity helper and distant FetchAccount dispatch arm are clear;
the existing command-entry capacity assertion is preserved.

Pure inventory prune removes only `venues/binance/src/identity.rs`:
`c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548: 1`
and `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa: 1`.
All other manifest bytes/maps remain exact; Binance shrinks from 26 fingerprints
/ 30 occurrences to 24 / 28. No rehash, site growth or checker exemption.

## Phemex unsigned public-trades Context ownership

The unsigned /md/v2/trade query now owns one genuine public Read root and
managed delivery, including its existing native pause queue. Row admission and
terminal ownership use ReadCompletion/ReadHistoryRows; stale429 cannot extend a
pause, and typed preparation/network/expiry errors survive to the correct owner.
Native signed public/private, income, identity, trading and socket paths remain
held consumers. Their shared legacy fingerprints are retained. Only the public
tuple codec's conservative f913fd4f macro occurrence is removed (1F/1O); this is
not a claim that the shared physical request funnel has migrated.

## Selected managed signed Read composition

Shared admission/selection/queue/refusal composition is additive in engine
selected_read.rs, with genuine callback and original-generation controls. The
original BingX/WEEX copies erase capacity denial fields; those two native
assertion REDs are retained for separate consumer adoption, not marked fixed by
engine probes. No venue P0, private signer, Plan history or inventory-zero
claim is closed by this library seam.

## BingX identity selected Read adoption

FetchAccount now consumes actual #1962 rather than the legacy local completion
and admission-error fallback. Its two identity terminal fingerprints are removed;
family inventory is 32F/35O at actual 9f405e55403. Original capacity/clock and
typed Network/Timeout assertions pass alongside native wire/config/codec controls.
The generic managed helper and all other family callers remain with their owners.


## Shared CPU certification owner

CPU remeasurement and aggregation belong to one harness policy, parameterized
by the existing exact suite registry. Every CPU-gated suite adopts the same
planner, eligible-only second-run matrix and unconditional final checker; venue
code owns no bypass or local copy. Existing strict gate validators rederive both
retained reports before arithmetic. Original report bytes and declared results
stay immutable. A sealed runtime capsule transports the original measured
inputs to a separate physical job without rebuilding or installing dependencies.
Current run, source, physical producer, upload, case and cache identities bind
the evidence; missing or ambiguous authority rejects before certification.
No fixture/tape/profile/threshold change forms part of this policy.

Only a validated cpu-runtime suite name selects the 4 GiB artifact response type.
Metadata, logs and ordinary archives retain 512 MiB; cached bytes are rechecked
under the requested type. An empty immutable-artifact HTTP 200 shares the existing
two-attempt retrieval budget with socket recovery. Exact artifact size/digest,
producer/source, archive and restored-runtime ownership are unchanged. The failed
Bybit attempt remains failed and supplies no timing or mean; transport recovery
creates no physical retry or third sample.


### Replay terminal audit observation

The original WOO exported-audit probe reproduced four sticky false coverage
diagnostics per receiver after a discarded checkpoint query. Four working
controls passed and two assertion controls failed before the fix. The
behavior-preserving shared extraction and original REDs are committed at
`d59643b0b16`; the independent causal review binds the exact eight historical
keys without claiming full-tape lifecycle completeness.

One shared capture helper now attaches terminal protocol diagnostics only at
the explicit final boundary in both receivers. The audit implementations stay
unchanged. Controls retain genuine pre-auth violations and genuine late private
errors, propagate final audit exceptions, and reject either caller forgetting
its final flag. Existing capture-before-memory and performance ordering remain
covered. No known declaration is removed by this change.

## GRVT replay coverage diagnostic proof

The final-capture controls retain two original assertion REDs and eight working
controls against the exact pre-correction shared module. The corrected shared
API passes ten new controls plus 14 existing GRVT controls. Fresh authenticated
raw pairs from runs 40037292055/40040892521 differ only in metrics and protocol
errors: TS3/Rust6 error occurrences become empty arrays, covering six unique
keys. The remaining 27 ordered failures, fixers and strict FAIL stay exact.
Independent raw receipt `/tmp/codex-grvt-replay-evidence/independent-review.json`
SHA256 `0b1de5f1261505a046f5cd10647a02f5ea4b6f1f56e16b3f4a7bbf2e0f5d1b10`.
Only the six-key current-tape group is pruned; all other manifest bytes remain.
This one-pair proof does not claim five-repeat or final-head CI qualification.
