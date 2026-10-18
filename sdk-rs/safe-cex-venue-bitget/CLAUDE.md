Private-position freshness timestamps read the shared virtualClock, matching the
REST request-start clock. The synced-clock private_binding_frames regression feeds
an actual position frame and reconciles an older signed REST reply through Store.

The concrete order and position mapper callbacks read the shared VirtualClock.
`tests/native_clock.rs` checks synchronized timestamps through signed REST reads
for Classic, USDC, inverse and both UTA routes; inverse positions already used it.

# Bitget venue family

The private-topic host consumes the shared iterator protocol instead of indexing
Map/Set storage. It admits native iterator objects, caches the next method, keeps
live insertion positions and propagates iterator errors with their original value.
Private-topic loops close on body errors, preserving that throw over close errors;
step/value errors do not close.
The constructed-host `private_host_*` tests reproduce both iterator-object refusal
and deletion skipping on master. Native references, signed zero, Unicode code
points and permanent exhaustion stay in the runtime. Maintained wire frames remain
JSON arrays; iterator-object cases cover the reachable host boundary, not a claimed
current venue payload. `private_binding_frames` retains the four private topics.

The typed `Exchange::cancel_all_orders` routes to the family's assembled override.
Classic accounts send venue-wide cancellation even with an empty local order store;
UTA sweeps venue normal and strategy orders before cancellation; inverse uses each
unique market margin coin. Venue errors retain source emission and swallowing.
`tests/cancellation_binding.rs` exercises real constructors/signing/HTTP, and
`scripts/cancel-all-binding-controls.py` removes only the trait override to require
four compiled named failures and exact restoration. The actual private socket
witnesses cover Classic orders, fills, positions and account balances. Terminal
order frames patch status only, as TS does; fill amounts come from the new frame.

The concrete binding connects both signature-rejection retry imports to the
shared client replay path and shared request-stamp key. Non-2xx refusals stamp
the live config; resolved refusals copy and stamp it before re-entering request
interceptors. Tests cover all three families, writes, fresh signing timestamps,
the single-resend limit and unrelated failures. The write-retry witness advances the
shared VirtualClock between actual transport attempts, separately from the runtime
timeout clock, and verifies each observed timestamp and its exact HMAC preimage.
It must not depend on two wall-clock milliseconds happening to differ. The signature predicate uses
shared string-hint coercion; its source witness compares all 22 outcomes, including
custom hooks and getter throws. Older pending notes below describe standalone
extraction stages.

The concrete HTTP globals use the shared client factory and the runtime's transport
provider for routing and optional tunnel-adapter installation. A provider with no
adapter retains the original HTTP host. Network read retries delegate string-code
eligibility to the shared transport predicate, preserving GET/HEAD gating, response
truthiness and strict non-string code behavior. `tests/transport_retry_binding.rs`
drives real constructors and Exchange calls over scripted I/O; its live TS witness
checks 54 retry cases and three installer scenes. Run
`scripts/transport-retry-controls.py` to independently revert provider routing,
tunnel installation, code exclusions and response truthiness: each must compile,
fail its named real-host assertion and pass after exact restoration.

Behavioral reference: [TypeScript venue guidance](../../packages/safe-cex/src/exchanges/bitget/CLAUDE.md).
Variants remain modules inside this crate. The source variants are USDT, USDC,
and inverse; the disabled inverse variant is still part of the parity scope.

**Protocol oracle:** `yarn venue-oracle:bitget` executes current TypeScript
constants with the real shared enum declarations. It compares every exported
value, nested table key and monitoring-array entry against Rust. Its AST reader
also derives finite JSON wire-shape cases (this is declaration conformance,
not a claim that TypeScript validates server responses at runtime). Unsupported
syntax fails closed, including interface heritage: enumerating local members cannot
prove inherited fields. A maintained source injection adds a required numeric
base field and requires a declaration-named refusal. Refresh the frozen fixture with
`node sdk-rs/safe-cex-venue-bitget/scripts/types-oracle.cjs --write` only after
reviewing a source change. CI runs generation checks and ignored live tests.

