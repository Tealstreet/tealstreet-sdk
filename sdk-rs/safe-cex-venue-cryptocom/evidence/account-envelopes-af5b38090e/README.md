# Crypto.com account/envelope contracts and API error formatter

Code/gate SHA `af5b38090e039e6fcf9cd3c3f4e5046ae7a066bf`; actual fresh integration parent `ff33a3d2b187e596e1c58511c3bf35b6306d4d7b`. No unreviewed lane work is an ancestor; parent is the requested integration head. Later evidence-only commits do not claim new Cargo measurements.

**8 pending → ported/adapted, 0 divergent**: seven remaining source static interfaces plus the complete `formatCryptocomApiError` body. TOML comparison via tomllib: 76/18/43 → 84/18/35 (ported/divergent/pending), floor137 rows across6files, stopping `['sdk-rs/parity/manifest/exchanges/cryptocom/cryptocom.ws-public.ts.toml', 'CryptocomPublicWebSocket.dispose']`. This is venue census, not repo completeness proof.

`src/account_envelopes.rs` covers OrderRequest, Position, AccountSummary (anonymous Account), WsRequest, WsResponse, WsSubscription (anonymous Params), and generic ApiResponse<T>. Three literal carriers cover six request types, four time-in-force values and singleton subscribe; existing native WS enums supply side/ref-price/position/isolation. Every field matches its source static type/optionality. Primitive strings retain UTF16 and Number fields f64. Generic native unknown/record references and caller T remain opaque original carriers. Optionality means member read: absent and read-undefined both None; null unknown remains Some. Generic T is an explicit FnOnce projector, with T=V identity specialization tested, not every caller projector/error case. NativeRecord<V> retains original string-keyed object.

This is static member projection, not runtime interface validation or JSON/property binding. Getter/prototype/own-key/array/object alias bindings outside original generic record/result carriers, JSON/codecs/native ABI and class/API wiring remain pending. No shared crate/type/trait is edited; new traits are venue-local.

`src/api_error.rs` preserves the full actual formatter arrow body: status from first optional response chain, body from fresh second; undefined-body early return; record/code/first-message-type test and conditional SECOND message read; strict Number0 suppression; original code/message template order; stringifyResponseBody before status truthiness/relational>=500; proxy/HTTP/API fallback messages retain exact casing and original native operands/errors. Property operations, isRecord/stringifyResponseBody helper bindings, template/coercion/relational operators, builtin/JSON/proxy/Error ABI remain explicit dependencies. Source helpers themselves are executed by the oracle with captured native dependency returns. There is no Rust JSON encoder or wire/roundtrip comparison.

Direct decoded native values, enum variants, optional presence, UTF16 units, f64 bits and original generic pointers are asserted BEFORE independent schema/census. The decoded params/result carrier remains alive across parent field replacement; the old object can still mutate. Source schema checks after native checks reject missing/added fields, literal/optional changes, inheritance, readonly, or generic constraints/defaults within this measured source floor.

Independent type fixture floor **235**, stops index234 `CryptocomAccountSummary:account-undefined-account_id`; formatter floor **59**, stops index58 `status-coercion-throws`. Positive declared types include empty/omitted/read-undefined strings, all closed literal variants, boolean values, numeric -0/NaN/±Infinity/extrema/subnormal, empty/multiple rows and opaque unknown null/object/array/function/symbol/bigint. Formatter cases include different OLD/NEW response bodies (stale response use discriminated), repeated message getter with changing type, strict code0 vs bigint0, helper undefined/original-object results, JSON/String fallback, original getter/record/coercion errors. These are bounded floors, not every input/class/schedule/host mutation.

Sub-agent static review `review.md` binds eight rows/54 distinct actual qualified targets to the code SHA; no missing/nonexistent targets in that scope. Reviewer ran no gates. Provisional compiler failures (borrowed count, unused checker args/function) were corrected before this SHA; they are not runtime proof. Review caught same-body fresh-response fixture and non-configurable getter redefinition; fixed with distinct actual response carriers before this SHA.

## SHA-bound measured gates

Exact commands/env/exits/logs in `gates.json`, `baseline.json`, `controls.json`. Cargo ran sequentially with jobs2; TypeScript path set. Raw process exits are measured directly, not tail/pipeline statuses. Yarn output inspected as well as exit.

| Gate | Exit | Raw log |
|---|---:|---|
| New native baselines235+59 | 0 | `baseline.log` |
| venue-tests | 0 | `venue-tests.log` |
| check | 0 | `check.log` |
| host-clippy | 101 | `host-clippy.log` |
| wasm-clippy | 101 | `wasm-clippy.log` |
| fmt | 0 | `fmt.log` |
| writer | 0 | `writer.log` |
| extractor | 0 | `extractor.log` |

Host and wasm clippy each exited 101 at inherited `safe-cex-transport/src/tealstreet.rs:157,177` (`clippy::arc_with_non_send_sync`), before reaching the venue. These are separate measurements and neither is a lint pass. Writer/extractor must state Updated/Would update0 manifests;0 added,0 orphaned.

## Selected discriminating controls

