# Bybit venue port (in progress)

The fill-history source witness compares diagnostic messages separately from
the console severity argument. Its native observer verifies both argument count
and severity before comparing all 45 maintained-source request/result/log cases.

The 19 concrete SessionHost, SnapshotHost and AccountRefreshHost receiver rows
are pinned to their full production impl items. The native `receiver_tests`
construct primary/demo exchanges and observe cells, UTF-16, signed zero,
captured methods, one-level flattening, callbacks and real Store updates.
Run `cargo test -p safe-cex-venue-bybit --lib receiver_tests --jobs 1` from `sdk-rs`.
`scripts/receiver-policy.mjs <fresh-rustdoc-json>` proves all19 missing-policy
guard failures before restoring the unchanged guarded compiler inventory.
`scripts/receiver-controls.py` compiles getter-loss and strong-owner faults,
requires their named native failures, then restores exact bytes and replays.
Receiver classification leaves package alias/decoder measurement separate.

Behavioral reference: [TypeScript venue guidance](../../packages/safe-cex/src/exchanges/bybit/CLAUDE.md).

The first checkpoint maps tables, declared types, API predicates, site routing
and response normalization. Position snapshot reliability selection is extracted
into exchange_state and compared to live class methods, including placeholder
rows, exact ties, first-seen ordering and JS coercion. api_factory now composes
the request/response, retry and probe lifecycle with witnessed divergences.
Exchange/WS composition and concrete factory binding remain incomplete.
Variants stay inside this crate. Exchange work consumes the shared BaseExchange contract. Venue-specific BBO/TickSize options use the Plain/Bbo/TickSize representation.
The merged core associated PlaceOpts contract retains complete options through
preflight; the venue placement implementation remains in progress. Do not bypass
preflight or carry validated options in a side channel. Numeric coercion and
request parameter spelling use safe_cex_num::js_number_string, never Rust Display.

`yarn venue-oracle:bybit` runs every live TypeScript
oracle via Node and the workspace TypeScript dependency. It compares all runtime
exports of both table files and the rate table, plus origin normalization over
all non-surrogate BMP code points. JavaScript trim excludes U+0085: do not replace
the shared `safe_cex_num::js_trim` shim with Rust `is_whitespace`. Origin and site helpers consume Rust Unicode strings. Response messages and
clock/signature predicates retain UTF-16 code units, including lone surrogates.

Read the TS symbolType table before instrument mapping: empty and innovation
both mean crypto; marketRegion is not an instrument-class substitute.

The response oracle is included in the same mandatory command. It evaluates
bybit.response.ts directly, including fallback precedence, raw JSON proxy bodies,
JavaScript numeric-string syntax and signed zero. Radix strings must round once
to binary64; accumulating rounded floating-point digits disagrees with Number().


Type-shape fixtures are generated from live TS declarations, including every
union member and optional omission. Rate-limit numeric fields use f64, not an
integer restriction absent from TypeScript. Simulator NEXT_PUBLIC settings are
Cargo build-time values; the live oracle tests both empty and configured builds.

Raw proxy envelopes and response messages retain UTF-16, including lone
surrogates. BybitResponseMeta stores message code units with strict string Serde;
normalization and retry predicates preserve them. Native Errors reuse the shared
retained-cause projection through the typed ExchangeError diagnostic view.

Live Node tests require the frozen root dependency installation (including qs
and Axios). CI runs the command above in the `types` job after that installation;
the wiring audit pins both the command and its order. The native matrix also
discovers this crate, but its ordinary `cargo test --locked` ignores these Node
tests. Manual Cargo invocation must include `-- --include-ignored`.
This is root-installed execution, not proof of isolated runtime dependencies:
`SAFE_CEX_TYPESCRIPT_PATH` selects a compiler without isolating npm resolution.
If the native job starts running these ignored tests, its isolated dependency
selectors must cover the runtime packages too.

The private api_request module supplies api_factory's request interceptor.
Its former temporary dead-code expectations are removed because the factory
now calls it; concrete exchange-constructor binding remains separate. Preserve both exact auth
messages including the em dash, and keep absent statusText distinct from a reason
phrase; RestResponse.reason now carries direct-path statusText; api_factory consumes it before venue response classification.

Header sanitation has two stages in locked Axios 1.20.0. Bybit's explicit
AxiosHeaders construction cleans C0/DEL and SP/TAB before BOTH adapters. The
Node HTTP adapter additionally removes characters outside ByteString; the HOW
header projection preserves them. Do not move that direct-only filter into the
shared helper. The live test pins signed/KLINE branches and loopback HTTP versus
the source HOW projection; Rust direct transport integration remains pending.

Submission diagnostics live in exchange_errors: keep payload and fallback message
separate when composing the HTTP client. The live-source oracle compares complete
strings, including first-non-null field selection (a blank retMsg blocks later
message fields), HyroTrader remediation and JS trimming/Number spelling.
Constructor/submission remain pending implementation. Core ed304ce751 unblocked
them: RestFailure retains response separately from message, and HttpClient
implements RestAdapter directly. Do not flatten that payload or smuggle it
through side-channel state.

The private exchange module now composes the shared BaseExchange, primary/read-only
secondary clients, a separate unlimited client, and both core socket slots through
WsExchangeParent. Protocol factories receive Weak<BybitExchange>, so retained
sockets cannot keep the venue alive. The constructor starts no task or dial.
The live constructor oracle executes the source constructor, BaseExchange's
secondary-client method and createApiWithSecondaryKey; it replaces only the API,
rate-limit and socket constructors. Rust additionally exercises actual five-RPS
throttling and disposal. Ten scalar state declarations now match the source constructor, and the linear
accountType and supportsTradeDerivedCandles getters are independently claimed.
Account type uses strict numeric equality, retaining raw status rather than
coercing a string or boolean. The fixture extracts the actual declarations and
compares numeric defaults by bits. Concrete API binding/public-protocol factories,
remaining state initializers and placement registration remain incomplete: this private composition is not a
completed constructor claim or a public factory. Preserve the core fallback brand
until placement handlers are actually implemented.
Its submit_create_order_requests path builds an Untransformed POST body and calls
the primary client through the sequential submission operation. It retains raw
RestResponse and RestFailure values; API response decoding and the final venue
diagnostic callback still need composition before submitCreateOrders is claimed.

Public/private socket protocol sends are extracted into ws_public/ws_private and
consume BaseWebSocket directly. Subscribe preserves Object.values ordering,
10-topic chunks and the final 10ms wait. Public dispatch searches the raw exact
`"topic":"` marker, then matches the full topic or its first dot-delimited
segment; it does NOT match arbitrary containment between handler keys. Callback
arrays are captured before dispatch but remain mutable: removing the current
callback skips its shifted successor, appends can run in the same dispatch, and
removing a map entry does not invalidate an already captured list.

