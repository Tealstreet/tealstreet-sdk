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
- Client ids use OS randomness in the deterministic core (`crypto/src/uuid.rs`).
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
  the comparison. Rust live-fill frames lack those fields; other TS handlers
  lack them too. Null/null equality cannot establish those capabilities. Missing wire fields
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
Reviewed on `feat/rust-bitunix` at `846f285d13` (PR #1309, not yet merged); paths
are relative to `sdk-rs/venues/bitunix/src`. REST and WS signing match TS byte for
byte; positions, aggregate balance and history paging are at parity.
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
- Medium: reconnects have no backoff (`lib.rs:407`); a refused connection loops at
  network speed across futures, private and every ticker shard.
- Medium: one unparsable catalog/ticker row fails the whole list and retries every 5 s
  forever (`lib.rs:453,477,570,587`; strict `Pair`/`Daily` in `wire.rs:118-143`); a
  tickers failure also blocks the private start (`lib.rs:541`). TS skips such rows.
- Medium: one unmappable positions/orders row keeps the previous snapshot forever
  (`account.rs:347,409,436,446`), e.g. a closed position stays displayed; TS skips it.
- Medium: every WS position/order/tpsl event bumps the snapshot generation and the
  in-flight REST read is discarded (`account.rs:466,791,1051`), while WS position
  events do not update positions; under steady activity positions never commit.
- Medium: history failures return success with empty or partial rows
  (`history.rs:526,554`); TS surfaces an error. Partial income ledgers look complete.
- Medium: one failed `batch_order` chunk fails the command and drops ids already
  placed by earlier chunks (`trading.rs:1225,1249-1343`); TS returns placed ids.
- Medium: account-read failures (auth, IP whitelist) are logged at Warning and retried
  at ~1 Hz (`account.rs:446`, `lib.rs:577`): no toast, account never loads.
- Medium (unverified venue behaviour): leverage serialized as `30.0`
  (`trading.rs:344`, TS sends an integer); replacement after a virtual-leg cancel
  requires order detail to be final immediately (`trading.rs:1437`); a login
  rejection without `data` is ignored (`account.rs:744`).
- Low: no clock sync or signature-rejection (10007) resend (`sign.rs:73`); rejected
  login retries every 2 s on the same socket (`account.rs:766`); private watchdog arms
  only on `op:"pong"` (`lib.rs:394`); `tealstreetWsRoute` not honoured; book kept
  while disconnected; secondary read keys dropped; `data: null` envelopes treated as
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
  failed order reads no longer wipe orders; fills dedupe by `tradeId`; history
  respects from/to; funding time from `nft`; decimal depth precision in topics;
  unsubscribes actually sent.

### OKX
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
  order, duplicate algo cancels. Shared by both: reduce-only conditional sent
  without `reduceOnly` and un-inverted `posSide` in hedge mode.

### Hyperliquid
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
  spot vault signs as vault (TS as master) — confirm; ~~more terminal statuses~~
  (Rust treats every `*Canceled`/`*Rejected` as terminal, a superset of TS);
  ~~`stopLoss: 0` rejects whole order~~ (0 means no bracket on every market, as
  TS); ~~signed fees~~ (deliberate: TS `abs` mislabels rebates); ~~unified
  balance spot slices not populated~~ (spot holdings publish as `coins`;
  currencies stay unset because TS's `currencies.USDC` is a synthetic sizing
  slice, not the raw spot row); fills lack execution id, realized PnL and fee
  (engine `Fill`; claude-i2upd7).

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
  Fixed: a well-formed row of an unsupported type keeps its last known state (or is
  reported as not shown) while other orders commit; malformed rows still preserve
  the whole last-good snapshot. Hyro stays strict.
- Low: ~~OHLCV from+limit~~ (a limit asks for the newest bars up to `to`, as
  TS); no market refresh (needs an engine refresh command); ~~Hyro market filter;
  subscription breadth~~ (decided: USDT perpetuals only, Hyro's own rule; foreign
  categories are dropped by row, bandwidth only); split-lot error handling (RISKY:
  one unformattable lot rejects the batch before writes, where TS skips
  that order); ~~classic upnl; inverse reference price~~ (Rust is the
  same or better: live upnl on every account type, row mark before tickers);
  demo key hint (open: needs a signed probe of the other host on a 10003);
  ~~recvWindow~~ (decided: capped at 20 s); rate limiter (RISKY: send timing);
  Hyro early pings (unproven; check supervisor staleness first).
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
  One unmappable REST row still prevents good-peer positions/orders refresh
  (`account.rs::account_item`). Public-startup integration restores the prior
  whole scope because the incoming all-unmappable-only guard could delete owned
  exposure/protection when another peer mapped. Identity-preserving row merges
  remain pending in the private-row follow-up; valid private stream peers can
  still apply independently.
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
  (`rust.exchange.ts:945`) and frame mode strips `fetchedLeverage`~~; command
  rejections don't emit the error toast (one-error rule); ~~`privateOnly`/
  `externalPublicData` ignored~~ (publicFeed policy; in-process source served, #1441); `getAccount` missing for ~18 venues; dispose
  mid-command reports a placed order as failed (`host.mjs:83`); base
  `cancelAllOrders`/`nuke` read the 250 ms-lagged TS store; fills lack
  realizedPnl/fee; `fetchOrders`/`fetchPositions`/`removeOrders` don't reach the
  engine (needs a resync command); `validateAccount` leaves a started engine.
- Low: ~~Asterdex unsupported interval mapped to 1h breaks candle matching~~
  (typed `Interval` refuses it);
  ~~`num::adjust` with step ≤ 0~~ (a market without two positive finite steps is
  a malformed catalogue row on every venue; the guard stays as defence in depth);
  ~~no command deadline, `execute` drops the batch tail
  on an unknown action~~ (wasm host: 120 s deadline with unknown write outcome,
  `hostcommand.test.mjs`; batch tail kept, "unknown actions preserve batch tail
  completions"; the undeployed native host has no command deadline); candles drop `lastSide`, orders lack generic `triggerType`.

### Kraken
Futures matches TS on nearly every input; no high regression.
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
  [RUST_LIVE_GATE.md](RUST_LIVE_GATE.md)); one bad row ~~rejects
  catalogue/tickers~~ (skipped since the startup lane) and still rejects open
  orders (row isolation: SnapshotBuilder batch 3, claude-nfncln); missing
  funding field set to 0 (decided: Kraken omits a zero rate; b778fa3); ~~raw error
  text~~ (non-2xx failures carry the body's error code); spot 3m candles
  (decided: typed `Interval` refuses rather than serving 1m bars as 3m).
- Confirm (TS bugs Rust fixed): index trigger signal; full_fill not canceled;
  live % change/volume/OI; stop amend racing a fill; spot fills over-reported
  and mislabelled XBT/BTC.

### Orderly and TxFlow (already default to Rust)
TxFlow loses nothing (master TS was a placeholder).
- ~~High (Orderly, all six brokers): standalone SL/TP rejected because the app always sends `triggerPrice: price` (`orderly/src/trading.rs:362`); master TS ignored `triggerPrice`. Chart-drag stops, stop form and hotkeys all fail.~~ Matching protective prices are accepted and covered by app-shaped regressions.
- ~~Medium: cancel-all only loops symbols in the store instead of the unscoped DELETE (`rust.exchange.ts` has no Orderly override; engine supports `symbol: ''`); SL/TP blocked when a trailing stop or quantity TP_SL exists on that side (`trading.rs:254`).~~ All broker aliases cancel globally; independent trailing/quantity protections are preserved, and mixed-parent contradictions fail before cancellation. See [verification](RUST_ORDER_RACES.md).
- Low: ~~one bad market row or unknown order row fails closed~~ (stale:
  discovery skips malformed rows; order snapshot coverage retains rejected
  roots and commits valid peers); ~~trailing-stop display price~~ (maps native
  extreme/callback value or rate using order side, for REST and WS);
  ~~cancels skipped for unknown/finished ids~~ (unknown IDs reject explicitly;
  finished IDs are an intentional no-op. **RISKY** to infer normal/algo endpoint,
  symbol or sibling repair from an uncached ID; keep reconciled-order preflight);
  ~~candle `from`/`to` filtering a latest-N fetch~~ (bounded reads use native
  `/v1/tv/kline_history`, validate parallel arrays and filter exact bounds).

### KuCoin
- High: ~~cancel-symbol also deletes the symbol's TP/SL stops
  (`kucoin/src/trading.rs:515`; TS only `/api/v3/orders`)~~ (stop-book deletion
  now only on account-wide cancel); one failed or
  malformed markets/tickers read stops the venue (startup lane), ~~and futures
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
  key (602/401/402/406/70x or HTTP 401/403) now fails startup instead of retrying.
- Low: ~~every plan order marked reduce-only~~ (explicit native flag or closing
  side; dedicated position stops stay reduce-only); ~~`fetchOrderBook` refused
  by the adapter~~ (independent public depth snapshot with captured contract
  scaling, cumulative base totals and exact version string).
  **RISKY — retained:** signed fill fee. Taking absolute value without verified
  debit/rebate conventions can turn rebates into charges; keep native sign.

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
  Fills accept `id`/`tradeId`/`fillId`; an absent identity field rejects the query,
  while an empty identity string is still skipped. Inventing a fill identity is
  unsafe; the empty-string handling remains a limitation. Income with neither
  `asset` nor its `coin` alias rejects the affected page (range walks continue
  with an explicit incomplete marker); assuming USDT without accounting evidence
  can mislabel value.

### Coinbase (already defaults to Rust)
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

- High: ~~position `initial_margin_fraction` ("20.00" = 5x) read in basis points
  (`lighter/src/account.rs:424,877`), so leverage reads as the market max and a
  margin-mode change signs max leverage (`trading.rs:567`).~~ Position-only
  ratio/percentage/tick normalization now has REST/WS and 5x signing regressions.
- Medium: ~~windowed fill history pages the whole account history then fails
  (`history.rs:404,744`)~~ (newest-first paging stops past `from`); one bad market/account row stalls startup or sync;
  ~~account tier not exposed and `changeAccountTier` missing~~ (shared
  `Command::Venue` action; Lighter implements fetch/set tier).
- Low: thinner error mapping (rate-limit guidance, 21613 refresh); reduce-only
  re-check races a fixed 2 s timer. ~~Shared with TS: amend re-places the full
  original size after a partial fill.~~ Rust now re-places the observed remaining
  size for price-only amendments; explicit replacement quantities are preserved.

### WOO
No confirmed highs; valid data matches TS.
- Medium: ~~algo child rows don't inherit the parent's side/status/reduceOnly, so
  fills can be lost (`woo/src/account.rs:700`; verify one live frame); one bad
  position row freezes positions (`account.rs:749,949`); one unmappable
  fill/order-history/instrument/ticker row fails the read or startup.~~
  Children inherit missing parent fields (snapshot and stream); bad rows are
  skipped keeping stored copies; hidden exposure on unknown markets warns once.
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
  ~~snapshot starvation under load (revision discard, `account.rs:581,1310`)~~;
  malformed snapshot never sets loaded.
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
- Low: caller client id rejected on stops/brackets/splits; float remaining size;
  zero-price candle rejects batch; leverage always sent. Unchecked: private
  socket via Tealstreet proxy.
- Confirm (TS bugs Rust fixed): TS values positions at an all-zero placeholder
  ticker on startup, showing a fabricated loss (private tape: balance 1300 / upnl
  -300 while the real quote equals the 60000 entry and native PnL is 0) until its
  next balance fetch; Rust uses reported PnL until a real price arrives.

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
  venue (~~Paradex logs at Warning~~; now Error with a native regression;
  verification: a rejected TP no longer drops the queued SL, and wrong-side
  brackets reject before any write instead of placing a bare entry).
- Batches that fail midway must return placed ids with the error (~~Paradex~~,
  ~~Binance COIN-M~~, ~~Bitrue cancels~~ repaired; see their venue sections).
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
  row freezes positions~~; auth failures invisible.

### Registration and options (all RUST_VENUES aliases)
Alias→product mapping, credentials, testnet, routing and broker/builder
attribution are correct for all 48 names (~~BloFin `brokerId` aside~~; repaired
with its production/proxy gate).
- Medium: ~~Coinbase accounts with a saved read-only key fail to start — no
  `secondaryReadHeaders` entry or `read_credentials` (`transport/engine.ts:76,116`,
  `hosts/wasm/src/lib.rs:656`); Coinbase is Rust-only.~~ Read keys sign REST
  reads after a same-portfolio `key_permissions` check.
- Low/medium: polling controls ignored — `extra.tickInterval` (incl. `Infinity`
  to stop polling), the server slowdown multiplier and credential-rejection
  backoff reach only BreakoutProp (and raw tickInterval to Orderly, where
  `Infinity` becomes `null`) (`rust.exchange.ts:308,512`); every other venue polls
  a fixed constant, so the rate-limit incident lever does nothing on Rust.

### Backpack
Signing, order payloads (incl. app stop shapes), amend, cancel and positions match.
- Medium: ~~spot markets dropped — 88 SPOT pairs vanish and open spot orders
  can't be seen or cancelled (`backpack/src/lib.rs:543`); decided: retire
  Backpack spot (remove any spot surface, no port).~~ No spot surface remains in
  the Rust crate. ~~A mark-price failure or one bad ticker row
  blocks startup (`lib.rs:211,787`).~~ Bad rows skipped; startup proceeds on last
  price until marks load.
- Low: ~~one bad fill row fails fill history (`history.rs:312`).~~ Verify live:
  open-order rows with `"quantity": null`.

### BreakoutProp (disabled in production)
App stop payloads accepted; amend stricter than TS.
- High if re-enabled: ~~one unexpected portfolio row (XBTUSD lot, terminal-placed
  order type, multi-leg order) rejects the whole snapshot
  (`breakoutprop/src/account.rs:159,226,261`, `lib.rs:920`)~~ (such rows are
  skipped and logged; off-catalogue position margin still counts).
