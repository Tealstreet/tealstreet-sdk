# Historical revalidation count review

Reviewed pin: `4cc1de5013e31e1ff553b49fba6de99feadb65ae`.
Prior accepted implementation: `0753b7ed8dcc85ce972d72153a4a201ee9dcc061`.

Verified from the committed classification JSON at both pins: exactly eleven selected `classification=behaviour` rows already have `resolution=repaired` at the accepted implementation. The same eleven records are unchanged at the reviewed pin. Their result statuses are five ported and six divergent. Read-only TOML checks of only these eleven selected manifest identities confirm current status and Rust mapping match the ledger; the six divergent notes retain CHOSEN/MIXED rationales rather than newly open FINDING classifications.

The eleven are a historical repaired-row count from the original sixteen-row selection. They are not eleven current divergent defects: five are already ported, and the six remaining divergent rows retain separately declared interface/representation/runtime rationale after their selected behavior repair. Class and constructor also share a root cause, so eleven rows are not eleven independent root causes.

Consequently `11/106` would mix the historical repaired numerator with a current divergent-row denominator. It cannot measure the current divergence defect rate, current unresolved findings, or fresh repairs. The valid historical selection ratio is eleven repaired behavior rows out of the original sixteen selected rows, with the row/root-cause qualification. This review does not independently census the current106 divergent rows or assert that their other behaviors are free of defects.

Measured reviewer commands: initial python3 inspection exited1 before work because tomllib was unavailable; Python3.11 read-only JSON/TOML assertions over git-show blobs then exited0. Selected rows checked: indices0,2,3,4,5,7,11,12,13,14,15; stopped at15 subscribePublicTopics. No new venue census, Cargo, source execution, mutation controls, edits to tracked files, commits or pushes. Parent-reported controls at4cc are not reviewer-measured exits and are not recertified here.

# Pinned control-delta review

Reviewed SHA: `4cc1de5013e31e1ff553b49fba6de99feadb65ae`.
Comparison SHA: `ee0e0562034338c5d2504a2bdae03cd613c4f076`.

No blocker found in the inspected delta. This records the earlier read-only review; no new audit or gate execution was performed when saving this receipt.

## Finite scope and conclusions

Reviewed the six new control recipes, indices16 through21: constructor-message, encoded-action-callsite, wallet-primary-type, wallet-integral-f64, public-user-composition and public-dispose-composition. Static checks confirmed exactly one committed replacement anchor for each. The runner now requires all of exact exit101, no recognized compiler failure, a native `test result: FAILED` marker and the intended named assertion. Restored runs must exit0 and report one passing native test. This narrows control acceptance; it does not itself prove that any recipe compiled or failed at runtime.

The four added original-subdefect recipes are targeted by inspection:

- Constructor-message preserves argument conversion/use while replacing the constructed message with empty text. The existing independent arbitrary-message assertion should reject it.
- Encoded-action-callsite restores the old whole-Value serde decode at the encoded selector entry. The existing native terminal-cancellation emission assertion should reject loss of the ignored-surrogate action type.
- Wallet-primary-type honors the caller metadata in place of the inferred root. The existing wrong-primary-type accepted fixture should reject that regression.
- Wallet-integral-f64 bypasses integer lowering only for original f64-backed serde Numbers, retaining earlier canonical metadata. The existing uint-number-one accepted fixture should reject that regression.

These first-case/failure predictions were static review conclusions, not reviewer-executed negative controls. The signing test now includes the JSON case name in its native positive-result panic, making the two wallet assertions identifiable as `SIGN native source-accepted wallet payload "wrong-primary-type"` and `SIGN native source-accepted wallet payload "uint-number-one"`. Expected quotes match serde Value display. The old result comparisons and independent38/20/8 counts remain intact. Signing bytes before the first cfg(test) marker are unchanged; its seven-line delta only improves test failure diagnostics.

The public additions invoke installed subscribePublicTopics and prototype dispose through the fixture host. Independent topic argument fields/counts, owner state, send count/order and stop/super effect receivers are asserted before both source feeders. The original source feeder remains seven cases; the separately labeled composition supplement adds two cases with independent completion count and stop label. These are bounded installed-callable composition witnesses, not a full class/runtime or arbitrary-input proof.

## Checks actually performed in the earlier review

