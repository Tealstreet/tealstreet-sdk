---
title: Commands
sidebar_position: 1
---

# Commands

Every command available at the REPL, alphabetical. For the size/price grammar
that fills the `<size>` / `<price>` slots below, see
[Sizing & prices](./sizing-and-prices.md). For chain operators (`;` / `&`),
loops, and conditionals, see [Workflow](./workflow.md).

> **Web vs standalone** — most commands run identically in the in-app web CLI
> and the standalone binary. Exceptions are flagged with **standalone-only**.
> See [Web CLI subset](./web-cli.md) for the full split.

---

## Trading

### `buy` / `sell` &nbsp; _(aliases `b`, `s`)_ {#buy-sell}

Place a market, limit, or `best`-priced order on the focused symbol, with
optional reduce-only / post-only / bracket stop.

| Form                         | Result                                    |
| ---------------------------- | ----------------------------------------- |
| `buy $100`                   | Market buy $100 notional                  |
| `buy 100$`                   | Same — suffix form                        |
| `buy 50%bal`                 | 50% of free balance                       |
| `buy SYMBOL $100`            | Symbol override (leading or trailing)     |
| `buy WIF,ENA $100`           | Multi-symbol fan-out (comma list)         |
| `buy $100 at 50000`          | Limit at absolute price                   |
| `buy $100 at +100` / `at 1%` | Limit at relative / percent offset        |
| `buy $100 at 50000 reduce`   | Reduce-only limit                         |
| `buy $100 at 50000 po`       | Post-only limit                           |
| `buy $100 best`              | Limit at best bid (buy) / best ask (sell) |
| `buy $100 stop 49500`        | Bracketed entry with attached stop loss   |

