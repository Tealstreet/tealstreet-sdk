# Shared JavaScript primitive gates

## Truthiness

Compared all seven located copies before consolidation: sizing; types model_utils
and accounts; crypto; transport how; store; exchange error_reporting.
Types' `is_none_or` and crypto's parse fallback agree with Node on overflow.
Sizing, transport, store and error_reporting use `is_some_and`, incorrectly
returning false for arbitrary-precision `1e400` / `-1e400` (Node: true).
All agree on null, booleans, strings and truthy arrays/objects.
Sizing now delegates to `js_truthy`; outside-crate consumers must be migrated by
their owners. Types cannot depend on num without a cycle; relocation to the
lowest layer requires the types owner's change.

Live oracle: Node v26.7.0, V8 14.6.202.34-node.28, ICU 78.3.
Generated truthiness corpus: 1,635 inputs including undefined, BOM, overflow,
underflow, signed zero, nested arrays, objects and numeric/string variants.
Negative control: restored the plausible `as_f64().is_some_and(...)` branch;
`truthiness_matches_live_node` failed at its equality assertion on `1e400`.
Restoring the parse fallback passed. Sub-agent review: no findings.

## Number(any)

1,680 generated JSON/undefined inputs measured against Number and unary +;
400 executable-hook cases measured against live Node for ordinary and exotic
conversion ordering, primitive results, thrown errors, BigInt and Symbol.
Shared JsCoercionError preserves error identity; no panic substitutes a throw.
Callable hooks use a host adapter because serde_json cannot contain functions.
Sweep included unary + (e.g. Binance fill fields, Bitrue tickers and Orderly
parentId/trigger_price), not just Number calls.
Negative control substituted parseFloat for array/object Number conversion;
`number_any_matches_live_node` failed at its assertion on [] (NaN versus 0).
Restored tests and strict clippy passed; follow-up sub-agent review clean.

## Math.round publication

`math_round` exposes the existing implementation; `display_round` names the
unrelated rendering helper, with `round` retained as a compatibility alias.
20,016 live-Node cases compare bits across half-integer neighbors, arbitrary
binary64 patterns, nonfinite values, negative zero and BOM-coerced input.
Negative control replaced the implementation with Rust round: the named test
failed on -0.5 (-1 versus -0). Restored oracle and strict clippy pass;
sub-agent review clean.

## Own keys

Before consolidation, compared transport/how::normalize_object_order with
accounts::normalize_numbers' recursive object arm. Both parse u32 and exclude
u32::MAX and spellings unequal to index.to_string(); both retain ordinary-key
insertion order (transport partitions; accounts uses stable sorting). No ordering
disagreement found. The new shared functions use that identical canonical-index
predicate and stable sorting, verified by 300 generated live-Node objects plus
nested normalization. External delegation is routed to those owners; types
requires lowest-layer hosting to avoid the num dependency cycle.
Review caught missing recursive assertions; added unordered descendants inside
objects and nested arrays. Negative control admitted u32::MAX as an index;
`own_key_order_matches_live_node` failed at equality on 4294967295. Restored
oracle and strict clippy pass.

## JSON.stringify

1,914 live-Node byte comparisons cover every decimal exponent -324..308,
negative zero/nonfinite numbers, arbitrary-precision JSON numeric input, own-key
ordering, BOM/control/astral strings, duplicate assignments, nested undefined
omission and array null substitution. BigInt returns TypeError. The owned
prepared-data representation deliberately excludes callbacks/replacers/cycles;
it retains missing fields independently of a caller's signing representation.
Negative control emitted undefined members as null (the msgpack-like mistake);
`stringify_matches_live_node_bytes` failed at the first object byte assertion.
Restored oracle and strict clippy pass; sub-agent review clean. Core websocket
stringify delegation remains out-of-scope and routed to its owner.

## Math.log10

Source: V8 13.6.233 src/base/ieee754.cc (log10 and its internal log), Sun/V8
license retained in the port. 333,938 generated cases pass on Node v24.20.0 /
V8 13.6.233.17-node.53 AND Node v26.7.0 / V8 14.6.202.34-node.28, darwin arm64.
Includes ±256 ULP around decimal exponents -324..308, both named witnesses,
nonfinite/signed-zero/negative inputs and 10,000 generated binary64 patterns.
Literal C++ expression translation initially disagreed at 9.999999999999672e-245.
Clang arm64 assembly confirmed contracted polynomial and reconstruction FMAs;
explicit mul_add reproduces the measured reference and is stable in Rust builds.
CHOSEN reference: Node darwin arm64. Other V8 compiler/architecture contraction
settings may have different last bits; this is not a claim of universal V8 parity.
Negative control substituted platform f64::log10 for finite inputs; the named
bit assertion failed on 100.00000000000006 (2.0000000000000004 versus 2).
Restored corpus, strict clippy and wasm target check pass. Sub-agent review
found no arithmetic defects; public docs now explicitly name the arm64 reference.

