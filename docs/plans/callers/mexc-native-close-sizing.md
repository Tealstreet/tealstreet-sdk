# MEXC native close sizing

Found by the review of NX #9411: a 130-contract DOGE_USDT close became 100.
MEXC's public contract detail reports 100 DOGE per contract and `volUnit: 1`.
The native SDK publishes the amount precision and limits in base units, so the
market has `precision.amount: 100`, although placement takes native contracts.
Applying that base step directly to 130 contracts rounded the close to 100.
The old downsizer also only converted fractional contract sizes, missing DOGE.

The shared policy now derives MEXC's native step and maximum by dividing the
published base rails by `contractSize`. It compares positions and resting orders
in base units, then returns the requested native quantity. A reduce that cannot
preserve its quantity refuses explicitly: an unaligned native size, an amount
above a known maximum, or a clamp that would silently shrink it. Missing unit
metadata refuses; an absent maximum remains unknown. Other venue rules retain
their existing behavior, including native-sized OKX precision.

Preserve-or-refuse covers TP/SL kinds even without `reduceOnly`, explicit close
orders, and amendments of reducing/protective orders. The requested amount is
checked before rounding and before either amendment send boundary. Only an
explicit zero on an eligible full-position stop receives the infinity exception;
a positive off-step quantity cannot round into it. Price-only amendments keep
their amount omitted.

`prepareOrder` and `clampReduceOnlyOrder` already take `exchangeName`; the shared
CoreAdapter supplies it for placements, closes and amendments. Direct host calls
must also carry it:

```ts
roundMarketCloseSize(nativeAmount, market, exchange.name);
roundMarketOrderSize(nativeAmount, market, { reduceOnly: true, exchangeName: exchange.name });
roundOrderSizeToNearest(nativeAmount, market, exchange.name);
downSizeReduceOrder(market, order, positions, restingOrders, price, { exchangeName: exchange.name });
```

Before rounding a reducing/protective amendment, direct hosts must call the same
`assertCloseLikeOrderAmount(requestedNativeAmount, existingOrder, market, exchange.name)`
guard that CoreAdapter uses. Its classification includes the stored order kind
and does not require the caller to repeat a protective order's reducing flag.

## Runtime callers covered

| Caller                                                              | Native quantity handling                                                                                |
| ------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| CoreAdapter placement, protection, close and amend                  | Checks the requested amount before rounding and both amendment boundaries.                              |
| MCP `amend_order`                                                   | Carries the fetched order and venue through the shared guard and rounding rule.                         |
| CLI position/balance percentages and notional sizing                | Leaves MEXC's derived native intent unrounded until shared preparation validates it.                    |
| CLI swarm                                                           | Validates the requested total before fragmentation and uses a native lot step for slices.               |
| OEMS close, reduce, placement, protection and amend                 | Uses the shared guard before any amount rounding, including remaining-to-total amendments.              |
| Copy-trade delta conversion and Market/LimitSlippage/Chase dispatch | Preserves the unrounded native count and refuses invalid reductions before placement or chaser changes. |

The SDK owns the common unit-rule leaf; script-api-core adapts its refusals to
`ScriptValidationError`. SDK copy-trade reuses that leaf without depending on
script-api-core. Price rounding, display arithmetic and retained reference
adapters do not change.

OEMS `amountStep` is declared native, unlike MEXC's base-sized Market precision.
Snapshot producers must populate it with `nativeAmountRules(market, exchange.name).step`.
The resolver refuses incompatible/off-step declarations rather than converting
130 into 100. It preserves unknown amount/remaining fields as refusals; it does
not replace them with zero. NX's snapshot and direct-helper adoption remains a
consumer requirement.

NX's web order helpers must pass this context when consuming these functions.
No host-local arithmetic or fallback contract size is needed. A missing native
quantity must remain absent or be refused, rather than being changed to zero
(which can mean full-position TP/SL). Intentional zero-sized infinity stops
retain their existing contract.

The regression uses the public DOGE_USDT snapshot from 2026-11-10:
`contractSize: 100`, `volUnit: 1`, `minVol: 1`, `maxVol: 445000`,
`limitMaxVol: 445000`. It checks 99/100/101 and 130 contracts, the maximum and
either side of it, invalid unit metadata, conflicting resting orders, and the
CoreAdapter's actual placement and close path. No authenticated requests occur.

Source: <https://contract.mexc.com/api/v1/contract/detail?symbol=DOGE_USDT>;
[contract field definitions](https://mexcdevelop.github.io/apidocs/contract_v1_en/#get-the-contract-information).
