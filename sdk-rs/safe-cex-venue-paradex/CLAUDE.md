# Paradex venue

Behavioral reference: [TypeScript venue guidance](../../packages/safe-cex/src/exchanges/paradex/CLAUDE.md).
The actual TypeScript source is the behavioral authority when prose differs.

`constants.rs` preserves `paradex.types.ts` own-entry order. Ordinary-object
lookups must use `safe_cex_num::js_table_lookup`, including inherited identities;
a direct slice search would change the behavior of `constructor`/`__proto__`.
`types.rs` represents declared response shapes, not the unchecked Axios runtime.
Optional properties distinguish missing from present null (null is refused).
Record decoders require objects; string unions refuse externally tagged objects.
Every public decoder captures numeric ingress before its private wire companion,
with explicit exhaustive `JsNumberOutput` registration. Keep numbers as f64 and
decimal strings as strings. No opaque passthrough fields occur in these types.

The live oracle in `tests/types_oracle.cjs` executes once per test process, with
its complete corpus shared across tests through `OnceLock`. Never spawn per case.
It executes the actual venue module and
shared enum declarations. Erased types have compiler-checked, executed identity
witnesses plus a fail-closed AST shape census; heritage is refused explicitly and
has a permanent inherited-field control. `tests/types.rs` observes decoded native
fields independently of serialization, across six ingress routes. Preserve
field-distinct fixture values, signed-zero bits, overflow and underflow cases.
Lone-surrogate restrictions cite the shared UTF-8 boundary in the manifests.

During porting, use `cargo check` or one scoped `--test` target. Reserve the full
crate suite and strict host/wasm clippy for merge-up readiness. Use `--jobs 1`
and never run two Cargo invocations concurrently. Oracle tests require `SAFE_CEX_TYPESCRIPT_PATH` pointing to the installed TypeScript package.
The typed-data signing lifecycle now maps to the venue adapter below; the
manifest retains the shared encoder limitations separately.

`python3.14 scripts/check_structure.py` reproduces the venue-only extractor
identity comparison, types-and-utilities parity gate, direct dependency layer audit and
real TOML counts. It states its limits explicitly; it is not a global gate.
See `GATE_EVIDENCE.md` for the mutation assertions and measured slice scope.

`income.rs` and `order_history.rs` mirror the two standalone mapper sources,
including each private numeric helper. Their input boundary is JSON-shaped
own-property maps and optional values. Capture raw JSON with retained numeric
ingress before constructing a Value; ordinary Value decoding can lose binary64
semantics. Observe shared domain outputs directly; do not assume numeric decoder registration.
`tests/utilities_oracle.cjs` invokes the actual source bodies once per corpus.
Native observers preserve absent properties, callback traces, coercion errors,
record order and every field. Preserve production-collapse cases and the labelled
distinct-fields supplement. Runtime capture headers record CPU architecture,
Node version/major and V8; canonicalize NaN only, never signed zero.

`exchange_helpers.rs` contains the source's three pricing helpers and private
TradingView interval conversion. Its oracle extracts exact AST bodies and executes
actual shared `safe-math.adjust`; the harness does not construct the exchange.
The interval regex accepts ASCII digits and six units only; source uppercase W
switch arm is unreachable. Preserve huge-number rounding and error text.
The helper target checks an exact corpus census, native bits and errors, with
architecture and Node-major capture headers and NaN-only canonicalization.

`account.rs` maps getPositionMode and getAccount as standalone async methods.
getAccount's injected capabilities are the existing xhr.get, template-literal error
coercion and log boundaries, not a second API or authentication layer.
The get capability returns the declared AccountResponse; unchecked malformed
Axios acceptance differs and is observed/cited in the row. Both synchronous
get failures and rejected futures reach catch. Stringification or logging can
replace the caught error; otherwise return the same generic E untouched.
The account corpus checks pointer identity and effects before and after manual
future settlement. A native account_number/address swap must fail even though
both input fields are strings. Complete exchange/API composition remains pending.

Before porting market precision derivation, IOC pricing, amendment or bracket
payloads, apply [the precision-axis audit](PRECISION_SWAP_AUDIT.md). The current
single-step helper corpus does not prove caller price-versus-amount selection.
Use fractional values whose derived results differ for each step pair, observe
native payload fields and prove every production axis mutation separately.

