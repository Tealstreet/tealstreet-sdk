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

### Every price and size is rounded to the venue before it is sent

Whatever you type is snapped to the market's real tick size and lot step
before it leaves your machine:

| You type            | What is sent                                                                    |
| ------------------- | ------------------------------------------------------------------------------- |
| a limit price       | rounded to the market's tick size                                                |
| a stop `trigger`    | rounded to the market's tick size                                                |
| a size              | rounded to the market's lot step                                                 |
| a size below one lot | **rejected** — `order size 0.0004 rounds to 0 for 'BTCUSDT'`                    |

A `reduce`-only market close is the one exception to the last row: a residual
position smaller than one lot step keeps its raw size, so dust can still be
closed rather than becoming uncloseable.

Two rules follow from the venue rather than from your input:

- **Hyperliquid spot sizes round DOWN**, not to nearest, so a buy can never ask
  for more base than the wallet holds. Perps on the same connection still round
  to nearest.
- **On nine venues a `reduce`-only order is shrunk** against the reduce-only
  orders already resting ahead of it, which they would otherwise reject:
  bitunix, weex, mexc, bitget, bitrue, woo, hyperliquid, grvt and coinbase.
  The flag is `autoDownSizeReduceOrders` in `exchangeMetadata.ts` — read it
  there rather than trusting this list.

This is the same rounding the web app and the mobile app apply — one
implementation, so the three cannot disagree about a number.

### Bare numbers are rejected

```text
buy 100        # ❌ "USD or % required"
buy $100       # ✅
buy 100$       # ✅
buy 0.01 BTC   # ✅ — bare number in the SIZE slot is raw contracts, when no other size shape matches
```

A bare number where a PRICE was meant is rejected at any magnitude — `buy $100
50000` and `sell 33$ 0.051533 reduce` both ask "did you mean `at <price>`?"
rather than dropping the number and going out at market. This used to apply
only to numbers ≥ 100, a threshold that assumed BTC-shaped prices: on a coin
quoted in cents the check never fired and the order silently market-filled.

That check runs over the tokens AFTER the size, so a bare size in its own slot
(`buy 0.01 BTC`, `buy BTCUSDT 0.01`) is untouched by it.
Source: `TradingBaseCommand.ts`, `BaseBuySellCommand.ts`.

### `tokenLooksLikeNonSymbol` filter

Tokens that already parse as size or percent (`$100`, `100$`, `50%`) are
**filtered out** of fuzzy symbol matching before the parser ever tries to
resolve a market. This stops `1000$` from being absorbed when an exchange
happens to have a base symbol of `"1000"` (some Binance memecoin pairs).

Source: `TradingBaseCommand.ts:282-301`.

### Inverse contracts

For inverse markets (Bybit inverse, Bitget inverse, …) sizing
uses **notional ÷ contractSize** instead of notional ÷ price. `contractSize`
defaults to 1 and should be populated by the exchange adapter for venues
with non-1 contract sizes (Binance Coin-M, Deribit, …).

Source: `TradingBaseCommand.ts:166-208`.

---

## Price

| Form                         | Meaning                                                    |
| ---------------------------- | ---------------------------------------------------------- |
| `50000`                      | Absolute price                                             |
| `+100`                       | Relative offset (added to last)                            |
| `1%`                         | Percent offset (anchor × (1 + pct/100))                    |
| `$anchor`                    | Constant — `$entry`, `$last`, etc.                         |
| `$anchor +N` / `+N%` / `-N%` | Anchor with [inline offset](./variables.md#inline-offsets) |

### Negative offsets

**A token starting with `-` is refused for now on order commands**, and nothing
is placed: `buy`, `sell`, `close`, `stop`, `be`, `trail`, `move`, `scale`, `bump`,
`chase`, `twap`, `swarm`, `nuke`, `cancel`, `leverage` and `kill`.

| Refused              | Use instead                                    |
| -------------------- | ---------------------------------------------- |
| `buy $100 stop -1%`  | `buy $100 stop $last -1%` or `stop 49500`      |
| `stop -1%`           | `stop $entry -1%` or `stop 49500`              |
| `be -0.5%`           | `be $entry -0.5%`                              |
| `move <id> -5`       | `move <id> 49995`                              |
| `bump -5`            | No replacement yet; `move` each order          |
| `stop $entry -3*$ticksize` | `stop $entry-3*$ticksize`                |

An anchor with an offset, such as `$last -1%`, is allowed because it becomes one
absolute price before the command runs. **Anchors read the focused symbol**, so
with a symbol override like `buy $250 SOLUSDT at ...` pass an absolute price.

See [Variables](./variables.md) for the full list of price anchors and
which ones support inline offsets.

---

## Trailing modifiers

Per-command modifier vocabulary. All are optional; whether they can stack
depends on the command's grammar.

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

`scale` is linear-only. After its price range it accepts `reduce`, `taker`,
or both, followed by an optional symbol scope. `cubic`, `icubic`, and other
distribution names are not implemented; they are rejected with
`modifier '<x>' is not implemented; supported: reduce, taker`.

---

## Putting it together

A few full sentences to anchor the grammar:

```text
buy $100 at $bid +0.1% po reduce
chase sell %all% 20% reduce to 1%
scale buy $1000 into 10 from $last -0.25% to $last -1% reduce
twap chase buy $500 into 5 over 30 reduce timeout 30% next fail next
cancel buys ro from 49000 to 50000
```

If a sentence parses, the result lines up with the table above. If it
doesn't, drop one modifier and try again — the parser bails fast on
ambiguity rather than guessing.
