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

Settled: Backpack spot retired; Kraken market+TP/SL rejects with a clear error
(futures and spot; spot was placing the entry unprotected until verification);
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

- ~~Main-thread writes: web places orders on the main-thread exchange
  (`apps/web/src/order/sendOrder.ts:24`); a main-thread `RustEngineExchange` has
  no engine (`rust.exchange.ts:701`). Route commands to the worker engine.~~
  Fixed: a bridged main-thread instance forwards its trading methods to the worker engine (B4).
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

- ~~Numeric decoders duplicated 40× across 19 venues, already divergent → `engine::de`.~~
  Done: every venue decodes through `engine::de` (RUST_QUALITY helper 1).
- ~~Socket supervision copied per venue, no jitter, two without backoff →
  `engine::socket::Supervisor`.~~ Fixed: every venue socket runs under the shared supervisor (#1385, #1406, #1423 and the final socket PR).
- ~~Venue registry lives only in the wasm host (`hosts/wasm/src/lib.rs:592`) →
  `safe-cex-venues` crate shared by both hosts.~~ Done: `sdk-rs/registry` is shared
  by the hosts (RUST_QUALITY helper 6).
- Bybit fields and venue-named features in the engine (`model.rs:411`, `engine/Cargo.toml:10`).
- ~~Client ids use OS randomness in the deterministic core (`crypto/src/uuid.rs`).~~
  Host-owned identity and separate signing entropy now enter through `Engine`/`Cx`;
  production hosts bind OS/WebCrypto and replay hosts inject tape-derived identity.
  Missing providers fail construction; CI bans ambient core/venue draws.
- ~~~30 URL encoders in three dialects → `engine::url`.~~ Done (RUST_QUALITY helper 2).
- ~~`RangeWalk` used by 6 of 26 venues~~ (shared `CursorWalk`/`windows`/`OffsetWalk`
  page 12 venues; the rest stay hand-rolled with reasons in
  [RUST_HISTORY_PAGING.md](RUST_HISTORY_PAGING.md)); three book-stitch designs.
- Read-key routing re-implemented ~14 times plus 12 copied test files.
- `docs/plans` 47 files; collapse to this file, `RUST_REWRITE.md`,
  `VENUE_PORT_BRIEF.md`, the cutover docs, and per-venue READMEs. Roster stale.
- Workspace manifest, `rust:*` scripts and `.cargo/config.toml` comment cleanup.

## Gate honesty

Gate implementation and measurements are tracked in
[RUST_GATE_HONESTY.md](RUST_GATE_HONESTY.md). Strict replay failures remain FAIL;
CI can accept only the exact venue, tape hash and diagnostic fields in the
single [`known-gate-failures.json`](../../sdk-rs/harness/known-gate-failures.json)
list. An accepted known failure is not a parity pass. Baseline/workload and
candidate/fixer integrity failures cannot be exempted.

Implemented on the gate-honesty lane:

- Binance/Bybit margin expectations come from taped native position/configuration
  fields. Isolated TS fixers cannot receive candidate output; AST and mutation
  controls reject circular fixes.
- Compare TP/SL and each leg's trigger source, conditional execution price,
  parent IDs, open/flat-position realized PnL and fill
  price/fee/time/execution identity. Client IDs and fill prices default on; any
  exclusion requires an explicit venue/field/reason. Duplicate public symbols
  fail before filtering or map construction. Compare production receiver state
  at startup, tape quarters, commands and terminal checkpoints, before fixers.
  Validate each runner's boundaries against the tape plan independently, with
  required account/book sections; matching omissions cannot establish coverage.
- Per-venue leaf permissions and structural rules constrain every fixer;
  overwritten-field counts are reported. No fixer rewrites candidate requests.
- At least seven fresh interleaved performance pairs, alternating first runner;
  require 10% improvement in main/steady/process CPU and retained memory.
  Implementation loading and WASM compilation count in total CPU. Audit
  callbacks, checkpoint reconstruction and baseline GC are outside performance
  intervals. Retention subtracts the idle-loaded module baseline and counts
  WASM growth once. Each performance run must reproduce its own audited final
  output and transport receipt. No rerun-to-green performance selection.
- Fixed combined shipping WASM budget: 13,631,488 bytes, checked on both browser
  and Node artifacts before standalone overwrite. Standalone is 1,310,720 (#1421).
- Bitget USDC has public/private CI cases with explicitly authored synthetic
  provenance. A private query timeout remains incomplete evidence, not a pass.

Remaining evidence limitations:

- No venue has live private evidence; private tapes are documented/synthetic.
  Several TS handlers emit execution identity or timestamps, now preserved by
  the comparison. Rust now preserves Hyperliquid execution IDs; other venues'
  execution IDs and live-fill timestamps remain unreported where not supplied.
  Other TS handlers lack them too. Null/null equality cannot establish those capabilities. Missing wire fields
  remain missing, never reconstructed into candidate output from the tape.
- Some tape-derived TS corrections still rebuild sections. Their declared leaf
  fields and overwrite counts are visible; raw checkpoints do not use them.
- About 20 TS venues changed as paired fixes, so the oracle moved with the port.
  Freeze outputs against master TS and declare deltas before removing references.
- Existing public recordings were driven by Rust; TS-only subscriptions can
  still be absent. Fixtures were refreshed alongside behavior changes.
  HyroTrader, BreakoutProp and Bitget USDC public fixtures are synthetic.
- The harness measures implementation loading/runtime plus replay routing and
  minimal receipt recording. Common harness imports precede those intervals;
  this is not a browser cold-start or pure venue-handler benchmark.
- CI performance replays use the selected standalone venue build. The combined
  shipping build has a separate size budget; these standalone measurements do
  not establish combined-artifact cold-load or browser performance.

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
