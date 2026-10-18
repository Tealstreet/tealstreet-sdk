# Hyperliquid cardinality and four-class inventory

The cardinality sweep remains **OPEN**. This iteration stops at the local venue
test binaries and exchange unit-test consumers listed below. Shared crypto
consumers are residual callees, not freshly tested in this iteration. Historical
measurements below are citations to GATE_EVIDENCE.md and FIELD_IDENTITY_AUDIT.md;
they are not fresh exits. Fresh gates and controls belong to a SHA-specific
receipt in GATE_EVIDENCE.md.

The source-level four-class inventory is generated in ROW_DISPOSITIONS.json by
`python3.11+ scripts/audit-row-dispositions.py`. Check it with
`--check ROW_DISPOSITIONS.json`. It enumerates all 531 current TOML rows, lists
all 414 implemented rows, and counts the 117 pending rows without inventing
Rust targets. All 63 earlier roots are attributed individually (35 callable,
19 type, 9 constant). The four classes are same-valued field blindness,
symmetric serde roundtrips, NaN capture portability, and per-case subprocesses.
Source-file witness references are callee inventories, not per-row mutation
proofs. Each divergent row retains its full canonical bucket and rationale.
Original feeders and manifest arrays are untouched.

Porting-first continuation from integration 2bc83d78d4 moves five pending
order-inference rows: three ported, two CHOSEN for the witnessed shared UTF-8
boundary. That inference boundary measured66/106/359 (baseline63/104/364). Further
hardening of the older104 divergent rows is paused; the sweep stays OPEN.
The inference source program executes62 cases (34 inference,16 numeric helper,
12 TIF) in one process. Native result fields, enum variants, numeric bits and
coercion errors are read directly; three escaped-surrogate cases document the
source/Rust ingress divergence. This new-row evidence does not close the older
residual consumers below.

The next builder/constants leaf starts at integration dab5ce9827 and moves19
pending rows to ported:13 constants,5 functions,1 configuration model. The
current census is85/106/340. Native UTF-16 retains the full address string
domain; optional builder/testnet getter reads are fallible and ordered, provided
configuration identity is borrowed, and fallback configuration is fresh. These
new-row tests do not close the older residual consumers listed below.

| Consumer | Existing bound / measured control scope | Residual proof obligation |
| --- | --- | --- |
| `typed_null_boundaries::compiler_rejected_null_and_missing_fields_are_rejected_by_public_decoders` | Historical NULL 150: 138 optional-null / 12 required-nullable omission; fixed native walk and 150 scalar refusals | No arbitrary collection output claim |
| `typed_ingress_audit::deep_public_ingress_repairs_existing_findings_in_four_forms` | Historical ZERO 53 roots, 238 cases, 952 native bit-map comparisons; empty/subset with source guards bypassed | Four bare routes only; broad collection decoder cardinality remains open |
| `typed_numbers::shared_utf8_refusal_and_scalar_unicode_remain_separate_from_numeric_flips` | Historical STRINGS 49/147 plus 2/6; fixed native walks, per-root refusal/positive splits | String maps/refusals only; no arbitrary collection decoder claim |
| `record_shapes::every_record_is_map_only` | 77 fixed roots, 77 qualified positional refusals, 539 boolean decode receipts; unchanged fixture plus actual AST regeneration | Acceptance/refusal receipts do not establish native record field identity; field supplements own that claim |
| `literal_unions::actual_serde_literal_sets_match_ts_and_reject_widening` | 14 fixed roots, 34 accepted / 351 rejected / 351 qualified-rejected receipts; fixed dimensions and unchanged input content | Replacing rejected input with another rejected value survives when the content guard is removed; named FINDING below |
| `typed_numbers::all_numeric_types_match_node_bits_before_serialization` | Historical 53 roots / 2380 cases / 16660 entry comparisons; 7980 fields / 55860 field comparisons across seven callers | Other types, collections and scalar helpers are not discharged |
| `types::every_ts_type_parses_its_raw_source_shape` | 207 source cases / 72 roots, fixed native walk; legacy record roundtrip | No fresh source-bypass controls here; symmetric serde is not field identity |
| `types::endpoint_tables_match_live_ts`, `typed_numeric_overflow_matches_js_ingress` | Table/edge assertions | No fresh dimension-removal or redistribution controls |
| `field_identities::per_field_identity_survives_public_decode`, `enum_field_identity_survives_native_decode` | 931 / 91 fixed native projections; historical symmetric-swap repair | Whole corpus/root redistribution controls remain open; subset coverage is not universal native identity |
| `field_identities::multi_element_collection_identity_survives_public_decode` | Historical 260 cases / 19 roots / 33 array paths / 650 elements; truncate and native-length-bypass controls | Reverse control failed in shared number checking before the supplement order assertion; remaining original collection paths open |
| `api::api_deferred_and_waiting_identities_are_distinct` | Historical four cases and native member/ID-vector controls, including empty/pop/extra and count bypass | Main API corpora remain separate targets |
| Other `api` corpus consumers: signatures, wire numbers, predicates, relay, response diagnostics, errors, coercion, user signatures, wire/refusal/guard supplements, earlier parser | Case loops, witness predicates and native/signing assertions | Independent per-subcorpus counts, fixed walks, count-bypass and consuming collection controls remain open |
| `order_history::historical_order_native_field_identity` | Eight fixed cases / 96 native field comparisons; historical timestamp-swap control | No fresh empty/subset count-bypass controls; main, boundary and private-helper corpora remain separate |
| `order_history` main / boundary / private-helper consumers | Full selected-row and transition assertions; legacy serialized model comparisons coexist with native supplement | Whole corpus/category cardinality and original collection-path controls open |
| `numeric_helpers::actual_exchange_numeric_helpers_pin_pre_serialization_values` | Whole source corpus plus string-refusal supplement and required witness predicates | Fixed native comparison totals and source-count-bypass controls open |
| `exchange::tests::{source_nonce_bits_match_before_serialization,actual_source_identity_tif_and_cancel_state_matches,placement_reads_match_source_values_and_errors}` | Historical nonce 15/30, state 533/563, placement 30/60; native count-bypass and applicable consuming controls | Scalar nonce/price/TPSL are not collection decoders; state controls do not prove constructor composition |
| Remaining exchange string / alias / reentry consumers | Separate source acceptance/effects witnesses | Independent corpus/category bounds and count-bypass controls open |
| `nan_capture_policy` | Historical NaN-only canonicalization with signed-zero and finite-payload negative controls | Capture policy only, not consumer cardinality |
| Shared crypto signing/hash consumers reached by this venue | Historical native signature and Stark domain identity evidence; external SDK builder blind spots individually disclosed in FIELD_IDENTITY_AUDIT.md | No fresh crypto build/test or broad crypto cardinality verdict |

