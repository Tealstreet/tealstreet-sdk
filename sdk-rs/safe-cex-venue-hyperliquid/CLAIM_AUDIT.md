# Current exchange-state slice

Branch accounting: **63 ported / 104 divergent / 364 pending**, 531 total.
This adds eight implemented state/getter rows with shared representation bounds
(six CHOSEN, two MIXED), plus TWO explicitly incomplete class/constructor FINDING
rows. These two are unfinished composition, not completed constructor behavior.
The original 13 findings remain open. Buckets: 80 CHOSEN / 17 FINDING / 7 MIXED.

The actual source state-member oracle has 533 typed-input cases, recorded before
implementation. It excludes client/socket construction. An earlier untyped cache
probe uses GTC/IOC/FOK; those runtime inputs are not a claim about the declared
OrderTimeInForce accepted set. The active native test uses the independently
captured declared enum-valued corpus. Code-unit string, clock-reentry and
caller-options witnesses are separate so numeric/operational success cannot
absorb their representation differences.

The privacy getters cite BOTH shared UTF-8 and owned-snapshot declarations.
The source ownership statement and getter bodies observe caller alias changes;
Rust snapshots remain frozen, while an internal account-identity patch positively
updates the getters. The constructor's other dependencies remain excluded.
No native batch override, API construction or socket lifecycle is claimed.

# Numeric/null repair snapshot (consolidated)

Current measured rows: **62 ported / 87 divergent / 382 pending**, 531 total,
from 58 / 91 / 382. All 53 numeric findings are closed by deep public
Deserialize adoption, with 2,380 actual-Node cases on seven named entry routes
(16,660 bit comparisons before serialization). Raw overflow becomes Infinity;
existing signed zero remains negative zero. TriggerOrderType.triggerPx is a
signed-zero regression guard and independently an overflow repair, not a blanket
no-change claim. The old Value-first candidate was rejected and never adopted.

Four numeric-only roots become ported. Forty-nine numeric roots remain CHOSEN
for their separately witnessed UTF-8 boundary. AssetCtx and SpotAssetCtx also
close their optional/null findings but retain that same boundary. Therefore
**51 of 72 CHOSEN rows cite one shared representation choice**, not 51 independent
Hyperliquid decisions. The remaining buckets are **13 FINDING / 2 MIXED**.
The remaining findings are the three local dictionary ordering rows, intersection,
limiter defaults, constructor attribution, UTF-16 diagnostic truncation,
signature parser, generic EIP-712 bounds, and four propagated vault diagnostics.

Public record and numeric-union Deserialize is the measured caller: bare raw
str/slice, owned/borrowed Value and three shared entries. Private companions and
strict map/string guards remain. Optional-null and required-nullable presence
are separate axes, pinned by 150 actual-source compiler rejections captured first.
Four production bypasses compiled and failed named runtime assertions; the source
context-widening control failed its named rejection assertion. Restored tests and
generator check pass. The scoped adversarial review returned MERGE; this does not
claim the remaining venue findings or exchange/WS are complete.

# Hyperliquid claim audit

Historical baseline below: cd47115bb7 (superseded classifications, retained as the audit record). This is an audit with unresolved findings, **DO NOT MERGE**, not a behavioral completion claim.

The original 86 divergent rows resolve to **61 FORCED, 0 CORRECTED, 3 CHOSEN, 9 findings and 13 incorrectly blanket-labelled rows promoted to ported on declared JSON shapes**. The original 53 ported rows resolve to **42 retained, 4 FORCED, 1 CHOSEN and 6 findings**. Counts are row accounting, not cryptographic complexity.

The ratio was substantially inflated by applying one generic note to every type. The dominant 53-row forced group is the concrete existing typed serde f64 ingress, which rejects overflow tokens; Rust f64 itself can hold Infinity. There were no deliberate corrected TypeScript bugs: the three documented quirks remain preserved.

Declared-shape type claims cover TS-declared JSON fields, not arbitrary structural excess fields or lone UTF-16 surrogates. Thirteen types have no reachable numeric/optional-nullable mismatch on that domain. Required nullable PerpDex fields retain null. AssetCtx/SpotAssetCtx accept TS-invalid null at optional non-null fields; these remain findings rather than receiving the false old blanket explanation.

## Findings

