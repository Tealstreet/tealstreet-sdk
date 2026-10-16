# Selected sixteen findings: pinned sub-agent reviews

These are read-only sub-agent review receipts. Their pins and scopes remain distinct from the final delivery gates. External probe filenames below are normalized identifiers for private review artifacts, not repository paths. Known review blockers were fixed forward; none of these reviews establishes complete-function parity.


---

# Final read-only class/findings review

Exact review pin:a1b1a9846b32b863eb096b99a949b6972eb36b03. All reads used committed git-show blobs because the shared worktree was under parent mutation controls. No Cargo, native/source executions, mutations, production edits or pushes. No parent gate exits are attributed to this review.

## Prior behaviour defects repaired

Compared core_exchange_leaves.rs and core_exchange_class.rs to494cf3b6fa8230b65fa3b226de31c1d1a4ec8b4b. Finite-positive callback now uses finite_positive_predicate with explicit module-singleton identity rather than fresh arrow allocation. finite_positive returns original falsy Number.isFinite handle, and shouldUsePositionTpsl consumes truthiness at its source boundary. finding_core_class.rs directly asserts two native find argument handles are identical and calls that installed predicate with overridden Number.isFinite yielding negative-zero, asserting original native f64 bits before launching source. The source oracle independently uses the actual module helper and checks its retained identity/negative-zero return. No remaining instance of the two previously reported production mismatches was identified.

## Documentation/proof finding at this pin

finding_public_class.rs validates user candle/depth/BBO coin/interval fields only after parsing h.sent native String arguments into serde wire Values. Pre-wire checks already cover send/Set counts, mutation order, original error identity and exact fixed-topic String arguments, but not complete user-topic sender String arguments. Add direct N::S/JsString equality against literal complete expected strings for sent[4],sent[5],sent[6],sent[7] before the serde projection, or narrow the row note's native-subscription assertion claim. This is a documentation/proof gap, not a newly identified production mismatch; no survivor was executed by this reviewer.

Other finite limits: core metadata checks do not assert every captured body enum or execute all94 entries; only selected installed state/async entries run. Ledger and class notes explicitly distinguish census from body proof. Native return-adoption witness counts actual adapter calls and checks original native handles; same-realm source oracle independently measures then getter reads. This does not implement a generic native Promise/thenable runtime, which remains an explicit binding. No fullfunction, eventloop or live-network proof is inferred.

## Manifest and classification checks

Independent Python3.11 tomllib over committed Hyperliquid manifests confirms420ported/111divergent at selectionbaseaa5a1bebfc66525719957e4e66824180d893accf and425ported/106divergent here. Exactly five statuses change D->P: HyperliquidExchange, its constructor, WsUserEvent, HyperliquidPublicWebSocket and subscribePublicTopics. Zero pending rows move. FINDINGS_16_CLASSIFICATION.json has exactly indices0..15 and11behaviour/3explanation/2already-known; ratio is rows, not distinct causes. The assigned later half remains5behaviour/1explanation/2already-known. Address diagnostic choice is explicitly portable Result text with priced lost ethers Error metadata; actionHash/signL1Action identify propagation of that same cause. No impossible-representation claim or successful-signature diagnostic proof is substituted.

Rows8..15 notes retain finite signing/parser/intersection/class bounds and do not claim all EIP712/parser/runtime inputs. Three legacy typed subscription rows still contain embedded FINDING text while additionally mapping the new raw native routes; ledger explicitly excludes earlier mixed notes from this16-row closure. Therefore this report does not claim all keyword FINDING occurrences were closed. A future note cleanup could explicitly prefix those old limitations with legacy typed path to improve attribution.

## C3 and class implementation scope

Twenty-three new exact C3 class/leaf exemption symbols have no duplicate; they cover aliases, discriminator/capture, host traits and individual native allocation/property/lexical-scope/settled-observation capabilities. Actual class creation, constructor, installed dispatcher, predicate and state/selector algorithms remain manifest-mapped rather than exempted. The shared reason is a generic category list, but its exact symbols do not hide a business algorithm or whole-module exclusion.

