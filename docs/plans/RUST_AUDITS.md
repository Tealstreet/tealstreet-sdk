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
4. **Rollback.** `engine` accepts only `'rust'` (`types.ts:137`). Add
   `'typescript' | 'rust'`, a per-venue downstream flag, and a live soak before
   deleting a family.

Settled: Backpack spot retired; Kraken market+TP/SL rejects with a clear error;
Bybit Partial TP/SL keeps TS semantics (sensitive, no change). Standing licence:
obvious improvements (clearer errors, better messages) need no decision.

Already defaulted to Rust with TS removed on this branch: `coinbase`,
`coinbasespot`, the six Orderly brokers, `txflow`. The branch cannot reach master
until 1–3 are settled for them.

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

- Main-thread writes: web places orders on the main-thread exchange
  (`apps/web/src/order/sendOrder.ts:24`); a main-thread `RustEngineExchange` has
  no engine (`rust.exchange.ts:701`). Route commands to the worker engine.
- No `socketHealth`, `reconnect` or `recoverAfterOffline` on Rust
  (`base.ts:1663,2053`); CLI stale-stream gate fails open
  (`cli-core/src/commands/BaseCommand.ts:172`).
- Rust lacks per venue: `fetchHistoricalPositions`, `fetchClosedPnl`,
  `fetchOrderBook`, Hyperliquid `scheduleCancel`, `updateSymbolMetadata`
  (bitrue, weex), typed GRVT builder error, Hyperliquid `enableSpotMarkets`.
- `instanceof BlofinExchange` downstream (`BlofinExchangeReactiveStateHelper.tsx:20`).
- Deletion inventory misses downstream deep imports: `hyperliquid.constants`,
  `bitget.family`, `asterdex.types`, `bybitinverse`/`bybitspot` exchanges,
  `exchanges/registry`, `exchanges/base`.
- exchange-mcp npm release ships without the engine wasm
  (`.github/workflows/exchange-mcp-release.yml:133`, `tsup.config.ts:36`).
- Combined wasm is 11.7 MB with no budget; no browser cold-start measurement.
  Decide combined vs per-family wasm.
- Replay gate references live TS venues (`harness/venues.mjs`); freeze TS outputs
  as golden fixtures before deleting any TS venue, and drop
  `__tests__/reference/`.

## Trading correctness

High:
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

Medium:
- Phemex cancel-all fails while any place is in flight (`phemex/src/trading.rs:806`).
- Cancel-all misses just-placed orders: okx, asterdex, breakoutprop, bitget, blofin, bitrue.
- Backpack amend replaces full size after partial fill (`backpack/src/trading.rs:411`).
- No per-order lock between amend and cancel in most venues.
- ~~Stale snapshot reopens canceled orders: Binance USD-M/PM (`trading.rs:1463`),
  Bybit Demo/Hyro (`account.rs:1824`).~~ Fixed: write/event revision fences plus native terminal retention
  cover in-flight reads and replica-stale reads issued after acknowledgement.
- REST snapshot swallows stream fills: bitrue `account.rs:1291`, bingx `account.rs:726`.
- Binance Spot quantified cancel acknowledgement can advance the execution
  cursor before its matching TRADE, losing the execution notification. Reverified
  on `bd94abb524`; tracked for the Binance follow-up after the derivative fixes.
- Binance PM strategy/regular client-ID reuse on one symbol can let an old
  strategy terminal replay cancel the newer regular row. Reproduced in PR #1348;
  inherited from the base, tracked for native-owner fencing in the follow-up.
- BloFin replays pre-session fills (`blofin/src/account.rs:1231`).
- Stale books stay visible during resync (`engine.rs:1147`); woo ignores `prevTs`,
  kraken spot ignores checksum, asterdex/lighter keep books while waiting.
- ~~Bybit inverse amend writes `remaining` in contracts (`bybit/src/trading.rs:1934`).~~
  Dropped after reverification: the cited local-ack branch only runs for Demo/Hyro, which cannot enable inverse.
  A regression verifies authoritative inverse amend updates preserve coin `remaining` and native `remainingNative`.
- Write timeout discards optimistic state instead of refreshing: orderly, okx.