The [cardinality audit](CARDINALITY_AUDIT.md) pins source counts and completed
comparisons independently. Preserve exact declaration identities, per-case income
output counts and native Vec/tuple lengths. Whole-map/array equality already
detects production truncation; a source census alone cannot detect skipped loops.
New collection targets need both empty and subset production controls, and a
source-count bypass control that still fails independent native length equality.

`request_helpers.rs` preserves describeError's two JavaScript runtime capability
calls: JSON.stringify first, String only on throw. Successful undefined is None,
not a fallback trigger; a String failure replaces the original error unchanged.
Generic output retains host UTF-16 where needed. The source's comment claiming a
log line cannot replace its error is not a stronger contract than this body.
`signer_headers` borrows an opaque supplied object into a new config/array;
empty objects are truthy and post-call header mutations remain visible.
The 25+5 case witness executes both actual AST bodies in one Node process.
Host capabilities cover exotic toJSON/cycle/String effects explicitly; they are
not additional implementations of those JavaScript intrinsics.

`balance.rs` maps fetchBalance on retained JSON data. Poll SDK initialization
before reading JWT presence. Logging is fallible both inside try and inside
catch: failures in catch replace the original without another catch. Keep all
numeric alias precedence and parseFloat(String(value)) finite filtering. Used
falls back to Math.max(total-free,0); upnl remains zero. The JSON-data boundary
excludes executable getters/prototypes and UTF-16 lone-surrogate strings; the
actual-source witness records eager getter evaluation and surrogate logging.
The balance oracle reuses scalarRaw from utilities_oracle.cjs byte-for-byte,
executes one Node process, and holds GET settlement while draining cross-realm
initialization promise adoption. Native fields and completed counts are pinned.

Production `ParadexExchange` now overrides its stream and history trait hooks;
the constructed member dispatcher calls the existing account REST bodies instead
of leaving their allocated arrows unbound. Its fills/income/order capabilities
are branded, and the existing state-operation journal delegate uses the canonical
shared journal reader with symbol-scoped fills. The native adapter retains wrapped
income failures as native causes; it does not duplicate reconstruction arithmetic.
Socket registrations, REST candle polling and cleanup reuse their existing source
bodies. Book registration and cleanup are split to retain the captured Set safely
across the native callback lifetime, with no leaked host or second listener state.
Real regressions are the `real_trait_` cases in `tests/exchange_trait.rs`.

`history_reads.rs` maps fetchIncomeHistoryRange through one fallible callback and
await. Missing and empty incomeType bypass filtering. Shared owned records do not
represent JavaScript array/object aliases or own-undefined property reflection.
The 54-case actual-source witness reuses the utility numeric feeder and checks
55 outputs, per-case native lengths, every native field and completed comparisons.
Fixture numeric decoding is separate from production: the callback returns native
IncomeRecord values. Keep fetchIncomeHistory transport/auth as a separate row.
fetchTickers remains pending for the shared Ticker timestamp representation gap;
never drop the virtualClock value or add a venue-local sidecar.

`order_reads.rs` maps fetchOrders orchestration through xhr.get and mapOrder
capabilities. Bind the generic mapper capability to `order_mapper::map_order`
when composing the venue. GET supplies canonical `RetainedJsonInput` own-property data directly to the
order mapper; never JSON-project the response or row before capturing info. Preserve initialization before JWT observation, all four exact
open statuses and the filter-before-map ordering. Logging/coercion errors can
replace a caught error; all such capabilities are fallible. The live corpus pins
per-case input content independently in Rust, not merely the case count.

`order_maps.rs` supplies order type, status and time-in-force lookups. Keep inherited
stock prototype identity via shared TableHit; never silently turn a function or
Object.prototype into the default enum. `map_order` explicitly refuses inherited
values at Order's closed enum boundary. `map_order_type` accepts the six declared
OrderType variants; unchecked non-enum runtime arguments are outside that domain.
None for reduce_only means source undefined (default true), not runtime null.
The declared bool/string call boundary excludes unchecked nonboolean arguments.
Its oracle pins all 32 key contents and all three reduce-only variants separately,
observes 128 native results before any serialization comparisons, and distinguishes
__proto__ from the eleven callable prototype members. Preserve the UTF-8 witness.

`leverage_reads.rs` maps fetchLeverageSettings through existing GET, Map.set,
error coercion and log capabilities. Preserve stale entries, duplicate updates,
strictly positive leverage filtering and partial updates on a set failure.
The source logs and discards caught failures; coercion/log failures in catch
still propagate. Typed MarginResponse ingress refuses unchecked string leverage
and falsy configs, as observed in the actual-source witness. Its native state
assertions precede serialized effect traces; start/tick and API composition remain
separate pending rows.