Finite raw JSON numbers round once to binary64 before dispatch, retaining -0.
The onMessage row declares and source-pins the selected boundary's inherited
constructor-key, lone-surrogate and nonfinite-number differences. Do not replace
this parser with arbitrary-precision serde dispatch and call that JS parity.
These operations are not yet the full socket class: route selection, core-owned
start, auth/reconnect and subscription lifecycle integration follow separately.

The extracted connect operations call core start_websocket synchronously after
selecting every argument. The runtime can poll inline: never attach state after
starting on the assumption the dial remains pending. Private relay URLs already
carry ts-relay; do not append requestId. Runtime-only payload-signer and relay
flags are explicit inputs until constructor wiring lands. Remote WS auth consumes the landed shared signer request/response union through
BaseExchange.signers.remote_signer; it never projects WS into HTTP SigningConfig.
The concrete private protocol now wires auth through onOpen/reconnect. Its live
source test covers 1,150 serialized envelopes and auth frames, not equality of
in-memory nonfinite extraData: Rust raw JSON represents those expires as null.
The shared response type also narrows signature/error values to optional strings.
Those malformed-input boundaries remain unclaimed on the auth row.
Pending-drop and wrong-response-kind tests require no transmission, and a
compiled wrong-kind-as-empty-WS substitution fails that assertion.

Literal unions and records require an accepted-input oracle, including rejected
values. Private wire enums own canonical serde names; public enum decoders first
require strings. Private remote record companions sit behind map-required public
decoders. Never use remote=Self on the public type: its generated inherent
`deserialize` bypasses a strict trait implementation. Keep the public qualified
call regressions, canonical metadata inventory and exhaustive conversions.
The shared BybitSiteId object-tag widening is closed by the merged strict decoder.
The venue witness now requires rejection through the public qualified decoder
and inventories its canonical serde spellings through unknown_variant metadata.

The private order_mapping slice is not yet wired to either exchange class. Do
not unify spot and inverse semantics: spot synthetic stops copy parent amount;
inverse recomputes remaining coin units at each stop price and retains native
notional. Only inverse classifies opening triggers as Conditional. Spread-created
synthetic orders can inherit the parent's other trigger field; preserve it.
Malformed typed fields and inherited protocol-table keys remain unclaimed until
both sides of those representation boundaries are measured.

`inverse_position::map_position` is the extracted mapper with real Store lookup,
lazy injected clock and logging. Clone the selected ticker before invoking the
logger: a host callback may mutate Store. Its CHOSEN typed-model divergence
refuses non-string symbols and inherited side-table members that TS can retain;
numeric positionIdx 1/2 must still bypass side-key coercion. Constructor and
fetch orchestration do not follow from this mapper claim.

Private rest_positions owns no task or latch. Request rejection marks reinit and
returns the original generic error intact; venue rejection differs between
symbol-scoped and account-wide reads. Do not keep partial pages on the latter.
The mapper callback and client are explicit dependencies until class wiring;
the pagination oracle does not claim their composition has been tested.

Response field lookup in inverse_position, fill_event, order_mapping,
rest_positions and rest_fills now consumes shared get_key::v. The local adapter
only projects a mapped JSON value and explicitly refuses inherited values;
current callers use fixed source field names, not prototype names. It performs
no casing or alias search itself. Preserve exact/snake/camel precedence and
present-null hits. In particular positionIM has a third spelling, positionIm,
which the former two-key helper missed. The 496-case inverse-position oracle
pins that spelling, null shadowing and exact-key precedence. Order table lookup
also consumes shared TableHit; the closed Order fields still refuse inherited
results rather than taking the missing-key branch.

Inverse position side lookup consumes shared `js_table_lookup` and the existing
POSITION_SIDE table. Numeric positionIdx 1/2 still bypasses side coercion. Missing
uses Long; Inherited must not fall through to that default. The function-valued
output refusal is CHOSEN under MANIFEST-SPEC.md#closed-typed-output-boundary.
This mapper refuses the whole record, unlike TS serialization which retains the
record and omits only the function-valued side. __proto__ instead produces an
object, serialized as side: {}; its refusal remains a separate FINDING. TableHit
does not remove either output difference.

The private submission operation preserves sequential request order and retains
successful response/payload pairs after individual failures. Its reporter receives
the complete RestFailure; response data and fallback message must remain separate.
A reporting error aborts the series, and dropping a pending request never starts
the next payload. This is not yet wired through the exchange constructor or HTTP
response decoding, so submitCreateOrders remains pending. The live source gate
covers all 127 failure masks for zero through six payloads; stopping at the first
HTTP failure is a compiled negative control, not the source behavior.

Core options are read through snapshot-returning accessors. Socket routing and
constructor client creation consume those snapshots without retaining a borrow
across a callback or await. This accessor migration does not claim equivalence
to the source caller-owned options alias; that boundary belongs to the shared
`owned-snapshot-vs-live-alias` declaration.

The private RequestClient composes the venue interceptor with the real shared
HttpClient through execute_with_interceptor: core assembles all defaults before
the venue callback, so KLINE clears them permanently. Query serialization and body
transformation follow signing, and preflight refusal uses the common
HTTP failure path without starting request statistics. Signing JSON delegates to
shared js_json_stringify. Its live Axios dispatch comparison covers omitted,
object, JSON-looking string and plain-string bodies. api_factory's separate
witness covers response, clock-retry and environment-probe composition.
Integration 91db928805 closes the core Content-Type insertion finding for an
object body after KLINE clears the headers; the comparison now requires equality.
Ticker event updates consume native ModelPatch numbers and propagate Store errors.
NaN and both infinities now overwrite the prior value exactly as in TypeScript;
the inherited-symbol membership difference remains separate.

The full header comparison includes Axios Accept without adjustments. The shared
execute_with_interceptor operation closes the late-default seam: neither factory
nor core defaults are restored after KLINE clears them. Interceptor refusal goes
through the core failure pipeline exactly once, before request statistics.
api_factory composes response, retry and probe handling on that request path.

Closed numeric records use shared numeric_ingress whole-entry functions and
JsNumberInput inside their map-required private wire decoders. Registration is
an explicit JsNumberOutput impl with exhaustive js_number_output! expression.
Use js_from_str/js_from_value/js_from_value_ref before serde can lose an existing
Value's integer-lexeme negative zero. The 297-case, 33-path live Node corpus
compares bits on raw, owned and borrowed entries, including nested raw rate
records, native -0.0, overflow and negative underflow.
BybitResponseMeta declares raw_response as named passthrough and captures it
with shared js_deserialize_passthrough before constructing Value. Its retCode and
nested raw-response bits now pass the same whole-entry guard; no bypass is used. Placement now uses JsNumberInput before flattening and serializes native
numbers without a Value buffer. Its explicit output eligibility exhaustively
destructures all three variants and every embedded PlaceOrderOpts field, then
checks the numeric fields through the expression macro. A new shared field must
fail compilation for review; no foreign trait implementation is needed. All nine
placement numeric paths now pass raw, shared-raw, owned and borrowed bit checks.
Standard serde on an already-Value input can erase lexical -0 before a custom
decoder runs: use the shared whole-entry API at that boundary.


