---
title: TWAP
---

Split one big order into N slices spread over a window so you don't move the
book and don't telegraph your size. The CLI's TWAP supports two slice
strategies: **market slices** (the default — each slice fires as a market
order at its scheduled time) or **chaser slices** (each slice runs a chaser
with a timeout/fail policy). List runs with `twap list`, stop with
`twap stop <id>`.

## Minimum viable

```text
twap buy $1000 into 10 over 60
```

Ten market slices of $100 each, spaced evenly across 60 minutes.

## Variations

```text
twap sell $500 into 5 over 30 reduce
```

Plain TWAP, reduce-only. (See gotchas — `reduce` here is a fatfinger
bypass marker, not a per-slice guarantee.)

```text
twap buy $1000 into 10 over 60 chaser
```

Same shape as the minimum viable, but each slice fires as a chaser instead
of a market order. With no explicit `timeout` / `fail`, defaults apply.

```text
twap chase buy $1000 into 10 over 60
```

Same as the `chaser` form above — the `twap chase` head is the explicit
chaser-per-slice entry point. Defaults: `timeout 50% market`, `fail market`.

```text
twap chase sell $1k into 10 over 60 timeout 30% next fail next
```

Custom policy. If a slice's chaser hasn't filled by 30% of its slice
window, **move on to the next slice** instead of going market. If the
chaser errors out, also skip to the next slice. Result: a more patient
TWAP that accepts under-fill rather than crossing.

```text
twap buy $1000 duration=1h slices=10
```

Legacy `key=value` form, still parsed. Equivalent to
`twap buy $1000 into 10 over 60`. Prefer the positional form in new code.

```text
twap list
twap stop <id-prefix>
twap stop all
```

Management. `<id-prefix>` is any unique leading substring of the run id.

## Confirmed vs legacy

These exact USDT-linear capabilities default to **confirmed** market TWAP:

- `binance:usdt-linear-perp:twap-market`
- `bybit:usdt-linear-perp:twap-market`
- `okx:usdt-linear-perp:twap-market`
- `bitget:usdt-linear-perp:twap-market` (Classic V2 only; UTA stays ineligible)
- `blofin:usdt-linear-perp:twap-market`

Confirmed TWAP gives every slice one stable logical ID, writes through stock
`placeOrder({ clientOrderId })`, and derives fill truth from exact client-ID
reads. It is session-bound: if this CLI exits, scheduling and observation stop;
orders already sent remain live at the venue. No task is resumed on restart.

```text
twap buy $100 into 5 over 1
twap buy $100 into 5 over 1 legacy
```

`legacy` forces the standard `TwapInstance` path. Noneligible markets keep the
standard behavior. A listed capability that cannot initialize errors with the
exact `legacy` command and never silently falls back.

One-shot `exec twap` waits for confirmed settlement. Exit `0` means every slice
is exact-ID confirmed filled; `1` means command/startup failure; `2` means
terminal partial/rejected/stopped work without unresolved quantity; `3` means
unresolved, ambiguous, or incident quantity; the second SIGINT exits `130`.
It waits through the scheduled end plus a 15-second outcome grace; after that
deadline it fences the task, drains entered work for the shared shutdown
deadline, and reports any remaining quantity as exit `3`.
Every nonzero confirmed result states that the process makes no further claim.

`twap list` and `tasks` label `CONFIRMED` versus `LEGACY`; `kill` is the only
cross-engine panic verb. Flatten/close still executes while a confirmed TWAP is
active, but prints that later slices may reopen the position. Stop or kill the
TWAP first when flatness must persist.

When `audit on` is already enabled, each confirmed slice adds one diagnostic
event immediately before placement and one after placement classification,
including its logical, caller, and wire IDs. Confirmed execution never reads
the audit log as recovery state.


## Gotchas

- `reduce` sends every slice as reduce-only and skips the exposure-increasing
  fatfinger gate. The venue remains authoritative: an invalid reduce-only slice
  can still reject.
- `timeout <pct> <action>` — pct is `(0, 100]`, action is `market` or
  `next`. `market` crosses at slice timeout; `next` skips the slice and
  carries its size into the schedule.
- `fail <action>` — same `{market, next}`. Fires when the chaser errors
  out (rejected, exchange returned non-ok, etc.), not when it just hasn't
  filled.
- `twap chase` consumes a chaser slot per slice. If your venue rate-limits
  cancel-replace, a 60-slice TWAP can burn the budget fast.
- The CLI doesn't randomize slice timing — it fires on a uniform grid. If
  you want jitter, layer `swarm` slices on top, or pipe through `repeat`.
- Stopping a TWAP doesn't cancel orders the active slice already placed.
  Run `cancel buys` / `cancel sells` after if you want a clean book.
- The legacy `duration=1h slices=10` form accepts `1h`, `30m`, `90s` — but
  not mixed (`1h30m` doesn't parse). Stick to one unit.

## Related

- [`twap`](../reference/commands.md#twap) — full grammar + management
- [Recipe: Chaser](./chaser.md) — the per-slice chaser semantics
- [Recipe: Scale ladder](./scale-ladder.md) — limit-ladder alternative
- [`tasks` / `kill`](../reference/commands.md#kill) — task control
- [Safety → fatfinger](../reference/safety.md#fatfinger) — per-order caps
