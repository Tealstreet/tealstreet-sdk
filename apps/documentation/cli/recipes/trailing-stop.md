---
title: Trailing stop
---

Roll your own trailing stop using `track`, a reactive `sets`, and a blocking
`wait`. The CLI doesn't ship a one-shot `trail` verb because the right
trailing behavior is too workflow-specific — this pattern is the canonical
build. Use it for runners where you want the stop to ratchet up but never
pull back.

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
- `wait` polls at 100 ms and times out at 30 s default. For trails that may
  run hours, set an explicit `timeout=8h` (cap is 10 m for `sleep`, but
  `wait`'s ceiling is also 10 m — split into a `repeat` loop for longer
  runs).
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