- Read-only git rev-parse/show/diff inspection of the pinned five-file delta: exit0.
- Read-only Python assertions over committed blobs: exit0. Verified unchanged signing production prefix and one exact anchor for each of the six new recipes.
- Inspected the supplied initial-control receipt: sixteen records bound to comparison SHAee0e, each recording compiled101/restored0 and intended assertion. These are parent measurements inspected as historical evidence, not reviewer-measured command exits or gates at4cc.

No Cargo check/test, source oracle execution, native/source mutation, restoration control, clippy, build, extractor or formatting command was run by this reviewer. No production/test edits, commits, pushes or new census were performed. No exits are inherited from another SHA; the parent owns later control execution and its receipts. The eleven historical repaired rows remain eleven already-repaired rows, with no new ported credit claimed by this test/control delta.

# Public composition proof follow-up

Exact review pin:4cc1de5013e31e1ff553b49fba6de99feadb65ae. Read-only committed git-show/diff review; no Cargo/native execution, edits or mutations. Pinned source supplement independently executed0 with2cases stopping installed-dispose-dynamic-receiver.

The two previously predicted proof gaps were actually reproduced by parent at ee0e0562034338c5d2504a2bdae03cd613c4f076: both missing-user and missing-dispose controls exited0/restored0, recorded in hyper-public-proof-probes.json read by this reviewer. These remain historical SURVIVOR findings; this review does not turn them into production defects or silently discard them.

The strengthened helper runs first inside the existing native public target, before existing serde/source checks. It constructs owner/other, populates owner topics before invoking the actual installed fixed arrow with other as calltime receiver. It independently expects3actual SendSubscription argument handles and directly checks their native type/coin/interval fields; independently expects5sends, before_send_sizes[0,0,1,2,3], ownerSet3/otherSet0. Captures occur in native function invocation before forwarding into the Rust send body, retaining original object handles rather than decoded/reencoded JSON.

It then gets the actual prototype dispose function and invokes it with other. Native Host observes actual StopPingLoop arrow invocation and original captured receiver, followed by actual super.dispose call with dynamic receiver; test independently requires2events in order and both receiver handles equalother/differentowner, plus nativeundefined return. This proves the previously omitted dispatch edges within the stated primitive fixture domain. It does not prove every ping/cleanup/proxy/error schedule; StopPingLoop body here has no active timeout.

Controls added at indices20/21 use exactunique source anchors and should discriminate the newfirsthelper counts. The omission sites retain import/mutableborrow uses, avoiding deniedwarnings compilation rejection. Driver now requires101, no compiler-error marker, actual test-resultFAILED and namedassertion before caught; restoration still requires0 plus1test. No new control execution is claimed by reviewer. Parent runner was active separately.

Core/public/type/helper production source is byte-unchanged ee0e..4cc1 (scoped git diff exit0). The src/signing.rs delta is inside findings_regression test code and improves assertion case identification; no production signing behaviour change is established by this proof commit. Original7case source feeder remains unchanged; new2case supplement is explicitly later proof, not retroactive independence.

Remaining small documentation/proof qualification: depth/BBO interval absence currently uses h.get()==N::U, which also accepts present-own interval:undefined. Assert exact native N::O.fields ordered keys against independent expected [type,coin] or[type,coin,interval] before wire, or rename the assertion to undefined/missing. Source supplement can similarly wrap actual sendSubscription and assert Object.keys on the3original payload arguments if exact source shape is claimed. This is not an executed survivor or production mismatch. The focused two new edge controls are otherwise structurally discriminating.

Measured here: pinned source supplement0 (casecount2, namedstop); committed diffs/inspection0. NOT RUN: Cargo/native target, newmutationcontrols, all16/22controls, workspace/check/Clippy/wasm/extractor/fmt or wholevenue census. Parent baseline/native/control exits remain parent evidence. Review covers only the new public composition assertions/host/driver delta.

# Native subscription own-key follow-up

Exact review pin:e5e8a9801c744cf218f85b0efe64806727bfd6c9. Read-only committed git-show delta from4cc1de5013e31e1ff553b49fba6de99feadb65ae. No Cargo/native execution, edits or mutations; parent controls20..22 were active separately.

The prior qualification is addressed within the finite3subscription floor. Native test first inspects original captured N::O own fields, compares exact ordered key vectors to independently stated [type,coin,interval] for candle and[type,coin] for depth/BBO, then asserts every corresponding descriptor enumerable. The exact nonempty key equality precedes all value accesses, serde/wire assertions and source receipts, so enumerability cannot pass vacuously on an empty object. Present-own interval:undefined now differs from absence despite equal get()==undefined and serialized text.

