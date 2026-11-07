---
title: Signal ladder
---

Execute a Telegram-style signal end to end: limit entry, protective stop, a
take-profit ladder, stop moved to your entry once the first target hits, and a
trailing runner on what is left.

The signal looks like this:

```text
KERNEL  LONG  10x
ENTRY  $0.051124  (limit)
TP1 $0.051533   TP2 $0.051942   TP3 $0.053169
SL  $0.049570
```

## The whole thing

```text
lev 10
buy 100$ at 0.051124 stop 0.049570
sell 33$ at 0.051533 reduce; sell 33$ at 0.051942 reduce; sell 33$ at 0.053169 reduce
when (price >= 0.051533) (be +3%)
when (price >= 0.053169) (trail 20%)
```

Five lines. The last two arm and return, so you keep trading while they wait.

## Line by line

### `buy 100$ at 0.051124 stop 0.049570`

Entry and protective stop in ONE command, so there is no window where the
position exists unprotected.

**How the stop is carried is venue-specific, and it matters if you cancel.**
On Bybit it is a FIELD on the entry order, so cancelling the resting limit
takes the stop with it. On Binance and Hyperliquid it is a SEPARATE
conditional order submitted straight away — cancel the limit there and the
stop is still armed, with no position behind it. Check your open orders after
cancelling an unfilled bracket. See [Bracket order](./bracket-order.md).

### The ladder

```text
sell 33$ at 0.051533 reduce; sell 33$ at 0.051942 reduce; sell 33$ at 0.053169 reduce
```

**`at` is not optional.** `sell 33$ 0.051533 reduce` is not a limit order — it
is a MARKET sell, because without `at` there is nothing for the price to attach
to. The CLI now refuses that line rather than filling it, but the habit is
worth having: every resting order says `at`.

**`33$` is 33 dollars of notional, not a third of the position.** Three rungs
of `$33` against a `$100` entry leaves a small tail unsold, because each rung
buys fewer contracts as the price rises. If you want exact thirds, use `33%` —
but only AFTER the entry has filled, since a percentage is a percentage _of a
position_ and there is no position yet while the limit is resting.

### `when (price >= 0.051533) (be +3%)`

Move the stop to your entry once TP1 is reached, with a 3% buffer for slippage.
[`be`](../reference/commands.md#be) cancels stop losses only — the rest of the
ladder stays where it is.

The offset is a plain direction, not a side-aware one: `+` is up the price
axis. This signal is a LONG, so `+3%` puts the stop above entry. **On a short
it would be `be $entry -3%`** — `+3%` there widens the stop rather than
tightening it, and a bare `be -3%` is [refused for now](../reference/sizing-and-prices.md#negative-offsets).

**`price >=` means the price TOUCHED your target, not that your rung filled.**
The two are usually the same moment and occasionally are not: your own resting
offer can be the best ask, and a touch without a trade leaves the rung open.

There is no reliable "this specific rung filled" trigger today. The obvious
spelling — capture the order and arm on `fill` — does not work: `fill` is true
while nothing matching is open, and a just-placed order has not reached the
local store yet, so the trigger fires within a second of being armed, while
you are still flat, and `be` fails with "No position found". Use the price
condition, and know that it is a touch.

### `when (price >= 0.053169) (trail 20%)`

The runner. Once TP3 is reached, put a trailing stop on whatever is still open
and let it ride. See [Trailing stop](./trailing-stop.md).

## Things that will bite you

- **Use literal prices in the condition, not `$roe100`.** An ROE anchor is
  resolved when you ARM the trigger and needs an open position with known
  leverage, so arming one before the entry fills fails with
  `no position for KERNELUSDT`.
- **Triggers do not survive the CLI closing.** They live in the session that
  armed them. `tasks` shows what is armed, `kill <id>` drops one.
- **A trigger expires after 12 hours** without firing.
- **`be` places the stop at entry**, not at a fee- and profit-adjusted
  break-even. See [`be`](../reference/commands.md#be).
- **Not every venue has trailing stops.** `trail` refuses by name rather than
  sending an order the venue will reject.

## Related

- [Bracket order](./bracket-order.md) — entry plus stop, on its own
- [Trailing stop](./trailing-stop.md) — the runner, and the build-it-yourself version
- [Scale ladder](./scale-ladder.md) — evenly spaced rungs, generated
- [Workflow → `when`](../reference/workflow.md#when) — the condition vocabulary
