# safe-cex Rust rewrite

One Rust exchange engine. It runs as WebAssembly in the browser worker and in
Cloudflare Workers, and natively on servers. It replaces TypeScript safe-cex
exchange by exchange. It must use less CPU and less memory than TS on the same
traffic, or it does not merge.

This replaces the earlier port, which reimplemented JavaScript semantics in Rust
(`safe-cex-exchange/js_runtime` and the 29 venue crates built on it). That code
is deleted on this branch.

## Targets

| Target | Build | Host provides |
| --- | --- | --- |
| Browser web worker | `wasm32-unknown-unknown`, wasm-bindgen | `WebSocket`, `fetch`, `setTimeout`, `postMessage` |
| Cloudflare Worker / Durable Object | `wasm32-unknown-unknown`, workers-rs | Workers `WebSocket`, `fetch`, alarms |
| Server (exchange-server, CLI) | native, tokio | tungstenite, reqwest/hyper, tokio timers |

Constraints shared by every target: single-threaded, no blocking, and a memory
budget that fits a 128 MB Workers isolate with headroom.

## Architecture: sans-I/O core

The engine performs no I/O. It is a deterministic state machine:

```
engine.handle(Event) -> appends Actions

Event   = WsOpened | WsMessage(&str) | WsBinary(&[u8]) | WsClosed
        | HttpResponse { id, status, body }
        | TimerFired(id) | Command(PlaceOrder | Cancel | Subscribe | ...)
        | Now(ms)
Action  = WsConnect | WsSend | WsClose
        | HttpRequest { id, method, url, headers, body }
        | SetTimer { id, at_ms } | ClearTimer(id)
        | StoreChanged | Emit(event)
```

- There is no async runtime, no futures and no `Rc<RefCell>` graph in the core.
  Hosts run the event loop: a thin layer per target, a few hundred lines each.
- Time comes in as an event. Replaying recorded traffic is exact and needs no
  fake clock.
- HTTP effects may carry optional `direct` / `publicCors` route hints. Hosts
  consume them as routing metadata and never send them to the venue. Public
  CORS endpoints come from host/provider policy; unconfigured native fallback
  fails explicitly. Ordinary requests retain account routing.
- Signing is a plain function call inside the core (`safe-cex-crypto`). The key
  stays wherever the engine runs. A host that delegates signing returns the
  signature as an event.

## Data model

- Wire messages decode with serde into borrowed, venue-specific structs
  (`&str`/`Cow` fields, numeric strings parsed straight to `f64`). There is no
  intermediate `Value` tree anywhere in engine or venue code.
- Symbols are interned to a `u32` market index. Tickers live in a `Vec` indexed
  by it, with a per-field dirty mask.
- Order books are sorted `Vec<(price, amount)>` per side, keyed per market.
- The Store holds typed state (`sdk-rs/engine/src/model.rs`) and change
  tracking; it is written out only as flush frames.

## Data shapes: the flush frame

Every data shape that leaves the engine is redesigned for transfer, not for
JavaScript object graphs. Clients (web main thread, mobile, server consumers)
are updated to read these shapes; nothing keeps the old object arrays alive.

The goal is maximum performance with no loss to what the user sees: full book
depth, every change, and the same or faster update cadence. A shape change is
acceptable only when it carries the same information at least as fast.

Once per flush (250 ms in the browser) the engine writes **one frame**: a
single `ArrayBuffer`, transferred to its consumer with zero copies. Consumers
read numbers through typed-array views over that buffer and decode strings only
when they display them. There are no per-record JS objects on either side.

### Layout

Little-endian. The header and every section payload start 8-byte aligned so
`Float64Array` views need no copies.

| Bytes | Field |
| --- | --- |
| 4 | magic `SCXF` |
| 2 | format (1) |
| 2 | section count |
| 8 | engine time (ms, f64) |

Each section: `u16 kind`, `u16 reserved`, `u32 payload bytes` (padded to 8),
then the payload. Absent optional numbers are `NaN`; absent strings are
`0xFFFFFFFF`. Strings are indices into the frame's `STRINGS` section.

