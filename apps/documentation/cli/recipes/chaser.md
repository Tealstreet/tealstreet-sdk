---
title: Chaser
---

A chaser is a passive limit order that re-pegs to the top of the book every
time the best bid (for buys) or ask (for sells) moves. It earns the maker
rebate and rarely crosses — at the cost of fills being uncertain when the
market walks away. Use it for size you want filled cheaply but don't need
filled instantly. Each chaser runs as a background task in the safe-cex
worker; list them with `chasers` and kill via `kill <id>` or
`chase cancel`.

## Minimum viable

```text
chase buy $100
```

Posts a limit at the best bid, re-pegs on every bid update, no termination
guard — runs until filled, killed, or the exchange rejects.

## Variations

```text
chase sell 50% reduce
```

Reduce-only chaser that closes 50% of the focused position. Side gets
auto-inverted relative to the position when used with `%all%` or `loop`.

```text
chase buy $500 to 1%
```

Terminate-at guard. If the price drifts more than 1% from the chaser's
starting price, the chaser cancels itself. Accepted forms for `to <length>`:
absolute (`to 49500`), percent (`to 1%`), dollar (`to $50` or `to 50$`).

```text
chase buy WIFUSDT,ENAUSDT $100
```

Multi-symbol fan-out — each symbol spawns its own chaser instance with its
own $100 notional.

```text
chase sell %all% 20% reduce
```

The `%all%` token (only valid with `chase … reduce`) expands to every
position on the implied side. With `sell` it picks longs; with `buy` it
picks shorts. 20% partial close per position.

```text
chase cancel
```

Stop the **newest active chaser** and cancel its resting order. Use
`chase cancel keep` to stop the chaser but leave the order resting in the
book.

```text
loop longs: chase sell 50% reduce
```

Spawn one reduce-only chaser per long position. See
[`loop`](../reference/workflow.md#loop) for the loop variants.

## Gotchas

- Chasers run in the **safe-cex worker** — the REPL doesn't block, the
  chaser keeps re-pegging even if you walk away. `tasks` and `chasers`
  both list them; `kill <id>` and `chase cancel` both stop them.
- **`chase cancel` always targets the newest chaser**, not all of them.
  For bulk cancel use `chasers cancel all` or `kill all`.
- `%all%` only resolves under `chase reduce`. `chase buy %all%` without
  `reduce` is rejected — there's no sensible expansion for opening.
- The chaser's "starting price" for `to <length>` is the **first peg**,
  not the price at which you typed the command. Latency between the two
  is usually tiny but worth knowing if the book is fast.
- Stops aren't chased — only limits. To trail a stop, see
  [Trailing stop](./trailing-stop.md).
- The chaser keeps a single resting order at a time. It cancels-and-replaces
  on every peg — busy markets burn through cancel quota faster than a static
  limit. If you hit rate limits, raise `to <length>` or use `twap`.

## Related

- [`chase`](../reference/commands.md#chase) — full syntax + termination forms
- [`chasers`](../reference/commands.md#chasers) — list + bulk cancel
- [`kill`](../reference/commands.md#kill) — stop tasks by id or `all`
- [Workflow → `loop`](../reference/workflow.md#loop) — fan-out across positions
- [Recipe: Fan out across symbols](./fan-out-symbols.md)
- [Recipe: TWAP](./twap.md) — chaser-per-slice variant
