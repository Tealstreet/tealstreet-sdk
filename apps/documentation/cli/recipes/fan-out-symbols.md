---
title: Fan out across symbols
---

Three ways to apply one command to many symbols at once: the **comma-list
syntax** in the symbol slot, the **`%all%` token** under `chase reduce`, and
the **`loop` primitive** that iterates over positions. Use these to manage
baskets without retyping per symbol.

## Minimum viable

```text
buy WIFUSDT,ENAUSDT $100
```

Two $100 market buys, one per symbol. Each leg resolves size independently —
notional stays the same across legs.

## Variations

```text
last-price BTC,ETH,SOL
```

Read-only fan-out — `last-price`, `mark-price`, `index-price` all accept
the same comma list. Returns one row per symbol.

```text
chase buy WIF,ENA,DOGE $100 reduce
```

Three reduce-only chasers, one per symbol. Each one runs as its own
background task — kill them individually (`kill <id>`) or in bulk
(`chasers cancel all`).

```text
chase sell %all% 20% reduce
```

`%all%` expands to **every position on the implied side**. With `sell` +
`reduce` it picks every long; with `buy` + `reduce` it picks every short.
Each expansion spawns a reduce-only chaser closing 20% of that position.
Only valid under `chase … reduce`.

```text
loop longs: chase sell 50% reduce
```

`loop` iterates over a position set and appends each symbol to the body.
Variants: `longs`, `shorts`, `openpos` (alias `open`), `allpos` (alias
`all`). Body runs once per matched position.

```text
close longs
close shorts
close all
```

`close` has its own global variants — `longs` / `shorts` / `allpos` /
`all` — that close every position on the side, no `loop` needed. These
honor the [whitelist](../reference/safety.md#whitelist).

## Gotchas

- **Multi-statement bodies aren't supported in `loop`.** `loop longs:
cancel; close` does NOT do what you want — the `; close` runs once after
  the loop, not per iteration. Alias the body and loop the alias:

  ```text
  alias cleanup "cancel; close"
  loop longs: cleanup
  ```

- The comma-list and the trailing-symbol shorthand collide: `buy $100 BTC`
  is fine (single symbol); `buy $100 BTC,ETH` only works in the **leading**
  position (`buy BTC,ETH $100`). Stick to leading-symbol for fan-out.
- `%all%` does **not** work outside `chase`. There's no `close %all%` or
  `cancel %all%` — use `close all` / `cancel` (no arg) instead.
- Fan-out is fire-and-don't-await — failure on one leg doesn't stop the
  rest. Pair with `set ec` if you want explicit warnings on each, or with
  `retry N` per-symbol if you alias.
- Each leg of a chaser fan-out consumes its own task slot. `chasers` lists
  them all; budget cancel-replace quota accordingly (see
  [Recipe: Chaser](./chaser.md)).
- `loop allpos: <body>` and `loop all: <body>` are the same — `all` is a
  synonym for `allpos`. Don't confuse with `close all` (a `close` variant,
  not a `loop` variant).

## Related

- [Workflow → `loop`](../reference/workflow.md#loop) — variants + body rules
- [`chase`](../reference/commands.md#chase) — `%all%` semantics
- [`close`](../reference/commands.md#close) — global close variants
- [Safety → whitelist](../reference/safety.md#whitelist) — opt-out from global closes
- [Aliases & vars → aliases](../reference/aliases-and-vars.md#aliases) — multi-statement loop bodies
