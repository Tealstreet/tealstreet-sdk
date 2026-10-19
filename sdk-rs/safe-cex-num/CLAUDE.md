# JavaScript numeric and language semantics

The individually reviewed accepted contracts and their native/default evidence
are described in [ACCEPTED_CONTRACTS.md](ACCEPTED_CONTRACTS.md). Keep decoder
observations separate from body-specific non-decoder classifications. The
numeric_output_invariant target checks native changed-bit refusals and nested
paths; its compiling comparison fault must fail before exact restoration passes.

This crate hosts shared ECMAScript compatibility helpers below services and venues.
BuiltinReference::prototype selects the actual native parent identity in the
same retained realm. It reads metadata rather than the shadowable `__proto__`
property and never allocates a second intrinsic heap. The complete accepted
BuiltinReference contract observes all five parent roles plus existing get/set.
Use `js_trim` when porting JavaScript String.prototype.trim. Its exact set is:
U+0009–000D, U+0020, U+00A0, U+1680, U+2000–200A, U+2028, U+2029,
U+202F, U+205F, U+3000, U+FEFF. It preserves internal whitespace.

Rust char::is_whitespace excludes U+FEFF and includes U+0085; neither
predicate includes U+001C–001F. Adding BOM to Rust's predicate is insufficient.
Rust str cannot represent lone UTF-16 surrogates.

`cargo test -p safe-cex-num --test js_trim` invokes Node and compares every
Unicode scalar against the actual runtime's trim, trimStart, trimEnd and split(/\s+/). Keep this test non-ignored.

`js_trim_start` and `js_trim_end` preserve the opposite boundary.
`js_split_whitespace` reproduces split(/\s+/), including empty boundary
segments and `[""]` for empty input; it is not Rust split_whitespace.
All helpers share the existing numeric parser whitespace predicate. Store
instrument classification and risk parsing, and candle-cache chunk-key parsing,
use these helpers instead of maintaining private character sets.

Use `js_number_string` for ECMAScript Number-to-String, including signed zero and
exponent thresholds. Rust Display and serde number spelling are not substitutes
when bytes are signed or compared. This function returns numeric text, not a JSON
serializer; NaN and Infinity require the caller's JSON policy.

The Number spelling gate pins 10,034 source results and native inputs separately,
then checks the number of comparisons actually executed. Empty and partial
capture controls, including truncation after the source count, must fail by name.

`js_truthy(Option<&Value>)` implements Boolean(any), with None as undefined.
Do not use `Number::as_f64().is_some_and(...)`: arbitrary-precision overflow
returns None but JavaScript Infinity is truthy. Arrays/objects are always true.
`tests/js_primitives.rs` compares generated inputs against live Node.

`js_number` is Number / unary + for JSON values plus undefined. For executable
objects, implement `JsNumberObject`: exotic conversion first, then valueOf,
then toString; return None for skipped methods or ordinary object results.
An exotic hook returning an object must return TypeError, never None.
`js_number_primitive` permits BigInt; `js_to_number` (unary +) rejects it.

Use `math_round` for Math.round arithmetic (including NaN and negative zero).
`display_round` is magnitude-dependent rendering precision and maps NaN to zero;
`round` remains only a compatibility alias for existing display callers.

Own-key ordering delegates to the lowest-layer types `js_coercion` backend, also
used by typed Balance dictionaries; types cannot depend upward on num.
`js_own_keys` takes unique string keys in insertion order and returns canonical
array indices first. `4294967295`, `01`, `1.0`, signs and exponent forms are
ordinary strings. `normalize_object_order` is shallow; `normalize_json_order`
recurses through objects and arrays. Symbol keys are outside JSON's domain.

`JsJsonValue` retains undefined and nonfinite numbers until `js_json_stringify`.
Do not turn missing fields into serde_json Null before signing/wire views split:
undefined object fields are omitted, array entries become null, and top-level
undefined returns None. This prepared-data serializer excludes executable
callbacks/replacers/cycles; BigInt returns the shared TypeError.

