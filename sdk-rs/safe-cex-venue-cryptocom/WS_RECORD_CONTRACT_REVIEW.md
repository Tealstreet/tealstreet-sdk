# Crypto.com captured websocket record contracts

This slice proposes individual native/default contracts for seventeen existing
websocket field carriers and the private field projection helper. Source TS
interfaces declare fields without a runtime validator. The actual native
from_value functions project captured PayloadValue primitives into static
carriers; the contracts review that explicit boundary. Manifest statuses and
the declared static projection boundary remain unchanged. These witnesses do not claim
JSON/property-host binding, getter/prototype behavior, own-key presence, retained
object/array aliases, or complete source-runtime parity.

The existing tests/ws_payloads.rs witness checks actual source declarations
and native fields across its independent 310-scenario inventory. It remains
part of the full scoped suite; no source interface runtime validator is implied.
The manually reviewed TS schemas for this slice contain 121 declared fields. Independent
fixtures contain 618 record cases using captured primitive recipes: Undefined,
null, bool, raw f64 bits, UTF16 units, arrays and object maps. Native observers
read each field, variant, optional branch, collection length/order and tuple
component directly. Independent fixture counts and executed counters pin all
618 record cases and twenty private-field cases. They do not serialize the output DTO and compare that
serialization. Numeric observations retain negative zero and nonfinite values;
string observations retain lone surrogates. Required-field and type refusals,
Undefined optionals, explicit-null refusal, present empty values, distinct
neighboring fields, nested zero/one/two cardinalities and complete positional
arrays are authored independently of native execution. Actual Clone results
are checked by native field observations; NaN is not compared with PartialEq.

Carrier names below have the `CryptocomWs` prefix. The field and case counts
are independently authored inventory limits, not counts inferred from native output.

| Carrier | Declared fields | Independent cases |
| --- | ---: | ---: |
| TickerData | 11 | 51 |
| ValuationData | 2 | 22 |
| TradeData | 7 | 37 |
| BookData | 4 | 34 |
| CandlestickData | 6 | 34 |
| OrderData | 26 | 77 |
| PositionData | 16 | 56 |
| PositionBalance | 9 | 43 |
| BalanceSummary | 11 | 49 |
| TickerUpdate | 4 | 28 |
| ValuationUpdate | 4 | 28 |
| TradeUpdate | 4 | 28 |
| BookUpdate | 4 | 28 |
| CandlestickUpdate | 4 | 28 |
| OrderUpdate | 3 | 25 |
| PositionUpdate | 3 | 25 |
| BalanceUpdate | 3 | 25 |

BookData's captured bids/asks contain three-string tuples, matching both the
actual WS and REST declarations. The WS data record additionally has tt beside
t; both fields are observed independently. WS order type/time-in-force/status
strings remain unrestricted as declared; REST order literal unions are not
substituted.

The private field helper is reviewed separately with twenty native-kind,
missing-key, non-object, UTF16-key and borrowed-reference cases. Its actual
private function is called directly by a cfg(test) unit witness. Downstream
record bodies are not waived by that helper contract.

The 62 executed controls target every owner's native field observations and every
complete positional array. Array controls explicitly widen the shared native
field boundary by mapping each selected owner's complete array positions; they
do not claim a one-line Serde guard removal. Collection controls fully decode
before retaining an empty/subset output at twelve actual sites. Separate
private-field controls corrupt lookup and non-object refusal. Two additional
truncated-walk controls exercise the independent executed-case counters. Every control compiled, failed its named native assertion with exit 101, restored exact source
bytes, and passed again. No expectation is updated from mutant/native output.

Run sequentially from sdk-rs:

```sh
cargo test --locked --jobs 1 -p safe-cex-venue-cryptocom --test accepted_ws_record_shapes
cargo test --locked --jobs 1 -p safe-cex-venue-cryptocom --lib ws_payloads::accepted_ws_field
python3 safe-cex-venue-cryptocom/scripts/ws-record-controls.py
python3 safe-cex-venue-cryptocom/scripts/ws-cardinality-controls.py
python3 safe-cex-venue-cryptocom/scripts/ws-field-controls.py
python3 safe-cex-venue-cryptocom/scripts/ws-case-count-controls.py
node safe-cex-venue-cryptocom/scripts/ws-record-contracts.mjs
```

Selected validation passed: all 17 direct native field tests (618 cases), plus 17 named
complete-array tests, the existing 310-scenario source-declaration/native witness,
the direct 20-case private-field test, all 62 compiled named runtime
corruption/restoration controls, and 18 fresh actual native decoder probes with
zero classification-only receipts or failed owners. Integrated readiness results
are recorded in the [PR validation record](https://github.com/Tealstreet/tealstreet-sdk/pull/369).
Scope is eighteen original
native/default owners, separate from other owners, aliases, construction,
newer discovery, local scopes, feature configurations and WASM contract
execution. Full-package completion is not claimed.
