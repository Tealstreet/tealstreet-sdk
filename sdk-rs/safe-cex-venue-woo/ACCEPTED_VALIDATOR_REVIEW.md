# Reviewed validator contracts

This cohort adds 30 individually reviewed free functions: eleven public
validators plus `required_stats_string`, ten private read validators, seven
regular-write validators, and the legacy `deserialize_present` helper. It keeps
the preceding 62 enum/record reviews unchanged. It does not clear the remaining
Woo owners, aliases, construction reviews or transitive dependencies.

The live-source expectation script executes the actual TypeScript helper bodies,
loaded with their maintained runtime imports. Its authored raw-text corpus covers
missing versus null, booleans, empty/whitespace strings, radix and exponent
strings, decimal syntax, signed zero, safe-integer limits, overflow, underflow,
arrays, records and parameterized minimum/allow-empty/allow-zero behavior. The
two generic serde helpers have separate concrete String/Value observations;
these make no universal deserializer/type or parent-field-omission claim.

Native adapters inspect actual return values. They retain numeric bits, string
and boolean payloads, optional presence, record length/selected fields, ordered
array slots, and the allocation identity of public borrowed maps/slices/values.
Errors preserve their actual path and message. No DTO serialization supplies a
native observation. The 1912 cases are independent of the Rust implementations: 1896 live-source
expectations and 16 concrete presence cases.

The public finite-number helper deliberately performs JS Number coercion;
private finite-number helpers accept numeric events. Public decimal strings
permit the source Number syntax, while private decimal strings require the
source decimal pattern. Regular-write `require_decimal` checks syntax and
nonzero digits without a Number-finiteness check: a 310-digit decimal string is
therefore retained. These differences are preserved. Minimum NaN, signed zero,
and optional-absent behavior are tested according to each actual helper.

Run from the repository root, with pinned SDK tools selected by the runner:

```sh
node sdk-rs/safe-cex-venue-woo/scripts/accepted-validator-oracle.cjs --check
node sdk-rs/safe-cex-venue-woo/scripts/accepted-validator-contracts.mjs
node sdk-rs/safe-cex-venue-woo/scripts/accepted-validator-contracts.mjs --control
node sdk-rs/safe-cex-venue-woo/scripts/accepted-validator-contracts.mjs
node sdk-rs/safe-cex-venue-woo/scripts/accepted-validator-contracts.mjs --target=wasm32-unknown-unknown
```

The runner uses ordinary complete compiler discovery, selected actual Cargo
artifacts, authenticated macro/number-provider origins and selected source-scope
checks before the maintained production `runProbes`. It retains all 645 owners,
10 aliases and nine construction obligations in discovery, then requests these
30 complete function APIs for native evidence. The existing exact `sorted`
local-function review remains load-bearing. Every Cargo invocation uses one job.

The control changes only copied production functions after unchanged contract
validation: each actual body is short-circuited to an error. Original bodies stay
in the compiled item so their dependencies remain referenced under warnings as
errors. Every changed function must compile and fail its named native observation;
a compile failure is not evidence. A fresh baseline then proves restoration.
Real production sources remain byte-identical throughout.
