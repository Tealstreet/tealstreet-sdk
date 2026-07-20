# Conditional Opening Orders

This guide is for adding first-class conditional opening order support to a
`safe-cex` exchange integration.

It covers the exchange layer only. The web UI already knows how to display
`OrderType.Conditional` separately from TP/SL when the exchange advertises
support through metadata.

## Core Contract

Use `OrderType.Conditional` only for opening trigger orders.

Do not use it for:

- Position-bound take profit / stop loss orders
- Reduce-only close orders
- Trailing stops
- Virtual TP/SL children derived from a parent order's `stopLoss` or
  `takeProfit` fields

The UI and order-management code treat this distinction as semantic, not just
display text:

- `OrderType.Conditional` means an opening order that rests behind a trigger.
- `OrderType.StopLoss` / `OrderType.TakeProfit` mean protective exits.
- `OrderType.TrailingStopLoss` remains a protective trailing exit.
- `OrderType.Limit` / `OrderType.Market` remain non-trigger entry orders.

## Metadata

Enable explicit support in `src/exchangeMetadata.ts`:

```ts
supportsOpeningTriggerOrders: true,
```

Only set this on variants that have been implemented and tested. For example,
Bybit linear, inverse, and demo support opening trigger orders, while spot
remains unsupported.

The metadata flag controls UI availability and tab wording. It does not fix
order mapping. The exchange integration still must map, place, update, and
cancel conditionals correctly.

## Mapping Rules

Every exchange needs one clear classifier in its order mapper.

The classifier should answer:

1. Is this a trigger/stop-like order according to the exchange payload?
2. Is it definitely protective? Examples: reduce-only, close-on-trigger,
   position TP/SL mode, attached TP/SL fields, or exchange-specific close
   strategy markers.
3. If it is trigger-like and not protective, map it to `OrderType.Conditional`.

Prefer positive protective checks over truthiness checks. Many exchanges send
sentinel strings in websocket payloads, such as `""`, `"UNKNOWN"`, `"None"`, or
`"0"`. Treat only documented protective values as protective.

For Bybit, the important lesson was:

```ts
const isPositionTpslMode = mode === 'Full' || mode === 'Partial';
```

`tpslMode: "UNKNOWN"` from websocket partials is not position TP/SL. A
truthiness check would misclassify opening conditionals as protective stops.

## REST and Websocket Parity

Do not stop after the REST mapper works.

Most exchanges have at least two order payload shapes:

- REST snapshot / open orders endpoint
- Private websocket order delta or partial payload

The websocket payload is often thinner or uses different sentinel values. A
manual refresh may look correct while live placement remains wrong if only the
REST shape was handled.

Before marking an exchange complete:

- Feed a REST open-order payload into `mapOrder`.
- Feed a newly-created websocket order event into `mapOrder`.
- Feed a websocket partial/update event into `mapOrder`.
- Confirm all three produce the same `OrderType`.

If the private websocket handler maps through the same `mapOrder`, the mapper
must tolerate both shapes. If it has a separate mapper, extract a shared
classifier and test both call sites.

## Placement

`placeOrder` should accept `PlaceOrderOpts` with `type:
OrderType.Conditional`.

Common requirements:

- Require `triggerPrice`.
- Use the exchange's trigger-order endpoint or trigger fields.
- For opening conditionals, force protective flags off when the exchange allows
  them to be provided:
  - `reduceOnly: false`
  - `closeOnTrigger: false`
- Preserve limit-vs-market trigger behavior if the exchange supports both:
  - Conditional market: trigger order with no limit price.
  - Conditional limit: trigger order with both trigger price and order price.
- Use the correct position/hedge index for an opening order, not a close order.

Do not route `OrderType.Conditional` through existing TP/SL helper methods that
call position trading-stop APIs. Those APIs usually create position-bound
protective orders, not standalone opening trigger orders.

## Updating

Conditionals usually amend like normal exchange orders, but the field names are
often trigger-specific.

Examples:

- Limit update: amend `price`
- Conditional trigger update: amend `triggerPrice`, `stopPrice`, or the
  exchange equivalent
- Conditional amount update: amend quantity if the exchange supports it

Do not treat conditionals as position TP/SL during updates. A websocket-origin
conditional may carry sentinel fields, so use the same protective classifier
that `mapOrder` uses.

Bybit regression to avoid:

- REST conditionals mapped correctly.
- WS conditionals with `tpslMode: "UNKNOWN"` displayed correctly after mapper
  changes.
- Drag/amend still needed the same `Full` / `Partial` protective-mode test;
  otherwise the update path could miss direct `triggerPrice` amend.

## Canceling

Cancel conditionals by order ID through the normal order cancel endpoint unless
the exchange explicitly documents a special trigger-order cancel path.

Do not cancel conditionals through position TP/SL reset endpoints. That can
change a user's protective stops instead of canceling the standalone trigger
order.

Batch cancel support should include conditionals if the exchange's batch cancel
endpoint accepts trigger orders. If trigger orders need a different category or
endpoint, split them before batching.

## Virtual Brackets

Some exchanges expose attached TP/SL values on a parent order. `safe-cex`
integrations may synthesize virtual child orders such as:

- `${parentId}__stop_loss`
- `${parentId}__take_profit`

Those virtual orders must remain `OrderType.StopLoss` and
`OrderType.TakeProfit`. They are not `OrderType.Conditional`, even though they
may have trigger prices.

When adding conditional support, verify that parent orders with attached TP/SL
still produce their virtual protective children exactly as before.

## Tests to Add

Add focused exchange tests before wiring UI behavior.

At minimum, cover:

- Placing conditional market trigger orders.
- Placing conditional limit trigger orders, if supported.
- Mapping REST opening trigger orders to `OrderType.Conditional`.
- Mapping websocket opening trigger partials to `OrderType.Conditional`.
- Mapping position TP/SL payloads to `OrderType.StopLoss` /
  `OrderType.TakeProfit`.
- Mapping close-only/reduce-only trigger orders as protective, not
  conditional.
- Amending conditional trigger price through the exchange's direct amend path.
- Canceling conditionals by order ID.
- Preserving virtual TP/SL children on parent orders.

Also run:

```bash
yarn workspace @tealstreet/safe-cex typecheck
yarn workspace @tealstreet/safe-cex test-unit <focused exchange tests>
```

If exchange metadata changes, include `src/exchangeMetadata.test.ts`.

## Review Checklist

Before enabling `supportsOpeningTriggerOrders` for an exchange variant:

- The exchange has a documented payload discriminator for opening triggers vs
  protective stops.
- REST and websocket payloads use the same classifier.
- Sentinel values are handled explicitly.
- `placeOrder` does not route conditionals through position TP/SL helpers.
- `updateOrder` uses trigger-specific amend fields.
- `cancelOrders` cancels the standalone trigger order, not position stops.
- Existing limit, market, TP/SL, trailing stop, and virtual bracket tests still
  pass.
- Unsupported variants keep `supportsOpeningTriggerOrders: false`.
