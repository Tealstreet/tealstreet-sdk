---
title: Scale ladder
---

Place N limit orders evenly spaced between two prices, with the total size
split across the rungs. Use this when you have a band of prices you'd be
happy filling at — a support zone for entries, a resistance zone for exits.
Unlike a chaser, the orders sit static; unlike a TWAP, they're all live at
once.

## Minimum viable

```text
scale buy $1000 into 10 from $last -0.25% to $last -1%
```

Ten limit buys totalling $1000 notional, evenly spaced between 0.25% and
1% below the last price on the focused symbol. Each rung is $100.

## Variations

```text
scale sell $2000 into 20 from 50000 to 48500 reduce
```

Reduce-only sell ladder using **absolute prices**. Use this to scale out of
a long with 20 rungs between 50000 and 48500.

```text
scale buy 5 into 10 from 31000 to 32000 ETHUSDT
```

Raw contract size (`5`) instead of notional, with an explicit symbol
override. Mix and match — size + range can each be absolute or relative.

```text
scale sell $10000 into 25 from 50000 to 48500 reduce taker
```

`reduce` makes every rung reduce-only. `taker` drops the default post-only
TIF on every rung. Use this when you want the ladder to be **immediately
fillable** if price walks through it (e.g. a panic-exit ladder) rather than
rejecting at exchange-side post-only checks.

```text
scale buy $500 into 5 from $bid -0.1% to $bid -0.5%
```

Anchor both endpoints to `$bid` with inline offsets. Same anchors that work
on `at <price>` work here — see [Variables](../reference/variables.md).

```text
loop longs: scale sell 50%pos into 5 from $entry +1% to $entry +3% reduce
```

Per-long ladder-out at 5 rungs between +1% and +3% of each position's
entry. Combines with [`loop`](../reference/workflow.md#loop) for
fan-out.

## Gotchas

- Distribution is linear-only. `cubic`, `icubic`, and other distribution
  modifiers are not implemented; the parser rejects them with
  `modifier '<x>' is not implemented; supported: reduce, taker`.
- Default TIF is **post-only**. If any rung would cross the book at placement
  time the exchange rejects it. Add `taker` to drop post-only.
- The ladder errors **before placing any order** if rounding would shrink any
  rung to zero size — similar to [`swarm`](../reference/commands.md#swarm).
  Either reduce `into <N>` or increase total size.
- Order of `from` / `to` doesn't matter — the ladder fills the range either
  way. But the rung at `from` is placed first, so if you want the closest-to-
  market rung in first, put it on the `from` side.
- Stops are not ladderable via `scale` — for trailing exits across rungs
  use a per-rung [`stop`](../reference/commands.md#stop) or
  [Trailing stop](./trailing-stop.md).
- `reduce` here **is** wired through to each rung's reduce-only flag (unlike
  `twap reduce`). It's a real per-order guarantee.

## Related

- [`scale`](../reference/commands.md#scale) — full syntax
- [Sizing & prices](../reference/sizing-and-prices.md) — `$`, `%`, `%pos`, `%bal`
- [Variables → anchors](../reference/variables.md) — `$bid +N%` inline offsets
- [Recipe: TWAP](./twap.md) — time-spread alternative
- [Recipe: Fan out across symbols](./fan-out-symbols.md)
