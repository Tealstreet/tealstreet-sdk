Private websocket lifecycle evidence

Code/gate SHA: `cce8c97b0a0c965a25c360d8928827888eaf03a1`. Actual fresh integration parent: `85f3b9e596582fce3738807c4d9b7032c5b5a802`. No parked feature-branch work was inherited. Each raw log and gates.json records the SHA, exact command, variant and measured exit; mutation JSON supplies the only source patch. An evidence-only tip is not relabelled as the measured code SHA.

Nine rows moved pending -> ported, zero -> divergent: connection, open/timer, authentication, replay, order/position/balance subscription helpers, unsubscribe, dispose. `census.json` uses tomllib against both commits, with nine independently expected symbol IDs. Crypto.com counts: 40/18/79 -> 49/18/70 (ported/divergent/pending). Census floor: six manifest files, 137 rows, stopped at cryptocom.ws-public.ts / CryptocomPublicWebSocket.dispose. Census does not prove all pending rows implemented.

Native witness floor: 84 input scenarios, stopped at index83 dispose-retained-set. Actual complete TypeScript arrow initializers and actual WSS_URLS initializer are executed by the oracle. Rust asserts native fields/state/UTF-16/number bits, original error/auth message/URL/prepared handle/old clear-timer identity, effect order and immediate pre-poll effects before inventories; no Rust wire encoding or decode-reencode comparison. Expected frame/auth counts are explicit input expectations, not copied from oracle output. Source custom forEach observes callback deletion booleans; clock/send array growth retains original channel carrier and later deletion semantics.

Declared scope is complete function bodies at typed state / native carrier / explicit host boundaries. JS class-field binding, URL/options builtins, dynamic property/array/builtin/Set adapters, JSON/stringification, actual socket/timer/generator/parent bindings and live transport integration remain pending. Static reviewer verdict and empirical socket/new-timer pointer-identity limitations are in review.md. The old TIMER HANDLE is retained and observed; no old timer CALLBACK firing proof is claimed. Source ignored rejected promises are exercised; the native timer return is opaque and ignored at the typed boundary.

Controls floor: seven planned families through disposal (index6), all compiled, each failed a named native assertion (101), each exact restoration passed (0). No survivors in those seven controls. This is not an exhaustive mutation enumeration and does not repeat earlier slices' controls. Controls ran one at a time with --jobs 2. Restoration is byte-exact; mutation variants are not clean-tree gate claims.

| Measured command/variant | Exit | Raw evidence |
| --- | --- | --- |
| native-baseline | 0 | [native-baseline.log](native-baseline.log) |
| connection-mutation | 101 | [connection-mutation.log](connection-mutation.log) |
| connection-restored | 0 | [connection-restored.log](connection-restored.log) |
| timer-mutation | 101 | [timer-mutation.log](timer-mutation.log) |
| timer-restored | 0 | [timer-restored.log](timer-restored.log) |
| authentication-mutation | 101 | [authentication-mutation.log](authentication-mutation.log) |
| authentication-restored | 0 | [authentication-restored.log](authentication-restored.log) |
| subscriptions-mutation | 101 | [subscriptions-mutation.log](subscriptions-mutation.log) |
| subscriptions-restored | 0 | [subscriptions-restored.log](subscriptions-restored.log) |
| replay-mutation | 101 | [replay-mutation.log](replay-mutation.log) |
| replay-restored | 0 | [replay-restored.log](replay-restored.log) |
| unsubscribe-callback-mutation | 101 | [unsubscribe-callback-mutation.log](unsubscribe-callback-mutation.log) |
| unsubscribe-callback-restored | 0 | [unsubscribe-callback-restored.log](unsubscribe-callback-restored.log) |
| disposal-mutation | 101 | [disposal-mutation.log](disposal-mutation.log) |
| disposal-restored | 0 | [disposal-restored.log](disposal-restored.log) |
| venue-tests | 0 | [venue-tests.log](venue-tests.log) |
| check | 0 | [check.log](check.log) |
| host-clippy | 101 | [host-clippy.log](host-clippy.log) |
| wasm-clippy | 101 | [wasm-clippy.log](wasm-clippy.log) |
| fmt | 0 | [fmt.log](fmt.log) |
| writer | 0 | [writer.log](writer.log) |
| extractor | 0 | [extractor.log](extractor.log) |

Both host and wasm clippy measured 101, each blocked by existing safe-cex-transport/src/tealstreet.rs:157 and :177 arc_with_non_send_sync diagnostics. The scoped Rust compile floor and venue tests are green at this SHA, but these clippy gates are FAILED; the venue lint surface is not certified past that dependency failure.

Host and wasm clippy are separate measured exits; a 101 is a failed gate, never a passing claim based on another target or older SHA. Any diagnostics are preserved in their raw logs; no lint suppression or unrelated dependency repair is included.

Not run: workspace/repository-wide build or tests, canonical index build, wasm runtime tests, live Crypto.com API/network tests, CI reruns, human review, wire encoder comparison, exhaustive mutation controls, empirical native socket/new-timer pointer identity. No PR was opened or integration branch merged into by this lane.

Post-gate integration merge and unchanged-source/recursive-input comparisons must be read separately in the final merge receipt; they cover their recorded SHA only. gate-inputs.json recursively lists local dev/build/target/path dependencies (eight crates), not just direct venue sources, with Cargo.lock covering external resolutions. Additional integration changes never turn an earlier failed clippy into a pass.

Integration merge survival

Merged integration `3042a36afe21890c7cb5eaa5ba78333f35e5bd40` into the lane, producing `5f0da30e55d8550278832ddf073ea65c5b20c3e1` (parents recorded in merge.json). Merge-only movement: 0 rows. Fresh original parent remains 85f3b9e596; no unmerged external feature ancestor was introduced.

Scoped source/tests survival diff from cce8c97b0a0c965a25c360d8928827888eaf03a1 to this merge SHA measured 0. Recursive local dependency inputs, workspace Cargo.toml/lock, actual TS sources and private manifest diff also measured 0; toolchain/config comparison separately measured 0 in merge-config-survival.log. No Cargo tests/check/clippy/fmt were rerun after this merge because those inputs were unchanged. The exact earlier exits stand: tests/check/fmt/writer/extractor 0; host AND wasm clippy 101. This merge does not clear the transport clippy findings. Extractor rerun at the MERGE SHA measured 0 with Would update 0 manifests; 0 added, 0 orphaned symbols.

merge-census.json independently compares this merge SHA to its actual integration parent: nine pending -> ported, zero -> divergent, Crypto.com49/18/70, same six-file/137-row enumeration floor. Raw commands/exits are in merge-gates.json and merge-*.log. The subsequent evidence-only commit is not a newly measured Cargo gate SHA.