Private send_reconnect_sequence invokes the clock operation before returning to
its caller, then registers only the async tail with start_message_operation.
Do not add an auth latch: this source method has none. Auth rejection logs the
original message, disregards its own silence and reconnects without subscribing
or pinging. Success subscribes then pings and starts the private snapshot only
in reconnecting mode. The extracted operation retains explicit clock and auth callbacks; the concrete
private protocol below binds auth to the actual exchange. Cancellation is CHOSEN: the source
promise continues after disposal, while core owns and cancels both this tail and
the unawaited snapshot. Do not replace either with a venue-owned spawn.


BybitExchange::with_private_protocol constructs the real private WsProtocol with
Weak<BybitExchange>, shared WsRuntime and explicit clock/account-name inputs.
Its open callback starts send_reconnect_sequence immediately and returns a ready
future: awaiting auth there would change the source callback contract. After the
clock and delay, auth uses the venue's actual BaseExchange signer union or local
secret. There is no separate WS signer. The binding owns no task handles and
keeps the venue weak. Private onMessage, the public protocol, concrete HTTP
factory binding and remaining class state still need implementation; this is not a
finished exchange constructor. Generic constructor injection stays available
for variant-specific protocols and existing source constructor tests.

The four concrete exchange URL selectors are now source-pinned independently
of constructor completion. REST and private WS select demo by name OR is_demo;
public WS always uses ordinary URLs. The private protocol consumes the private
getter while retaining is_demo itself for connection-option routing. Closed
Network/PublicCategory inputs mirror source literal unions, so these table
keys are not venue-controlled. The oracle extracts actual methods and tables,
covering 32 combinations including a bybitdemo name with is_demo false.

Private auth is now separately claimed as MIXED divergent. Its shared response
decoder refuses malformed non-string signature/error fields and non-object
responses that the source method consumes. The boundary corpus retains null
and scalar Unicode positives plus a double-escaped lone-surrogate refusal.
Nonfinite expires is measured at both native signer boundaries before any JSON
projection: TypeScript retains the number, Rust RawValue holds null. Serialized
null agreement does not close that finding. Auth's send also crosses the shared
notification finding on the no-socket reconnect path. These findings do not
prevent the already-tested concrete onOpen/local/remote composition from being
claimed; they prevent a ported label. Constructor and onMessage remain separate.

Execution-query splitting now consumes the source seven-day range constants and
compares endpoint bits before serialization. Preserve its strict loop guard:
a range [0,604800000] yields [1,604800000] only, omitting the terminal singleton
[0,0]. The row marks this source defect; do not repair it by adding a final
window. Full fill-fetch orchestration remains independently pending.

Store mutation error conversions must name their current error type explicitly.
The ticker handler binds StoreMutationError and retains it in TickerEventError::Store,
including subscriber identity and cause. Socket notification results propagate
through core ExchangeError without a venue string conversion, retaining Store-local
subscriber IDs inner-to-outer. Synchronous reconnect rejection timing remains
a separate finding. See STORE_NOTIFICATION_AUDIT.md.

The concrete linear order hooks now mirror source: requiresOrderLinkId is false,
and Exchange::validate_order_capabilities is intentionally empty even for BBO
and TickSize options. Shared preflight and derived-venue policy are separate; do
not insert their restrictions into this hook. These two method claims do not
claim generated IDs, placement registration or the complete constructor.

Inverse position fetch composition distinguishes source errors from closed-output
refusals. map_position_deferred evaluates the remaining numeric expressions,
logging and clock before returning a representation outcome. The fetch helper
keeps that outcome through every later row and symbol request; a later source
throw or HTTP rejection wins. Only a source-successful whole operation becomes a
recoverable representation refusal. Do not replace this with map(row)? or refuse
at a per-symbol boundary: either changes error identity and can skip HTTP sends.
The source oracle composes the actual mapper, pagination and reinit methods using
p-iteration's real mapSeries, and captures inherited types before JSON erases
functions. Constructor and outer category fanout remain independent work.

The concrete reinit_on_throw operation preserves the original request result,
sets needs_reinit on rejection, and never clears a prior reinit flag on success.
It owns no task and borrows the exchange only while its future exists. Dropping
that future releases its request without late mutation, unlike the source's
unawaited promise tail; this is a declared cancellation choice, not source parity.

AST field inventories refuse interface heritage before reading own members.
check-shapes covers nested referenced records and supplies the numeric-ingress
corpus; check-unions guards its explicit interface-field lookup. Each entrypoint
runs a permanent inherited-required-field refusal control. Removing either guard
must fail that control rather than silently producing a subset of fields. Named
class-method extraction is separate from a whole-interface field census.

Field-identity gates must inspect decoded native fields before serialization:
a symmetric serde rename can survive even a distinct-valued roundtrip. The
shape generator retains its original 269 cases and labels an identity supplement;
its pair audit includes nested arrays and runs before the numeric feeder. See
FIELD_IDENTITY_AUDIT.md for consumer scope, controls and remaining boundaries.
Execution-window captures canonicalize only NaN payloads, never signed zero or
finite bits; live bit fixtures record their capture architecture.

Inverse fetchPositions now composes category/settlement fanout with deferred
position mappings, finalizing typed refusals only after every successful batch.
Nonempty symbols bypass both getters; the concrete inverse defaults are inverse
and an omitted settleCoin. Expanded category cases are separately labeled
algorithm probes, not claims that the concrete source supports other categories.
Later source errors keep their identity and precede any representation refusal;
dropping the future releases its pending request without starting another batch.


Reconnect timing witnesses compose actual source callers, not just base send.
Public subscribe continues both chunks/delays after unawaited reconnect rejection;
private subscribe and diagnostics still emit. Rust returns the failure immediately
and skips those later effects. Keep this FINDING separate from repaired ID retention.
Timer rows have unimplemented FINDING notes and remain pending: parity requires
a real Rust target for divergent status. Core owns individual task cancellation.


The concrete linear category selector ignores symbol spelling. Ticker category
selection reads current Store markets, filters by quote only, preserves duplicates
and order, then delegates active-symbol fallback to BaseExchange. Do not infer
category from a symbol suffix, active flag or isInverse. Its source corpus pins
those disagreements and a lone-surrogate refusal at typed Market ingress.