**Wire fields:** optional non-null fields need `default` plus `present`; required
nullable fields need `nullable` without `default`. Plain `Option` loses the
missing/null distinction. Flattened extra fields force object decoding (serde
otherwise accepts positional arrays) and preserve unknown venue keys. Numeric
serialization uses the shared ECMAScript formatter, including timestamp fields.
**UTF-16 limitation:** escaped lone surrogates are valid JSON and TS strings,
but cannot be carried by the shared Rust UTF-8 `String`/`Value` boundary.
Affected manifest rows are divergent. Double-escaped live witnesses pin both
TS acceptance and Rust decoder rejection; never silently replace surrogates or
call them invalid JSON. Astral pairs remain supported.

**Tables:** each endpoint carries explicit `ApiGeneration` metadata (Classic v2
or Unified v3), independently pinned by name in the oracle. Slices preserve
declared order and values. They do not yet expose
lookup APIs. Every future mapper that performs a plain-object TS lookup must
account explicitly for inherited `Object.prototype` keys; table-value parity
alone does not establish lookup parity.

Implement exchange lifecycle against the [shared exchange trait](../safe-cex-exchange/CLAUDE.md).

**Pure predicates:** `family` projects the anonymous TS order argument to its
only observed member, `order.id`; its string/undefined inputs become
`Option<&str>`. `api` retains unknown-message type guards and exact boolean
suppression. The pure oracle executes current source bodies and tests Classic
versus UTA strategy-cancel routing; substring URL matching is intentional.

**Order decisions:** `order_logic` projects receiver state explicitly and uses
shared numeric adjustment. The maintained order oracle executes the actual
TypeScript class members with actual safe-math. Classic v2's reduction side
means position side; Unified v3 uses trade direction plus posSide. Preserve
first-position selection, warning timing, zero sentinels and NaN propagation.

**Signing reference:** `scripts/signing-capture.cjs` records actual TS hashing
preimages and real Axios adapter payloads offline. API rows remain pending.
The TS signing view deliberately differs from config.data for dollar stripping,
underscore replacement, and array spread; a string body (a replayed config's
already-serialized bytes) is signed raw. Preserve those differences and
the text-replacement bug; do not repair them while porting. The pure
toJsonString helper now consumes shared JSON.stringify and first/all replacement
operations through JsJsonValue, retaining undefined keys and nonfinite values.
Dollar tokens expand even inside keys, and prefix/suffix expansion can corrupt
the signed JSON while the adapter body remains intact. Literal replacement or
replace-all silently changes those bytes. The createAPI lifecycle remains pending.

**Closed unions:** output round-trips do not prove accepted-set parity. The
category decoder requires a string; its derived remote enum stays private so
qualified calls cannot bypass that guard. Metadata tests inspect actual serde
variants (including aliases), and malformed-input cases cover every literal.
Shared enums now also require strings. Their private derive inventories are
pinned through serde unknown_variant metadata, including aliases; the maintained
negative corpus rejects externally tagged objects and other non-string shapes.

**Classic history:** fills derive trade direction from hedge leg and open/close,
keep profit gross, negate fees without absolute value, and propagate callback
errors in source evaluation order. Bills split gross PnL and signed commission
with distinct IDs; unknown types remain visible. Inherited income categories use shared TableHit: tran_id retains the exact
member-specific shared String interpolation. The closed optional category is
refused as None while retaining the record; callable JSON omission agrees, but
in-memory category values differ and __proto__ retains {} only in TypeScript.
These are declared representation differences, not an Other fallback. Both
modules consume central coercion APIs.

**Order mapper:** `orders::map_order` retains protocol hits through all source
branches before projecting to the shared Order schema. Inherited type suppresses
strategy recovery; a later hedge/plan branch can overwrite an inherited side.
Do not refuse those intermediate values early. The [refusal-timing audit](REFUSAL_TIMING_AUDIT.md) pins later source errors by name/message and distinguishes attached legs from pending outer batch callers. Representation errors reject the
whole mapping result, not just a field; source callable omission and prototype
object serialization remain separate witnesses. `info` is an owned snapshot,
with both directions of source alias mutation pinned independently of NaN loss.

