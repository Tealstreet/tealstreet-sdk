# WEEX order routing evidence

Measured gate SHA: `29cad4ed4478b92c83dfe44f9e1d0d48a8644c02`.
Authoring parent: `e9b4202fc66f3f4ae6bc6ec7f78f09f27795c5d5`.
No unrelated unpublished lane work was used as an ancestor.
This narrative-only descendant adds no source or test changes.

## Measured movement

Three pending to ported, zero to divergent:

| Source row | Rust target |
| --- | --- |
| WeexExchange#isBatchableRegularOrder | order_routing.rs::is_batchable_regular_order and order_routing.rs::WeexBatchOrderFields |
| WeexExchange#isAlgoOrder | order_routing.rs::is_algo_order |
| WeexExchange#inferPositionSide | order_routing.rs::infer_position_side |

Python tomllib read committed manifests through git show, compared against an
independent fixed list of these three expected transitions and exited 0.
WEEX baseline: 38 ported / 26 divergent / 92 pending. Slice: 41 / 26 / 89,
across 156 rows. Counts are not inferred from Rust item counts.

These are local decisions over explicit data projections. Batch routing
retains undefined-versus-defined and strict equality to true; position routing
uses caller-supplied source truthiness. Shared enum variants preserve strict
type/side comparisons, with None representing non-enum values. Algo IDs retain
the full declared string domain as UTF16, including lone surrogates.
Unread fields are not projected into a narrower full-order model. The enclosing
exchange class/constructor, arbitrary getter runtime and malformed nonstring
IDs remain outside this slice; no class skeleton was fabricated.

## Gates at the measured SHA

All cargo commands ran serially. Oracle environment:
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Command | Observed exit/result |
| --- | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --all-targets --jobs 2 | 0 |
| python3.12 sdk-rs/safe-cex-venue-weex/tests/order-routing-mutations.py | 0; three compiled assertion failures, each restored and passed |
| python3.12 sdk-rs/safe-cex-venue-weex/tests/parity-check.py | 0; canonical invention/status gates passed: 156 rows, 77 public Rust items, zero WEEX exemptions |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0; output: 7341 symbols, zero updates/added/orphaned |

Extractor writer ran after the manifest edit before the measured commit:
one normalized manifest, zero added/orphaned. The initial draft target also
passed, but the SHA-bound assertions below were measured through the restored
test runs in the mutation script, rather than transferring draft results.

## Native proof

order-routing-oracle.cjs extracts the three actual private field declarations
verbatim and compiles them in a minimal class harness with the actual shared
TypeScript enums. It does not copy function bodies or claim constructor coverage.
Input descriptors are captured independently before the source call. Native
booleans and PositionSide variants are asserted directly, before any encoding;
there is no roundtrip comparison.

The first corpus iteration stops at 44 batch cases, 77 algo cases and 45 position
cases: 166 results. Fixed ordered IDs and literal counts are independent of the
artifact under test. Native outcome totals require 13 accepted batches, 59 algo
orders and 22 long positions. Cases distinguish zero/nonfinite/undefined/null
presence, strict true versus other truthy values, ignored order fields, all six
declared order types plus a non-enum type, eleven UTF16 IDs, and fifteen
reduceOnly values across three side groups. This enumeration is a coverage floor.

The mutation command runs the affected target:
`cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --jobs 2
--test order_routing`, named test `order_routing_matches_actual_typescript`.
Each selected production mutation compiled, exited 101 at the named native
assertion, restored the original bytes and then passed this target with exit 0.

| Mutation | Failed native assertion | Restored exit |
| --- | --- | --- |
| Admit Conditional to regular batch types | routing::batch::base_Conditional | 0 |
| Change second underscore marker to dash | routing::algo::Limit_double | 0 |
| Reduced buy returns Long instead of Short | routing::position::Buy_true | 0 |

Three controls caught; zero survivors in this iteration. Other mutations were
not enumerated. Compile failures do not satisfy the control script.

Sub-agent weex_review inspected source equivalence, projections, manifests,
native tests and controls; no concrete issues found. It ran no cargo, tests,
oracles or mutations and made no edits or commits.

Not run: workspace compilation, full venue/monorepo suites, host clippy,
wasm clippy, live venue requests or CI reruns. No income pager was added.
Ignored local logs: sdk-rs/target/weex-order-routing-gates/.

## Delivery merge and remeasured gates

Fetched and merged integration `8cbc4aed134a44085fb6c20e261d2bf504d7adc6`
into merge SHA `4eb5f6d8300a751383d2ede5d6140d38ea0308a3` before delivery.
Python required git diff --exit-code from the original gate SHA to this merge
to exit 0 for WEEX src/tests/Cargo.toml/manifests. It exited 0: own source, tests
and mappings are byte-unchanged. Shared Rust dependencies, Cargo lock, global
exemptions and package scripts changed with integration, so relevant gates
were remeasured at the merge SHA instead of transferring their results.

| Command | Exit at 4eb5f6d8300a751383d2ede5d6140d38ea0308a3 |
| --- | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --all-targets --jobs 2 | 0 |
| cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --jobs 2 --test order_routing | 0; order_routing_matches_actual_typescript passed, 166 cases |
| python3.12 sdk-rs/safe-cex-venue-weex/tests/parity-check.py | 0; 156 rows, 77 public items, zero WEEX exemptions |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0; 7341 symbols, zero updates/added/orphaned |

The three discriminating controls retain their original measured SHA
`29cad4ed4478b92c83dfe44f9e1d0d48a8644c02`; controls were not rerun at
the merge. All cargo commands remained serial. Independent committed-TOML
comparison against the fetched integration again required the same fixed three
pending-to-ported rows and exited 0: counts remain 41 / 26 / 89. This evidence
commit changes only this narrative.
