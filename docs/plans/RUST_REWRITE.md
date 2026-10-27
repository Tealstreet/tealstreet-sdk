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
- Live polling intervals arrive as typed host events; null pauses the WASM
  control, and positive infinity pauses native polling. The host can forward
  incident slowdown changes without rebuilding an account.
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

Each section: `u16 kind`, `u16 revision`, `u32 payload bytes` (padded to 8),
then the payload. Absent optional numbers are `NaN`; absent strings are
`0xFFFFFFFF`. Strings are indices into the frame's `STRINGS` section.

| Kind | Section | Payload |
| --- | --- | --- |
| 1 | `STRINGS` | `u32 n`, `u32 offsets[n + 1]`, UTF-8 bytes |
| 2 | `MARKETS` (replace) | `u32 n`; `u32 strings[n][4]` (id, symbol, base, quote); `u32 flags[n]` (bits 0-5 active, inverse, spot, perpetual, expiring, read-only; bits 8-11 quantity units: base, native placement, inverse native, contract count, inverse contract-sized); f64 columns: precisionAmount, precisionPrice, minAmount, maxAmount, minLeverage, maxLeverage, minPrice, contractSize, takerFee, makerFee |
| 3 | `TICKERS` (replace) | `u32 n`; `u32 market[n]`; 11 f64 columns in ticker field order; optional appended `u32 symbol[n]`, `u32 id[n]` string references |
| 4 | `TICKER_DELTAS` | f64 `[ticker, mask, ...changed values]` repeated |
| 5 | `BOOK` | `u32 market`, `u32 bids`, `u32 asks`, `u32 0`; f64 rows `[price, amount, total]` for bids then asks |
| 6 | `CANDLES` | `u32 n`; `u32 market[n]`; `u32 interval[n]` (string); f64 rows `[time, open, high, low, close, volume]` |
| 7 | `ORDERS` (replace) | `u32 n`; `u32 strings[n][3]` (id, clientOrderId, parentId); `u32 market[n]`; `u32 enums[n]`; f64 columns: price, amount, filled, remaining, lastUpdate, takeProfit, stopLoss; optional trailing f64 columns remainingNative, then filledInPlacementUnits (NaN when absent per row) |
| 8 | `POSITIONS` (replace) | `u32 n`; `u32 market[n]`; `u32 enums[n]`; f64 columns: entryPrice, notional, leverage, unrealizedPnl, realizedPnl, contracts, liquidationPrice, marginSize, lastUpdate; optional appended u32 position-ID string references |
| 9 | `BALANCE` | f64 `[used, free, total, upnl]` |
| 10 | `STATUS` | `u32 loaded` bits, `u32` flags (bit 0: hedge mode), f64 latency |
| 11 | `FILLS` | `u32 n`; `u32 strings[n]` (order id); `u32 market[n]`; `u32 side[n]`; f64 rows `[price, amount]`, one row per execution; optional appended f64 realizedPnl[n], fee[n] (NaN means unreported) |
| 12 | `SETTINGS` (replace) | `u32 n`; `u32 market[n]`; `u32 marginType[n]` (0 unknown); f64 `leverage[n]`; optional appended f64 `buyLeverage[n]`, `sellLeverage[n]` |
| 13 | `PREDICTION_MARKETS` (optional replace with MARKETS) | `u32 n`; `u32 market[n]`; `u32 strings[n][6]` (conditionId, outcome, question, description, endDate, slug); `u32 flags[n]` (bit 0 negRisk); aligned f64 tickSize[n] |
| 14 | `PREDICTION_HOLDINGS` (optional replace with POSITIONS) | `u32 n`; `u32 market[n]`; `u32 flags[n]` (bit 0 redeemable, bit 1 mergeable) |
| 15 | `BOOK_DELTA` | `u32 market`, `u32 bid changes`, `u32 ask changes`, `u32 0`; f64 rows `[price, amount]` for bids then asks; zero amount deletes the price |
| 16 | `INSTRUMENT_CLASSIFICATIONS` (optional replace with MARKETS) | `u32 n`; `u32 market[n]`; `u32 strings[n][3]` (native symbolType, marketRegion, underlyingTicker); absent and explicitly empty strings differ |
| 17 | `DERIVATIVE_PRODUCTS` (optional replace with MARKETS) | `u32 n`; `u32 market[n]`; `u32 kind[n]` (1 Future, 2 Call, 3 Put); `u32 strings[n][5]` (underlying, settleAsset, premiumAsset, style, settlement); aligned f64 expiry[n], strike[n]; unknown terms are absent |
| 18 | `POSITION_GREEKS` (optional replace with POSITIONS) | `u32 n`; `u32 strings[n][2]` (position ID, units); aligned f64 delta[n], gamma[n], vega[n], theta[n] |
| 19 | `CAPABILITIES` (snapshot frames only) | `u32` flags (bits 0-11: clientOrderLookup, fillHistory, orderHistory, incomeHistory, fillsByOrderId, accountLeverage, accountMarginType, cancelWithSymbols, amendWithSymbol, orderBookQuery, options, accountTier); `u32` four-bit fields: account, cancelAll, leverageQuery, maxLeverage, quantityUnits |
| 20 | `UNDERLYING_TOKENS` (optional replace with MARKETS) | `u32 n`; `u32 market[n]`; `u32 strings[n][2]` (venue token name, optional full name): the token behind a wrapped pair, e.g. Hyperliquid `UBTC`/`Unit Bitcoin` behind `BTC/USDC`; applied as `market.info.hlTokenName`/`hlTokenFullName` |
| 21 | `ACCOUNT_READ` (optional marker) | Empty payload; a current private-state read, rather than a new streaming observation. Apply values without advancing order/position freshness IDs; the normal stream retains its pending changes. |

The envelope remains FORMAT 1; native `sections()` and JS reject other formats.
Section revision 0 preserves the layouts above. Revision 1 has explicit fixed
extensions (all absent optional numbers are NaN and strings are `0xFFFFFFFF`):