## Math.pow — corrected implementation premise

Live Node --v8-options and nodejs/node v24.20.0 deps/v8/src/numbers/ieee754.cc
show use_std_math_pow enabled by default. The legacy fdlibm function is NOT the
measured execution path. Its port disagreed in thousands of generated cases.
The enabled V8 path wraps std::pow with JS NaN/infinity handling, direct x*x
for exponent 2, and sqrt(x+0) for 0.5 (including negative-infinity handling).
Matching that path passes 228,973 bit comparisons: every binary exponent's
neighbors, cross-products of special values, subnormal squares, MAX_VALUE^0.5,
generated random base/exponent pairs and BOM numeric input.
CHOSEN host profile: general finite powers inherit platform libm, as this V8
path does. Other targets' libm implementations require their own differential;
a wasm compile check alone is not cross-target numeric parity evidence.
Both Node24.20 and Node26.7 pass. Negative control replaced finite-input paths
with bare powf: 1,030 bit mismatches, starting at MAX_VALUE^0.5 with the exact
reported 5ff0000000000000 versus 5fefffffffffffff witness; named final assertion
failed. Restored tests and strict clippy pass; sub-agent review clean.

## Integration merge after item 7

Merged parity-data first, then integration dfa2ff2318 per hunk. Named CI step
counts: parent bb89097c55 = 55, integration parent = 52, result = 55; both parent
step multisets retained. Both exemption sets retained, 228 unique records.
CI wiring plus live/populated layer audit: 101 passing tests. All seven new
primitive test targets (8 tests) pass after merge. Venue dependency removals
apply only to empty scaffolds (all 26 have zero noncomment source lines).
Sub-agent independently confirmed step/exemption preservation and no markers.

## BigInt

10,156 generated JSON/primitive cases plus 400 executable-hook cases pass live
Node, comparing bigint digits, error classes, messages and exact UTF-16 units.
Includes every decimal exponent, signed/radix/whitespace strings, BOM/NEL,
noninteger/nonfinite Numbers, nested arrays, non-callable toString and throwing
hooks. Review discovered V8 diagnostic truncation at 1,000 UTF-16 units; added
999/1000/1001 and astral split-boundary witnesses, retaining exact UTF-16 errors.
Follow-up review clean. Negative control changed numeric RangeError to
SyntaxError; `bigint_matches_live_node_error_classes` assertion failed on 1.5.
Restored oracle and strict clippy pass.

## Item 9 — root collation for signing

`JsRootCollator::try_new` selects `und` explicitly and returns initialization
failure; `compare` has no fallback. CHOSEN profile: well-formed Unicode strings,
ICU4X 2.3.1, generated Unihan root tables from CLDR 48.2.1 / ICU 78.1rc, measured
against Node 24.20.0 and 26.7.0 (both ICU 78.3 / CLDR 48.0). The Node oracle uses
explicit English standard collation, which uses root data; Intl `und` would
fall back to a host default. This is not a claim about every Node/ICU version.

Both ICU4X 2.1.1 and 2.3.1 stock data failed U+30D2F versus U+9A68: Node sorts
the former first, implicit-Han data the latter. ICU4X source defaults
`CollationRootHan::Implicit`; ICU's standard root uses Unihan radical-stroke.
Replacing only the root exposed a Jamo offset overflow on U+0000 `10` versus
U+117C U+3C321. All seven generated collation tables must stay coherent. Both
witnesses remain in the corpus.

`js_collation` passes 165,422 pairs on each Node version: 100k generated strings,
adjacent BMP scalars, and full seed pairs covering punctuation, digit strings,
case, accents, canonical equivalents, invisible characters, Han and Jamo.
Data gates assert actual resolved crate metadata plus the complete generated provider
SHA-256 and the root/Unihan profile (spelled `unihan` in the artifact).
The regeneration script checks the executable's actual version before writing.
Review caught and fixed an earlier unchecked generator-version attribution.

