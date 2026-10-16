Public websocket lifecycle, cap/cache and valuation evidence

Actual code/gate SHA: `06c0dba6741006d7f47a307d9bd786a7e72f055f`. Fresh integration parent: `ef054ba0b8e9038241576f44c09f652348f2f4bb`. No parked/unmerged external feature branch was inherited. Raw logs/gates.json attach every exact command, SHA, variant and exit. Mutations are only documented patches to src/public_lifecycle.rs; no compilation failure is accepted as a runtime control. Subsequent evidence-only tips are not relabelled as measured Cargo gate SHAs.

Rows moved: NINE pending -> ported, ZERO -> divergent. Eight complete function bodies (connection, open/reconnect, capped market subscription, cached index lookup, valuation update, candlestick dispatch, unsubscribe, dispose), plus the two-field CryptocomMarketDataSubscription native schema. Original tomllib census uses independently expected nine source IDs. Crypto.com49/18/70 ->58/18/61 (ported/divergent/pending). Six-file/137-row census floor stops cryptocom.ws-public.ts / CryptocomPublicWebSocket.dispose. A census is not proof of remaining pending rows.

Native witness floor:162 scenarios, stopped at index161 dispose-index-map-retained. Source executes actual complete TypeScript arrow initializers and reads actual WSS_URLS,350/400 caps, market type-literal members, and Warning initializer. Native assertions decode UTF-16 (including NUL/astral/lone-surrogate strings), fields/optional undefined read values, f64 bits/signed zero, insertion order, original pointer identities/errors/receivers, patch field names, callback return values, effect order and cleanup before final inventory assertions. No Rust encoder, re-encoding comparison, or wire roundtrip is used. Source wire encoding is only required by the existing unsubscribe source call; frames are captured and native-asserted first. Explicit input expected frame/registration/model counts are independent of source output, as are162 scenarios, two model fields and350/400 constants.

Important cases include ticker priority over all enrichments; cap346/347/348/349/350/400 and349/351 markets; duplicate channels at cap; shared-index associations; ignored registration values; cached-empty Set identity; no empty-result caching; custom map results; sparse catalogs; Map replacement; String/Number finite/positive guards and funding negative zero; live ID growth/deletion; custom IteratorClose side effects on throwing body with throwing close preserving original error; sparse/captured-length/custom forEach callbacks and return values; original unsubscribe alias changing at clock/send; Set/Map replacement/retained objects; dispose clearing Map while retaining previously stored Set values.

The native schema represents source field READ types: Vec<u16> and Option<Vec<u16>> (None is undefined from absent or explicit-undefined). It does not implement own-property enumeration/presence/descriptors, identity or a JSON codec. All bodies are complete at declared typed state, stock native primitive semantics and explicit callee/array/filter/map/iterator/store/timer/socket/log/JSON boundaries. JS host adapters/class binding/prototype mutation and live venue orchestration remain pending. Host iterator implementations own IteratorClose effects (including original body error priority); the test Drop adapters demonstrate this, not a universal JS iterator adapter completion. No claim is made that the public websocket class/constructor or live transport is implemented. Static scope/review is in review.md.

Control floor:8 planned families through disposal(index7). All compiled; each failed its expected named native assertion101; exact restoration passed0. Each mutation JSON records the failing scenario/index where that variant stopped. Baseline and each restored run completed all162 scenarios. No survivors among these eight controls. This is not exhaustive mutation enumeration, and older slices' controls were not repeated.