| Finding | Original affected rows | Evidence |
| --- | --- | --- |
| F-MAP-ORDER | AllMids, UserRateLimit, OrderStatus | BTreeMap sorts z/a keys; JS keeps non-index insertion order. Ordered maps are available; no worse-mirroring rationale. |
| F-INTERSECTION | WsUserEvent | Bare Value erases union/intersection; oracle invents user-only object that matches no alternative. No real type-shape evidence. |
| F-OPTION-ACCEPTANCE | AssetCtx, SpotAssetCtx | Optional non-null strings accept/erase null; required nullable midPx is retained. Original optional-null note conflated these. |
| F-DEFAULTS | RateLimiter.constructor | TS defaults capacity/window; Rust requires both. Clock injection does not justify omitting defaults. |
| F-SIGNATURE-PARSER | splitSig | General ethers recovery/input normalization omitted; private generated-output splitter does not establish general equivalence. |
| F-DIAGNOSTIC | addressToBytes, actionHash, signL1Action, addSigningInterceptor | BytesLike diagnostic loss repaired: native Realm and ApiClient retain TypeError, versioned message, code/argument/value/shortMessage through the canonical owned cause and transport projection. Five accepted and three rejected vault profiles execute maintained TS plus installed ethers. The earlier message-only assertion is corrected. |
| F-WIRE-GUARDS | orderTypeToWire | CLOSED: oracle-first null/invalid-object cases now pin TypeError/Error names and exact messages. |
| F-WIRE-BYTES | orderRequestToOrderWire | CLOSED: request-owned views preserve JS nil signing versus JSON omission, and missing/null order_type plus root access error names/messages. Original signing-mismatch inference was disproved by actual TS bytes (see measurement below). |
| F-CONSTRUCTOR | HyperliquidUnreadableOrderResponseError.constructor | Mapping points to a response parser, not arbitrary-message error construction. Class-level error divergence does not validate this rs target. |

Nine original-divergent and six original-ported finding rows remain labelled divergent with explicit FINDING notes. They have no approved forced/corrected/chosen explanation and must not be counted as completed behavior. Existing green structural parity checks only prove source resolution, not these semantics.

## Chosen rationale

The original three CHOSEN rows are getOrderErrorTelemetry and both API factories. Every row now carries its own worse-mirroring sentence: typed optional callbacks avoid JS-object downcasts, and injected adapters retain shared transport lifecycle instead of venue-owned platform clients. createAPI inherits that factory choice and is newly divergent. This does not excuse missing behavior: qs repeat serialization is separately BLOCKED on codex-f6xyia core HttpClientOptions optional serializer with SIGN-then-SERIALIZE-then-TRANSMIT, wrapped across tunnel installation. It is not a permanent divergence.

## Ported-row audit method and controls

Two isolated reviewers inspected the 72 type rows and all original 53 ported rows independently at /tmp/hl-types-taxonomy-audit and /tmp/hl-claims-audit. Both returned DO NOT MERGE unchanged claims. Live Node paired with raw-string Rust probes demonstrated the wire/null and propagated-error findings. The baseline live-source/negative-control evidence remains in GATE_EVIDENCE.md; it was insufficient to expose these omissions.

All original ported rows were checked for missing guards, idiomatic bug fixes, Display versus interpolation, ?? versus ||/undefined versus null, error identity/timing, shared state and forgiving builtins. Remaining tables, weight predicates, order-response selection/dedup/deferred handling and retry distinctions showed no concrete mismatch on tested representable inputs. Error constructor was excluded from that conclusion. Wallet-facing signatures now announce raw-key/Result ABI and inherited signInner bounds explicitly.

## Row ledger

