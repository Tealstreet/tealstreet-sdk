# Shared types

The two gateway non-UTF-8 audit verdicts are in `per-row-rulings.md`. Production
gateway ingress reads raw frames by channel; the stricter standalone
GatewayMessage decoder must not become a precondition for that ingress.

The bottom layer: every crate above — venues, store, transport, crypto,
exchange core — reads its wire vocabulary from here, so a type added anywhere
else is a second copy by definition. Naming, literal-union identifiers and
collision rules are in [TYPE_PARITY.md](TYPE_PARITY.md); per-field provenance is
in [P1A_FIELDS.md](P1A_FIELDS.md) and gate evidence in
[GATE_EVIDENCE.md](GATE_EVIDENCE.md). Representation decisions are DECLARED once
in [`../parity/MANIFEST-SPEC.md`](../parity/MANIFEST-SPEC.md) and cited by
anchor; the closed-typed-output boundary in particular is a declaration, not a
per-row argument, and it **excludes open-String fields, which carry the wire
value**. The coordinator's 2026-10-18 ingress ruling permits one explicit text
projection: isolated UTF-16 surrogate units become U+FFFD when a retained value
enters a typed Rust String or String key. Valid scalar text is unchanged. Raw
RetainedJsonInput and model info keep their original units, including lone-unit
keys, through the native payload marker; do not normalize the retained tree.
This keeps a lone escaped unit from rejecting an entire otherwise valid batch.
The real HTTP/client/Store proof is exchange's js_json_parse target; it executes
the maintained TS Store over seven independent text profiles and checks both
typed replacement and retained raw/info units before any JSON projection.

Retained JSON projection tests pin two native array elements and two object members,
with independent source/native lengths and executed comparison counts. Consuming-empty,
proper-subset and extra-output controls also remove both length guards; comparison and
index/key coverage still reject the mutations. This is finite projection-arm coverage,
separate from sequence capture and the num crate's nested model/output checks.

**`strict_decode` is the shape guard, and its permissive companions are
private.** The derived record decoders accept maps only; a positional array is
rejected before field decoding. The metadata companions that keep field names
alive are unreachable from outside the module, proven by `compile_fail`
doctests — which show non-compilation, not a specific diagnostic. Tuple-shaped
protocols stay arrays on purpose and are not candidates for the guard.

**`object_map::JsObjectMap` is own data properties with JS enumeration order** —
integer indices ascend before other names, other names keep insertion order,
replacement keeps position, delete-and-reinsert moves to the end. **Prototype
mutation is a deliberately separate operation**, because the three TypeScript
`__proto__` write mechanisms are not one thing: an ordinary shadowing insert is
reproducible here exactly, while a whole-object assignment that replaces a map's
prototype, and a nested mutation that reaches `Object.prototype` globally, are
REFUSED by choice with a finding — not by Rust incapacity.

**`js_coercion` is the lowest-layer `String(any)` and `num` owns the public
API.** Callers apply their own `??`/`||` branch BEFORE converting, because the
whole point of the inherited-member work above is that the branch outcome, not
the conversion, is where TypeScript diverges.

**`ws_auth` holds the canonical remote-signer WS payloads and crypto re-exports
them.** They live below every consumer so there is one wire shape; unknown
metadata is retained as `RawValue` specifically to avoid imposing a retained-field
depth limit on data we only pass through. `safe-cex-types` carries
`serde_json/raw_value` on its own dependency line — that is the per-crate pattern
for a capability feature, and it does not touch the shared workspace entry that
transport's gate pins against `infra/tradetunnel`.

`accounts` generates its credential policy from live TS metadata, so a venue
does not hand-write which fields it needs. `parse_import_account_data` is MIXED:
observable materialized lone units cannot cross its required UTF-8 return surface,
while the 128-level serde recursion limit remains a separate CHOSEN budget.
MANIFEST-SPEC §19 defines the observability discriminator; diagnostic-only text
must not be assumed when a caller can read the retained unit programmatically.

Gates: `cargo test -p safe-cex-types`, which drives the Node oracles in
`tests/`, plus strict clippy on the host **and** the wasm target. Generated
bindings come from root `yarn types:generate`; `tests/generate.test.cjs` is their
freshness check — regenerating is not a substitute for running it. The oracles
are the reason a field rename here is cheap and a shape change is not.

