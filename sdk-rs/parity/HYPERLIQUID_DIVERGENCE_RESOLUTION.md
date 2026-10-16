# Hyperliquid divergence resolution

Frozen census: e6a8ffaf1384d18c00fb2b0e16ca3fed2ec20667. Fresh branch
fix/rust-hyperliquid-divergence-audit follows the accepted socket-options unit.
Fetching origin/feat/safe-cex-rust resolves to53ee502232b005173d8a6f3f0d7cf94bad7f072c,
already an ancestor; merge reports already up to date. The overseer's newer
aab488a8e2 batch is still gating and is not represented as fetched here.
No unrelated unreviewed feature ancestor is introduced. No push.

Python tomllib measures all29 preserved manifests in exchanges/hyperliquid,
including spot:425ported/106divergent/0pending,531rows. Enumeration stops at
hyperliquidspot.ws-public.ts.toml. Nine files contain the106divergent rows:

| Source family | Divergent |
| --- | ---: |
| API |8|
| Exchange |12|
| Order history |5|
| Order inference |2|
| Order response |2|
| Rate limiter |9|
| Signing |12|
| Types |53|
| Public socket |3|
| Total |106|

The frozen prefixes are93CHOSEN/12MIXED/1FORCED. Six rows contain an embedded
FINDING even though none starts with it: addRateLimitInterceptor,
getHyperliquidRestRequestWeight, RateLimiter.waitForToken, sendSubscription,
sendUnsubscription and subscribeUserTopics. Those are reviewed before the
remaining notes. Census, completed review and remaining defects are separate
measurements; this opening snapshot makes no completed-audit claim.

## Recovered historical selection

FINDINGS_16_CLASSIFICATION.json selects indices0..15 at
aa5a1bebfc66525719957e4e66824180d893accf and records an integration merge of
7bac54167ca1344621ff98cfc492c2838926bffb. Independently counting its16row records
gives11behaviour/3explanation/2already-known:11behaviour versus5rationale-or-known.
These are selected source symbols, not eleven distinct causes or current defects.
The class and constructor share one cause. Five rows moved to ported in that
earlier repair; its425/106/0after-census agrees with today's independent count.
Those repaired historical rows are not counted again as new behaviour findings.

## Rules and scope

Classification cites MANIFEST-SPEC.md#divergence-taxonomy. Representation causes
cite #shared-utf8-string-boundary, #closed-typed-output-boundary,
#owned-snapshot-vs-live-alias, #numeric-ingress-defect and
#forced-surrogate-conflict. Declarations remain at their canonical anchors.
Maintained TypeScript and native bodies/callers determine each verdict; checker
diagnostics and row counts do not prove semantics. Reviews are read-only
sub-agent reviews; production repairs and manifest reconciliation remain in this
unit. Native assertions precede wire comparisons, and controls retain their
actual failure, restoration, SHA and bounded iteration floor.

The local required gates are cargo fmt --check, locked workspace build and
workspace all-target host clippy. Canonical wasm and central wiring/layers/routing
remain overseer gates. No full suite or live-venue execution is implied.

## Completed review

106/106 divergent rows reviewed,0remaining. The finite pass ends at iteration106,
HyperliquidPublicWebSocket.subscribeUserTopics. Every frozen identity remains in
its original manifest. Each row has one primary audit classification; an affected
row can also retain independently priced representation/ownership costs.

| Source family | Supported | Explanation | Scheduling | Behaviour | Documentation | Already-known | Total |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| API |1|0|0|4|3|0|8|
| Exchange |12|0|0|0|0|0|12|
| Order history |3|0|0|0|2|0|5|
| Order inference |1|1|0|0|0|0|2|
| Order response |2|0|0|0|0|0|2|
| Rate limiter |6|0|0|1|0|2|9|
| Signing |7|1|0|3|1|0|12|
| Types |2|0|0|50|1|0|53|
| Public socket |0|0|0|0|3|0|3|
| Total |34|2|0|58|10|2|106|