| File | Symbol | Original status | Audit classification |
| --- | --- | --- | --- |
| hyperliquid.api | getRequestActionType | ported | PORTED |
| hyperliquid.api | getOrderErrorTelemetry | divergent | CHOSEN |
| hyperliquid.api | addResponseInterceptor | divergent | FORCED |
| hyperliquid.api | addSigningInterceptor | ported | FINDING |
| hyperliquid.api | addRateLimitInterceptor | divergent | FORCED |
| hyperliquid.api | createExchangeAPI | divergent | CHOSEN |
| hyperliquid.api | createInfoAPI | divergent | CHOSEN |
| hyperliquid.api | createAPI | ported | CHOSEN |
| hyperliquid.order-response | EMPTY_HYPERLIQUID_ORDER_RESPONSE_MESSAGE | ported | PORTED |
| hyperliquid.order-response | ParsedHyperliquidOrderResponse | ported | PORTED |
| hyperliquid.order-response | HyperliquidUnreadableOrderResponseError | divergent | FORCED |
| hyperliquid.order-response | HyperliquidUnreadableOrderResponseError.constructor | ported | FINDING |
| hyperliquid.order-response | isRecord | ported | PORTED |
| hyperliquid.order-response | getRecord | ported | PORTED |
| hyperliquid.order-response | getNonEmptyString | ported | PORTED |
| hyperliquid.order-response | describeResponse | ported | PORTED |
| hyperliquid.order-response | isOnlyHyperliquidTerminalCancelError | ported | PORTED |
| hyperliquid.order-response | getResponseData | ported | PORTED |
| hyperliquid.order-response | getStatuses | ported | PORTED |
| hyperliquid.order-response | getOid | ported | PORTED |
| hyperliquid.order-response | isHyperliquidDeferredOrderStatus | ported | PORTED |
| hyperliquid.order-response | extractHyperliquidResponseError | ported | PORTED |
| hyperliquid.order-response | parseHyperliquidOrderResponse | ported | PORTED |
| hyperliquid.order-response | hyperliquidOrderResponseHasWaitingForTrigger | ported | PORTED |
| hyperliquid.order-response | hyperliquidOrderResponseHasDeferredOrderStatus | ported | PORTED |
| hyperliquid.rateLimiter | isEndpoint | ported | PORTED |
| hyperliquid.rateLimiter | parseRequestData | ported | PORTED |
| hyperliquid.rateLimiter | getArrayLength | ported | PORTED |
| hyperliquid.rateLimiter | getExchangeBatchLength | ported | PORTED |
| hyperliquid.rateLimiter | getInfoBaseWeight | ported | PORTED |
| hyperliquid.rateLimiter | getInfoExtraWeight | ported | PORTED |
| hyperliquid.rateLimiter | getHyperliquidRestRequestWeight | ported | PORTED |
| hyperliquid.rateLimiter | RateLimiter | divergent | FORCED |
| hyperliquid.rateLimiter | RateLimiter.constructor | divergent | FINDING |
| hyperliquid.rateLimiter | RateLimiter.waitForToken | divergent | FORCED |
| hyperliquid.rateLimiter | hyperliquidRestRateLimiter | divergent | FORCED |
| hyperliquid.signing | orderTypeToWire | ported | FINDING |
| hyperliquid.signing | addressToBytes | divergent | FINDING |
| hyperliquid.signing | actionHash | ported | FINDING |
| hyperliquid.signing | constructPhantomAgent | ported | PORTED |
| hyperliquid.signing | signL1Action | ported | FINDING |
| hyperliquid.signing | signUserSignedAction | ported | FORCED |
| hyperliquid.signing | signUsdTransferAction | ported | FORCED |
| hyperliquid.signing | signWithdrawFromBridgeAction | ported | FORCED |
| hyperliquid.signing | signAgent | ported | FORCED |
| hyperliquid.signing | signInner | divergent | FORCED |
| hyperliquid.signing | splitSig | divergent | FINDING |
| hyperliquid.signing | floatToWire | ported | PORTED |
| hyperliquid.signing | floatToIntForHashing | ported | PORTED |
| hyperliquid.signing | floatToUsdInt | ported | PORTED |
| hyperliquid.signing | floatToInt | ported | PORTED |
| hyperliquid.signing | getTimestampMs | divergent | FORCED |
| hyperliquid.signing | orderRequestToOrderWire | ported | FINDING |
| hyperliquid.signing | CancelOrderResponse | ported | PORTED |
| hyperliquid.signing | cancelOrderToAction | ported | PORTED |
| hyperliquid.signing | orderWiresToOrderAction | ported | PORTED |
| hyperliquid.signing | orderWiresToNormalTPSLAction | ported | PORTED |
| hyperliquid.signing | orderWiresToPositionalTPSLAction | ported | PORTED |
| hyperliquid.types | BASE_URLS | ported | PORTED |
| hyperliquid.types | WSS_URLS | ported | PORTED |
| hyperliquid.types | ENDPOINTS | ported | PORTED |
| hyperliquid.types | GRAFANA_ENDPOINTS_EVERY | ported | PORTED |
| hyperliquid.types | GRAFANA_ENDPOINTS_PERIODIC | ported | PORTED |
| hyperliquid.types | INFO_TYPES | ported | PORTED |
| hyperliquid.types | EXCHANGE_TYPES | ported | PORTED |
| hyperliquid.types | WEBSOCKET | ported | PORTED |
| hyperliquid.types | HyperliquidUserAbstractionMode | divergent | PORTED |
| hyperliquid.types | Tif | divergent | PORTED |
| hyperliquid.types | Tpsl | divergent | PORTED |
| hyperliquid.types | LimitOrderType | divergent | PORTED |
| hyperliquid.types | TriggerOrderType | divergent | FORCED |
| hyperliquid.types | Grouping | divergent | PORTED |
| hyperliquid.types | OrderType | divergent | FORCED |
| hyperliquid.types | Cloid | divergent | PORTED |
| hyperliquid.types | OidOrCloid | divergent | FORCED |
| hyperliquid.types | AllMids | divergent | FINDING |
| hyperliquid.types | Meta | divergent | FORCED |
| hyperliquid.types | ClearinghouseState | divergent | FORCED |
| hyperliquid.types | UserFills | divergent | FORCED |
| hyperliquid.types | OrderResponseStatus | divergent | FORCED |
| hyperliquid.types | OrderResponse | divergent | FORCED |
| hyperliquid.types | WsTrade | divergent | FORCED |
| hyperliquid.types | WsBook | divergent | FORCED |
| hyperliquid.types | WsBbo | divergent | FORCED |
| hyperliquid.types | WsLevel | divergent | FORCED |
| hyperliquid.types | WsOrder | divergent | FORCED |
| hyperliquid.types | WsUserEvent | divergent | FINDING |
| hyperliquid.types | WsFill | divergent | FORCED |
| hyperliquid.types | WsUserFunding | divergent | FORCED |
| hyperliquid.types | WsLiquidation | divergent | FORCED |
| hyperliquid.types | WsNonUserCancel | divergent | FORCED |
| hyperliquid.types | SpotClearinghouseBalance | divergent | FORCED |
| hyperliquid.types | SpotClearinghouseState | divergent | FORCED |
| hyperliquid.types | FrontendOpenOrders | divergent | FORCED |
| hyperliquid.types | UserRateLimit | divergent | FINDING |
| hyperliquid.types | OrderStatus | divergent | FINDING |
| hyperliquid.types | L2Book | divergent | FORCED |
| hyperliquid.types | CandleSnapshot | divergent | FORCED |
| hyperliquid.types | AssetCtx | divergent | FINDING |
| hyperliquid.types | MetaAndAssetCtxs | divergent | FORCED |
| hyperliquid.types | UserFundingDelta | divergent | PORTED |
| hyperliquid.types | UserFundingEntry | divergent | FORCED |
| hyperliquid.types | UserFunding | divergent | FORCED |
| hyperliquid.types | UserNonFundingLedgerDelta | divergent | PORTED |
| hyperliquid.types | UserNonFundingLedgerEntry | divergent | FORCED |
| hyperliquid.types | UserNonFundingLedgerUpdates | divergent | FORCED |
| hyperliquid.types | FundingHistoryEntry | divergent | FORCED |
| hyperliquid.types | FundingHistory | divergent | FORCED |
| hyperliquid.types | SpotToken | divergent | FORCED |
| hyperliquid.types | SpotMarket | divergent | FORCED |
| hyperliquid.types | SpotMeta | divergent | FORCED |
| hyperliquid.types | SpotAssetCtx | divergent | FINDING |
| hyperliquid.types | SpotMetaAndAssetCtxs | divergent | FORCED |
| hyperliquid.types | UserOpenOrder | divergent | FORCED |
| hyperliquid.types | UserOpenOrders | divergent | FORCED |
| hyperliquid.types | OrderRequest | divergent | FORCED |
| hyperliquid.types | OrderWire | divergent | FORCED |
| hyperliquid.types | CancelOrderRequest | divergent | FORCED |
| hyperliquid.types | CancelOrderRequests | divergent | FORCED |
| hyperliquid.types | CancelByCloidRequest | divergent | PORTED |
| hyperliquid.types | ModifyRequest | divergent | FORCED |
| hyperliquid.types | ModifyWire | divergent | FORCED |
| hyperliquid.types | ScheduleCancelAction | divergent | FORCED |
| hyperliquid.types | Signature | divergent | FORCED |
| hyperliquid.types | Notification | divergent | PORTED |
| hyperliquid.types | Candle | divergent | FORCED |
| hyperliquid.types | HyperliquidCandle | divergent | FORCED |
| hyperliquid.types | WsUserFill | divergent | FORCED |
| hyperliquid.types | WsUserFills | divergent | FORCED |
| hyperliquid.types | WsUserFundings | divergent | FORCED |
| hyperliquid.types | WsUserNonFundingLedgerUpdate | divergent | FORCED |
| hyperliquid.types | WsUserNonFundingLedgerUpdates | divergent | FORCED |
| hyperliquid.types | PerpDex | divergent | PORTED |
| hyperliquid.types | PerpDexs | divergent | PORTED |
| hyperliquid.types | Hip3MarketInfo | divergent | FORCED |
| hyperliquid.types | BuilderInfo | divergent | FORCED |
| hyperliquid.types | BuilderFeeApprovalResult | divergent | PORTED |
| hyperliquid.types | BuilderFeeCheckResult | divergent | FORCED |
| hyperliquidRetry | isHyperliquidRead | ported | PORTED |