Low: ~~no reconnect backoff (asterdex, bitrue, bingx, kucoin, cryptocom, phemex,
coinbase public, grvt, paradex, polymarket)~~ — fixed for the requested feeds
by the shared supervisor, including fresh KuCoin token POSTs. Reverification
dropped the literal public no-backoff claim for AsterDEX, Bitrue, BingX and
Paradex: they already had exponential delays, but reset at open; their fixed
private recovery, missing jitter and repeated-flap reset are now covered.
KuCoin client-id collisions;
~~`uuid().unwrap_or_default()` in Binance~~ (fixed: propagate entropy failure instead of producing a broker-only ID); Backpack empty positions on any 404;
OKX `partially_effective` final; Lighter late-fill drop; ~~Bybit unused `exec_id`~~ (fixed: bounded native execution-ID dedupe retained across reconnects).

Scenario tests, independent phase reviews and acceptance evidence for these Binance/Bybit
closures: [`RUST_AUDIT_BINANCE_BYBIT.md`](RUST_AUDIT_BINANCE_BYBIT.md).
Execution and Bybit terminal retention are bounded; private verification remains scripted.

Engine root causes: `Fill` has no execution id and the store replaces lists with
no freshness fence, so every venue reinvents dedupe and terminal fencing.

## Wire contract

Repair lane: `feat/rust-wire-fixes`, based on `bd94abb524`. Reverified against
the current code: all requested findings below still hold; ticker orphan
identities already work and must remain intact. Full-auto phases:

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
  fills precede orders by up to 250 ms; ~~`TickerState` views detach on transfer~~.

Repair evidence: actual Rust-writer → JS decoder → FrameApplier regression,
actual FILLS frame → GRVT chaser partial/completion regression, native layout
and market-reorder/queue tests, decoder legacy/revision/bounds/transfer tests,
worker midpoint bridge tests, and local/worker recovery RPC regressions.
FORMAT remains 1 because the envelope is unchanged; section revisions identify
new fixed tails. Both readers check FORMAT; unknown enum values stay absent.
Independent phase reviews found missing STRINGS acceptance, observer exceptions
masking recovery, worker midpoint loss and orphan native-quantity NaNs; all fixed
with regressions. Execution-vs-order flush cadence remains a separate finding.

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
Fresh combined gates and exact-head CI are required before merging PR #1345.

## Security

- Workers Durable Object has no client auth and serves the first client's account
  to all (`hosts/wasm/js/workers.mjs:189`); start race leaks running engines (`:206`).
- Native transport errors include signed URLs (`hosts/native/src/transport.rs:390,404`).
- Custom credential headers follow cross-host redirects (`transport.rs:101`, `host.mjs:352`).
- live-smoke: no notional cap, no post-only, testnet reads mainnet env names.
- Per-instance nonces collide across engines on one key: hyperliquid, txflow, kraken.
- Low: `setTickInterval` unclamped; unbounded gunzip; credential `Debug`;
  partial zeroization; Paradex 24 h auth signature; Lighter non-hedged Schnorr
  nonce; denylist tape redaction; `set-cookie` in fixtures; broad gitleaks test exemption.
- Clean: no committed secrets; native frame server auth and isolation; TLS; RNG.

## Architecture

- Numeric decoders duplicated 40× across 19 venues, already divergent → `engine::de`.
- Socket supervision copied per venue, no jitter, two without backoff →
  `engine::socket::Supervisor`.
- Venue registry lives only in the wasm host (`hosts/wasm/src/lib.rs:592`) →
  `safe-cex-venues` crate shared by both hosts.
- Bybit fields and venue-named features in the engine (`model.rs:411`, `engine/Cargo.toml:10`).
- Client ids use OS randomness in the deterministic core (`crypto/src/uuid.rs`).
- ~30 URL encoders in three dialects → `engine::url`.
- `RangeWalk` used by 6 of 26 venues; three book-stitch designs.
- Read-key routing re-implemented ~14 times plus 12 copied test files.
- `docs/plans` 47 files; collapse to this file, `RUST_REWRITE.md`,
  `VENUE_PORT_BRIEF.md`, the cutover docs, and per-venue READMEs. Roster stale.
- Workspace manifest, `rust:*` scripts and `.cargo/config.toml` comment cleanup.

## Gate honesty

No venue has live private evidence; private tapes are generated in CI from
documented or synthetic payloads (14–24 frames for several venues).

- Binance/Bybit oracle copies Rust's `marginSize` into TS when TS is 0
  (`harness/gate.mjs:646`). Circular.