`math_log10` ports V8 fdlibm with native arithmetic profiles: Node's darwin
arm64 build contracts multiply/add while Linux x64 keeps separate operations.
Preserve that distinction: forcing either profile globally changes boundary
results by one ULP. The private profile regression pins the CI decimal-boundary
witness in both modes; the live oracle prints Node/V8 versions and probes ±256
ULP around every decimal power plus random bits. Other V8/compiler profiles
remain outside these measured profiles.

`math_pow` follows V8's DEFAULT `use_std_math_pow` path, not legacy fdlibm.
The wrapper must retain JS NaN/infinity rules and direct square/square-root
paths; bare powf disagrees at MAX_VALUE^0.5 and subnormal squares. General finite
powers use host libm, matching the measured Node path on that host.
`js_bigint` preserves RangeError for fractional/nonfinite Numbers and SyntaxError
for malformed strings; null/undefined/Symbol yield TypeError. Executable objects
reuse numeric-hint hooks. Error `message_utf16` preserves exact V8 diagnostics,
including 1,000-code-unit truncation splitting a surrogate; `message` is its
well-formed UTF-8 display view. Do not collapse error kinds into a parse error.

`JsRootCollator::try_new` loads explicit `und` with coherent generated Unihan
root tables; compare keys with `compare` and use stable sorting for equal keys.
ICU4X's stock implicit-Han root disagrees with Node for CJK. Do not replace the
data profile or mix its auxiliary tables. Tests assert data metadata/checksums,
compare a generated Unicode corpus to Node, and preserve the dependency audit.
Initialization failure is returned, never replaced by byte order. Rust str
excludes lone surrogates; this is a CHOSEN Unicode-string API boundary.

`math_min`/`math_max` take two Number operands, propagate NaN and select -0/+0
respectively on mixed zero signs. Rust f64 extrema ignore a NaN operand; they
are not substitutes. Apply numeric coercion separately before calling.

`js_time_clip` implements the Date Number boundary, including +0 normalization.
`js_date_parse` and `js_date_parse_utf16` transcribe V8's ISO/legacy parser;
invalid input returns NaN. Callers provide a local-wall-time-to-UTC adapter with
host DST gap/overlap rules. Date-only ISO and explicit offsets bypass it. Never
silently choose UTC for a local datetime; use `|ms| ms` only for an explicit UTC
host. The measured Node profile has a 32-bit signed Smi timezone limit.

`lodash_snake_case` and `lodash_camel_case` reproduce pinned lodash 4.18.1,
including deburring, ordered word alternatives, ordinals and emoji grouping.
Do not replace the explicit character classes with Unicode categories or generic
case converters. The oracle asserts the resolved package version and source SHA;
it loads lodash beside the existing isolated `SAFE_CEX_TYPESCRIPT_PATH` install.
The chosen lone-surrogate restriction is declared in
[the shared string boundary](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary).

`js_table_lookup` preserves own entries and all twelve inherited Object.prototype
identities in `TableHit<T>`, including the non-callable `__proto__` result. It
assumes an ordinary unmodified prototype; null-prototype tables use own lookup.
`is_present` answers presence even for own null/undefined, never generic `??`.
`is_truthy` delegates JSON mapped values to `js_truthy`; `is_nullish` tests the
nullish fallback separately. Typed mappings use the corresponding `_by` methods.
A mapped zero is present and falsy; mapped null is present and nullish. Inherited
identity does not implement invocation: consumers must preserve their call path.

Use `ProtoMember::js_string` for open-string interpolation of inherited hits.
Constructor becomes `function Object() { [native code] }`; `__proto__` becomes
`[object Object]`. The key name is not the value. Closed typed fields still need
their own explicit policy for carrying or refusing the inherited value.