Metadata-only reclassification outcome: venue 55 ported, 84 divergent (including
15 unresolved finding rows), 392 pending; branch report 331/6979. extract/check/
report/progress and diff-check were rerun and passed. Production code and fixtures
were unchanged; native/clippy/wasm gates were not rerun for this metadata-only change.
The prior 24-test baseline remains historical evidence, not proof of the findings.

## P2 measurement and repair (supersedes original signing inference)

Actual live TypeScript keccak input capture corrected the audit: undefined object
members encode as MessagePack nil, while JSON.stringify omits them. The old Rust
null substitution matched signing bytes but transmitted the wrong JSON. The original
F-WIRE-BYTES wording overstated the signing mismatch; omitting keys in one common
representation would instead create that mismatch.

Fixture capture 742f88195a precedes implementation. SigningPayload owns separate wire
and encoding Values; no process-global side table exists. Prepared order arrays keep
those views through action building and native signing. Serialize selects only wire;
SigningInput selects only encoding. A plain Value stays a plain Value, so replaying
serialized JSON reproduces TypeScript's different replay preimage rather than guessing
missing undefined metadata. Prepared actions must be signed before serialization;
RestRequest.body alone cannot retain non-JSON members and no end-to-end prepared
request API is claimed here. This is a preserved ts-bug, not a silent correction.

