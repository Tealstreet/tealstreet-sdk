# WEEX subscription control evidence

Authoring parent: `50b1dac9770bb23a8e69e1101945c337c468c194`.
Measured implementation SHA: `d4ed0f8c3322dfe7211900a9bac74c1c22e6b0cf`.
Fresh branch: feat/rust-venue-weex-subscription-budget.
No unrelated unpublished lane work was used as an ancestor.

## Measured movement

Eight pending to ported; zero to divergent:

| WeexPublicWebsocket source row | subscriptions.rs function |
| --- | --- |
| #sendSubscriptionOperation | send_subscription_operation |
| #subscribeTopics | subscribe_topics |
| #reconcileTickerTopics | reconcile_ticker_topics |
| #subscribeTickerTopics | subscribe_ticker_topics |
| #resubscribeTopics | resubscribe_topics |
| #subscribeTopic | subscribe_topic |
| #unsubscribeTopic | unsubscribe_topic |
| #handleActiveTickerSymbolsChanged | handle_active_ticker_symbols_changed |

The operation row also maps WeexSubscriptionState, WeexSubscriptionMethod and
WeexSubscriptionRuntime. All eleven targets are actual public production
items. Python read committed TOML through git show against the authoring
parent and required the fixed independent set of these eight pending-to-ported
transitions, with no other status changes: exit 0. It checked each target in
committed Rust source and that the full class row remains pending. Counts
across 156 rows: 44 ported / 26 divergent / 86 pending became 52 / 26 / 78.
Extractor writer normalized one edited manifest, zero added/orphaned rows.
Arrays were serialized programmatically then normalized by the writer.

## Scope

These are source local control flows over partial websocket state. Topic
vectors project insertion-ordered unique Sets; strings retain full UTF16.
Timestamp and subscription-ID fields retain binary64 including nonfinite
numbers and signed zero. Callers supply clock, desired-topic store/parent
reads, warning logging and JSON.stringify/send as fallible capabilities.
Callbacks receive mutable state without a retained Store borrow guard.
The desired-topic test adapter calls the already-ported Rust selection helper
with input market/active symbols and current topic count, not oracle outputs.

Tracking happens before connectivity guards. Batches keep input array order
and duplicates while tracked topic Sets deduplicate. Budget rejection does
not stop subsequent batches. Reconciliation unsubscribes first, subscribes
second, and commits the reconciled ticker Set only after the operations finish.
Rejected operations preserve the corresponding members; exceptions preserve
state changes already performed. Unsubscribe ignores a false operation result
and still reconciles. Disposal skips active-symbol reconciliation.

No full websocket class, constructor, transport or host-runtime implementation
is claimed. State fields use declared value projections; arbitrary field
getters, mutable input-array aliases and patched Set primitives are outside
this slice. The ordered-unique vector invariant is part of the Set projection.

## Gates at d4ed0f8c3322dfe7211900a9bac74c1c22e6b0cf

All cargo commands ran serially. Oracle environment:
SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript.

| Command | Measured exit / result |
| --- | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --all-targets --jobs 2 | 0 |
| python3.12 sdk-rs/safe-cex-venue-weex/tests/subscriptions-mutations.py | 0; eight compiled assertion failures, each individually restored and passed |
| python3.12 sdk-rs/safe-cex-venue-weex/tests/parity-check.py | 0; canonical invention/status gates passed: 156 rows, 92 public Rust items, zero WEEX exemptions |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0; output: 7341 symbols, zero updates/added/orphaned |

The mutation command runs only the affected target:
cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --jobs 2
--test subscriptions; named test subscription_state_matches_actual_typescript.
A draft target passed before the commit; the SHA-bound passes are the eight
restored target runs in the mutation command.

## Native proof

subscriptions-oracle.cjs extracts eight actual private source field functions,
their actual desired-topic helper dependency and all four actual constants
from TypeScript ASTs. It transpiles those declarations with the actual shared
LogSeverity enum. It does not copy function bodies or constant values.
The serializer hook captures the source's native packet fields before actual
JSON.stringify. Native outcomes, state fields, method variants, UTF16 topic
vectors, binary64 IDs/timestamps and warning messages are asserted directly
before effect traces. There is no roundtrip or native wire comparison.
JSON serialization itself remains an explicit host capability.

The first corpus iteration stops at 59 cases and 60 calls, with fixed ordered
IDs and literal totals independent of the artifact: 15 operation, 10 batch,
10 reconciliation, 5 ticker, 4 resubscription, 6 subscribe, 5 unsubscribe and
4 active-symbol cases. Independent native assertions also require 50/50/1
batches for 101 topics, 11 tracked topics and 89 ticker slots after adding a
topic to ten existing topics, and signed-zero input bits.