Market-step formatters require fractional cases whose adjusted outputs differ
under price and amount precision, including every bracket field. The formatter
gate retains 548 original cases and adds 32 labeled precision cases; inspect
native payload fields before serialization. Its stopLoss-only swap survived the
original target, whereas four other swaps were caught by the then-current corpus
which already included identity-sweep additions; no never-blind claim follows.

The linear settlement selector returns USDT/USDC only for UNIFIED and USDT for
CONTRACT, via the existing strict numeric-status getter. Unknown categories,
including inverse, report before rejecting. A reporting failure must propagate
its original metadata instead of being replaced by the later category error.
The 112-case selector corpus executes the source getter and method, then checks
concrete Rust error listeners. Removing USDC fails settlement account coins case 0;
discarding the reporter Result fails settlement rejection identity case 3.

Cardinality gates require fixed source totals and independent native counts.
Candle/book state tests execute every input before comparing expected outcomes;
ticker pins its ordered eleven-field inventory. Constructor subloops use fixed
16/4/5 bounds rather than oracle lengths. Keep corpus guards separate from
comparison-count assertions; equal empty collections must not pass a gate.


Consuming collection controls run through scripts/check-consuming-collections.mjs.
The runner temporarily changes one private permissions/IP wire field at a time,
drains SeqAccess before returning an empty vector, then removes only that field's
native identity assertion to probe the independent outer comparison. It restores
both files byte-identically; run it only in the owner's otherwise idle worktree.
The ten fields use whole-array equality, not a truncating element walk. A second
failure proves redundant coverage, not an indispensable standalone length guard.

The concrete protective-side helper uses shared field lookup and Number coercion
before inspecting typed Store positions. Exactly one open matching position is
required; NaN and zero contracts are closed, infinities remain open. Its source
oracle asserts native sides, fixed totals, coercion messages and typed ingress
refusals separately; source getter traces do not establish native getter parity.

Linear candle validation is specialized to its always-true trade-derived-candle
getter: it does not inspect the interval or emit a diagnostic. Do not reuse this
no-op for a subclass overriding that getter; its rejection path is separate.
Its test pins the complete ordered input inventory and requires an executed
hostile-getter witness. Native coverage records successful method returns inside
the assertion, so removing the call cannot leave a passing loop counter.

Protective-side evidence pins all ordered orders and position lists independently,
including branch totals and invocation-attached native return coverage. Source
getter and surrogate witnesses must execute; candle validation has no production caller yet.

`cached_api_key_info` returns an owned clone with native numbers intact. Source
returns a live alias; the accessor row declares both mutation directions under
the shared snapshot boundary. Cache population and decoding remain separate.

Linear `generated_order_link_id` returns None because the concrete requirement
hook is false. Its oracle extracts both source methods and pins zero generator
calls; do not reuse this specialization for a derived venue requiring IDs.

Opposite-position size checks use signed contracts, strict nonzero and >= amount,
with some/any selection rather than exactly-one matching. Keep malformed typed
ingress separate from the numeric predicate; placement does not call this helper yet.

The public `journal_fold` adapter now retains native Js rows, dependency callbacks
and records through the actual primary/demo historical-position reader. Closed
rows join the historical-order hedge axis before grouping, and each quote resolves
from the live Store. The 595 source-case corpus compares native number bits and
callback traces, including inherited close reasons and optional context. The
real signed-HTTP binding tests cover both families and source variant refusals.

exchange_helpers ports the pure mode/cancel predicates and balance scalar/fold
helpers. Preserve strict string guards, whitespace-only Number zero versus
parseFloat absence, first finite zero, signed zero and nonfinite accumulated
sums. Wallet fallback is lazy; finite usdValue zero blocks later balances, and
used balances clamp with shared Math.max. Typed JSON coin records exclude the
source's unchecked null-with-empty-fields success. The actual inverse selectors
share order_format's existing implementation, including take-profit triggerBy /
comparison-price asymmetry. stringFromUnknown uses the existing exchange_errors
implementation. The focused live test is --test exchange_helpers; venue consumers
and constructors are separate pending rows.

api_factory composes the concrete request client, rejection retry, venue response
handling, clock reentry and one cached opposite-environment probe. It retains
returned request configuration and transformed body phase through retries, then
runs the venue response callback at each retry unwind. Clock reentry retains
consumed rejection retries rather than granting a new budget. The same VirtualClock
supplies signing and clock synchronization. Probe work belongs to BaseExchange's
shared-operation owner, while the API retains only a weak exchange reference.
The factory selects the shared Axios default response transform before common
HTTP hooks, including rejected responses; it never reparses after those hooks.
Owned options, original read-token protection and whole-attempt retry budgeting
are separately witnessed and classified in the manifest. The factory row is
divergent/FINDING for scalar astral params after its committed gates and controls;
GATE_EVIDENCE.md binds acceptance to its measured revision.

Every physical dispatch normalizes its method before the signing interceptor,
including an empty method defaulting to get after a signature rejection.
The rejection predicate still observes the returned config before normalization.

Tunnel construction forwards the configured region unchanged to the provider,
including empty and unknown strings. A provider returning no replacement keeps
the existing adapter, matching the maintained factory's optional installation.

Query params use the request interceptor's object spread before auth guards:
arrays and scalar strings contribute indexed own entries; other primitives
contribute none. Keep legacy and native query carriers through signing and
receipt projection. api_factory_params_astral pins the remaining scalar-param
refusal and actual source qs bytes as an input FINDING; see
[the string boundary](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary).

The native query profile explicitly selects `QueryKeyOrder::Preserve` for factory
signing and receipt replay. Shared renderer support for sorted keys must not
change Bybit's existing signed parameter order; render errors remain visible.


private_snapshot_refresh implements the maintained debounce and scheduling field
bodies using SnapshotHost's retained receiver and Promise handles. Its global
timeout uses the shared independent registry; cancel retains the cached result
and does not cancel started snapshot work. The helper remains dynamically
selected on the actual owner; Base snapshot orchestration is not substituted.
Class/constructor binding remains pending. This module does not establish the
mapped helper's concrete class binding.
The focused oracle is private_snapshot_refresh_native_source_oracle in the
core_snapshot_readers target; run with --include-ignored.


The local-order gate runner uses workspace formatting and locked workspace
build, alongside scoped native targets and diff checking. Clippy, wasm, wiring,
layers and routing run centrally on integration. The historical snapshot-refresh
runner still records its original three-gate policy; it is not the current local
delivery command.


local_order_reconciliation retains the captured local entry across both awaits,
uses the strict grace boundary and checks disposal after each fetch. The
primary local-write adapter computes remaining from the original filled amount;
its record-options initializer shares the identical Spot implementation. Bare
parseFloat lookup captures the raw callable before evaluating its argument, then
invokes it without a global receiver. Concrete class binding remains pending.