Missing Order status/type/side table entries project to own `ModelField::Undefined`;
JSON omits those fields while the native Store retains their presence. Inherited
values, unsupported raw side values and missing required IDs still refuse at the
final projection. `algo_frame_binding` covers real private frames and REST pages
across linear, USDC and inverse bindings, including later-row evaluation order.

**Inverse order mapper:** `orders::map_inverse_order` is a distinct source body,
not the linear mapper with a product flag. Keep its narrower aliases, absent
unknown-type strategy recovery and optional-method reduceOnly call (booleans do
not become strings; an owned non-callable toUpperCase throws after the clock).
Compute native remaining before table reads, then use shared Math.round on the
inverse conversion; retain native remaining in Order.extra. The source oracle
constructs the actual Store and calls update({markets}), because assigning a
markets property does not populate its lookup index. Both primary and attached
info are live source aliases but owned Rust snapshots; generated extra is also
shared across source primary/SL/TP while each Rust OrderExtra is independent; non-finite info loss is a
separate FINDING. Source numeric outputs are tagged before JSON erases their bits.

**Numeric ingress:** the six declared requestTime leaves use shared
JsNumberInput::decode::<f64> before serde_json's strict numeric decoder. Their
existing direct struct path reaches these fields before the flattened extras
buffer; this is a scalar opt-in, not whole-record eligibility. Unknown fields
retain the existing Value path and are never fed through the shared numeric
adapter. This avoids both dropping extras and changing the separate ECMAScript
JSON output projection. One hundred eight maintained source cases compare pre-serialization
bits for raw, owned, borrowed and native Values, including overflow, negative
underflow and signed zero. Do not use this leaf adapter to claim repair of an
upstream tagged/flattened buffer: an erased sign cannot be recovered downstream;
any future buffered envelope needs whole-entry shared ingress and its own proof.

`ApiCode` captures the whole scalar before selecting its number/string arm.
Both arms reuse checked `JsNumberInput` scalar decoders; the error-code enum gains
no serialization API. Direct text, owned/borrowed Value and qualified decoder
witnesses inspect native variant identity and numeric bits, including decimal,
signed zero, wide rounding and overflow.

**Public ticker helpers:** both public socket source members map to `ticker`.
Select nullish aliases before finite validation: a present invalid value blocks
later aliases. Percentage multiplication follows the finite check and may
produce infinity; return native ordered field pairs for ModelPatch rather than
JSON, retaining signed zero and field absence. Native NaN source inputs remain
explicit JSON-domain witnesses, never converted to null (which enables a later
alias). The scalar helper accepts native NaN directly. Socket lifecycle remains
pending; the live oracle executes both actual class bodies independently.

**History page reads:** `history_reads::fetch_fills_page` owns one supplied async
read at xhr.get's already-decoded response.data boundary. It propagates read
errors unchanged and keeps shared coercion failures fallible. A failure is never
an empty page. Truthy cursor selection is distinct from nullish message fallback.
Returned rows are owned snapshots; source response/row edits and array appends
are witnessed in both directions. Dropping the page drops the pending read;
no venue task registry or retry machinery is introduced. HTTP client assembly,
response decoding and the outer history pager remain separate pending rows.

**Async plan selection:** get_order_plan_type_and_amount receives lazy store,
resolver and selector operations. PlanSelectionOptions retains either original
PlaceOrderOpts or Order reference, including Order metadata. Position arrays are
owned snapshots: source selector mutations reach store and vice versa, whereas
Rust snapshot edits remain local; both directions have maintained witnesses. Zero (including negative zero) and trailing
stops read stored positions without starting resolution. Other amounts await
the opposite-side resolver; a rejection propagates unchanged and selection does
not run. Dropping the caller drops its pending resolution future. This owns no
cache or task registry; resolvePositionsForPlanType remains separately pending.
The options borrow also excludes concurrent caller mutation during resolution:
source selection sees a later amount/symbol/side through the same options object;
Rust callers must retain stable borrowed options until the future completes.
This is separate from preserving which original Place/Order reference is passed.