Actual-source supplement wraps installed sendSubscription, retaining the3original argument handles while forwarding to its original lexical arrow. It independently asserts argumentcount3, Reflect.ownKeys exact ordered strings and native descriptor enumerability before JSON payload comparisons. Wrapper preserves original owner capture. The source floor remains2cases ending installed-dispose-dynamic-receiver; the original7case feeder remains unchanged.

Recipe22 adds only an enumerable own interval:undefined to noncandle producer payloads through a unique source anchor. It should now fail PUBLIC composed native own subscription keys1 before wire; strictdriver requires compiled/runtime101+namedassertion and restored0. This reviewer did not run the control. Historical receipt read here records the actual same ownundefined control SURVIVOR0/restored0 at4cc1; that finding remains retained rather than discarded. This is test/proof strengthening, not a production behaviour repair or additional status move.

Measured reviewer execution: pinned source oracle0 at this exact SHA,2cases with namedstop. Committed delta inspection0; changes are confined to the native test, source supplement and control recipe. No additional blocker identified in this narrow qualification. No generic descriptor/prototype/runtime completeness or broader venue audit claim.

NOT RUN by reviewer: Cargo/native baseline,20..22controls/restores, broader controls, tests/check/build/Clippy/wasm/workspace/extractor/fmt or census. Parent-reported native baseline and control results are not reviewer-measured exits.


# Three historical public-proof survivors: current status review

Review pin:df8f7434dffd005ca500d9a4c72ea10821b11d72. Read-only committed blobs during parent mutation run. No Cargo, tests, mutations, source edits or broad audit by reviewer.

Conclusion: zero unresolved cases among these exact three historical survivors. They are retained history, not three still-open production defects. FINDINGS_11_REVALIDATION.json retained_survivor_findings links each to a later caught recipe in mutation_floor batch3 at e5e8a9801c744cf218f85b0efe64806727bfd6c9. Each matching record reports compilation, runtime test failure101, intended native assertion, exact restoration0 and one passing native test. Current df8 repeats were still running when reviewed; no fresh df8 test exit is claimed here.

1. public-missing-user-edge survived0/restored0 at ee0e0562034338c5d2504a2bdae03cd613c4f076 because fixed subscription arrow was only called with empty topics; nonempty topics were tested through direct user arrow. New first native helper invokes installed fixed arrow with populated topics, independently requires3original native subscription arguments, exact fields/keys,5sends and Set mutation order. Matching recipe20 public-user-composition now catches at PUBLIC candle plus depth plus BBO native send floor: installed fixed-to-user edge, recorded101/restored0.

2. public-missing-dispose-edge survived0/restored0 at ee0e because source oracle exercised dispose but native target only inspected descriptor. New native helper calls installed prototype dispose with the second object as dynamic receiver and requires2actual effects, stopPingLoop then super.dispose, with original receiver handles and nativevoid return. Matching recipe21 public-dispose-composition catches at PUBLIC installed dispose reaches stop then super, recorded101/restored0.

3. public-interval-own-undefined survived0/restored0 at4cc1de5013e31e1ff553b49fba6de99feadb65ae because get(interval)==undefined could not distinguish absence from ownundefined, and JSON dropped the added field. New native ordered-own-key/enumerability assertions inspect original captured objects before values/wire; source supplement uses Reflect.ownKeys/descriptors. Matching recipe22 public-interval-own-undefined catches at PUBLIC composed native own subscription keys1, recorded101/restored0.

All three are documentation/proof repairs: production already contained correct edges/shapes. Historical SURVIVOR disposition must remain visible, with an explicit separate current status such as closed-by-subsequent-control and closure/control SHA to avoid confusion. No new production repair or manifest status movement follows merely from this clarification.

Reviewer verified all3historical links resolve to exactly recipes20/21/22 in batch3, each complete caught/restored record, and all3current producer blobs match recorded controlbase SHA256s. Relevant native test/host/source supplement/production and driver are byte-unchanged e5→df8. This is code-preservation evidence, not transfer of old exits across all current shared inputs.

Claim bound: zero unresolved among the three enumerated cases, ending recipe22 ownundefined. Not zero possible survivors across all variants, all class bodies, every FINDING or the venue. The parent fresh3recipe run can provide current-SHA exits once complete; a running gate is not passed.

Measured here: git preservation checks and Python receipt-link/blob verification exit0. NOT RUN: Cargo/native/source runtime, new controls/restores, builds/check/Clippy/wasm/extractor/fmt or census. No extra audit findings in this narrow follow-up.