**Safety**: Bare numbers ≥ 100 with no `at` are rejected — the parser asks
"did you mean `at <price>`?" so a typo can't coerce to market silently.
[fatfinger](./safety.md#fatfinger) checks fire per-order.

Source: `packages/cli-core/src/commands/trading/BuyCommand.ts`, `SellCommand.ts`.

---

### `stop`

Reduce-only stop loss or take-profit. Decided by trigger price vs mark.

| Form                             | Result                                       |
| -------------------------------- | -------------------------------------------- |
| `stop at 18500`                  | Market stop for full focused position        |
| `stop at -1%`                    | Same, relative to entry                      |
| `stop buy at +100`               | Explicit side (when hedge-mode is ambiguous) |
| `stop $500 at 1%`                | Partial-size stop                            |
| `stop sell 0.01 at -100 ETHUSDT` | Symbol override                              |
| `stop at -1% trigger 18500`      | **Limit** stop — trigger ≠ limit price       |

Source: `StopCommand.ts`.

---

### `close` &nbsp; _(alias `c`)_ {#close}

| Form                       | Result                               |
| -------------------------- | ------------------------------------ |
| `close`                    | Full close, focused symbol           |
| `close SYM,SYM2`           | Full close, named symbols            |
| `close long` / `short`     | Hedge-side disambiguator             |
| `close longs` / `shorts`   | All long / short positions globally  |
| `close all` / `allpos`     | Every open position                  |
| `close 50%` / `close $100` | Partial close of focused             |
| `close longs 50%`          | Partial close of every long globally |

Global variants (`longs`, `shorts`, `all`, `allpos`) honor the
[whitelist](./safety.md#whitelist). Source: `CloseCommand.ts`.

---

### `cancel` &nbsp; _(alias `x`)_

Filters compose: side × kind × reduce-only × position-selector × range × id.

| Form                          | Result                                          |
| ----------------------------- | ----------------------------------------------- |
| `cancel`                      | All orders, focused symbol                      |
| `cancel ETHUSDT`              | All orders, named symbol                        |
| `cancel buys` / `sells`       | Side filter                                     |
| `cancel limits` / `stops`     | Kind filter                                     |
| `cancel ro` / `xro`           | Reduce-only / excluding reduce-only             |
| `cancel buys ro ETHUSDT`      | Compose filters                                 |
| `cancel top 3` / `bottom 5`   | Sorted by price (top = highest)                 |
| `cancel first 2` / `last 1`   | Sorted by last update                           |
| `cancel from 49000 to 50000`  | Price-range, limits only                        |
| `cancel <id>` / `<id-prefix>` | Specific order (prefix ≥ 3 chars)               |
| `cancel <id1>,<id2>,…`        | Comma list — what `cancel @capturedVar` becomes |

Source: `CancelCommand.ts`.

---

### `bump`

Shift open **limit** orders by a price delta. Stops are untouched.

| Form                       | Result                            |
| -------------------------- | --------------------------------- |
| `bump +10`                 | Shift every limit by +10, focused |
| `bump ETHUSDT +10`         | Symbol override                   |
| `bump buy +10` / `sell -5` | Side filter                       |
| `bump top 3 +5`            | Top-N price selector              |
| `bump 100 --buys`          | Legacy flag form — still accepted |

Source: `BumpCommand.ts`.

---

### `move` &nbsp; _(alias `mv`)_

Move a specific limit order by ID (full or unique prefix).

| Form                        | Result          |
| --------------------------- | --------------- |
| `move <orderId> 50000`      | Absolute        |
| `move <orderId> +10` / `-5` | Relative offset |

Source: `MoveCommand.ts`.

---

### `scale`

Ladder **N limit orders** between two prices, split evenly.

```text
scale <buy|sell> <size> into <N> from <p1> to <p2> [reduce] [taker] [SYMBOL]
```

| Modifier           | Effect                                                   |
| ------------------ | -------------------------------------------------------- |
| `reduce`           | Reduce-only                                              |
| `taker`            | Drop the default post-only TIF                           |
| `cubic` / `icubic` | _Accepted but distribution is still even (no-op today)._ |

Examples:

```text
scale buy 10 into 10 from 31000 to 32000
scale sell $1000 into 25 from 50000 to 48500 reduce
scale buy $1000 into 10 from -0.25% to -1% ETHUSDT
```

Source: `ScaleCommand.ts`.

---

### `swarm`

Fire **N concurrent market orders** summing to `<size>`.

```text
swarm <buy|sell> <size> into <N> [reduce] [irregular] [SYMBOL]
```

| Modifier    | Effect                                    |
| ----------- | ----------------------------------------- |
| `irregular` | Randomise slice weights in `[0.5x..1.5x]` |
| `reduce`    | Reduce-only                               |

Errors before any order is sent if a slice would round to zero.
Source: `SwarmCommand.ts`.

---

### `chase`

Passive limit at top of book that re-pegs to best bid / ask. Runs as a
background task — list with [`chasers`](#chasers), kill with `kill <id>`.

| Form                                 | Result                                        |
| ------------------------------------ | --------------------------------------------- |
| `chase buy $100`                     | Chase on focused symbol                       |
| `chase sell 50% reduce`              | Reduce-only chase                             |
| `chase buy WIF,ENA $100`             | Multi-symbol fan-out                          |
| `chase sell %all% 20% reduce`        | All positions on the implied side             |
| `chase buy $100 to 1%`               | Terminate-at distance (price-walk guard)      |
| `chase buy $100 to 49500` / `to $50` | Same, abs / dollar form                       |
| `chase cancel`                       | Stop newest active chaser + cancel rest order |
| `chase cancel keep`                  | Stop chaser but leave the order in book       |

Source: `ChaseCommand.ts`.

---

### `chasers`

| Form                  | Result                       |
| --------------------- | ---------------------------- |
| `chasers`             | List active + recent chasers |
| `chasers cancel <id>` | Cancel by id-prefix          |
| `chasers cancel all`  | Cancel all chasers           |

Source: `ChasersCommand.ts`.

---

### `twap`

Spread an order over time. Three forms accepted.

```text
twap buy <size> [SYMBOL] into <N> over <minutes> [reduce] [chaser]
twap chase buy <size> into <N> over <M> [reduce] [timeout <pct> <action>] [fail <action>]
twap buy <size> duration=1h slices=10        # legacy key=value form
```

`twap chase` runs a chaser per slice. Defaults: `timeout 50% market`, `fail market`.
Action ∈ `{market, next}`. `<pct>` ∈ `(0, 100]`.

| Management              | Result            |
| ----------------------- | ----------------- |
| `twap list`             | List all TWAPs    |
| `twap stop <id-prefix>` | Stop by id-prefix |
| `twap stop all`         | Stop all TWAPs    |

> **Gotcha**: `reduce` is parsed and gates fatfinger checks, but is not yet
> threaded through to slice placement. Treat `twap … reduce` as a
> fatfinger-bypass marker, not a hard guarantee per slice.

Source: `TwapCommand.ts`.

---

### `nuke`

Cancel + close everything. Up to 3 retries internally.

| Form                | Result                            |
| ------------------- | --------------------------------- |
| `nuke` / `nuke all` | Cancel all orders + close all pos |
| `nuke positions`    | Close all positions only          |
| `nuke orders`       | Cancel all orders only            |

Source: `NukeCommand.ts`.

---

### `leverage` &nbsp; _(alias `lev`)_

| Form                  | Result                            |
| --------------------- | --------------------------------- |
| `leverage`            | Read (only if a position is open) |
| `lev 5`               | Set 5x on focused symbol          |
| `leverage 10 ETHUSDT` | Set 10x on named symbol           |

Strings like `-5`, `5x`, `2..5` are rejected loudly. Source: `LeverageCommand.ts`.

---

### `max`

Largest possible order size given free balance × current leverage.
Falls back to 1x with a note if no position is open. Source: `MaxCommand.ts`.

---

### `orders` &nbsp; _(alias `o`)_

| Form                  | Result                                 |
| --------------------- | -------------------------------------- |
| `orders` / `o`        | Open + creating orders, focused symbol |
| `orders --all` / `-a` | Multi-account view across all symbols  |

Source: `OrdersCommand.ts`.

---

### `positions` &nbsp; _(alias `ps`)_

| Form                       | Result                          |
| -------------------------- | ------------------------------- |
| `positions` / `ps`         | Focused symbol                  |
| `positions long` / `short` | Side filter                     |
| `positions --all` / `-a`   | Multi-account view, all symbols |

Source: `PositionsCommand.ts`.

---

### `position` &nbsp; _(aliases `p`, `pos`)_

Card-style display for a single position (size, entry → mark, uPnL, liq,
leverage). `pos btc` accepts fuzzy symbol match. `show` modifier is a no-op.
Source: `PositionCommand.ts`.

---

### `margin` &nbsp; _(aliases `balance`, `bal`)_

| Form             | Result                   |
| ---------------- | ------------------------ |
| `margin` / `bal` | Account margin / balance |
| `bal --all`      | Multi-account view       |

Source: `MarginCommand.ts`.

---

### `spot`

Spot balances when the exchange exposes them. `spot --all` for multi-account.
Source: `SpotCommand.ts`.

---

### `last-price` / `mark-price` / `index-price`

Read live price from the focused or named ticker(s).

| Aliases (in order)       | Reads      |
| ------------------------ | ---------- |
| `last-price`, `lp`, `l`  | last trade |
| `mark-price`, `mp`, `m`  | mark       |
| `index-price`, `ip`, `i` | index      |

All accept a trailing comma-separated symbol list. Source: `PriceCommand.ts`.

---

### `echo` &nbsp; _(alias `say`)_

Print args after [variable substitution](./variables.md). `echo @hi` **does**
ratchet a tracker on read; `print @hi` does not. Source: `EchoCommand.ts`.

---

### `chart` &nbsp; _(standalone-only)_

ASCII candlestick chart with position + open-order overlays.

| Form               | Result                                |
| ------------------ | ------------------------------------- |
| `chart`            | 1h, 30 candles, focused               |
| `chart 15m`        | Timeframe override                    |
| `chart 4h 50`      | Timeframe + bar count (capped at 100) |
| `chart 1d ETHUSDT` | Symbol override                       |

Source: `ChartCommand.ts`.

---

## Session

### `symbol` &nbsp; _(aliases `sym`, `switch`)_

Change the focused symbol: `switch BTCUSDT`. Bare instrument typed at the
prompt (`BTCUSDT<enter>`) is shorthand for this. Source: `SymbolCommand.ts`.

### `markets` &nbsp; _(aliases `mkt`, `m`)_

List available markets in a 4-column grid. Source: `MarketsCommand.ts`.

### `status` &nbsp; _(alias `st`)_ {#status}

Account / exchange / symbol of the current session. Source: `StatusCommand.ts`.

### `connection` &nbsp; _(aliases `conn`, `ws`)_ {#connection}

| Form                              | Result                      |
| --------------------------------- | --------------------------- |
| `conn`                            | WS health, current account  |
| `conn --all`                      | Multi-account               |
| `conn reconnect` / `ws reconnect` | Force a fresh WS connection |

Source: `ConnectionCommand.ts`.

### `help` &nbsp; _(aliases `?`, `h`)_

`help` for general index, `help <cmd>` for a specific command.

### `prompt`

Configure the REPL prompt format. See [Output → Prompt](./output.md#prompt).
Source: `PromptCommand.ts`.

### `clear` &nbsp; _(alias `cls`)_

Clear screen.

### `login`, `logout`, `auth` &nbsp; _(standalone-only)_

OAuth flow. `whoami` shows the current logged-in user. Listens on port 9876
for the OAuth callback.

### `logs` / `log` / `clearlogs` &nbsp; _(standalone-only)_

| Form                       | Result                       |
| -------------------------- | ---------------------------- |
| `logs`                     | Print the log file path      |
| `logs clear` / `clearlogs` | Wipe `~/.tealstreet/cli.log` |

### `audit` &nbsp; _(standalone-only)_ {#audit}

Durable, append-only log of **write-ops** (orders / cancels / closes /
leverage). **Off by default.** When on, each write-op appends a JSON line
to `~/.tealstreet/audit.log` with timestamp, account, the command, and
the result (ok + order IDs, or the error) — including failed attempts.

| Form             | Result                                |
| ---------------- | ------------------------------------- |
| `audit`          | Show status + log path                |
| `audit on`       | Enable the write-op audit log         |
| `audit off`      | Disable                               |
| `audit tail [N]` | Print the last N entries (default 20) |

Reads (positions / orders / watch / etc.) are never logged — only
exposure-affecting writes. Script-driven writes are captured too:
`script:<name>` internal write ops (orders, cancels, updates, closes,
leverage) are logged (prefixed `[script]`) along with the named
invocation. Every write from one typed line shares a **`runId`** (shown
as `run=<id>` by `audit tail`), so all the orders from a `repeat 10` or
a single script run group together. **Fills are attributed back** to the
order that produced them: when a tracked order fills, a `[fill]` entry is
logged with the same `runId` and a `← <originating command>` pointer
(unattributable fills — placed before auditing was on, or in a prior
process — are skipped). The log is append-only but **size-capped**: past
~5 MB the live `audit.log` rolls to `audit.log.1` (one backup) so disk
use stays bounded; `audit tail` reads from the end and dips into the
backup when needed. Source: `audit-log.ts`, `script-adapter.ts`.

### `footer` &nbsp; _(standalone-only)_ {#footer}

Controls the persistent **bottom status footer** — a pinned line at the
bottom of the terminal, above which your prompt and output live, showing
always-on live data (connection, account, symbol, balance, PnL, clock,
…). It updates in place ~once a second and is a glanceable companion to
the transient top alert bar. **On by default** — `footer off` opts out
(persisted). The setting + layout live in `~/.tealstreet/config.json`
(`footer` block — see [config files](../operations/config-files.md)).

| Form                               | Result                                                  |
| ---------------------------------- | ------------------------------------------------------- |
| `footer`                           | Show on/off state + the current segment layout          |
| `footer on` / `footer off`         | Enable / disable the footer (persisted)                 |
| `footer list`                      | List the configured segments with their indices         |
| `footer reset`                     | Reset the layout to the default set                     |
| `footer add <type> [right]`        | Append a segment, optionally to the right-aligned group |
| `footer add script <name> [right]` | Append a custom-script segment (see below)              |
| `footer remove <index>`            | Remove the segment at the given index                   |

Built-in segment types: `connection`, `account`, `symbol`, `balance`,
`pnl` (focused-symbol uPnL $ + %), `totalpnl`, `position`, `mark`,
`last`, `funding`, `clock`, `version` (the CLI version — in the default
layout; `footer remove` it if you don't want it), `text`. All read
already-streamed state — no extra API calls. Each segment can set `group` (`left` / `right`) and a
`color` override in the config file.

**`script` segments** show the return value of one of your
[custom scripts](./scripts.md): `footer add script <name>` runs the named
script ~once a second and renders the string it returns. These runs are
**hard read-only** — capabilities are forced so the script cannot place or
cancel orders or call `runCommand` (any such call fails); only reads work.
They're also silent (the script's `print()` output is suppressed — only
the return value is used) and time-boxed (a slow or throwing script shows
a small error token like `!myfooter` and never blocks the footer).

Example footer script (`~/.tealstreet/scripts/risk.ts`):

```ts
export const meta = { name: 'risk', targets: ['core', 'cli'], capabilities: { accounts: [], writes: [] } };
export default function (_args, api) {
  const p = api.getPositions()[0];
  return p ? `risk ${p.notional.toFixed(0)}` : 'flat';
}
```

Source: `footer.ts`, `footer-segments.ts`, `scroll-region.ts`,
`repl.ts` (`runFooterScript`).

### `update` &nbsp; _(standalone-only)_

`update` runs the binary updater. `update --check` / `-c` does a version check only.

### `exit` / `quit` &nbsp; _(standalone-only)_

Stops monitors, listener, exchanges, then `process.exit(0)`.

---

## Accounts

### `account` &nbsp; _(aliases `acc`, `use`)_

| Form                             | Result                             |
| -------------------------------- | ---------------------------------- |
| `account mybybit`                | Switch focused account by name     |
| `account 1` / `account -# 1`     | By 1-indexed number                |
| `account enable acc1 acc2`       | Start streaming for named accounts |
| `account enable --all`           | Start streaming for every account  |
| `account disable acc1` / `--all` | Stop streaming                     |

Source: `AccountCommand.ts`.

### `accounts` &nbsp; _(alias `config`)_

| Form                                                    | Result                                                                                     |
| ------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `accounts` / `accounts list`                            | List all configured accounts                                                               |
| `accounts add -n <name> -e <ex> -k <key> -s <secret>`   | Add — use `--tealstreet-proxy [--proxy-region <region>]` to match web "Use Tealstreet IPs" |
| `accounts import <json>`                                | Import from web export JSON                                                                |
| `accounts remove <name>` / `--all`                      | Remove (tombstoned for 30 days)                                                            |
| `accounts enable <name…>` / `disable <name…>` / `--all` | Bulk streaming control                                                                     |
| `accounts enabled` / `streaming` / `active`             | List only streaming accounts                                                               |
| `accounts help`                                         | Full reference                                                                             |

Source: `AccountsCommand.ts`.

### `exchanges` &nbsp; _(alias `ex`)_

List supported exchanges with required credential fields.
Source: `ExchangesCommand.ts`.

---

## Tasks (in-process)

### `tasks`

Unified list of in-process chasers + TWAPs, insertion order.

### `kill` &nbsp; _(aliases `fuck`, `terminate`)_ {#kill}

| Form               | Result                  |
| ------------------ | ----------------------- |
| `kill`             | Most-recent active task |
| `kill last`        | Same                    |
| `kill <id-prefix>` | By id-prefix            |
| `kill all`         | Every running task      |

Source: `KillCommand.ts`. `fuck` / `terminate` are ichibot-parity aliases.

---

## History

### `history`

| Form          | Result                 |
| ------------- | ---------------------- |
| `history`     | Last 50 commands       |
| `history 100` | Last N — capped at 200 |

Source: `HistoryCommand.ts`.

Persisted to `~/.tealstreet/cli_history` (standalone) or a jotai atom (web).

---

## Variables, aliases, workflow

Defined and detailed in their own pages — listed here for completeness:

- [`set` / `sets` / `track` / `untrack` / `unset` / `print` / `vars`](./aliases-and-vars.md)
- [`alias` / `unalias`](./aliases-and-vars.md#aliases)
- [`sleep`, `wait`, `loop`, `repeat`, `if`/`else`/`end`, `retry`, `set ec` / `set ro` / `set po`](./workflow.md)

---

## Safety

Defined in [Safety](./safety.md). Listed here:

- `simulation` / `sim` / `dry-run`
- `fatfinger` / `unfatfinger`
- `budget` — cumulative notional cap (`budget $5000` / `budget off` / `budget reset`)
- `whitelist` / `unwhitelist`

---

## Watch modes &nbsp; _(standalone-only)_

| Alias | Long forms                                     | Shows                             |
| ----- | ---------------------------------------------- | --------------------------------- |
| `wa`  | `watch account`, `watch acc`                   | Margin + positions + orders combo |
| `wp`  | `watch positions`, `watch pos`                 | Live positions table              |
| `wo`  | `watch orders`, `watch ord`                    | Live orders table                 |
| `wm`  | `watch margin`, `watch balance`, `watch bal`   | Live margin                       |
| —     | `watch chart` &nbsp; _(shorthand `watch c …`)_ | Live ASCII chart                  |
| —     | `watch chase`                                  | Live chaser status panel          |

All of `wa`/`wp`/`wo`/`wm` accept `--all` / `-a` for multi-account view.
Exit: `Escape` or `Ctrl+X`. Refresh: 1000 ms.

See [Output → Watch modes](./output.md#watch-modes) for terminal-recovery
quirks.

---

## Listener subsystem &nbsp; _(standalone-only)_

Detailed in [Operations → Listener](../operations/listener.md) and
[Hooks & webhooks](../operations/hooks-and-webhooks.md). Listed here:

- `hook` / `hooks` / `hook create` / `hook revoke` / `hook rotate` / `hook show`
- `clients` / `clients list` / `clients revoke`

---

## Cross-trade &nbsp; _(standalone-only)_ {#cross-trade}

Detailed in [Operations → Cross-trade](../operations/cross-trade.md). Top-level
binary surface:

| Form                                                                                   | Result                                                                                          |
| -------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| `tealstreet cross-trade list`                                                          | List configs in `~/.tealstreet/cross-trade.json`.                                               |
| `tealstreet cross-trade add --master <a> --copier <b> [--symbol] [--mode] [--enabled]` | Add a single-copier config. `--mode ∈ LimitSlippage \| Market \| Chase` (Chase skipped on CLI). |
| `tealstreet cross-trade enable <id>` / `disable <id>` / `remove <id>`                  | Mutate by id prefix (8 chars matches one row).                                                  |
| `tealstreet cross-trade run`                                                           | Foreground daemon. Boots accounts, runs the engine, blocks until SIGINT.                        |
| `tealstreet cross-trade status`                                                        | Read the heartbeat and print running/not + uptime + active configs.                             |

---

## Recording &nbsp; _(standalone-only)_ {#recording}

Detailed in [Operations → Recording](../operations/recording.md). REPL surface:

| Form                      | Result                     |
| ------------------------- | -------------------------- |
| `record <exchange>`       | Start orderbook recording  |
| `record start <exchange>` | Same — explicit            |
| `record stop <exchange>`  | Stop recording             |
| `record list`             | List configured recordings |
| `record stats [exchange]` | DB stats                   |

Alias: `rec`.

---

## Import / export

| Form     | Result                                                  |
| -------- | ------------------------------------------------------- |
| `export` | Write vars + aliases to `~/.tealstreet/cli-export.json` |
| `import` | Read same — replaces current vars + aliases             |

> Distinct from `tealstreet account export/import`, which dumps credentials.

---

## Scripts &nbsp; _(standalone-only)_ {#scripts}

Scripts live in `~/.tealstreet/scripts/` — one `.ts` file per script.
Open the directory in your editor for full IntelliSense via the
seeded `tsconfig.json` + `types.d.ts`. Full layout in
[Operations → Config files → Scripts](../operations/config-files.md#scripts).

| Form                                         | Result                                                                                                                                                                                                                                                 |
| -------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `script:<name>` &nbsp; (e.g. `script:hello`) | Run the loaded script. The `script:` prefix is required — bare `<name>` is never resolved to a script. Args after the name are parsed and handed to the script as `args`. Composes with `retry N`, `@<x> = `, `set ec`, chain operators, and webhooks. |
| `script reload` &nbsp; (or `scripts reload`) | Re-scan `~/.tealstreet/scripts/` and rebuild the in-memory registry. Edits to script files don't auto-reload — run this after saving.                                                                                                                  |

**Grammar:** `script:<name>` with no space after the colon. `script: foo` is a parse error. `script:` with no name is a parse error. Invoking a name the loader didn't find errors loudly — it does NOT fall through to the regular command dispatcher.

**Why a prefix?** Eliminates every name-collision case. Filenames like `~/.tealstreet/scripts/buy.ts` are legal; the builtin `buy` keeps working. Future CLI upgrades adding new builtins never silently shadow existing user scripts.

---

## The `tealstreet` binary

The CLI is also a standard command-line tool. With no args, it starts the
REPL. With a subcommand, it runs one-shot.

### Top-level flags

| Flag               | Effect                                 |
| ------------------ | -------------------------------------- |
| `-v` / `--version` | Print version and exit                 |
| `--update`         | Trigger updater                        |
| `--check-update`   | Version-check only                     |
| `--disable-all`    | Start REPL with all exchanges disabled |

### Subcommands

| Subcommand                                                                            | Aliases    | Purpose                                                                              |
| ------------------------------------------------------------------------------------- | ---------- | ------------------------------------------------------------------------------------ |
| `interactive` _(default)_                                                             | `i`        | Launch REPL. `-a <name>`, `--simple`, `--disable-all` accepted                       |
| `exec <command…>`                                                                     | —          | One-shot. `-a <account>`, `-s <symbol>` for context scoping                          |
| `buy` / `sell` / `close` / `cancel` / `scale` / `chase` / `nuke` / `balance` / `stop` | —          | Shorthand wrappers around `exec <verb> …`. All accept `-a` / `-s`                    |
| `account` (group)                                                                     | `accounts` | `list` / `add` / `remove` / `default` / `symbol` / `exchanges` / `import` / `export` |
| `tasks list` / `tasks clear`                                                          | —          | Persisted tasks from `~/.tealstreet/tasks.json` _(see warning below)_                |
| `record` (group)                                                                      | —          | `start` / `stop` / `list` / `stats`                                                  |
| `update`                                                                              | —          | `-c` / `--check` for check-only                                                      |
| `logs`                                                                                | `log`      | `-c` / `--clear`, `-n <N>` lines                                                     |
| `mcp`                                                                                 | —          | Run as an MCP server over stdio — see [MCP server](#mcp-server) below                |

> **`tealstreet tasks` is different from REPL `tasks`.** Top-level reads
> persisted task records from `~/.tealstreet/tasks.json`. REPL `tasks` lists
> live chasers + TWAPs from memory. Don't conflate them.

One-shot `exec` commands that start TWAPs keep the process alive until those
TWAPs finish, then disconnect the exchange session.

Source: `apps/cli/src/index.ts`.

---

## MCP server &nbsp; _(standalone-only)_ {#mcp-server}

`tealstreet mcp` runs the CLI as an [MCP](https://modelcontextprotocol.io)
server over stdio, so an AI assistant (Claude Code, Claude Desktop, …) can
read, write, and validate your scripts, inspect local account state, and read
Tealstreet data tools.

Register it with your MCP client — for Claude Code:

```bash
claude mcp add tealstreet-scripts -- tealstreet mcp
```

**You must be logged in.** Start the CLI (`tealstreet`) and run `login` at the
prompt — `login` is a REPL command, there is no top-level `tealstreet login`.
The MCP server exits immediately with `not logged in` on stderr rather than
starting and letting each tool fail separately, since an assistant would
otherwise keep retrying instead of telling you to log in. If your MCP client
reports the server as failed, check your login first — tokens expire.

### Tools

| Tool                                                                                                        | Purpose                                                                                |
| ----------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| `list_scripts`                                                                                              | Every script on disk, including ones that currently fail to load                       |
| `read_script`                                                                                               | One script's source — works even when the script is broken                             |
| `get_api_types`                                                                                             | The exact TypeScript surface available to scripts (`core` / `cli` / `web` / `all`)     |
| `validate_script`                                                                                           | Typecheck + capability check without writing. Fast, local, free                        |
| `write_script`                                                                                              | Create or replace a script. **Validates first and refuses to write invalid source**    |
| `delete_script`                                                                                             | Delete a script                                                                        |
| `export_scripts`                                                                                            | Export your own scripts as a portable JSON bundle (backup / moving machines)           |
| `list_accounts`                                                                                             | Configured accounts — names and exchanges only, never credentials                      |
| `get_positions` / `get_orders` / `get_balance` / `get_ticker`                                               | Live reads for an account                                                              |
| `run_script`                                                                                                | **Dry run only** — see below                                                           |
| `list_market_data_sources`                                                                                  | Public Tealstreet data sources and publicdata exchange health                          |
| `get_public_tickers` / `get_public_ticker`                                                                  | Public ticker snapshots from Tealstreet publicdata                                     |
| `get_top_movers` / `get_top_volume` / `get_top_funding`                                                     | Public market top lists from Tealstreet publicdata                                     |
| `get_exchange_proxy_candles`                                                                                | Historical candles through exchange.tealstreet.io's public candle service              |
| `get_recent_news`                                                                                           | Public normalized TealNews items through news.tealstreet.io's recent-news endpoint     |
| `get_safe_cex_markets` / `get_safe_cex_market`                                                              | Safe-cex market metadata through exchange.tealstreet.io's public candle service        |
| `get_polymarket_events` / `search_polymarket_events`                                                        | Public Polymarket prediction-market events and search                                  |
| `get_polymarket_midpoints` / `get_polymarket_order_book` / `get_polymarket_price_history`                   | Public Polymarket CLOB data, with bounded book depth                                   |
| `get_hyperliquid_leaderboard`                                                                               | Crawled Hyperliquid leaderboard rows, when analytics DB env vars are configured        |
| `get_hyperliquid_wallet_positions` / `get_hyperliquid_wallet_fills` / `get_hyperliquid_account_value_curve` | Crawled public Hyperliquid wallet analytics, when analytics DB env vars are configured |

### What it will refuse

`write_script` runs the same checks as `validate_script` and will not write a
file that fails any of them:

- **Value imports.** `import { OrderSide } from '@tealstreet/script-api-core'`
  typechecks but fails at load, because scripts run from a directory with no
  `node_modules`. Use `import type` for types and `api.constants.*` for values.
- **Undeclared capabilities.** Every `api.exchange.*` method the body calls
  must appear in `meta.capabilities.writes`. `closePosition` additionally
  requires `placeOrder`.
- **Top-level calls.** Code outside the default-exported function runs when the
  CLI loads the scripts directory, not when the script is invoked. Declarations
  (types, helper functions, `const` config) are fine.

This is stricter than editing a file by hand on purpose: a script an assistant
wrote is code you have not read.

### `run_script` is dry-run only

`run_script` executes a script against **live market data** but every write —
`placeOrder`, `cancelOrder`, `runCommand`, everything — is _recorded and
returned as an intent_ rather than executed. Nothing this server can do places a
real order.

You get back the full list of what the script would have done, which is the
useful part; a mechanism that simply refused at the first write would tell you
about one action and nothing else.

To actually run a script, run `script:<name>` yourself in the REPL.

Two caveats worth knowing:

- Dry run bounds a script's `api.*` calls. It is **not a sandbox** — scripts
  execute with full runtime globals, so a script that reaches outside the API
  entirely is unaffected. `run_script` therefore refuses to run source that
  doesn't validate.
- The server connects accounts on demand and **never writes `active` back to
  your config**, so it cannot disturb a REPL running alongside it.

### There is no import tool, on purpose

`export_scripts` exists; a matching import does not. Exporting your own scripts
is a backup. **Installing scripts written by someone else is a different
proposition entirely**: scripts currently run on the same boundary as code you
wrote yourself, reaching `fetch`, `localStorage` and your session. Third-party
scripts wait for the worker sandbox.

If you want to move a bundle to another machine, paste each script through
`write_script` — it validates first, and you see what it declares before it
lands.

### Notes

- **Local only.** stdio transport, no network listener. It does not expose your
  accounts to anything but the MCP client you launched it from.
- **Data reads may call Tealstreet services.** Public market tools make outbound
  HTTPS calls to Tealstreet publicdata. Hyperliquid analytics tools read a
  Postgres analytics database only when you set `HYPERLIQUID_ANALYTICS_DATABASE_URL`
  or `HYPERLIQUID_ANALYTICS_POSTGRES_*`; otherwise they return a configuration
  error.
- **Credentials are never exposed.** `list_accounts` returns an explicit
  allowlist (name, exchange, testnet, connected) — never the stored keys.
- Script writes, deletions, **and dry runs** are recorded in the audit log
  tagged `[mcp]`, so agent activity is distinguishable from your own after the
  fact. A dry-run entry carries the number of write intents the script
  attempted — that count is what makes a run worth reviewing later.
- Scripts are not executed by this server — write them here, run them from the
  REPL with `script:<name>`.

Source: `apps/cli/src/mcp/`.

---

## Multi-symbol fan-out

Most trading commands accept a leading **comma-separated symbol list**:

```text
buy WIFUSDT,ENAUSDT $100
chase sell %all% 20% reduce
last-price BTC,ETH,SOL
```

`%all%` (in `chase … reduce` only) expands to every position on the implied
side. See [Recipes → Fan out across symbols](../recipes/fan-out-symbols.md).

---

## Notes on accepted-but-no-op surface

A few keywords are accepted by the parser but don't yet have runtime effect.
Treat them as "doesn't break the command":

- `scale … cubic` / `… icubic` — accepted, distribution still even.
- `twap … reduce` — accepted; gates fatfinger but not slice placement.
- `import` payload's `binds` field — read but discarded (reserved for hotkeys).
