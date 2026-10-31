---
title: Aliases & user variables
sidebar_position: 3
---

# Aliases & user variables

The CLI gives you three kinds of named values plus aliases for shorthand
commands. Read this page once and you'll know which one to reach for.

| Need                                                               | Use                               |
| ------------------------------------------------------------------ | --------------------------------- |
| Snapshot a value right now and reuse it later                      | [`set`](#set)                     |
| Re-evaluate a formula every time you read it                       | [`sets`](#sets)                   |
| Track the high-water or low-water mark of a live value             | [`track`](#track)                 |
| Give a multi-step chain a short name                               | [`alias`](#aliases)               |
| Capture the ID of the order you just placed, for later cancel/wait | [`@name = body`](#capture-syntax) |

`set`, `sets`, and `track` share a single namespace: at any moment a given
`@name` is exactly one of (static, reactive, tracker). Reusing the name
clears whichever kind held it before.

---

## `set`

Snapshot a value. The expression on the right is substituted **once**, at the
time you run `set`, and frozen into a static variable.

```text
set @x $entry        # @x := today's entry price
set @size 100$       # @x := 100$
echo @x              # prints the frozen value
```

Static variables persist to `~/.tealstreet/vars.json` (standalone) or a
jotai atom (web). They survive restarts.

---

## `sets`

Reactive — the source expression stays literal and is re-evaluated **every**
time you read the variable.

```text
sets @spread $ask-$bid
sets @stopline $entry*0.99

echo @spread          # re-reads bid/ask now
sleep 5s; echo @spread # re-reads again
```

Reactive vars are **session-scoped** — they don't persist across CLI
restarts.

---

## `track`

Aggregator. Stores a high-water or low-water mark that ratchets in one
direction. Ratchets on every `echo` or other ratcheting read; never on `print`.

| Form                 | Effect                    |
| -------------------- | ------------------------- |
| `track max @hi $pnl` | `@hi` only ever increases |
| `track min @lo $pnl` | `@lo` only ever decreases |
| `track list`         | List all active trackers  |

```text
track max @peakpnl $pnl
echo @peakpnl       # ratchets if $pnl is higher than stored
print @peakpnl      # read-only, no ratchet
```

Trackers are **session-scoped**. Use them for trailing stops + peak-PnL
trips. See [Recipes → Trailing stop](../recipes/trailing-stop.md).

---

## `untrack` / `unset`

| Form          | Effect                           |
| ------------- | -------------------------------- |
| `untrack @hi` | Remove a tracker by name         |
| `untrack all` | Remove every tracker             |
| `unset @x`    | Remove whichever kind holds `@x` |
| `unset all`   | Remove every user variable       |

`unset` is the kind-agnostic remove — it doesn't care whether `@x` was a
`set`, a `sets`, or a `track`. `untrack` is tracker-only.

---

## `print` / `vars`

Read-only inspection. Never ratchets a tracker.

| Form                 | Effect                   |
| -------------------- | ------------------------ |
| `print @x`           | Value of `@x` (any kind) |
| `print all`          | All vars across kinds    |
| `print list`         | Same — alias             |
| `vars` / `vars list` | Alias for `print list`   |
| `vars @x`            | Alias for `print @x`     |

> **`vars` is just an alias for `print`** — they share the same handler.
> Use whichever feels natural.

`echo @x` does the same substitution but **does ratchet** if `@x` is a
tracker. Pick `print` when you want to inspect without side effects.

---

## Aliases

Aliases are just named command bodies. The body is resolved at **use time**,
so `$` constants and `@` vars re-evaluate on every call.

```text
alias                                # list
alias z "buy at $entry -50; sleep 5s; close"
z                                    # runs the body
```

Multi-statement bodies splice back into the outer chain — so chain operators
(`;`, `&`) inside an alias work as if you'd typed them at the prompt.

| Form                          | Effect             |
| ----------------------------- | ------------------ |
| `alias`                       | List all           |
| `alias list`                  | Same               |
| `alias z "buy at $entry -50"` | Create / overwrite |
| `unalias z`                   | Remove one         |
| `unalias all`                 | Remove all         |

Aliases persist to `~/.tealstreet/aliases.json` (standalone) or browser
storage (web), and **sync between the CLI and every browser signed in to the
same account**. An alias you define in one shows up in the others within a
few seconds (a running REPL checks every few minutes and right before it runs a
webhook). That is what lets a [TradingView webhook](../recipes/webhook-from-tradingview.md)
call an alias on whichever listener runs it.

If the same alias is changed in two places, the change the server receives
last wins. The first time a device syncs, aliases it already had are uploaded
unless the account already has one with that name, in which case the account's
version wins. Up to 500 aliases; a name is at most 64 characters and a body at
most 2000.

---

## Capture syntax

The capture form attaches a writeback to a command head. Useful when you
need the **order ID** the command produced — to cancel, move, or wait for it.

```text
@o1 = buy $100 at 50000; cancel @o1
```

1. `buy $100 at 50000` runs.
2. When it lands, the runner writes `result.data.orderIds.join(',')` into
   `@o1` via `setVar`.
3. The next segment (`cancel @o1`) substitutes `@o1` after that writeback.

Forward references later in the chain get a `deferredSubstitution` field so
the runner finalises them after the prior step's writeback.

```text
@o = buy $100 at 49500; wait fill @o; close
```

Source: `packages/cli-core/src/preprocess/preprocess.ts:38-39, 165-172`.

> **Why this matters**: chain operators are fire-and-don't-await
> (see [Workflow → Chains](./workflow.md#chain-operators)). Capture is
> the escape hatch for the rare case where you need to wait on a
> specific order.

---

## Storage at a glance

| Kind                 | Lives in                                                     | Survives restart? |
| -------------------- | ------------------------------------------------------------ | ----------------- |
| `set` (static)       | `~/.tealstreet/vars.json` / jotai                            | yes               |
| `sets` (reactive)    | in-process                                                   | no                |
| `track` (aggregator) | in-process                                                   | no                |
| `alias`              | `~/.tealstreet/aliases.json` / jotai, synced to your account | yes               |

Persistent ones round-trip through
[`export` / `import`](./commands.md#import--export). Session-scoped ones
do not — they vanish on REPL exit.
