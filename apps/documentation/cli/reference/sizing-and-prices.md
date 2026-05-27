---
title: Sizing & prices
sidebar_position: 4
---

# Sizing & prices

The grammar that fills `<size>` and `<price>` slots in every trading command.

---

## Size

| Form     | Meaning                                                |
| -------- | ------------------------------------------------------ |
| `$1000`  | USD notional, prefix form                              |
| `1000$`  | USD notional, suffix form                              |
| `50%`    | Percent — scope depends on command class _(see below)_ |
| `50%bal` | Explicit percent of **free balance**                   |
| `50%pos` | Explicit percent of **open position**                  |

### Default percent scope

Bare `50%` doesn't mean the same thing for every command:

| Command class                                          | Default scope |
| ------------------------------------------------------ | ------------- |
| `buy` / `sell` / `scale` / `swarm` / `chase` (entries) | balance       |
| `buy` / `sell` with `reduce` (closing)                 | position      |
| `close` / `stop` (exits)                               | position      |

A `reduce`-only `buy`/`sell` is closing, not opening, so a bare `%`
resolves against the **open position** (e.g. `sell 50% reduce` closes
half your position), not free balance. Use `%bal` or `%pos` explicitly
when you want to override the default.

A size that rounds to **0** at the market's amount precision is rejected
rather than sent as an empty order.

### Bare numbers are rejected

```text
buy 100        # ❌ "USD or % required"
buy $100       # ✅
buy 100$       # ✅
buy 0.01 BTC   # ✅ — 0.01 is interpreted as raw contracts when no other size shape matches
```

The rejection of bare integers ≥ 100 is deliberate — it stops a typo like
`buy 100` from coercing to a market order with notional ambiguity.
Source: `TradingBaseCommand.ts`.

### `tokenLooksLikeNonSymbol` filter

Tokens that already parse as size or percent (`$100`, `100$`, `50%`) are
**filtered out** of fuzzy symbol matching before the parser ever tries to
resolve a market. This stops `1000$` from being absorbed when an exchange
happens to have a base symbol of `"1000"` (some Binance memecoin pairs).

Source: `TradingBaseCommand.ts:282-301`.

### Inverse contracts

For inverse markets (BitMEX XBTUSD, Bybit inverse, Bitget inverse, …) sizing
uses **notional ÷ contractSize** instead of notional ÷ price. `contractSize`
defaults to 1 and should be populated by the exchange adapter for venues
with non-1 contract sizes (Binance Coin-M, Deribit, …).

Source: `TradingBaseCommand.ts:166-208`.

---

## Price

| Form                         | Meaning                                                    |
| ---------------------------- | ---------------------------------------------------------- |
| `50000`                      | Absolute price                                             |
| `+100` / `-100`              | Relative offset (added to last)                            |
| `1%` / `-0.5%`               | Percent offset (anchor × (1 + pct/100))                    |
| `$anchor`                    | Constant — `$entry`, `$last`, etc.                         |
| `$anchor +N` / `+N%` / `-N%` | Anchor with [inline offset](./variables.md#inline-offsets) |

See [Variables](./variables.md) for the full list of price anchors and
which ones support inline offsets.

---

## Trailing modifiers

Per-command modifier vocabulary. All are optional and most can stack.

| Modifier                            | Used by                                                     | Effect                                                               |
| ----------------------------------- | ----------------------------------------------------------- | -------------------------------------------------------------------- |
| `at <price>`                        | `buy`, `sell`                                               | Convert to limit at given price                                      |
| `stop <price>`                      | `buy`, `sell`                                               | Bracketed stop loss attached to the entry                            |
| `trigger <abs-price>`               | `stop` (paired with `at`)                                   | Limit-stop — trigger differs from limit                              |
| `to <length>`                       | `chase`                                                     | Terminate-at distance (abs / `N%` / `$N` / `N$`)                     |
| `into <N>`                          | `scale`, `swarm`, `twap`                                    | Slice count                                                          |
| `over <minutes>`                    | `twap`                                                      | Time horizon                                                         |
| `from <p1> to <p2>`                 | `scale`, `cancel`                                           | Price range (scale: ladder; cancel: filter)                          |
| `reduce`                            | `buy`, `sell`, `scale`, `swarm`, `chase`, `twap` _(parsed)_ | Reduce-only                                                          |
| `po`                                | `buy`, `sell` (limit only)                                  | Post-only — see [Workflow → `set po`](./workflow.md#chain-modifiers) |
| `best`                              | `buy`, `sell`                                               | Limit at best bid (buy) / best ask (sell)                            |
| `taker`                             | `scale`                                                     | Drop the default post-only TIF                                       |
| `irregular`                         | `swarm`                                                     | Randomise slice weights in `[0.5x..1.5x]`                            |
| `chaser`                            | `twap`                                                      | Run a chaser per slice                                               |
| `keep`                              | `chase cancel`                                              | Stop chaser but leave the resting order                              |
| `last` / `top` / `bottom` / `first` | `cancel`, `bump`                                            | Sort-and-pick selectors                                              |
| `from <p1> to <p2>`                 | `cancel`                                                    | Price-range filter (limits only)                                     |

---

## Putting it together

A few full sentences to anchor the grammar:

```text
buy $100 at $bid +0.1% post-only reduce
chase sell %all% 20% reduce to 1%
scale buy $1000 into 10 from -0.25% to -1% reduce
twap chase buy $500 into 5 over 30 reduce timeout 30% next fail next
cancel buys ro from 49000 to 50000
```

If a sentence parses, the result lines up with the table above. If it
doesn't, drop one modifier and try again — the parser bails fast on
ambiguity rather than guessing.