`position_record` defines the canonical 24-field linear-perpetual journal type.
Gross realized PnL, fees and signed funding remain separate; closeCount is the
source's numeric count, not an integer restriction. Optional numbers/venue ID/
close reason reject explicit null. Private companions and string/map front gates
retain serde inventories without public permissive paths. This contract imports
only PositionSide and does not fold fills or implement venue adapters.

For journal wire numbers use safe-cex-num's whole-entry `js_from_str`,
`js_from_value` or `js_from_value_ref`; PositionRecord is registered with the
exhaustive output graph. Plain serde entrypoints remain ordinary serde numeric
decoders and are not claimed to reproduce JavaScript overflow semantics.

StoreSubscriberError is shared below Store and the public-data view. It carries
the callback's message and optional response payload so caller error policy can
retain its source precedence. It is a host Error-like callback failure, not a
wire record decoder or an arbitrary JavaScript thrown-value interpreter.
PublicDataStore subscription callbacks are fallible, matching the Store path.

Subscriber errors may retain an opaque Rc error cause. Cloning preserves its
identity; Error::source exposes it for consumer downcasting without a dependency
on exchange error types. Equality compares present causes by pointer identity.

retained_json contains the canonical UTF-16 native data carrier. All five shared
model info fields use it, preserving their Option or ModelField outer presence.
NaN payloads, infinities, signed zero, own undefined, text and keys survive native
Serde and Store capture. The named native codec is distinct from raw JSON tokens;
its internal payload never appears in serialized JSON. The private native payload adapter supplies UTF-16 storage as native-endian bytes.
One borrowed read-only byte view lasts only through visit_bytes; the codec copies
pairs back into u16 storage. The wire payload keeps numeric unit sequences and
their existing range, type and cardinality checks. The single retained JSON
writer lives here below Num and applies JSON.stringify projection only on output.
Payload tags decode directly into the closed eight-variant tag enum. Short ASCII
field identifiers use stack storage; long and Unicode names retain the existing
String projection. Explicit key context preserves StringDeserializer's rejection
of optional and newtype map keys. retained_utf16 checks these admissions against
stock Serde and covers borrowed/owned UTF-8 byte tags independently.
`safe-cex-exchange/tests/js_runtime.rs` runs the maintained TS model/Store corpus
against actual typed bridges and the production Store facade before wire checks.
StoreData and StoreUpdate use the same native Store record profile: undefined
optional JSON-view properties are omitted rather than turned into null, while
retained zk config and nested model/info presence keep their native markers.

Stock Serde F64Deserializer can ignore the raw-token newtype hint and emit its
float directly. Capture accepts that event using the same Float path as the
newtype buffer, preserving signed zero, infinities and NaN payload bits. JSON
projection still refuses NaN and carries infinities as valid overflow lexemes;
the raw-token map protocol remains strict. The retained_native_f64 and Num
passthrough_native_f64 targets cover both direct capture and public projection.

OrderRiskProfile serialization follows its source construction branch. The
loss-side limit branch places positionSize before fee; all other risk branches
place it last. Rust-only position_size_before_fee metadata selects the order,
is omitted from TypeScript bindings and wire output, and defaults false on
record decode. Numeric values and optional-field nullability are unchanged.

CurrencyBalance uses the same private remote-companion pattern as the records
below, with the object guard at its concrete Deserialize implementation. It
retains declared Balance and available/locked payloads without normalization and
has no hidden function-local decoder.

The field checker refuses interface heritage in both dependency discovery and
shape enumeration. Required inherited fields must never disappear behind a
successful member count; a permanent inherited-number control pins this refusal.

AccountValidationError retains JSON runtime values without coercion. The source
ValidateAccountResult declares error:string but KucoinSpot returns truthy numeric
or object msg unchanged (ts-bug: validation-error-value). Generated error:string
is the documented static projection; the Rust field carries the runtime value.
Canonical retained capture preserves numeric lexemes. Non-JSON function values
and alias identity remain outside this JSON-valued contract.

Field-identity supplements assert native decoded fields rather than only
roundtrip bytes. Equal-value and symmetric-name swaps are separate risks.
Preserve shared feeder artifacts byte-identically; supplements record their
later provenance and do not retroactively strengthen past gate claims.