Cases cover the exact one-hour expiry boundary, 239/240 budget, log-once,
pruning, future/nonfinite timestamps, nonfinite/signed-zero IDs, clock/log/
serializer/send errors, duplicates, UTF16, disconnected/disposed guards,
continuation after rejection or synchronous disconnection, and partial state
when reconciliation operations fail. This enumeration is a coverage floor;
broader callback reentrancy/state mutation was not enumerated.

Each production mutation compiled, failed its named native assertion with
exit 101, restored the original bytes, then passed the target with exit 0:

| Mutation | Named assertion, prefixed subscriptions:: | Restored exit |
| --- | --- | --- |
| Change cap >=240 to >240 | operation/budget240::result | 0 |
| Reverse tracking guard | batch/disconnected_track::state_topics | 0 |
| Clear disconnected ticker Set instead of replacing | reconcile/disconnected_replace::state_tickers | 0 |
| Clear initial ticker Set instead of selecting | ticker/basic::state_tickers | 0 |
| Resubscribe only when topic list is empty | resubscribe/basic::state_timestamps | 0 |
| Omit adding topic before reconciliation | subscribe/basic::state_tickers | 0 |
| Keep the topic being unsubscribed | unsubscribe/basic::state_topics | 0 |
| Reverse disposed guard | changed/disposed::state_tickers | 0 |

The script required exit 101, the named assertion and assertion left==right
failure text, then individually restored/passed; a finally block restored
original bytes. Eight controls caught, zero survivors among these controls.
Other mutations were not enumerated. Compiler failures are not accepted.

Sub-agent weex_review inspected actual source, all eight functions, typed
projections, native fixtures, mappings and controls; no concrete findings.
It ran no cargo, tests or mutations and made no edits or commits.

Not run: workspace compilation, full venue/monorepo suites, host clippy,
wasm clippy, live venue requests or CI reruns. No income pager was added.
Ignored local logs: sdk-rs/target/weex-subscription-gates/.

## Delivery merge

Merged integration `75c4e8562c72cf36338eaefaf5df8b9b4a4d0f2d` into
`0f390a6e4b9acd5866aaf2f2625ccfa149e5beae`, with no conflicts.
The integration SHA is the actual second merge parent, not a later moving
shared remote-tracking ref. A committed-manifest audit against that parent
exited 0 and again required exactly the eight transitions and counts above.

Git diff --exit-code from the implementation SHA to this merge exited 0 for
WEEX src/tests/manifests, workspace Cargo manifest/lock, shared types/num/store,
actual WEEX TypeScript/shared enums/logging enum and Cargo configuration.
Cargo check and mutation results remain bound to their original SHA; they
were not rerun at this merge because those inputs are byte-unchanged.
Global exemptions and package.json changed, so the affected gates were
remeasured at the merge SHA rather than transferring their results:

| Command | Exit at 0f390a6e4b9acd5866aaf2f2625ccfa149e5beae |
| --- | --- |
| python3.12 sdk-rs/safe-cex-venue-weex/tests/parity-check.py | 0; 156 rows, 92 public items, zero WEEX exemptions |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0; 7341 symbols, zero updates/added/orphaned |

This evidence commit changes only this narrative.

## Final delivery fetch

Fetched integration again, captured this worktree's FETCH_HEAD as
`661022ccc76613805ba357730a6a30475608c4ea`, and merged that exact SHA into
`89a719c408a7da8b4162ad11a3bdd00975b095ef`, with no conflicts. This avoids
mistaking another worktree's moving remote-tracking ref for our merge parent.

The same compiler/oracle/mapping input diff against the original measured SHA
again exited 0. Canonical parity checker, global exemptions, package scripts
and WEEX mappings are unchanged since the prior parity measurement at
0f390a6e4b9acd5866aaf2f2625ccfa149e5beae; canonical parity was not rerun
at this final merge. Three other lanes' manifests changed, so extractor check
was remeasured at 89a719c408a7da8b4162ad11a3bdd00975b095ef: exit 0, output
7341 symbols, zero updates/added/orphaned. Committed-TOML comparison against
the exact fetched integration SHA again exited 0: eight pending-to-ported,
zero divergent, counts 52 / 26 / 78. Cargo and mutation measurements retain
their original SHA. This follow-up evidence commit changes only the narrative.