primary_startup is the BybitExchange.start body over SnapshotHost. It retains
cached public-only mode, the margin probe's then(ok, fail) attached at creation,
the eager cache launch, the optional inverse public socket connect and
post-market parser ordering. Its detached cache rejection reports the original
value instead of the source's empty continuation (CHOSEN, divergent row).
tests/primary_startup.rs compares 22 scenarios with the executed source member;
that report is the only native-only effect it removes before comparison.
The concrete Exchange binding now invokes this body through the shared
js_runtime receiver; its independent SnapshotHost witness remains separate.

The production binding now constructs linear and demo exchanges through
`BybitExchange::create` and runs startup, snapshots and order writes through the
shared js_runtime receiver. The Exchange trait's three inherited unsupported
methods preserve the source BaseExchange rejection. Amend handles virtual
brackets through the parent, free conditional orders through amend, and
position stops through trading-stop or cancel-and-replace. Cancels filter virtual
rows before category batching and retire already-gone orders. Demo reconciliation
reads executions to exhaustion and fails closed at the page limit.
`tests/exchange_binding.rs` drives the concrete trait over a scripted transport;
its live-source order-write comparison covers 92 amend/cancel cases. External
public-data startup also loads the account fee overlay and private margin probe;
the provider receives this exchange's BaseExchange and startup awaits its loaded
flags. The shared native typed-value bridge preserves nonfinite numbers and
signed zero before request formatting or result projection. Concrete regressions
cover leverage clamping and ticker results; restoring either former JSON bridge
makes its corresponding regression fail.

The Spot facade now uses the same explicit dependency carrier while installing
its own source members in `binding/members/spot.rs`. Startup keeps Spot's
sequential awaited operands, skips the primary fee overlay and key cache, and
subscribes the private socket only to orders. Markets are spot/1x, positions
are empty without a REST query, and unsupported leverage/mode methods keep
BaseExchange's exact rejection. Native generic host adapters call the shared
lodash import rather than implementing a venue-local `times` policy.
`tests/spot_binding.rs` exercises the trait, request category, caller-ID guard,
public/private startup, Brazil local cancellation and disposal. The compiled
`spot-binding-controls.py` runner disables 17 trait consumers one at a time;
every control must fail a runtime assertion and the final six tests must pass. A further identity-field control removes only the early isSpot initializer and must fail the typed Spot flag assertion.
The registry reuses its existing Bybit dependency/layers edge for Spot.

The concrete private protocol now delivers frames through `WsProtocol::on_message`
to `ws_private/messages.rs`. It calls the variant's live receiver mappers,
clears recently-placed snapshot grace on sighting, maintains one canonical
incremental-fill tracker per protocol, publishes terminal rows, and updates
positions/balance in source order. Primary/Demo filter categories by the account
switches. Dedicated Spot omits positions; dedicated Inverse preserves its
qty/value fill pricing and [position, position] Store update pairs. Spot, dedicated Inverse and HyroTrader constructors are bound; Hyro uses its
acknowledged private session and USDT-only reconciliation. Execution and wallet
topics are ignored in maintained TS and remain ignored here.
`tests/private_event_delivery.rs` feeds the actual core socket pump. Its source
oracle executes maintained handler, mapper, fill helper, selectors and Store
bodies for 48 fixed compositions (primary/Spot, four flag modes, six scenes). `scripts/private-delivery-controls.py` proves
six independent regressions fail and restores exact production bytes.

The Spot constructor now uses the shared factory's source-field initializer, so
its immutable typed base receives is_spot=true before publication. Its actual
socket pump delivers only order frames and maintains one incremental-fill cache;
private_event_delivery compares the actual Spot handler/mapOrder alongside the
primary source bodies. Valid-array source cases are distinct from malformed-frame
error coverage. The registry uses its existing Bybit host/dependency edge with the
caller-owned limiter, including local and secretless hosted HTTP signing.

The concrete public ticker lifecycle now composes the existing topic/recovery
and lifecycle ports in `ws_public/topics.rs`. Primary linear/inverse sockets
subscribe every eligible non-option market in their category; active-symbol
selection belongs to Spot and the separate dedicated Inverse class. Spot calls
its live receiver selector on each synchronization, retains the union of prior
and desired subscriptions, registers the active callback before dialing, and
replaces its own markets callback on each open. Market additions preserve source
duplicates; removing active symbols does not unsubscribe prior topics.
The canonical subscribe and Spot market-addition bodies execute the first send
and sleep factory eagerly, before their remaining Future tail is registered with
core ownership. Weak socket/venue captures avoid retaining the facade. Source
OHLCV/order-book methods and recovery-timer class wiring remain separate pending
rows. `public_topic_binding` has four concrete tests, including 24 maintained
source constructor/handler compositions with 360 snapshots and pending-tail
cancellation/weak-reference witnesses. `scripts/public-topic-controls.py` has six
compiled consumer controls; all must fail runtime assertions and restore exact
production bytes before the four positive tests pass.

Primary Bybit/Demo batch placement now installs the source `placeOrders`,
`placeBatchOrders`, `placeBatchOrdersForCategory`, `processBatch` and
`formatOrderForBatch` members. Validate the entire batch before writes, run
special orders before regular category groups, preserve TP/SL on every request,
and keep the original batch for individual fallback. Successful result indices
refer to the filtered formatter entries. The pure formatter receives the awaited
canonical 0/1/2 position index; other request numerics are source string fields.
Spot and inverse install the same native recent-order recorder: cache the order
and optionally publish Brazil REST fallback without primary reconciliation timers.
Spot previously inherited the primary recorder; its compiled master-red HTTP
probe regression now passes, with inverse and primary/demo timer controls.

The unused typed local_order_cache facade, its fake-host oracle and its mutation
runner are removed. Spot/inverse cache manifest rows name the live native
lifecycle handlers; the canonical PrimaryOrderState owns the actual cache.

Spot and dedicated Inverse keep their own `placeOrders` overrides.
Native promise tails belong to Scope and stop on disposal. The batch source
oracle executes the actual five member bodies with maintained helpers; the
physical transport tests separately cover BBO/options, fallback and disposal.

The primary/Demo leverage and position-mode receiver bodies retain `Js` Position
values through cached reads, fetched mapping and Store publication. Never pass
those native models through JSON: NaN current leverage must trigger a write,
and NaN/-0 fetched leverage must survive mode discovery during placement.
`exchange_binding` has native regressions and 32 actual-source compositions;
`scripts/native-position-controls.py` reinstates four projection faults and
requires runtime failures plus exact restoration. The older public JSON helper
APIs retain their separately scoped contracts. Primary not-modified responses
remain accepted; the dedicated Inverse source's stricter policy is separate.


The dedicated Inverse constructor selects `BybitInverseExchange` on the shared
retained receiver, with typed is_spot/is_demo both false before publication.
Its separate `publicInverseWebsocket` occupies PublicSecondary; startup dials
that socket and private only. Public active-topic selection and both change
callbacks retain the source union and duplicate additions. Disposal follows the
source super call, then secondary/private sockets, using weak ownership.