Accepted-set controls run sequential Cargo invocations via
`scripts/check-accepted-set-cardinality.py --target record|literal`. They leave
production and fixture bytes alone, restore test consumers in `finally`, require
named runtime failures rather than compile failures, and then measure the
restored target. `--only` explicitly narrows the iteration; the full inventories
are eight record controls and eleven literal controls. This iteration selects
the original survivor and the distinguishing accepted-input replacement only;
the older full control matrix is not rerun.

The literal `replace-content-bypassed` survivor is a **FINDING** at
`tests/literal_unions.rs::actual_serde_literal_sets_match_ts_and_reject_widening`:
the concrete pair is `HyperliquidUserAbstractionMode: "" -> null`. The actual
strict TypeScript checker rejects both with code2322, while `"unifiedAccount"`
is accepted. Their raw values and diagnostic messages differ: equivalence is
only for the declared-type assignability boolean. The original survivor remains
inventoried, exit0 expected with the content guard bypassed. The additional
`replace-with-accepted-content-bypassed` control replaces `""` with
`"unifiedAccount"` and must fail `LITERAL-NATIVE rejected result
HyperliquidUserAbstractionMode`. With the content guard present, either input
replacement fails `LITERAL-CONTENT native input`. The permanent pair test also
reads the native accepted enum variant directly. This classifies the survivor;
it does not manufacture a decoder defect or claim diagnostic identity.

The 13 original findings remain individual rows, with current details copied
from their manifest notes into ROW_DISPOSITIONS.json:

1. `AllMids` — dictionary order.
2. `UserRateLimit` — dictionary order.
3. `OrderStatus` — dictionary order.
4. `WsUserEvent` — intersection model.
5. `RateLimiter.constructor` — constructor defaults.
6. `HyperliquidUnreadableOrderResponseError.constructor` — attribution.
7. `addResponseInterceptor` — UTF-16 truncation.
8. `splitSig` — general parser bounds.
9. `addSigningInterceptor` — propagated signing diagnostic contract.
10. `addressToBytes` — vault diagnostic contract.
11. `actionHash` — propagated vault diagnostic contract.
12. `signL1Action` — propagated vault diagnostic contract.
13. `signInner` — generic EIP-712 accepted-set/coercion differences for bytes arrays and boolean number/string inputs.

Additional current FINDING rows remain separate:
`HyperliquidExchange`, `HyperliquidExchange.constructor`,
`HyperliquidPublicWebSocket`, and
`HyperliquidPublicWebSocket.subscribePublicTopics`. No finding or row status is
promoted by this inventory.

