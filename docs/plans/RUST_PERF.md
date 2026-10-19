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

### Allocation follow-up (reviewed)

Measured Bybit's first frame allocation added five pages by repeatedly growing
its output Vec. Whole-section reservations eliminate that growth; standalone
trace saves four pages overall (1,835,008 -> 1,572,864 bytes after later batches).
Store now offers an allocation-free instrument-ID ticker update, preserving
symbol lookup and dirty masks across reordered/replaced lists. OKX subscriptions
serialize only 100 borrowed topics at a time with identical original ordering;
ticker updates borrow native IDs and stop allocating duplicate set keys.

Two independent static reviews pass; engine's 16 tests and clippy pass. Full
workspace tests/build plus repository typecheck/lint/tests are running.

Final optional diagnostics (recorded cadence, three instrumented runs) observed
OKX 8,904 single calls -> 427 batches, Bybit 8,742 -> 126. Raw text crossing is
~4 ms CPU per fixture, prepared 64-message buffer crossing ~0.1 ms. Combined
instrumented WS wall medians: OKX 240.23 -> 34.17 ms, Bybit 48.84 -> 18.96 ms;
frame calls 4.49 -> 3.24 / 3.64 -> 3.45 ms. Wall samples include scheduler noise;
these are diagnostics, not final uninstrumented CPU gate numbers.

### Four-family integration and adversarial fixes

Merged Crypto.com landing `c63c3e5670` with the lane's family isolation. It also
landed stable async backing-buffer collection and captures CPU before final GC;
preserved both. The controlled before build is now rebuilt from that landing
source with the final replay/gate but original aggregate binary/unbatched host,
so all four venues share the same timing, memory accounting and burst cadence.
Earlier exploratory numbers above are not a final before/after comparison.

Adversarial review found/fixed:

- Native command clock was captured after yielding to drain a ready prefix;
  later arrivals could then carry an earlier time. Capture the selected command
  time first; regression verifies prior/command/later clocks 70/80/90 even when
  dispatch wall time reaches 100.
- Disposal left in-flight command promises pending after late HTTP completions
  were suppressed. Reject/clear them and reject new commands after disposal.
- Disposal from an action callback could leave later socket/timer actions live;
  stop action execution and recheck disposal after every draining barrier.
- Crypto.com profiling used the wrong default symbols. Explicit family defaults
  and a cadence option now match each fixture.

Independent review reproduces the disposal fix and finds no remaining harness
or family-build issues. Native six tests/clippy, 21 JS host tests, 20 SDK WASM
adapter tests and four isolated wasm32 clippy checks pass. Repository-wide
44 typecheck and 28 lint tasks pass; full tests are still running.

Family WASM bytes/budgets after integration: Binance 769031/786432,
Bybit 748585/765952, Crypto.com 617650/638976, OKX 711518/729088.

Follow-up adversarial test found a synchronous invalid WASM command registered an
unreturned pending Promise; rejecting it on dispose could become an unhandled
rejection. Validate first, register before executing returned actions, and clean
up transport/action errors through the returned Promise. The regression fails
before the fix and passes afterward. All 24 JS host tests and six native tests
now pass; independent review reproduces resource-disposal safety.

Full native workspace fmt, bans, all-target clippy, tests and wasm32 workspace
clippy pass after Crypto.com integration. Four-family baseline aggregate build
is complete. Controlled before/after public runs and seven final private tapes
are running; the first baseline OKX run passes (1.08x CPU, 1.70x retained memory)
under stable collection, illustrating why earlier exploratory memory results
must not be mixed with final measurements.
