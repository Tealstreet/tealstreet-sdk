# WEEX response and websocket selection evidence

Authoring parent: `1f6e115b0cb3aec1c42860a0f97cf71bd84376d8`.
Measured implementation SHA: `8d7420964e6b4bc698269961f92a2be72c6ed712`.
Fresh branch: `feat/rust-venue-weex-array-extraction`.
No unrelated unpublished lane work was used as an ancestor.

## Row movement

Three pending to ported; zero to divergent:

| Source row | Actual production Rust target |
| --- | --- |
| extractArray | selection.rs::extract_array and selection.rs::WeexArrayRuntime |
| isNodeOrBunRuntime | selection.rs::is_node_or_bun_runtime |
| WeexPublicWebsocket#getDesiredTickerTopics | selection.rs::get_desired_ticker_topics |

Python tomllib read manifests from both committed revisions with git show.
The independent expectation was the fixed set of these three rows, each
pending to ported, with no other status changes. It exited 0. Each of their
four public production targets was checked against committed Rust source.
The canonical parity gate additionally resolved all mapped WEEX targets.
Across 156 rows, counts changed from 41 ported / 26 divergent / 89 pending to
44 ported / 26 divergent / 86 pending.
Extractor writer ran after manifest editing: three normalized manifests,
zero added/orphaned rows. Arrays were serialized programmatically and
normalized by the writer, rather than hand-spelled.

## Implementation scope

extract_array preserves payload/data/key precedence, per-key root-before-nested
search, repeated getter reads, original selected value identity and thrown
errors. Its generic capability record supplies JavaScript value operations:
fallible Array.isArray and property access, nullish classification, undefined
and fresh empty-array allocation. It does not restrict the payload to JSON.
The source can return a changed getter value even if that later value is not
an array; the Rust return type retains this behavior.

is_node_or_bun_runtime preserves the undefined-process guard and evaluates
Node then Bun lazily, propagating either callback error. The caller supplies
the source process property chain and truthiness primitives. The helper is
the local decision; it does not implement a JavaScript global environment.

get_desired_ticker_topics accepts declared string market/active projections
and the existing topic Set count. UTF16 retains lone surrogates. Filtering
keeps market order and duplicate market symbols, uses all markets when the
active list is empty, clamps against 90 ticker / 100 total channels, and
appends @ticker. Store/parent reads and websocket lifecycle remain pending;
no class skeleton or fabricated target was introduced.

## Gates at 8d7420964e6b4bc698269961f92a2be72c6ed712

All cargo commands were serial. Oracle environment was
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Command | Measured exit / result |
| --- | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --all-targets --jobs 2 | 0 |
| python3.12 sdk-rs/safe-cex-venue-weex/tests/selection-mutations.py | 0; three compiled assertion failures, each restored and passed |
| python3.12 sdk-rs/safe-cex-venue-weex/tests/parity-check.py | 0; canonical invention/status gates passed: 156 rows, 81 public Rust items, zero WEEX exemptions |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0; output: 7341 symbols, zero updates/added/orphaned |

The mutation command runs the affected target, not a full venue suite:
`cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --jobs 2
--test selection`, named test selection_matches_actual_typescript.
A draft test also passed before the implementation commit; it is not counted
as a SHA-bound gate. The SHA-bound passes are the restored target runs below.

## Native proof and discrimination

selection-oracle.cjs extracts the actual two const function declarations,
the actual private topic field declaration and the actual channel constants
from TypeScript ASTs, then transpiles them in minimal contexts. Function
bodies and constant values are not copied into the oracle. Independent input
descriptors populate the Rust host; complete function outputs are never fed
back as a primitive implementation.

Native tagged values, array identities and elements, errors, booleans and
UTF16 topic vectors are asserted before property/callback trace comparisons.
There is no encode/decode roundtrip. The payload array also has a literal
native expectation: identity 2 and elements [101, 102]. Topic duplicates and
the 90-channel bound have literal native expectations. Counts and ordered
case identities are fixed independently in the Rust test.

The first corpus iteration stops at 23 array cases, 10 runtime cases and
14 topic cases: 47 native results. This is a coverage floor. Array cases cover
precedence, supplied-key order, nullish/primitive/missing fallbacks, getters
changing returned values, repeated reads that throw, lone-surrogate keys,
and revoked proxies at payload, data and nested probes. Runtime cases cover
the absent process, Node short-circuit, Bun fallback, several truthiness
inputs and both callback errors. Topic cases cover order, duplicates,
unavailable symbols, empty markets, UTF16 and counts 0, 9, 10, 11, 98, 99,
100 and 101 against 105 available markets.

| Compiling production mutation | Named native assertion / failing exit | Restored target exit |
| --- | --- | --- |
| Return a new empty array for an array payload | selection::array::payload_array / 101 | 0 |
| Return true for an undefined process | selection::runtime::absent / 101 | 0 |
| Add existing topics instead of subtracting capacity | selection::topics::capacity_11 / 101 | 0 |

The script required exit 101, the named assertion and the assertion left==right
failure text before accepting each control. Original bytes were restored in
a finally block and each restored target passed. Three controls caught;
zero survivors among these controls. Other mutations were not enumerated.

Sub-agent weex_review inspected implementation, actual source, projections,
fixtures, mutations and mappings. It found the initial infallible Array.isArray
capability could miss the actual revoked-Proxy TypeError, confirmed by a small
Node diagnostic. The capability is now fallible at all four probes, and three
actual revoked-Proxy fixtures passed in the SHA-bound native tests. It also
caught a draft mutation that would panic before a named assertion; that
control was corrected before measurement. Its subsequent inspection found
no remaining concrete issues. It ran no cargo, oracle suites or mutations
and made no edits or commits.

Not run: workspace compilation, full venue/monorepo suites, host clippy,
wasm clippy, live venue requests or CI reruns. No income pager was added.
Ignored local gate logs live under sdk-rs/target/weex-selection-gates/.

## Delivery merge

Fetched and merged integration `cc3d25150b320656935009943f72d17b27e380a0`
into merge SHA `945d92e62a2978c317ccad8c9d826663c3a4bd8f` before delivery.
There were no conflicts.

A measured git diff --exit-code from implementation SHA to merge SHA exited 0
for WEEX source/tests/manifests, workspace Cargo manifest/lock, shared types,
num/store dependencies, canonical parity checker/exemptions, extractor source,
actual WEEX TypeScript source/shared enums/logging enum, package.json and
Cargo configuration paths. Original gate results remain bound to the original
implementation SHA; cargo checks and mutation controls were not rerun at the
merge. Their inspected inputs are byte-unchanged.

Extractor check was separately rerun at the merge SHA: measured exit 0 and
output 7341 symbols, zero updates/added/orphaned. A new committed-manifest
audit against the fetched integration exited 0 and required the same fixed
three pending-to-ported transitions. Counts remain 44 / 26 / 86. This evidence
commit adds only this narrative and does not change measured inputs.