- "Identical output" is final-state only and partial (`harness/replay.mjs:803`):
  no TP/SL, realizedPnl, fees, fill timestamps or execution ids; fill prices
  skipped on binance, bybit, bitget, bingx, cryptocom, mexc, hyperliquid, weex,
  breakoutprop; rows keyed by symbol collapse duplicates (`gate.mjs:87`).
- Whole-section rewrites of the TS side (kraken, backpack, coinbase, orderly,
  kucoin, phemex, cryptocom, bingx, lighter, weex, binance variants); some
  normalizations apply to Rust too and can mask regressions.
- ~20 TS venues were edited on this branch as paired fixes, so the oracle moved
  with the port. Gate against master TS and declare deltas.
- Perf: median of 3 with no margin, harness audit cost inside the window, wasm
  compile outside it, process CPU ungated (Rust loses on grvt, hyperliquidspot,
  bybit, woo, breakoutprop), failed perf jobs re-run to green, retained memory
  mostly TS module graph and redefined ~10 times.
- Size budget covers per-venue builds; the shipped combined build (≈10–11.7 MB)
  has none. Paradex has 678 bytes of headroom.
- Public fixtures are recorded by driving Rust, so TS-only subscriptions cannot
  diverge; fixtures refreshed alongside behaviour fixes.
- `bitgetusdc` has no CI replay gate; HyroTrader and BreakoutProp public data are synthetic.

Before cutover: live-smoke private captures on every venue (mainnet minimum size
for coinbase, backpack, mexc with approval), a 3–7 day read-only production
shadow diffing full store snapshots, public fixtures re-recorded with TS, and a
per-venue canary holding the circular-evidence venues last.

## Port reviews (Opus, per venue vs TS)

### OKX
- ~~High: blank `bidPx`/`askPx`/volume or any failed instruments/tickers read at
  startup halts the venue with no retry (`okx/src/lib.rs:841,995`); TS treats
  blanks as 0 and continues. Parse blanks as 0, retry with backoff.~~ Fixed:
  blank or malformed seed quotes start at 0 (row kept for live updates); a failed
  or unusable catalog/ticker read retries the whole product chain with backoff.
- ~~Medium: one bad instrument row aborts the catalogue (`lib.rs:738`); leverage
  for flat symbols missing (no `leverage-info`); account config (posMode, acctLv)
  read once, so a mode change in the OKX app makes every order reject.~~ Fixed:
  bad rows are skipped and counted in a warning; `fetchLeverage` reads
  `leverage-info` (it previously never answered); config is re-read every 30 s
  and a changed mode/level reconciles positions.
- Low: `mmp`/`elp` order types get an empty id; conditional `price: 0`
  rejected; income limit truncates.
- Confirm (TS bugs Rust fixed): OCO leg edit deleting its pair, triggered algos
  staying open, X-Perp history SWAP hardcode, market-order amend placing a new
  order, duplicate algo cancels. Shared by both: reduce-only conditional sent
  without `reduceOnly` and un-inverted `posSide` in hedge mode.

### Hyperliquid
Signing hash, order payload and price/size rounding match TS; no new highs.
- Medium: HIP-3 leverage change sends cross where TS sends isolated
  (`hyperliquid/src/trading.rs:758`); read `onlyIsolated`/`marginMode`. HIP-3
  margin-type change with no position fails (default leverage 20 > market max,
  `trading.rs:759`). Spot open orders never reconciled and can go stale
  (`account.rs:426,1604`). Edit aborts when the source order already filled
  (TS places a second order) — confirm fail-closed is intended.
- Low/confirm: spot testnet signing and spot size rounding (TS bugs Rust fixed);
  spot vault signs as vault (TS as master) — confirm; more terminal statuses;
  `stopLoss: 0` rejects whole order; signed fees; unified balance spot slices
  not populated; fills lack execution id, realized PnL and fee (engine `Fill`).

### Binance family
- ~~The known USD-M amend double exposure also hits PM, where TS throws on -2011
  and Rust re-places (`binance/src/trading.rs:530`): a regression.~~
  Fixed with the USD-M/PM cancellation-proof and racing-fill regression matrix.