- MARKETS appends `u32 prettySymbol[n]` references, preserving absent and empty.
- TICKERS appends f64 `midPrice[n]` after the eleven original columns, then
  `u32 symbol[n]`, `u32 id[n]`. TICKER_DELTAS bit 11 updates/clears midpoint;
  the first eleven bits retain their meanings.
- ORDERS appends four f64 columns: remainingNative, filledInPlacementUnits,
  remainingNotional, conditionalOrderPrice; then `u32 bracketTriggers[n]`
  (TP low byte, SL next byte, zero absent, trigger enum plus one), and
  `u32 symbol[n]`.
- POSITIONS appends mandatory `u32 positionId[n]`, then `u32 symbol[n]`.
- FILLS appends `u32 symbol[n]` after its execution rows, then pads to 8 bytes.
  If any execution reports financial metadata, append f64 realizedPnl[n] and
  fee[n] after that padding; NaN means unreported. Symbol-only revision-one
  frames preserve their existing bytes, and readers retain legacy revision-zero
  layouts with or without financial columns.
- SETTINGS appends mandatory f64 buyLeverage/sellLeverage columns, then
  `u32 symbol[n]`.

FILLS now writes revision 2: the unchanged numeric rows and revision-1 symbol
refs are followed by `u32 executionId[n]` string refs, then aligned f64
`realizedPnl[n]` and `fee[n]`. Missing IDs use `0xFFFFFFFF`; missing financials
use NaN. Readers retain revision 0/1 support, including revision 1 both without
financials and with the optional length-detected realized-PnL/fee tails from
#1430. Revision 2 is accepted only for FILLS. SDK events use the execution ID
when reported, falling back to the order ID, and keep `orderId` separately.

Writers use revision 0 for MARKETS without display labels and TICKERS/DELTAS
without midpoint fields. Unknown enum values decode as absent. Unknown section
kinds can be skipped; unsupported revisions of known kinds require a snapshot.
Symbol tails preserve rows on absent markets without synthesizing markets.
Settings retain symbol identity across table changes; market-only flushes wait
until pending MARKETS is delivered. TickerState owns its baseline, including
identity strings, so transferring source frames cannot detach its state.

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
is replayed through TS and Rust. The `StoreData` at startup, fixed tape quarters, every
command and terminal checkpoints, the emitted order-book/candle deltas and the outbound REST/WS requests must match after
normalisation: numbers compared as numbers, map order ignored. A known TS bug
is fixed in Rust and listed in the PR. The test is the same outcome, not
byte-identical behaviour.

## Performance gate

On the same recording, in a worker-equivalent harness, TS posting its objects
as it does today against Rust transferring frames:

