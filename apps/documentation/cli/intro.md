---
title: Welcome
slug: /
sidebar_position: 0
---

# The Tealstreet CLI

A typing-first trading interface. Place orders, build chains, run TWAPs,
and listen for webhooks — all from a prompt that feels like a shell and
trades like a terminal.

```text
:hltest [BTC-PERP $81,284.5] > buy $100 stop -1%
:hltest [BTC-PERP $81,284.5] > chase sell %all% 20% reduce to 1%
:hltest [BTC-PERP $81,284.5] > set ec; cancel; close; nuke
```

The CLI ships in two runtimes:

| Runtime            | Where                               | Best for                                                                              |
| ------------------ | ----------------------------------- | ------------------------------------------------------------------------------------- |
| **Web CLI**        | A panel inside the trading terminal | Quick fire-and-forget trades with the terminal already up                             |
| **Standalone CLI** | `tealstreet` binary on your machine | Watch modes, recording, hooks, TradingView webhooks, leaving things running overnight |

Same grammar, same commands, same constants. The web CLI is a strict
subset — every standalone command except watch modes, hooks, recording,
and a few host-process commands also works in the web CLI. See
[Web CLI vs standalone](./reference/web-cli.md).

---

## 30-second tour

1. **Place orders by intent**, not by form-filling. `buy $100` is a $100
   notional market buy on the focused symbol.
2. **Chain steps with `;`**. `buy $100; cancel; close` runs three things
   without waiting for round-trips — fire-and-don't-await is the default,
   and [`wait`](./reference/workflow.md#wait) is how you opt into blocking
   when you need it.
3. **Variables and constants**. `$entry`, `$bid`, `$last`, `$pnl` resolve
   live. User vars (`@x`) come from [`set`](./reference/aliases-and-vars.md)
   for snapshots, [`sets`](./reference/aliases-and-vars.md#sets) for
   reactive expressions, [`track`](./reference/aliases-and-vars.md#track)
   for high/low-water marks.
4. **Aliases for whole chains**. `alias z "cancel; close"` lets you bind a
   common cleanup to one keystroke.
5. **Listener mode** turns the standalone CLI into a webhook target — pair
   it with the web terminal or with TradingView alerts.

---

## Where to go next

| You want to…                                    | Read                                                   |
| ----------------------------------------------- | ------------------------------------------------------ |
| Install the standalone CLI                      | [Install](./install.md)                                |
| Run a few real recipes                          | [Quickstart](./quickstart.md)                          |
| See every command in one page                   | [Commands reference](./reference/commands.md)          |
| Understand `$entry`, `$bid`, math, offsets      | [Variables & constants](./reference/variables.md)      |
| Master `;`, `&`, loops, `wait`, `if`, retry     | [Workflow](./reference/workflow.md)                    |
| Customize the status footer / where output goes | [Output](./reference/output.md)                        |
| Send a TradingView alert into the CLI           | [Hooks & webhooks](./operations/hooks-and-webhooks.md) |
| Pair the web terminal to a standalone listener  | [Pairing](./operations/pairing.md)                     |

---

## House rules

- **Fire-and-don't-await**. `;` and `&` don't block on round-trips. If you
  need to wait, use [`wait`](./reference/workflow.md#wait) or
  [capture syntax](./reference/aliases-and-vars.md#capture-syntax).
- **Bare numbers are rejected** as sizes. `buy 100` errors with "USD or %
  required". Use `$100`, `100$`, or `50%`.
- **Reduce-only and post-only** can be set per-order (`buy … reduce`,
  `buy … po`) or chain-wide (`set ro` / `set po`).
- **Simulation** is a single toggle. Flip it on with `sim on` before trying
  anything new.