| Kind | Section | Payload |
| --- | --- | --- |
| 1 | `STRINGS` | `u32 n`, `u32 offsets[n + 1]`, UTF-8 bytes |
| 2 | `MARKETS` (replace) | `u32 n`; `u32 strings[n][4]` (id, symbol, base, quote); `u32 flags[n]`; f64 columns: precisionAmount, precisionPrice, minAmount, maxAmount, minLeverage, maxLeverage, minPrice, contractSize, takerFee, makerFee |
| 3 | `TICKERS` (replace) | `u32 n`; `u32 market[n]`; 11 f64 columns in ticker field order; optional appended `u32 symbol[n]`, `u32 id[n]` string references |
| 4 | `TICKER_DELTAS` | f64 `[ticker, mask, ...changed values]` repeated |
| 5 | `BOOK` | `u32 market`, `u32 bids`, `u32 asks`, `u32 0`; f64 rows `[price, amount, total]` for bids then asks |
| 6 | `CANDLES` | `u32 n`; `u32 market[n]`; `u32 interval[n]` (string); f64 rows `[time, open, high, low, close, volume]` |
| 7 | `ORDERS` (replace) | `u32 n`; `u32 strings[n][3]` (id, clientOrderId, parentId); `u32 market[n]`; `u32 enums[n]`; f64 columns: price, amount, filled, remaining, lastUpdate, takeProfit, stopLoss; optional trailing f64 column remainingNative (NaN when absent per row) |
| 8 | `POSITIONS` (replace) | `u32 n`; `u32 market[n]`; `u32 enums[n]`; f64 columns: entryPrice, notional, leverage, unrealizedPnl, realizedPnl, contracts, liquidationPrice, marginSize, lastUpdate; optional appended u32 position-ID string references |
| 9 | `BALANCE` | f64 `[used, free, total, upnl]` |
| 10 | `STATUS` | `u32 loaded` bits, `u32` flags (bit 0: hedge mode), f64 latency |
| 11 | `FILLS` | `u32 n`; `u32 strings[n]` (order id); `u32 market[n]`; `u32 side[n]`; f64 rows `[price, amount]`, one row per execution |
| 12 | `SETTINGS` (replace) | `u32 n`; `u32 market[n]`; `u32 marginType[n]` (0 unknown); f64 `leverage[n]`; optional appended f64 `buyLeverage[n]`, `sellLeverage[n]` |
| 13 | `PREDICTION_MARKETS` (optional replace with MARKETS) | `u32 n`; `u32 market[n]`; `u32 strings[n][6]` (conditionId, outcome, question, description, endDate, slug); `u32 flags[n]` (bit 0 negRisk); aligned f64 tickSize[n] |
| 14 | `PREDICTION_HOLDINGS` (optional replace with POSITIONS) | `u32 n`; `u32 market[n]`; `u32 flags[n]` (bit 0 redeemable, bit 1 mergeable) |

Prediction venues map each outcome token to one ordinary market, with shares
as base size and collateral as quote. Optional typed metadata sections preserve
condition/outcome identity, display text and signing constraints without generic
JSON blobs or fields on every venue's market/position record. They accompany
MARKETS/POSITIONS replacements; missing sections clear that replacement's old
metadata, while unrelated updates leave it intact. A new market table invalidates
both indexed metadata tables. Holding flags survive updates for the same outcome,
and are removed when its holding closes; authoritative holdings snapshots replace
them explicitly. Other venues retain every existing section's byte layout.

`TICKERS` appends identity string references only when a ticker has no current
market row. Those rows retain market index `0xFFFFFFFF`; consumers use the tail
to preserve their native symbol and ID. The eleven numeric columns and packed
delta row indexes retain their existing offsets. No market is synthesized.

