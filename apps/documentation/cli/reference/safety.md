---
title: Safety
sidebar_position: 6
---

# Safety

Guards designed to keep an off-script command from doing damage.
All of these are state that lives in the CLI, not the exchange.

> **Standalone CLI only.** The web terminal does not enforce these guards: it
> refuses to turn them on, and its orders, including webhook alerts, are always
> live.

| Guard                     | Scope                                         | Persistence                       |
| ------------------------- | --------------------------------------------- | --------------------------------- |
| [Simulation](#simulation) | All write-ops on every account                | Persistent (per CLI install)      |
| [Fatfinger](#fatfinger)   | Single-order size cap, per symbol             | Persistent                        |
| [Budget](#budget)         | Cumulative notional cap on new exposure       | Cap persistent; spend per-session |
| [Whitelist](#whitelist)   | Symbols that survive `close longs/shorts/all` | Persistent                        |

---

## Simulation

Aliases: `sim`, `dry-run`.

```text
simulation              # show status
simulation on           # enable
simulation off          # disable
simulation status       # same as bare form
sim on
dry-run off
```

When ON, every write-op (place / cancel / close / move / bump) short-circuits
at `simulationGate(ctx)` before any exchange call. Reads (`positions`,
`orders`, `margin`, etc.) are unaffected.

Persisted to `~/.tealstreet/simulation.json`. Source: `SimulationCommand.ts`.

> Use simulation when porting a new alias or recipe. Confirm the chain prints
> the right intent before you flip simulation off.

---

## Fatfinger

A per-symbol cap on individual order size, in **contracts**. Checked just
before `placeOrder` on:

- `buy`, `sell`
- `scale`, `swarm`
- `chase`
- `twap`

Reduce-only orders are skipped — closing a position cannot blow it up.

| Form                    | Effect                     |
| ----------------------- | -------------------------- |
| `fatfinger`             | List all configured limits |
| `fatfinger <contracts>` | Set on focused symbol      |
| `fatfinger 0.5 BTCUSDT` | Set on named symbol        |
| `unfatfinger <symbol>`  | Remove for one symbol      |
| `unfatfinger all`       | Remove every limit         |

Persisted to `~/.tealstreet/fatfingers.json`. Source: `FatfingerCommand.ts`,
`TradingBaseCommand.ts:enforceFatfinger`.

---

## Budget

An **opt-in, off-by-default** cumulative cap (in USD notional) on
**exposure-increasing** orders placed this session. Where fatfinger caps
a single order, budget caps the running total — useful for a hard ceiling
on how much you can open in a sitting.

```text
budget                  # show the cap and how much is spent
budget $5000            # set a $5000 cap (also: budget 5000)
budget off              # disable (also: budget 0)
budget reset            # zero the spent accumulator, keep the cap
```

| Form           | Effect                      |
| -------------- | --------------------------- |
| `budget`       | Show cap + session spend    |
| `budget $5000` | Set the cap (USD)           |
| `budget off`   | Disable                     |
| `budget reset` | Reset the spent accumulator |

The **cap persists** across sessions (`~/.tealstreet/budget.json`); the
**spent total is per-session** and resets on restart, on `budget reset`,
and whenever you change the cap. Each exposure-increasing order
(`buy`/`sell`/`scale`/`swarm`) charges its USD notional against the total
and is blocked once `spent + order > cap`. **Reduce-only / `close` /
`cancel` never count** — the budget gates new exposure, not exits. _Known
limitation:_ `twap` / `chase` task-spawners are not yet budget-counted
(their slices fire in a worker outside the command's state).

Persisted to `~/.tealstreet/budget.json`. Source: `BudgetCommand.ts`,
`TradingBaseCommand.ts:enforceBudget`.

---

## Whitelist

Symbols you don't want a global "close all" to touch. Applies to:

- `close longs`
- `close shorts`
- `close allpos` / `close all`

Per-symbol close calls (`close BTCUSDT`, `close long`) ignore the
whitelist — the guard is only for the global variants.

| Form                  | Effect                        |
| --------------------- | ----------------------------- |
| `whitelist`           | List                          |
| `whitelist BTCUSDT`   | Add (uppercased before store) |
| `unwhitelist BTCUSDT` | Remove                        |
| `unwhitelist all`     | Remove every entry            |

Persisted to `~/.tealstreet/whitelist.json`. Source: `WhitelistCommand.ts`.

---

## Combining guards

These layer. Simulation short-circuits **before** fatfinger checks; fatfinger
fires **before** the exchange call.

```text
simulation on
buy 99999 BTCUSDT        # blocked by simulation — no fatfinger needed

simulation off
fatfinger 0.5 BTCUSDT
buy $5000000 BTCUSDT     # blocked by fatfinger

whitelist BTCUSDT
close longs              # touches every long except BTCUSDT
```

---

## Symbol resolution fails closed

`buy` / `sell` reject an explicit symbol token that doesn't resolve to a
market on the connected exchange instead of silently routing the order to
the focused symbol:

```text
buy $100 XBTUSD          # error: Unknown symbol "XBTUSD" (on an exchange with no XBTUSD)
buy $100                 # OK — no symbol given, uses the focused symbol
buy $100 BTCUSDT         # OK — explicit, resolvable symbol
```

This catches typos like trading the wrong contract on a multi-variant
exchange (e.g. `XBTUSD` vs `XBTUSDT`). Omitting the symbol entirely still
falls back to the focused symbol by design. Source: `BaseBuySellCommand.ts`.

---

## Venue rules the CLI refuses to break

Beyond the guards you configure, some orders are refused because the venue
cannot accept them. These fire before anything reaches the exchange, so you
get a named error rather than a venue rejection:

```text
buy 0.0004 BTCUSDT     # error: order size 0.0004 rounds to 0 for 'BTCUSDT' — below the venue lot step
stop 58000             # on Paradex: 'paradex' does not support Full (Infinity) TP/SL orders
```

A bare `stop <price>` with no size means "cover the whole position", which is
sent as a zero-size stop. Most venues read that as a Full stop; twelve do not
— Weex, Backpack, Crypto.com, Bitrue, GRVT, Apex Omni, Coinbase, Kraken,
Kraken Spot, Paradex, Polymarket and Lighter. Give the stop an explicit size
there.

An order for a symbol whose market has not loaded yet is also refused rather
than sent unrounded. On a global `close`, that refusal is per position — one
symbol that is not ready does not stop the rest of the sweep.

See [Sizing and prices](./sizing-and-prices.md#every-price-and-size-is-rounded-to-the-venue-before-it-is-sent)
for the rounding itself.

---

## Degraded-connection lock

When an account's **private data stream goes stale** (the connection was
up and has lost its heartbeat), commands that **open or increase
exposure** are blocked — your position/order snapshot can't be trusted,
so firing new orders blindly is unsafe:

```text
buy $100                 # error: connection degraded — refusing to open exposure
scale buy $1000 into 10  # blocked
twap buy $500 into 5 over 30   # blocked (won't start the task)

buy $100 reduce          # OK — reduce-only reduces risk
close BTCUSDT            # OK
cancel                   # OK
nuke                     # OK
```

Risk-**reducing** actions (`close`, `cancel`, `nuke`, and any
`reduce`-only order) stay allowed so a degraded connection can never trap
you in a position you can't exit. The lock **fails open** — if the
private stream was never up or the exchange doesn't report health, it
doesn't block. Run `health` to see connection status. Source:
`BaseCommand.healthGate`.