**Private order lifecycle caches:** source uses insertion-ordered Map, so inherited
object names are ordinary keys here. IndexMap::shift_remove retains survivor
order; replacement does not move an existing key. Pruning removes expires <= now
and retains unordered NaN comparisons. Each mark samples Date.now once, then
prunes BOTH caches only when the updated map exceeds 2000 entries. Marking open
does not erase a terminal tombstone; marking terminal erases the open entry.
These helpers own no socket tasks and perform no Store mutations.

**Ticker handlers:** both source classes execute against the real Store in the
handler oracle. Standard inherited ticker hits survive through shared TableHit:
their absent id coerces to "undefined" and can update a different stored ticker
with exactly that id. Preserve this ts-bug, including the resulting notification.
Native ModelPatch carries derived infinity and signed zero. Release the ticker
view borrow before Store mutation so notification may re-enter safely. The
notification failure propagates after the first committed row, preserving the
StoreMutationError, Store-local subscriber ID and original cause. Later listeners
and rows do not run. Dictionary prototype traversal remains a separate FINDING;
see STORE_NOTIFICATION_AUDIT.md. Keep the concrete From<StoreMutationError> and
callee result annotation; do not flatten the subscriber error to a message.
This handler does not convert to ExchangeError. The shared conversion at
9af82bfeae retains subscriber IDs; a native consumer witness pins that projection
without claiming concrete socket assembly.

**Bill page read:** fetch_bill_page keeps the public-only guard before option
coercion and resolver/read operations. Its resolver boundary is the source
market id projection, not a new market resolver. Request params use shared
JsJsonValue so Math.min's NaN and signed zero stay native until transport.
Use Number here, not parseFloat, and preserve truthy limit/time guards versus
nullish resolved-id fallback. Errors propagate unchanged; the outer history
catch remains a separate row. Nested malformed object-valued options become
owned request snapshots; the oracle records source aliasing in both directions.

**Income history catch:** fetch_income_history owns the supplied page operation,
projects its failure to the thrown JS value, and reports through a fallible
operation. An empty message, false, zero or empty array is retained by ??; only
null/undefined falls back to the thrown value. Coercion and reporter errors occur
inside the catch and therefore escape it. Do not flatten the reporter failure or
replace it with []. The shared core reporter is still unit-returning at this
base; wiring that reporter is not established by the supplied operation.

**Field identity:** roundtrips prove wire shape, not native member identity.
Separate live supplements inspect all declared wire members and category variants,
ticker seeds, position-fold inputs and async selector fields directly. Original
frozen identity corpora remain unchanged; FIELD_IDENTITY_AUDIT.md records the
candidate classification and compiling swap controls. Run each source capture
once per corpus, never once per case.

**Historical fills:** fetch_historical_fills uses the shared candle-cache splitter
and actual fill mapper, with supplied detection, market resolution, page read and
fallible reporting operations. Preflight errors escape; caught page failures keep
prior completed pages but none of a partially mapped failing page. Dedup is global
across windows and runs after whole-page mapping. Empty-string IDs are present;
short pages do not terminate. Params remain native JS-shaped numbers. Concrete
exchange assembly and the shared reporter notification seam remain unclaimed.

**Historical positions:** fetch_historical_positions delegates unchanged typed
options to the shared journal fold with symbol_scoped=false. The shared folder
owns derivation; packages/venue-data owns grading and closed-identity admission.
This adapter assigns no independent trust to a raw axis. Live source cases pin
native argument bits, original result/error forwarding and cancellation. Owned
JSON records differ from source live aliases and callable/undefined fields;
those boundaries are explicit, and concrete Exchange registration stays pending.

**Positioning consumers:** get_or_fetch_position_mode and get_or_fetch_margin_type
await the supplied selected-field resolver before any UTA or reporting effect.
A present shared enum returns immediately. Missing values read UTA lazily, emit
the exact diagnostic synchronously, then return its error; reporter failure
escapes unchanged instead. Resolver and reporter metadata allocation is retained.
The other field is not observed. The source corpus gives it a throwing getter.
These adapters do not implement resolveSymbolPositioning's cache or registration;
closed optional input fields exclude source open values and executable getters.