`js_replace` and `js_replace_all` wrap the types-owned UTF-16 replacement core;
use the `_utf16` variants for arbitrary code units. Dollar tokens expand against
the original source, including for a string needle. First replacement preserves
valid UTF-8 inputs; all-replacement can insert between surrogate halves for an
empty needle and returns `RepresentationError` rather than substituting a
character. This is the CHOSEN shared boundary declared at
`parity/MANIFEST-SPEC.md#shared-utf8-string-boundary`.
The mandatory `js_replace` test measures 33,780 live Node input triples. Its
private-copy fixture records the old emoji empty-needle bug and two agreeing
nonempty controls; dollar-literal and scalar-advance mutations must fail.

`ProtoMember::length` returns the stock inherited function arity. The prototype
object has no length (`None`), distinct from the three zero-arity methods.
The existing mandatory table oracle measures all twelve against live Node;
changing the prototype length to zero must fail `inherited length metadata __proto__`.

`js_array_index` exposes the canonical index predicate used by own-key ordering.
`js_object_or_array_entry` reads object members or array elements using a string
key. It does not implement prototypes, array length or boxed scalar properties.
Noncanonical spellings still find ordinary object keys but never array elements.
The mandatory `js_property` oracle checks 5,135 keys against actual array property
creation and own lookups, including the nested string-zero/items path.

`lodash_random` accepts three `JsPrimitive` arguments (Undefined for omission)
and an injected `FnOnce() -> f64` draw. `lodash_random_json` accepts optional
JSON references and also preserves lodash's iteratee-call guard and reference
identity. The source runs once after successful coercion, even for equal bounds;
production callers provide their host RNG, normally in [0, 1). CHOSEN host RNG
profile is a source-only divergence: deterministic bounds/coercion remain exact.
The fractional path's epsilon depends on the JavaScript string form of the draw.
TWAP reaches two numeric bounds; zero/single arguments, boolean overloads,
Symbol/BigInt and JSON object forms are completeness coverage. Callable host
objects are outside these adapters. String ingress uses the shared UTF-8
representation contract, with live escaped-surrogate refusal witnesses.
The mandatory `lodash_random` oracle pins lodash 4.18.1 and its source SHA,
checks 123,081 injected-draw bit cases and call counts, and requires an actual
package integer-span mutation and a missing-package refusal to fail by name.

`get_key::{v,vn}` uses presence for exact/snake/camel lookup. Own null stops
both searches; own undefined stops v but vn advances its outer alias list.
The borrowed Entries adapter retains undefined and delegates inherited reads to
TableHit. JSON/ordinary stock-prototype data is supported; executable getters
and custom prototypes are outside this chosen adapter. Private memoization has
no value-observable effect. tests/get_key runs the actual TypeScript source
using the isolated compiler/Lodash install and tags inherited values before JSON.


Numeric ingress lives in `numeric_ingress::{js_from_str, js_from_value,
js_from_value_ref, JsNumberInput, JsNumberOutput, js_value_as_f64}`. Capture at
whole entry, before tagged/untagged/flatten or explicit map buffers can erase
integer-spelled negative zero or turn native infinity into null. Explicit nested
buffers and generic JSON payloads use `JsNumberInput`: e.g.
`ApiResponse<JsNumberInput>`, then `data.decode::<Typed>()`. Raw Value decoding
itself is not declared lossy; the typed adapter must emit real numeric events
for Serde Content, whose downstream Value visitor cannot retain infinity.

Typed String/keys use the same std UTF-16 well-formed projection as the lower
native typed adapter: isolated units become U+FFFD under the coordinator's
2026-10-18 ingress ruling. Capture and retained info remain lossless. The numeric
eligibility, Value-buffer refusal and native output checks still apply unchanged;
text normalization must not weaken any of them. Exchange's js_json_parse target
checks the actual Vec<Market> numeric wire entry alongside the real HTTP/Store
binding over seven source profiles, retaining all rows and exact info units.

