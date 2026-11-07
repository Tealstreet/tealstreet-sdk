# P4 model utility gates

2026-10-12: fmt, strict all-target clippy, all-target tests with ignored live
source oracles, wasm build, extraction coverage, parity-check and parity-report
passed baseline/negative/restored. Independent probes: malformed formatting,
unused private function, retaining __proto__ as an own cloned key, wasm-only
compile_error, deleted fastClone row, nonexistent fast_clone target and
nonexistent clone target. Logs: `/tmp/p4-model-gates.log` and
`/tmp/p4-model-<gate>-<phase>.log`.

Model helper oracle pins 107 cases, including integer-key ordering, duplicated
inverse values, nested clone/prototype setter behavior, URL replaceAll dollar
substitution and inherited property lookup. Symbol oracle pins 48,384 cases:
47 exchanges × all exchange pairs and both directed conversions for 21 strings,
plus parsing. Clone is limited to the finite JSON model; Date/Map/Set and runtime
prototypes are outside Value. Public notes state this representation boundary.
Integer-key order has an independent vector assertion; compound model values
compare key sets and numeric bits, so that comparator does not detect key order.

Adversarial review in `/tmp/rust-p4-model-review-c66e2s` at 9a1b307170 found
lodash's forbidden own-key omission and integer-key ordering; both are preserved
and independently checked. It also identified shadowed non-callable toString:
Rust now rejects it with the same source coercion reason. Live corpus grows to
114 cases. Independently retaining forbidden keys and accepting shadowed
coercion each fail (`/tmp/p4-model-security-negative.log`,
`/tmp/p4-model-coercion-negative.log`); restored three tests pass in
`/tmp/p4-model-review-restored.log`. Strict clippy passed after fixes.

Remaining seven standalone types rows pass all7green/negative/restored gates
(`/tmp/p4-extra-gates.log`, `/tmp/p4-extra-<gate>-<phase>.log`). Tests compare
all27 timeframe entries in source order and all3read-only statuses, distinguish
the mutable store projection from full StoreData, and round-trip typed HTTP/WS
signer messages. Signed body has two keys and signed params preserve signature
last; all7WSformat spellings are checked. Test negative changed 10h duration
from36000to999 and the live constant comparison rejected it. Other controls are
formatting/unused code/wasm-only compile_error/deleted timeframe row/missing
constant target/missing WritableStoreData target.

Core merge7518bacdc8 (6757937367) supplied the canonical RemoteSigner split:
transport signing plus exchange-core availability. The duplicate complete
capability/protocol models introduced in a68fc088f0 and their standalone wire
round-trip test were removed on merge. The earlier evidence describes that
historical commit, not current additional signer coverage. Current model,
constant, producer and facade gates passed after merge in `/tmp/p4-seams-*.log`.
DisplayedPosition key is checked by the actual producer/facade seam, including
an independent missing-field compile control; see safe-cex/GATE_EVIDENCE.md.

Accounts: live source corpus covers 14,785 schema/build/normalize/import/validity
cases across all current metadata exchanges, constructor/__proto__ inherited
property acceptance, aliases, proxy truthiness, CORS/testnet precedence and
unknown saved fields. Numeric values compare binary64 bits, objects compare key
sets, arrays preserve order; this comparator does not gate object key order.
The private metadata/default-region projection has a live source drift check.
No layer or serde_json feature change was made.

All seven green/independent-negative/restored gates pass in
`/tmp/p4-accounts-<gate>-<phase>.log`: formatting probe, unused private function,
wrong name-validation error observed by the live corpus, wasm-only compile
error, deleted supported-names row, missing Rust names target and missing schema
target. Summary: `/tmp/p4-accounts-gates.log`.

Importer divergence is explicit: Rust String cannot retain lone UTF-16
surrogates (FORCED); serde's 128-level parser limit is retained (CHOSEN), rather
than allowing unbounded recursive imports. Native tests pin rejection and live
source tests pin JS acceptance; `/tmp/p4-accounts-boundaries.log` passes. These
are not ported claims. Canonical input/output stay open JSON to preserve
unknown saved fields and values admitted by runtime casts. JS non-JSON object
identity and undefined are represented by ownership and absent properties.

Accounts adversarial review `/tmp/rust-p4-accounts-review-c66e2s` at de53730e24
found raw integer-key enumeration ordering was masked by the serialized JS
inputs and key-set comparator. Recursively restore ECMAScript integer-key-first
ordering (excluding 2^32-1 and zero-prefixed strings). Independent raw-input
key-vector assertions check outer and nested saved fields. Replacing sorting
with a comparator that always returns Equal fails specifically at
`raw_import_integer_keys_follow_js_enumeration_recursively`; restored all three
accounts tests pass. Logs `/tmp/p4-accounts-order-{negative,restored,clippy,wasm}.log`.

## Isolated oracle dependency audit

2026-10-12: repaired root-only TypeScript resolution in constants/symbol and
facade position-producer oracles, root-only Lodash resolution in model/store
oracles, and external-module loading in accounts-source. The existing isolated
CI dependency install now includes the source SDK's Lodash selector and frozen
repo lockfile. No new install or skipped oracle was added.

All-target tests (including ignored live oracles) for types, num, store and the
facade passed with SAFE_CEX_TYPESCRIPT_PATH pointing to a fresh isolated install
and a Node resolver guard rejecting this worktree's root node_modules. Replacing
the symbol oracle's isolated TypeScript lookup with its old root lookup failed
specifically on the resolver guard; restoring it passed. Logs:
`/tmp/p4-isolated-oracles.log`, `/tmp/p4-isolated-install.log`.

