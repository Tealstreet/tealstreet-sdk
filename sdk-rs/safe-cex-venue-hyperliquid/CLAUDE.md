The Realm iterator entrypoint uses its existing property/call/iterator-record
operations for every value. It honors iterator overrides independently of function
names and refuses primitive iterator results before reading next.

# Hyperliquid Rust venue

Behavioral authority: [Hyperliquid guidance](../../packages/safe-cex/src/exchanges/hyperliquid/CLAUDE.md).
The production constructor binds the class to shared BaseExchange, store, REST and
socket hosts. The native registry supplies options-aware transport, hosted signers
and a caller-owned limiter. Earlier continuation notes below are historical;
the production binding section records the current entrypoint and runtime choices.
`deriveClosedPositions` is separate from `fetchClosedPnl`. Future exchange work
must preserve the surgical store mutation in `fetchOrdersInternal` and the
explicit fire-and-forget `refreshOrders` wrapper, never generalize the exception.
All requests are POST; use the shared Hyperliquid read classifier, not method
classification. Signing consumes the P5 MessagePack/EIP-712 primitives.

**Pure API host boundary.** Read [README.md](README.md) before wiring a caller:
transport selection, one shared runtime limiter and optional telemetry are explicit
host injections, not automatic Axios construction. Query serialization is blocked on codex-f6xyia’s core HttpClientOptions optional
serializer with the SIGN-then-SERIALIZE-then-TRANSMIT adapter wrapper. Live raw-string TS oracles run during normal native cargo tests;
CI dependency resolution uses `SAFE_CEX_TYPESCRIPT_PATH` and the existing pinned
`SAFE_CEX_CRYPTO_ORACLE_DEPS`. Capture is exclusive; never change a golden to
make Rust pass. [GATE_EVIDENCE.md](GATE_EVIDENCE.md) records actual negative controls.

**Drop contract.** Canceling a limiter waiter removes only its ticket and must
wake the next waiter; retained pacing reservations prevent cancellation from
creating a burst. A real counting waker pins this: manual polling alone missed
an executor-hang defect. Pure API futures perform no exchange/store mutation.

**JS boundaries.** Use the shared `safe_cex_num::js_trim`, never Rust whitespace.
Order numeric fields refuse coercion because TS calls `toFixed` directly; weight
limits and BigInt nonces use their distinct JS coercions. JSON diagnostics use JS
number/key semantics and UTF-16 length. Lone-surrogate replacement is disclosed.

**Audit claims, not only divergences.** [CLAIM_AUDIT.md](CLAIM_AUDIT.md) records
known gaps that green source-resolution gates cannot detect. Do not use the old
blanket type note to infer numeric/null bounds for string-only or required-nullable
types; propagate callee diagnostic limits onto signing callers.

**Shared number spelling.** Use public `safe_cex_num::js_number_string` for numbers
in signed/compared payloads and diagnostics. Venue-local ryu wrappers were removed
when integration exposed this ABI. Preserve the caller's JSON null policy for NaN
and infinities; the helper returns numeric text, not JSON.

**Prepared signing views.** order_request_to_order_wire returns request-owned
SigningPayload, not a JSON-only Value. Pass prepared vectors directly to order-action
builders and sign the prepared action before serializing. TS msgpack includes nil for
undefined b/r while JSON omits those keys; capture proved that omitting keys from the
signing view would create a wrong signature. Plain serialized/reparsed Value follows
TS replay semantics. Never recover metadata through a global side table.

**Accepted input shapes.** Literal unions deserialize a String before private
serde-derived wire metadata. Records enter a map-only visitor before private
wire decoding. Canonical names appear only on those private companions; never
use remote=Self or publish a permissive decoder on the public type. Qualified
PublicType::deserialize probes pin the public call path. The source generator
preserves these guards; source-derived corpora cover all 14 literal unions and
77 records, including positional-array rejection. Public records and numeric unions capture JsNumberInput before their private wire
decoder, repairing bare serde and shared entry paths. Optional fields reject a
present null unless their declared type is nullable; required nullable fields
reject omission. The compiler-rejection corpus pins both independent axes.
Numeric parity compares native f64 bits before serialization on raw, slice,
owned Value, borrowed Value and the three shared entry functions. Never replace
this capture with Value-first rebuffering or bypass the private strict decoder.

