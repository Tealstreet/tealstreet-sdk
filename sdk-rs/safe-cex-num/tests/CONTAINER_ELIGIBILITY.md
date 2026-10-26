# Numeric output container eligibility

Audited on integration aa632b5578 plus GRVT records 01adf13f84. Conservative
source-spelling scan of shared types and all venue src files, including private
companions, function signatures and tests; runtime capability occurrences are
classified separately rather than assumed to be DTO fields. This complements
shared-output-census.json's named-type inventory.

| Container | Eligibility |
| --- | --- |
| Option<T>, Vec<T> | Existing recursive T bound; unchanged |
| [T; N] | Existing recursive T bound; unchanged |
| Box<T> | Recursive; now permits unsized eligible targets |
| [T], Box<[T]> | Added recursive element checks |
| () | Existing unit |
| (T,), pairs, tuples through 16 elements | Each element recursively checked; pair existed, remaining Serde arities added |
| BTreeMap<String, T>, HashMap<String, T, S> | Existing recursive value checks; unchanged |
| types::object_map::JsObjectMap<T> | Added recursive values; read-only iteration preserves JS key ordering |
| indexmap::IndexMap<K, T, S> | Added recursive keys AND values; preserves insertion order |
| &T, str, String, Cow<str> | Existing recursive/string eligibility; unchanged |
| Rc, Arc | Scan hits runtime sockets/adapters/clock capabilities, not deserialized DTO fields; no blanket eligibility added |
| HashSet, BTreeSet | Scan hits dedup/filter state and literal-audit tests, not wire DTO fields; no eligibility added |
| Result | Operation return/error paths, not wire records; no eligibility added |
| Box<RawValue>, Box<dyn Fn> | Shared ws-auth raw payload and runtime callbacks remain ineligible; Box never excuses its target |

No VecDeque, LinkedList, IndexSet or SmallVec generic spellings occur in the
scanned source. This is not a promise about unmerged venue branches. New public
container choices still require a recursive registration and a refusal control.

The implementation lives with the trait in num, avoiding orphan-rule workarounds
and a types-to-num cycle. IndexMap uses the exact already-resolved 2.14.2 with
serde, identical to types: before/after full cargo metadata shows zero package,
version or resolved-feature deltas. Only num's direct lock dependency is added.

Tests pin map-bearing records through raw/owned/borrowed checked entries against
live Node JSON.parse bits, JS integer-key order and insertion/reinsertion order.
A visit counter proves every element is traversed for both maps, nested
Box/slice/Option/Vec, arrays and every tuple arity. Compiler controls retain E0277
for Value-valued JsObjectMap and IndexMap (also a Value key), triple/16-tuple payloads and a
boxed Value slice, alongside all earlier unmarked/reference/skipped/renamed
Value refusals. No blanket Value implementation or passthrough waiver is added.

## Landing verification

All13 numeric-ingress tests pass on Node24.20.0 and26.7.0, including the unchanged
2,019-spelling matrix and48,456 new ordered-map bit comparisons per runtime
(2 maps ×4 values ×3 raw/owned/borrowed entries ×2,019 spellings). All7 library
tests pass; doctests contain zero cases. Host strict all-target Clippy PASS;
wasm32 strict all-target Clippy separately PASS, both with jobs2. Scoped fmt,
237 wiring tests and78 layer tests pass; parity passes7,334rows/2,547publicitems.

Six additional refusal expressions live in the existing pinned generic_value.rs
fixture: both Value-valued maps, a Value IndexMap key, triple and16-tuple Value
elements, and Box<[Value]>. All fail specifically with E0277 Value:JsNumberOutput,
and the plain trybuild rerun passes without overwrite. Existing &Value, generic,
skipped/renamed field and omitted-field controls remain pinned. The four-file CI
registry is unchanged: review caught and removed an initial fifth-fixture design.

A production mutation truncating JsObjectMap checking to its first value compiles
and fails `JsObjectMap visits every value`. Restored implementation passes the
full13-test suite. Final adversarial sub-agent review returned MERGE. These
registrations only make opt-in checked entries usable; existing venue decoders
are not silently rerouted.