Venues with independent hedge leverage append f64 `buyLeverage[n]` and
`sellLeverage[n]` columns to `SETTINGS`. Section length distinguishes this
optional extension; older frames use `leverage` for both rails. Venues with
one leverage keep the original payload unchanged.

A market index is a row of the most recent `MARKETS` section; a ticker index is
a row of the most recent `TICKERS` section. `POSITIONS` holds open positions
only; per-market leverage and margin type live in `SETTINGS`, replacing the
zero-size placeholder positions TS kept for every market. SETTINGS may append
independent buy/sell leverage columns; absent/non-finite side values fall back
to the scalar leverage, so older frames remain readable. An order's packed
enum word carries its reject reason (flags bits 4-5). Enum codes and flag bits are
defined once in `sdk-rs/engine/src/wire.rs` and mirrored by the decoder in
`hosts/wasm/js/wire.mjs`.

### Crossing rules

- A websocket frame crosses into the engine once, as text or borrowed binary
  bytes for explicitly negotiated binary protocols; an HTTP body once, as bytes.
  Nothing else crosses per message. Paradex negotiates SBE schema1 version2;
  its typed codec lives in the venue, and hosts preserve the application bytes.
- Frames cross out once per flush. A section appears only when its data
  changed; an idle flush writes no frame.
- Logs and command results are rare and stay JSON.
- Phase 1 keeps the TS `ExchangeWorker` shell (chasers, TWAP, CLI, handlers)
  and swaps the exchange and Store behind it. The shell reads the same frame.

## Correctness gate

Recorded traffic (websocket frames, REST responses, user commands, timestamps)
is replayed through TS and Rust. The final `StoreData`, the emitted
order-book/candle deltas and the outbound REST/WS requests must match after
normalisation: numbers compared as numbers, map order ignored. A known TS bug
is fixed in Rust and listed in the PR. The test is the same outcome, not
byte-identical behaviour.

## Performance gate

On the same recording, in a worker-equivalent harness, TS posting its objects
as it does today against Rust transferring frames:

- CPU time: Rust < TS.
- Peak and steady memory (Wasm linear memory plus JS heap retained by the
  worker): Rust < TS.

Every venue PR states both numbers. A PR that loses on either does not merge.

## Banned (CI fails)

- Any `Js*` value type, JS number/string emulation (`Vec<u16>` strings,
  `ToString`-of-number reproduction), or emulated promises, microtasks or
  timers.
- Axios- or BaseExchange-shaped facades and per-class transcription of TS
  method bodies.
- `serde_json::Value` on a hot path.
- Dependencies on the deleted crates.

## Reused crates

`safe-cex-crypto` survived the audit, trimmed to signing primitives (HMAC,
SHA, ed25519, EIP-712, ES256, StarkNet, zkLink, Lighter). Every other crate of
the earlier port failed the bans and is deleted.

## Plan

Each phase ends with a commit on `feat/rust-rewrite` and green CI. Status is
kept current in this section.

### 1. Clean slate — done

- Old port deleted: `js_runtime`, the 29 transcribed venue crates, the JS
  emulation crates, their scripts, tests and CI jobs.
- `safe-cex-crypto` audited and trimmed. CI bans enforce the rules above.

### 2. Core and Binance public data — done

- Sans-I/O engine, typed Store, streaming JSON decode, flush frame writer and
  its JS reader (`wire.mjs`), browser host (`host.mjs`).
- Binance USD-M public data: markets, tickers, full-depth books with
  snapshot/diff sync and ban-safe backoff, candles.
- Harness: live tape recorder, TS and Rust replay, correctness and
  performance gate. On a 5-minute, 10-book tape: identical output, 15.2x less
  worker CPU, 1.4x less retained memory.

### 3. Binance USD-M private data and trading — done (live testnet tape pending keys)

Engine side, all inside `safe-cex-venue-binance`:

- Signing: HMAC-SHA256 over the query string, `timestamp` corrected by the
  server-time offset, `recvWindow`.
