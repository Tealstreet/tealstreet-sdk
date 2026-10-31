---
title: Variables & constants
sidebar_position: 2
---

# Variables & constants

The CLI exposes a fixed set of built-in constants as `$tokens`. Any one of
them can appear wherever a price or a number is expected. They're resolved
fresh on every command — there's no caching layer.

User-defined variables (`@name`) live under
[Aliases & user variables](./aliases-and-vars.md). This page covers the
read-only `$` tokens plus the math + offset grammar that operates on them.

---

## Position-anchored prices

The focused position is the one for the symbol you have set with `switch`.

| Token     | Reads                                                |
| --------- | ---------------------------------------------------- |
| `$entry`  | Entry price of focused position                      |
| `$lentry` | Long entry price (focused symbol, hedge-mode aware)  |
| `$sentry` | Short entry price (focused symbol, hedge-mode aware) |
| `$liq`    | Liquidation price — errors when ambiguous or zero    |
| `$roe<N>` | The price at which this position's ROE would be N%   |

All five accept an [inline offset](#inline-offsets).

`$roe` carries its number in the token — `$roe25`, `$roe101`, `$roe-50` — and is
the same ROE the positions table shows, so a target you type and a figure you
read mean the same thing. It scales with leverage: at 10x, `$roe100` is a 10%
price move; at 20x it is 5%. It errors rather than guessing when leverage is
unavailable, because a 0 would silently put every target at your entry.

A take-profit ladder and a trailing runner, denominated in ROE:

```
sell 30%pos $roe25 reduce; sell 30%pos $roe60 reduce; sell 30%pos $roe100 reduce
wait $last >= $roe101; trail 20%
```

Placing all three legs up front is what makes each one 30% of the ORIGINAL
position — they size against the same unfilled position. Once they fill, what
is left is the runner, and `trail` takes the whole remaining position.

---

## Ticker-anchored prices

| Token                | Reads                                                       |
| -------------------- | ----------------------------------------------------------- |
| `$bid`               | Top-of-book bid                                             |
| `$ask`               | Top-of-book ask                                             |
| `$last`              | Last trade price                                            |
| `$mark`              | Mark price                                                  |
| `$index`             | Index price                                                 |
| `$mid` / `$midprice` | Exchange-published mid if present, else synthesized average |

`$bid`, `$ask`, `$last`, `$mark`, `$index` accept inline offsets. `$mid` does
not have an offset shortcut — combine with math (`$mid +1*$ticksize`) if you
need one.

> **Why `$mid` is different**: Hyperliquid publishes a true mid (`ticker.midPrice`)
> separate from `(bid+ask)/2`. The CLI prefers the exchange value when present.

---

## Position sizes

| Token    | Reads                                               |
| -------- | --------------------------------------------------- |
| `$size`  | Total contracts of focused position (sum if hedged) |
| `$lsize` | Long position contracts                             |
| `$ssize` | Short position contracts                            |

---

## Balance, PnL, and exposure

| Token            | Reads                                                |
| ---------------- | ---------------------------------------------------- |
| `$free`          | Free balance                                         |
| `$used`          | Used / locked balance                                |
| `$total`         | Total balance                                        |
| `$upnl`          | Unrealized PnL (exchange-reported, account-wide)     |
| `$pnl`           | Unrealized PnL summed on the focused symbol          |
| `$totalupnl`     | Unrealized PnL summed across all positions           |
| `$totallongusd`  | Long notional (USD) across all positions             |
| `$totalshortusd` | Short notional (USD) across all positions            |
| `$totallong`     | `$totallongusd / $total`                             |
| `$totalshort`    | `$totalshortusd / $total`                            |
| `$margin`        | Focused-symbol position margin (notional ÷ leverage) |

---

## Market shape

| Token       | Reads                                         |
| ----------- | --------------------------------------------- |
| `$ticksize` | `market.precision.price` for focused symbol   |
| `$minsize`  | `market.limits.amount.min` for focused symbol |

---

## Synthesized

| Token     | Returns                                                |
| --------- | ------------------------------------------------------ |
| `$random` | Uniform in `[0.35, 1)` — re-evaluated per substitution |

> **`$random` floor is 0.35, not 0** — to avoid sub-min-size rounding when
> used as a size multiplier (`buy $100*$random`).

---

## UI-only constants &nbsp; _(web terminal)_

These resolve only in the in-app web CLI. Standalone CLI throws "only
available in the web terminal" if you reference them.

| Token                 | Reads                                      |
| --------------------- | ------------------------------------------ |
| `$click`              | Last chart-click price from a linked chart |
| `$ordersize`          | Current size in the linked order form      |
| `$quick1` … `$quick7` | Quick-size slots 1 through 7               |

Resolved through `PreprocessContext.uiResolvers`
(`apps/web/src/components/cli/uiTokenResolvers.ts`).

---

## Inline offsets

Any **price anchor** can be followed by a second token of shape
`[+-]<number>[%]`. The anchor and offset collapse into a single absolute
price before downstream parsing.

```text
buy $100 at $entry +1%      # buy 1% above entry
sell $100 at $liq -5%       # 5% under liquidation
chase buy $100 to $bid +0.1%
stop at $entry -2%
```

The anchor reads the **focused symbol**. With a symbol override, pass an absolute
price. A bare negative offset with no anchor is
[refused](./sizing-and-prices.md#negative-offsets).

With `sets`, write the source as **one token** (`sets @x $entry*0.99`).
`sets @x $entry -1%` loses the offset and `@x` reads as `$entry`. `set` is not
affected.

Anchors that accept inline offsets:

```text
$entry  $lentry  $sentry  $liq  $roe<N>  $bid  $ask  $last  $mark  $index
```

Source: `packages/cli-core/src/preprocess/substitute.ts:322-339`.

---

## Within-token math

Tokens shaped like `<operand> <op> <operand>` (with no spaces between, or
spaces only between operands and operators) get evaluated as expressions.

| Element     | Allowed                                         |
| ----------- | ----------------------------------------------- |
| Operands    | Float literals, `$constants`, `@vars`           |
| Operators   | `+` `-` `*` `/`                                 |
| Precedence  | `*`/`/` > `+`/`-`, left-to-right within a level |
| Unary minus | Allowed on **first** operand only — `-5*@x`     |
| Parens      | Not supported                                   |
| Functions   | Not supported                                   |

```text
sets @half $size/2
sets @stop $entry-3*$ticksize
buy $100*$random
```

Source: `packages/cli-core/src/preprocess/math.ts`.

---

## Composing math with `sets`

`set` snapshots a value; `sets` re-evaluates on every read. The two combine
cleanly with math:

```text
sets @stopgap $entry*0.99      # reactive — re-reads $entry each time
set  @epoch   $last            # snapshot — frozen at the moment you ran set
echo @stopgap                  # live
echo @epoch                    # frozen
```

See [Aliases & user variables](./aliases-and-vars.md) for the full
`set`/`sets`/`track`/`print` lifecycle.

---

## At-a-glance cheatsheet

```text
PRICES                       SIZES + BALANCE              MARKET
$entry  $lentry  $sentry     $size  $lsize  $ssize        $ticksize
$bid    $ask    $last        $free  $used   $total        $minsize
$mark   $index  $mid         $upnl  $pnl    $totalupnl
$liq                         $totallongusd   $totalshortusd
                             $totallong      $totalshort
                             $margin

SYNTH                        WEB-ONLY
$random                      $click  $ordersize  $quick1..$quick7

INLINE OFFSET (any price anchor except $mid)
  $entry +1%   $liq -5%   $bid +100   $ask -0.05%

MATH (within a single token, no parens)
  $entry-3*$ticksize    $size/2    $100*$random
```