QueryValue is a non-JSON carrier: Undefined, native f64 (including NaN and
infinities), arrays and ordered object data remain distinct. It deliberately has
no serde Serialize/Deserialize or JSON conversion; transport chooses its source
renderer. The actual qs indexed and repeat profiles are separate from Axios
form-body URLSearchParams rendering. RestRequest ABI adoption remains core-owned.

Field identity is checked on decoded native fields, not inferred from a symmetric
serde roundtrip. Later distinct-value and one-hot supplements preserve original
feeder artifacts; identity_audit/DISPOSITIONS.md records candidate and gate scope.

Ticker.timestamp retains Paradex fetchTickers runtime clock values from
virtualClock.getCurrentTimeMs(). Other mapped tickers leave it absent. The TS
declaration projection omits this undeclared runtime extra; canonical numeric
ingress and the exhaustive output inventory include it as an ordinary number.

GatewayTickerData boxes its singular Ticker variant to keep enum storage bounded
as Ticker grows. Serde keeps the same untagged object/array wire shapes; plural
construction and the shared clock contract are unchanged.
The boxed_ticker target asserts 42 native field values over one singular and two
plural records before checking independently specified object/array wire shapes.
Its tagged-enum control must compile and fail at the singular native decoder.

telemetry contains host callback contracts, not wire records. Opaque Rc<dyn Any>
values preserve identity and non-JSON values in event data, context tags/extra,
and thrown failures. Optional source callbacks default to no-ops; callback Err
retains the original opaque cause. The store service owns the realm-local sink.

Gateway opaque data and the raw provider frame use canonical retained JSON capture
before Value construction. The public-data provider gate asserts literal reserved-
looking info objects through both entrypoints before source/wire comparisons.

Retained object capture selects the last raw duplicate before decoding its value.
Discarded values still require valid JSON syntax, but cannot impose UTF-8 or
semantic depth refusals on the surviving object; first key insertion stays fixed.

The discarded_duplicate_capture target independently guards overwritten lone-unit
and depth140 values, native first-key position and unrelated lexemes. Invalid
JSON syntax and surviving-value refusals remain separate negative cases.

ExchangeOptions.polymarket carries the five runtime account fields consumed by the Polymarket constructor. Its typed ExchangeOptionsPolymarket record uses existing UTF-8 credential strings and native f64 signerType; do not coerce numeric signature modes or replace the venue retained UTF-16 Text record with it. The source root declaration lacks this opts-as-any extension, so the field is skipped only in generated static TS, never in native serialization or source projection.

Balance.currencies stores CurrencyBalance: either the declared recursive Balance or the actual Crypto.com REST available/locked row. Serialization preserves the selected source field names; there is no computed free/used/total normalization. The static TypeScript declaration still names Balance, so its generated annotation remains unchanged while the runtime violation is recorded on the owning manifest row. Both runtime shapes require objects; available/locked are required non-null numbers.

DisplayedPosition decodes its flattened Position through the original MapAccess.
The private streaming filter consumes display-only fields and forwards payload
seeds unchanged, preserving raw decimal/-0 numbers and retained info lexemes
without Serde Content buffering. The remote serializer companion keeps exact
canonical field/serde metadata; optional null and duplicate-field rejection remain
explicit. Direct raw JSON, private-companion and bounded map-seed witnesses cover
this seam; plain serde numeric decoding still has its ordinary overflow policy.

OHLCVOptions.interval uses the existing AnyTimeframe union for worker runtime requests. Its 27 literals match maintained AnyTimeframe exactly; the source request annotation remains narrower. The ts-rs field projection exports Timeframe to match that live declaration, while the private wire retains AnyTimeframe and its existing object and optional-field guards.

Order and Position use `ModelField<T>` for output enums, Order.price, and their
optional fields. `Absent`, own `Undefined`, and `Value` stay distinct through the
native model codec; JSON serialization omits the first two. Explicit JSON null
remains invalid for non-nullable values (info may retain null). Numeric undefined
is not NaN: arithmetic readers explicitly use `to_number()`. Copying a missing
property into an object literal uses `into_property()` to create own undefined.
The static TS declarations and strict public PlaceOrderOpts stay unchanged.

`PreparedOrderOpts` is the one internal placement carrier for amendments copied
from output models. Its type/side can be undefined; venue source branches decide
what to do with them. It does not add a permissive public placement decoder.
`tests/support/model-field-corpus.cjs` supplies the 126-case Node presence corpus
used by the real native bridge and account encoder tests. Store merge policy is
a separate change; Option-like read helpers do not implement patch policy.