`cloid::HyperliquidPlacementRecord` uses the same numeric capture and a private
remote schema behind `strict_decode::object`. Its optional `timeInForce` admits
absence and rejects present null. `tests/placement_record_ingress.rs` checks native
fields, presence and binary64 bits through public Serde and shared numeric entry
paths. It retains the [128-level capture budget](#record-capture-budget) and the
existing scalar-Unicode `String` boundary for `symbol`.

**History views and tables.** Use shared TableHit on the original SIDE/ORDER_TYPE
tables. SIDE uses truthiness and ORDER_TYPE nullish fallback; these are distinct.
Preserve source field coercion before refusing an inherited value at the closed
HistoricalOrder output. Collapse keeps first-seen order, replacing only the latest
transition by timestamp/rank, and returns an owned snapshot rather than a live
alias. The live-source oracle records that choice; see the three canonical
boundaries in the committed MANIFEST-SPEC, never claim serialized convergence.

**Exchange state composition.** HyperliquidExchange owns the shared BaseExchange;
its dyn privacy override reads vault/applicationId, not signer availability. Its
PlaceOpts is the shared PlaceOrderOpts, with no source venue extension. The
constructor is explicitly incomplete until API clients and sockets are attached.
TIF replacement deletes then appends, preserving eviction order at 500 entries;
cancellation expires strictly after 120,000 ms, not at the boundary. These state
helpers neither write the store nor dial/spawn. New notification-crossing handlers
may be implemented before the ABI lands, but their rows must remain FINDING and
every notifying path must be added to STORE_NOTIFICATION_AUDIT.md.

**Interface census.** All members-only interface inventories refuse heritage clauses before generation or sampling, including signing cancel records. The mandatory inherited-number control mutates read-only source reads and requires the named guard at all seven paths. Inherited field provenance requires an explicit future design; never silently enumerate only own members.

**Placement read helpers (in progress).** Price candidates preserve spot/perp source precedence; position TPSL checks zero amount and finite positive size before reading positions, and uses the first matching positive-contract position rather than aggregating. The source-first fixture records observable read order/counts; native effect tests now use a forwarding read observer backed by DefaultStore. Compiling precedence/early-read controls fail named assertions, and a separate live-source string program witnesses the shared UTF-8 refusal. The two rows are CHOSEN; full placement and constructor remain incomplete.

**Field identity and portable bits.** Never treat a serde roundtrip as native field identity: symmetric rename corruption can cancel itself. Identity supplements read decoded fields recursively before record serialization; the original feeder artifacts remain byte-identical. Supplements are self-authored and inherit no original independence claim. Float captures canonicalize NaN only; finite and signed-zero payloads remain exact. The architecture profile is separately labeled so original fixtures need not be rewritten. FIELD_IDENTITY_AUDIT.md distinguishes completed native checks from outstanding API/history/crypto consumer controls.

**Earlier parser coercion.** getOid uses the shared fallible String(any) operation. A resting-ID coercion failure propagates before considering filled fallback; no object-Object substitution or panic is permitted. HyperliquidOrderResponseError::Coercion retains source class and message. The single-process live-source earlier-response probe pins malformed owned toString fields and recursive array coercion; the old parser witness failed before repair.

**Notification cutover.** Production helpers remain read-only. The test-only Store observer forwards all 26 notifying results as StoreMutationError and retains StoreSubscriber unchanged; fixture mutations assert success. STORE_NOTIFICATION_AUDIT.md inventories the two fixture writes separately from production reachability.

**Public subscriptions.** The production class installs ws_emission's live-receiver
subscription bodies. They retain Set changes before sending, independent key and
envelope stringification, fixed-feed ordering, and candle/depth/BBO enumeration.
`finding_public_class` compares installed native arrows and state/effects against
actual TS; `binding` covers real book/candle subscription and cleanup frames.
The unused prepared-value adapter, its standalone tests and three source scripts
were removed. Historical row-disposition receipts describe their earlier snapshot.

**Corpus cardinality (in progress).** API field identity fixes four native cases; numeric ingress fixes53 root inventories,2380 source cases and16660 entry comparisons, with separate per-root totals. The scalar nonce target fixes15 cases and30 returned/stored bit comparisons. State tests fix533 cases across seven categories and563 native field comparisons. Placement tests fix30 cases across price/TPSL and60 native value/read-effect comparisons. Source-count assertions do not replace fixed native index coverage. Original source-first artifacts remain unchanged. Other targets and collection paths retain an open audit; do not infer whole-venue coverage from these controls.


**Multi-element supplement.** collection-identities.cjs preserves original feeders and compiler-checks260 expansions across33 array paths. The native field-identity target independently pins case/path counts and650 elements before whole-record comparison. Six constant-only cases assert cardinality, not order. A consuming truncate on a one-element input is a no-op and must never be described as evidence of assertion blindness.
Response checks read the explicit adapter model without serializing native data.
Encoded empty responses retain the existing Null fallback; non-JSON encoded text
retains its string value. This model read does not install a response parser.

**Filtered corpus bounds.** Signed-zero audit pins53 exact roots and238 cases per four routes; string boundaries pin49/147 and2/6 with independent per-root refusal splits. Fixed native walks remain mandatory when source-count guards are bypassed. Original feeders remain unchanged and broad cardinality coverage is not inferred from these targets.

**Accepted-set receipts.** Literal and record consumers bind every root input to the unchanged source fixture and invoke fresh AST regeneration once per test binary. Record checks retain all77 qualified positional calls plus seven fixed native receipts per root; literals retain distinct accepted, rejected and qualified-rejected receipts. Counters do not replace receipt assertions or source-content checks.

**Audit inventory.** CARDINALITY_AUDIT.md records bounded evidence and residual
consumers. ROW_DISPOSITIONS.json and audit-row-dispositions.py preserve an earlier
fixed manifest snapshot; they are historical receipts, not a current-master gate.
File-level witness inventory is not per-row proof; the broader sweep remains open.

**Survivor classification.** HyperliquidUserAbstractionMode empty string/null both fail actual strict declared-type assignability (2322); their values and diagnostic messages differ. The boolean rejection receipt legitimately cannot distinguish them. Keep that original survivor and its call site inventoried; the separate empty-string/unifiedAccount replacement crosses the accepted-set boundary and must fail the native rejection assertion with the content guard bypassed. literal-rejection-equivalence.cjs checks the source pair plus positive once per process; its permanent native test reads UnifiedAccount directly. Further old-row audit matrices are paused under the porting-first priority.

**Order inference.** order_inference.rs mirrors the pure source helper and preserves Limit-before-trigger, TP-before-SL, truthiness (including string zero) and the source-label choice based on any nonempty orderType. Coerce tpsl before entering even the Limit branch; poisoned toString must still throw. to_number_or_zero is parseFloat(String(nullish-to-empty)) with finite fallback, not Number. TIF is a strict Alo/Gtc test with no coercion. The native source/result types and predicate are ported; numeric/inference callers retain CHOSEN shared UTF-8 ingress witnessed by escaped source strings. Client/store/order-placement composition remains separate and incomplete.


**Builder/constants leaf.** constants.rs and spot_constants.rs mirror subscription arrays, API URL, builder wallets/tiers and asset/precision constants. Builder/rate validation is a paired closed-own-key lookup, with JS trim (BOM included, NEL excluded). Address inputs and HyperliquidBuilderConfig.address are UTF-16 code units; unpaired surrogates are preserved in configurations and correctly cannot match ASCII allowlist wallets. Fee rate rendering must retain ordered /10/100, including7's IEEE string artifact. resolve_hyperliquid_builder uses a fallible HyperliquidBuilderProvider for optional property reads: builder once, then testnet only on fallback; provider errors propagate untouched. Ordinary HyperliquidBuilderOptions has Infallible reads. Configured Cow::Borrowed preserves pointer identity, fallback is fresh Cow::Owned. This leaf does not attach approval/network/client workflows. Further old-row audits remain paused while pending rows are ported.

Spot table/order-schema slice: `spot_types` maps eight table exports and six
closed order declarations from `hyperliquidspot.types.ts`. Keep the spot WSS
URLs and 18 info types separate from perpetual tables. The six aliases reuse
strict core decoders only while `scripts/spot-types.cjs` verifies every actual
source declaration/transitive dependency matches; its 47 strict compiler cases
feed native variant/member/presence/number-bit assertions in `tests/spot_types.rs`.
Census after this slice: 99 ported / 106 divergent / 326 pending (531 rows).
Historical cardinality sweep remains OPEN; pending porting stays primary.

Approval leaf slice: `builder_fee` ports five approval constants with derived
default fee/rate; `agent` ports APPROVE_AGENT_TYPES, four-fresh-read name creation
and the host-bound window/ethereum predicate. Host bindings preserve getter
errors and JavaScript ToBoolean without a JSON/UTF-8 coercion. Browser wallet
creation, signing and submission remain pending. `spot_types::CancelOrderRequests`
reuses the identical numeric core decoder; `spot_types::ScheduleCancelAction`
uses Option<Option<f64>> to preserve absent/null/number because spot permits null
and perpetual does not. `approval_leaves.rs` reads native values/effects for
50name/18wallet/18cancellation source cases. Census109/106/316,531rows; sweepOPEN.


### Spot raw model continuation (integration parent ed7ac407c7)
Fifty spot type roots move pending->ported in spot_models.rs. Deterministic
spot-models-generate.cjs writes native DTOs/projections; independent
spot-models-oracle.cjs checks actual declarations and JSON.parse projections.
Raw JSON string/slice ingress uses spot_json::JsString UTF-16 units and f64
bits, including overflow and signed zero, without serde Value ingress claims.
All nested object records retain the last declared duplicate and ignore unknown
UTF-16 keys; local closed order adapters keep this rule at nested order callers.
Gate floor:400cases/50roots plus five independent witnesses, finalcase404.
Four dictionary/intersection spot roots remain pending; older sweep stays OPEN.
Venue census159ported/106divergent/266pending. Original13FINDINGs remain named.


### JSON boundary continuation (integration parent6ee57c1e6a)
15rows pending->ported:8json_models.rs declarations +7json_boundaries.rs leaves.
json_values.rs supplies owned UTF-16-key/string and binary64 raw JSON snapshots,
last duplicate selection before typed value decoding and JS integer-key order.
The shared `RetainedJsonInput` capture preserves surviving lone UTF-16 code units
like JSON.parse; its explicit UTF-8 Value projection refuses them. The duplicate
selection control in `tests/divergence_types.rs` compares actual retained units
against Node and separately requires projection refusal and syntax validation.
Any-indexed UserRateLimit/OrderStatus also accept arrays in strict TypeScript.
The helpers bind opaque JavaScript host primitive operations and preserve native
object/error identity, receivers and catch boundaries, including revoked proxies.
AST model writer is independent of strict71case oracle; actualsource215case
host oracle supplies primitive results/effects, with native assertions first.
Cross cache lifecycle/fast decoder and the historical sweep remain OPEN/pending.
Census174ported/106divergent/251pending; earlier named findings stay unchanged.


### Error/market continuation (parent852b13d6c0)
12pending rows ported:6core/spot Error class/constructor/handler roots in
api_errors.rs plus6market_helpers.rs scalar/fast-decoder/pure-tuple leaves.
Opaque host contracts retain Error/prototype/coercion/property and external
Unicode/ccxt/Precise/get-key/base64/pako/TextDecoder/JSON/iterator/log semantics.
Actual-source275case native gate ends274; receiver/effect/error/field assertions
plus decoder/tuple/code identity are separate from wire (no wire assertions).
Error code slot follows source ESNext define-field before assignment semantics.
Census186ported/106divergent/239pending; historical sweep OPEN and earlier
findings unchanged. Async startup fetches and live networking remain pending.

HTTP attempts retain the inner success receipt. Response URL/action classification,
telemetry and typed refusal provenance consume its effective configuration,
including inner replacements and omissions, rather than the outer input clone.

### Divergence audit continuation

The historical continuation censuses above are snapshots, not current backlog:
tomllib at e6a8ffaf13 measures425ported/106divergent/0pending across531rows.
parity/HYPERLIQUID_DIVERGENCE_RESOLUTION.md owns the complete review outcome.
Raw weight classification projects consumed fields before decoding unused data,
parses physical text once, and preserves fallible Number coercion. The separate
parse_request_data Value-returning adapter keeps its declared representation cost.
Response model reads and prepared signing wire canonicalization retain literal
reserved-looking JSON objects; split-surrogate caps
return a visible representation refusal before emitting any substituted payload.
Action type String conversion throws before unknown-action emission or wallet use.
Weight coercion retains JavaScript error name/message through preparation and
execution, before pacing or adapter invocation; capture bypass remains explicit.
SigningPayload authenticates native JsJsonValue numbers, including nonfinite
assets, while its wire view keeps JSON.stringify's null projection. Generic
array user actions use a request-local wallet property overlay for signing;
Value::Array cannot expose the two named post-call properties to the caller.

<a id="record-capture-budget"></a>

The typed core records' raw JSON capture retains the canonical128-level nesting-depth budget.
JSON.parse admits an unused140-deep extension; native capture refuses the whole
containing record/batch before field projection. This is a CHOSEN bound on
recursive work and stack use for the common typed decoder, independent of
selected String-field observations and the repaired discarded-duplicate bug.
The source supplement measures50 selected roots; the native depth control is
one shared-capture witness, not50 independently executed depth controls.

<a id="api-execution-contract"></a>

The legacy ApiClient deliberately accepts an explicit RestAdapter, error emitter,
shared limiter and typed options rather than constructing an Axios object or
owning browser regional/proxy selection. The host must install its regional and
proxy behavior and qs-repeat query renderer; this factory neither installs nor validates them.
The shared HttpClientOptions.params_serializer seam exists, but this audit does
not prove the concrete venue/host binding. This price preserves transport-owned
lifecycle instead of adding a second platform client inside the venue.
Optional telemetry is an injected callback, without exchange duck-method identity.
It receives current request data unchanged: encoded UTF-8 text remains a String,
while untransformed native objects retain their model shape.
Execution starts when its future is polled; disposal drops pending host work
rather than leaving an eager JS Promise task running. Native Result/String
errors preserve the modeled failure fields, not JS Error stack/object identity.

<a id="limiter-execution-contract"></a>

One host-created1200/60000 limiter Rc must be shared across clients/accounts;
there is no automatically constructed process singleton with a global clock.
Unpolled waits reserve nothing; a dropped started waiter keeps its budget and
releases only its queue ticket. This host-owned cancellation choice avoids
unowned background work after disposal, at the cost of source eager scheduling.
RateLimitClock uses u64 milliseconds: fractional delays truncate and infinity
saturates at the sleep boundary. This is not arbitrary JS timer-host equivalence.
Raw weight selection inspects consumed fields once; the separately exported
Value-returning parse adapter retains its whole-return representation cost.

<a id="signing-adapter-contract"></a>

Raw private keys and Result signatures use the shared native crypto primitives;
ethers Wallet/Promise/Error heap identity stays outside the venue. Invalid vault
BytesLike failures retain the installed ethers diagnostic fields in the canonical
owned callback cause, including TypeError classification and original value.
Generic array actions now sign with their indexed/length/prototype semantics and a request-local metadata overlay. Caller Value::Array does not
expose signatureChainId/hyperliquidChain after the call; a richer named-property
array ABI could retain them, but the request adapters keep their existing JSON
carrier. Object arguments retain the writes before wallet/projection failures.
Prepared input limit objects remain owned snapshots; their source alias price
is sdk-rs/parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias.

Opaque Encoded signing inspection retains the incoming provenance policy: source
String data skips signing, while native signable decoded JSON refuses without an
untransformed template; invalid wallet validation may fail first. Inspection can
emit an unexpected-action warning. Proven Transformed String data skips signing.
The typed telemetry callback maps a missing body to JSON null and owns its JSON
context; no source undefined/alias/error-object identity is asserted for that ABI.

## Production binding (`binding.rs`, `realm/`)

`HyperliquidExchange::new(options, store, HyperliquidDependencies)` builds the
shared BaseExchange with the required `HyperliquidDependencies.signers`, then runs the ported class constructor inside `realm`, the
crate-private production JS host for the generic class bodies. Every `Exchange`
trait method invokes the instance's ported method; six inherit the TS
BaseExchange `Not implemented` rejection (getAccount, fetchMarkets, fetchTickers,
fetchSymbolMaxLeverage, changePositionMode, fetchPublicTrades). Facades bind the
JS BaseExchange, BaseWebSocket (over the core socket), store and REST clients to
their Rust owners; imports bind by name to the existing ports, and an unbound
name throws a ReferenceError rather than resolving to a stand-in.
`tests/binding.rs` drives all 16 methods over a scripted host and asserts a
clean startup. Run `python3 sdk-rs/safe-cex-venue-hyperliquid/scripts/binding-controls.py`
from the repository root for reproducible dispatch controls: each of the 16
trait overrides is corrupted independently, must compile and fail its named
E2E test, and is restored byte-for-byte. The restored suite must pass all 14
tests. These controls establish dispatch coverage; they do not establish full
JavaScript semantics or live exchange parity. Logs stay outside the repository.

Startup's successful `applicationId`/`vault` assignments also patch the native
BaseExchange identity immediately. A host write observer preserves the realm
object's ordinary property descriptors; it runs after successful assignment and
outside the object's borrow. The scripted legacy-account test covers user,
vault, agent and subAccount roles against both native options and privacy checks.
Native identity strings retain the shared scalar-Unicode boundary.

The native emitter regression keeps shared Js arguments through its observations,
checks binary64 bits and listener reference identity, then compares the actual
shared JS stringify output with the maintained BaseExchange emitter/log source.
Serde's numeric token representation is not a JS wire-output assertion. Original
Realm object aliases still cross the existing DTO snapshot bridge; the approved
shared-Js consolidation owns its independent mutation-visibility regression.

Declared realm choices: promise reactions run as runtime tasks, not a V8
microtask queue; store reads are snapshots (writes go through Store methods);
the cross cache is per realm and memory-only (no BroadcastChannel or
sessionStorage on native hosts); `close` listeners are not invoked because the
core socket runs the close/reconnect path. External public data uses the shared
BaseExchange initialization and host-supplied provider factory, retaining the
same Store and provider disposal owner. The native host supplies its gateway
factory; custom embeddings must supply one when the option is configured.
Signing converts actions with `undefined` members retained, matching msgpack nil.

The production host supplies `ExchangeSigners` through the existing dependency
struct; construction attaches them to BaseExchange before realm assembly. Native
`can_sign` reflects a hosted signer even with no local secret, and availability
waiters subscribe to that same signer. HTTP relay signing is supplied separately
by the host's composed adapter, which must honor the API's skip-signing tag on info
requests. Every caller must pass the signer bundle explicitly; local callers use
`ExchangeSigners::default()`.


Date construction and Date.now apply the shared TimeClip operation to injected
wall-clock values. A fractional host clock must still produce an integer nonce;
`tests/binding.rs` checks local signing with a fractional input. NativeRuntime also
supplies integer epoch milliseconds, while monotonic clocks retain their precision.

The original 53-root/2380-case numeric fixture deliberately excludes WsUserEvent;
its complete intersection has separate finding_user_event/spot_runtime proofs.
The source generator keeps that scope even after the seed repaired the previously
fabricated user-only feeder row to valid funding plus user. Collection identity
pins the repaired feeder hash and reconstructs the old excluded row to verify
that every other feeder byte is unchanged. Neither numeric goldens nor native
production behavior are rewritten to reconcile this source-corpus boundary.


The production `fetch_historical_positions` trait method invokes the source
`fetchHistoricalPositions` arrow. Its realm superclass fold calls the existing
shared `build_records` engine through the source instance's income-range and
execution readers. Income mapper/pager imports use the existing `income` ports;
async REST callbacks must use `start_rest_callback`, never the synchronous
callback dispatcher. Default journal bounds use realm Date.now (integer ms) and
the source 30-day window, with shared 14-day straddle lookback. Returned journal
records retain the shared typed output boundary. This is distinct from the
venue's `deriveClosedPositions` and `fetchClosedPnl` capability.


### Standalone declaration cleanup

Unused standalone DTOs and their generated children are removed. Keep
`scripts/not-modeled-types.cjs` aligned with the core generators; the spot model
writer/oracle retain WsFill, WsUserFunding, WsLiquidation and WsNonUserCancel.
Five spot order aliases still resolve through `spot_types` to core models; deleting
their duplicate spot_models definitions does not make those rows not-applicable.
The current census and reduced fixed corpus counts are in README.md. Historical
controls for deleted owners are removed from active scripts and contract/schema
files; earlier audit documents retain their measured trees and original counts.

The unused `spot_rate_limiter` duplicate and its fake-host fixture/oracle are
removed. Spot limiter rows now name only the production `spot_client` bodies.
`realm::spot_limiter_tests` constructs the class with the concrete Realm host and
checks per-instance tokens, default weight, the exact minute refill and recursive
Promise/timer acquisition. Historical SPOT_LIMITER_BODIES_EVIDENCE.json records
the removed adapter; it does not establish production binding coverage.

The accepted request-weight contract includes cancelByCloid batches of 39, 40 and
80 for both raw and native JSON bodies. `rate_encoded` compares the actual TS
weights and the production ApiClient.prepare_request limiter reservations. The
cloid-weight binding control deletes each classifier arm independently, requires
a compiled pacing assertion failure, restores exact bytes and reruns the witness.

The realm Store patch bridge uses the existing ModelPatch carrier for own
undefined properties and native scalar numbers. Opaque info retains its JSON
boundary. REST order reconciliation must clear an undefined mapped timeInForce
once; repeating the same snapshot must not notify or advance the order revision.
The production binding regression covers this path, including cloid restoration.

The realm's imported `virtualClock.getCurrentTimeMs` reads the shared synced
singleton for both family constructors, matching TS. Realm `Date.now`, nonce
helpers that explicitly use it, and monotonic pacing keep their runtime clocks.
Registry `native_host_hyperliquid_clock` performs a real signed leverage update,
checks the transmitted action/signature, and requires its nonce to reflect a
90-second server correction. The binding schedule-cancel fixture uses a deadline
relative to that synced clock; the fractional Date nonce regression stays raw.

ApiClient.build_request uses the effective safeCexAxiosCreate timeout of20_000ms
before read-retry classification. extra.recvWindow remains account configuration;
it must not become a per-request timeout override. The native registry timeout
witness verifies terminal deadlines and signed HTTP; the Node info-read witness
requires a delayed real response to arrive before method acknowledgement.

The unused hyperliquidspot.api factory/interceptors and their fake HTTP fixtures
are removed. Source Spot construction imports the perp API, as does the native
binding; the Spot API module has only a TS test consumer and no package export.
Keep the separate RateLimiter class and its real Realm witness. spot_runtime
retains23 source cases across8 files after removing only7 API cases and1 source.

The public socket subscription bodies live only in ws_emission.rs and are installed
by public_class.rs on the production Realm. The unused ws_public prepared-value
copy and its standalone tests/oracle were removed; real listener coverage remains.

The concrete resolveHyperliquidBuilder import returns a configured native value
unchanged and samples testnet only for a nullish fallback. The actual builder
consumer keeps feeTenthsBps as supplied in the signed action; it lowercases the
address at the source callsite. Configured identity, lazy default getter reads,
fresh fallback objects and action fee types have real Realm/source witnesses.
No shared ExchangeOptions field types or Realm carriers are widened.


The production orderRequestToOrderWire import reads native Realm fields directly.
It formats price, size and trigger price before the HTTP JSON boundary, so NaN
and infinities become the maintained wire strings rather than null. floatToWire
uses the installed numeric/string operations, including source exponent-zero
stripping; order-type limit objects and undefined fields retain their identities.
The separate prepared signing API keeps its documented typed snapshot boundary.
The actual constructed Exchange placement regression and native/source import
matrix cover this adapter; no shared body or Realm carrier migration is implied.

The constructed native ticker census tests in `binding` and `spot_binding`
compare all Ticker numbers, including optional midPrice, with the maintained
constructed TS exchanges and real DefaultStores. Futures covers eight mids
and sixteen compressed fast-context frames; Spot covers eight mids frames.
Malformed input sits between distinct good rows, with finite Store resets.
The shared control runner misroutes midPrice independently in both live socket
bodies and requires compiled native failures followed by exact-restored passes.


## Legacy Realm Store hook failures

Exotic lookup and key hooks return Result; property reads, presence checks,
enumeration, spread, for-in and JSON propagate the original thrown value.
Callbacks run outside property borrows. Existing Realm objects may retain one
opaque canonical ExchangeError origin; native escape recovers it before reading
message/code/response, and property reflection or serialization never copies it.
The successful legacy DTO snapshot profile remains until the atomic family
Realm consolidation. Store table enumeration must read canonical map keys without
projecting unrelated rows; selected lookup projects only its captured row.
The native Store-backed poison-row witness checks zero getter reads during keys
and unrelated lookup, then one reached getter and its exact Projection cause.
The table_hook_tests target compares 13 real Node operations, checks native cause
identity/release and reentry, and scripts/realm-table-hook-controls.py proves both
lookup/key swallowing mutations fail before exact restoration passes.

The public Cloid, FundingHistory, UserOpenOrders and perpetual/Spot
CancelOrderRequests aliases execute their actual native decoder traits through
`scripts/wire-alias-contracts.mjs`. Authenticated compiler claims and root types
remain guarded. Observations read every member and list position directly,
including signed-zero bits; all five String conversion routes retain direct
unit/byte witnesses. This is wire-alias evidence, not a Realm/HTTP behavior claim.


The BytesLike signing regression runs the actual bound `signL1Action` on a
NativeRuntime/Realm and the native ApiClient preparation path. It compares five
valid vault byte/hash/signature profiles and three rejected vault envelopes with
the maintained TS and installed ethers. Both hex prefix cases and empty bytes
are accepted; short vault right-padding keeps the source behavior. The native
transport retains the same canonical cause through RestFailure and the Realm
REST rejection.
`tests/byteslike-signing-controls.py` requires compiled native failures for prefix
admission and dropped error metadata, then exact restoration and passing tests.

The four Spot `spot_models` records have separate map decoder contracts from
the perpetual wire records. `scripts/spot-decoder-contracts.mjs` runs their real
native owners and all four delegated OnlyMap visitors through the canonical
compiler inventory. Observations retain raw UTF-16 units and binary64 bits;
`scripts/spot-decoder-controls.py` corrupts each actual visitor and requires a
named compiled failure plus exact-source restoration. See
`SPOT_DECODER_REVIEW.md`; no Realm or Value-ingress equivalence follows from
these raw decoder receipts.

The cloid FIX audit has constructed exchange/cache and exported lookup probes
in `tests/support/cloid_audit.rs`. `FIX_AUDIT_CLOID.md` records the two bounded
ACCEPT decisions: native pattern representation and nonboolean record inputs
outside the typed placement producer. `scripts/cloid-audit-controls.py` requires
compiled production mutations to fail those consumers before exact restoration.

The L1 signer preserves RangeError for fractional numeric nonce failures before
the owned cause crosses the production Promise/API boundaries.
`binding::signing_audit` compares native signature fields and Error inheritance
with the complete maintained signing module on both networks. Preserve the
independent BytesLike probes; `FIX_AUDIT_SIGNING.md` states the finite input scope
and keeps the typed raw-key Result adapter distinct from arbitrary wallet hooks.

The unreadable order-response class audit uses the actual constructed binding,
its original parser and placement catch branch. Preserve the custom Error
constructor/prototype identity and the unreadable-only reconciliation branch.
`FIX_AUDIT_ORDER_ERROR.md` records four native constructor profiles, fourteen
source/native consumer profiles and three compiled semantic controls. The
portable typed Result adapter is distinct from the production exception object.


The signing interceptor supplies a fallible request-emission hook through
`prepare_request`/`execute`; the native factory returns its execution-local
original listener error before transport. See `FIX_AUDIT_API_SIGNING.md`.