- Private startup: balance and positions (`/fapi/v3/account`), position mode,
  open orders and open algo orders, then the user data stream (listenKey
  create, 30-minute keepalive, reconnect on `listenKeyExpired`).
- Stream events: `ORDER_TRADE_UPDATE`, `ALGO_UPDATE`, `ACCOUNT_UPDATE`,
  `ACCOUNT_CONFIG_UPDATE` into ORDERS, POSITIONS, BALANCE and FILLS sections.
- Commands: place (batch of 5, algo endpoint for conditional types), amend,
  cancel (batch), cancel all for a symbol, leverage, margin type, position
  mode. Optimistic `creating`/`canceling` states go out in the next frame;
  results and venue errors come back as `CommandDone`.
- Rare request/response calls (order history, fills history, income, closed
  PnL, public trades) are commands whose result is JSON in `CommandDone`;
  they never touch the frame.
- Reconciliation: a periodic REST resync of orders and positions, paused
  under rate limits exactly like depth snapshots.

Correctness: a private tape built from Binance's documented payloads
(`harness/private-tape.mjs`: startup reads, order, algo and account events,
place, cancel and cancel-all commands) replays through TS and Rust with real
signing and must produce the same orders, positions, balance, settings, fills
and command results, with every request matching a taped one. A live private
tape is recorded on Binance testnet once test keys are available.

Large REST lists stream into the engine element by element
(`Cx::request_streamed`), so a startup response never sits whole in wasm
memory.

### 4. Hosts

Account-backed hosts reuse safe-cex's TS transport provider and routing resolver.
Venues emit real URLs and explicit public/private/business socket roles. Stable
feed identities isolate parallel sockets sharing a role and URL; protocol-health
recovery hints let hosts prefer a declared fallback. The JS
host applies proxy prefixes and per-operation authorization, package HOW/relay,
and sticky fallback selection. Workers provide a header-capable raw socket
constructor to those same protocol clients. Native launchers consume the
serialized TS `resolveEngineRoutingPlan` output through
`NetworkTransport::with_routing`, with a context JWT callback; no native venue
routing table or credentials are embedded in the policy. Disposal and deadlines
cancel token acquisition before a stale write can be sent.

- Browser worker: `hosts/wasm` + `host.mjs` — done.
- Native: `hosts/native` (`safe-cex-native`), tokio + tokio-tungstenite + reqwest over a
  pluggable transport, driving the same engine — done (`examples/watch.rs`
  runs it live; `tests/host.rs` drives the loop from a tape).
- Cloudflare: the same wasm build and `host.mjs` inside a Durable Object —
  done (`js/workers.mjs`: a Workers transport over upgrade `fetch` and
  streaming bodies, and `createEngineObject`, which runs one account's engine
  for its connected clients, streams frames to them as binary messages and
  drops the engine and its credentials when the last client leaves). Tested
  against fake Workers primitives; a deployed Worker is not set up yet.
- Host tests: each host runs recorded traffic through its own loop and must
  reach the engine state a direct replay reaches.

### 5. Worker shell integration — done in the SDK

- `ExchangeOptions.engine: 'rust'` plus a venue in `RUST_VENUES` makes
  `ExchangeWorker` build a `RustEngineExchange` (`exchanges/rust/`) instead of
  the TS venue. Every flush frame updates its `DefaultStore` through
  `FrameApplier` (`store/frameApplier.ts`), so chasers, TWAP, the CLI and
  post-processors read the store they always have; trading, history and
  subscriptions are engine commands.
- Realms register how to load the wasm engine (`loadWebEngine`,
  `loadNodeEngine`), as they register the transport.
- With `__workerConfig.acceptsFrames`, the worker posts each frame to main
  (`op: 'frame'`, transferred) and store payloads carry only worker-derived
  fields. Books, candles and fills flush at the venue's book tick, store
  sections at 250 ms. Without it, accounts on the engine post objects through
  the existing handlers.

### 6. Client adoption (tealstreet-next) — after the master merge

