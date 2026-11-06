# Bybit attached stop-loss trigger reference

Attached derivative SL amendments send `slTriggerBy` with the SL leg's own known
MarkPrice, LastPrice or IndexPrice reference. Missing, empty or unrecognized
references use the app's existing MarkPrice SL default. Spot amendments omit
this derivative field, matching spot creation.

A virtual SL leg no longer borrows its parent opening order's unrelated trigger
when its own native `slTriggerBy` is unknown or absent. This keeps the write
default independent from the opening condition. Known read references are kept.
Full-position and missing-mark trading-stop paths are unchanged: their field is
optional and no equivalent rejection was established there. Fresh SL placement
and its existing defaults are unchanged.

Native order fixtures cover known Mark/Last/Index, UNKNOWN, a future enum, empty
and absent references across linear, demo, HyroTrader and inverse accounts.
They deliberately give the parent a different opening-trigger reference, then
assert the actual attached amend body and no cancel/trading-stop request. A
spot control pins omission. The TP trigger fix and partial-stop release veto
are separate PRs; composing them must retain each leg's own reference policy.

[Bybit amend-order docs](https://bybit-exchange.github.io/docs/v5/order/amend-order)
require an SL reference when no initial value exists. No live native SL refusal
body was retained; this is the corresponding code-established omission.
