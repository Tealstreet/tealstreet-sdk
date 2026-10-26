# P4 numeric gates

2026-10-12: sizing, currencies, balance slicing and leverage all pass fmt,
strict all-target clippy, all-target tests with ignored source differentials,
wasm build, extraction coverage, parity-check and parity-report. Each gate
passed baseline and restored runs around an independent negative control:

- fmt: malformed whitespace in a dummy function produces a rustfmt diff.
- clippy: private unused function produces the expected unused warning.
- tests: removing abs from position leverage fails the live sizing differential.
- wasm: target-specific compile_error prevents the wasm build.
- coverage: removing getSizeCurrency row fails extraction reconciliation.
- parity: nonexistent get_size_currency target fails target validation.
- report: nonexistent get_margin_currency target fails report validation.

Live sizing runs 2,211 current-source cases, with binary64 bit comparisons;
compound balance objects compare key sets and numeric bits, not object key order.
Logs: `/tmp/p4-sizing-gates.log`, `/tmp/p4-sizing-<gate>-<phase>.log`.
The earlier precision helpers additionally use the >10k current-source numeric
corpus and per-gate controls documented in the store contract evidence.

Detached adversarial review at 5499c1e6dd in `/tmp/rust-p4-sizing-review-c66e2s`
found the missing empty-currency return. Added empty-key currency fixtures for
both slice and total; removal of the guard fails the live getBalanceSlice case
(`/tmp/p4-sizing-empty-currency-negative.log`). The expanded 3,939-case corpus
also includes negative zero, NaN and both infinities for all sizing formulas.
Restored source passes that corpus and strict clippy (`/tmp/p4-sizing-expanded.log`,
`/tmp/p4-sizing-fixed-clippy.log`).

Remaining contracts/quote helpers: 1,216 current-source cases cover negative and
zero contract sizes, nonfinite values and signed zero; ticker quote/last/price
truthiness and missing market match JS, with exact bits compared. All seven
green/independent-negative/restored gates pass in `/tmp/p4-quotes-*-*.log`.
Test control adds one to base amount and fails specifically the live quote/
contract test; others inject formatting, dead-code, wasm compile failure,
missing coverage row and nonexistent mapping targets. `/tmp/p4-quotes-gates.log`
records each accepted/rejected/restored outcome.

Candle utility boundary warning uses the canonical public js_number_string:
the existing private ryu-js formatter was exposed/renamed, with no duplicate
formatter. Nine direct vectors pin JS exponent spelling, -0, integer-looking
floats and nonfinite text. Replacing its body with Rust Display fails specifically
shared_js_number_strings; restored passes (`/tmp/p4-shared-number-*.log`).
Both numeric and candle all-target tests/strict-clippy/wasm passed after exposure
(`/tmp/p4-calendar-final-*.log`), and fmt/parity passed.

Measured shared coercion family: distinct full-string Number, decimal-prefix
parseFloat and radix-prefix parseInt corpora (10,018 / 10,018 / 30,264 cases).
String(any) is fallible and recursive, with undefined and shadowed non-callable
toString tests. Actual bitget income num arrow is extracted from live AST and
executes its finite-or-zero wrapper; primitive NaN/Infinity remain distinct,
and negative zero plus numeric1e21→String→parseInt10→1 are pinned. Generic radix
rounding follows the measured V8 13.6 bounded-chunk approximation, including
fused multiplication/addition rather than exact-integer rounding.

The shared String/number-format implementation is in the lowest model layer,
with num API aliases/delegation: this avoids the forbidden types→num cycle and
removes duplicate conversion logic. All four model utility string callers now
propagate Result TypeError, including the direct-method-specific path error in
replace_url_params; accounts map that same error to their existing Result.
No production panic/unwrap or object-Object substitution remains in those
coercion paths. Store's existing scalar PnL formatter now calls the canonical
num formatter; no store private arbitrary-JSON string() caller was found.

All7 green/negative/restored gates cover types+num+store+facade. Formatting,
dead-code, shadowed-toString substitution (rejected specifically by the facade
TypeError test), wasm compile error and deleted normalization row controls pass
in `/tmp/p4-coercion-*.log`. Initial parity found unregistered cross-crate API
aliases; exact glue exemptions were added and parity/report controls then pass
in `/tmp/p4-coercion-parity-controls.log`. The separate primitive corpora pass
in `/tmp/p4-coercion-source.log`; expanded live model corpus passes in
`/tmp/p4-fallible-model.log`. Plain clippy uses the workspace's strict lint table.

## Math.log10 measurement before implementation

2026-10-12, Node v24.20.0 / V8 13.6.233.17-node.53, darwin arm64,
rustc 1.99.0 (b940084d7): **both libm 0.2.16 and platform f64::log10 disagree
with live V8 in 736 of 323,932 cases**. The generated corpus probes ±256 ULP
around Number(`1e${exponent}`) for every decimal exponent -324 through 308,
plus the two named witnesses and positive infinity. Comparison uses exact f64
bits, not a numeric tolerance. This negative measurement justifies using V8's
own ieee754 log10 algorithm rather than either available implementation.

- Input `100.00000000000006`: Node `2`, both candidates `2.0000000000000004`.
- Input `9.999999999999672e-254`: Node `-253`, both candidates `-253.00000000000003`.

This records measurement only; the V8 algorithm port and maintained differential
gate remain outstanding. No claim is made that either candidate matches V8.

## get-key presence contract

836 actual get-key.ts cases execute through isolated TypeScript and canonical
Lodash, retaining inherited identities before JSON. Null terminates vn; own
undefined stops v spelling lookup but advances vn aliases. All twelve inherited
names, own shadows, falsy values, Unicode and positive astral keys are covered.
Valid JSON escaped lone-surrogate keys are accepted by TS and refused by the
CHOSEN shared UTF-8 ingress; custom prototypes/getters are outside the adapter.
Four compiled mutations (null miss, undefined miss, own-only, vn null skip)
failed named assertions and restored green. Mutating actual source to skip null
also fails its named assertion. Host and wasm strict Clippy were run separately
and passed with jobs2. Parity check passed; extractor added/orphaned zero rows.