**Socket ping fields:** socket_ping ports the public, secondary public and private
ping bodies independently. Read the disposed guard first, then sample the supplied
performance clock once and publish pingAt before sending literal "ping". Private
ping uses sendWithLog; public pings use send. Cell<f64> permits the send callback
to observe the updated timestamp without borrowing the receiver exclusively.
Send errors keep their original value; clock and socket construction are host
operations, and concrete websocket assembly remains pending.

**Classic-to-UTA promotion:** account_predicates::promote_uta_routing_from_classic_unsupported
passes the original optional payload to the existing supplied predicate and reads
supportsUtaRouting then testnet lazily. False guards leave both flags unchanged;
only successful promotion sets isUta and accountTypeDetected. Failures propagate
before later getters or writes. The predicate retains its separate manifest
contract; this field adds no string/JSON coercion boundary.
**Account read:** account_read::get_account consumes the supplied parsed
xhr response.data operation at ACCOUNT_SPOT. Parent selection is truthy OR,
not nullish fallback; subId and affiliateId remain present even when null.
Use shared fallible String coercion in user/sub/affiliate order. Preserve the
original read error and distinguish null/missing outer and nested data errors.
The caller owns the read future. The account oracle finishes the independently
fixed native walk before serializing results; missing source inputs and surplus
rows fail without a source-total guard. JSON input excludes executable field getters
and custom prototypes; concrete REST and Exchange assembly remain separate.

**Subscription delegates:** subscriptions::listen_ohlcv and listen_order_book
forward to the supplied existing ExternalPublicData socket without connection,
readiness checks or cleanup wrapping. Rc callback identity and the returned
Cleanup allocation are retained; cleanup follows the shared single-use FnOnce
contract. OHLCVOptions and symbol follow the shared owned
typed provider boundary, excluding caller-visible JS object aliases, open option
fields and lone surrogates; emitted records follow the provider's existing owned
Candle/OrderBook model. Concrete websocket construction and Exchange registration
remain pending. The source supplement executes the actual two class fields through
type-erasing transpilation; it does not claim semantic TypeScript checking.


**Concrete secondary ticker socket:** secondary_public::BitgetSecondaryPublicWebsocket
constructs a real BaseWebSocket with its WsProtocol bound from construction. It
uses the shared exchange parent bridge, event emitter, routing resolver, JSON
parser and ticker handler; host open/message/close callbacks are core-owned.
Numeric topic keys use JsObjectMap order, and inherited __proto__ numeric writes
are ignored unless connect created an own key. Reopen replaces this instance's
market listener. Core has_websocket checks presence independent of OPEN or first
traffic; use it for source `!this.ws`. Borrowed state accessors expose the protocol's cells/maps without allowing
facade handles to be rebound; scalar counters read the bound protocol directly.
The shared weak parent bridge requires callers to retain the venue. Core-owned
asynchronous dial differs from the source synchronous wrapper installation,
and native JSON rejects an entire packet with a later lone surrogate although
source processes an earlier ASCII row. Binary frames follow Node Buffer semantics.
WsProtocol's ExchangeError conversion retains subscriber metadata/first-row commit
at direct entry; event-loop error reporting retains the shared core contract.
These choices are manifest divergences. Full exchange and primary/private socket
assembly remain pending. The separate ping adapter stays in PR 7428 and is not
counted again by this class slice.

**Two modules export `fetch_open_positions`, and the test fixtures implement
BOTH host traits.** `account_positions` and `inverse_positions` each have one,
and `tests/inverse_positions.rs` glob-imports both modules while its `H`
implements `AccountPositionsHost` and `InversePositionsHost`, so the bare call
resolves to neither (E0659). Import the one you mean explicitly. That test
merged without compiling on 2026-10-14 because `rust.yml` had cancelled or
failed all 100 of its last runs; do not read a green PR page as a compiled test.

