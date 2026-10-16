# Three ported Bitrue WebSocket primitives

Fresh branch parent: `df065d29b58fd12bdc103e2ef156c8de602663a0` (reviewed integration, including merged PR7380). Code commit `e065dfdb65` introduced the primitive bodies/witnesses. Integration `1181108d2dfeee7dd6cb1d21a3a39922e1e13b9e` was merged down before the initial measured SHA `a92859adcdbb90a778150f3fdfa59956160cd2be`. An exact helper-target note was corrected at that measured SHA after sub-agent review caught it.

Before delivery, integration `ed7ac407c716cea8b8115aea53e466c9ac69d129` was merged down cleanly at `0b21217cc5b190c3fccee8cb1695025131526d65`. No foreign unmerged work is an ancestor. The initial primitive source/tests/oracle and compared shared sources/package manifests are byte-unchanged (diff exit0, post-merge-byte-comparisons.json). Initial gate numbers remain attached to their actual SHA; the separate post-merge table lists only runs actually repeated at the later SHA. The delivery evidence commit changes documentation/logs only.

## Behavior and scope

BitruePrivateWebsocket.isSuccessfulCode returns true only for number0, number200, string"0", string"200". SuccessCode represents the entire unknown-value argument domain without a UTF-8 refusal: Number carries f64, String carries UTF-16 units, Other combines all remaining values, each strictly comparison-equivalent false without property reads or coercion. Rust returns a native boolean. The enum is a real input model mapped alongside the function, with its exact target named in the note.

Both public/private ping bodies are empty in the maintained TypeScript because ping direction is reversed. Their Rust bodies are empty and return native unit. These are stateless field-body ports, not constructor, WebSocket transport, scheduling or caller completion. Other class rows remain pending.

The witness extracts and executes each unchanged actual TypeScript arrow initializer. It checks actual decoded Rust booleans against both independent accepted-case expectations and source booleans. Source undefined ping results decode to native unit; Rust panic outcomes become Err, which the named native comparison rejects. All native result assertions precede inventory/effect counters; no Rust result is serialized or roundtripped for comparison. Exact expected case IDs and counts are declared independently in Rust. TypeScript captures UTF-16 units and f64 bits without coercing other values.

The empirical floor is46code cases through o-proxy-trap, then2pings through public. It includes signed zero, NaN/infinities/subnormals, the next number above200, ordinary and lone/paired-surrogate strings, wrappers, BigInt, Symbol, functions, arrays, and throwing coercion/proxy hooks. It is not exhaustive test enumeration. Source equality semantics justify the argument partition independently of this floor.

## Runtime discrimination

At base a92859adcd, three separate production mutations were run sequentially, each with exact patch JSON and dirty-variant labels. Each compiled and failed its named native assertion at runtime with exit101; its source bytes were restored in finally before a clean-tree run exited0. No compilation failure counts as a discriminator, and no attempted control survived.

| Row/control | Mutation | Named assertion | Failure floor | Mutation/restored exits |
| --- | --- | --- | --- | --- |
| private isSuccessfulCode | numeric200 comparison changed to201 | BITRUE-WS-CODE-NATIVE independent n-200 | code index2, n-200 | 101 / 0 |
| private ping | empty body changed to panic | BITRUE-WS-PING-NATIVE private | all46codes, then first/private ping | 101 / 0 |
| public ping | empty body changed to panic | BITRUE-WS-PING-NATIVE public | all46codes, then second/public ping | 101 / 0 |

These are one wrong-result/exception control per row, not an exhaustive mutation census. Restored runs reached all46code inputs and bothpings. The witness was not edited during the controls.

## Rows moved

Independent Python3.11 tomllib comparisons in census.json and post-merge-census.json show **3 pending->ported, 0 pending->divergent**. Rows: BitruePrivateWebsocket.ping, BitruePrivateWebsocket#isSuccessfulCode, BitruePublicWebsocket.ping. Bitrue changes from **15ported/25divergent/67pending** to **18ported/25divergent/64pending**. No source-row additions or exemption changes were made. The census floor stops after6manifests/107rows at BitruePublicWebsocket.listenOrderBook; this is a status census, not complete venue proof.

## Completed gates

All Cargo ran sequentially; compiling commands used jobs2. The venue oracle used SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript. Exact commands, SHAs, variants, times, measured exits and log paths are in gates.json and post-merge-gates.json. Node/yarn output was read: writer Updated0 manifests/0added/0orphaned; check Would update0/0added/0orphaned at both measured SHAs.

Initial measurements at a92859adcdbb90a778150f3fdfa59956160cd2be:

| Gate | Exit |
| --- | ---: |
| native-baseline | 0 |
| success-code-mutation | 101 |
| success-code-restored | 0 |
| private-ping-mutation | 101 |
| private-ping-restored | 0 |
| public-ping-mutation | 101 |
| public-ping-restored | 0 |
| scoped-check | 0 |
| host-clippy | 0 |
| wasm-clippy | 0 |
| fmt | 0 |
| canonical-index | 1 |
| extractor-writer | 0 |
| extractor-check | 0 |

Post-merge measurements at0b21217cc5b190c3fccee8cb1695025131526d65:

| Gate | Exit |
| --- | ---: |
| post-merge-native | 0 |
| post-merge-check | 0 |
| post-merge-writer | 0 |
| post-merge-extractor | 0 |

The canonical repository index **exited1** before manifest validation: the inherited safe-cex-webworker/src/twap_manager.rs thread_local macro cannot be indexed without expansion. Its source and validator are byte-unchanged from the merged integration snapshot (canonical-gap.json); the index was not run at that integration commit. No canonical Bitrue mapping pass is claimed, no diagnostic was suppressed, and no exemption or gate was weakened.

Native sub-agent static review at a92859adcd found no remaining blockers, approved the full-domain comparison partition and the three ported-body claims, and read the root-run mutation logs. The reviewer ran no gates. The exact helper-target note finding was fixed and re-reviewed.

Not run for this slice: full Bitrue/Store/Exchange/Hyperliquid suites, other package checks/clippy, workspace builds/suites, wasm runtime, the previous balance-guard mutation or old mutation collections, live exchange requests, or CI. Host/wasm clippy, fmt and mutation controls were not repeated at0b21217; their earlier measured results and unchanged-source comparison are explicit above. No unmeasured iteration or class completion is claimed.