Inverse writes use integer USD contracts and retain coin `remaining` alongside
USD remainingNative/remainingNotional. Do not reuse primary/Demo sizing, generated
client IDs, reconciliation timers or options classification. Its single-write
filtered-response/original-payload index behavior is preserved. Position mode
changes intentionally send linear/USDT irrespective of the caller's symbol;
110043 leverage responses reject here, unlike primary/Demo. Position mappings
retain raw BTC PnL and native NaN/-0 leverage. Real private position events update
existing rows without inserting absent positions, through the shared native
ModelPatch number bridge. Execution and wallet topics remain source no-ops.

`tests/inverse_binding.rs` contains nine native facade/socket E2Es.
`tests/inverse_batch_binding.rs` compares 36 actual-source batch/cache
compositions, including four independently tagged nonfinite/signed-zero inputs.
The consumer control runner disables 17 trait methods, disposal and constructor
admission in turn; all 19 compile before failing assertions, then exact restored
bytes pass. The separate registry-entry removal control rejects both local and
hosted constructions before its restored native-host tests pass. Both runners
bound Cargo subprocesses and print their private log directories. History,
candle/book and recovery class wiring retain their separate pending rows;
these construction and trading witnesses do not claim their completion.


Per-request `axios-retry.retries` now crosses the retained receiver's typed HTTP
adapter into `api_factory`. The factory defaults to three retries, compares the
integer retry count against the request's numeric budget, and retains consumed
retries across clock reentry. Fractional budgets follow Axios's comparison rather
than integer truncation. Resolved signature envelopes still use the separate
one-shot response-interceptor resend, including when the rejection-retry budget
is zero; preserve this source distinction.

`native_request_retry_override_retains_source_budget` checks six independently
specified request profiles against the maintained API with real Axios and
axios-retry, plus two native clock-reentry profiles. The private adapter witness
checks zero and omitted namespaces. `scripts/request-retry-controls.py` restores
the fixed budget and dropped namespace independently; each must compile and fail
its named runtime assertion, then exact restored bytes must pass. Its subprocesses
have timeouts and private temporary logs. The retained-config supplement now
covers own `retries: undefined` and NaN through the constructed venue's actual TypedHttpHost, BybitApi and terminal
adapter. Both match the maintained Axios/axios-retry one-attempt outcome; native
NaN and own undefined are observed before any JSON projection. Missing retries
still defaults to three. Executable per-request retry callbacks and arbitrary
live Axios configuration objects remain outside the data-only typed profile.

The Exchange cancellation surface invokes the venue `cancelSymbolOrders` member,
so an empty local order cache still sends `/v5/order/cancel-all` through the
unlimited client. Base `cancelAllOrders` dispatches that override once per stored
symbol. Primary/Demo and dedicated Inverse retain the source category lookup;
Spot retains its existing spot-only body. Retirement consumes only `id` and
`symbol`, including cancellation snapshots, and reports resolved venue rejection
without throwing or retiring rows. `tests/cancel_all_binding.rs` exercises all
four production constructors and the real retained receiver/HTTP client path.

Retry-condition coverage uses the real HTTP factory in `tests/api_factory.rs`.
The dead `api::should_retry_bybit_request` copy and its fake-only optional-config
supplement were removed. Keep the factory attempt, HMAC, effective error-config
and retained-error assertions. `scripts/factory-retry-condition-control.py`
inverts the production instruments-info exclusion, requires a compiled attempt
count failure and restores exact source bytes before requiring a passing run.

The original-method retry guard is an approved safety residual. A rewritten
network-error config must not replay create/cancel/amend POST bodies; the source
retries those endpoints as signed GETs with the old body retained. The nine
native factory profiles and six maintained Axios rewrite/control profiles in
`api_factory` pin this distinction. The mutation script also removes this guard
and requires the named replay-count refusal before exact restoration.

The Exchange candle-read override reaches the concrete receiver for all four bound
families. `binding/members/ohlcv.rs` preserves primary validation/errors and the
dedicated Spot/Inverse optional list mapping over the shared date/tuple mapper.
`candle_read_binding` supplies four compiled master-fail controls and six HTTP
witnesses. `private_frame_binding` sends order/fill/position/balance-related frames
through each family's real private protocol and Store bridge.

The unused `inverse_socket_bodies` adapter and its fake realm/oracle are removed.
Inverse socket manifest rows point to the live `ws_private` and `ws_public`
bodies. The real inverse socket witness replays a cumulative fill frame and then
advances it, checking suppression, USD delta and inverse qty/value price.
`scripts/inverse-private-tracker-control.py` resets the production tracker per
frame, requires the duplicate-fill assertion to fail after compilation, restores
the exact source bytes and requires the same witness to pass. Historical socket
body evidence describes the removed adapter, not production coverage.

Primary placement preserves Lodash `times` count admission before materializing
lots. A zero market maximum produces Infinity and no payloads, as in TypeScript.
`tests/zero_order_max_binding.rs` drives both actual primary/demo placement paths
in bounded child processes so the pre-fix infinite loop fails safely.

The Spot binding installs its source `omit` import over native object-literal
requests, so later split lots drop SL/TP fields without mutating the first lot.
`spot_binding::split_spot_orders_omit_brackets_only_after_the_first_lot` proves the
pre-fix missing-import rejection and validates both signed placement payloads.

Private reopen snapshots now dispatch through the actual venue receiver for all four
families. Primary/Demo tick uses BaseExchange's real remote-signer gate, and omitted
fetchOrders arguments retain the source true default. `private_refresh_binding`
and the two periodic `exchange_binding` witnesses drive signed HTTP and Store
publication; the source single-attempt snapshot policy ignores the retry argument.

The Exchange order-book listener now selects the source public socket and installs
real snapshot/delta subscriptions: depth 200 for Spot and 1000 for derivatives.
Core-owned retry/resync timers and retained topic maps preserve delayed connection,
sequence-gap recovery, reconnect and cleanup. `public_stream_binding` exercises all
four families; the maintained source book oracle includes array-like level lists.

Closed-PnL reads now dispatch through Exchange for primary/demo and dedicated
inverse, preserving each source category/refusal policy; Spot keeps Base's empty
history. Native mapper/iterator values survive until typed output. The actual
signed-client tests cover windows, cursors, cap warnings, malformed rows and the
shared signing virtual clock; do not replace it with the runtime wall clock.

Bulk candle streaming now binds primary/demo and dedicated Inverse Exchange calls
to their live public socket handlers. The maintained Spot Exchange inherits the
base bulk no-op, although its socket declares a separate bulk member. Source
subscriptions retain topics across reopen, replace the generic kline handler and
send chunks of 50; delayed connections use core-owned retries. Callback metadata
uses the shared String search position implementation. The output interval is
ModelField<AnyTimeframe>, retaining monthly and own-undefined source values while
public OHLCV(S) request intervals retain their existing strict type.