- Medium: PM upserts `fs:"CM"` rows as phantom positions (`account.rs:1381`);
  REST candles with `from`+`limit` return the oldest N instead of newest, no
  1500 cap (`lib.rs:1439`); COIN-M strict batch discards placed ids on a partial
  reject (`trading.rs:397,700`); PM `close: true` with an amount closes the whole
  position (`trading.rs:1087`); COIN-M unpriced coin excluded from totals
  (TS counts 1:1 — confirm); ~~CancelOrders with unknown ids reports success
  without sending (`trading.rs:224`)~~ (explicit errors; see
  [cancel audit](RUST_UNKNOWN_CANCEL.md)); history maps `EXPIRED_IN_MATCH` to filled
  (`history.rs:687`); spot/COIN-M history returns plain `[]` instead of
  unsupported; USD-M journal (`fetchHistoricalPositions`) not wired in the host.
- Low: fixed `recvWindow`; closePosition conflict cancel in batches; Binance UI
  STOP orders amend to stop-market; PM -2011 plain cancel marks canceled.
- Confirm (TS bugs Rust fixed): fill price `L` not `ap`; spot fill price on
  market orders; spot cancel uses `C`; amend keeps TIF; PM conditional client id
  `newClientStrategyId`; PM TRIGGERED closes; PM cancel-all both endpoints;
  COIN-M rejects caller client ids; spot retired streams; depth snapshot retry.

### Bybit family
- High: Brazil inverse/option sockets routed to the linear host (engine
  transport keys routes by channel, `packages/safe-cex/src/transport/engine.ts:552`,
  `bybit/src/lib.rs:407`); per-coin balances never set (`account.rs:662`);
  fills history includes funding/settlement rows (`history.rs:564`); income
  dedupe drops hedge-mode funding legs (`history.rs:131`).
- Medium: option failures stall linear startup (`lib.rs:857`); option catalog
  `baseCoin=All` unverified; live candles lack trade aggregation and seconds
  intervals; option WS ticker field names likely wrong; no UNIFIED retry on the
  wallet call; demo reduce-only fill check missing; Hyro tears down on one bad
  poll (`account.rs:1838`); order/income history caps; Partial TP/SL amend
  semantics — decided: match TS (flip SL/TP on drag), do not amend in place; spot create params (`marketUnit`); one unmappable
  order invalidates every later snapshot (`account.rs:1569`).
- Low: OHLCV from+limit; no market refresh; Hyro market filter; subscription
  breadth; split-lot error handling; classic upnl; inverse reference price;
  demo key hint, recvWindow, rate limiter; Hyro early pings.
- Confirm (TS bugs Rust fixed): spot amount precision, option books socket, spot
  cancel-all groups, spot trailing/full TP/SL endpoint, inverse position-mode
  category, stop with only price, category-filtered private topics,
  PartiallyFilledCanceled, inverse units, failed position read zeroing, create
  network errors, live candle volume units.

### BloFin
- High: a failed or malformed ticker read at startup means the private side
  never starts, no retry (`blofin/src/lib.rs:617,651`); ~~`brokerId` never sent,
  losing broker attribution (`trading.rs:126`, TS `blofin.exchange.ts:2159`)~~
  ([attribution audit](RUST_BROKER_ATTRIBUTION.md));
  ~~per-symbol margin mode stays stale after a position closes, so orders go out
  in the wrong margin mode (`trading.rs:271`, `account.rs:614`).~~
- Medium: ~~`null` in order/position/ticker fields rejects whole snapshots/frames
  (`wire.rs`; verify against one live frame)~~ (documented optional protection
  nulls and row-local WS recovery covered; captured public tape has no nulls;
  invalid required private REST quantities still preserve the snapshot);
  ~~WS login timestamp serialized as float (`account.rs:284`)~~;
  one odd instrument row aborts startup
  (`lib.rs:505`); ~~cancel of an id the store lacks resolves OK with no request
  (`trading.rs:671`)~~ (explicit error).
- Low: strict leverage fetch; trigger TP/SL not tick-rounded; missing
  iso/cross warnings; 102143 not translated; history shape.
- Confirm (TS bugs Rust fixed): IOC/FOK rejected; TP/SL trigger type honored;
  zero-size partial TP/SL rejected; amend checks order-detail (endpoint
  unexercised live); per-row cancel errors; empty snapshot commits; income
  filters; boolean reduceOnly.