| Measured command/variant | Exit | Raw evidence |
| --- | --- | --- |
| native-baseline | 0 | [native-baseline.log](native-baseline.log) |
| connection-mutation | 101 | [connection-mutation.log](connection-mutation.log) |
| connection-restored | 0 | [connection-restored.log](connection-restored.log) |
| timer-mutation | 101 | [timer-mutation.log](timer-mutation.log) |
| timer-restored | 0 | [timer-restored.log](timer-restored.log) |
| cap-and-model-mutation | 101 | [cap-and-model-mutation.log](cap-and-model-mutation.log) |
| cap-and-model-restored | 0 | [cap-and-model-restored.log](cap-and-model-restored.log) |
| index-cache-mutation | 101 | [index-cache-mutation.log](index-cache-mutation.log) |
| index-cache-restored | 0 | [index-cache-restored.log](index-cache-restored.log) |
| valuation-mutation | 101 | [valuation-mutation.log](valuation-mutation.log) |
| valuation-restored | 0 | [valuation-restored.log](valuation-restored.log) |
| candlestick-mutation | 101 | [candlestick-mutation.log](candlestick-mutation.log) |
| candlestick-restored | 0 | [candlestick-restored.log](candlestick-restored.log) |
| unsubscribe-mutation | 101 | [unsubscribe-mutation.log](unsubscribe-mutation.log) |
| unsubscribe-restored | 0 | [unsubscribe-restored.log](unsubscribe-restored.log) |
| disposal-mutation | 101 | [disposal-mutation.log](disposal-mutation.log) |
| disposal-restored | 0 | [disposal-restored.log](disposal-restored.log) |
| venue-tests | 0 | [venue-tests.log](venue-tests.log) |
| check | 0 | [check.log](check.log) |
| host-clippy | 101 | [host-clippy.log](host-clippy.log) |
| wasm-clippy | 101 | [wasm-clippy.log](wasm-clippy.log) |
| fmt | 0 | [fmt.log](fmt.log) |
| writer | 0 | [writer.log](writer.log) |
| extractor | 0 | [extractor.log](extractor.log) |

Both host and wasm clippy measured101, each blocked by existing safe-cex-transport/src/tealstreet.rs:157 and:177 arc_with_non_send_sync diagnostics. These are FAILED gates; venue lint surface beyond that dependency failure is not certified. Scoped check and venue tests0 are not workspace compilation claims. No suppression or unrelated transport repair was performed.

Not run: workspace/repo-wide builds/suites, canonical index build, wasm runtime tests, live exchange/API network tests, CI reruns, human review, exhaustive mutations, wire encoder comparisons, global prototype overrides, completed JS host adapter/class integration. Local path dependency/config/toolchain source inputs are enumerated recursively (8 crates) in gate-inputs.json, with Cargo.lock covering external resolution.

Post-gate merge survival and any additional measurements belong to their actual merge SHA in separate merge receipts. Earlier failed clippy exits never become passing claims merely because a merge occurred. No PR was opened and the lane never merged into integration.

Integration merge and final rechecks

Merged integration `61102ead04c728512ff39ee6ae5cf952e82bf2ad` down, producing `6cc1419ae9921b47373219dba281d68a70af9113` (actual parents in merge.json). Original fresh parent is ef054ba0b8. No unmerged external feature ancestor was introduced. Merge-only movement:0 rows.

Scoped source/tests diff from 06c0dba6741006d7f47a307d9bd786a7e72f055f to the merge SHA measured0, including source of all eight proven controls. Recursive input comparison measured1 solely for Cargo.lock: OKX added indexmap and safe-cex-crypto dependencies. All local dependency sources/Cargo manifests, actual TS source inputs, source caps/Warning enum, private/public crate source/tests, and toolchain/config were unchanged; separate config receipt measured0.

Because lockfile input changed, the scoped crate gates were rerun sequentially at this MERGE SHA. Exact measured exits: venue tests0, check0, host clippy101, wasm clippy101, fmt0, writer0, extractor0. Both clippy failures remain the existing transport arc_with_non_send_sync diagnostics at tealstreet.rs157/177; they are not passing gates. Writer text Updated0 manifests;0 added,0 orphaned symbols; extractor text Would update0 manifests;0 added,0 orphaned symbols. No mutation matrix was rerun after merge because the controlled source/tests were byte-unchanged. Native162-case witness ran again as part of the crate tests.

merge-census.json independently compares the actual merge SHA to its actual integration parent: nine pending -> ported, zero -> divergent, Crypto.com58/18/61, same six-file/137-row floor. Additional commands/exits/raw diagnostics are in merge-gates.json/merge-*.log. Subsequent evidence-only tip is not a newly measured Cargo gate SHA; source/tests survival remains separately checked before push.