All three missing-field cases and the complete control compare exact ordered JSON,
actual TS preimage bytes, digest and signature. Retained reverse-order payload checks
pin independent request ownership. Collapsing encoding to wire fails101 at preimage;
serializing encoding fails101 at transmitted JSON, each tested in an isolated checkout.
The scoped repair reviewer returned MERGE after stale manifest text was corrected.
P2's null-root/missing-order_type diagnostic paths are closed by the wire-error repair
below. The entire branch remains DO NOT MERGE until other findings close.

## Wire diagnostic repair

F-WIRE-GUARDS is closed for null roots and missing/null order_type: the oracle-first access-errors corpus pins both TypeError/Error identity and message, rather than only successful wire values. WireError retains both fields. Prepared actions must sign before serialization; parsed Value follows TS replay semantics including the different missing-field hash. Other ledger findings remain open.

## Current taxonomy correction after crypto/core merges

The earlier FORCED accounting above is historical, not the current ruling.
All 86 divergent notes now carry a bucket: 18 CHOSEN and 68 FINDING; zero
FORCED or CORRECTED. The 53 overflow-ingress rows are instances of ONE open
conversion defect, not 53 independent impossibilities. A raw-number deserializer
can place Infinity in the existing f64 field, so the old concrete-ingress argument
did not establish that mirroring was worse. Those rows are now FINDING
F-NUMERIC-INGRESS, pinned by the existing overflow witness, and remain unfixed.

Clock injection, lazy-future ownership and raw-key/Result signing interfaces are
CHOSEN; an eager host task or a wallet callback/structured error can express the
alternative. Each note names why the chosen boundary avoids duplicated host
ownership. BTreeMap ordering, null acceptance and diagnostics remain findings.
The split-surrogate diagnostic replacement is also a local FINDING, not the
shared UTF-8 ingress refusal: the declaration explicitly does not cover local
formatting choices. The two applicable canonical contracts are
[shared-utf8-string-boundary](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary)
and [closed-typed-output-boundary](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary);
neither erases these independent defects or claims serialized convergence.