## Strict shared enum and record codecs

2026-10-12: inventory: **36 inhabited string enums and 62 named records in
safe-cex-types, plus two store-contract records**. Four uninhabited capability
markers retain their original implementation and cannot be exercised; they are
neither observed widenings nor measured strict codecs. Output-only account
error/result records have no Deserialize contract. PackedTickerDelta remains an
array by source design. This inventory is broader than the gate owner's 78
observed widened shared-type owners and must not be conflated with that census.

Every inhabited enum has a serde-generated accepted-spelling inventory and
rejected shape probes. Twenty-four named/inline unions (135 spellings) are also
compared directly against live TypeScript ASTs. The remaining private tags and
model unions retain their existing canonical spelling sets, including the
already-disclosed outbound gateway auth cast. Records reject scalar and array
shapes with specific invalid-type diagnostics, rather than missing-field errors.
A syn inventory checks all private companion container/field/variant metadata
against the canonical declarations, detects newly derived public records/enums,
and prevents drift in the generated companion copies.

Types/store/facade fmt, strict all-target clippy, all-target tests including
ignored live source oracles, wasm, extraction coverage, parity check and parity
report passed with independent negative/restored controls. Probes: malformed
formatting, unused private helper, disabling deserialize_map, wasm-only
compile_error, deleted StoreUpdate row, and nonexistent StoreUpdate Rust target.
The disabled-map test control fails BOTH the bare-array StoreUpdate test and the
complete22-field witness, whose private companion accepts the exact same array.
The first script expected a types failure, but Cargo ran store tests first;
its diagnostic check was corrected to that specific store assertion, then the
restored suite passed. A fixture's wrong field types were caught by review and
corrected before this successful control; it is not counted as evidence.

An injected BUY alias on the actual private OrderSide companion fails its
accepted-spelling metadata assertion. Three compile-fail visibility doctests each
have an independent control replacing ONLY their private path with the matching
public type; each then fails because it compiled successfully. All restored
doctests pass. No inert error-code annotation is used. Both isolated adversarial
reviews returned MERGE after the complete-array fixture correction.
Logs: `/tmp/p4-strict-gates-rest.log`, `/tmp/p4-strict-controls.log`, and per-gate
`/tmp/p4-strict-<gate>-<phase>.log`. Builds/tests now use --jobs 2; reviews and
parent builds are serialized.
Progress reporting also passed, rejected an independently malformed StoreUpdate
manifest TOML, and passed after restoration. Explicit case-change rejection
probes were added for every inhabited union and revalidated with its targeted
suite and strict all-target clippy.

## UTF-16 replacement dependency contract

The public js_coercion::js_string_replace_utf16 backend is published separately
at the user's request to unblock num's already-prepared 33,780-case oracle.
It retains UTF-16 units and dollar-token expansion over the original source;
empty patterns advance one unit, including between surrogate halves. The private
model replacement helper delegates and returns RepresentationError if its UTF-8
wrapper cannot carry the result; URL patterns here are nonempty brace tokens.
Scoped fmt, strict clippy, focused backend tests and parity check pass. Advancing
empty matches two units fails the named empty_pattern_advances_by_utf16_units
test; restoring one-unit advancement passes. Full wrapper differential/wasm and
remaining gate controls belong to the downstream num landing and are not claimed
as executed for this urgent dependency-only contract.

## Canonical journal position record

The position_record gate extracts all 24 fields and five string unions (three
named plus the inline leg/state unions) from the actual TypeScript declaration.
Serde's generated accepted-spelling inventory is compared to that source set;
object/array/null/number/boolean shapes, unknown literals, case changes, BOM and
whitespace are rejected. Complete positional arrays and null at every optional
non-null field are permanent negatives; public direct-call and qualified trait
paths are exercised. Private wire companions carry canonical spellings once.

The facade numeric gate measures 84 Node cases over all 12 numeric fields: raw
-0/-0.0, decimal/exponent, positive/negative overflow and negative underflow.
Raw, owned lexical Value and borrowed Value results are compared by native f64
bits, with native Value(-0.0) positive controls. This uses the shared numeric
ingress and an exhaustive PositionRecord eligibility graph, not a Value-first
shim. This is a type contract; no fold, buildRecords or venue adapter is claimed.

## Runtime validation error values

ValidateAccountResult.error now uses AccountValidationError, a retained JSON
newtype without coercion. The actual KucoinSpot.validateAccount AST runs with
prepared xhr response.data (not an Axios raw-response approximation):32 cases
cover rejection/catch paths, numbers, objects, arrays, booleans, strings,
fallbacks, signed zero, overflow and nested numeric bits before serialization.
The generated error:string is the source static declaration projection and
carries a comment naming ts-bug:validation-error-value and its manifest row.
A native from_value-to-string mutation compiled and failed VALIDATION-ERROR
actual method retains runtime value; a source String(data.msg) mutation executed
and failed the same assertion. Restored test passes. Map-only record negatives
and optional non-null balance remain enforced. This does not repair Kucoin
callers, core raw-value emitter inputs, non-JSON functions or alias identity.

## Retained model info

The actual maintained DefaultStore corpus covers five models across thirteen
presence/native-value profiles, four patch paths across the same profiles, and
twenty Number/parseFloat/String profiles. The production Store facade and shared
native codecs retain NaN bits, infinities, signed zero, own undefined and lone
UTF16 text/keys before JSON.stringify projection. The scoped compiler inventory
contains 230 owners; twelve selected production decoder owners and five outbound
classifications pass the actual native contract runner. No decoder exemption is
used for the new info visitors or native payload decoder.