Register ALL output fields with `js_number_output!`. The recursive eligibility
contract includes skipped and asymmetrically renamed Serialize fields. Exhaustive
destructuring refuses omitted fields; Value-backed fields fail the trait bound.
Implement `check_number_output` with the expression macro, e.g.
`js_number_output!(self, struct { field_a, field_b })`. It expands no item
declarations, so the parity source indexer can still inspect the explicit impl.
Do not hand-write an unchecked method body. A custom
Deserialize implementation must use JsNumberInput for every intermediate buffer;
no helper can repair a sign erased upstream or inside arbitrary user code.
The additional native-event output check compares retained numeric wire paths
before JSON serialization could conceal a projection. It is defence in depth,
not a substitute for the exhaustive field contract. Wire-shaped Serialize is
required for that check; deliberate numeric transformations belong after ingress.

Use f64 where TypeScript reads Number, strings where it retains decimal strings,
and BigInt helpers only at actual BigInt sites. JSON numbers round to binary64
BEFORE integer visitors. No exact-integer improvement over JavaScript is claimed.
The raw, owned and borrowed entry paths retain the shared serde_json recursion
budget; the refusal is a CHOSEN input-depth boundary. Local raw_value adds only
capture capability; never alter the workspace serde_json feature declaration,
which is shared with tradetunnel's frame implementation.

Output eligibility includes `str`, references to eligible values, and
`Cow<str>` in both owned and borrowed forms. Keep reference eligibility recursive:
a reference must not make `serde_json::Value` or another ineligible type eligible.
This permits borrowed/static configuration fields without changing public models;
it does not turn the owned ingress decoders into zero-copy borrowed decoders.

Shared model eligibility is implemented in `numeric_ingress/shared_output.rs`,
where the trait is owned, to avoid a types-to-num dependency cycle. Registrations
use exhaustive field/variant checks. `tests/shared-output-census.json` records
venue field sites, their transitive registrations, named info passthroughs and
uninhabited host-capability graphs; it is a conservative source census, not a compiler name
resolver. Ordinary shared PlaceOrderOpts and OrderBook graphs are eligible.

Opaque passthrough fields keep their public Value or Option<Value> shape only
with an explicit declaration:
`js_number_output!(self, struct { normal_field } passthrough { raw_response })`.
Use `numeric_ingress::js_deserialize_passthrough` in that field's deserializer
(an optional present field can map its result to Some). Name the field and this
operation in the parity row. The helper captures JsNumberInput before building
Value, including after tagged Content buffering. Native infinities use valid
overflowing JSON lexemes until JS wire projection; NaN is refused, never emitted
as invalid JSON. This is NOT an unchecked output escape: the retained numeric
comparison remains active, and a stock Value visitor projecting infinity to null
still fails. Ordinary unmarked Value and &Value remain compile-time refusals.

The shared registrations deliberately centralize the compile-time field census:
a new shared field makes the exhaustive shared impl stop compiling, so it must
be reviewed once at its owner rather than copied into every venue. Venues still
own live numeric witnesses for their paths. A venue can instead destructure an
embedded shared model inside its own implementation when a per-venue review of
all future fields is desired; no foreign trait implementation is necessary.

A named passthrough is an audited exception: the macro cannot inspect Serde
attributes or prove that the capture helper is paired with the field. Do not
skip or asymmetrically rename its serialized output, which would hide that path
from the numeric comparison. Review the field decoder and attributes together.
Ordinary tuples of arity 1 through 16 recursively check EVERY element (unit is
also eligible), matching Serde tuple support. JsObjectMap recursively checks all
values; IndexMap checks keys and values without sorting or mutating either map.
Slices and Box<[T]> are eligible when T is; Vec, Option, arrays and Box retain
recursive bounds. An unmarked Value at any depth remains ineligible. See
[container census](tests/CONTAINER_ELIGIBILITY.md) for scope and exclusions.

`numeric_ingress/position_record_output` registers the canonical journal record
and its inline/string-union wrappers with exhaustive graph checks. It consumes
the shared PositionSide implementation; no second numeric ingress engine or
private lexeme conversion is introduced. The facade journal gate observes all
12 numeric fields through raw, owned and borrowed entrypoints before JSON hides
non-finites or signed zero.