### Cross-venue patterns from the port reviews
- Startup is fail-closed with no retry on OKX and BloFin where TS continues on
  partial data; check every venue and set one engine policy.
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
  One bad market row or a transient error halts startup with no retry
  (`lib.rs:552,684`) — left to the startup/broker lane.
  ~~One unmappable row discards the positions/orders snapshot
  (`account.rs:505,806`).~~ Typed rows skip/log independently; malformed reads
  and wholly unmappable nonempty snapshots preserve known state.
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

Verdict: reads and streaming match; writes and lifecycle do not yet. Blocks
further cutover and warrants re-checking coinbase, Orderly, txflow.

- High: cancel sends ids only and venues drop ids not yet in their store, so a
  chaser stop during placement leaves the order live (`rust.exchange.ts:789`,
  `bybit/src/trading.rs:422`); ~~`setLeverage` drops side (see Bitget)~~; a wasm
  panic freezes the account silently — no containment in `host.mjs:131,175`;
  `fetchMarkets`/`fetchTickers` on an unstarted instance return `[]` and
  `PublicExchangeFactory` caches it (`rust.exchange.ts:856`); book/candle
  subscriptions made before the engine starts are lost (`rust.exchange.ts:1000,1022`).
- Medium: ~~`fetchLeverageForSymbol` returns market max for mexc, bingx, weex
  (`rust.exchange.ts:945`) and frame mode strips `fetchedLeverage`~~; command
  rejections don't emit the error toast (one-error rule); `privateOnly`/
  `externalPublicData` ignored; `getAccount` missing for ~18 venues; dispose
  mid-command reports a placed order as failed (`host.mjs:83`); base
  `cancelAllOrders`/`nuke` read the 250 ms-lagged TS store; fills lack
  realizedPnl/fee; `fetchOrders`/`fetchPositions`/`removeOrders` don't reach the
  engine (needs a resync command); `validateAccount` leaves a started engine.
- Low: Asterdex unsupported interval mapped to 1h breaks candle matching;
  `num::adjust` with step ≤ 0; no command deadline, `execute` drops the batch tail
  on an unknown action; candles drop `lastSide`, orders lack generic `triggerType`.

### Kraken
Futures matches TS on nearly every input; no high regression.
- Medium: isolated positions flip to cross when a WS row omits
  `maxFixedLeverage` (`kraken/src/account.rs:1056,1132`; confirm live);
  `more_candles: true` fails the whole candle fetch, empty chart (`lib.rs:727`);
  spot `EGeneral:Permission denied` on an order deactivates the account
  (`spot/account.rs:513`).
- Decided: market entry with attached TP/SL is rejected (Rust behaviour);
  the error must say the TP/SL legs are unsupported on market entry.
- Low: income over 500 rows errors (TS partial); one bad row rejects
  catalogue/tickers/open orders and blocks private start (startup lane);
  missing funding field set to 0; raw error text; spot 3m candles.
- Confirm (TS bugs Rust fixed): index trigger signal; full_fill not canceled;
  live % change/volume/OI; stop amend racing a fill; spot fills over-reported
  and mislabelled XBT/BTC.

### Orderly and TxFlow (already default to Rust)
TxFlow loses nothing (master TS was a placeholder).
- High (Orderly, all six brokers): standalone SL/TP rejected because the app
  always sends `triggerPrice: price` (`orderly/src/trading.rs:362`); master TS
  ignored `triggerPrice`. Chart-drag stops, stop form and hotkeys all fail.
- Medium: cancel-all only loops symbols in the store instead of the unscoped
  DELETE (`rust.exchange.ts` has no Orderly override; engine supports
  `symbol: ''`); SL/TP blocked when a trailing stop or quantity TP_SL exists on
  that side (`trading.rs:254`).
- Low: one bad market row or unknown order row fails closed (startup lane);
  trailing-stop display price; cancels skipped for unknown/finished ids;
  candle `from`/`to` filtering a latest-N fetch.

### KuCoin
- High: ~~cancel-symbol also deletes the symbol's TP/SL stops
  (`kucoin/src/trading.rs:515`; TS only `/api/v3/orders`)~~ (stop-book deletion
  now only on account-wide cancel); one failed or
  malformed markets/tickers read stops the venue, and futures private REST
  waits for the private socket `welcome` (`lib.rs:767,954,1026`).
