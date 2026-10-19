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

Event   = WsOpened | WsMessage(&str | &[u8]) | WsClosed
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
| 3 | `TICKERS` (replace) | `u32 n`; `u32 market[n]`; 11 f64 columns in ticker field order |
| 4 | `TICKER_DELTAS` | f64 `[ticker, mask, ...changed values]` repeated |
| 5 | `BOOK` | `u32 market`, `u32 bids`, `u32 asks`, `u32 0`; f64 rows `[price, amount, total]` for bids then asks |
| 6 | `CANDLES` | `u32 n`; `u32 market[n]`; `u32 interval[n]` (string); f64 rows `[time, open, high, low, close, volume]` |
| 7 | `ORDERS` (replace) | `u32 n`; `u32 strings[n][3]` (id, clientOrderId, parentId); `u32 market[n]`; `u32 enums[n]`; f64 columns: price, amount, filled, remaining, lastUpdate, takeProfit, stopLoss; optional trailing f64 column remainingNative (NaN when absent per row) |
| 8 | `POSITIONS` (replace) | `u32 n`; `u32 market[n]`; `u32 enums[n]`; f64 columns: entryPrice, notional, leverage, unrealizedPnl, realizedPnl, contracts, liquidationPrice, marginSize, lastUpdate |
| 9 | `BALANCE` | f64 `[used, free, total, upnl]` |
| 10 | `STATUS` | `u32 loaded` bits, `u32` flags (bit 0: hedge mode), f64 latency |
| 11 | `FILLS` | `u32 n`; `u32 strings[n]` (order id); `u32 market[n]`; `u32 side[n]`; f64 rows `[price, amount]`, one row per execution |
| 12 | `SETTINGS` (replace) | `u32 n`; `u32 market[n]`; `u32 marginType[n]` (0 unknown); f64 `leverage[n]`; optional appended f64 `buyLeverage[n]`, `sellLeverage[n]` |

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

- A websocket frame crosses into the engine once, as text; an HTTP body once,
  as bytes. Nothing else crosses per message.
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
  wasm-bindgen, an explicit combined-venue size budget in `rust.yml`, committed public fixtures
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
| MEXC | done (#1304); conditional/trailing parity check pending |
| BloFin | done (#1300) |
| Backpack | done (#1299) |
| Bitget, Hyperliquid, Bitunix | wave 1, PRs in review |
| Engine transport routing (Tealstreet proxy and tunnel for venues that require them) | lane starting |
| KuCoin, Phemex | wave 2 lanes running |
| Engine performance: batched message delivery, per-venue wasm builds | lane running |
| WOO, Lighter, Paradex, GRVT, AsterDEX, Orderly family, ApeX Omni, Kraken, Coinbase, Bitrue, Polymarket, BreakoutProp, TxFlow, Binance spot and inverse | wave 2, as wave 1 lanes merge |

### 9. Delete TypeScript and merge

- Each venue that passes both gates and ships has its TS implementation
  deleted in the same PR.
- After the last venue: delete `BaseExchange`, the TS Store and the object
  payload paths, then merge `feat/rust-rewrite` into master once.

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
`packages/safe-cex/src/exchanges/` is deleted once its Rust port ships, and
the shell shrinks as its pieces move into the engine.

## Branching

All PRs target `feat/rust-rewrite`. Master receives one merge when the browser
build is ready to ship. Nothing reaches master or the tealstreet-next mirror
before then.