Controls: substituting byte comparison fails `root_collation_matches_live_node`
at an equality assertion; changing the recorded Han profile to implicit fails
`root Han data choice`; substituting Python for the generator is rejected before
generation. Restored live tests pass; strict scoped Clippy, wasm library check,
formatting and diff checks pass. Generated data carries the Unicode license.

The dependency audit retains before/proposed/final cargo trees and metadata,
per-feature reasoning and the rejected 2.1 patch. That proposal downgraded six
packages; the accepted 2.3 graph changes zero existing versions and exactly 14
feature sets. Two expansions do change unused dependency APIs (locale parsing
and owned payload storage), but existing IDNA uses borrowed baked data. The
57-case live Node URL oracle passes on baseline, rejected 2.1 and accepted 2.3.
See `tests/dependency-audit/README.md` for the durable reproduction.

A later explicit scope grant permits append-only own exemptions and CI targets.
Collation's three public items are registered, and `js-primitives` runs the
mandatory `--test js_collation` target on Node 24.20.0. The wiring audit rejects
removal, conditions, soft failure, and replaced commands. Existing step names
are retained (55 before, 61 after).

The parity index intentionally refuses declaration-producing item macros.
Generated provider macro arms are therefore expanded to checked-in private Rust
declarations by the pinned regeneration script. Data is unchanged; the checksum
now covers the resulting provider source. This avoids weakening the index gate.


## Added seam — Math.min / Math.max

Public two-Number `math_min` and `math_max` propagate NaN and deterministically
select negative/positive zero respectively. `js_math_extrema` compares 63,121
operand pairs with live Node: special values, both operand orders, equal inputs,
random bit patterns and U+FEFF at the Number boundary. NaN payloads are not a JS
observable guarantee; zero signs and all non-NaN results are compared by bits.

Replacing both helpers with Rust extrema assertion-fails at Math.min(NaN,1).
Using the left operand's zero sign for max assertion-fails at Math.max(-0,+0).
Restored oracle passes. Scoped strict Clippy, wasm check, fmt, all 111 CI/layer
checks and parity (6,906 rows / 1,105 public items) pass. Own exemptions and the
mandatory `--test js_math_extrema` CI step are appended with bypass controls.
Sub-agent review: MERGE.

## Added seam — Date.parse and TimeClip

`date.rs` transcribes Node v24.20.0 V8's DateParser tokenization, ISO-first and
legacy fallback grammar, composers and timezone arithmetic; V8's BSD license is
retained. Gregorian day calculation uses equivalent integer civil-date arithmetic.
Reference: https://github.com/nodejs/node/tree/v24.20.0/deps/v8/src/date

`js_date_parse` accepts Unicode strings; `js_date_parse_utf16` also preserves JS
NUL and unpaired-surrogate behavior. Both accept an explicit local-to-UTC host
adapter; ISO date-only and explicit timezone forms bypass it. Invalid input and
out-of-range pre-UTC dates never invoke it. Host DST disambiguation is the
adapter's responsibility, not claimed by fixed-offset parser measurements.
CHOSEN reference profile: measured Node V8 with 32-bit signed Smi timezone bound.
A narrower initial bound rejected GMT+123456789:00; Node preserves wrapped
2,062,808,912 seconds. This architecture/configuration detail is now explicit.

The live oracle passes 30,161 parse cases and 10,013 TimeClip values per timezone
(UTC and UTC+8), on both Node 24.20.0 and 26.7.0. Generated valid/invalid ISO and
legacy forms include overflowing calendar days, comments, named months, long
numerals/fractions, signed years, timezone variants, U+FEFF/NEL/line separators,
NUL and surrogate units. Adapter-call tests verify bypass, cutoff and invalid
adapter results. `js_time_clip` maps finite in-range values to truncated integer
milliseconds and positive zero; nonfinite/out-of-range values become NaN.

Controls assertion-fail by name: truncation alone fails TimeClip(-0), ISO-only
parsing fails Date.parse("0"), and treating local time as UTC fails the adapter
call test. Restored tests pass. Strict scoped Clippy, wasm check, formatting,
116 CI/layer tests and parity (6,906 rows / 1,108 items) pass. Three own exemptions
and mandatory `js_date` target are appended. Sub-agent source review: MERGE.

Out-of-scope finding: store `virtual_clock::date_ms` uses truncation alone and
therefore retains -0 for -0, -0.5 and -1e-300; live new Date(value).getTime() returns
+0. Store delegation to `js_time_clip` is routed to its owner, not edited here.