Canonical JSON numeric capture now lives in `safe_cex_types::retained_json` so
shared model decoders cannot depend upward on num. Num delegates Deserialize,
Value capture and named passthrough construction to that implementation; it owns
the typed numeric visitor events and exhaustive output guard. Keep this a single
capture implementation, including the raw recursion budget and NaN refusal.

Market, Order, Position, Ticker and Balance explicitly register `passthrough
{ info }`. Their private shared wire companions capture through the lower helper
before constructing Value. Missing info is None; present null is Some(Value::Null).
Rows consuming these models must name info and its retained-capture passthrough;
registration alone is not a numeric repair. Unmarked Value and &Value remain
ineligible, and the whole-output numeric comparison still checks named info.
ExchangeOptions and both external-data union arms are registered exhaustively.
The four capability markers remain uninhabited: an empty match admits no host
handle, and wire capability payloads still refuse. TaskOrigin, TwapStatus and
SliceAction are exhaustive enum registrations under `safe_cex_types::store_model`.

The collation data-version test runs locked Cargo metadata with network access
permitted: a scoped num build does not prefetch unrelated workspace packages.
Do not force offline metadata unless CI first fetches the whole locked graph.
Metadata-query failure remains a hard failure before version/CLDR/ICU assertions;
it must never be interpreted as a successful pin check.
The CI repair was controlled by temporarily changing the expected data version
to 2.3.1: compilation and metadata succeeded, then icu_collator_data failed its
named equality assertion. Restoring 2.3.0 restored both collation tests.

Numeric field-identity supplements keep the original edge and arithmetic feeder
artifacts unchanged. Assert native fields with distinct values; roundtripping
through Serialize is not an identity check. Per-element traversal counters must
identify which element was visited, not only the total number of visits.
See tests/FIELD_IDENTITY_AUDIT.md for the gate inventory and scoped exclusions.
NaN-capable oracle captures use tests/support/number-bits.cjs: only NaN payloads
are canonicalized, while finite values and signed zero retain exact bits. Capture
headers record the executing architecture; this is not a cross-host libm claim.

`get_key::{v_unknown,vn_unknown}` is the separate fallible JSON-data entry.
None/null short-circuit to Undefined; other primitive RHS values produce the
V8 TypeError containing the exact key and RHS string. An empty vn alias list
never evaluates the RHS. Existing v/vn/GetKeyValue and own-undefined semantics
are unchanged; both old and new oracles run in the mandatory get-key CI step.
`GetKeyUnknownValue` retains borrowed JSON, computed array length, Object
prototype identity, ArrayMember identity and the distinct Array prototype object.
Arrays are dense JSON arrays with stock unmodified prototypes: no holes, custom
array properties, getters or executable invocation are claimed. Array methods
are identities, not callable host handles. ArrayMember::js_string retains the
Array constructor rather than substituting Object. The common UTF-8 input
boundary remains chosen; no replacement of lone surrogate code units occurs.
The unknown-input oracle measures 2,961 actual-source rows on Node 24 and 26.
Compiling primitive-as-missing, missing-array-length and wrong-array-prototype
mutations each fail `UNKNOWN RHS source identity`; restoring production passes.

The shared-book numeric oracle pins native bids and asks separately before
walking rows, then pins six executed numeric comparisons per decoded book.
An empty decoder result must not turn the numeric corpus into an empty walk.

The own-key gate calls both normalization wrappers directly against the same
live source rows. Recursive normalization delegates below num and cannot prove
that the separate public object-normalization wrapper executes correctly.

Cardinality supplements independently exercise retained array/map capture,
opaque serialization and named passthrough outputs with two members/elements.
Keep the original numeric-edge corpus and its field-identity fixtures unchanged.

