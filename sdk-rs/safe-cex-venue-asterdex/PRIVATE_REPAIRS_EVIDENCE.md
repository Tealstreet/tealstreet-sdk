# Asterdex private position, balance and order repairs

Branch: `feat/rust-asterdex-private-repair-9`. Actual starting parent:
`0f7fd2ce92e79d0c38e87ea5dd15b21927ea8c1a` (integration, PR 7607 merged).
This slice does not stack on an unmerged lane. The full private socket class,
constructor, connection and message-dispatch rows remain pending.

## Independently expected movement

The Python 3.11 `tomllib` script `scripts/private-repairs-census.py` compares
the starting parent's eight manifests with the current files and independently
expects nine named source methods. Iteration 1 stops at eight files / 231 rows.

| Status | Parent | After this slice |
| --- | ---: | ---: |
| ported | 115 | 118 |
| divergent | 49 | 55 |
| pending | 67 | 58 |

**3 pending -> ported, 6 pending -> divergent.** The three timer-clear bodies
are ported on the native timer-registration seam; each has an actual
schedule-clear-tick cancellation witness. Six distinct CHOSEN notes and six
distinct real method targets describe scheduling and run boundaries separately.
`private_repairs.rs` has all nine executable bodies and persistent timer/queued/
busy state. This is neither a patch plan nor a fake complete socket target.

The native BaseExchange owns monotonic timeouts and detached tasks. Its disposal
cancels work and releases busy guards, while source RPC promises remain alive
until settlement. Native scheduling returns fallible registration results;
typed parent RPCs and closed Position/Order/loaded records replace the source
runtime objects. Structured uncaught errors are visibly reported through the
existing runtime. Those boundaries are disclosed on their own divergent rows.
No shared type field or existing shared trait method was added. Store changed
from a dev dependency to a regular dependency because these bodies apply writes
to the actual Store contract.

## Behavioral floor and review

The oracle extracts the unedited nine TypeScript members and their state/delay
fields. It executes production DefaultStore, production clone and real lodash
uniqBy, with controlled timers and parent RPCs. LogSeverity comes from its real
logging source module. Constructor/connection behavior is not substituted into
a completed-row claim.

Iteration 1 stops at **six scenarios / 25 snapshots / nine RPC dispatches**,
with two scenarios per position/balance/order family. These are literal
expectations declared independently in the test, not derived from result length.
All native Store records, loaded flags, queued/busy state, timer/dispatch/
notification counts, and captured balance versions are checked before logs.
No decode/re-encode comparison is used. The floor covers stale position
rejection and queued follow-up, real subscriber failure after Store mutation,
preserved initial balance version and latest-version retry (3 then 8), rejection
after queue clear, ten-second debounce replacement, overlapping order requests,
and the empty-message Error fallback. Native-only owner-cancellation behavior
has a separate witness and is not presented as source-equivalent timing.

Read-only sub-agent review found no ordinary-path body mismatch. Its position/
balance cancellation coverage gap was fixed with schedule-clear-tick cases that
cross the 1000ms deadline and observe no dispatch. The reviewer verified the
real targets, distinct notes, independent counts, and compiled-control sites;
it ran no Cargo. Fixture-only Store trait binding and cleanup errors were fixed
before the final SHA-bound gates; no failing subscriber is left attached during
fixture disposal.

## Gates and exclusions

All final gates were measured at **`bbf148f8282c1274d5f15c342a6e2e15db7c62e5`**. Exact argv and raw outputs
are in tracked `PRIVATE_REPAIRS_EVIDENCE.json` and `PRIVATE_REPAIRS_CONTROLS.json`.

| Gate | Exit |
| --- | ---: |
| Scoped cargo check, Asterdex all targets, jobs 2 | 0 |
| Full Asterdex cargo tests, all targets, jobs 2, include ignored, test threads 2 | 0 |
| Extractor check: output zero updates/additions/orphans | 0 |
| tomllib census and distinct-target/note expectation | 0 |

| Compiled control / named assertion | Broken | Restored |
| --- | ---: | ---: |
| Position contracts: `position-stale-follow-up native position contracts`, 1234 vs 9 | 101 | 0 |
| Balance retry: `balance-first-version-and-latest-retry native balance version`, 3 vs 8 | 101 | 0 |
| Order delay: `order-debounce request count`, 2 vs independently expected 1 | 101 | 0 |

All failures reached the named assertion after successful compilation. Each
control restores the exact original bytes before its positive run; all original/
restored hashes match. Iteration 1 stops at these three controls, one per family;
**zero survivors**. The source assertion suite observes decoded native fields and
arguments before logs, not a re-encoded representation.

Not run: workspace compilation, other venue suites, host clippy, wasm clippy,
wasm builds, real-network exchange calls, full monorepo suites, or CI reruns.
The six scenarios do not enumerate arbitrary schedulers, JavaScript object
hooks, all RPC record shapes or every exception value.

## Merge-down preservation

Merged integration `671cdadf56e6ec1c1955795f0976a8cfd0d28d09` at
`8ed6d288a6c5cd7b9b3f9020ec028a725dbb5516`. Against code-gate SHA
`bbf148f8282c1274d5f15c342a6e2e15db7c62e5`, the scoped source/tests diff
exited **0**. The extended diff covering this crate's scripts/manifest, live
TypeScript oracle inputs, Asterdex parity manifests, workspace Cargo manifest,
and shared Rust dependency sources/manifests also exited **0**.

Cargo.lock changed only the OKX package dependency list (indexmap and crypto).
The resolved Asterdex reachable lockfile package graph compared equal (exit **0**);
the comparison script and outputs are embedded in the JSON evidence. The Cargo
exits above remain measurements at the original code SHA, preserved by these
input comparisons; Cargo was not rerun at the merge SHA.

At the merge SHA, extractor check exited **0** with zero updates/additions/orphans,
and the tomllib census exited **0**, confirming nine moved and 58 pending. Exact
commands, outputs and measured SHAs are recorded under `post_merge_gates`.