- Medium: ~~one unknown id rejects the whole cancel batch (`trading.rs:505,1034`)~~
  (uncached IDs sent directly; later cancellations continue and failures surface);
  one malformed REST row freezes reconciliation (`account.rs:652`); caller client
  id on attached TP/SL rejected (`trading.rs:120`).
- Low: leverage sent as `10.0`; fill without `matchPrice` dropped; spot polls
  5 s vs 500 ms; per-symbol cancel-all; partial balance rows ignored.
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
  one unknown order status/type drops the whole private frame incl. positions
  and balances (`account_wire.rs:298`, `account.rs:601`).
- Medium: one bad public row stops startup; one bad position row freezes a
  currency slice (`account.rs:286`); no fallback read when a symbol's mode or
  leverage context is uncached (`trading.rs:229,713`).
- Low: cancel batch refused on one uncached id; index triggers refused; daily
  candle `to`; no 1 s REST resync after terminal events.
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
  startup halts on tickers/markets/position-mode/plan-list failures
  (cross-venue startup lane; not changed by the venue-highs lane).
- Low: every plan order marked reduce-only; signed fill fee; `fetchOrderBook`
  refused by the adapter.

### Cross-venue: rows and subscriptions
- One unmappable row discarding a whole snapshot/frame recurs (Bitget, Phemex,
  MEXC, KuCoin, Kraken, Orderly, Bybit): skip and log the row, keep the store's
  copy of that id.
- Check every venue honours `SetTickerSymbols` (~~MEXC ignores it~~; repaired).

### BingX
Normal inputs send the same requests as TS; the risk is resilience.
- High: ~~one bad market/ticker row or failed funding read keeps the private side
  from ever starting (`bingx/src/lib.rs:529,639`); one bad position/order row
  freezes that list (`account.rs:491,557`).~~ Bad rows skipped (placeholder
  tickers, stored copies kept); funding retried separately.
- Medium: ~~`TRIGGER_MARKET` stops (attached brackets) can't be moved
  (`trading.rs:842`); no one-shot resend on signature error 100001;~~ multi-order
  cancel sends nothing if any id is unknown (`trading.rs:780`) — intended per
  [the unknown-cancel audit](RUST_UNKNOWN_CANCEL.md) (error before any write).
- Low: hedge leverage sets both sides; `cancelRestrictions=ONLY_NEW` on amend;
  per-symbol leverage/margin not read; trigger move requires trigger_type;
  bracket `workingType`; history fails on one bad row; WS message with one bad
  field dropped.

### WEEX
- High: ~~one `null` numeric field in a public row stops the venue, private start
  gated on tickers (`weex/src/wire.rs:73`, `lib.rs:671,727`); one unparsable
  order/position row freezes the list, no `planType` fallback (`account.rs:335`).~~
  Bad public and account rows are skipped (stored copies kept, exact hedge ids);
  ticker failure no longer blocks private start; `planType` fallback added.
- Medium: ~~failed config read backs off all polling to ~60 s (`account.rs:518`);~~
  balance stops updating during active trading (revision discard,
  `account.rs:408`) — resilience lane; ~~accepted conditional reported failed if
  the venue echoes `clientAlgoId` (`trading.rs:1303`; verify live); imported limit
  with TP/SL can't be amended (`trading.rs:343`); missing passphrase looks like a
  valid empty account (`lib.rs:148`).~~ Settings-only backoff, algo id echoes
  accepted, imported TP/SL amend keeps row working types, partial credentials
  deactivate visibly.
- Low: isolated leverage sets both sides; index triggers rejected; fill without
  `id` dropped; income row without asset fails the page; fetchLeverage returns max.

### Coinbase (already defaults to Rust)
- High: one unmappable open order (e.g. a bracket or TWAP placed on Coinbase's
  site) rejects every snapshot and blocks all trading incl. cancel
  (`coinbase/src/account.rs:336,1577,1842`, `trading.rs:1028`); a failed
  placement leaves a reservation that breaks cancel-symbol for the session
  (`trading.rs:1109,1283,1164`); CLI market orders with GTC rejected
  (`trading.rs:264`, `cli-core/.../BaseBuySellCommand.ts:368`); browser-side
  `signWsAuth`/HTTP signing lost (decision 2).