One per new row. Documented variants compiled and failed at runtime at named native assertions, then exact committed bytes restored and both full native floors passed. Every variant is base SHA plus retained patch; see controls.json. Floor8 stops index7 formatter; no survivor within this selected floor, no exhaustive mutation claim.

| Row family | Named assertion prefix | Runtime exit | Exact restoration exit |
|---|---|---:|---:|
| order-request | `CRYPTOCOM-ACCOUNT-NATIVE-STRING CryptocomOrderRequest:full.instrument_name` | 101 | 0 |
| position | `CRYPTOCOM-ACCOUNT-NATIVE-STRING CryptocomPosition:full.quantity` | 101 | 0 |
| account-summary | `CRYPTOCOM-ACCOUNT-NATIVE-NUMBER CryptocomAccountSummary:full.accounts[0].available_balance` | 101 | 0 |
| ws-request | `CRYPTOCOM-ACCOUNT-NATIVE-NUMBER CryptocomWsRequest:full.id` | 101 | 0 |
| ws-response | `CRYPTOCOM-ACCOUNT-NATIVE-NUMBER CryptocomWsResponse:full.code` | 101 | 0 |
| ws-subscription | `CRYPTOCOM-ACCOUNT-NATIVE-STRING CryptocomWsSubscription:full.params.channels[0]` | 101 | 0 |
| api-response | `CRYPTOCOM-ACCOUNT-NATIVE-NUMBER CryptocomApiResponse:full.code` | 101 | 0 |
| formatter | `CRYPTOCOM-APIERROR-NATIVE-STRING code-5` | 101 | 0 |

Full venue executable floor 17 stopping `tests/ws_policy.rs`; no workspace compilation scope.

## Explicit non-runs

No workspace build/test/clippy, full monorepo suite, wasm runtime or dedicated wasm check, live exchange/API/websocket, CI rerun, canonical-index/exhaustive source proof, human review, PR creation or integration merge. Static contracts do not claim runtime binding completion. Clippy exits do not imply an unmeasured target or workspace gate passed.

## Delivery on the requested fresh integration base

New branch `feat/cryptocom-account-envelope-types-next` was created from `8cdc8f6eb4a04700bd2c7f3c706ecd9fdf1d3ebf`. Code and evidence were cherry-picked without rewriting history. Historical af5b measurements above retain their original SHA and parent; they are not relabeled as fresh-base executions. No unreviewed lane work is an ancestor beyond the explicitly recorded submitted slice and integrated commits.

Integration `039d308cd99cfd09e22205fb1083c3df5a306548` was merged down before the full refresh at `a543f1be8db2ebb954db041fc22563b46602a413`. Source/tests diff against af5b exited 0; Cargo.lock changed, so all seven scoped gates were remeasured. `fresh-base.json`, `fresh-gates.json` and `fresh-review.md` bind those facts to their actual SHA. Fresh tomllib census against requested 8cdc parent remains **8 pending→ported, 0 divergent**; 84 ported/18 divergent/35 pending, floor137 rows across6 manifests, stopping `cryptocom.ws-public.ts.toml::CryptocomPublicWebSocket.dispose`.

| Fresh gate at a543f1be8d | Exit | Raw log |
|---|---:|---|
| venue-tests | 0 | `fresh-venue-tests.log` |
| check | 0 | `fresh-check.log` |
| host-clippy | 101 | `fresh-host-clippy.log` |
| wasm-clippy | 101 | `fresh-wasm-clippy.log` |
| fmt | 0 | `fresh-fmt.log` |
| writer | 0 | `fresh-writer.log` |
| extractor | 0 | `fresh-extractor.log` |

Refreshed venue executable floor 17, stopping `tests/ws_policy.rs`. Both new native floors235+59 pass. Host and wasm clippy each fail in unchanged transport `tealstreet.rs:157,177` with arc_with_non_send_sync; neither reaches a venue lint verdict.

Later integration `6c27a13bdd9dc5645060283a5d74459405b7a0e2` merged down at `57e97bf244e7bda7c3748e679d479ee5452825db`. Crypto.com source/tests diff against a543f1be8d exited0. All recursive local source/Cargo.toml inputs and existing locked packages are unchanged; only a new unrelated safe-cex-lighter-sdk workspace member/lock package was added. `merge-survival.json` records parsed workspace/lock comparisons and exact source/test diff command. Venue tests, formatting and failing host/wasm clippy remain measured at a543f1be8d, with unchanged scoped inputs; those four commands were not rerun at57e97bf. The scoped check and writer/extractor were rerun at57e97bf:

| Merge gate at57e97bf | Exit | Raw log |
|---|---:|---|
| check | 0 | `merge-check.log` |
| writer | 0 | `merge-writer.log` |
| extractor | 0 | `merge-extractor.log` |

Both writer/extractor runs state0 updated manifests,0 added,0 orphaned. Merge-only movement0rows; all8 moved rows belong to the port. Eight compiling runtime controls remain measured at historical af5b (101→0, selected floor8 stopping formatter index7), not reexecuted on the fresh base; exact source/test survival is proved. Evidence-only delivery commit adds no production/test/manifest changes. No Bitrue source or manifests were touched by this slice. Explicit non-runs above continue to apply.
