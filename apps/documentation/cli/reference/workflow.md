---
title: Workflow
sidebar_position: 5
---

# Workflow

Chain operators, loops, conditionals, sleeps, waits, retries, and chain
modifiers. The composition primitives that turn the CLI into a scripting
language.

---

## Chain operators

```text
buy $100 ; cancel  ; close
buy $100 & cancel
```

| Operator             | Behavior                                                                                   |
| -------------------- | ------------------------------------------------------------------------------------------ |
| `;`                  | **Sequential** — waits for all prior steps to finish before this one runs.                 |
| `&`                  | **Parallel** — launches without waiting; runs alongside the prior step.                    |
| `sleep` / `wait`     | Always block the chain (carry `block: true`), regardless of separator.                     |
| legacy `@cmd` prefix | Stripped silently (one-time warning per call). Skipped when `@<word>` is a known user var. |

`;` is a **barrier**: the runner drains every in-flight step before
dispatching the next, so `close all; buy $1000` waits for the close to
complete before buying. `&` is **parallel** — `close all & buy $1000`
fires both at once. Mix them freely: `a & b; c` runs `a` and `b`
together, then waits for both before `c`. `repeat` joins its iterations
with `;`, so iterations are sequential by default. For condition-based
waits use [`wait`](#wait); to act on a specific order use
[capture syntax](./aliases-and-vars.md#capture-syntax).

---

## `sleep`

| Form          | Effect                              |
| ------------- | ----------------------------------- |
| `sleep 5`     | 5 seconds (default unit is seconds) |
| `sleep 250ms` | 250 ms                              |
| `sleep 5s`    | 5 seconds (explicit)                |
| `sleep 2m`    | 2 minutes                           |

Hard cap **10 minutes**. Source: `SleepCommand.ts`, `preprocess/sleep.ts`.

---

## `wait`

Condition-blocked sleep. Polls the local exchange store at 100 ms.

```text
wait <condition> [timeout=<dur>] [for=<dur>]
```

### Condition forms

| Form                                       | Meaning                                                                            |
| ------------------------------------------ | ---------------------------------------------------------------------------------- |
| `wait 5s` / `wait 250ms`                   | Bare duration — same as `sleep`                                                    |
| `wait time HHMM`                           | Wait until next occurrence of HHMM **UTC**                                         |
| `wait position [SYM]`                      | Any position, focused or named                                                     |
| `wait long [SYM]` / `wait short [SYM]`     | Side-filtered                                                                      |
| `wait anyposition`                         | Any account, any symbol                                                            |
| `wait price > 50000`                       | Spot the trigger — uses **ask** on upside, **bid** on downside, falls back to last |
| `wait mark > 50000` / `wait index < 50000` | Mark / index price                                                                 |
| `wait pnl > 200` / `pnl < -50`             | uPnL on focused symbol                                                             |
| `wait fundingrate > 0.0001`                | Funding rate                                                                       |
| `wait premium > 5`                         | Premium                                                                            |
| `wait fill` / `wait fill BTCUSDT`          | Wait for **any** fill on the symbol                                                |
| `wait fill <id>` / `<id1>,<id2>`           | Wait for a specific order to fill                                                  |
| `wait @stop > 0`                           | LHS can be a user var (resolves via tracker/live/static)                           |

### Comparators

`>` `<` `>=` `<=` `==` `!=`. The exact-compare forms are rarely useful
for prices but they exist for integer / zero checks.

### Combinators

```text
wait both (price > 50000) and (pnl > 0)
wait either (price < 49000) or (price > 51000)
```

Combinators nest. Source: `preprocess/conditions.ts`.

### Modifiers

| Modifier        | Default | Cap  | Behavior                                                            |
| --------------- | ------- | ---- | ------------------------------------------------------------------- |
| `timeout=<dur>` | 30 s    | 10 m | Give up after this long                                             |
| `for=<dur>`     | —       | —    | Condition must hold **continuously** for this window (anti-fakeout) |

> **`wait time 1200` and it's currently 12:00:00 UTC** — the parser
> interprets that as "next 12:00 UTC", i.e. ~24 hours from now. Use
> `wait time 1201` if you want the next-minute case.

---

## `when`

Arm a command to run **once, later** — without holding the prompt.

```text
when (<condition>) (<command>)
```

`wait` blocks the line it is typed on and gives up after 30 seconds by default
— and cannot be told to wait longer than ten minutes — so it cannot sit there
waiting for a take-profit that fills in two hours. `when` arms
and returns immediately; you keep trading.

```text
when (price >= 0.051533) (be +3%)
when (price >= 0.053169) (trail 20%)
when (both (price > 50000) and (pnl > 0)) (close 50%)
```

**The condition vocabulary is exactly `wait`'s** — everything in the table
above, combinators included.

### Why the parentheses

A condition is variable-length and so is a command, so there is no separator a
parser could find on its own: `when price >= 100 close 50%` has no unambiguous
split. The parens are the same ones [`if`](#if--else--end) already uses, and they are
paren-aware, so a `;` **inside** the action belongs to the action:

```text
when (price >= 100) (be +3%; trail 20%)     # both run when it fires
when (price >= 100) (be +3%); buy $50       # the buy runs NOW
```

### What resolves when

| Half          | Resolved     | Why                                                                            |
| ------------- | ------------ | ------------------------------------------------------------------------------ |
| the condition | **when armed** | `$entry` and `$roe101` mean the position you are looking at now              |
| the command   | **when it fires** | `be +3%` has to read the position as it will be then — that is the point |

So `when (price >= $roe100) (trail 20%)` pins the target price at the moment you
arm it, and the `trail` is computed against whatever the market is doing when it
triggers.

### Managing armed triggers

`tasks` lists them alongside chasers and TWAPs; `kill <id>` drops one. Bare
`kill` still targets the most recent chaser or TWAP rather than a trigger.

```text
> when (price >= 51000) (be +3%)
Armed [a3f9c1] price >= 51000 → be +3%
> tasks
[a3f9c1] when  BTCUSDT  price >= 51000 -> be +3%  armed
> kill a3f9c1
```

### Limits

| Limit                      | Value | Why                                                                      |
| -------------------------- | ----- | ------------------------------------------------------------------------ |
| Fires                      | once  | It disarms itself; arm another if you want it again                     |
| Expiry                     | 12 h  | A forgotten trigger is one that fires a market order next week          |
| Nesting                    | 5     | A trigger whose command arms a trigger is fine; one that arms itself is a loop |

A trigger **is not persisted**. It lives in the session that armed it, so
closing the CLI drops it. It also needs the interactive REPL — a one-shot
`tealstreet "when (…) (…)"` refuses rather than arming something that would be
destroyed the moment the command returns.

Whatever it fires goes through the same path a typed command does: preprocessing,
your aliases, the safety guards, and the [audit log](./commands.md#audit).

Worked end to end in [Recipe: Signal ladder](../recipes/signal-ladder.md).

Source: `WhenCommand.ts`, `triggers/triggerRegistry.ts`.

---

## `loop`

Fan out one step per matching position. Symbol is appended to the body.

```text
loop <variant>: <body>
```

| `<variant>` | Matches                                       |
| ----------- | --------------------------------------------- |
| `longs`     | All long positions                            |
| `shorts`    | All short positions                           |
| `openpos`   | All open positions (`open` accepted as alias) |
| `allpos`    | Every position (synonym for `openpos`)        |
| `all`       | Same                                          |

```text
loop longs: buy 1$
loop shorts: close
loop allpos: cancel
```

**Multi-statement bodies are not supported.** If you need one, alias the
body and loop the alias:

```text
alias cleanup "cancel; close"
loop longs: cleanup
```

Source: `preprocess/loop.ts`.

---

## `repeat`

```text
repeat 5 (buy 1$; sleep 1m)
```

| Cap               | Value |
| ----------------- | ----- |
| Max iterations    | 1000  |
| Max nesting depth | 8     |

Source: `preprocess/repeat.ts`.

---

## `if` / `else` / `end`

```text
if (<condition>) (<then-body>) [else (<else-body>)]
```

`<condition>` uses the same vocabulary as [`wait`](#wait).
Evaluated **once at preprocess time** against current store state, so
this is a one-shot conditional, not a runtime loop.

The body is fully reprocessed — nested `if`, `repeat`, `loop`, `retry`,
`set ec` all compose inside it.

```text
if (long) (close)
if (pnl > 50) (close) else (sleep 5m)
if (position) (chase sell %all% 100% reduce) else (echo "nothing to close")
```

### `end`

A bare `end` in the chain (or inside an `if` branch) **terminates the
outer chain**. Anything after is dropped.

```text
buy $100 at $entry -1%
wait fill
if (pnl < -10) (close; end)
buy $100 at $entry +1%
```

If the `if` fires, `end` cancels the trailing `buy`.

Source: `preprocess/ifElse.ts`, `preprocess.ts:204-213` (end),
`:218-275` (if recursion).

---

## `retry`

```text
retry N <body>
```

Re-runs `<body>` up to N times after a failure. First attempt is "try 0",
so `retry 3` means up to 4 total attempts.

If an exchange reports an unknown write outcome or a batch reports accepted
orders before a failure, the CLI stops retrying that command. Reconcile the
account before placing a replacement; a timeout does not prove an order failed.

| Bound       | Behavior                                                       |
| ----------- | -------------------------------------------------------------- |
| N = 0       | Same as not using `retry`                                      |
| N ∈ [0, 10] | Accepted                                                       |
| N > 10      | Falls through as a normal step — the dispatcher will reject it |

```text
retry 3 chase buy $100 to 1%
retry 5 close
```

Source: `preprocess/preprocess.ts:31-32, 146-157`. Runner side at
`apps/cli/src/repl.ts:1618+`.

---

## Chain modifiers

Mid-chain flags that apply to **every subsequent** step in the same chain.
Idempotent — won't double-inject if the keyword is already on the step.

| Modifier              | Effect                                                                      |
| --------------------- | --------------------------------------------------------------------------- |
| `set ec`              | Continue-on-error — subsequent failures print as warnings, chain runs on    |
| `set ro` / `unset ro` | Inject `reduce` into subsequent `buy`/`sell`/`scale`/`swarm`/`chase`/`twap` |
| `set po` / `unset po` | Inject `po` into subsequent `buy`/`sell` heads. Market orders ignore it     |

```text
set ec
cancel
close
nuke                       # all three continue even if one fails
```

```text
set ro
close                       # reduce-only is implicit now
cancel
buy $100                    # still gets reduce injected
```

```text
set po
buy $100 at 50000
sell $100 at 50100
unset po
buy $50 best                # market-best, post-only no longer active
```

Source: `preprocess/preprocess.ts:20-57, 133-142, 332-356`.

---

## `po` keyword on a single order

You can mark one order post-only without affecting the rest of the chain:

```text
buy $100 at 50000 po
```

Same effect as `set po; buy $100 at 50000; unset po`.

---

## Composition cheatsheet

A few canonical combinations:

```text
# retry × repeat × sleep × set ec
repeat 5 (set ec; retry 2 buy 1$; sleep 1m)

# one-shot conditional
if (long) (close)

# fan-out reduce-only chase
loop longs: chase sell 50% reduce

# capture + wait fill + close
@o = buy $100 at 50000; wait fill @o; close

# soft-error cleanup
set ec; cancel; close; nuke

# chain reduce-only
set ro; close; cancel; buy $100

# mid-chain post-only toggle
set po; buy $100 at 50000; sell $100 at 50100; unset po; buy $50 best
```
