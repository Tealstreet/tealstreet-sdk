# Orderbook function observations

Scope: four live orderbook functions on frozen master df357d330516f87c18f56f32b63f5df587cb473f.
The maintained canonical module measurement reported each missing contract by name;
the existing BinanceDepthUpdate map alias passed. No production body, generated
wire codec or shared accepted-set policy is changed by this review.

All four rows execute their actual Rust function. Their 123 independently obtained
source cases execute maintained binance.orderbook.ts and updateOrderBook.ts:

| Owner | Cases | Native observations |
| --- | ---: | --- |
| load_binance_order_book_snapshot | 36 | Both sides' level order, price/amount/total bits and both sequence IDs after mutation |
| get_buffered_binance_depth_updates | 25 | Borrowed record identity/order, original u bits/type, Number coercion failure name |
| apply_binance_depth_update | 36 | Native result enum, both sequence IDs, every level and partial mutation before TypeError |
| depth_index | 26 | Indexed native cells and immediate numeric consumption of split UTF16 string cells |

Number descriptors inject actual native -0, NaN and infinities. Raw JSON text
retains overflowing exponent and signed-zero lexemes until the actual JSON input
codec reads them. Native observations precede JSON output; no serializer is used
to infer nonfinite values or borrowed identity. An unset Option ID is observed as
unset, without claiming object-property ownness.

The private depth_index helper only feeds String/parseFloat in the live algorithm.
For primitive strings that split a surrogate pair, its owned UTF8 cell replaces
the lone unit, while TypeScript's cell retains that unit. Both immediate numeric
consumers yield NaN. Four explicitly marked consumer cases observe that result;
they do not claim raw lone-unit identity. Other raw string cells are whole values
from arrays/records and compare their UTF16 units. The private source expressions
are extracted unchanged with the TypeScript AST (o[0], o[1], bids[i]).

Real production consumer coverage uses native_dependencies/ExchangeOwner with
terminal snapshot HTTP held while socket frames buffer, and verifies the retained
book through subsequent updates and cleanup. Independent compiled body faults
corrupt snapshot ID assignment, buffered selection, final-ID finite guarding and
indexed row selection. Each must fail its named actual native constructor/socket
test, then pass after byte-exact source restoration. A compilation error or a
zero-test run never counts as a successful control.

Run scripts/orderbook-function-contracts.mjs for maintained source freshness and
the canonical scoped native measurement, and scripts/orderbook-function-controls.py
for the four production faults. The ordinary orderbook_function_contracts target
also checks all 123 source cases so CI cannot silently retain stale fixtures.
The existing 3,132 snapshot, 2,565 buffered and 3,080 apply source-oracle cases remain
unchanged. This review covers the existing typed/JSON boundary, not arbitrary JS
getters/prototypes, unrelated owners or other feature/wasm combinations.