That is34supported and72primary findings, including two previously recorded
findings. There are60rows with code work:58behaviour rows plus those two.
Fifty of the58 share ONE discarded-duplicate capture mechanism. These are row
counts, not58independent defects or a claim about58production venue incidents.
The remaining12findings are explanation/documentation corrections. Status moves0;
the425/106/0 census is unchanged. Code work does not erase residual priced costs.

## Eleven repair mechanisms

These are historical causes repaired in the accepted audit, not an outstanding
defect backlog. The follow-up sections separately measure regression sensitivity;
remaining priced divergences do not imply that these repaired causes are open.

| Mechanism | Affected source rows / bounded evidence |
| --- | --- |
| Last raw duplicate selected before value decoding |50 public typed roots, from Meta through BuilderFeeCheckResult. Each has an explicit native decoder and direct member/variant assertion. Earlier discarded D800 no longer refuses the valid surviving record. Malformed discarded syntax, surviving D800 and surviving depth140 still fail. First key position stays fixed. |
| Reserved-looking JSON object capture |The raw MessagePack adapter, prepared signing wire canonicalizer, parseRequestData and addResponseInterceptor model reads retain the literal private-Number-looking object. Native object/member assertions precede weight/error observations. The separately observable Axios transform is unchanged. |
| Fallible action.type conversion |addSigningInterceptor propagates native TypeError before unknown-action emission or wallet validation. Ordinary unknown actions still emit exactly once. This preparation test does not prove an adapter execute route. |
| Split diagnostic refusal |addResponseInterceptor refuses the capped String before emitting substituted text. Actual source Error/emitter/HealthMonitor getters retain D83D; this remains a FORCED application under #forced-surrogate-conflict. Complete scalar prefixes still emit normally. |
| Raw weight field projection and physical parse boundary |getHyperliquidRestRequestWeight and addRateLimitInterceptor retain userRole60 with unused D800/depth140, Number poison order and selected array length. Physical JSON string literals stay strings after one parse and reserve20. Later native requests expose the actual reservation. |
| Typed weight error propagation |addRateLimitInterceptor now retains the modeled TypeError name and unprefixed message through prepare_request and actual execute, before limiter or adapter invocation; capture bypass still reaches the adapter. The existing JavaScript error variant can carry these fields, so their loss was not a supported native-ABI choice. |
| Omitted weight default |RateLimiter.waitForToken accepts None as omitted and reserves one token; its next wait observes50ms. Lazy/cancellation/u64-timer costs remain priced. |
| Admitted asset number retention |orderRequestToOrderWire retains NaN/infinities/-0 natively through vectors and all three grouping helpers before authenticated encoding and JSON projection. The witness does not claim that production market indices are nonfinite. |
| Generic array wallet projection |signUserSignedAction uses a request-local metadata overlay over its array, preserving index/length/inherited-constructor reads for signing. Caller Value::Array still cannot expose the two named post-call properties. |
| Source MessagePack depth |actionHash now refuses depth101 with the source error; depth100 remains accepted. This is a bounded encoder witness, not a general very-deep Value allocation/stack proof. |
| Current telemetry data phase |getOrderErrorTelemetry passes encoded UTF-8 text as String instead of parsing it before the provider. The actual source response interceptor/provider chain preserves the original object/reference or string input; native callback assertions precede source observation. |

The actual hash path now uses the native-value MessagePack entrypoint. Its raw JSON
entrypoint remains a public codec adapter; the actionHash mapping names both roles.
The weight mapping also names the four raw field/coercion helpers that now carry
its raw-input branch. These are two mapping expansions, with no status moves.
The shared capture and response-model changes affect other consumers, but this
unit executes the selected Hyperliquid routes and makes no all-venue inheritance
or production-host wiring claim.

## Rationale and documentation