- At least seven fresh interleaved public pairs with alternating first runner.
- Main/steady/process CPU and idle-loaded retained memory: Rust medians
  clearly below TS, except that small main/steady CPU windows only need to be
  within run-to-run noise of TS. The exact thresholds, pair counts and their
  rationale live only in `sdk-rs/harness/performance-policy.mjs`.
  Overseer decision (2026-10-21): small windows trade the improvement margin
  for a noise floor (#1461; evidence in `RUST_GATE_TIMING.md`); revisit if a
  small-window regression slips through. Include cold implementation loading/WASM compilation in total CPU;
  exclude audit callbacks, checkpoint reconstruction and baseline collection.
- Retention counts JS heap/buffer deltas plus WASM growth above its idle-loaded
  reservation; never clamp releases to zero. Each measured run must reproduce
  its implementation's audited output and transport receipt.
- Shipped combined WASM: at most 14,155,776 bytes (13.5 MiB). Standalone builds
  use the 1,343,488-byte family budget (RUST_WASM_FAMILIES.md). Check browser and Node artifacts before overwrite.

Peak memory and browser cold-start remain cutover requirements; this replay
gate measures idle-loaded retained memory rather than establishing either.

Every venue PR states the measurements and incomplete evidence. A venue port
that loses does not merge. Gate-infrastructure changes may land explicit,
tape-hash-bound known failures in `harness/known-gate-failures.json`; they retain
strict FAIL and do not certify venue parity. New or invalid evidence fails CI.

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

Each phase ends with a commit on `master` and green CI. Status is
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

Then: exchange-server moves to the native host and streams the same frames.
The first step is the native frame server (plan and acceptance checkpoints in
`RUST_FRAME_SERVER_PROGRESS.md`). Deployment and application adoption remain
separate from this engine/transport change.

#### Native frame server migration

The mirror's `packages/safe-cex-server` is a library, not the deployed
exchange-server application. Its current exports are:

- `RedisCandleCacheBackend`: adapts a caller-supplied Redis/Valkey client to
  the SDK key-value candle cache; prefers SCAN, with KEYS as a fallback.
- `PublicExchangeFactory`: owns a public-only exchange per venue, hydrates
  market stores from quantized UTC-hour snapshots, serializes refreshes with
  an in-process lock, and falls back to the latest valid cached snapshot.
  Market snapshots and their validated latest pointers have three-hour TTLs.
  Ticker-cache helpers provide a 150-second bootstrap seed with a 300-second
  TTL; these seeds are not executable quotes.
- `PublicdataTickerBridge`: filters explicitly requested Binance/Bybit
  capabilities and fresh connected health, loads market indexes, and maps
  the publicdata service's JSON ticker snapshots/deltas into SDK ticker state.
  An empty eligible set does not subscribe to all venues.

The native host replaces the per-account venue/Store execution loop, outbound
venue REST/WebSockets/timers, and repeated serialization of live account and
book state. A configured account owns one engine independently of its client
connections. Its authenticated clients receive a binary `snapshot_frame`
first, then ordered SCXF flush frames; commands and their correlated results
remain typed JSON. An account token grants full command access to that account;
client-supplied venue credentials and account creation are outside this protocol.
The embedding application supplies account engines, routing and token policy.
The runnable example uses Binance; the server API accepts other venue engines.

`BOOK_DELTA` carries only changed `[price, amount]` levels since the previous
flush (zero deletes). Full `BOOK` replaces the baseline on subscription,
resnapshot/gap, or market identity changes. Local WASM consumers retain full
books by default. The engine's `book-deltas` feature is enabled by the native
host; default WASM builds omit the producer and baseline state while retaining
the shared wire decoder. Network consumers reconstruct sorted levels and cumulative
totals. A new connection first establishes the current flush boundary before
its snapshot, so snapshot interleaving cannot omit a later reverted level.
A slow client is disconnected instead of dropping frames; reconnect starts
with a fresh snapshot. A lost connection is a stream gap, never permission to
apply more deltas against stale state.

For now TS retains Redis persistence, hourly refresh/stale-fallback policy,
bootstrap ticker caches, the publicdata capability/health adapter, and the
worker shell's chasers, TWAP, CLI, post-processors and derived store fields.
Application session authorization, TLS termination, credential provisioning,
process supervision and public routing remain deployment responsibilities.
No cache export or existing hosted-account protocol is removed in this step.
`packages/safe-cex-server` now exposes `ServerSessionFactory` and
`NativeFrameSession` for application session routes. Explicit Rust selection
resolves an authorized, preconfigured native account and relays binary SCXF
snapshots/flushes unchanged, then routes native/WASM JSON commands and their
client-scoped results. It constructs no TS exchange for that session and never
falls back after native failure. A client uses a fresh `FrameApplier` per
connection; ordered async delivery is bounded, and overflow or a stream gap
closes the session instead of dropping deltas. AbortSignal covers client
disconnect during resolution/connection/delivery. The explicit TypeScript path
returns the existing PublicExchangeFactory handle unchanged; its startup,
hourly refresh and cache policies remain owned by the consumer-blockers work.
See `packages/safe-cex-server/src/frameSession/README.md` and the committed
`RUST_SERVER_ADAPTER_PROGRESS.md` plan/gates. This mirror contains the library,
not the deployed application: session-route adoption and preservation of worker
shell behavior/cache hydration remain application work before its old worker
execution path can stop. Browser
adoption is still the separate master/Copybara step above.

Bandwidth is measured with a deterministic two-book fixture, without a load
test. Report depth per side, number of changed levels, flush cadence, full and
delta SCXF bytes, WebSocket framing and initial/resync snapshots separately;
compression and TCP/TLS/IP overhead are not inferred from payload bytes.
The deterministic Rust writer/JS reader check (`yarn rust:test-frame-wire`)
measures two books with 1,000 levels **per side**, changing ten levels per side
per book at four flushes/second: full SCXF frames are 96,064 bytes and deltas
704 bytes, a 99.267% payload reduction (384,256 versus 2,816 B/s). With one
unmasked WebSocket message per frame the totals are 384,296 versus 2,832 B/s.
A book-only initial/resync snapshot still costs 96,064 bytes; market and account
sections add their current serialized size. Change counts and cadence are
fixture assumptions, not a live-feed bandwidth forecast.

### 7. CI — Binance, Bybit, Backpack, OKX, Crypto.com, WEEX and BingX

- Bans, fmt, clippy, native tests, wasm32 build and clippy.
- Replay gate (`rust.yml`, job "replay gate"): release wasm with the pinned
  wasm-bindgen, an explicit per-venue size budget in `rust.yml`, committed public fixtures
  (`harness/fixtures/`) gated on identical output and lower CPU and memory,
  and each venue's generated private tape gated on identical account state.

The stricter gate-honesty matrix currently requires 32 suites / 110 exact cases
([coverage contract](../../sdk-rs/harness/ci-gate-suites.mjs)). Its single
[`known-gate-failures.json`](../../sdk-rs/harness/known-gate-failures.json)
list records exact venue/tape/diagnostic failures; accepted known failures stay
FAIL and do not certify parity. Port states below describe implementation and
historical checks, not a claim that every stricter replay case passes.

### 8. Port inventory and historical receipts

The original venue lanes used independent worktrees, phased plans/reviews and
PRs into `feat/rust-rewrite`. Their port receipts below are historical evidence,
not an active lane queue or a claim of complete strict private parity. At
`b6cb612bc7c`, the replay catalog contains 50 aliases and browser delivery has
28 families. Current cutover/deletion obligations remain in the caller audit;
the [plan index](README.md) retains the implementation evidence.

| Venue | State |
| --- | --- |
| Binance USD-M, Binance PM | done; regression gates retained by variants lane |
| Binance Spot, COIN-M inverse | port complete in existing Binance crate ([#1326](https://github.com/Tealstreet/tealstreet-sdk/pull/1326)); all four Binance profiles and fresh Hyro regression gates pass |
| Bybit linear | done ([#1298](https://github.com/Tealstreet/tealstreet-sdk/pull/1298)) |
| Bybit inverse, spot, demo; HyroTrader profile | port complete ([#1307](https://github.com/Tealstreet/tealstreet-sdk/pull/1307)); Hyro live connectivity unverified |
| OKX (+EU, demo) | done (#1302) |
| Crypto.com | done (#1303) |
| WEEX | done (#1301) |
| BingX | done (#1305) |
| MEXC | done (#1304); conditional/trailing parity checked (#1311) |
| BloFin | done (#1300) |
| Hyperliquid perps | done (#1306) |
| Hyperliquid Spot, Kraken Spot | verified opt-in products in existing crates ([PR #1339](https://github.com/Tealstreet/tealstreet-sdk/pull/1339)); gates 1–6, independent correctness reviews and all-47-alias consumer checks pass ([spot plan](RUST_SPOT_PRODUCTS.md)); live public/documented private evidence, no TS venue deletion |
| Backpack | done (#1299) |
| Bitget (USDT-M, USDC, inverse) | done (#1308); inverse sizing and UTA ledger/trailing parity done (#1317) |
| Bitrue futures | done (#1314); cache/open-order client-ID lookup parity implemented and gated ([evidence](RUST_PARITY_GAPS.md)); TS has no complete order/PnL/funding histories or hedge/margin mode changes |
| Bitunix | port merged [#1309](https://github.com/Tealstreet/tealstreet-sdk/pull/1309), `3acf9e46392`; later repair evidence is indexed separately |
| Engine transport routing (Tealstreet proxy and tunnel for venues that require them) | done ([#1313](https://github.com/Tealstreet/tealstreet-sdk/pull/1313)) |
| WOO | done ([#1316](https://github.com/Tealstreet/tealstreet-sdk/pull/1316)) |
| ApeX Omni perpetuals | Implemented; [PR #1324](https://github.com/Tealstreet/tealstreet-sdk/pull/1324), native zkLink/API-key signing; all local gates and independent code-correctness re-reviews pass |
| KuCoin, Phemex, Orderly family | ports merged [#1312](https://github.com/Tealstreet/tealstreet-sdk/pull/1312) (`373cc098643`), [#1310](https://github.com/Tealstreet/tealstreet-sdk/pull/1310) (`45d7a3fb988`) and [#1318](https://github.com/Tealstreet/tealstreet-sdk/pull/1318) (`f66bdc26f9a`); port completion is separate from cutover/parity |
| KuCoin spot | Implemented and gated: HF spot public/private/trading, caller IDs and supported queries/settings; [scope and evidence](RUST_PARITY_GAPS.md) |
| Engine artifact isolation and performance experiments | family delivery merged [#1393](https://github.com/Tealstreet/tealstreet-sdk/pull/1393), `cb371229da1`; the earlier combined proposal [#1344](https://github.com/Tealstreet/tealstreet-sdk/pull/1344) closed without merging. Current artifact/profile evidence is in [the size audit](RUST_WASM_ARTIFACT_SIZE.md); [GRVT profiling](GRVT_RUST_PERF.md) remains historical evidence |
| GRVT perpetuals | implemented; [PR #1321](https://github.com/Tealstreet/tealstreet-sdk/pull/1321), all local gates passing; [non-perpetual product decision](GRVT_PRODUCTS_DECISION.md) retires unexposed mapper-only Future/Call/Put paths |
| Paradex perpetuals | Implemented; [PR #1320](https://github.com/Tealstreet/tealstreet-sdk/pull/1320), public/private gates PASS; remote/payload signing implemented and gated ([scope and evidence](RUST_PARITY_GAPS.md)) |
| AsterDEX perpetuals | implemented; [PR #1322](https://github.com/Tealstreet/tealstreet-sdk/pull/1322), all local gates passing |
| Lighter perps | port merged [#1319](https://github.com/Tealstreet/tealstreet-sdk/pull/1319), `31ec401f601`; later production and replay repairs remain separately recorded |
| Polymarket CLOB V2 | implemented; [PR #1327](https://github.com/Tealstreet/tealstreet-sdk/pull/1327) into `feat/rust-rewrite`; local gates and phased reviews pass; prediction mapping, scoped benchmark and progress in `venue-ports/POLYMARKET.md` |
| Coinbase (CFM, spot, legacy INTX; coinbasespot) | ported; [PR #1325](https://github.com/Tealstreet/tealstreet-sdk/pull/1325), all local gates and correctness reviews pass; native JWT and measurements in [RUST_COINBASE.md](RUST_COINBASE.md) |
| Kraken futures | implemented; [PR #1323](https://github.com/Tealstreet/tealstreet-sdk/pull/1323), all local gates pass; code-correctness reviews accepted (`KRAKEN_RUST_PORT.md`) |
| TxFlow testnet perpetuals | implemented Rust default; [PR #1329](https://github.com/Tealstreet/tealstreet-sdk/pull/1329), all local gates and independent code-correctness reviews pass; signing, frozen TS reference and measurements in `TXFLOW_RUST_PORT.md` |
| BreakoutProp legacy DXtrade | implemented; [PR #1328](https://github.com/Tealstreet/tealstreet-sdk/pull/1328), documented public/private gates PASS (scripted public fixture accepted by Sam); live capture needs Sam's legacy DXtrade credentials (`RUST_BREAKOUTPROP.md`) |

### 9. Prove consumer cutover, then delete TypeScript — readiness work

**Do not delete TS venues in the readiness change.** A venue replay gate proves
its protocol and performance; deletion also requires the SDK's consumers to
work with its frames. The implementation/checkpoints are in
[RUST_CUTOVER_READINESS.md](RUST_CUTOVER_READINESS.md). The exhaustive
[caller inventory and deletion dependencies](RUST_CUTOVER_CALLERS.md) lists
each direct `BaseExchange`/venue/registry importer across `packages/` and
`apps/`, including type-only callers, tests and dynamic reference loaders.

The SDK has 49 registered names: 48 in `RUST_VENUES`, nine already defaulting
to Rust (Coinbase's two, Orderly's six and TxFlow testnet), 39 with opt-in Rust adapters but
TS defaults, and one without a Rust registration. `createExchange` now honors
explicit `engine: 'rust'`, as `ExchangeWorker` does; changing the default
registries remains a separate cutover decision.

#### Consumer evidence required for every registered Rust alias

`packages/safe-cex/src/__tests__/cutover/*.wasm.test.ts`, selected by
`SAFE_CEX_WASM_TESTS=1`, enumerates `RUST_VENUES` directly:

- Real combined-WASM venue startup uses recorded public responses/native
  documented-schema inputs through `RustEngineExchange` and the actual
  `ExchangeWorker` frame mode. Main-thread transfer and late-consumer snapshots
  must preserve nonempty markets and tickers, including spot/inverse identity.
  This includes HTTP-polling venues: BreakoutProp consumes documented
  authenticated session/account/quote fixtures without a venue WebSocket. Its
  scripted fixture is not live credential-capture/connectivity proof.
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
or downstream tealstreet-next deployment. Generic per-alias algorithm fixtures
prove the shared frame/store contract; they do not enable venue capabilities.
Polymarket remains development-only (`enabled: false`, `enabledInDev: true`),
with `supportsChaser: false` and native 1x leverage rails. BreakoutProp also has
`supportsChaser: false`; its generic algorithm inputs do not enable chasers.
Native venue/private replay and performance matrices remain required alongside
consumer conformance.

#### Per-venue deletion readiness

All rows retain TS/reference code in this change. “Opt-in” means the consumer
suite covers the named adapter, not that every TS product/account option is
ported. The linked inventory records exact callers and product constraints.

| Names | Readiness / remaining deletion prerequisites |
| --- | --- |
| `coinbase`, `coinbasespot` | Rust defaults; CFM/spot and saved spot-only covered. Retired INTX is fixture compatibility only. Keep frozen TS reference and inert Starbase evidence separate. |
| `tealstreet-dex`, `kodiakfi`, `mode`, `salsadex`, `what`, `woofi` | Rust defaults, one broker-configured Orderly crate. Keep frozen reference until its replay oracle is replaced deliberately. |
| `txflow` | Rust default for testnet USDC perpetuals; mainnet rejects before I/O, production/preview availability remains disabled. Both registries use Rust; retain the independent TS replay/audit reference under `__tests__/reference/txflow` and the production `txflow.client-id` helper. Generic consumer proofs do not enable measurement-gated order controls; preserve the [exact protocol/query scope and gate evidence](TXFLOW_RUST_PORT.md). |
| `binance`, `binancepm` | Opt-in USD-M/PM; preserve their account/query/routing behavior and regression gates while the shared crate now also supplies Spot/COIN-M. Shared TS/oracle removal still requires all callers/defaults to migrate. |
| `binancespot` | Opt-in Spot; signed user-data WebSocket API ACK/liveness, native currency balances, no derivative positions/settings, native client-ID order identity and final-fill-aware cancel/replace. Preserve supported regular/protective/order-list preflight; private history/fills/income retain TS empty results, public-trades/client-lookup unsupported. Live public and documented private evidence: [variant plan](BINANCE_VARIANTS_PORT.md). |
| `binanceinverse` | Opt-in COIN-M; native contract quantities, USD-per-contract multiplier, base position quantities and coin-denominated PnL/margin; worker/main native-count/quote-notional bridge retained. Regular/stops/trailing, identity/final-fill-safe replacement, caller client IDs rejected. Public trades implemented; private histories retain TS empty-result contract. Keep USD-M/PM controls and [exact scope/evidence](BINANCE_VARIANTS_PORT.md). |
| `bybit`, `bybitinverse`, `bybitspot`, `bybitdemo`, `hyrotrader` | Opt-in; typed native classification now survives worker/main frames and the real CLI TradFi preflight. The `bybit` linear account supports all four inverse/options flag combinations in its existing crate, with category-specific feeds, reconciliation, native quantities, option terms, chain/expiries and account/position Greeks. Plain option limit/market and limit-IV orders retain sizing/caller IDs; unsupported option controls and queries fail explicitly. Standalone inverse/spot/demo/Hyro retain their own profile boundaries; they do not inherit combined options merely from registration. Actual TS/native and documented private fixtures do not prove live combined/private connectivity; Hyro live connectivity remains unverified. Preserve region/Hyro policy helpers and remaining consumer/default blockers; [scope/evidence](RUST_PRODUCT_SEMANTICS.md). |
| `bitget`, `bitgetusdc`, `bitgetinverse` | Opt-in; keep inverse/native sizing, UTA ledger/trailing and account-mode evidence; extract shared transport constants. |
| `okx`, `okxeu`, `okxdemo`, `okxeudemo` | Opt-in; global linear/inverse swaps and EU `FUTURES` with `ruleType=xperp` share the existing crate. Preserve native placement counts, inverse base quantities at reference price, USD-valued position PnL/margin, native ledger currency, worker/main chaser sizing and account-scoped EU discovery. The parity sweep additionally preserves primary definitive lookup and secondary GET routing. Private subscriptions/reconciliation and product-scoped/joined histories retain host/demo boundaries; documented fixtures and live public captures do not prove live private units or connectivity. Defaults, external-public/account overlays, broker extraction and other consumer gates remain prerequisites; [scope/evidence](RUST_PRODUCT_SEMANTICS.md). |
| `apexomni` | Opt-in; preserve native zkLink/API-key and consumer/protocol evidence. |
| `asterdex` | Opt-in; retain V1/V3, builder/STP, public wallet/approval exports and secondary-read-key policy. |
| `backpack` | Opt-in; perpetuals and uint32 decimal IDs, no spot claim. |
| `bingx` | Opt-in; default/routing/account-option migration and reference retention required. |
| `blofin` | Opt-in; native units and shared broker constants must survive deletion. |
| `bitrue` | Opt-in; futures, no implicit spot coverage. |
| `cryptocom` | Opt-in; preserve account/product option and routing gates. |
| `kucoin` | Opt-in futures; preserve native units/broker options and shared files still needed by spot. |
| `kucoinspot` | Opt-in HF spot; native spot IDs/base units, trade-account currency balances, caller-context amendments/cancels and fixed spot settings. Keep the [supported scope and replay/SDK evidence](venue-ports/KUCOIN_SPOT.md); no futures alias. |
| `hyperliquid` | Opt-in perps/HIP-3; preserve wallet/builder/agent and caller-ID helpers. The separate spot alias does not enable combined `enableSpotMarkets` accounts. |
| `hyperliquidspot` | Opt-in spot product in the Hyperliquid crate; phase reviews, live public/documented private gates and native worker/late-consumer identity checks pass ([spot plan](RUST_SPOT_PRODUCTS.md)). TS/default migration and wallet/helper consumers remain deletion prerequisites. |
| `kraken` | Opt-in futures; preserve futures authentication and protocol gates alongside the distinct spot product. |
| `krakenspot` | Opt-in spot product in the Kraken crate, with separate REST authentication and WS v2 token state; phase reviews, live public/documented private gates and native worker/late-consumer identity checks pass ([spot plan](RUST_SPOT_PRODUCTS.md)). TS/default migration remains a deletion prerequisite. |
| `lighter`, `lighter-robinhood` | Opt-in perpetuals; four explicit main/test deployment profiles, USDC/USDG and domain-separated local signing. TS remote-only mode polls unsigned account snapshots without private WebSocket/auth/transactions; payload-only starts public, mixed local/callback credentials sign locally. No remote transaction provider exists in current TS. Retain wallet approval/account-tier exports, the API-key-index parser used by the wrapper, and frozen TS oracle; [variant scope/evidence](LIGHTER_VARIANTS_PORT.md). |
| `mexc` | Opt-in; preserve native sizing and conditional/trailing evidence. |
| `paradex` | Opt-in; retain binary feed, native signing and routing evidence. |
| `phemex` | Opt-in; retain account identity/native protocol and broker helper. |
| `weex` | Opt-in; preserve host/header/routing evidence. |
| `woo` | Opt-in WOO X; separate from Orderly WOOFi; keep position-history adapter. |
| `polymarket` | Opt-in, development-only (`enabled: false`, `enabledInDev: true`), `supportsChaser: false`, native 1x leverage. Prediction CLOB V2 with typed outcome/holding metadata; local-wallet signature types 0/1/2/3, supplied/derived L2 credentials, limit/GTC and final-fill-safe cancel-replace. No caller client IDs, market/conditional/bracket/reduce-only orders or generic history/fills/income/public-trades query coverage; price history is bounded sampled data. Preserve [exact port scope and fixture/live boundaries](venue-ports/POLYMARKET.md) and default-factory migration evidence. |
| `grvt` | Opt-in perpetuals; local exact EIP-712 chains 325/326, session/API-key/private routing, subaccount discovery, mandatory builder authorization and decimal caller IDs 1..2^63-1. Preserve one-way/account leverage, protective/bracket and safe cancel-replace, bounded query/client-lookup and chaser fill-verification contracts. Margin-mode changes unsupported. [Explicit product decision](GRVT_PRODUCTS_DECISION.md) retires unexposed mapper-only Future/Call/Put paths: TS discovery defaults to perpetuals, explicit non-perpetual public catalogs were empty, and generic dispatch does not establish their lifecycle support. Keep TS/reference sources and public builder-wallet exports; [GRVT scope/evidence](GRVT_RUST_PORT.md). |
| `breakoutprop` | Opt-in, legacy DXtrade and development-only; authenticated REST polling with username/password and exact selected FULL_TRADING account even for public-data reads. LIMIT/MARKET/last-price STOP, GTC and proven zero-fill cancel-replace; one-way/cross and estimated margin/PnL. Caller IDs, brackets/close-all/settings writes unsupported; generic history/fills/income queries retain branded inert behavior, public trades unsupported. Quote books/live candles are synthetic. Documented public fixture waiver is explicit; live credential capture/connectivity remains unverified, and new Breakout Terminal accounts are outside scope. Preserve [scope/evidence](RUST_BREAKOUTPROP.md). |
| `bitunix` | Opt-in USDT futures; streamed discovery/ticker seeds, full-snapshot depth, signed private reconciliation, guarded trading and native history queries. `realizedPNL` is already net of fees and funding and remains verbatim. Live public and documented private replay are separate evidence; default cutover and credentialed lifecycle proof remain later gates. Preserve [port scope and gate evidence](RUST_BITUNIX.md). |

#### Known consumer blockers and replacement order

The source audit found additional gates that must stay visible before default
cutover or deletion:

- **Consumer-blockers implementation:** `privateOnly` and the pre-start
  suppression hook now select native Public-role suppression, with explicit
  topic guards for multiplexed account sockets. Explicit
  direct/disabled/shared feed policy and transferred-port `RustPublicFeedProvider`
  fan-out retain private metadata/settings, ingest public data natively, and
  keep shared fields authoritative across local polling. Tests cover real
  `ExchangeWorker` instances with transferred ports, native snapshots,
  identity/catalog checks and disposal.
  Rust rejects legacy TS `externalPublicData`; application/gateway owners must
  migrate to the explicit policy before changing their account defaults.
- Rust frames carry typed Polymarket prediction/holding metadata and typed
  instrument/product metadata; they do not carry arbitrary native `info`.
  Bybit's native `symbolType`, `marketRegion` and `underlyingTicker` now survive
  real discovery, worker application, buffer transfer and main application.
  The real CLI preflight refuses classified stock/ETF/commodity/FX instruments;
  empty and innovation crypto labels pass, while absent/new labels remain
  unknown. This closes that metadata gap within the documented fixture scope;
  it does not change defaults or certify live private connectivity. See
  [product semantics evidence](RUST_PRODUCT_SEMANTICS.md).
- **Public factory lifecycle repaired:** a cache miss starts Rust before
  reading metadata; a Redis hit remains a lazy metadata-only handle. Forced
  refresh starts a replacement engine to re-fetch native metadata and swaps
  `handle.exchange` after successful persistence. Failed refresh retains the
  previous good engine/catalog. Callers re-read `handle.exchange` after refresh;
  bounded cold startup, lazy recovery from failed starts, delayed cache reads,
  concurrent/hour-rollover/disposal paths have native/unit coverage. Native
  product/environment selectors also isolate current/latest cache and lock keys,
  including registry-default Rust venues. Shared catalog validation preserves
  typed classification, future and option identity while private metadata stays
  native.
- GRVT's apparent Future/Call/Put catalog gap has an [explicit retirement
  decision](GRVT_PRODUCTS_DECISION.md): TS discovery sends `{}`, which defaults
  to `PERPETUAL`; injected non-perpetual mapper rows are not currently exposed
  products. Do not infer non-perpetual capability from those generic paths or
  perpetual fixtures. Future enablement requires typed semantic metadata and
  complete product lifecycle evidence. This decision does not authorize TS
  deletion or default cutover; the other consumer/dependency gates still apply.

The consumer-blockers phase also proves saved CFM account CLI reads through the
real factory, MCP catalog/balance/positions/orders over its actual protocol,
plus store-only ticker freshness through native Binance public frames and
BreakoutProp REST polling,
isolated compiled Bun CLI loading, and isolated packaged Node MCP loading with
Rust artifacts and dependency notices. `EXCHANGE_ENGINE=rust` explicitly selects
the MCP engine. Mobile has an explicit cached host-loader registration tested
with the real browser WASM bytes and two saved accounts; **a Hermes/native-bridge
binary and device integration remain unproved**. These are deterministic offline
consumer/packaging proofs, not live account authentication or deployment claims.
See [RUST_CONSUMER_BLOCKERS.md](RUST_CONSUMER_BLOCKERS.md) for checkpoints/gates.

Additional [rewrite audit findings](RUST_AUDITS.md) remain cutover gates.
The consumer repairs and offline proofs do not settle downstream engine delivery,
hosted keyless signing, a Hermes/native bridge, full live private parity or the
production shadow/canary evidence. Keep these separate from the repaired startup,
shared-feed and package-loading boundaries documented here.

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
   units. Retire the four Binance TS profile paths only after their default
   callers and shared oracle migrate, preserving USD-M/PM gates. Keep KuCoin,
   Kraken, Hyperliquid and Lighter shared modules while unported siblings still
   depend on them. The inventory maps every
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

Credentialed lifecycle coverage uses `corepack yarn rust:live-smoke --venue
<name> [--testnet|--demo]`. Its reviewed environment profiles default to an
engine-supported testnet or demo; production-only profiles require explicit
`--mainnet`. This task was verified with scripted transports only: **no real
account was contacted**. Sam still needs to supply dedicated, funded simulation
accounts and authorize actual runs. A minimum-size, distant limit order can be
rejected by price bands, minimum notional, permissions, or account configuration;
the runner must report the rejection rather than increase exposure silently.
Use an otherwise empty dedicated account because cancel-all is account-wide.
The generic cancel-all path visits the selected symbol and any other observed
open-order symbols, matching the SDK fallback for venues without a global
endpoint. Unknown minimum-notional rules or original leverage refuse placement;
add an authoritative discovery decoder before enabling a product the runner
cannot size. Independent hedge leverage rails must match.
The runner uses the Wasm engine with the Node transport. `--symbol <symbol>`
selects a market and `--out <path>` selects an exclusive, mode-0600 tape file;
the default is `sdk-rs/harness/tapes/<name>-live-private.jsonl`. Replay a completed
recording with `corepack yarn rust:gate --venue <name> --tape <path> --runs 1
--books ''`. A failed or incomplete lifecycle is recorded as a failure and
cannot serve as live lifecycle proof. The selected Wasm build must include the
requested venue.
The gate automatically subscribes the recording's selected book. It rejects
failed/incomplete live captures. After disposal, a fresh Wasm instance reads the
account again to verify empty orders/positions and restored leverage independently
of mutation acknowledgements. These redacted reads are retained under
`live-smoke-verify-*`; their success marker is an operational check by the runner,
while the TS/Rust replay compares the primary lifecycle. A scripted Binance run
round-trips through that gate; other profiles still need credentialed proof on
their actual products. The runner applies a fixed $100 quote-notional cap to both the initial and amended
order, including inverse face value and native contract multipliers; rounding a
minimum above the cap refuses before any account mutation. No CLI or environment
option raises the cap. Limit orders request post-only on engine-supported profiles
(all currently reviewed profiles except Bitrue and Polymarket). The runner never silently upgrades size after rejection
or closes an unexpected fill. SIGINT/SIGTERM requests bounded pending-write drain
and cleanup; incomplete cleanup remains a failure.
After cancel-all, mutable leverage is lowered by one (bounded by the market
minimum) and restored to its captured value, exercising both writes. Fixed or
already-minimum leverage is set to its original value without increasing risk.

All variables below have the exact prefix `RUST_LIVE_SMOKE_`; concatenate the
family, selected mode (`TESTNET`, `DEMO`, or `MAINNET`) and suffix, for example
`RUST_LIVE_SMOKE_BINANCE_TESTNET_KEY`. Every optional field uses the same mode
prefix. Legacy names and another mode's values are never used as a fallback. Credentials are read
only from the process environment, never command-line arguments or committed
configuration. Do not print them or commit shell history, environment files, raw
requests, or unredacted recordings. The private tape retains the selected
environment's official URLs and redacts credential material before writing.
Credential fields shorter than four characters are refused before I/O because
their arbitrary echoes cannot be scrubbed without corrupting unrelated values;
short public account/API-key indices remain valid. Unrecordable returned auth
material fails the capture instead of persisting a usable credential.
Availability below describes the **current Rust engine's endpoint support**,
not every environment a venue might offer. Profiles reject arbitrary endpoint
overrides and refuse unsupported environments before reading credentials.

| Venue name(s) | Family | Required suffixes | Engine-supported nonproduction environment |
| --- | --- | --- | --- |
| `binance` | `BINANCE` | `KEY`, `SECRET` | Testnet: `demo-fapi.binance.com`, `demo-fstream.binance.com` |
| `binancepm`, `binanceinverse`, `binancespot` | — | — | Newly landed during this harness change; their distinct endpoint/auth, unit and leverage contracts have no reviewed live-smoke profile yet. Refused before credential access. USD-M `binance` remains supported. |
| `bybit`, `bybitinverse`, `bybitspot` | `BYBIT` | `KEY`, `SECRET` | Testnet: `api-testnet.bybit.com`, `stream-testnet.bybit.com` |
| `bybitdemo` | `BYBIT` | `KEY`, `SECRET` | Demo by default: `api-demo.bybit.com`, `stream-demo.bybit.com`; public market data on `stream.bybit.com`. Testnet also supported. |
| `bitget`, `bitgetusdc`, `bitgetinverse` | `BITGET` | `KEY`, `SECRET`, `PASSPHRASE` | Testnet selects `susdt-futures`, `susdc-futures`, `scoin-futures` and simulated collateral on the shared `api.bitget.com` / `ws.bitget.com` origins. UTA testnet is unsupported. |
| `orderly`, `tealstreet-dex`, `kodiakfi`, `mode`, `salsadex`, `what`, `woofi` | `ORDERLY` | `KEY`, `SECRET`, `ACCOUNT_ID` | Testnet: `testnet-api-evm.orderly.org`, separate `testnet-ws-evm.orderly.org` and `testnet-ws-private-evm.orderly.org` sockets. Key and secret use the engine's `ed25519:` format. |
| `lighter` | `LIGHTER` | `PRIVATE_KEY`, `ACCOUNT_INDEX`, `API_KEY_INDEX` | Testnet: `testnet-api.lighter.xyz`, `testnet.zklighter.elliot.ai`; indices are decimal strings. |
| `lighter-robinhood` | — | — | Broker endpoint and signing replay are covered, but credentialed lifecycle capture/sizing has no reviewed live-smoke profile. Every mode refuses before credential access. |
| `phemex` | `PHEMEX` | `KEY`, `SECRET` | Testnet: `testnet-api.phemex.com` |
| `cryptocom` | `CRYPTOCOM` | `KEY`, `SECRET` | UAT through `--testnet`: `uat-api.3ona.co`, `uat-stream.3ona.co` |
| `hyperliquid` | `HYPERLIQUID` | `SIGNER_ADDRESS`, `PRIVATE_KEY`, `ACCOUNT_ADDRESS` | Testnet: `api.hyperliquid-testnet.xyz`; optional `VAULT_ADDRESS` |
| `apexomni` | `APEXOMNI` | `KEY`, `SECRET`, `PASSPHRASE`, `OMNI_KEY_SEED` | Testnet: `testnet.omni.apex.exchange`, `qa-quote.omni.apex.exchange` |
| `asterdex` | `ASTERDEX` | `KEY`, `SECRET` | Testnet: `fapi.asterdex-testnet.com`, `fstream5.asterdex-testnet.com`. Optional V3 suffixes: `API_VERSION=v3`, `USER_ADDRESS`, `SIGNER_ADDRESS`, `PRIVATE_KEY`; generic key/secret remain required by the engine. |
| `paradex` | `PARADEX` | `PRIVATE_KEY`, `ACCOUNT_ADDRESS`, `SUBKEY_ADDRESS` | Testnet: `api.testnet.paradex.trade`, `ws.api.testnet.paradex.trade` |
| `woo` | `WOO` | `KEY`, `SECRET`, `APPLICATION_ID` | Staging through `--testnet`: `api.staging.woox.io`, `wss.staging.woox.io`; public candles use `api-pub.woox.io` |
| `blofin` | `BLOFIN` | `KEY`, `SECRET`, `PASSPHRASE` | Demo: `demo-trading-openapi.blofin.com` |
| `okx`, `okxdemo`, `okxeu`, `okxeudemo` | `OKX` | `KEY`, `SECRET`, `PASSPHRASE` | Demo: simulated-trading header, `openapi.okx.com` / `wspap.okx.com` (default TLS 443); EU `eea.okx.com` / `wseeapap.okx.com` (default TLS 443) |
| `kraken` | `KRAKEN` | `KEY`, `SECRET` | Futures demo: `demo-futures.kraken.com` |
| `coinbase`, `coinbasespot` | `COINBASE` | `KEY`, `SECRET` | None wired; explicit `--mainnet` only. CDP key name and ES256 private key PEM. |
| `backpack` | `BACKPACK` | `KEY`, `SECRET` | None wired; explicit `--mainnet` only. Base64 Ed25519 public key and seed. |
| `mexc` | `MEXC` | `KEY`, `SECRET` | None wired; explicit `--mainnet` only. |
| `bingx` | `BINGX` | `KEY`, `SECRET` | None wired; explicit `--mainnet` only. |
| `weex` | `WEEX` | `KEY`, `SECRET`, `PASSPHRASE` | None wired; explicit `--mainnet` only. WS upgrade headers enabled. |
| `bitrue` | `BITRUE` | `KEY`, `SECRET` | None wired; explicit `--mainnet` only. |
| `kucoin` | `KUCOIN` | `KEY`, `SECRET`, `PASSPHRASE` | None wired; engine explicitly rejects testnet. Explicit `--mainnet` only. Bullet-derived sockets are limited to the reviewed `ws-api-futures.kucoin.com` origin. |
| `kucoinspot` | — | — | No reviewed Spot endpoint/sizing profile; every live-smoke mode refuses before credential access. The futures profile is not reused. |
| `polymarket` | `POLYMARKET` | `PRIVATE_KEY` | None wired; explicit `--mainnet` only. Optional complete L2 tuple: `API_KEY`, `API_SECRET`, `API_PASSPHRASE`; absent tuple derives credentials with L1 auth. Optional `FUNDER_ADDRESS`, `SIGNER_TYPE` (`0`–`3`; default `0`). |
| `hyrotrader` | — | — | No reviewed built-in profile; refused, including with `--mainnet`. Simulation URLs cannot be inferred safely from caller-supplied endpoints. |
| `grvt` | — | — | Newly landed during this harness change. Engine has testnet endpoints, but its pinned replay auth/routing contract currently accepts production origins only. No reviewed live-smoke profile yet; refused before credential access. |
| `breakoutprop` | — | — | Newly landed during this harness change; no reviewed credential/environment/sizing profile yet. Refused before credential access. |

The environment table and credential option mapping live in
`sdk-rs/harness/live-smoke-venues.mjs`; add a reviewed profile before enabling
another venue or endpoint. A supported endpoint profile does not itself prove
that every lifecycle command is available for every product/account type.
Polymarket's optional `RUST_LIVE_SMOKE_POLYMARKET_MAINNET_MARKET_SLUGS` is a comma-separated
list of 1–100 distinct market slugs to bound discovery; pair it with `--symbol`
for the desired outcome. It is public selection metadata, not a credential.
Phemex PerpetualV2 sizing requires explicit `minOrderValueRv`, `qtyStepSize` and
`maxOrderQtyRq` from the matching listed, linear `/public/products` row. Missing
or malformed fields refuse before placement. Crypto.com still refuses because
quantity ticks alone do not establish the monetary floor. Bybit inverse requires an
explicit monetary floor in discovery. These profiles can read accounts but will
refuse trading before placing until their complete rules can be established.

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

All PRs target `master` (since 2026-10-27, after #2299 merged the rewrite). The
former `feat/rust-rewrite` landing branch is retired. Copybara imports master
into the tealstreet-next mirror, so every merge reaches that import.

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