At prior494cf review, source AST census and committed source confirmed121fields/12statics in exact order,94 installed names/lengths and94 match arms (constructor separate). Subsequent production delta changes only finite predicate handling and historical-position adoption, plus documentation of settled observation. No new mismatch identified in source order/defaults/receiver/class scope/factory composition or public17-arrow/dispose/four-subscription routing. Existing body algorithms were not broadly re-audited. Source member census ends exchange index152 placeOrders/public index27 dispose; selected classification ends index15 subscribePublicTopics.

Historical-position bridge now uses start_adopted because mapped fetch_historical_positions owns final return_await_value; observe_exchange_task explicitly exposes settled outcome without a second Resolve. This is consistent with the mapped return-adoption contract and the supplied source counterexample. Other async paths were not proven by this one observation.

## Commands/evidence

Read-only git rev-parse/show/diff; selected committed Rust/native tests/source oracles/manifests/exemptions/ledger; Python3.11 tomllib census, status-diff, exact-index/category and exemption checks, all final checks exit0. No extractor/fmt/build/check/clippy/wasm/workspace or source runtime gate was run. Scope is this exact-pin static adversarial review, with the one native-before-wire proof qualification above.


---

# Follow-up review: native user-topic sender assertions

Exact review pin:8de628d8f6cf6be01e8df36e25974feece300f0a. Read-only git-show/diff review, no Cargo/runtime executions, mutations or production edits.

The finding_public_class.rs delta adds four independent literal expected strings for native sent[4..7]: candle BTC/1m, l2Book ETH, bbo ETH and l2Book ETH unsubscribe. Each assertion compares the original sender argument against a native N::S(JsString) expected value. public_class_host::same forwards to private_class_host::same/cache_host::eq; its String branch compares JsString code units directly. s(expected) encodes the independent literal into native UTF16; it does not re-encode an observed payload. All four assertions precede construction of serde wire payloads and source receipt reads.

This resolves the native-before-wire evidence request from a1b1a9846b32b863eb096b99a949b6972eb36b03. No additional production issue was identified in this delta. Scoped git diff for core_exchange_class.rs/core_exchange_leaves.rs/public_class.rs/ws_emission.rs is empty from the previously reviewed a1 pin. Signing changes are outside this follow-up scope.

No gate exits are claimed. Existing finite class/body/host-runtime bounds from the prior review remain; this confirms only the stated test qualification correction.


---

# Final Hyperliquid selected-findings review

Exact reviewed commit: `a1b1a9846b32b863eb096b99a949b6972eb36b03`.

Read-only committed-blob review while the working tree was being mutated by another agent's serialized controls. This reviewer did not execute Cargo, native Rust tests, mutation controls, clippy, extractor, formatting, workspace/wasm builds, or CI. No production edits. Scope: original selected rows 0–7, current signing repairs, classification/manifest consistency, and the specific historical-position async bridge. This is not complete-function proof or a new audit of all divergent rows.

## Verdict

**One remaining behaviour closure blocker at this pin; one representation-witness gap.** Do not treat the classification census or the successful source oracles as proof that every repair is complete. The parent has acknowledged the inherited-bool blocker and plans a forward repair; that later repair is not covered by this SHA.

## Findings

### P2 — behaviour: inherited bool members still become false

`src/signing.rs:673–676`, in `normalize_wallet_field`, inserts false whenever a declared bool field is absent from the JSON object's own members. Maintained ethers property lookup sees the stock inherited member for `toString` and `constructor`, so an ordinary empty JSON message object supplies a truthy function for either field. Native false projection therefore changes the signature. The input is ordinary JSON; this is not a custom-prototype, executable-getter, or function-input exclusion.

Pinned actual-source observation: two cases, last index 1 `constructor`, exit 0. For each field name, the empty-object signature equals the signature for an explicit own true field and differs from the explicit own false field. Actual maintained `signInner` and `splitSig` plus installed ethers 6.17.0 executed. No Rust counterexample or mutation was executed by this reviewer; the native false assignment is established by committed source inspection.

Portable source probe and pin/hash receipt:

- `external-review-artifacts/inherited-bool/inherited-bool.cjs`
- `external-review-artifacts/inherited-bool/receipt.json`