`order_mapper.rs` maps canonical retained data into shared Order. Both the real
REST consumer and direct constructed mapOrder receiver retain unknown raw info
keys, own undefined, native binary64 values and UTF16 metadata without Value ingress. The
shared owned info copy differs from the source alias, and None differs from its
own undefined timeInForce property. Observe both boundaries in the oracle.
Required non-string identity fields and inherited prototype enum values fail
visibly. Ordinary unmapped side keys retain ModelField::Undefined, using shared
source property-key coercion before lookup. No enum is widened. Size and remaining_size use the shared String/parseFloat coercion,
including numeric inputs and NaN from missing or nonnumeric values. created_at
retains Date conversion: null/false become +0, true becomes 1, missing becomes
NaN, Numbers use TimeClip, and other JSON data is string-converted then parsed.
String dates retain the existing parser and timezone capability.
The constructed REST regression checks startup and Store order loading. Preserve boolean reduce_only priority over flags, trigger display-price
selection, unclamped filled subtraction and created_at dates. Shared js_date_parse
requires the host's local-to-UTC capability; the mapper corpus pins UTC only.
`public_ws_helpers.rs` maps channel naming independently of websocket lifecycle.

`batch_predicate.rs` reads only the eight native-batch eligibility fields; keep
undefined distinct from present zero/NaN. `subscription_delegates.rs` forwards
opaque host arguments/callbacks/results/errors once without encoding or awaiting.
`sdk_state.rs` retains opaque SDK field values and ordered fallible address reads;
its borrowed JWT cache slots clear references without owning or cancelling host
refresh work. Constructors, imports and complete JWT/websocket lifecycles remain
separate. The SDK structural scope includes the typed-data signing adapter row.

`sdk_reads.rs` retains SDK load/method/field/read/stringify order with opaque
host values and named request records. Method selection precedes field reads;
loss-factor reads never touch account/address/token. `lifecycle.rs` sets disposal
state before public then private callbacks and stops on an error.
`bracket_validation.rs` preserves numeric truthiness, inclusive invalid TP/SL
comparisons, TP-before-SL warning order and fallible logging. It does not validate
payload precision or implement signing.

`typed_data_signing.rs` owns `signParadexTypedData` property/call order, remote
signer routing and felt-pair formatting. Its host retains original JavaScript
method receivers, map/species behavior, coercions and promise settlement. Native
bindings use `safe-cex-crypto` hashing/signing; inherited typed-data encoder
refusals remain explicit in the manifest rather than claiming SDK-wide parity.


`native_exchange::ParadexExchange::new` now composes the body ports on the shared
`js_runtime`. Runtime, per-options HTTP factory and external public-data factory
are explicit constructor dependencies. See [BINDING_GATES.md](BINDING_GATES.md)
for constructed trait coverage, actual SDK signatures and compiled controls.
Preserve source fetchPositions balance writes and exchange disposal without base
cleanup. The required `local_wall_time_to_utc` dependency reaches both exchange
order-mapper callers. Embeddings supply their host timezone policy; the native
registry explicitly chooses UTC. The constructed UTC-7 regression compares REST
and stored dates with Node Date, checks explicit-Z/date-only bypass, and keeps
private order updates on their separate raw Date.now clock.
Only constructed private onOpen uses the approved shared authentication
recovery contract; direct authenticate keeps the original catch and log behavior.

Native `cancel_all_orders` and `cancel_symbol_orders` call the venue receiver
methods, whose DELETE /orders behavior also covers orders absent from Store.
Keep the symbol in the original params.market config so the shared HTTP adapter
owns query serialization. Both methods preserve the existing catch/log/rethrow
path; an empty Store must not turn a rejected venue cancellation into success.

The actual SDK quantum binding calls BigNumber.isFinite before the separate NaN
and sign checks, matching the source. Infinity/-Infinity are rejected before
signing or order POST; finite arbitrary-precision overflow remains accepted.
The constructed RequestSigner witness executes eight original source profiles
plus two finite1e400 supplements, and the public placement regression observes
zero order POSTs for infinite price. It uses the shared BigNumber facade and the
actual SDK constructor rather than the standalone injected signing callbacks.

