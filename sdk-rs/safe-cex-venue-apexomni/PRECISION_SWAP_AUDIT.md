# Price/amount precision swap audit

Audited production source: `d50b9f84411344c85df9ad33b4da33a777d539bc`.

Result: **zero implemented two-axis price/amount formatting sites**. No compiled
precision-swap mutation was possible, so this is not a swap-proof claim.

The inventory follows `src/lib.rs` through all eight declared modules (nine Rust
files including the root), classifies their callable bodies, and traces the
precision-bearing record fields through their uses:

- `protocol` and `interface`: protocol tables, enums, strict wire decoding and
  numeric-output registration. They do not format order prices or amounts.
- `public_records`, `private_records`, `config_records`, `order_records`: wire
  records, capture-before-decode, field transfer, optional/opaque shape checks and
  numeric-output registration. `step_size`, `tick_size`, `price_precision`,
  `size_precision`, `price_step` and `size_step` remain data; they are not selected
  as rounding arguments. Price/size/trigger fields also remain data.
- `utils`: `get_precision` counts the source number/string spelling's fractional
  suffix. It does not select market price versus amount steps. The other two
  functions generate a client ID and remove a string prefix.
- `signer_utils`: clock-to-seconds flooring, assertion/type checks and the source
  subaccount no-op. Timestamp flooring has no price or amount axis.

A source search only locates candidate references; the result above comes from
classifying the module closure and the field-use paths, not from absence of a
keyword. Shared record ingress/format witnesses do not substitute for an order
formatter precision-swap witness.

## Obligations before the pending formatter rows can be mapped

The TypeScript does have both axes. `fetchMarketsHelper` derives amount from
`stepSize` and price from `tickSize`. `placeOrder` consumes both: amount, split
remainder and lot size use amount precision; ordinary price, stop loss, take
profit, explicit trigger and market execution use price precision. Its stop/TP
execution and both attached bracket legs call `calculateTpSlPrice` with the price
step. `updateOrder` may reconstruct a parent and its brackets through placement.
All four manifest rows (`fetchMarketsHelper`, `placeOrder`, `updateOrder`,
`calculateTpSlPrice`) remain pending at the audited SHA.

Their future witnesses must execute actual TypeScript for fractional inputs with
several distinct step pairs whose outputs differ, both sides, limit and
stop-market paths, explicit triggers and attached TP/SL legs. Compare decoded
native request/signing fields before wire serialization. First run each compiled
production precision swap against the original target; record any surviving
mutation. Add separate supplements, then require every swap to compile, fail
exit 101 at its named payload assertion, restore and pass. Include a bracket-only
mutation that swaps the step only in an attached TP/SL or trigger calculation,
leaving parent price and amount formatting unchanged; a direct native bracket
payload assertion must kill that isolated mutation. One Node process
returns each complete corpus; no per-case subprocess loop.

No formatter gate or precision-swap mutation was run for this zero-site audit.
No manifest statuses changed. Real tomllib counts remain 21 ported / 45 divergent /
102 pending (168 total). PR 7256's remote head remains pinned; this audit is local.