tealstreet-next receives the SDK only through Copybara once master takes this
branch, so the app change happens then. It is small because the store stays
the seam: register `loadWebEngine` in `StoreWorkerEntry.ts`, push
`acceptsFrames` and set `engine: 'rust'` behind a flag, and add `case 'frame':
exchange.applyWorkerFrame(data)` to `exchangeAccountController`'s worker
listener. Readers of `exchange.store` and the `listenOrderBook` consumers stay
as they are; zero-copy book consumers (`Float64Array` rows into the order
book, DOM and depth chart) come after that. How the web build obtains the
compiled wasm is decided then.

Then: exchange-server moves to the native host and streams the same frames
(with `BOOK_DELTA` sections for networked clients — full books every flush
are ~450 KB/s for two 1,000-level books).

### 7. CI — Binance, Bybit, Backpack, OKX, Crypto.com, WEEX and BingX

- Bans, fmt, clippy, native tests, wasm32 build and clippy.
- Replay gate (`rust.yml`, job "replay gate"): release wasm with the pinned
  wasm-bindgen, an explicit per-venue size budget in `rust.yml`, committed public fixtures
  (`harness/fixtures/`) gated on identical output and lower CPU and memory,
  and each venue's generated private tape gated on identical account state.

### 8. Remaining venues — in progress

One Codex lane per venue, each in its own aimux worktree on
`feat/rust-<venue>`, working from `docs/plans/VENUE_PORT_BRIEF.md`: plan and
execute in phases with sub-agent reviews, then a PR into `feat/rust-rewrite`
reviewed adversarially until it passes, then merged. The overseer delegates,
compacts lanes at checkpoints, watches gates and keeps this list current.