The two explanation rows are orderTypeToWire's live limit alias and
inferHyperliquidOrderTypeAndPrice's decoded-record/closed-result boundary.
They cite #owned-snapshot-vs-live-alias and the appropriate representation anchors.
The50typed rows also price their independent capture budget at the venue-owned
CLAUDE anchor; the budget limits nesting depth, not total member count.

The ten documentation rows are getRequestActionType, createExchangeAPI, createAPI,
collapseHyperliquidOrderHistory, mapHyperliquidHistoricalOrder, getTimestampMs,
OidOrCloid and the three legacy public-subscription mappings. The extracted
private action-type observation did not prove a retained program consumer, so
its prior FORCED claim is withdrawn. Actual returned historical IDs and selected
public typed fields have explicit returned-unit observations and whole-input
refusal costs. No all-field or concrete network-consumer proof is inferred.
The timestamp getter is synchronous; copied Promise scheduling prose was removed.

The three public-subscription binding claims are stale: the constructed raw class
installs the native arrows and dispatches sendWithDiagnostics over the live
receiver, topics and Set. The separate prepared-value adapter stays divergent.
The factory's core params-serializer seam exists; concrete regional/proxy/qs-repeat
venue-host assembly remains unproved. Its caller obligations and why the factory
does not install/validate them are priced at the venue API contract anchor.

## Evidence boundaries

Direct native assertions cover50duplicate public roots, the scalar string union,
two any-indexed object/array models, eight historical cases with96field checks,
and the selected request/error/pacing/signing effects. Signing's source supplement
has16asset/form cases and three array schemas, plus the100/101depth boundary.
The type source verifier strict-checks153 selected string assignments and four
additional any-indexed shapes; it measures50source extension/duplicate cases.
The shared native depth control is one mechanism witness, not50depth executions.
Source supplements cover ten weight/default/interceptor cases, two telemetry phases and
three returned-history-unit/alias cases. Native values precede wire comparisons.

The read-only sub-agent reviews accepted the capture/native-field structure,
API phase/coercion work and signing approach after the depth and reservation
witness corrections. Those reviews did not execute Cargo. Exact local runtime
controls and delivery gate exits are recorded separately with the measured SHA;
no gate result is transferred from an earlier commit.

No wasm clippy, central wiring/layers/routing, full workspace tests, live venue
account/network calls, Node26 or other CPU architecture were executed by this
unit. The measured JS runtime is Node24 arm64. Complete browser/native host
assembly and arbitrary executable JS object domains are not established here.


## Runtime controls at4a14e85722

The clean code baseline has70audit tests at exit0. The combined compile-valid
mutant exits101 with ten named native assertions covering nine mechanisms:
raw duplicate capture, two marker-object callers, action type, capped text,
telemetry phase, omitted weight, native asset, array reflection and encoder depth.
All50public duplicate decoders fail independently; the extra shared validation
witness also fails. The exact mutated source hashes are retained in the receipt.

The isolated physical double-parse mutant exits101 at the named twenty-token
reservation assertion. The first whole-record weight control exited101 while
compiling because removing its caller orphaned four helpers; it is NOT a runtime
control. The replacement keeps the helpers reachable, compiles, and exits101 at
native weight case10. Production restoration returns the same70audit tests to0.

Survivor: audit_json_array_user_action_signs_like_equivalent_indexed_object passes
the array-base-flattening mutant. It proves only the indexed signing schema.
audit_array_wallet_view_preserves_length_and_inherited_constructor rejects that
same mutant at its named native signing assertion. The broader native supplement
also covers three reflection schemas; the surviving narrower check is retained.

Initial extended-test attempts exposed a source-fixture cwd error and incorrect
optional-field assertion types. Those101exits were instrumentation/compile errors,
not mutation proof; the fixed fixtures/assertions are exercised in restoration.


## Origin reconciliation

