# Kraken REST read receiver review

The concrete receiver is the `Scope<KrakenExtra>` held by a normally constructed
`KrakenExchange`. The unit witness is a child of its binding module solely to
access that existing private receiver; it reuses the production-binding tests'
terminal runtime/HTTP fixture. No replacement ReadHost implementation, fake
class methods, public test accessor or shared guard relaxation is introduced.

The canonical master inventory at `985f8505dc3f97996df02b4442ec420e3c41e3dc`
refuses 25 methods. This first cohort reviews the seven `rest_account::ReadHost`
methods. Each is a specific delegation to shared JS operations or the existing
HTTP/Store adapter, rather than a new record/model decoder. The other 18 methods
remain refused and owned by this lane; no alias census or whole-crate GREEN is
claimed before they are covered.

| Method | Actual native observation |
| --- | --- |
| keyed | Ordered literal/snake/camel lookup, inherited precedence, own Undefined, original object identity, lone UTF16, negative zero, infinity, getter error and primitive refusal. |
| starts_with | Native startsWith receiver, matching/missing/empty prefix, lone UTF16 and primitive/nullish method errors. |
| includes | Native case-sensitive includes receiver, matching/missing/empty needle, lone UTF16 and primitive/undefined method errors. |
| current | Real exchange accessors, real disposal, separate public-only construction, current Store snapshot cardinality, live latency update and absent slot. No array identity claim; the shared Store cutover remains separately owned. |
| preference | Both actual Map caches, lone UTF16 symbol, native object/NaN cell identity and missing entries. |
| read | Actual assembled signed xhr, GET fills query, ordinary GET without params, encoded POST body, Promise data identity, infinity and lone UTF16 response fields before JSON. |
| error_message | Original truthy object/string/infinity cells and fallback for null, Undefined, empty text, false, NaN and negative zero. |

`packages/safe-cex/src/utils/get-key.ts::v` uses `in`, including inherited
properties, and refuses primitive receivers after the nullish early return.
The previous Kraken host used `has_own`, incorrectly allowing an own camel-case
key to beat an inherited snake-case key. The actual constructed-receiver test
fails at that precedence assertion on the old body. The direct cutover uses
shared `library::in_operator`, matching the existing retained Kraken ticker
lookup; no local coercion or alternate path is added. A Node run against the
live TypeScript confirms inherited precedence and primitive TypeError.

The other source operations correspond to `kraken.exchange.ts`/`krakenspot.exchange.ts` member reads, cache get calls, signed xhr calls and
`err?.message || fallback`. The wire assertion includes signing's normal query
encoding and parameter clearing, rather than expecting pre-interceptor config.

Run the native witnesses from `sdk-rs`:

```sh
cargo test --locked --jobs 1 -p safe-cex-venue-kraken --lib accepted_read_receivers
python3 safe-cex-venue-kraken/scripts/read-receiver-controls.py
```

The control script changes each real production method independently. A control
counts only when it compiles and the method's named native assertion fails;
compiler errors and zero-test runs are rejected. It restores exact source bytes
in `finally` and requires the same named witness to pass after every restoration.
The receiver policy pins the stock rustdoc impl-item hash. The two existing
nested qs helper file pins are refreshed because this reviewed lookup change
alters their containing file; their bodies and roles are unchanged.
