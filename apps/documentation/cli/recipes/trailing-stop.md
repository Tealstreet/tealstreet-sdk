---
title: Trailing stop
---

Two ways to trail, and the first one is almost always the one you want.

## Use the venue's own trailing stop

```text
trail 20%
```

One command. The stop lives on the EXCHANGE, so it keeps trailing after you
close the CLI, and it survives your laptop going to sleep. Distance is measured
from the current price. See [`trail`](../reference/commands.md#trail).

To arm it only once the trade is working — the classic "runner after TP3":

```text
when (price >= 0.053169) (trail 20%)
```

[`when`](../reference/workflow.md#when) arms and returns, so you keep trading
while it waits. Not every venue offers trailing stops; `trail` refuses by name
on one that does not, and that is when you reach for the pattern below.

## Build one yourself

Roll your own using `track`, a reactive `sets`, and a blocking `wait`. This
runs in the CLI rather than on the exchange, which is the trade-off: it can
trail on anything (PnL, a custom expression) and on any venue, but it stops
the moment the CLI does.

## Minimum viable

```text
track max @hi $last
sets @stop @hi -1%
wait @last < @stop
close
```

Three reactive pieces plus one terminator:

1. `track max @hi $last` — ratchets `@hi` upward as `$last` ticks higher,
   never down.
2. `sets @stop @hi -1%` — **reactive** definition: every time `@stop` is
   read, it re-evaluates as `@hi - 1%`.
3. `wait @last < @stop` — blocks at 100 ms polling until last price drops
   under the trailing stop.
4. `close` — flat the position.

## Variations

```text
track min @lo $last
sets @stop @lo +1%
wait @last > @stop
close
```

Short-side mirror. `track min` ratchets downward; the stop sits 1% above
the running low.

```text
track max @hi $last
sets @stop @hi -1%
wait @last < @stop for=3s
close
```

Adds **`for=3s`** anti-fakeout. The condition has to hold continuously
for 3 seconds before `wait` releases. Filters single-tick wicks.

```text
track max @hi $pnl
sets @stop @hi -50
wait @pnl < @stop
close
```

Trail **PnL** instead of price. `@hi` ratchets on uPnL; stop is `$50`
under the high-water mark. Useful for `close at + green` workflows.

```text
track max @hi $last
sets @stop @hi -1%
wait either (@last < @stop) or (price > 60000)
close
```

Combinator — exit on either the trail OR a hard take-profit ceiling. See
[Workflow → combinators](../reference/workflow.md#combinators).

```text
alias trail "track max @hi $last; sets @stop @hi -1%; wait @last < @stop; close"
trail
```

Once you settle on a flavor, alias it so you can fire with one keystroke.

## Gotchas

- **`track` ratchets only on read.** A `wait @stop > 0` resolves it through
  the tracker and updates `@hi` at poll time, which is fine. But if nothing
  reads `@hi` between sessions, the high-water mark doesn't advance. The
  pattern above is self-driving because `wait @last < @stop` reads `@stop`
  → reads `@hi` → ratchets, every 100 ms.
- **`sets` (not `set`) is required for the stop.** `set @stop @hi -1%`
  snapshots a single number at definition time and never updates. `sets`
  keeps the source literal and re-evaluates on every read.
- **`echo @hi` ratchets a tracker on read; `print @hi` does not.** When you
  debug a trail manually with `print`, you're guaranteed not to advance the
  state — `echo` will. See
  [Aliases & vars → `track`](../reference/aliases-and-vars.md#track).
- **`wait` gives up after 30 s by default and CANNOT be told to wait hours.**
  `timeout=` is capped at 10 minutes and anything larger is rejected outright
  (`wait: timeout must be in (0, 10m]`), so a trail built this way needs a
  `repeat` loop around it to run for a session — or use `trail`, which has no
  such limit because the venue holds the order.
- Tracked + reactive vars are **session-scoped**. They don't persist across
  CLI restarts. Re-run the `track`/`sets` lines on startup, or alias the
  whole bundle.
- If the position is already underwater when you start the trail, `@hi`
  initializes at the first read of `$last` — meaning a flash-crash entry
  followed by recovery will set the high at the recovery's peak, not at
  the entry. Use `set @hi $entry` first if you want the trail anchored to
  entry instead.

## Related

- [Aliases & vars → `track` / `sets`](../reference/aliases-and-vars.md#track)
- [Workflow → `wait`](../reference/workflow.md#wait) — conditions + `for=`
- [Variables](../reference/variables.md) — `$last`, `$pnl`, `$entry`
- [Recipe: Bracket order](./bracket-order.md) — static stop alternative
- [Recipe: Chaser](./chaser.md) — for re-pegging entries, not exits
- [Workflow → `when`](../reference/workflow.md#when) — arm the trail on a condition
