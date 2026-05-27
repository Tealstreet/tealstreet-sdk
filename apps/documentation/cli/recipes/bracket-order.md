---
title: Bracket order
---

Open a position and attach a reduce-only stop loss in one command. The `stop`
modifier on `buy` / `sell` bolts the stop on after the entry fires, no second
command needed. Use this when you want the stop locked in atomically rather
than chasing a separate `stop` call after the fill.

## Minimum viable

```text
buy $100 stop -1%
```

Market buy $100 notional on the focused symbol. A reduce-only market stop
is placed **1% below the fill price** as soon as the entry resolves.

## Variations

```text
buy $100 at 50000 stop 49500
```

Limit entry + absolute stop price. Stop attaches after the limit reports a fill.

```text
buy $100 at $bid +0.1% stop -1%
```

Post-only-ish entry pegged just above the bid, percent-offset stop relative
to the fill.

```text
buy $100 at 50000 po stop -1%
```

Explicit post-only entry (rejected if it would cross), same percent stop.

```text
buy 50%bal stop $entry -2%
```

Size by 50% of free balance; stop anchored to **`$entry`** (current
position's entry price), not to the new fill. Useful when you're adding
to an existing position and want the stop on the average entry.

```text
sell $100 stop +1%
```

Short entry with stop 1% above the fill. The stop side flips automatically.

```text
buy $100 stop -1% trigger -0.9%
```

Limit stop — the order rests at -1% but only activates when mark crosses
-0.9%. See [`stop`](../reference/commands.md#stop) for the trigger grammar.

## Gotchas

- The bracket stop is **always reduce-only** — it can't accidentally flip
  your position. This is by design and not configurable.
- `stop -1%` is relative to the **fill price**, not to the limit price you
  set. If you want the stop pinned to a known number, use `stop $entry -1%`
  (after the position exists) or pass an absolute (`stop 49500`).
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