**The native-value harness is copied verbatim between the position suites**, and
`tests/inverse_positions.rs` reaches less of it than `tests/account_positions.rs`
does. It allows the unreached part rather than pruning its half: keeping the two
copies textually identical is the point of copying, and `V::Hole`/`V::U`/`V::Null`
are the missing/undefined/null trichotomy the wire decoders turn on, so a harness
that cannot spell null is weaker than its sibling in a way nothing would flag.

The public `cancel_symbol_orders` trait hook invokes the assembled family member.
Classic and USDC retain both the inner cancellation emission and the outer symbol
catch emission before propagating the original refusal; inverse retains its inner
swallowing behavior. The cancellation binding tests assert that distinction and
the selected symbol/order IDs through the actual signed client.

The shared ModelField cutover (#465) preserves missed mapOrder table lookups as
own Undefined when the source assigns them. Missing raw side is Undefined;
a present unsupported raw side and unreplaced inherited table members retain
the explicit closed-model refusal. Algo frames still evaluate the mapper before
branching on status. Do not skip mapping merely because status is unmapped.

The order oracle checks native own-Undefined fields before JSON projection. It
walks every output and checks native row counts before pairing with source rows:
1,327 linear cases (1,263 source successes, 78 accepted own-Undefined cases) and
1,166 inverse cases (1,095 source successes, 100 accepted own-Undefined cases).
The field-identity supplement adds exactly one independently named successful
case per family: 1,328/1,264 linear cases/successes and 1,167/1,096 inverse.
Its own-Undefined counts remain 78 and 100. Each caller selects its expected
profile explicitly; neither the base nor supplementary count is inferred from
the generated corpus. Both profiles reject a missing or duplicate identity case.
Representability checks visit every source row without short-circuiting.
The actual socket/Store and authenticated REST witnesses additionally distinguish
missing-side Undefined from absence across all three families, while unsupported
raw side still refuses. `scripts/map-order-undefined-controls.py` compiles the old
throw, absent-property and unsupported-side faults against both the real binding
and frozen oracle, then requires exact restored passes. Run it exclusively.
All three representation controls compile and fail named tests in both the
constructed binding and frozen oracle; each exact restoration passes both.
The actual TypeScript order oracle also passes without changing its fixture.

The imported response-key helper preserves primitive `in` errors before account
mapping and searches inherited native keys in original/snake/camel order. The
signed-response and constructed-host regressions pin refusal timing and shadowing.

The constructed ticker corpus in `tests/exchange_binding.rs` covers 720 frames
across primary, second and third ticker sockets through Classic, USDC and inverse
constructors. Every numeric alias receives ten independent values between good
siblings, with distinct later aliases and a large finite percentage that
produces infinity after multiplication. All eleven stored numbers are checked
against the maintained TS exchange and actual Store before JSON. The shared
fault runner swaps the live bid lookup and requires compiled RED/restored GREEN.
The native witness identifies shards by their actual subscription frames and
asserts pairwise socket identity, so three labels cannot exercise one socket.

Position freshness reads canonical retained info directly. Nullish aliases skip own
undefined, while a present NaN blocks later aliases. The constructed private-receiver
regression compares nine source timestamp cases across all three variants, including
parseFloat prefixes before unpaired UTF-16 surrogates and signed-zero bits.

Unknown UTF-16 plan metadata follows the source typed-order fallback after String
coercion. Only exact table keys select TableHit; an unpaired unit cannot match an
ASCII key. The constructed exchange control checks eight source cases across
Classic, USDC and inverse variants without projecting Order.info through JSON.

Both concrete order mappers capture the original row as retained input before
reading aliases. Unread UTF-16 units, nonfinite numbers, negative zero and own
undefined survive in the owned info snapshot; the constructed mapper witness
covers Classic, USDC and inverse. The live order corpus observes those native
values and property presence before its legacy JSON comparison.

The secondary public socket now starts the shared core-owned dial eagerly, using
canonical start_eager through the existing registry. Real Bitget and Backpack
call-prefix witnesses and independent production controls are documented in
EAGER_DIAL_AUDIT.md. Pending I/O is dropped on disposal before runtime polling.
The separate missing CONNECTING handle during a held handshake remains an open
FIX-audit finding; do not mark the assigned secondary connect row complete yet.
