# Eight ported Crypto.com WebSocket policy fields

Fresh branch parent: `cd7371a9b93dd166cfe6fcb5163f6a5439640dc5`. Code commit `71f46e925c33eb989b16676595b2c594dc1bc480` ports four bodies in each socket. Integration `3e4e053b47fbfa44475712b68fdf53c23f5071cd` was merged down before measurements at `46d08011f1c214801efde108c82bd168da0291f2`.

Before delivery, integration `2c04dafa36dd53b183f10b974fcd0a4adf2ada1a` was merged down cleanly at `c07a7cac17d52b3284d601a32aa257720dc42c5c`. No foreign unmerged branch is an ancestor. Source/tests and compared shared source/package manifests are byte-unchanged (post-merge-byte-comparisons.json, exit0). Earlier exits retain their actual SHA; only the separate post-merge table represents repeated gates. The delivery evidence commit changes no source, tests, or manifests.

## Actual behavior and witnesses

Each source class has the same four stateless arrow bodies: shouldEnforcePongMissReconnect returns false, getWatchdogStaleThresholdMs returns the immutable reconnect constant90,000ms, getWatchdogPingThresholdMs returns the immutable ping constant30,000ms, and ping has an empty body/void return. Rust reuses the existing types.rs constants and returns native boolean, f64, or unit. These bodies read no argument or receiver state. Class construction, transport, scheduling and callers remain pending.

The oracle executes the unchanged complete actual TypeScript constants module, then each unchanged actual arrow initializer with strict lexical receiver binding and the actual constant values. A receiver proxy rejects state access/mutation. Rust decodes source numeric bits into f64 and directly compares its native numbers, booleans, and unit outcomes to both independent expected values and source native results. Ping exceptions become a distinct Native::Panic result, causing the named assertion to fail. No Rust result is re-encoded or roundtripped. All native comparisons precede fixed eight-ID inventory and effect checks.

The empirical floor is one call to each of eight stateless field bodies, stopping at index7 public/ping. The independent expected inventory is explicitly declared in Rust and is not derived from the captured artifact. This is not exhaustive mutation or class/venue coverage.

## Runtime controls

At base46d08011f1, each control perturbs only its named Rust function, with exact JSON-preserved patches and dirty-variant labels in the logs. Each compiled and exited101 due to its named native `left == right` assertion, then source was restored in finally and the clean test exited0. No compilation failure is counted as proof. There were no survivors among the eight attempted controls.

| Socket | Method | Mutation | Stop index | Mutated/restored exits |
| --- | --- | --- | ---: | --- |
| private | shouldEnforcePongMissReconnect | false to true | 0 | 101 / 0 |
| private | getWatchdogStaleThresholdMs | constant +1.0 | 1 | 101 / 0 |
| private | getWatchdogPingThresholdMs | constant +1.0 | 2 | 101 / 0 |
| private | ping | empty body to panic | 3 | 101 / 0 |
| public | shouldEnforcePongMissReconnect | false to true | 4 | 101 / 0 |
| public | getWatchdogStaleThresholdMs | constant +1.0 | 5 | 101 / 0 |
| public | getWatchdogPingThresholdMs | constant +1.0 | 6 | 101 / 0 |
| public | ping | empty body to panic | 7 | 101 / 0 |

Each named assertion is CRYPTOCOM-WS-NATIVE socket/method independent. Restored runs complete all eight fields through public/ping. The witness is byte-unchanged during controls. Eight control pairs are a floor, not a claim that every possible mutation was tested.

## Rows moved and review

Independent Python3.11 tomllib comparisons (census.json and post-merge-census.json) show **8 pending->ported, 0 pending->divergent**. Crypto.com changes from14ported/18divergent/105pending to **22ported/18divergent/97pending**. Only the four named fields in each WebSocket are reclassified; no source rows or exemptions were added. Census stopped after6manifests/137rows at CryptocomPublicWebSocket.dispose. Census counts and parity evidence are separate claims.

Native sub-agent static review at46d08011f1 found no blockers in the bodies, source oracle, native comparisons or exact manifest targets. A second log review matched the independent eight-control table to all raw runtime101 failures and restored0 passes, finding no survivors. The reviewer ran no Cargo. No class/transport completion is claimed.

## Completed gates

Compiling Cargo commands used jobs2 and all Cargo ran sequentially. The venue oracle used SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript. The exact commands, variants, SHAs, times and exits are in gates.json and post-merge-gates.json. Writer/check output was read: Updated0/Would update0 manifests; 0added/0orphaned at both measured SHAs.

Initial measurements at46d08011f1c214801efde108c82bd168da0291f2:

| Gate | Exit |
| --- | ---: |
| native-baseline | 0 |
| private-flag-mutation | 101 |
| private-flag-restored | 0 |
| private-stale-mutation | 101 |
| private-stale-restored | 0 |
| private-ping-threshold-mutation | 101 |
| private-ping-threshold-restored | 0 |
| private-ping-mutation | 101 |
| private-ping-restored | 0 |
| public-flag-mutation | 101 |
| public-flag-restored | 0 |
| public-stale-mutation | 101 |
| public-stale-restored | 0 |
| public-ping-threshold-mutation | 101 |
| public-ping-threshold-restored | 0 |
| public-ping-mutation | 101 |
| public-ping-restored | 0 |
| scoped-check | 0 |
| host-clippy | 0 |
| wasm-clippy | 0 |
| fmt | 0 |
| extractor-writer | 0 |
| extractor-check | 0 |

Post-merge measurements atc07a7cac17d52b3284d601a32aa257720dc42c5c:

| Gate | Exit |
| --- | ---: |
| post-merge-native | 0 |
| post-merge-check | 0 |
| post-merge-writer | 0 |
| post-merge-extractor | 0 |

Not run for this slice: full Crypto.com or other package suites, workspace compilation/suites, canonical repository index, standalone checks/clippy for other packages, wasm runtime, old Crypto.com mutation collections, previous Bitrue controls, live exchange requests, or CI. Host/wasm clippy, formatter and mutation pairs were not repeated atc07a7; their initial measured results and explicit unchanged-source comparison are recorded above. No earlier exit is relabeled as a latest-SHA run.