Spot slice off 7fcfce7950 moves fourteen pending rows to ported: eight actual
spot tables and six closed order-schema aliases checked against both actual
source declarations, including their transitive references. Native tests cover
47 strict-TypeScript inputs, stopping on case 46 (zero-based). The current
census is 99 ported / 106 divergent / 326 pending. This is a new-row slice, not
a completion claim for the historical NULL/ZERO/STRINGS sweep, which stays OPEN.

Approval leaves off d3e8141626 move ten pending rows to ported: six constants,
two host-bound agent helpers and two spot cancellation types. The spot schedule
model explicitly preserves null, unlike the perpetual declaration. New source
corpora stop at name case49, wallet case17 and cancellation case17 (zero-based).
Current census109/106/316;215 implemented of531. This refresh does not close the
historical cardinality sweep or promote any old finding.


The spot raw-model slice off ed7ac407c7 moves 50 pending rows to ported,
zero to divergent: current census159/106/266,265 implemented of531.
Its floor is 50 fixed roots,400 strict cases (200 accepted/200 rejected),
plus five independently chosen nested-order/boolean witnesses, stopping at404.
Native values are asserted through raw JSON from_str/from_slice before any
wire assertions (there are no wire assertions here). Declared UTF-16 members,
binary64 number bits, enum tags, optional presence, last duplicate keys and
unknown-key omission are covered. Arbitrary dictionaries/intersections remain
pending with no fabricated target. This does not close the older cardinality
sweep or establish exhaustive field-pair coverage, serde Value ingress,
other formats, prototypes, excess-property retention or live alias identity.


JSON boundary continuation off6ee57c1e6a moves15pending->ported,0->divergent:
eight declared models plus seven host-bound diagnostic/payload leaves. Current
census174/106/251;280implemented of531. Model floor71cases/8roots ends70;
function floor215cases/7leaves ends214. Native receiver values, UTF-16 members
and keys, integer-key ordering, duplicate replacement before typed decoding,
array alternatives, f64 bits, fallback errors and revoked classifier catch
boundaries are asserted directly. Intrinsic JSON/String/Array operations remain
host bindings; no intrinsic implementation/live browser request claim. Cross
cache lifecycle and the fast decoder remain pending. No existing core findings
are relabeled by the new owned JSON snapshot models. Historical sweep stays OPEN.


Error/market continuation off852b13d6c0 moves12pending->ported,0->divergent.
Current census186/106/239;292implemented of531. Fixed12symbol floor275cases
ends274. Core/spot Error classes retain distinct native prototypes, own fields,
code descriptors and opaque exceptions; source handler reads are fresh. Market
scalar/fast decoder and pure tuple algorithms preserve native values, receiver
arguments and effect order, decoder/tuple/code handles and UTF-16 strings. The
external primitive implementations remain host contracts; async fetches/network
are pending. Original findings stay individually named. Historical sweep OPEN.


Cache method slice moves10pending to ported,0divergent:196/106/229;302implemented of531. Ten fixed methods/80actual-source cases end79, with independent5/5/4/17/8/17/8/3/7/6 family counts. Cache class/constructor and higher placement/pending-order lifecycle remain pending; explicit browser/JS intrinsic bindings and general cardinality are not implementation proofs. Original findings remain individually named; sweep OPEN.


Pending-cache lifecycle slice moves10pending->ported,0divergent:206/106/219;312implemented of531. Five cache methods plus five exchange callers;95new actual-source cases through94, independent7/9/17/12/8/8/10/8/8/8 counts. Existing80case leaf gate remains positive. Native stale entry retained before cleanup, 60000boundary, strict setter/deletion rejection, record/entry aliases and callback order are exercised. Full classes/constructor/singleton and other cache lifecycle remain pending; intrinsic bindings remain external, historical findings remain individually named; sweep OPEN.


Private position/state scalar slice, freshparentd47e29496d:11pending->ported,0divergent,217/106/208;323implemented of531. Eleven actual source methods/133cases through132; independent12/21/10/4/10/11/11/11/12/13/18 counts. Native values/typed summary fields and PnL/queue/Set state are checked; full WS composition/handlers and intrinsic/virtual callee implementations remain outside scope. Original findings stay individually named; sweep OPEN.

Websocket ping/opening slice, freshparentb5ac6229e2:10pending->ported,0divergent,227/106/198;333implemented of531. Public/private onOpen/start/schedule/stop/ping sourcealgorithms with123fixedcases through122 and native lexical callback body. Native fields/timer registrations/Set clear/UTF16/calleepointer and sourceorder/throws asserted; global/timer primitives and virtual callees remain host contracts, full class/live composition unproved. Historical sweepOPEN; originalfindings remain individual.