Shared Ticker output eligibility includes timestamp in the exhaustive ordinary
numeric inventory. Its runtime clock value is not an info passthrough and cannot
bypass the output check. The ticker_timestamp target observes native field bits.

Source-corpus cardinality checks use exact totals and completed assertion counts;
thresholds are not completeness checks. The test-only source_cardinality walker
keeps before-count empty/subset controls separate from after-count truncation.
Original feeder/oracle bytes stay unchanged; see tests/CARDINALITY_AUDIT.md.

`order` mirrors the portable script-api-core rounding, break-even, notional and
spot-rounding rules. `order_policy` supplies their Market-shaped adapters and
shared preparation/clamp operations. Generic Borrow/BorrowMut inputs retain
venue extensions without serialization; clamp diagnostics use an injected logger.
The two metadata predicates consume only a frozen stock projection in
`order_metadata.rs`, not a full ExchangeMetadata port. Unknown infinity-stop
names error; inherited Object.prototype entries return undefined (`None`);
unknown auto-downsize names return false. Metadata mutation and JS Error stack
objects are declared boundaries, not silently supported runtime features.
The mandatory `order_policy` and `order_policy_market` tests run the actual
TypeScript modules, inspect native numeric bits and output fields before any
wire assertions, and pin independent feeder/compared counts. The scripts use
`SAFE_CEX_TYPESCRIPT_PATH` with the existing source loader. Deliberate compiled
regressions are restored by `scripts/order-policy-controls.py`; failures must
include the named native assertion, never only a compiler diagnostic.

get_balance_slice returns a borrowed BalanceSlice containing the source account or exact currency row. An available/locked row remains that shape; get_total_balance_from_balance sees its absent total and applies the source zero default. Do not fall back to the account total merely because a present currency row has a different runtime shape.

Externally tagged numeric enum objects enter through MapAccessDeserializer, whose
EnumAccess bridge invokes the enum visitor. MapDeserializer's ordinary enum hint
forwards to visit_map and cannot decode these variants. Require exactly one tag
before dispatch; the bridge alone need not consume extra keys. Raw/owned/borrowed
regressions inspect unit/newtype/tuple/record variants and numeric bits, while
shared string-union front guards still reject tagged maps.


`js_date_to_iso_string` applies the existing TimeClip boundary and returns the
native RangeError name/message for invalid dates. It renders UTC milliseconds,
including year zero and signed six-digit extended years, without a local timezone
adapter. The formatter is shared with websocket diagnostics and native DXtrade
candle bounds; no venue-local calendar copy is introduced. `tests/date_iso.rs`
compares 1,044 independently counted native bit inputs with actual JavaScript
Date construction/toISOString, including fractional, nonfinite and clip-edge
values. It does not implement object-to-primitive Date constructor coercion.

The retained JSON writer traverses the canonical UTF-16 carrier directly. It
shares the lower quotation helper and numeric/key-order primitives with prepared
JSON rendering. Undefined and nonfinite numbers remain native until the final
wire projection; typed Rust Strings normalize isolated units only at their typed
view, while explicit strict Value projection remains a separate boundary.
The retained_json_writer target compares every UTF-16 unit and a native object
containing duplicate/index/lone-unit keys, undefined, infinities and signed zero
with Node JSON.stringify. It also observes the retained input after rendering.
`python3 scripts/retained-json-controls.py` runs exclusively, injecting the former
string refusal, former key replacement, writer text replacement and a truncated
writer array. Each compiled fault must fail its named production boundary or
Node comparison and pass again after exact source restoration.
`node ../safe-cex-parity/scripts/retained-json-contracts.mjs --package=safe-cex-num`
checks the six compiler-owned writer APIs: one native prepared-value conversion
and five specific outbound classifications. Those classifications are separate
from the executed Node comparisons and corruption controls above.

`sorting::stable_sort` owns the single V8 13.6 TimSort implementation, moved
from Exchange. Generic comparator errors remain original values; infallible
callers use Infallible. Store and native JS callers share the same algorithm.
