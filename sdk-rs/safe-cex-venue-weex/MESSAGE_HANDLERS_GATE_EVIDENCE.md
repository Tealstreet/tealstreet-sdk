# WEEX message-handler evidence

Authoring parent: `0650e658b488165c0a49081aecce3114a35e2861`.
Measured implementation SHA: `d3042eb1da0a3763c44b8f0f9983d24df578bd21`.
Fresh branch: feat/rust-venue-weex-message-handlers.
No unrelated unpublished lane work was used as an ancestor.

## Row movement

Three pending to ported, zero to divergent:

| WeexPublicWebsocket source row | message_handlers.rs target |
| --- | --- |
| #addHandler | add_handler and WeexMessageHandlerRuntime |
| #removeHandler | remove_handler |
| #handleRoutedMessage | handle_routed_message |

Python tomllib read committed manifests through git show and required the
independent fixed set of these three pending-to-ported transitions, no other
status changes: exit 0. All four targets were checked against actual committed
public production definitions, and the full websocket class remains pending.
Across 156 rows, counts changed from 52 ported / 26 divergent / 78 pending to
55 / 26 / 75. Extractor writer normalized one edited manifest, zero added or
orphaned symbols; mapping arrays were serialized programmatically.

## Scope

The three local algorithms operate on generic host values and full UTF16 keys.
The explicit runtime supplies property/prototype lookup and assignment,
truthiness/nullish checks, array creation/spread/filter, strict equality,
iterator operations, bare handler invocation and stats cleanup. No payload or
registry is cast to a narrower JSON production model. Host engine primitives,
including filter/species and IteratorClose, and the enclosing websocket
class/constructor/lifecycle remain unported; no class target was fabricated.

Addition preserves separate LHS/RHS registry reads and append order. Removal
filters by strict handler identity, assigns a new array before re-reading the
registry/length, and retains an empty own key while calling cleanup. Dispatch
holds the original handlers value and a live iterator across callbacks rather
than a vector snapshot. It passes the same message reference, ignores callback
returns, propagates iterator errors, and invokes the close-on-throw primitive
on a handler exception. Helper code retains no registry collection borrow
across mutable host callbacks. The native host witnesses reentrant removal
and message mutation with RefCell data; this is not an actual Store audit.

## Gates at d3042eb1da0a3763c44b8f0f9983d24df578bd21

All cargo commands ran serially. Oracle environment:
SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript.

| Command | Measured exit / result |
| --- | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --all-targets --jobs 2 | 0 |
| python3.12 sdk-rs/safe-cex-venue-weex/tests/message-handler-mutations.py | 0; three compiled assertion failures, each individually restored and passed |
| python3.12 sdk-rs/safe-cex-venue-weex/tests/parity-check.py | 0; canonical invention/status gates passed: 156 rows, 96 public items, zero WEEX exemptions |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0; output: 7341 symbols, zero updates/added/orphaned |

The mutation command runs only the affected target:
cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --jobs 2
--test message_handlers; named test message_handlers_match_actual_typescript.
SHA-bound passes are the three restored target runs. A draft native target
also passed before this commit, and is not counted as SHA-bound proof.
An initial oracle draft exited 1 because diagnostic capture re-executed a
throwing custom iterator. Capture was corrected to read indexed array slots,
so it does not execute that iterator during observation.

## Native proof

message-handlers-oracle.cjs extracts the three actual private field declarations
from the TypeScript AST and transpiles them in a minimal class harness. It
copies no function bodies. Independent pre-call input descriptors populate
the Rust host; full function outputs are not fed back as primitive results.

Native outcomes, own registry keys, tagged values, array contents/hole presence,
array identity, callback identities, decoded message fields, shared message
identity and final message mutation are asserted before effect trace checks.
There is no encode/decode roundtrip or native wire comparison. Literal native
expectations include number 37, UTF16 units [0xd800, 0xd83d, 0xde00], retained
empty own keys, 16 callback invocations and four cleanup invocations.

The first corpus iteration stops at 35 cases: 12 add, eight remove and 15
routing cases. Ordered IDs and totals are fixed independently in Rust. Cases
cover append/duplicate order, missing/null/false fallback, array holes, lone
surrogate keys, inherited __proto__/constructor/toString behavior, differing
LHS/RHS registries, getter/setter errors, retained empty arrays, cleanup errors,
live append, registry replacement, reentrant removal, shared-message mutation,
handler errors, custom iterator close and close/next failures. These are finite
local-flow witnesses, not proof of an entire JavaScript engine. Other inherited
keys, custom spread/filter/species implementations and broader iterator/state
mutation combinations were not enumerated.

| Compiling production mutation | Named native failure, exit 101 | Restored target exit |
| --- | --- | --- |
| Append handler at the beginning | handlers::add/append::state_value | 0 |
| Keep equal handlers instead of unequal ones | handlers::remove/all_duplicates::state_value | 0 |
| Return before dispatching a nonempty list | handlers::route/ordered::call_count | 0 |

The script required exit 101, the named assertion and assertion left==right
failure text. It restored original bytes in a finally block and passed after
each restoration. Three selected controls caught; zero survivors among these
controls. Other mutations were not enumerated; compiler failures do not count.

Sub-agent weex_review inspected actual source, implementation, native fixtures,
mappings and controls; no concrete findings. It ran no cargo, tests or mutations
and made no edits or commits. Its review scoped delegated engine behavior
separately from the native local algorithms.

Not run: workspace compilation, full venue/monorepo suites, host clippy,
wasm clippy, live venue requests or CI reruns. No income pager was added.
Ignored local logs: sdk-rs/target/weex-handler-gates/.

## Delivery merge and remeasured gates

Fetched and captured this worktree's FETCH_HEAD as integration
`9d6f40b96faafcfa73e19ffcfb2825e4d3e78444`, then merged that exact SHA into
`2133fecb257e8d92c8b57c16e335d6eae9f6c2af`, with no conflicts.
Git diff --exit-code from the implementation SHA to this merge exited 0 for
WEEX src/tests/manifests. A new committed-TOML audit against the actual second
merge parent exited 0 and required the same three transitions and counts above.

A broader compiler-input diff exited 1: Cargo.lock added base64 to ApexOmni.
Global parity exemptions also changed. Relevant gates were remeasured at the
merge SHA; original results were not relabeled as new measurements:

| Command | Exit at 2133fecb257e8d92c8b57c16e335d6eae9f6c2af |
| --- | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --all-targets --jobs 2 | 0 |
| cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --jobs 2 --test message_handlers | 0; 35 cases, 16 callbacks, four cleanups |
| python3.12 sdk-rs/safe-cex-venue-weex/tests/parity-check.py | 0; 156 rows, 96 public items, zero WEEX exemptions |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0; 7341 symbols, zero updates/added/orphaned |

The three mutation controls retain their original measured SHA; they were not
rerun at the merge. Own production/test bytes are unchanged. This evidence
commit changes only this narrative.