- Medium: reduce-only capacity counts protective stops against TPs and spot
  reduce-only always rejected (`trading.rs:654,676`); portfolio probe stalls on
  404/other non-2xx (`account.rs:1322,1687`); one bad CFM position row rejects
  the snapshot (`account.rs:1246`).
- Low: CFM `upnl` and spot `total` sources differ (equity double-count risk);
  OHLCV honours `from`; placement errors lose Coinbase's message.

### Lighter

Signing, integer scaling, tx encoding and nonces match TS.

- High: ~~position `initial_margin_fraction` ("20.00" = 5x) read in basis points
  (`lighter/src/account.rs:424,877`), so leverage reads as the market max and a
  margin-mode change signs max leverage (`trading.rs:567`).~~ Position-only
  ratio/percentage/tick normalization now has REST/WS and 5x signing regressions.
- Medium: windowed fill history pages the whole account history then fails
  (`history.rs:404,744`); one bad market/account row stalls startup or sync;
  account tier not exposed and `changeAccountTier` missing.
- Low: thinner error mapping (rate-limit guidance, 21613 refresh); reduce-only
  re-check races a fixed 2 s timer. ~~Shared with TS: amend re-places the full
  original size after a partial fill.~~ Rust now re-places the observed remaining
  size for price-only amendments; explicit replacement quantities are preserved.

### WOO
No confirmed highs; valid data matches TS.
- Medium: algo child rows don't inherit the parent's side/status/reduceOnly, so
  fills can be lost (`woo/src/account.rs:700`; verify one live frame); one bad
  position row freezes positions (`account.rs:749,949`); one unmappable
  fill/order-history/instrument/ticker row fails the read or startup.
- Low: SL trigger falls back to `triggerType`; algo reduceOnly default; leverage
  readback can report a false error; isolated warning; stop without reference
  price rejects the snapshot.

