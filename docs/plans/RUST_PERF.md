# Engine performance

Branch: `feat/rust-perf`, based on `origin/feat/rust-rewrite`.

Work is full auto under VENUE_PORT_BRIEF Process: phase reviews, commits,
public/private replay gates, PR to feat/rust-rewrite, adversarial reviews,
green CI, merge and fresh follow-up branch. No stash/rebase/force push.

## Phases

1. Baseline and profiling. Replay committed public recordings, measure total
   CPU/memory, websocket boundary/parse-and-update time and flush frame time.
   Keep profiling optional so production measurements carry no timers.
2. Ordered host batching. Queue socket arrivals to one delivery per macrotask;
   retain socket, text and arrival time. Borrow decoded messages in Rust.
   Flush before non-message events/commands/snapshots, bound memory and preserve
   action ordering. Native host drains ordered ready messages with arrival time.
3. Venue builds. Optional WASM Cargo venue features, one package per family,
   explicit venue-selected web/node loaders, per-family CI budgets. Aliases
   share family artifacts. Remove superseded aggregate imports/artifacts.
4. Gates and landing. Merge landing frequently, run all registered venue public
   and private gates, compare same-machine before/after CPU and retained memory,
   phase/adversarial review, repository checks and green PR CI, then merge.

## Progress

- Read current brief/spec and relevant package guidance. Clean feature branch
  created from landing. Existing host crosses/drains per message; current
  replay yields per frame, which may hide real network bursts. Batching replay
  must use recorded timing fairly for TS and Rust, retain existing correctness.
- Phase work assigned to disjoint native-host and venue-build subagents.

### Initial measurements and phase review

Original aggregate build, original per-frame replay, three-run medians on the
same macOS/Node 26 host (shared machine, absolute CPU varies with contention):

| Venue | TS CPU ms | Rust CPU ms | Ratio | Retained ratio |
| --- | ---: | ---: | ---: | ---: |
| OKX | 649.5 | 595.7 | 1.09x | 1.02x |
| Bybit | 232.6 | 156.6 | 1.49x | 1.35x |
| Binance | 171.6 | 62.2 | 2.76x | 1.12x |

Optional, independently reviewed probes show raw text crossing is not the main
cost: OKX 3.62 ms CPU, Bybit 4.11 ms for a complete fixture. JSON syntax
validation plus crossing is 6.80/7.45 ms; this is a parser floor, not the full
venue decoder. Instrumented wsMessage (crossing + typed parse/state + action
serialization) is 170.5/222.5 ms wall; takeFrame is 3.78/26.13 ms. Instrumented
wall samples are diagnostic and include scheduler noise, not gate CPU results.

Native phase review passed after removing asynchronous admission backpressure
that allowed later HTTP/timer events to overtake an earlier socket arrival.
Native drains a finite ready prefix before commands/flush/shutdown, in bounded
slices. Existing transport inbox remains unbounded, with no copied batch buffer.

JS phase review found and fixed a capacity-flush reentrancy reorder. It also
identified external ArrayBuffer storage omitted by the old memory gate. The
new measurement counts ArrayBuffer delta alongside heap and WASM memory;
original baseline will be remeasured with the same accounting. Scratch is lazy.
The initial batching run improved CPU but regressed memory; gates remain open.

### WASM batching and family-build implementation checkpoint

- Batch wire: little-endian socket u32, UTF-8 byte length u32, arrival f64,
  followed by borrowed UTF-8 text. Whole batch validates before any state change.
- JS queues up to 256 messages / roughly 1 MiB, preserving arrival order across
  sockets; barriers drain first. Large scratch is released after delivery;
  retained scratch is capped at 64 KiB. Disposal ignores late transport events.
- Family modules are selected by venue across browser, Node, harness and Workers;
  aliases share modules, browser initialization is cached per family with retry.
  Isolated clippy, 20 SDK WASM tests, 3 Rust WASM tests, 16 host tests pass.
- Independent reviews passed profiling, native batching and family registration.
  Workers review exposed duplicate/orphan async initialization and commands lost
  during a pending download; guards and deferred-loader regression tests added.
- Exact escaped-string output reservation lowers OKX startup WASM highwater from
  1,441,792 to 1,114,112 bytes (five pages). Same subscription JSON and replay
  output. A first strict-memory recorded-burst gate passes (1.16x CPU, 1.01x
  retained memory) before scratch-retention improvement; broader gates remain.
- Public replay now yields between distinct recorded timestamps, preserving
  equal-timestamp network bursts for both implementations. `--cadence frame`
  keeps the earlier pacing; `--delivery single` isolates batching changes.
  Re-running the original isolated binary with identical cadence and strengthened
  ArrayBuffer accounting for the controlled before/after table.
