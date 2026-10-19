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

## Worker boundary (browser)

Main is unchanged. The Rust worker emits exactly the `StoreWorkerPayload` ops
and `StoreData` shapes that TS emits today (`store`, `store:delta`,
`tickerDeltas`, `listenOrderBook[:delta]`, `listenOHLCV`, `fill`, ...). That
includes the update-id contract and the cloning rules in
`packages/safe-cex/src/webworker/CLAUDE.md`.

- Crossings between Rust and JS happen per store flush (the existing 250 ms
  throttle) and per subscribed book or candle delta, never per websocket
  message. A websocket frame crosses in once, as a string or bytes.
- Phase 1 keeps the TS `ExchangeWorker` shell (chasers, TWAP, CLI, handlers) and
  swaps only the exchange and Store behind it, through one narrow typed API.
  Porting the shell comes after the venues.

## Correctness gate

Recorded traffic (websocket frames, REST responses, user commands, timestamps)
is replayed through TS and Rust. The final `StoreData`, the emitted
order-book/candle deltas and the outbound REST/WS requests must match after
normalisation: numbers compared as numbers, map order ignored. A known TS bug
is fixed in Rust and listed in the PR. The test is the same outcome, not
byte-identical behaviour.

## Performance gate

On the same recording, in a worker-equivalent harness:

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