| Venue | State |
| --- | --- |
| Binance USD-M, Binance PM | done |
| Bybit linear | done ([#1298](https://github.com/Tealstreet/tealstreet-sdk/pull/1298)) |
| Bybit inverse, spot, demo; HyroTrader profile | port complete ([#1307](https://github.com/Tealstreet/tealstreet-sdk/pull/1307)); Hyro live connectivity unverified |
| OKX (+EU, demo) | done (#1302) |
| Crypto.com | done (#1303) |
| WEEX | done (#1301) |
| BingX | done (#1305) |
| MEXC | done (#1304); conditional/trailing parity checked (#1311) |
| BloFin | done (#1300) |
| Hyperliquid perps | done (#1306) |
| Backpack | done (#1299) |
| Bitget (USDT-M, USDC, inverse) | done (#1308); inverse sizing and UTA ledger/trailing parity done (#1317) |
| Bitrue futures | done (#1314) |
| Bitunix | wave 1, PR in review |
| Engine transport routing (Tealstreet proxy and tunnel for venues that require them) | done ([#1313](https://github.com/Tealstreet/tealstreet-sdk/pull/1313)) |
| WOO | done ([#1316](https://github.com/Tealstreet/tealstreet-sdk/pull/1316)) |
| ApeX Omni perpetuals | Implemented; [PR #1324](https://github.com/Tealstreet/tealstreet-sdk/pull/1324), native zkLink/API-key signing; all local gates and independent code-correctness re-reviews pass |
| KuCoin, Phemex, Orderly family | wave 2 lanes running |
| Engine performance: batched message delivery, per-venue wasm builds | lane running |
| Paradex perpetuals | Implemented; [PR #1320](https://github.com/Tealstreet/tealstreet-sdk/pull/1320), public/private gates PASS; plan: `RUST_PARADEX.md` |
| AsterDEX perpetuals | implemented; [PR #1322](https://github.com/Tealstreet/tealstreet-sdk/pull/1322), all local gates passing |
| Lighter perps | implemented; [PR #1319](https://github.com/Tealstreet/tealstreet-sdk/pull/1319), adversarial review fixes accepted; CI pending |
| Polymarket CLOB V2 | implemented; [PR #1327](https://github.com/Tealstreet/tealstreet-sdk/pull/1327) into `feat/rust-rewrite`; local gates and phased reviews pass; prediction mapping, scoped benchmark and progress in `venue-ports/POLYMARKET.md` |
| Coinbase (CFM, spot, legacy INTX; coinbasespot) | ported; [PR #1325](https://github.com/Tealstreet/tealstreet-sdk/pull/1325), all local gates and correctness reviews pass; native JWT and measurements in [RUST_COINBASE.md](RUST_COINBASE.md) |
| Kraken futures | implemented; [PR #1323](https://github.com/Tealstreet/tealstreet-sdk/pull/1323), all local gates pass; code-correctness reviews accepted (`KRAKEN_RUST_PORT.md`) |
| GRVT, BreakoutProp, TxFlow, Binance spot and inverse | next, as lanes free up |

### 9. Prove consumer cutover, then delete TypeScript — readiness work

**Do not delete TS venues in the readiness change.** A venue replay gate proves
its protocol and performance; deletion also requires the SDK's consumers to
work with its frames. The implementation/checkpoints are in
[RUST_CUTOVER_READINESS.md](RUST_CUTOVER_READINESS.md). The exhaustive
[caller inventory and deletion dependencies](RUST_CUTOVER_CALLERS.md) lists
each direct `BaseExchange`/venue/registry importer across `packages/` and
`apps/`, including type-only callers, tests and dynamic reference loaders.

The SDK has 49 registered names: 39 in `RUST_VENUES`, eight already defaulting
to Rust (Coinbase's two and Orderly's six), 31 with opt-in Rust adapters but
TS defaults, and 10 without a Rust registration. `createExchange` now honors
explicit `engine: 'rust'`, as `ExchangeWorker` does; changing the default
registries remains a separate cutover decision.

#### Consumer evidence required for every registered Rust alias

`packages/safe-cex/src/__tests__/cutover/*.wasm.test.ts`, selected by
`SAFE_CEX_WASM_TESTS=1`, enumerates `RUST_VENUES` directly:

- Real combined-WASM venue startup uses recorded public responses/native
  documented-schema inputs through `RustEngineExchange` and the actual
  `ExchangeWorker` frame mode. Main-thread transfer and late-consumer snapshots
  must preserve nonempty markets and tickers, including spot/inverse identity.
- An independent binary format-1 corpus tests private/lifecycle edge cases
  through the real host/worker/adapter shell, against ordinary TS store inputs.
  This corpus is an explicit frame-boundary oracle, not a private venue emulator.
  It checks split ticker deltas, orders/positions and empty-section clearing,
  balance, leverage rails, IDs, books/candles/fills, terminal-order opt-in,
  update-ID bumps, and `serialize`/stub parity including derived fields.
- Actual post-processors, CLI monitor/positions/orders reads, chaser fill
  arithmetic and supervisor notifications, and TWAP limit-price selection must
  agree with the TS store consumer contract. Preserve native quantity extras:
  the readiness audit found and repaired OKX's missing native fill bridge.
  Main-thread orders also reconstruct worker-derived chaser tags through the
  public order mutation path, preserving update-ID observers in either frame/
  derived-payload arrival order.
  Do not infer terminality from a frame's `Closed` enum: some triggered orders
  are deliberately reopenable unless explicitly marked final.

These tests do not certify private live connectivity, optional account modes,
or downstream tealstreet-next deployment. Native venue/private replay and
performance matrices remain required alongside consumer conformance.

#### Per-venue deletion readiness

All rows retain TS/reference code in this change. “Opt-in” means the consumer
suite covers the named adapter, not that every TS product/account option is
ported. The linked inventory records exact callers and product constraints.

| Names | Readiness / remaining deletion prerequisites |
| --- | --- |
| `coinbase`, `coinbasespot` | Rust defaults; CFM/spot and saved spot-only covered. Retired INTX is fixture compatibility only. Keep frozen TS reference and inert Starbase evidence separate. |
| `tealstreet-dex`, `kodiakfi`, `mode`, `salsadex`, `what`, `woofi` | Rust defaults, one broker-configured Orderly crate. Keep frozen reference until its replay oracle is replaced deliberately. |
| `binance`, `binancepm` | Opt-in; preserve shared modules for unported spot/COIN-M siblings; prove default account/routing modes before deletion. |
| `bybit`, `bybitinverse`, `bybitspot`, `bybitdemo`, `hyrotrader` | Opt-in; combined inverse/options toggles and semantic instrument classification remain blockers. Hyro live connectivity unverified. Preserve region/Hyro policy helpers. |
| `bitget`, `bitgetusdc`, `bitgetinverse` | Opt-in; keep inverse/native sizing, UTA ledger/trailing and account-mode evidence; extract shared transport constants. |
| `okx`, `okxeu`, `okxdemo`, `okxeudemo` | Opt-in; native chaser bridge repaired. Rust filters linear USDT/USDC markets; TS inverse and EU product coverage need explicit resolution before deletion. Extract broker constants. |
| `apexomni` | Opt-in; preserve native zkLink/API-key and consumer/protocol evidence. |
| `asterdex` | Opt-in; retain V1/V3, builder/STP, public wallet/approval exports and secondary-read-key policy. |
| `backpack` | Opt-in; perpetuals and uint32 decimal IDs, no spot claim. |
| `bingx` | Opt-in; default/routing/account-option migration and reference retention required. |
| `blofin` | Opt-in; native units and shared broker constants must survive deletion. |
| `bitrue` | Opt-in; futures, no implicit spot coverage. |
| `cryptocom` | Opt-in; preserve account/product option and routing gates. |
| `kucoin` | Opt-in futures; preserve native units/broker options and shared files still needed by spot. |
| `hyperliquid` | Opt-in perps/HIP-3; spot absent, wallet/builder/agent and caller-ID helpers still imported. |
| `kraken` | Opt-in futures; spot remains TS. |
| `lighter` | Opt-in; retain wallet/account-tier exports; broker alias and remote-signing modes unresolved. |
| `mexc` | Opt-in; preserve native sizing and conditional/trailing evidence. |
| `paradex` | Opt-in; retain binary feed, native signing and routing evidence. |
| `phemex` | Opt-in; retain account identity/native protocol and broker helper. |
| `weex` | Opt-in; preserve host/header/routing evidence. |
| `woo` | Opt-in WOO X; separate from Orderly WOOFi; keep position-history adapter. |
| `polymarket` | Opt-in prediction CLOB V2 with typed outcome/holding metadata; local-wallet signature types 0/1/2/3, supplied/derived L2 credentials, limit/GTC and final-fill-safe cancel-replace. No caller client IDs, market/conditional/bracket/reduce-only orders or generic history/fills/income/public-trades query coverage; price history is bounded sampled data. Preserve [exact port scope and fixture/live boundaries](venue-ports/POLYMARKET.md) and default-factory migration evidence. |
| `binanceinverse`, `binancespot`, `bitunix`, `breakoutprop`, `grvt`, `hyperliquidspot`, `krakenspot`, `kucoinspot`, `lighter-robinhood`, `txflow` | No registered Rust adapter. Port each product/broker or make an explicit supported-product retirement decision; do not silently alias it to a different product. |

#### Known consumer blockers and replacement order

The source audit found additional gates that must stay visible before default
cutover or deletion:

- `privateOnly` does not currently suppress Rust public sockets, and Rust does
  not initialize/use the TS `externalPublicData` provider. Replace these with
  explicit shared-public-feed policy and test private-worker/provider fan-out.
- Rust frames carry typed Polymarket prediction/holding metadata, reconstructed
  into its specific `info` fields; they do not carry arbitrary native `info`.
  Bybit's instrument classifier still lacks `info.symbolType`, so CLI TradFi
  preflight returns unknown and no longer enforces the existing refusal. Carry
  typed semantic classification
  through frames and test the real preflight before changing those defaults.
- `safe-cex-server`'s `PublicExchangeFactory` creates without starting an
  exchange, then calls `fetchMarkets`; Rust currently returns its store. Cold
  starts are empty and forced refresh does not refresh native metadata. Define
  and test explicit startup/refresh semantics before adopting this factory.

Deletion proceeds in separate, reviewable PRs:

1. Close missing products/account modes and the blockers above, then prove
   saved-account CLI, MCP, public factory, native/mobile loading and hosted/object
   paths. Switch ready names in **both** registries; keep factory APIs stable.
2. Extract the neutral exchange interface, callable-method allowlist,
   capability branding, broker/site/routing constants and exported wallet
   utilities. `RustEngineExchange` still extends `BaseExchange`; deleting that
   base now would delete the working Rust adapter's shell too.
3. Remove independent ready implementations first, after freezing/replacing
   the replay oracle. Remove Bybit/Hyro, OKX and Bitget subclasses as family
   units. Keep Binance, KuCoin, Kraken, Hyperliquid and Lighter shared modules
   while unported siblings still depend on them. The inventory maps every
   caller to its interface, host, shared-helper or test-reference replacement.
4. Migrate test/harness imports deliberately. Preserve independent documented
   protocol fixtures and a reviewed performance baseline; never compare Rust
   with itself and call it TS parity.
5. Remove TS-only Axios/signing/socket scaffolding only after its last runtime
   **and reference** caller is gone. Keep `DefaultStore`, chasers, TWAP,
   post-processors and object fallback while real consumers need them. Replace
   each contract before removing its implementation; the store is the current
   compatibility seam, not dead code.
6. Merge `feat/rust-rewrite` into master only after phase 6 adoption/build and
   every cutover gate passes. This readiness PR targets `feat/rust-rewrite`.

### Blockers that need Sam

- Binance testnet (or live test-account) API keys for the live private tape.
  The documented-payload tape does not need them.

## Repository layout

```
sdk-rs/                   Rust workspace
  engine/                 safe-cex-engine: the sans-I/O core, store, frame
  crypto/                 safe-cex-crypto: signing primitives
  hosts/wasm/             safe-cex-wasm + js/ (host, frame reader, Workers
                          host); the @tealstreet/safe-cex-wasm package
  hosts/native/           safe-cex-native: tokio host
  venues/<venue>/         safe-cex-venue-<venue>, one crate per venue
  harness/  scripts/
packages/                 TypeScript
```

TypeScript lives on beside it: `packages/safe-cex` keeps the shell (worker,
store, chasers, TWAP, CLI, types) and reaches Rust only through
`@tealstreet/safe-cex-wasm`. Each venue under
`packages/safe-cex/src/exchanges/` is deleted only after its Rust port and the
phase-9 consumer/dependency gates pass, and the shell shrinks as its contracts
receive tested replacements.

## Branching

All PRs target `feat/rust-rewrite`. Master receives one merge when the browser
build is ready to ship. Nothing reaches master or the tealstreet-next mirror
before then.

### Optional collateral balances

A `BALANCE` payload may append `u32 count`, `u32 coin[count]` string references,
8-byte padding, then `f64 amount[count]` and `f64 notional[count]` after its four
scalar values. A legacy 32-byte payload leaves coin balances unspecified; an
appended zero count explicitly clears them. Bitget inverse frames carry native
order amount/filled and display remaining; their position notional is the native
contract quantity. The worker adapter retains these native quantities in the
existing TS `extra` fields alongside coin balances.

The collateral tail may itself append a spot currency table: `u32 count`, `u32 flags`,
`u32 currency[count]` string references, 8-byte padding, then f64 columns
`total`, `free`, `used`, `upnl`. The optional table preserves reserved quantities;
an absent table leaves currencies unspecified, while an appended zero count
clears them. Flag bit 0 leaves collateral unspecified, preserving its independent
absent-versus-empty contract. Writers without collateral insert an empty collateral table before
this extension. Existing collateral-only and 32-byte balance frames retain their
layout.