RetainedJsonInput text and own keys use UTF-16 units. Its shared iterative parser
serves both native JSON.parse (binary64 numbers) and raw Serde capture (numeric
lexemes). Duplicate assignment keeps the first key position; only surviving
values receive the lexical typed-ingress depth budget. Native parsing has no
explicit typed depth budget. Projection to serde_json::Value remains fallible
for lone units and NaN; it never substitutes replacement text.
The retained_utf16 target observes exact keys, both rows, numeric bits, reserved
Serde-looking keys and all 65,536 raw/escaped code units before projection.
`node ../safe-cex-parity/scripts/retained-json-contracts.mjs --package=safe-cex-types`
checks nine exact compiler-owned retained APIs and their bounded native/default
outputs. It retires the deleted ObjectVisitor and checks the shared parser's
actual signature; the full model/local/feature/target inventories remain separate.

The accepted-set ModelField visitor review executes the fixed f64 decoder through
ordinary JSON, native Serde unit and Default paths; native presence and number
bits are observed before serialization. PreparedOrderOpts, its private wire and
optional decoder have independent native field witnesses in the same scoped
`accepted-model-field-native.test.mjs` target. Generic helper delegation requires
an exact source review and the inventoried decoder's native observations.

Order.reduceOnly uses ModelField<bool>: absent, own undefined, false and true
remain distinct in native codecs. Its internal PreparedOrderOpts amendment copy
uses the same carrier; public PlaceOrderOpts keeps Option<bool> and refuses
non-booleans. The real native/Store presence target covers 48 actual-source
upsert/patch combinations. BingX's real amendment witness compares the complete
signed request against actual source execution and requires undefined reduceOnly
to be omitted from the wire and HMAC preimage.

Shared model `info` uses canonical `RetainedJsonInput`: Option retains absence on
Market/Balance/Ticker, and ModelField retains presence on Order/Position. Capture
native numbers, undefined and UTF16 before serialization; `into_value` is a real
JSON boundary, never an intermediate model conversion. The native Serde marker
and InfoRecord codecs preserve own undefined without exposing marker objects in
wire JSON. The single retained JSON writer lives below Num in this crate.

NumericValue retains Number versus Text for numeric runtime fields whose source
mapper passes strings unchanged. Market.limits.amount.max uses this carrier.
The static TS projection remains number, following its maintained declaration;
output preserves the runtime variant. Numeric consumers explicitly call num's
numeric_value_to_number, which uses the existing Number(string) parser.

Market.runtime is the shared carrier for runtime type/spot/swap/future/contract/
linear/inverse properties constructed outside the maintained TS Market declaration.
Its serializer writes these optional fields on the Market object; its private
object decoder reads the same flattened keys. Precision base/quote, price max and
cost limits are likewise optional runtime fields on the canonical nested models.
The static TS declaration projection omits them; native output and numeric ingress
retain them. Other venue constructors explicitly leave these fields absent.
The recorded public-market addon regression executes the actual Backpack and
Crypto.com TS constructors and compiled native binding against identical HTTP bytes.

Ticker.index is a required nullable number at runtime. WOO returns index_price:null for a real listed market; Option<f64> preserves that null in native and wire models while the generated index:number follows the maintained static declaration. Missing index, undefined and non-number/non-null values remain invalid. Null is not a zero price or a refused ticker.

MarketRuntime.fees retains the undeclared Paradex market literal's maker/taker
number object through the same flattened Market boundary. MarketFees requires an
object with both numeric fields; absence remains absent for other venues. The
static generated TS Market declaration is unchanged. The recorded public-market
addon regression compares all 66 Paradex markets against actual maintained TS.

Native retained payloads stream through the single `into_native_payload` Serde
adapter. UTF-16 units decode into validated u16 storage without an intermediate
Float-node tree or f64 vector. The tagged payload schema and malformed-unit
refusals are unchanged. JSON string parsing copies unescaped spans together;
escape/control validation and duplicate-property order keep their existing rules.

StoreData.socketHealth uses RetainedJsonInput so own undefined diagnostic fields survive typed notifications and native Store payloads. Ordinary JSON output omits those keys. StoreUpdate and the Store health getter/merge use the same carrier; a JSON map is an explicit ingress conversion.
