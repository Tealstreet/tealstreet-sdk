# Numeric ingress acceptance and controls

The opt-in APIs are `safe_cex_num::numeric_ingress::{js_from_str, js_from_value,
js_from_value_ref, JsNumberInput, JsNumberOutput, js_value_as_f64}` and the
root macro `safe_cex_num::js_number_output!`. All record fields/variants must be
registered, including skipped and serialize-renamed fields. Manual implementations
and custom deserializers require audit for temporary lossy buffers. Explicit
buffers and generic JSON payloads use `JsNumberInput`, then `decode` or
`decode_with`. The latter supports private wire-model callbacks returning the
registered public model.

The mandatory test runs live JSON.parse and compares binary64 bits before any
JSON.stringify projection. Node 24.20.0 / V8 13.6.233.17-node.53 and Node 26.7.0 /
V8 14.6.202.34-node.28 produced identical 2,019-row oracle output, SHA-256
`aeb05424e1e98cb2193e0b776061fc5277ef1e55ed60866186f203920a8f4e32`.
There are 50,475 numeric path comparisons per runtime: raw, owned and borrowed
entry through scalar, plain, tagged, untagged, flattened, nested second buffers,
generic typed and shared-buffer payloads; plus the explicit Value number accessor.
All 2,019 spellings remained accepted after strict map/sequence dispatch changed.
The corpus includes integer and float negative zero, overflow, negative
underflow, fractions, exponents, subnormal boundaries and integers beyond 2^53.

This is a shared-mechanism matrix, not a claim to have executed all venue fields.
Venue owners must rerun their committed production witnesses and flip their own
rows. Existing production decoders are untouched: **the clean regression guards
should have no behavior change until owners opt in and remeasure bits** (binance
nine fields and hyperliquid triggerPx, for signed zero only). GRVT InstrumentLite
bd/qd/fi were measured on an uncommitted candidate, not a shipped declaration;
retain that evidence as a regression fixture when the type is ported. Raw decoding to
Value is not globally declared unsafe. This typed adapter emits numeric visitor
events for Serde Content; its downstream Value visitor would project infinity to
null, so generic payloads require the shared buffer.

Compiler diagnostics are pinned with trybuild 1.0.116 for generic Value, skipped
Value, serialize-renamed Value and omitted registration fields. The positive
Envelope<JsNumberInput> fixture executes signed-zero/overflow/underflow assertions.
CI wiring pins all four diagnostic fixtures and the positive fixture. A stock
native-infinity Value visitor is deliberately retained as a named negative
control proving the null projection. Opaque tagged/untagged/flattened Value
wrappers separately exercise the native-event output guard inside private tests;
these bypass eligibility only to test that additional guard.

Three production-source mutations were applied separately, compiled and failed:

- Exact u64 before binary64 rounding: `Number rounds before integer target`.
- Raw/owned depth budget raised above 128: `raw capture recursion boundary at 128`.
- Native-event output guard bypass: `opaque Value projection consumption guard`.

All were restored before the final Node 24/26 runs. Depth tests compare ordinary
serde_json acceptance at 0, 1, 126, 127, 128, 129, 200 and 256 on raw, owned and
borrowed paths. This retains the CHOSEN shared recursion boundary. Number sites
use f64; string sites retain strings; actual BigInt sites use the BigInt primitive.
An integer output never preserves precision that JavaScript Number already lost.

Dependency audit: local serde_json raw_value capability only, plus serde and the
existing graph's pinned trybuild as a dev dependency. Resolved before/after graph:
zero added packages, zero removed packages, zero version changes, zero feature
changes. The only lock changes are safe-cex-num's serde/trybuild dependency names.
The workspace serde_json declaration and tradetunnel are untouched.

Final scoped verification: host strict Clippy PASS; wasm32-unknown-unknown strict
Clippy PASS separately (lint coverage, not wasm execution). The mandatory target
passes seven integration tests, including four pinned compiler refusals and one
executed positive compiler fixture; six library tests include the output guard.
Wiring/layer suite: 224 tests, zero skipped. Parity: 7,334 rows / 2,185 public
items. Extractor: zero additions/orphans/drift, using the existing isolated
worktree install (not a claim about a root clean install). Exemptions parsed with
tomllib: 323 blocks / 323 distinct, zero blank reasons. Registration and CI edits
are append-only; the workspace serde_json entry remains unchanged.

## Output adoption coverage

The shared-output census records 45 conservative registrations and eight blocked
Value/capability graphs with source sites; it includes construction-only sites
and is not an exact compiler-resolved list of required record fields. Every
registration exhaustively destructures its fields or variants. A shared field
addition therefore fails compilation at this central check until reviewed.

The existing 2,019 live Node spellings now also cover flattened shared
PlaceOrderOpts (all six numbers), flattened OrderBook (both sides, all three
numbers per side), tagged declared opaque passthrough (typed sibling plus nested
Value), and nested two-element tuples. Raw, owned and borrowed entry are checked
by bits. These add 102,969 comparisons to the original 50,475 per runtime.

The named passthrough preserves public Value/Option<Value> by capturing
JsNumberInput before construction. A negative control deliberately uses the
stock Value visitor after tagged buffering; native Infinity projects to null and
must fail `numeric ingress lost Number at $.raw_response.n`. A direct unit
control refuses native NaN before JSON construction. The declaration does not
prove helper pairing: decoder and wire-shaped serialization attributes must be
audited together, including absence of skip/asymmetric output renaming.
Unmarked Value, &Value and a tuple Value element remain pinned compiler refusals;
the skip/rename/omitted-field controls remain mandatory.
