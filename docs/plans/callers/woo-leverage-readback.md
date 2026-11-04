# WOO leverage readback

An accepted leverage write now confirms when the matching symbol's one-way,
BOTH-side read returns the requested leverage without `margin_mode`. The field
is optional in both the trading confirmation and account metadata decoders.
An explicit non-CROSS mode still refuses; symbol, position mode, side,
cardinality and leverage checks stay in place. Failed confirmation retries
reads only and retains the accepted-write Unknown outcome; it never resends
an accepted write.

A missing margin mode confirms leverage only. It preserves the stored margin
type, including unknown, and FetchLeverage omits an unproved `marginType`.
The SDK's FetchedLeverage contract already makes that field optional. NX and
server consumers must not interpret omission as a confirmed change to Cross.
No new margin-mode mutation is enabled: isolated mutation still refuses;
the existing Cross command remains its local no-op. Any future real margin
mode write needs explicit authoritative mode evidence before confirmation.

The [legacy WOO futures-leverage example](https://docs.woox.io/#get-futures-leverage-setting)
includes snake-case `margin_mode`, `position_mode`, and per-side details.
The reported production response omitted `margin_mode`; this change tolerates
that observed variation, rather than claiming the docs promise omission.
The [V3 replacement](https://developer.woox.io/api-reference/endpoint/futures/get_leverage)
uses camelCase. Routes and wire generations are unchanged here.

Readback audit:

| Decoder | Finding and action |
| --- | --- |
| Trading Leverage | Required margin echo caused false unconfirmed writes; now optional. Required identity/scope/value evidence stays. |
| Account Leverage (FetchLeverage and background metadata) | Same required echo and fabricated Cross publication; now optional, with margin knowledge preserved. |
| Trading position-mode Account | `accountMode` and `positionMode` directly prove the requested mode and remain required; no unrelated balance/account fields are decoded. |
| Account Info | Only account/position modes and collateral/account-value fields are required. They occur in the [legacy account-info example](https://docs.woox.io/#get-account-information-new); missing financial evidence must not become zero. No demonstrated absent-required field found. |
| PositionRow | Symbol, size, entry, leverage and margin mode establish exposure and scope; the [positions example](https://docs.woox.io/#get-all-position-info-new) includes them. `markPrice` is already optional. No new omission evidence found; retain snapshot-preservation behavior. |
| OrderRow and page envelope | Order rows already default optional fields and validate the evidence needed for each order kind. Page metadata remains required to prove census completeness. No additional confirmed omission found. |

This clears the leverage-only margin-echo prerequisite. It adds no deployment
ordering, dependencies or inventory/cap changes and is independent of #2996.