Repair can reuse the existing public `safe_cex_num::{js_table_lookup, TableHit, ProtoMember}` API. `js_table_lookup::<&str, serde_json::Value>(&[], name).is_truthy()` identifies all 12 stock Object.prototype members, including the truthy prototype object returned by `__proto__`. Preserve the current own-member-first branch, so own false/null values continue to shadow inherited members. There is no `TableHit::prototype_field_name` API in the inspected helper. Mutated/custom/null prototype semantics remain outside the stock table contract.

### P2 — explanation/witness: returned action-type surrogate clause lacks its direct observation

The selected `getRequestActionType` row now correctly distinguishes repaired ignored-field parsing from remaining return-string/depth restrictions. However, the MIXED note's returned-lone-unit clause cites §19 without naming an exact observed returned unit. Existing raw-action native cases put D800 in an ignored `unused` field; the 11-case parser source probe likewise tests ignored fields and the rate parser's whole-object result, not a returned action type containing D800.

Add a direct source observation for raw JSON with `action.type` equal to escaped D800: returned string length 1 and code unit D800. Assert the native `Option<String>` outcome before the source comparison and explicitly label that FORCED component in the MIXED note. This is completion of the already-declared representation rationale, not a new behaviour repair request.

## Repairs and previous findings checked

- Unreadable-error construction has a real constructor target, exact name/message, empty-message Display behavior, and parser reuse. Native four-message assertions precede its source construction witness.
- Limiter defaulted entry preserves omission versus explicit zero/NaN. Six independent native library cases remain distinct from the ten-case source constructor target. Current notes explicitly separate these targets, avoiding the earlier implication that the integration target alone establishes ten native cases.
- Ignored UTF-16 request fields now pass through the raw JSON decoder; the actual encoded cancellation-response path uses the repaired helper. The explicit 128-value-node limit remains a separately priced choice.
- Wallet key validation now occurs before vault hashing at the signing interceptor boundary. This reviewer inspected, but did not execute, the competing-error native test.
- Primary-type inference now follows the type graph rather than trusting `data.primaryType`, which TS ignores. JSON array/scalar struct projection was added. The current 21-case signing corpus includes the previously missing wrong/missing-primary-type and array-struct cases. The inherited-bool case above remains outside that corpus.
- Native signature r/s/recovery checks precede the source oracle; accepted/rejected floors are independently fixed. Compact/full string signature handling and error-contract divergence are explicitly bounded.
- The historical-position arm alone now calls `helper_tasks::start_adopted`. The retained helper stores the already-adopted result without another `settle_return`; ordinary `start` still assimilates raw returns. Native fixture assertions independently require return-adoption count 1 and outer-adoption count 0 before source invocation. The observer contract forbids executing/adopting the settled result again.
- The tracked historical source probe keeps the actual initializer and instrumented awaits in one realm. Direct return reads `then` once and preserves the original value; the additional async wrapper reads twice and rejects with the original second-getter marker. It is a source composition observation, not a Rust mutation result or complete async scheduling proof. The initial cross-realm reviewer failure remains an instrumentation failure.
- Core class native construction/state/callable evidence is finite. Source metadata counts do not prove execution of every one of the 94 installed bodies. No complete class/runtime/VM equivalence approval is given here.

## Classification and canonical notes

Read-only TOML/JSON audit exited 0:

- Exactly 16 selected identities, indices 0 through 15, match the original selection and their current manifest mappings/statuses.
- Split is exactly 11 behaviour, 3 explanation, 2 already-known; rationale total 5. These are source-symbol counts, not distinct root causes or an estimate for all divergent rows.
- Five selected statuses moved to ported; scoped census is 425 ported / 106 divergent / zero pending. Leading FINDING count is zero, which does not establish absence of findings inside mixed notes or the newly reported signing counterexample.
- Remaining divergent selected notes open with canonical buckets, and every cited anchor exists.
- The HTTP diagnostic row now identifies source D83D, the actual HealthMonitor getter observations, required native String/str surfaces, and the native U+FFFD outcome. It makes no human-only diagnostic claim and remains divergent under §19.
- The rate parser note names the whole parsed-object loss, original borrowed-string fallback and weight 20 versus 60, plus the separate depth choice.
- Crypto diagnostic notes state a local reason for retaining backend-independent Result strings and price lost ethers metadata. They do not invoke the unrelated diagnostic-config declaration as blanket permission.
- Canonical-reference lists are navigation, not evidence that every listed representation loss applies to every repaired native path. Local notes identify concrete causes rather than copying the declarations.