## Added seam — Lodash snakeCase / camelCase

The repository lock resolves lodash **4.18.1**. The live oracle asserts that
version and SHA-256 `f5465f55566bf544aad0a31c6135889ca1ed81eea8f53ec61c6cbe86926f07cf`
of its actual `lodash.js` before executing it. The port preserves the dependency's
190-entry deburr table, explicit character classes, ordered word alternatives,
ordinal boundaries and Unicode-symbol capitalization. Its license is retained.
These are dependency semantics, not ECMAScript builtins. No Rust dependency added.

Each Node 24.20.0 and 26.7.0 run compares 1,148,148 strings: every Unicode scalar
with casing context, full seed pairs and 30,000 generated strings. Both report
Unicode 17.0. A Fitzpatrick-pair/ZWJ witness initially exposed a difference between
word splitting and upperFirst symbol grouping; the final implementation matches.
Substituting non-backtracking acronym scanning assertion-fails the named live
comparison. Restored tests pass. A permanent child test executes an in-memory
mutation of the actual lodash compounders and requires the named comparison
failure; omission of the mutation was observed to fail that control itself.
Missing isolated lodash fails MODULE_NOT_FOUND rather than finding an ancestor.

The existing `safe-cex-types/ci-deps.cjs` selector already includes lodash. Its
frozen install was used for validation, including a disposable tracked-source
oracle mirror outside all worktrees. Every ancestor was checked for node_modules;
all four tests passed and the mirror was deleted. No source was authored there.
Mandatory CI uses the same selector and pins the installation and test command.
All 125 CI/layer tests pass, along with scoped strict Clippy, wasm, formatting,
and parity (6,906 rows / 1,110 public items). Sub-agent review: MERGE.

CHOSEN string boundary:
[sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary).
The live oracle records double-escaped valid-JSON lone-surrogate inputs and
Lodash outputs; the Rust ingress test records representation refusal without
replacement-character substitution. Scalar Unicode, astral pairs and whitespace
remain in the positive corpus. Memoization is an accepted omission: these pure
functions have the same output without a cache.

## Added seam — Ordinary-object table lookup

`TableHit<T>` retains Mapped, Inherited(ProtoMember), and Missing. Stock Node
has twelve inherited names, not the initial eight-name census: its four legacy
getter/setter definition/lookup methods are also callable and truthy. `__proto__`
is separately identified as an accessor returning an object. Own data properties
shadow all twelve, including __proto__; duplicate own entries use the last value.
This models reads over an unmodified Object.prototype, not prototype mutations.

Presence, truthiness and nullishness are distinct. JSON-value default methods
use Borrow<Value>; is_truthy delegates the existing js_truthy implementation.
Typed tables use closure variants, whose callbacks run only for Mapped. Zero,
false and empty string are present and falsy but non-nullish; null/undefined are
present and nullish. No enum widening is required to retain inherited identity.
Invocation and downstream retention remain consumer responsibilities: this read
representation does not execute inherited functions or serialize them away.

Node 24.20.0 and 26.7.0 each pass 40,850 lookup/branch rows plus five numeric
boundary witnesses. The corpus measures actual values before JSON serialization,
own shadows (including falsy/null/undefined), duplicate transitions, uppercase
immunity, whitespace/BOM, astral keys and generated unknown names. Separate
valid-JSON escaped lone-surrogate witnesses record the
[shared representation refusal](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary).

Three plausible substitutions assertion-fail by name: own-only lookup at
constructor, unconditional mapped truthiness at zero, and presence as the
nullish predicate at an own null. Restored tests pass. Scoped strict Clippy,
wasm, formatting, 130 CI/layer tests and parity (6,906 rows / 1,118 items) pass.
Eight own exemptions and the mandatory js_table_lookup target are appended.
Sub-agent review of the corrected three-predicate API: MERGE.

### Inherited-member String conversion

`ProtoMember::js_string` reproduces String(value) for all twelve stock V8 inherited
members. The constructor spells Object, and the prototype accessor's object
spells [object Object]. Eleven callable strings and the distinct object string
are measured before JSON serialization on Node 24.20.0 and 26.7.0. Replacing the
constructor result with its key name assertion-fails `inherited String conversion
constructor`; restored tests and scoped strict Clippy pass. This operation is
for open-string interpolation, not a claim of convergence for closed-enum record
serialization/refusal. Sub-agent review: MERGE. Existing mandatory table oracle
covers the new method; its single own exemption is appended.
