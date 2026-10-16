# Asterdex virtual clock: eight pending rows

Branch: `feat/rust-asterdex-next-10`. Actual starting parent:
`85f3b9e596582fce3738807c4d9b7032c5b5a802`, the requested integration head.
This slice starts directly on integration and does not stack on an unmerged lane.

## Census and implementation

`scripts/clock-lifecycle-census.py` uses Python 3.11 `tomllib`, compares the
committed parent with the current manifests, and independently expects eight
named source identities. Iteration 1 stops at eight Asterdex files / 231 rows:

| Status | Parent | After this slice |
| --- | ---: | ---: |
| ported | 113 | 115 |
| divergent | 43 | 49 |
| pending | 75 | 67 |

Moved **2 pending -> ported, 6 pending -> divergent**. `stop` and
`fetchServerTime` are ported in the declared native realm / decoded JSON host
domain. Six distinct notes and real distinct targets describe the class,
start, sync, inner sync, force-sync, and singleton differences. Native ownership
and console capabilities are explicit; owner disposal cancels outstanding work
without settling completion waiters. Normal `stop` retains the in-flight sync.

`virtual_clock.rs` contains the executable lifecycle bodies, shared completions,
and realm singleton. It uses the existing BaseExchange owned operations and
monotonic intervals. Generation increments use f64 `+ 1.0` to retain JavaScript
Number saturation rather than integer overflow. No shared type/trait changed.

## Behavioral floor

`clock_lifecycle_matches_source` executes the unedited source class with real
Dayjs and deterministic HTTP/timer host capabilities. Iteration 1 stops at five
named scenarios, independently expected **18 snapshots, 15 HTTP requests, and
8 observed operations**. The fixture checks direct-before-proxy ordering,
preferred-version fallback, exact 5000ms timeout, retained offsets after failure,
stop/restart coalescing and stale-result rejection, small offsets, invalid but
truthy Dayjs results, discarded waiters, and wall-clock changes during monotonic
intervals. Every decoded native value assertion precedes URL and log assertions.
No decode/re-encode comparison is used.

Separate native tests assert realm sharing/isolation, timer-slot removal before
reentrant host cancellation, and the documented owner-disposal pending-waiter
boundary. The source singleton witness observes a shared offset and shared stop
through two module-export reads. Counts do not come from the result under test.
This is a bounded fixture floor, not exhaustive scheduler or object enumeration.

## Review and an existing gate correction

Read-only sub-agent review identified and checked the fixes for the timer borrow
across host cancellation, HTTP-pending substituted for the native in-flight slot,
and a tautological source-singleton check. The reviewer ran no Cargo. Final review
accepted 2 ported / 6 divergent and the Number counter correction.

The first full crate run at `4258b28643572f58512a101be53d024e0290b04c` exited
101: an existing nonfinite market control expected a later wire assertion name,
but its child correctly failed first on `transformMarket native price precision
identity` (native 0.1, source 0.0). Those test/helper inputs were byte-identical to
the starting parent. Commit `95ccde0390` updates that specific expected assertion;
the required child exit remains 101. The full rerun at that commit exited 0.
Raw original failure and final measurements are retained in the JSON gate sheet.

## Gates

Final measurements and exact argv are in `CLOCK_LIFECYCLE_EVIDENCE.json`;
the compiled control is in `CLOCK_LIFECYCLE_MUTATION.json`. Both are tracked.
All final code gates below were measured at **`ae86c3f1a99b86fed639d390c9b2029ec5d1def5`**:

| Gate | Exit |
| --- | ---: |
| Scoped Asterdex cargo check, all targets, jobs 2 | 0 |
| Full Asterdex cargo tests, all targets, jobs 2, include ignored | 0 |
| parity:extract:check, output says zero updates/additions/orphans | 0 |
| tomllib census, eight independent named transitions | 0 |
| Compiled generation-guard mutation, named native assertion | 101 |
| Exact-byte restoration and same named assertion | 0 |

`scripts/clock-lifecycle-mutation.py` disables only the inner post-fetch generation
check. `clock_lifecycle_matches_source` fails at
`stop-restart-coalesces-stale-request offset/generation: native 500.0, source 0.0`.
The mutation compiled successfully; failure is a test assertion, not a compiler
error. The original/restored SHA-256 hashes match. One control attempted,
iteration 1; **zero survivors**. The earlier control at `95ccde0390` was rerun
after the Number fix, so the final control belongs to the final code SHA.

Not run: workspace compilation, any other venue suite, host clippy, wasm clippy,
wasm builds, real-network exchange calls, full monorepo suites, or CI reruns.
No claim covers those gates. No enumeration of all generation values, arbitrary
JavaScript object hooks, or throwing native consoles was run.

## Integration preservation

Merged integration **`ef054ba0b8e9038241576f44c09f652348f2f4bb`** down, producing
`58312a850a75d3054d54a8b0660c1bc75f571ae1`. Both measured diffs exited **0** against code-gate SHA
`ae86c3f1a99b86fed639d390c9b2029ec5d1def5`: own `src`/`tests`, and the wider scope covering scripts,
TypeScript oracle sources, Asterdex manifests, crate Cargo manifests, workspace
Cargo/lock files, and the exchange/transport/types/num/store/crypto dependency
sources. Those gate inputs are byte-unchanged, so their measured code exits stand;
this is preservation, not a new Cargo run on the merge commit.

At that merge SHA, the extractor check and tomllib census each exited **0**.
The extractor output again says zero updates/additions/orphans. Asterdex remains
115 ported / 49 divergent / 67 pending. The only non-integration ancestry is this
slice's implementation, control, test correction, Number correction, and evidence
commits; it does not inherit an unmerged lane.