## Measured source-only checks at the reviewed pin

All seven scripts were compiled in memory from the pinned commit, with relevant source/fixture reads supplied from committed blobs and writes blocked. Every script exited 0:

| Script | Floor and stop |
| --- | --- |
| `finding-constructors-source.cjs` | 10, last index 9 |
| `finding-signing-source.cjs` | 21 signing / 20 split / 8 keys; final key index 7 `order-minus-one` |
| `finding-parser-source.cjs` | 11, `unreadable arbitrary text/name` |
| `finding-http-health-source.cjs` | 1, two getter observations retaining D83D |
| `finding-crypto-diagnostics-source.cjs` | 9, `unprefixed signL1Action error` |
| `finding-history-adoption-source.cjs` | 1, last index 0; direct 1 versus additional-wrapper 2 then reads |
| `finding-core-class-source.cjs` | 2 constructions, 121 fields, 12 statics, 94 installed callables; 6 state cases |

The additional inherited-bool source probe also exited 0 for its two independently expected cases. Source execution does not substitute for native gates. Parent-run controls and gates remain unmeasured by this reviewer and must retain their own actual-SHA receipts.


---

# Read-only follow-up review

Reviewed pin: `8de628d8f6cf6be01e8df36e25974feece300f0a`.
Previous review: `a1b1a9846b32b863eb096b99a949b6972eb36b03`.

Scope: committed venue signing projection, parser/native witnesses, actual maintained `hyperliquid.signing.ts` initializers, relevant manifest notes, and shared stock lookup/coercion contracts. Working production files were not used or changed. This is not approval of the complete sixteen-row closure or arbitrary EIP-712 accepted-set equivalence.

## Prior issues

The direct inherited-bool issue is repaired for the six newly exercised cases: ordinary object `toString`/`constructor`, array `map`, number `toFixed`, string `charAt`, and own `toString:false`. Own-field lookup precedes inherited fallback. Stock array and object lookup helpers are reused; scalar-specific stock methods are explicitly enumerated. Native source-derived signature expectations remain before source execution within the signing regression test. I did not execute that Rust test.

The returned action-type witness now observes the actual source result as a one-unit string with charCodeAt(0) D800. Its literal-backslash ASCII control is distinct. Native assertions independently require None for the materialized unit and Some(literal backslash text) for the ASCII control. This supports the stated required Option<String> representation conflict; it does not imply the native return equals source or that every parser domain was examined.

## Remaining findings

**P2 behaviour: inherited nested struct field is still omitted.** `sdk-rs/safe-cex-venue-hyperliquid/src/signing.rs:740` recursively projects present own members, but line742 only resolves a missing inherited member when its declared kind is bool. With valid JSON input `message:{}` and types `Outer:[{name:"toString",type:"Inner"}], Inner:[{name:"toString",type:"bool"}]`, actual maintained signInner accepts the value: outer `toString` resolves the stock function, whose inherited `toString` is truthy. Source signature equals explicit `message:{toString:{toString:true}}` and differs from explicit false. Native projection leaves outer `toString` absent. The pinned shared alloy-dyn-abi1.7.3 struct coercion uses own map.get and rejects missing properties. This is a static native mismatch plus an executed actual-source positive, not an executed Rust failure or mutation survivor. Input contains only ordinary JSON; the function arises during source property lookup rather than being supplied as executable input. Resolve this path or retain the concrete behaviour finding; the raw-key/error ownership rationale does not account for it.

**Documentation: stale floor and stopping labels.** `sdk-rs/parity/manifest/exchanges/hyperliquid/hyperliquid.signing.ts.toml:83` still says21 signing cases ending array-struct-message; committed source/native fixture now has27 ending object-toString-own-false. `tests/finding-parser-source.cjs:73` and `tests/weight_ingress.rs:342` still label stop as unreadable arbitrary text/name, although two later action-type witnesses were added. Actual final parser witness is the literal ASCII returned backslash sequence, iteration12 of13. These do not establish production defects, but the reported stopping point is inaccurate.

## Measured checks