Before delivery, origin advances to ed8a613e1c09b49c2cf0b3864efef28801c78996.
Its independent four-row rate continuation already repairs telemetry phase,
physical parse-once classification and omitted weight. These overlap this frozen
review and are not claimed as additional fresh integration repairs. The original
58/34classification above describes e6a8ffaf13, not a new defect count at ed8a.

Merge reconciliation keeps that phase-preserving request_data helper and its
explicit opaque-signing provenance handling, while retaining this unit's raw
field projection, fallible action conversion, marker capture and cap repair.
Both manifest sides' mappings and annotations are reconciled row by row; the
incoming five telemetry/nine request/nine default cases and GATE_EVIDENCE history
remain. Two incoming native request expectations now use source60/3000ms because
the earlier whole-Value20/1000ms limitation is repaired here. Its historical
JSON/review/gate receipts remain unchanged. No TypeScript source is altered.

## Late boundary controls

After origin reconciliation, three additional native assertions reject the old
paths: raw MessagePack marker input, weight TypeError propagation and prepared
signing canonical-wire marker retention each exit101 at their named assertion.
The existing JavaScript transport variant carries name/message, so the API row
moves from already-known to behaviour in this audit classification only; its
manifest status stays divergent. The two marker callsites extend the existing
reserved-object repair mechanism, rather than adding independent row defects.

The first prepared-marker attempt failed compilation on an unused import after
the raw codec repair, and the hand-spelled byte golden initially used26 instead
of the source's28 key bytes. Those are instrumentation errors, not runtime
controls. The corrected source MessagePack golden and actual prepared-order
object/hash witness pass; the native prepared-marker control then fails101.
The incoming independent rate continuation remains separately credited above.

## Discarded-duplicate closure measurement

The follow-up branch fix/rust-hyperliquid-duplicate-capture merges origin
0286f63437e4320e8c12886d5fef8fc6dfa2eac3 before measuring. That accepted tree
already includes the production repair in35601526a37339a516e18168b7819ec64c19e7fd.
The58behaviour classification above describes defects found and repaired during
the frozen audit, rather than58still-open defects. This follow-up adds a shared
regression target and measures the existing repair; it invents no second repair.

At f40ca5e005f1092e59ea82239d6f81042a257948, the named target
safe-cex-types/tests/discarded_duplicate_capture.rs reads the native retained
object, selected text, independent two-member count, first key position and
unrelated numeric lexeme before any wire output. Restoring exactly the old
ObjectVisitor implementation gives these measured outcomes:

| Scoped execution | Before repair counterfactual | Repaired source restored |
| --- | --- | --- |
| Shared discarded_duplicate_capture target |101: two positive native assertions fail, two refusal guards pass|0: all four pass|
| Hyperliquid divergence_types audit_duplicate_root_01 through50 |101: all50 fail decoding|0: all50 direct-field assertions pass|

The shared positive assertions separately cover a discarded D800 value and a
140-deep discarded value. Discarded malformed JSON still refuses, as do surviving
D800/depth140 values. The counterfactual is not a newly introduced integration
regression: only the historical visitor is restored temporarily, then the fixed
production file is restored byte-for-byte. The named assertion is
DISCARDED_DUPLICATE_CAPTURE native winner must decode before discarded values.

The independent expectation is the union of typed-string-boundaries.json and
typed-null-string-boundaries.json, excluding scalar OidOrCloid. It names exactly
50public roots and matches the50explicit native test types and50manifest rows.
The actual maintained-source verifier also passes50duplicate cases and153strict
assignments. Enumeration stops at audit_duplicate_root_50, BuilderFeeCheckResult;
there is no all-venue or all-input-domain claim.

Measured closure:50/50 rows clear this discarded-duplicate behaviour defect,
zero survivors. Newly repaired production rows in this follow-up:0, because the
repair was already in the accepted batch. Manifest status moves:0/50. All50rows
retain their independently priced surviving String/depth boundaries, citing
MANIFEST-SPEC.md#shared-utf8-string-boundary, #forced-surrogate-conflict and the
venue's #record-capture-budget; this control does not earn ported status.
The425ported/106divergent/0pending census is unchanged. No manifest or TypeScript
source is edited, and none of the separate historical16repair lane's API,
signing, exchange or websocket files is touched. Delivery gates for this unit
are fmt and locked workspace build; central gates remain with the overseer.

