# Shared types

The bottom layer: every crate above — venues, store, transport, crypto,
exchange core — reads its wire vocabulary from here, so a type added anywhere
else is a second copy by definition. Naming, literal-union identifiers and
collision rules are in [TYPE_PARITY.md](TYPE_PARITY.md); per-field provenance is
in [P1A_FIELDS.md](P1A_FIELDS.md) and gate evidence in
[GATE_EVIDENCE.md](GATE_EVIDENCE.md). Representation decisions are DECLARED once
in [`../parity/MANIFEST-SPEC.md`](../parity/MANIFEST-SPEC.md) and cited by
anchor; the closed-typed-output boundary in particular is a declaration, not a
per-row argument, and it **excludes open-String fields, which carry the wire
value**.

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

retained_json contains the canonical raw-token and numeric-event capture extracted
from Num ingress. The five shared model info companions delegate to it: absence
is None and present null is Some(Null). Existing numeric lexemes, including
overflow and signed zero, survive; native NaN is refused. This is JSON capture,
not JavaScript wire projection; Num owns typed ingress and output eligibility.

OrderRiskProfile serialization follows its source construction branch. The
loss-side limit branch places positionSize before fee; all other risk branches
place it last. Rust-only position_size_before_fee metadata selects the order,
is omitted from TypeScript bindings and wire output, and defaults false on
record decode. Numeric values and optional-field nullability are unchanged.

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
