# Precision-axis audit

Audited implementation: local `a6c799d6a7012f0be7b8e3538f666d0f7af4941d`.
PR 7255's remote remains pinned at `18878d8ca5556499c52b7dcb8e034e5dae2c62c6`.

Result: **0 compiled two-axis formatter sites to swap yet**. This is not a
swap-proof result and makes no claim about future limit, stop or bracket paths.
No precision-swap mutation was run: production has no amount-precision read
that could replace a price-precision read. A fabricated test-only second axis
would not audit the production wiring described in the fleet report.

## Method and implemented scope

Read all implemented behavioral module APIs and bodies, traced the only rounding
call to shared `safe_cex_num::adjust`, and reconciled the corresponding source
methods against Python `tomllib` manifest rows. Text searches located candidates;
the conclusion comes from the argument/call-flow and mapped-row review.

| Rust module | Precision behavior |
| --- | --- |
| `exchange_helpers.rs` | `round_price_to_step(price, price_step, direction)` validates one caller-supplied step and passes that same step to `adjust`. It has no market or amount-step input. No implemented venue production caller binds it to market precision. |
| `order_history.rs` | Parses size, remaining size and prices; computes filled amount. No quantization or precision selection. |
| `income.rs` | Parses ledger fields and computes fee sign. No quantization or precision selection. |
| `account.rs` | Reads account identity and returns position mode. No numeric formatter. |
| `constants.rs`, `types.rs` | Tables, predicates and declared response codecs. Wire increment/tick-size fields are data, not derived steps or payload formatting. |

The existing helper corpus exercises 416 price/step/direction combinations and
compares native number bits with actual TypeScript. It proves behavior for its
supplied step, **not that a future caller selects the price axis**. Previous
rounding-direction mutation evidence must not be relabelled precision-swap
coverage.

## Pending TypeScript paths requiring the later audit

- `fetchMarkets`: derives amount precision from `step_size || order_size_increment`
  and price precision from `tick_size || price_tick_size`.
- `resolveMarketIocExecutionPrice`: reads `market?.precision?.price`, then supplies
  that step to the helper for execution price and band-limit correction.
- `updateOrder`: quantizes amended size with amount precision, limit execution
  price with price precision, and untriggered TP/SL trigger price with price
  precision. The latter is the bracket-specific swap candidate.
- `buildMainOrderPayload` / placement: formats size, execution and trigger values
  as strings, consuming the earlier order/preflight paths. Audit the complete
  caller chain when those pending rows become implemented; the source body alone
  is not proof that inherited preprocessing chose the correct axis.

All five named rows are pending in the parsed Paradex exchange manifest. The
local venue triple remains **10 ported / 25 divergent / 118 pending**.

When these rows land, preserve the original corpus, first compile each real
production axis swap and run its existing target, then add labelled live
fractional cases across multiple distinct step pairs and both sides. Observe
native limit and stop/trigger payload fields before serialization; ensure the
correct and swapped derived results actually differ. Require each mutation to
compile, fail exit 101 at its named field assertion, restore and pass. Invoke
Node once per complete corpus, never once per case.

## Per-field inventory (bucket-audit follow-up)

A venue-wide zero is not substituted for the following field-specific statuses.
Each pending field needs its own native assertion and applicable production
mutation when the row is implemented.

| Source owner / output field or branch | Precision provenance | Rust state / required control |
| --- | --- | --- |
| `roundPriceToStep` return value | Single supplied `priceStep`; no amount axis exists in its signature | Implemented helper only; valid negative for an axis swap, not caller coverage |
| `fetchMarkets` → `precision.amount` | `step_size || order_size_increment` | Pending; swap with the price derivation and observe native `precision.amount` |
| `fetchMarkets` → `precision.price` | `tick_size || price_tick_size` | Pending; swap with the amount derivation and observe native `precision.price` |
| `resolveMarketIocExecutionPrice` → initial execution price | `market.precision.price`, buy rounds up / sell rounds down | Pending; swap the bound price step with amount step, both sides |
| `resolveMarketIocExecutionPrice` → buy band-limit correction | Same price step, rounds down | Pending; separate case that actually enters this branch and produces distinct steps' results |
| `resolveMarketIocExecutionPrice` → sell band-limit correction | Same price step, rounds up | Pending; separate case that actually enters this branch and produces distinct steps' results |
| `updateOrder` → signed/payload `size` | `market.precision.amount` → `amendedSize` | Pending; amount-to-price swap; observe native signed size and transmitted size |
| `updateOrder` → signed/payload limit `price` | `market.precision.price` → `amendedPrice` for supported limit orders | Pending; price-to-amount swap; observe native signed and transmitted price |
| `updateOrder` → TP/SL `trigger_price` | `market.precision.price` → `amendedTriggerPrice` for supported untriggered TP/SL | Pending; independent bracket-only price-to-amount swap and native trigger assertion |
| `buildMainOrderPayload` → signed/payload `size` | `opts.amount` stringification; preprocessing is upstream | Pending; trace amount provenance through the caller and assert both payloads |
| `buildMainOrderPayload` → signed/payload `price` | `executionPrice`, possibly from IOC resolver | Pending; trace both limit and IOC provenance and assert both payloads |
| `buildMainOrderPayload` → `trigger_price` | Opening `opts.triggerPrice`, stop execution price, or explicit trigger fallback | Pending; exercise each populated branch, both sides, including stop-market; do not infer trigger correctness from ordinary limit-price coverage |

No fixture values were changed by this inventory. No unimplemented field is
labelled already-covered or swap-proof. The existing helper direction mutation
remains a direction control, not an axis-selection control.
