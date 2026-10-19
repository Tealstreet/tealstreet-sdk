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
  (`&str` fields, numeric strings parsed straight to `f64`). There is no
  intermediate `Value` tree on hot paths: order book, ticker, trade, order and
  position updates.
- Symbols are interned to a `u32` market index. Tickers live in a `Vec` indexed
  by it.
- Order books use integer price ticks in sorted vectors (or a B-tree where depth
  requires it), keyed per market. They are not maps of strings.
- The Store holds typed state (`safe-cex-types`) plus the monotonic `updateIds`
  the main thread depends on.

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
| 7 | `ORDERS` (replace) | `u32 n`; `u32 strings[n][3]` (id, clientOrderId, parentId); `u32 market[n]`; `u32 enums[n]`; f64 columns: price, amount, filled, remaining, lastUpdate, takeProfit, stopLoss |
| 8 | `POSITIONS` (replace) | `u32 n`; `u32 market[n]`; `u32 enums[n]`; f64 columns: entryPrice, notional, leverage, unrealizedPnl, realizedPnl, contracts, liquidationPrice, marginSize, lastUpdate |
| 9 | `BALANCE` | f64 `[used, free, total, upnl]` |
| 10 | `STATUS` | `u32 loaded` bits, `u32 0`, f64 latency |
| 11 | `FILLS` | `u32 n`; `u32 strings[n]` (order id); `u32 market[n]`; `u32 side[n]`; f64 rows `[price, amount]` |

A market index is a row of the most recent `MARKETS` section; a ticker index is
a row of the most recent `TICKERS` section. Enum codes and flag bits are
defined once in `safe-cex-engine/src/wire.rs` and mirrored by the decoder in
`safe-cex-wasm/js/wire.mjs`.

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

## Reused crates (audited before use)

`safe-cex-types`, `safe-cex-store`, `safe-cex-crypto`, `safe-cex-lighter-sdk`,
`safe-cex-transport` (wire framing only: hosts own sockets), and the precision
and rounding half of `safe-cex-num`. Anything that fails the bans above is cut
from these crates or rewritten.

## Order of work

1. Delete the old port on this branch. Audit and trim the reused crates.
2. Core: event/action types, Store, worker boundary, three hosts (browser,
   Workers, native), and the replay harness with both gates.
3. Binance USD-M end to end in the browser worker behind a per-venue switch:
   markets, tickers, order books, candles, balance, positions, orders, fills,
   place/amend/cancel. Both gates green. This sets the venue pattern.
4. The remaining venues in parallel, one PR each, copying the Binance pattern.
5. Each venue that passes both gates and ships has its TS implementation
   deleted. There is no permanent dual path.

## Branching

All PRs target `feat/rust-rewrite`. Master receives one merge when the browser
build is ready to ship. Nothing reaches master or the tealstreet-next mirror
before then.