Measured venue status after the numeric-helper slice: 56 ported / 86 divergent / 389 pending.
This correction is an audit of claims, not completion of the exchange or WS.

## Current pure-history slice

58 ported / 91 divergent / 382 pending, 531 total. Divergent buckets are 68 FINDING,
21 CHOSEN and 2 MIXED; no FORCED claims remain in current manifest notes. Historical
status/rank are ported; coercion/transition helpers cite the shared UTF-8 choice,
collapse additionally cites owned-snapshot-vs-live-alias with a live mutation witness,
and the mapper cites closed-typed-output-boundary alongside separate surrogate and
prototype-object witnesses. The historical counts and classifications above are
an audit trail, not the current manifest taxonomy. Exchange and WS remain pending.

Placement read slice adds two CHOSEN rows with native values/error/read-count witnesses and compiling precedence/early-read controls. The separate live-source string-boundary program retains valid JSON surrogate escapes and scalar/astral positives without rewriting the original corpus. These helpers are reads only; full placement, exchange construction and websocket composition remain incomplete.

Public subscription state adds one incomplete class FINDING and two MIXED methods: prepared/input-adapter representation choices plus unresolved concrete diagnostic sender/socket composition. Twenty-three actual-source cases pin ordered native subscription state, mutation-before-callback, failure retention, absent unsubscribe, synchronous reentry, and two independent input evaluations. NaN/Infinity/null and signed-zero collision cases assert native kind/bits before stringify. getOid/parse coercion repair remains cited at 41d69d6408; the roundtrip-symmetry finding and direct-native requirement remain in FIELD_IDENTITY_AUDIT.md, not re-derived here. Original Lighter feeder bytes remain untouched with later adoption at 74aa2775f1.

Subscription controls: moving insertion after the sender compiles and fails101 at mutation precedes sender callback; reusing the first evaluated value for the envelope compiles and fails101 at public subscription two evaluation count; replacing native NaN input with Null compiles and fails101 at pre-stringify native depth identity. Restoration passes the focused target. Source evaluations are fallible adapted input reads, not support for arbitrary native JS objects.

Reviewer correction at 2155719585: the initial unsubscribe test could accept Ok(None) when the source had a sender error, because both omitted a result. That earlier test must not be used as proof of unsubscribe error propagation. The corrected test asserts native Err iff the source has an error; swallowing the error at the actual public unsubscribe entrypoint compiles and fails101 at unsubscribe sender failure must propagate. Production propagation itself was unchanged.

Local post-PR public-topic slice: fixed sends bypass deduplication and preserve allMids/fastAssetCtxs/user-callback order. First/second-send and user-callback failures propagate with their specific message; callback-time state and synchronous nested subscription changes are checked. Five source cases come from a separate program so the earlier 23-case corpus is unchanged. This existing Rust operation is FINDING until concrete sender and user-topic/lifecycle composition; subscribeUserTopics itself remains pending with a finding because its Rust operation does not exist yet. PR7267 origin stays pinned at a670ccc8d1; these commits are local.

User-topic dispatch now has an existing Rust operation and moves from pending to MIXED: UTF-8 snapshot boundary plus incomplete registry/concrete subscription/lifecycle composition. Thirty-two actual-source cases execute pinned lodash flatten and compare native Undefined/field presence before JSON omission. Scalar/astral and escaped lone-surrogate witnesses are separate; callback false never ends iteration, errors do, and dictionary mutation/reentry leaves the original owned key snapshot intact. The snapshot adapter claims no dictionary-write parity. No original artifact is rewritten; prior 23-case subscription and five-case public-topic programs are unchanged.


## BytesLike signing repair

The former portable message-only diagnostic choice is removed. The native helper
accepts ethers' uppercase prefix and preserves the original BytesLike failure
fields through action hashing, real Realm signing and native API preparation.
`byteslike_signing` uses NativeRuntime and the production imports/transport; its
accepted signatures and rejected native fields precede source comparison. The
API diagnostic regression now asserts the retained protocol rather than the old
bug. This does not extend the typed string adapter to arbitrary JS objects or
claim unrelated wallet/nonce/MessagePack error equivalence.
