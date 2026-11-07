---
title: Quickstart
sidebar_position: 2
---

# Quickstart

> **New to the CLI?** Start with [Get started](./getting-started.md): install,
> sign in, sync your keys from the web app, and a first order.

Seven runnable recipes plus the current webhook status. Pick the one
closest to what you're doing and paste it. Every trading recipe assumes
you already have a focused symbol and an account selected — run
[`status`](./reference/commands.md#status) to confirm.

> **First-timer?** Flip simulation on for the first hour:
> `sim on`. Every write-op short-circuits without hitting the exchange.
> Turn it off with `sim off` when you're confident. See
> [Safety → Simulation](./reference/safety.md#simulation).

Each recipe links to a deep-dive page with variations, gotchas, and
common follow-ups.

---

## 1. Bracket order — market entry with attached stop

```text
buy $100 stop $last -1%
```

A $100 notional market buy on the focused symbol, with a reduce-only stop
**1% below the last price when you send it**. A bare `stop -1%` is
[refused for now](./reference/sizing-and-prices.md#negative-offsets). The stop attaches automatically — no separate
`stop` command needed.

Variations:

```text
buy $100 at $bid +0.1% stop $last -1%   # entry above the bid, same stop
buy 50% stop $last -2%                  # size by % balance
```

→ [Deep dive: Bracket order](./recipes/bracket-order.md)

---

## 2. Chaser — passive limit that re-pegs to top of book

```text
chase buy $500 to 1%
```

Walks the limit up the bid (or down the ask, for sells) every time the
top of book moves. Terminates if the price drifts more than 1% from the
starting price.

Variations:

```text
chase sell $200 reduce                # reduce-only chaser to close
chase buy WIF,ENA $100                # multi-symbol fan-out
chase cancel keep                     # stop chasing, leave the order
```

→ [Deep dive: Chaser](./recipes/chaser.md)

---

## 3. TWAP — spread an order over time

```text
twap chase buy $1000 into 10 over 60
```

Ten slices of $100 each over 60 minutes, each one fired as a chaser.
Default behavior: if a slice's chaser hasn't filled in 50% of the slice
window, it goes market. Tweak with `timeout <pct> <action>` and `fail
<action>`.

Variations:

```text
twap buy $500 into 5 over 30 reduce       # plain TWAP, reduce-only
twap chase sell $1k into 10 over 60 timeout 30% next fail next
```

→ [Deep dive: TWAP](./recipes/twap.md)

---

## 4. Scale ladder — N limits between two prices

```text
scale buy $1000 into 10 from $last -0.25% to $last -1%
```

Ten limit orders spaced evenly between 0.25% and 1% below the last
price, totalling $1000 notional.

Variations:

```text
scale sell $2000 into 20 from 50000 to 48500 reduce   # absolute prices, reduce-only
scale buy 5 into 10 from 31000 to 32000 ETHUSDT       # raw size + explicit symbol
```

→ [Deep dive: Scale ladder](./recipes/scale-ladder.md)

---

## 5. Trailing stop — track the high, close when retraced

```text
track max @hi $last
sets @stop @hi-1
wait @last < @stop
close
```

Three steps:

1. `track max` ratchets `@hi` upward as `$last` moves up.
2. `sets @stop` reactively re-computes the stop level as `@hi - 1%`.
3. `wait` blocks until the last price drops below the trailing stop,
   then `close` runs.

`echo @hi` ratchets the tracker on read; `print @hi` does not. See
[Aliases & user variables](./reference/aliases-and-vars.md#track).

→ [Deep dive: Trailing stop](./recipes/trailing-stop.md)

---

## 6. Fan out across symbols

```text
chase buy WIFUSDT,ENAUSDT,DOGEUSDT $100 reduce
```

Comma-separated symbol list works for `buy` / `sell` / `chase` / `close` /
`last-price` / `mark-price` / `index-price`. Each symbol fans out into a
separate command with its own size resolution.

```text
loop longs: chase sell 50% reduce      # close every long with a chaser
close longs                            # close every long, market
```

→ [Deep dive: Fan out across symbols](./recipes/fan-out-symbols.md)

---

## 7. Listener — drive the standalone CLI from the web terminal

Inside the standalone REPL:

```text
> # already listening on 127.0.0.1:53219
```

In the web terminal, set the active CLI tab's target to **`listener`**.
The first connection prompts the standalone to approve the browser; after
that every command you type in the web CLI runs in the standalone process
— picking up that machine's accounts, hooks, simulation state, and chasers.

→ [Deep dive: Listener from web](./recipes/listener-from-web.md)

---

## 8. TradingView webhooks

Paste your webhook URL (Settings → Webhooks) into a TradingView alert and
put CLI commands in the alert message. The logged-in REPL receives them and
runs them here:

```text
buy $250 SOLUSDT at 150
```

`webhook` shows which listener receives your webhooks; `webhook claim` moves
them to this CLI.

→ [Recipe: Webhook from TradingView](./recipes/webhook-from-tradingview.md)

Webhooks are separate from exchange geo-restrictions. If the CLI can execute a
command but Bybit returns a 403 geo-block, use a VPN or add the account
with Tealstreet proxy routing:

```text
> accounts add -n mybybit -e bybit -k <KEY> -s <SECRET> --tealstreet-proxy --proxy-region SG
```

→ [Hooks & webhooks](./operations/hooks-and-webhooks.md)

---

## Where to next

| You want…                             | Go to                                             |
| ------------------------------------- | ------------------------------------------------- |
| One command, every flag               | [Commands reference](./reference/commands.md)     |
| Math, `$entry`, inline offsets        | [Variables & constants](./reference/variables.md) |
| `;` / `&` / loops / wait / if / retry | [Workflow](./reference/workflow.md)               |
| Hooks, listener, pairing, recording   | [Operations](./operations/listener.md)            |