Single-candle listeners now bind all four Exchange families to real kline/trade
handlers. The live mapper consumes native Js values and preserves source numeric
coercions. Primary/Demo own a shared per-socket 25-second silence watchdog and
10-second recovery cooldown over the maintained recovery operations. Reopen
rearms existing handlers, and final cleanup cancels the primary timer/retry.
Dedicated Spot/Inverse retain their delayed-retry and topic/stats cleanup quirks.
The sequence oracle replays 72 maintained source frames through actual bindings;
the wired kline mapper retains its 2172-outcome source corpus.

Income history and range reads now bind primary/demo and dedicated inverse to the
maintained transaction-log helpers. Spot retains inherited empty history. Keep
linear/inverse context order, source combined-category sort/limit rules, inclusive
windows and repeated IDs. The mapper stays native until the typed public output;
its ordinary lookup tables retain actual Object.prototype members.

Spot's missing-market intrinsic constructs the source message before the caller
wraps it in Error. Real placement witnesses cover ordinary limit and full-position
SL/TP routes. The REST/private-frame unknown stopOrderType witness retains all
three unlisted stop kinds as own Undefined through the ModelField Store bridge.

Spot trailing-order placement retains the source `Ticker <symbol> not found`
error when market or ticker data is missing. The SessionHost intrinsic builds
the source template in UTF-16 before the existing Error construction. The real
Spot binding witness seeds a market without tickers and verifies no order write.

Historical order reads now bind primary/demo to the native mapper and signed
history client. Dedicated Spot/Inverse keep Base's empty method. Keep ascending
inclusive windows, category order and first-seen ID deduplication before sorting
and limits. All three ledger mappers share native numeric coercion.

Historical fills now bind primary/demo and dedicated inverse through their real
receiver, signed clients and native execution/fallback mappers. Keep the primary
valid prefix on mapping errors, but discard a failed recovery accumulator. Source
primary short/extended recovery differs from dedicated inverse single recovery;
Spot inherits empty history. The full reader source corpus covers requests, results
and diagnostics; mapper source corpora retain their separate scalar contracts.

Primary/demo option reads now bind the chain, expiry and account-Greeks receiver
members, including the source account opt-in guard. Quotes stay native through
numeric coercion, duplicate-symbol joins and sorting against live Store markets.
Dedicated Spot/Inverse retain the base options refusals. The actual HTTP/source
comparison has 79 passing cases, including tied-right sorting through the
canonical shared String.prototype.localeCompare implementation.

## Native HyroTrader composition

`BybitExchange::create(Hyrotrader, ...)` selects its configured production or
simulator origins, source receiver overrides and private acknowledgement/reconciliation protocol.
The registry reuses the existing Bybit dependency edge. Missing build-time REST
configuration returns the source error before IO; public/private WS origins use
the existing build-time tables and REST-derived defaults. Never route Hyro to
Bybit or Demo as a configuration fallback.

`binding/members/hyro_recovery.rs` keeps native Js recovery groups, original
submission IDs and publication boundaries. Creates use the existing per-request
zero retry budget; the shared API factory remains unchanged. Complete realtime
and history absence permits reuse of an uncertain identity; failed/incomplete
lookups preserve uncertainty. Authoritative terminal recovery returns its ID
without recording a new open order. Concurrent identical lists share one
sequence, and constructor disposal clears pending state. The old uncalled
`failure_diagnostics` rejection class/predicate and their 18 fake-only cases
were retired; the live recovery owns the retained Error brand and RegExp body.
All other diagnostic source groups and controls remain.

Hyro inherits dynamic closed PnL, fill and order history plus the closed-row/order
join for journal positions. Its income reads retain the source empty capability brand. Candle snapshots
use authoritative kline volume, never subscribe to publicTrade, and reject
seconds through the source validator. Existing typed interval/model ingress
bounds remain separate from the runtime body claim.

`tests/hyro_binding.rs` drives the actual constructor/receiver, signed client,
core socket pump and Store, including hosted WS signing, stale ACK/frame guards,
malformed snapshots, revision races, generated IDs and order reconciliation.
`safe-cex-registry/tests/native_host_hyrotrader.rs` constructs through the real
native host and verifies the transmitted HTTP HMAC on loopback. Configure the
existing `NEXT_PUBLIC_HYROTRADER_SIMULATOR_*` Cargo build-time origins for the
configured witnesses; the ordinary test also checks unconfigured refusal.
Constructor and native-registry refusals were compiled on master before repairs;
the added source policy witnesses also compiled/fail before their own repairs.
These local fixtures do not claim live simulator-account acceptance.

Dynamic history capabilities delegate to the same live Exchange methods: fills,
income/range and closed PnL for primary/demo/inverse, historical orders for
primary/demo. Spot keeps the branded base handlers, as does dedicated Inverse
order history. `history_capability_binding` exercises the actual capability calls
and signed HTTP; direct trait coverage alone does not witness this dispatch path.

The production `SpotExchangeHost::destructure` consumes the shared JS iterator
protocol for candle tuples, interval splits and guarded balance arrays. It reads
only the requested elements, skips value reads for elisions, pads exhausted
iterators and closes bounded iteration. `native_host_bybit` proves string KLINE
tuples through the real registry and HTTP reader for all four account variants;
the maintained TS source check is imported by the Spot read oracle. Noniterable
rows remain failures. The old array-only adapter compiled but failed this actual
registry witness at `234567 is not iterable`.

HyroTrader inherits all three option-read methods but its optionsEnabled override
always returns false. Route those trait and dynamic calls through the inherited
receiver so they preserve the source options-disabled Error before HTTP, including
when the account option-market flag is true. hyro_binding compares all six paths
with maintained Bybit methods and the actual Hyro guard; spot/inverse family
routing remains independently defined.

API response diagnostics can invoke fallible error listeners. Preserve the original
name/message/code through the canonical owned callback transport cause and the
Bybit HTTP receiver; do not stringify this rejection or attach a replacement
request config. The owned-value capture profile explicitly refuses unsupported
code values. `native_host_bybit` drives this path through signed native HTTP for
all four account constructors, including NaN and own undefined inside code. The
response-boundaries source oracle executes maintained createAPI with actual
Axios/retry/qs and asserts the injected diagnostic throw is the identical final
rejection after one physical attempt.

Margin and position-mode control bodies forward the generic fetched batch without
inspecting it. The production binding carries Js directly into Store publication;
never serialize these intermediates through JSON, which erases native NaN and
infinities. Real HTTP control-switch regressions cover primary and demo accounts.