Websocket transport slice, freshparent3042a36afe:10pending->ported,0divergent,237/106/188;343implemented of531. Four explicitvenue connectors, two spotopening/ping leaves and two private dispatchers. Native sourcefloor202through201, independent36/36/5/36/9/15/36/9/15/5. Distinctperpapi-ui/spotapi URLs, freshsocket/handler/callee reads, field/listener states and immediate refreshboolean preserved. Global/browser/JSON primitives and virtualcallees remainhostbindings; fullclasses/liveoperation/exhaustivecardinality unproved. Originalfindings stayindividual; sweepOPEN.

Websocket emission slice, freshparent71eabb7cb6:10pending->ported,0divergent,247/106/178;353implemented of531. Four diagnostic and six subscription algorithms with118fixedsourcecases through117; independent14/14/14/14/11/12/5/13/10/11. Native ordered literal trees beforewire, formatter scope/body, capturedcore/freshspotuser, venue6/7types and iteratorclose/order are checked. PrimitiveGetV/iterator/injection oracle gaps corrected and independenthappy-path/getter pins added. Fullclasses/hostprimitives/callee/liveoperation/exhaustivecardinality unproved; originalfindings individuallyretained; sweepOPEN.

Account/position leaf slice, freshparent6182276a69:10pending->ported,0divergent,257/106/168;363implemented of531. Accountcollateralguards, core/spot symbolconfig and fake-positioncache access, invalidation/reduceonly/oppositesize predicates. Native129cases through128; independent13/12/20/20/16/3/23/15/3/4. Nativefield/bits/defaultobject/cache/requested alias and someaggregate beforewire, freshreads/optionalboundary andcallee/errororder checked. Fullclasses/generators/hostintrinsics/generalreentrantcollections remainoutsideproof. Originalfindings individual; sweepOPEN.

Lifecycle/delegate slice ports10pending roots: four listener forwarding leaves, four disposal leaves, retry cleanup and debounced refresh scheduling with owned Rust timer/rejection callbacks. Native sourcefloor111through110; intrinsic/virtualcallee/fullclass and universal behavior remain outsideproof. Historical sweep OPEN; originalfindings retainedindividually.

Market/data selectors ports10pending roots; native sourcefloor161through160. Map/key/record aliases, Set insertion-order/dedup, summary91.25/17.5 and source actual spot/perp precision outputs inspected. Imported primitives/libraries/fullclass/universal behavior outsideproof; historical sweep OPEN, original findings stay individual.


Balance-state slice adds ten pending-to-ported leaves (287/106/138 census). Source floor152cases through151: core price22/builder16/availability17/mode15/snapshot12/refresh9; spot price28/builder16/apply8/refresh9. Native fields, arithmetic, UTF16/handles/prototypes, own literal data vs ordinary __proto__ assignment, iterator closing, and owned find predicate precede any wire checks. Two observed development harness findings are retained separately in GATE_DEVELOPMENT_BALANCE.json and their scoped regression controls. Host/intrinsic/virtual callee implementations, async refresh and full exchange classes remain outside this proof. Original thirteen findings retain individual names above; historical sweep OPEN.


Order-request slice adds ten pending-to-ported bodies (297/106/128). Native source floor143cases through142, independent counts12/25/9/12/8/17/15/15/15/15. Manual await counts3/0/5/0/3/15/16/16/18/18=94; pending-once checks source cutpoint ordinals and no continuation before resolution. Core five-argument vs spot four-argument signing, explicit payload/telemetry fields, catch/error replacement, source getter order and clock+5000 equality are native assertions before wire. First-poll/host-await scope only: arbitrary eager JS async invocation vs lazy Rust futures, Promise assimilation/thenables/scheduling, imported helper/signing implementations and fullclasses/live/general behavior remain outside proof. GATE_DEVELOPMENT_ORDER_REQUEST.json retains development compile failures and real corrected vault-spread evaluation-order failure; scoped committed controls retain production and harness regressions. Original findings remain individually named; historical sweep OPEN.


Browser approval slice adds eleven pending-to-ported bodies (308/106/117). Source floor134cases through133; independent rootcounts10/10/11/18/10/10/16/10/11/10/18 and manual hostawaitcounts11/9/16/58/15/11/24/9/16/0/58=227. Native fields/handles/type domains/decoded r-s-v, scalar defaults/nonfinite fees, iteratorless local retry bodies, error/catch evaluation order, JSON input fields before wire and first-poll pending-once cutpoints are checked directly. Four observed dynamic-message allocation-order defects corrected in production, retained with initial native101 in GATE_DEVELOPMENT_BROWSER_APPROVAL.json and scoped per-case committed regression controls. Live provider/network/ethers/JSON/imported or sibling-callee implementations, arbitrary eager JS invocation/local Promise microtask scheduling and general/full-class behavior remain outsideproof. Original findings remain individually named; historical sweep OPEN.