- Pinned in-memory execution of `tests/finding-signing-source.cjs`: exit0; signing27 (last object-toString-own-false), split20, keys8 (overall last order-minus-one). Fixed expected successful counts23/14/3 and rejection sets passed. Source files and fixture reads were redirected to committed blobs.
- Pinned in-memory execution of `tests/finding-parser-source.cjs`: exit0; independently completed13. Actual last iteration12 is literal ASCII action.type. Its printed stop is stale as recorded above.
- `node external-review-artifacts/nested-inherited-struct/nested-inherited-struct.cjs`: exit0; independent floor1, stop index0; source nested inherited value equals explicit true and differs from false. Portable source paths, exact reviewed commit and real installed ethers; no native emulation substituted for this source observation. Receipt is adjacent `receipt.json`.
- Read-only git show/diff inspections and shared helper/backend source inspections completed. Two initially guessed manifest paths did not exist; corrected repository paths were then inspected. No conclusion relies on those failed reads.

Not run: Cargo check/test, Rust native probe of the newly identified nested case, clippy, wasm, workspace build, extractor, formatting, Rust/source mutations or restoration controls. Parent-reported exits are not reviewer-measured gates and are not transferred here. No production/source edits, commits or pushes. No exhaustive prototypes/coercions/accepted-set proof.


---

# Targeted WalletView closure review

Reviewed committed pin: `62a0aeb3ba6dd06d6ef4130d2d2fc8f6af344ba0`.
Comparison: `8de628d8f6cf6be01e8df36e25974feece300f0a`.

No remaining blocker found in the targeted repair paths. This does not attest complete EIP-712 accepted-set equivalence or the final sixteen-row registration/gates.

The known ordinary-JSON nested inherited-member counterexample is resolved by inspection: Object `toString` is retained as a stock callable reference; declaring its type as Inner causes recursive property access; the inner `toString` resolves through Function.prototype and is consumed by bool truthiness. It is no longer dropped because it lacks an own JSON property. Own-field precedence remains first. Undefined/null member access remains fallible; restricted callable caller/arguments access rejects instead of silently becoming a false bool. Array/string/number metadata remains available until the declared field consumes it.

The regression fixture adds the exact nested object case, constructor/prototype bool, array-method length, string-method name, object-method integer length, and restricted caller rejection. The first five exercise accepted signatures; caller exercises rejection. Native expectations compare r and s strings plus v f64 bits, without re-encoding a native result. All38 signing,20 split and8 key native iterations/assertions finish before spawning the source oracle. Counts are explicit independent constants rather than fixture lengths. Five Number/integer cases explicitly replace fixture values with f64-backed serde Numbers and assert their literal expected bits before signing; integer projection preserves safe integral Numbers and rejects the tested fraction/unsafe value. Metadata zero is canonicalized into an integral string for the shared encoder.

The source oracle was executed using committed script/source/fixture blobs through an in-memory read-only Module loader. Measured exit0: signing38 (last signing iteration37 uint-number-overflow), split20 (last split iteration19 low-high-s), keys8 (last key iteration7 order-minus-one; overall output marker). The independent source accepted-set assertions31 signing successes,14 split successes and3 accepted keys passed. Known nested-object-inherited is signing iteration27. This run validates maintained-source behavior and committed fixtures, not the Rust implementation at runtime.

Read-only scope included the WalletView/property/projection implementation, stock PrototypeRealm and array lookup contracts, native regression body, source oracle and fixture/control-script deltas. No new concrete behavior regression identified within these inspected paths. Existing CHOSEN error/JSON/runtime boundaries remain separate from implementation coverage; arbitrary prototypes, executable input hooks, all native method metadata, all typed-data accepted sets and complete coercion domains were not audited. Stale manifest floors are outside this targeted code delta and are being reconciled separately by the parent.

Reviewer checks: pinned source oracle exit0; git rev-parse/show/diff inspection. One combined search ended exit1 because the requested exact function-name pattern was absent; the actual regression module was subsequently located and read. No Cargo check/test, native Rust probe, clippy, wasm/workspace build, extractor, fmt, mutations, restoration controls, edits to tracked files, commits or pushes. Parent-reported gates are not reviewer-measured and are not transferred to this pin.