### AsterDEX
No confirmed highs.
- Medium: ~~failed position-mode read silently leaves one-way, never retried, and
  orders can go before it lands (`asterdex/src/account.rs:566`); order events
  with unmapped types (e.g. `LIQUIDATION`) dropped with their fills
  (`account.rs:817`); one malformed row halts startup (`lib.rs:849,781`); one bad
  row discards open-orders/positions snapshots (`account.rs:402,415`); hedge
  amend of a closing order may open the opposite side (shared with TS; reuse the
  row's `positionSide`).~~ Mode read retries and holds placement until known;
  unmapped order types keep their fills; bad rows are skipped keeping stored
  copies; hedge amend reuses the recorded `positionSide` (TS bug fixed).
- Low: no credential check at start; ACCOUNT_UPDATE strict on `ep`/`cr`; fills
  history fails on one row and refuses >7 day windows; listen-key keepalive
  omits `listenKey` and tears down on failure; partial balance rows skipped.

### Crypto.com
Mostly matches TS.
- Medium: per-coin balances (`position_balances`) dropped
  (`cryptocom/src/account.rs:232`) — resilience lane. ~~Caller client id refused
  on stop/TP orders (`trading.rs:76`).~~ Matches TS and the client-order-id
  contract (`rust.exchange.ts` rejects it before the engine); a change is a
  contract decision, not a port bug.
- Low: ~~one invalid order fails the whole batch; null string fields fail reads;
  one bad instrument row stops startup.~~ Bad rows in reads and pushed frames
  are skipped with one summary warning, keeping stored copies; startup stops
  visibly only when no row maps. Placement still validates the batch before writes.

### Bitrue
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
- Low: one bad contract row blocks startup; stream stops misclassified;
  snapshot starvation under load (revision discard, `account.rs:581,1310`);
  malformed snapshot never sets loaded.

### Cross-venue: per-coin balances and snapshot starvation
- Per-coin balances missing on Bybit, Crypto.com, ~~Bitrue~~; check every venue.
- Revision-discarded snapshots starve under active trading on WEEX and Bitrue;
  check every venue that fences REST snapshots by stream revision.

### ApeX Omni
Signing and payloads match; no confirmed highs.
- Medium: cancelled ids hidden for good (terminal map, `apexomni/src/account.rs:150`;
  cancel-all marks untriggered stops terminal, `trading.rs:928`); one bad
  open-order row leaves the account side dead incl. signing state
  (`account.rs:975`); null fields drop whole private frames incl. fills
  (`private_wire.rs:205`); one odd `/v3/symbols` row blocks private start; a bad
  omni key seed kills read-only monitoring (`account.rs:504`).
- Low: caller client id rejected on stops/brackets/splits; float remaining size;
  zero-price candle rejects batch; leverage always sent. Unchecked: private
  socket via Tealstreet proxy.

### Paradex
StarkNet typed data, JWT, payloads and SBE decoding match.
- High: one blank ticker field halts the venue incl. private side
  (`paradex/src/public.rs:714`, `lib.rs:127`); ~~a failed TP/SL child logs at
  Warning so no toast — user believes they are protected (`trading.rs:855`)~~
  — child failures now produce Error notices while retaining accepted entry IDs;
  REST positions may never commit if `leverage`/`liquidation_price` are blank
  (`account.rs:1824`; verify live).
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
- Low: one unmappable order row discards refresh; leverage-0 config row; TP/SL
  of 0 rejects order; history fails on one row; stale uPnL after WS close;
  ~~README says remote signing rejected~~ — corrected to the landed payload
  signing contract exercised by the delegated-signing regressions.

### Cross-venue: silent partial failures
- Child-leg failures (TP/SL after an entry) must surface as errors on every
  venue (~~Paradex logs at Warning~~; now Error with a native regression).
- Batches that fail midway must return placed ids with the error (~~Paradex~~ repaired,
  Binance COIN-M, Bitrue cancels).
- Integers serialized as floats (`20.0`): ~~Paradex leverage~~ repaired, KuCoin leverage;
  check every venue's integer fields.

### GRVT
Signing and integer scaling exact; no highs.
- Medium: ~~builder check fails once → every order rejected for the session, and
  a matching row without `mfr` treated as unknown (`grvt/src/account.rs:1531,1011`,
  `trading.rs:777`); one unknown-instrument row hides all positions or orders
  (`account.rs:516,422,970`); GRVT's rejection message dropped and misleading
  "reconcile" advice added on definitive rejects (`wire.rs:180`, `trading.rs:1348`).~~
  Builder check retried (TS budget) with orders held FIFO; rows skipped keeping
  stored copies; venue code/message surfaced, reconcile advice only when unknown.
- Low: order history fails on one bad row; signer mismatch blocks trading;
  1008 loses the proxy-whitelist message; amend may cancel without replacing if
  cancels are asynchronous (verify live).

### Polymarket (development-only)
CLOB V2 signing and amounts match the official client.
- Medium: one `null` in a Gamma market row breaks the venue with no retry
  (`polymarket/src/wire.rs:33`, `lib.rs:688,832`).
- Low: cancel-all reports failure after a successful cancel; one bad position
  row freezes positions; auth failures invisible.

### Registration and options (all RUST_VENUES aliases)
Alias→product mapping, credentials, testnet, routing and broker/builder
attribution are correct for all 48 names (~~BloFin `brokerId` aside~~; repaired
with its production/proxy gate).
- Medium: Coinbase accounts with a saved read-only key fail to start — no
  `secondaryReadHeaders` entry or `read_credentials` (`transport/engine.ts:76,116`,
  `hosts/wasm/src/lib.rs:656`); Coinbase is Rust-only.
- Low/medium: polling controls ignored — `extra.tickInterval` (incl. `Infinity`
  to stop polling), the server slowdown multiplier and credential-rejection
  backoff reach only BreakoutProp (and raw tickInterval to Orderly, where
  `Infinity` becomes `null`) (`rust.exchange.ts:308,512`); every other venue polls
  a fixed constant, so the rate-limit incident lever does nothing on Rust.

### Backpack
Signing, order payloads (incl. app stop shapes), amend, cancel and positions match.
- Medium: spot markets dropped — 88 SPOT pairs vanish and open spot orders
  can't be seen or cancelled (`backpack/src/lib.rs:543`); decided: retire
  Backpack spot (remove any spot surface, no port). A mark-price failure or one bad ticker row
  blocks startup (`lib.rs:211,787`).
- Low: one bad fill row fails fill history (`history.rs:312`). Verify live:
  open-order rows with `"quantity": null`.

### BreakoutProp (disabled in production)
App stop payloads accepted; amend stricter than TS.
- High if re-enabled: one unexpected portfolio row (XBTUSD lot, terminal-placed
  order type, multi-leg order) rejects the whole snapshot
  (`breakoutprop/src/account.rs:159,226,261`, `lib.rs:920`).