The constructed undefined-side regression retains eight missing/unknown/null/
boolean/number/object/empty-array profiles in both returned and stored Orders,
plus BUY/SELL and an array key coercing to BUY. Native own-undefined presence is
checked directly; later Date errors still propagate. The 152-case pure source
corpus is unchanged. Its two historical unknown-side refusal labels now have
positive native ModelField assertions and all-field comparisons, with explicit
115 mapped / 37 refused / 2 recovered-side floors; prototype refusals remain.

Constructed REST order reads apply the final strict-field policy to id/market:
finish source numeric/Date work, report the offending field and value, refuse only
that row and retain good rows. All-refused pages return empty. Source failures
still reject through the original catch/log path. The binding uses the existing
generic mapper with optional results; its normalizer and public utility share one
pipeline, with unchanged identity-only refusal messages. Twelve mixed-page native
profiles plus all-refused/Date-error regressions execute the real Exchange. A
separate maintained-method probe pins TS retaining the raw malformed fields;
this per-row loss is intentional and does not widen shared IDs or assets.

The constructed ticker corpus in `tests/exchange_trait.rs` observes 264 cases / 504 summary, BBO, book and trade frames, including numeric aliases and all book level forms
through the production socket protocol and DefaultStore. Independent null,
string, NaN-producing, object, overflow, signed-zero and false fields accompany
distinct good rows. The maintained TS constructor and actual DefaultStore provide
native numeric-bit expectations. Fresh TS receivers and advancing native clocks
isolate numeric mapping from throttle timing. `ticker-native-controls.py paradex paradex-book`
compiles a live publication fault, requires a native assertion failure, then
restores exact source bytes and requires the named test to pass.

The constructed candle poll dispatches `maybeRecoverOhlcvTradeStream` to the
existing ingress helper. Its raw Date.now read and live per-key trade/recovery
caches preserve the stale guard, cooldown, warning-before-forced-subscribe order.
The real Exchange candle regression compares resubscription counts and warnings
at initial, three-second and thirty-second polls with the maintained TS method,
then verifies cleanup still cancels REST polling and callback delivery.

The retained-order-info source witness runs the unchanged maintained fetchOrders
and mapOrder methods for direct, REST and filtered-page profiles. The constructed
REST/Store and child-module receiver tests inspect raw info before serialization.
Input dates/numbers use shared retained coercion helpers; typed identity and enum
admission stay strict. Source live-info alias versus native owned snapshot remains
a declared boundary. `scripts/retained-order-info-controls.py` compiles three
isolated faults (member JSON projection, REST JSON projection, undefined loss),
requires the named real-binding test to fail and restores exact source bytes.

The typed native payload provider reaches options before constructor-started SDK
initialization. A hosted key reference therefore selects the source address-only
account branch instead of local private-key parsing. The child-module
`payload_signer_binding` witness runs NativeRuntime, the production REST host,
reqwest and a loopback config/auth server; it compares all three Order/ModifyOrder/
JWT hash-signature requests and auth headers with maintained Node SDK code, then
checks HMAC hex/base64 through the same provider and propagated signer rejection.
The labelled RED commit declares only the new host ABI/union; assembly is unchanged.
`payload-signer-controls.py` independently disables real provider installation and
truncates the forwarded hash, requires named compiled failures and restores exact
source bytes before rerunning the witness. Apex has no maintained payloadSigner
caller in this source tree; no Apex signing API is introduced by this repair.

The listenOHLCV audit keeps polling failures diagnostic-only: Host::start reports
the detached rejection without emitting an app error or writing Store state.
`candle_audit_failure_reports_only_and_preserves_polling_and_shared_cleanup`
compares the real REST/socket binding with the complete maintained public class
and original fetchOHLCV method. Both failing endpoints still lead to a poll at
3000ms, unchanged candles stay suppressed, and one listener cleanup preserves
the other until its own cleanup cancels polling. This extra report is the
authorized no-silent-failure residual; it is not a source parity claim.


Order-book sorting uses the canonical fallible JS merge sorter. NaN comparisons
mean equality and do not require a Rust total order; a throwing price getter
stops comparison immediately and reaches the original catch/log path. The full
maintained public class, real socket frames and constructed retained getter
witnesses cover both sides. The shared TimSort scheduler also preserves the
maintained 264-case ticker corpus, including finite levels separated by NaN.

The private onOpen specific-claims review records bounded disposal/recovery
differences separately from the shared original-cause diagnostic correction.
See `../safe-cex-exchange/PRIVATE_OPEN_REVIEW.md`; no Wasm-only row is closed.