Public ticker callbacks retain their socket category: primary/demo inverse streams
map turnover24h to coin volume and volume24h to quote volume. Dedicated inverse
and Spot retain their own source layouts. Real socket regressions cover all four
account constructors and partial updates; the maintained-source ticker corpus
now covers both primary categories in 1,944 cases and 21,384 field comparisons.

Market id/symbol/base/quote remain strict strings under the approved row-admission policy.
Primary/demo and Spot defer typed string refusals until source mapping completes;
dedicated inverse admits strings after building its native rows. Each refused row
reports its identity and field values, while valid siblings survive fetch/refresh.
This intentionally differs from TS retaining malformed string fields. Fee joins compare primitive symbol values strictly; distinct
JSON objects/arrays never match by contents. Source numeric/property errors retain
priority over typed row refusals.

Primary/demo single-order routing rechecks truthiness after price/trigger rounding.
A conditional trigger rounded to zero rejects before placement; a zero-rounded
conditional price selects Market and omits price/TIF. Adjusted NaN also skips
trigger-direction ticker reads. Raw option gates still send
zero-rounded bracket/trigger strings, and plain limit orders retain price zero.
`adjusted_order_binding` covers both real class bindings and those payload controls.

Submission checks unknown outcome only for one payload carrying a caller client ID.
The real HTTP regression sends a rejected first split lot with a non-coercible
error code and verifies later lots still place; a single order without a caller
ID also skips classification, while a caller-ID control still reaches it.

The retained HTTP body carries lone UTF-16 surrogate units through the common
response hook. The API factory carries those units through response metadata and status
handling. Its native Error message remains retained through the canonical
ExchangeError origin; the physical response and effective request stay unchanged. The native factory observer records canonical
retained input, and its compiled control fails if the original response is lost.
Numeric fixture comparisons check exact binary64 bits and source JSON spelling,
rather than serde number lexemes.

The remaining generic callback dispatchers take closed per-module `Callback`
selectors. Native routing converts only the original fixed factory tags; all
matches are exhaustive. Their former unknown-callback intrinsic arms had no
production route and are deleted. Hyro socket disposal uses the existing concrete
PrivateProtocol hook followed by shared BaseWebSocket cleanup; the uncalled
generic dispose wrapper and its three synthetic-only cases are removed. Preserve
the actual constructor/socket/Store witnesses and the remaining439 session cases.

Hyro defaults to production account classification; only the exact optional
`extra.hyrotraderMode: "demo"` sets isDemo. Production uses configured PROD REST/WS
origins, or borrows configured simulator origins while remaining production.
Missing REST configuration reports the selected family and source hint. The
local REST order-lifecycle fallback stays enabled in both modes through the
actual protected-method receiver override, including the typed lifecycle callers.
The shared option is a closed demo|production enum with ordinary optional-field
and string-union admission; it does not add credential fallbacks.

`tests/hyro_mode_binding.rs` compares the maintained constructor/hooks with real
construction, signed HTTP dispatch, public/private socket dials and cancellation
Store retirement. Its serialized child builds exercise production REST-derived
WS, explicit WS overrides and simulator-backed production, each with absent,
explicit demo and explicit production modes. The ordinary unconfigured profile
checks the exact source refusal. All Cargo subprocesses use jobs1 and a timeout.

The top-level account `hyrotraderDemo` boolean maps to the Hyro mode before
presence-aware extra overrides, matching accountToExchangeOptions; other venues
keep their existing extra shape. The 24 source/native account cases cover absent,
false and true flags with absent, undefined, demo and production overrides.
The live metadata snapshot keeps Hyro disabled in normal/preview discovery while
enabledInDev remains true, matching the source rather than account-mode selection.
The snapshot URL component oracle explicitly selects demo and executes the new
source configuration methods; actual default production/fallback routing is covered
separately through the constructed binding matrix.

The constructed ticker census in `public_stream_binding` compares primary/demo
linear and inverse categories, dedicated inverse and Spot with their maintained
TS exchanges and DefaultStores. The configured Hyro constructor uses the same
shared live helper from `hyro_binding`. Across616 cases and1848 real frames,
all11 source numeric inputs vary independently beside distinct good rows, with
finite Store resets and native binary64 assertions. Spot's ignored funding time
and truthy rate-to-zero behavior remain source-equal. The shared control runner
must fail all four family witnesses and the configured Hyro witness under live
last/bid misrouting, then pass after exact source restoration.

The response factory and typed HTTP receiver retain canonical Input through status,
retry and mapper dispatch. Market rows and option instruments consume Input;
the mutable public-data graph keeps UTF-16 text and keys without changing array
or object allocation identity. Native market/position regressions cover opaque
surrogates, overflow and the reliability score's nonempty UTF-16 price test.

Retained market/option numeric readers forward the coercion error's message to
the existing TypeError wrapper. Formatting the whole JsCoercionError there adds
a second class prefix to public errors and best-effort option-refresh warnings.
The actual primary/demo and option-category regressions compare maintained source
messages while preserving the option Store fallback. The bounded
scripts/accepted-retained-info-contracts.mjs checks nine changed retained-input
function owners with fresh compiler APIs and native field/bit observations;
it does not classify unrelated Bybit package inventory debt.

The retMsg diagnostic witness crosses real signed native HTTP, production
response normalization, `emitAndLogError` and typed error rematerialization.
Position-mode failure extras also retain raw message units while the source
continues with its `oneway` placement fallback. Runtime report contexts remain
canonical retained inputs through the terminal reporting capability.

The ticker prototype witness in `tests/ticker_prototype_native.rs` passes raw
frames through constructed primary/demo/inverse/spot sockets on NativeRuntime.
It compares 192 maintained-source cases against the full TS DefaultStore:
stock prototype members, own shadows, dictionary prototype tickers, parse
failures, notifications and the real `undefined` ID target. The handler uses
Store's canonical lookup instead of the borrowed own-entry map. SDK timing and
onMessage handler-table prototype dispatch remain separate rows.

The three no-socket send/subscribe rejection-timing rows now have a real native
reconciliation witness in `tests/detached_reconnect_native.rs`: constructed
primary/demo exchanges, actual Store notification and native stderr reporting.
Its compiled core controls distinguish synchronous caller rejection from a
detached report and require both continuation and reporting. This closes stale
notification timing notes; it does not change the public SDK wrapper family.

Protective-side inference preserves the get-key primitive TypeError without
embedding the class name in its message. The 242-case maintained-source test
checks both the constructed native receiver and the typed helper; primitive
diagnostics are compared directly rather than normalized to a custom string.

The SDK invoke/invoke_unit and get_account wrappers call their installed member
before returning a waiter and await the same canonical Promise. Preserve this
boundary when changing the typed facade; do not add a second Promise driver.
DIVERGENCE_FIX_AUDIT.md records the seven audited rows and the remaining 30.
