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
