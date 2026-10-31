---
title: Bracket order
---

Open a position and attach a reduce-only stop loss in one command. The `stop`
modifier on `buy` / `sell` bolts the stop on after the entry fires, no second
command needed. Use this when you want the stop locked in atomically rather
than chasing a separate `stop` call after the fill.

## Minimum viable

```text
buy $100 stop $last -1%
```

Market buy $100 notional on the focused symbol. A reduce-only market stop
is placed **1% below the last price when you send it**, as soon as the entry
resolves. A bare `stop -1%` is [refused for now](../reference/sizing-and-prices.md#negative-offsets).

**On a LIMIT entry, how the stop is carried differs per venue.** Bybit makes it
a field on the entry order, so cancelling an unfilled limit cancels the stop
with it. Binance and Hyperliquid submit a separate conditional order
immediately — cancel the limit there and the stop stays armed with no position
behind it. Check open orders after cancelling an unfilled bracket.

## Variations

```text
buy $100 at 50000 stop 49500
```

Limit entry + absolute stop price. Stop attaches after the limit reports a fill.

```text
buy $100 at $bid +0.1% stop $last -1%
```

Entry just above the bid, with the stop 1% below the last price.

```text
buy $100 at 50000 po stop 49500
```

Explicit post-only entry (rejected if it would cross), absolute stop.

```text
buy 50%bal stop $entry -2%
```

Size by 50% of free balance; stop anchored to **`$entry`** (current
position's entry price), not to the new fill. Useful when you're adding
to an existing position and want the stop on the average entry. With no
position open it fails with `$entry: no position` and places nothing.

```text
sell $100 stop +1%
```

Short entry with stop 1% above the fill. The stop side flips automatically.

```text
buy $100; stop at 49500 trigger 49550
```

Limit stop, placed as its own `stop` command because `buy` has no `trigger`
option. The order rests at 49500 but only activates when mark crosses 49550. On
Bybit this currently rests as a market stop at the trigger. See [`stop`](../reference/commands.md#stop) for the trigger grammar.

## Gotchas

- The bracket stop is **always reduce-only** — it can't accidentally flip
  your position. This is by design and not configurable.
- `stop $last -1%` is worked out from the **last price when you send it**, not
  from the fill or the limit price you set. To pin the stop to your entry, use
  `stop $entry -1%` after the position exists, or pass an absolute (`stop 49500`).
- Bare numbers ≥ 100 with no `stop` keyword and no `at` are rejected — the
  parser asks "did you mean `at <price>`?" so a typo can't coerce a market
  order. See [Safety → fatfinger](../reference/safety.md#fatfinger) for the
  per-order check that fires on top of this.
- If the entry never fills (post-only that crosses, limit too far away), no
  stop is created. Pair with `wait fill` if you want to react to a timeout.
- The stop sits in the exchange's stop book — closing the position manually
  doesn't cancel it. Run `cancel stops` or `nuke` to clean up.

## Related

- [`buy` / `sell`](../reference/commands.md#buy-sell) — full order grammar
- [`stop`](../reference/commands.md#stop) — standalone stop / limit-stop / TP
- [Variables → `$entry`](../reference/variables.md) — anchor stops to entry
- [Sizing & prices](../reference/sizing-and-prices.md) — `%bal` / `%pos` / `$N`
- [Recipe: Trailing stop](./trailing-stop.md) — ratcheted alternative