## Remaining-ten mechanism verification

Branch fix/rust-hyperliquid-mechanism-controls starts on the accepted
a19bf3b25be3600ab1d34d765cf03a276f00ed31 unit. Fetching origin still resolves to
0286f63437e4320e8c12886d5fef8fc6dfa2eac3, already merged; the newer703bc37382
batch is gating and is not represented as fetched. There is no new production
repair here. Each of the ten remaining mechanisms below already has its fix
in the accepted audit. Row subtraction does not establish outstanding defects.

At a19bf3b25be3600ab1d34d765cf03a276f00ed31, each mechanism gets its own
compile-valid regression counterfactual, named native test and restoration.
Only that mechanism's source is mutated for its run. Every counterfactual
returns101 at the intended native assertion, with0passed/1failed; every
restoration returns0 with1passed/0failed. The five production source files
are restored byte-for-byte and the tracked tree is unchanged afterward.
These are regression controls of landed repairs, not newly measured historical
pre-fix trees; for example the default control substitutes2 and the depth
control relaxes the cap to100000. No compile failure counts as proof.

| Mechanism | Named native test | Counterfactual / restored | Measured remaining |
| --- | --- | --- | ---: |
| Reserved-object capture | audit_encoded_response_retains_reserved_number_object_before_error_extraction |101/0|0|
| Fallible action.type | audit_poisoned_action_type_throws_before_unknown_action_emission |101/0|0|
| Split diagnostic refusal | audit_split_surrogate_response_refuses_before_emitting_substituted_error |101/0|0|
| Raw weight projection | weight_ingress_native_errors_batches_and_remaining_parser_findings |101/0|0|
| Typed weight error | api_weight_errors_precede_pacing_and_transport_while_capture_bypasses |101/0|0|
| Omitted weight default | audit_limiter_omitted_weight_defaults_to_one_token |101/0|0|
| Native asset retention | audit_nonfinite_asset_survives_before_authenticated_encoding |101/0|0|
| Array wallet projection | audit_array_wallet_view_preserves_length_and_inherited_constructor |101/0|0|
| MessagePack depth | audit_messagepack_source_depth_limit_is_enforced_before_signing |101/0|0|
| Telemetry data phase | audit_encoded_request_telemetry_preserves_current_data_phase |101/0|0|

Result by mechanism:10verified repaired,0remaining among these bounded checks,
0new runtime patches and0status moves. With the separately measured discarded-
duplicate mechanism, the eleven recorded mechanisms are verified closed at
their named boundaries. The425ported/106divergent/0pending census is unchanged.

The reserved-object counterfactual freshly discriminates the response-model
read; separate earlier native controls cover its raw codec/prepared-wire
callsites. Raw projection stops at native case10, so this mutation run does
not independently reach its subsequent depth140 case or re-mutate physical
parse-once classification. The typed-error mutant stops at the preparation
assertion; its restored test separately reaches execute and capture bypass.
The telemetry witness executes check_response and the actual provider callback,
without claiming a transport execution proof. The previously disclosed indexed-
array test survivor remains retained; the dedicated native reflection test is
the control that rejects flattened array length/prototype lookup.

Read-only sub-agent reviews cover the API/weight and signing/telemetry controls.
No new API, signing, socket, exchange or manifest change is delivered, avoiding
collision with the historical16repair lane. Residual FORCED/CHOSEN costs and
unproved arbitrary host/heap assembly remain priced in the original row notes;
this unit does not establish full-input or all-venue parity. The hard-stop
delivery gates are fmt and locked workspace build, measured at the delivery
commit; clippy, wasm, wiring, layers and routing remain central. No push.
