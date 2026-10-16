# GRVT divergent-row audit

Audit parent: dd5f6415f4955e2245bfdb22039b80897a94adc7, after merging
origin integration ca0ce4fb921387e55a180c9bf136b88950cd8869 into the reviewed
shared-diagnostics branch. That accepted ancestry is not an unreviewed feature.
Independent TOML census: 113 ported  / 51 divergent  / 0 pending.
The frozen enumeration is sorted manifest filename, then source row order.

Canonical rules are cited at
[14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary),
[16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect),
[18](../parity/MANIFEST-SPEC.md#divergence-taxonomy), and
[19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).
These declarations are not restated here or in individual row notes.

## Order-book map operation

Iteration51 exposed a behaviour defect independently of the existing sparse
output restriction. The maintained listenOrderBook invokes rawBids.map and
rawAsks.map; native code instead performed fixed length/index iteration.
A dense bids array with its own map returning [] therefore delivered one native
level where the source delivers none.

At 11412c52968757d4a1b38133741057219999c8ce, custom_orderbook_map_native
compiled and failed 101 at GRVT MAP native overridden map result (actual1,
expected0). Source and native assertions inspect callback values directly;
there is no native decode/encode roundtrip. Further successes and controls
will be attached to their measured commits, not inferred for this test commit.

The repair retains method/receiver lookup order and a native Rc projector.
The host must preserve opaque callback arguments and live relational coercion;
returned own fields keep Number bits. Both map calls precede strict output
admission without executing output getters. Unsupported map results visibly
refuse the whole callback under the required shared OrderBook surface.
Required host operations are capabilities, not a concrete JavaScript runtime
implementation or an all-venue assembly proof. The initial supplement stopped at nine
fixed cases, including throw/noncallability, retained callbacks, alternate
arrays, changed asks.map, and two typed-output refusals.

At b7c2d13c5617dcf9dbefc305a91acca684f7a52b, the scoped public_feeds
target passed 0: seven tests, zero filtered. The first nine-case supplement and
source producer passed, but their execution-count guards were insufficient.
Two compiling/valid controls then SURVIVED with 0 and restored 0:
public-map-source.cjs skipped its entire for-of loop; custom_orderbook_map_native
used names.into_iter().take(0). These are test-harness findings, not runtime
repairs or new claims that nine cases were irreducibly protected.
The follow-up adds independent completed 10 assertions to both loops and adds a
retained dense-result alias witness. It also replaces the misleading bare
accessor sentinel with an array containing an accessor row, distinguishing that
refusal from a nonarray refusal. Repeated controls must fail at the new named
completed-case assertions before these guards count as discrimination evidence.

At d326d2adaa0223c0a22287fe65b976545281ac69 the ten-case native/source
supplement passes0. Repeating the two previously surviving controls now gives
source 1 at GRVT MAP source completed case count and native 101 at GRVT MAP
native completed case count; both restored runs return 0 with bytewise recovery.
The control enumeration stops at 2. The earlier two survivors remain recorded;
they are not discarded or relabeled as successful initial controls.

## Record observation supplement

The source checker corpus did not establish a post-JSON.parse code-unit
observation for every record. In particular, nested subscription params.channel
and error.message lacked that evidence. The new supplement selects 22 concrete
fields, with five independent scalar/astral/replacement/literal-escape/whitespace
positives and two exact code-unit observations each. Native tests read those
selected fields directly and separately assert whole-record refusal. These are
selected-field witnesses, not an exhaustive field census or a re-encoded proof.
Seven numeric-capture records also exercise an unknown 140-deep extension;
source variable assignability is checked against actual interface declarations.
At 5ca9e8150f51bbc1d4b3600cff74e3fd38f78d6d, divergence_records
passed 0 (one test, zero filtered). At e506b521d2, the strengthened
divergence_records and order_semantics targets passed 0: five tests total,
including the two permanent source mutation controls. That depth assertion
was initially a diagnostic substring. An exact-text follow-up at
02fd30720bbde021ef780e009bffb9b878a16ccf failed 101 because the native error
also carries a parser-position suffix (line1 column128). The final assertion
checks the named diagnostic prefix and retains the shallow-extension positive;
no production repair was needed for that test-expectation error. Measurements
are repeated on the delivery commit below.

<a id="grvt-wire-data-choice"></a>
## Local protocol data scope and capture choice

The 22 records below remain protocol data, decoded from map-shaped Serde input,
rather than retained executable JavaScript structural objects. Supporting
getters, prototypes, functions or named properties on arrays would require an
object/callback carrier throughout these native records. We retain the common
data contract to keep wire records usable independently of a JavaScript host.
This is descriptive API scope, not an independently measured divergence for
each record. The supplement does not execute getters, inherited fields or
augmented arrays. It is not a claim that TypeScript rejects such structural objects or that Rust
cannot implement them. It is also not permission to replace dynamic operations
inside PublicFeedHost with fixed native iteration: that separate gap was repaired.

Seven records buffer the whole input through public numeric capture, including
unknown fields, before their private codec can ignore an extension. Their
140-deep unknown extension is accepted and remains usable in TypeScript; native
capture rejects the entire record with numeric ingress recursion limit exceeded.
A shallow extension succeeds and retains the selected field. We retain this
bounded capture rather than permit unbounded recursive buffer work at protocol
ingress. This is a CHOSEN resource-policy cost, separate from the selected-field
observations governed by the canonical links above. The seven are
GrvtBuilderCheckResult, GrvtBuilderInfo, GrvtSignature, OrderPayload,
GrvtInstrumentLite, GrvtWebSocketSubscription and GrvtWebSocketMessage.
No such depth measurement is claimed for the other fifteen.

## Complete frozen divergent block

The enumeration stops at iteration 51: every divergent row in the frozen GRVT
block, not every possible behaviour or future source revision. Ten original
notes had a supported local explanation; 41 needed resolution, with primary
finding types 14 explanation, 26 documentation, 1 behaviour, 0 scheduling and 0
already-known. These types describe the original audit deficiency, independently
of the five-bucket classification of each remaining divergence. Documentation
and explanation overlap; each row is counted only under the primary type below.
Cross-review withdrew 15 proposed extra data-only MIXED causes: the map-only
decoder did not independently establish an altered executable-object contract
for those plain records. Those 15 keep the measured FORCED field cause only.
The source unit observations resolve the held fetchMarkets explanation; there
is no unproved diagnostic-consumer claim behind a FORCED classification.

| Iteration | Source symbol | Original audit result | Remaining bucket | Resolution / evidence |
| --- | --- | --- | --- | --- |
|1|grvtBuilderFeeRateToRaw|explanation|MIXED|Actual authorization return retains fee-error units versus numeric-prefix input loss; native Result error choice; builder_helpers/divergence_helpers.|
|2|GrvtBuilderUnauthorizedError|explanation|MIXED|Inspectable custom message units; separately retained native error without engine prototype/stack.|
|3|GrvtBuilderUnauthorizedError.constructor|explanation|MIXED|Same construction boundary; builder_error plus returned-message supplement.|
|4|GrvtBuilderAuthorizationResult|explanation|FORCED|Selected message observation and whole-record refusal.|
|5|GrvtBuilderCheckResult|explanation|MIXED|Selected maxFeeRate, whole-record loss and measured numeric-capture depth cost.|
|6|GrvtBuilderInfo|explanation|MIXED|Selected builder, whole-record loss and measured numeric-capture depth cost.|
|7|viaGrvtProxy|explanation|FORCED|Selected proxy URL unit reaches actual authentication fetch URL, not an invented caller base.|
|8|resolveGrvtProxyHeaders|supported|FORCED|Existing fixed shared ABI: sorted headers and discarded pending future; eager invocation already repaired.|
|9|getStringField|explanation|MIXED|Actual authentication returns fundingAccountAddress unit via helper; separately data-only unknown input.|
|10|getValidAddress|explanation|CHOSEN|Invalid surrogate candidate skipped before later valid address; whole candidate-list admission is the cost.|
|11|getGrvtEdgeApiUrl|explanation|FORCED|Actual constants and both networks pass selected proxy-unit URL to actual caller fetch.|
|12|getGrvtTradingApiUrl|explanation|FORCED|Actual constants and both networks pass selected proxy-unit URL to actual caller fetch.|
|13|createGrvtBuilderInfo|explanation|MIXED|Actual returned builder field unit; separately native message-only Error carrier; builder_info retains scalar lowercase/fee evidence.|
|14|GrvtExchange|supported|CHOSEN|Native owned composition and required bound members; no concrete all-member network assembly claim.|
|15|GrvtExchange.getOrderCreationTimeMs|supported|CHOSEN|Data-only metadata; native decimal quotient/Number bits, not input-only surrogate as FORCED numeric loss.|
|16|GrvtExchange#symbolToInstrumentId|explanation|FORCED|Actual cancelSymbolOrders passes retained unit in normalized SDK instrument_id.|
|17|GrvtExchange.fetchMarkets|explanation|MIXED|Actual two-market batch returned with unit; whole Value-response refusal precedes method; separate later typed/data choices.|
|18|GrvtExchange.refreshMarkets|supported|CHOSEN|Existing old-value alias witness and immutable owned changes through shared Store.|
|19|GrvtExchange#getInstrumentSigningMetadata|documentation|CHOSEN|Removed obsolete pending-caller claim and duplicated declaration; retained data/error choice and 3204-case scope.|
|20|GrvtExchange#updateMarketLeverageLimits|documentation|CHOSEN|Removed stale implementation status; source sameFirst versus native clone witness retained.|
|21|GrvtExchange.getFakePositions|supported|CHOSEN|Existing inherited leverage/undefined whole-batch refusal and canonical closed-output rationale.|
|22|GrvtExchange#mapOrderStatus|supported|CHOSEN|Input-only surrogate costs, ordinary 47-case native variant floor; mapOrder implementation no longer called missing.|
|23|GrvtExchange.updateOrder|supported|CHOSEN|Existing raw extra-key diagnostic 2 versus 1 witness and cost of duplicate raw bag.|
|24|GrvtExchange#convertInterval|supported|CHOSEN|Input-only default and closed Timeframe caller; inherited TableHit preserved, not FORCED unit loss.|
|25|GrvtExchange.fetchLeverageForSymbol|documentation|CHOSEN|Existing live-ID 7.5 versus owned 10/cache alias price; remove obsolete runtime-pending sentence.|
|26|GrvtExchange.setLeverage|documentation|CHOSEN|Existing owned-ID and own-undefined SDK argument price; remove obsolete runtime-pending sentence.|
|27|getGrvtOrderTimeInForce|documentation|CHOSEN|Direct native variants now precede wire; data-only input cost, remove pending-mapper assertion.|
|28|getGrvtTriggerMetadata|documentation|CHOSEN|Direct native kind/trigger/flags/bits before wire; data-only cost, remove pending-mapper assertion.|
|29|GrvtSignature|documentation|MIXED|Selected r; numeric-capture depth cost; runtime existence no longer denied by type fixture.|
|30|OrderLeg|documentation|FORCED|Selected instrument and whole-record loss; parsed protocol-field observation.|
|31|OrderPayload|documentation|MIXED|Selected subAccountId; numeric-capture depth cost.|
|32|GrvtInstrument|documentation|FORCED|Selected instrument_id and whole-record loss; parsed protocol-field observation.|
|33|GrvtInstrumentLite|documentation|MIXED|Selected i; numeric-capture depth cost; earlier clean candidate stays fixture, fetchMarkets implemented.|
|34|GrvtTicker|documentation|FORCED|Selected instrument_id and whole-record loss; parsed protocol-field observation.|
|35|GrvtOrderBookLevel|documentation|FORCED|Selected p; native field control detects swapped p/s.|
|36|GrvtOrderBook|documentation|FORCED|Selected instrument_id and whole-record loss; parsed protocol-field observation.|
|37|GrvtTrade|documentation|FORCED|Selected trade_id and whole-record loss; parsed protocol-field observation.|
|38|GrvtKline|documentation|FORCED|Selected timestamp and whole-record loss; parsed protocol-field observation.|
|39|GrvtPosition|documentation|FORCED|Selected sub_account_id and whole-record loss; parsed protocol-field observation.|
|40|GrvtBalance|documentation|FORCED|Selected currency and whole-record loss; parsed protocol-field observation.|
|41|GrvtAccountSummary|documentation|FORCED|Selected sub_account_id and whole-record loss; parsed protocol-field observation.|
|42|GrvtOrder|documentation|FORCED|Selected order_id and whole-record loss; parsed protocol-field observation.|
|43|GrvtFill|documentation|FORCED|Selected fill_id; inline liquidity target retained; parsed protocol-field observation.|
|44|GrvtAuthRequest|documentation|FORCED|Selected api_key and whole-record loss; parsed protocol-field observation.|
|45|GrvtAuthResponse|documentation|FORCED|Selected session_token and whole-record loss; parsed protocol-field observation.|
|46|GrvtWebSocketSubscription|documentation|MIXED|New nested params.channel observation, numeric-capture depth; split targets retained.|
|47|GrvtWebSocketMessage|documentation|MIXED|New nested error.message observation, opaque data and numeric-capture depth; split targets retained.|
|48|GrvtPrivateWebSocket|supported|CHOSEN|Typed composition/lifecycle invariants; concrete member adapters remain unproved.|
|49|GrvtPublicWebSocket|supported|CHOSEN|Typed subscription/timer state ownership; concrete member adapters remain unproved.|
|50|GrvtPublicWebSocket#mapIntervalToGrvt|documentation|CHOSEN|Callable inherited builtin cost retained; remove obsolete class/connection/replay/batch-pending claim.|
|51|GrvtPublicWebSocket.listenOrderBook|behaviour|MIXED|Live map repaired; fixed typed output and owned dense-result alias costs remain explicitly separated.|

Remaining buckets: 20 FORCED, 17 CHOSEN, 14 MIXED, 0 CORRECTED, 0 FINDING.
Status moves: 0. Independent final census must remain 113 ported / 51 divergent / 0
pending. One new production mechanism repairs one divergent source row. Its
local callers are GrvtPublicWebSocket.listenOrderBook and the exchange's public
orderbook delegation. No other venue inherits PublicFeedHost, and no all-venue
repair claim is made. Existing composition, SDK and concrete JavaScript-host
assembly limits remain unproved rather than converted into passing claims.

The helper source review corrected the isolated crypto-dependency loader and
the captured cancel argument. The 22 helper observations describe required
string surfaces (individual proxy URL fields) or whole response admission;
refused helper calls themselves are not executed. Historical controls at
fa3e8e3d7e caught native record p/s, native TIF, native trigger kind, and skipped
record/helper producer loops (101/101/101/1/1, restored 0 each). Annotation
edits overlapped that historical control run; the delivery repeats them on
the committed tree rather than transferring those measurements.

Row 1 does not rely only on a fixture catch: the whole maintained
authorizeGrvtBuilderWithBrowserWallet body runs with configured pre-fee wallet
and authentication hooks, invokes the actual fee helper, and returns the caught
message in its public authorization result. The two materialized units are
asserted there before serialization; concrete wallet/network assembly is not
claimed. Constructor rows 2/3 instead inspect the directly returned error instance.

The helper supplement also executes whole authenticateGrvt and
fetchGrvtMainAccountId bodies with configured fetch responses. It observes both
networks' outgoing fetch URL arguments and authenticateGrvt's returned
fundingAccountAddress. The private URL/string helper classifications therefore
use executed maintained consumers, not only direct fixture reads. Timeout and
fetch hooks in this supplement are fixtures; no real timer, network or browser
assembly evidence is inferred.
