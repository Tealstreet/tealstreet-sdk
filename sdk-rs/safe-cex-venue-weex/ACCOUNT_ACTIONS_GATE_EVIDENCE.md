# WEEX cancellation and account-action family

Authoring parent: `82274b95ecbae81c5c8b20c4d3f65d60cf64a360` on
`feat/safe-cex-rust`. Fresh branch: `feat/rust-venue-weex-account-actions`.
No unrelated unreviewed feature branch was used as an ancestor.

## Rows and scope

Independent tomllib comparison against the authoring parent moved **9 pending
-> ported, 0 pending -> divergent**:

- updateSymbolMetadata
- cancelOrders
- cancelSymbolOrders
- cancelAllOrders
- setLeverage
- changeMarginType
- changePositionMode
- nukePositions
- nuke

WEEX census: 74 ported / 26 divergent / 56 pending, 156 total rows.
All nine functions live in account_actions.rs and implement the local source
algorithms over shared host handles. Normal cancellation keeps the two filters,
chunk size ten, normal-before-algo sequencing, stripped request IDs and late
original-ID status update. Parallel cancellation starts both requests eagerly,
attaches the source Promise.all primitive eagerly, and reads current store
orders only after success. Leverage retains post-fetch limit reads, JavaScript
min/max/number-string behavior, isolated-side carry and metadata refresh.
Margin and position-mode actions retain payload selection, position-only uniq,
un-deduplicated market fallback, post-before-cache, and final setting order.
Nuke positions keeps close/fetch/store ordering; nuke then calls native cancel
all. Metadata refresh requests config with refetch true.

Opaque order/config/market/position values and fallible getters/store calls are
host capabilities. Config spread-with-mode and Promise.all are explicit source
primitives. The aggregate contract retains the first rejection observed and
keeps independently owned peer work alive after dropping its observation.
No exclusive host borrow crosses await. This is a function-algorithm claim on
the declared native typed collections; class assembly, host value engine,
transport, object-reference identity and JavaScript microtask/executor behavior
were not established by these fixtures and remain separate runtime boundaries.
No income pager was added.

## Gates measured at implementation SHA

All gate exits below were measured at
`f2010ebf22590a2bcba48ad823ccaaad2b5af2be`.

| Command | Exit / observed result |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --all-targets --jobs 2` | 0; scoped crate compilation, zero errors |
| `python3.12 sdk-rs/safe-cex-venue-weex/tests/account-actions-mutations.py` | 0; five selected family controls caught, each restored target passed |
| `python3.12 sdk-rs/safe-cex-venue-weex/tests/parity-check.py` | 0; 156 rows, 125 public Rust items, zero WEEX exemptions |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0; 7341 symbols, Would update 0 manifests, 0 added, 0 orphaned |

The extractor writer ran after manifest edits. Check output text was read;
these claims do not depend solely on the Volta shim's status. The canonical
scoped parity gate validates actual declared Rust targets for off-pending rows.
Cargo commands, including cargo inside the parity gate, ran one at a time.

Each mutation runs the specific target with the actual TypeScript compiler:
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --jobs 2 --test account_actions`.
Each broken run exited 101 with the named native assertion and assertion
left==right failure text. Every restoration exited 0, one test passed.

| Family / selected production mutation | Named assertion | Broken / restored exit |
| --- | --- | --- |
| Normal cancellation: change chunk ten to nine | `native_requests::cancel_batches` | 101 / 0 |
| Parallel cancellation: await the first request before observing the second | `native_error_message::symbol_dual_rejection` | 101 / 0 |
| Configuration payloads: change isolated long-side preservation condition | `native_requests::leverage_isolated_long` | 101 / 0 |
| Metadata refresh: pass refetch false | `native_config_calls::leverage_cross` | 101 / 0 |
| Position closure: omit fetched-position store update | `native_positions::close_positions` | 101 / 0 |

Enumeration stopped after these five family controls: five caught, zero
survivors among these controls. These are not nine independent row-specific
mutation proofs. Other mutations were not enumerated. The script restores
source bytes in a finally block; compilation failures do not count as proof.
Git diff --exit-code against the implementation SHA exited 0 for source/tests
after the controls.

## Native oracle floor and resolved findings

Enumeration stops at the ordered 58 fixtures. Their IDs are fixed independently
in Rust. The batch fixture also asserts independently fixed six requests and
24 original status IDs. Decoded native request methods/endpoints/own field
presence/UTF16 values, statuses, config arguments, cache fields, account mode,
hedged setting, updated positions and errors are asserted before any eager-start
or complete-effect trace comparison. There is no decode/re-encode assertion.

The oracle extracts the actual nine source fields plus actual isAlgoOrder,
loads actual shared enums/venue constants and uses real lodash chunk/uniq and
real Promise.all. Request/config/fetch/store primitives are controlled input
fixtures, not results taken from the function under test. The owned-task model
covers reversed parallel completion and dual rejection, peer survival, latest
store orders after await, mutated IDs, delayed limits, isolated-side leverage,
NaN/signed-zero inputs, failure stop points, partial cache/state commits,
duplicate fallback markets, surrogate symbols, and eager empty-target setters.
This is a finite fixture floor, not completeness over arbitrary host engines.

Review found that the initial native collector could prefer the first array
entry's rejection when both futures were ready. The eager source Promise.all
capability replaced it, and dual-rejection fixtures now distinguish that error.
The initial native target also failed eager_prefix::mode_no_targets; empty-target
setters were made eager before the measured commit. Both findings were resolved
before the implementation SHA above; neither was silently discarded.

Sub-agent weex_review inspected production/source behavior, native fixtures,
all nine mappings and five control edits. Follow-up review found no concrete
remaining local defect. It independently ran a read-only TOML audit; it ran no
cargo/tests/mutations and made no edits or commits. Its inspection is separate
from the measured gates.

Not run: workspace compilation, full venue/monorepo suites, host clippy, wasm
clippy, live venue requests, or CI reruns. Local logs/results stay ignored under
`sdk-rs/target/weex-account-actions-gates/`.

## Delivery merge

Fetched this worktree's exact integration FETCH_HEAD
`e413ab5016a4451c935648a910a4a005b0414134`, then merged that SHA without
conflicts into `7c2c240073d72a062ce3c8f68918b69772d86862`.
An independent tomllib audit against that actual integration parent exited 0,
requiring exactly the nine transitions listed above, each pending -> ported,
156 total rows and no missing Rust target files. Census remains 74 / 26 / 56.

`git diff --exit-code f2010ebf22590a2bcba48ad823ccaaad2b5af2be HEAD -- sdk-rs/safe-cex-venue-weex/src sdk-rs/safe-cex-venue-weex/tests`
exited 0 at the merge. A separate input diff also exited 0 for WEEX
source/tests/Cargo.toml/manifests, workspace Cargo manifest/lock, shared
types/num/store, canonical parity engine/extractor, package.json, actual WEEX
TypeScript/shared enums/logging enum and Cargo configuration paths. The new
narrative document is outside that input check. Original scoped compilation,
native cases and five mutation measurements remain bound to the implementation
SHA above; they were not rerun at the merge because their inputs are unchanged.

Integration changed global exemptions and other manifests, so the affected
gates were remeasured at `7c2c240073d72a062ce3c8f68918b69772d86862`:

| Command | Measured exit / result at merge SHA |
| --- | --- |
| `python3.12 sdk-rs/safe-cex-venue-weex/tests/parity-check.py` | 0; 156 rows, 125 public items, zero WEEX exemptions |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0; 7341 symbols, Would update 0 manifests, 0 added, 0 orphaned |

This evidence update changes only this narrative.
