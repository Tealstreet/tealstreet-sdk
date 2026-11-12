---
title: Output, prompt, watch modes
sidebar_position: 7
---

# Output

How the CLI shapes what you see — output kinds, prompt config, watch modes,
fill notifications, connection dots.

---

## Windows console glyphs {#windows-console-glyphs}

On Windows outside Windows Terminal, or when the console code page is not
UTF-8, the CLI prints ASCII instead of glyphs the legacy console font cannot
draw. Every other terminal keeps Unicode.

| Unicode     | ASCII              |
| ----------- | ------------------ |
| `✗ ⚠ ✓ ℹ`   | `[x] [!] [ok] [i]` |
| `⛔`        | `[!!]`             |
| `─ ═ │ ┌ ┼` | `- = \| + +`       |
| `— … → ≥`   | `- . > >=`         |

`TEALSTREET_ASCII=1` forces ASCII on any platform; `TEALSTREET_ASCII=0` keeps
Unicode. The code page is read once at startup. ASCII only applies to what a
person reads in the terminal: `--json` documents, redirected or piped output,
and output sent to web clients over the listener always keep the original
Unicode text.

## Output kinds

Every line the CLI prints carries a `PrintKind`. Listener `cmd:output` frames
expose the same kind to web clients.

| Kind      | Color   | Listener field | Goes to log file? |
| --------- | ------- | -------------- | ----------------- |
| `print`   | default | `print`        | terminal only     |
| `success` | green   | `success`      | terminal only     |
| `info`    | gray    | `info`         | terminal only     |
| `warn`    | yellow  | `warn`         | terminal only     |
| `error`   | red     | `error`        | terminal only     |

Exchange output belongs to the login that connected the exchange. After switching
users, delayed output and disposal notices from the previous login are discarded.
Disabling an account within the same login still reports interrupted chases.

Where each stream goes:

- `logger.info / warn / error / debug` → `~/.tealstreet/cli.log` only — never
  the terminal.
- `console.log / info` → the log file always, and the terminal **only if it
  isn't operational noise**. Routine exchange-WebSocket chatter (keepalive
  pings, reconnect notices, balance-stream dumps) is classified as
  _operational_ and kept off the terminal — it still lands in `cli.log`.
  Everything else prints **above** the prompt (readline-aware, so async output
  never shreds what you're mid-typing).
- `console.warn / error` → log file + printed above the prompt, and the latest
  one is mirrored on the [top status bar](#status-bar--footer). The terminal
  line always carries the cause: an error's message, a venue envelope's
  `retMsg`/`ret_msg`/`msg` (also inside a proxied `raw_response`), else its
  code or kind, else the body serialized to 300 characters. The full body is
  always in `cli.log`.
- `console.debug` → log file **only when `DEBUG=1` / `DEBUG=true`** is set;
  dropped otherwise (it routes to `logger.debug`, gated by `MIN_LOG_LEVEL`,
  which defaults to `info`).
- `print()` / `originalConsoleLog()` → terminal only, no rate limit — what
  command handlers use for primary output.

Bybit timestamp refusals use the Rust engine's per-account clock correction and
one bounded re-signed retry. A detected system-clock skew prints one warning per
CLI session, including the measured fast/slow offset and Windows time-sync steps.

Verbose HoW/tradetunnel request diagnostics also use the existing console
pipeline: start the CLI with `DEBUG=1` or `DEBUG=true` to add sanitized
`[TradeTunnel]` request/response summaries to the same
`~/.tealstreet/cli.log`. The `[TradeTunnel]` prefix is classified as
operational, so these lines stay off the terminal. This is off by default, does
not create a second network log file, omits headers, redacts sensitive keys,
preserves URL query-key names without query values, and truncates large
payloads.

Routine ReconnectingWebSocket and HOW reconnect diagnostics stay in `cli.log`,
without a red error or a prompt line. URLs contain only their origin and path;
URL-provider functions, query strings, credentials and outgoing frames are not
logged. Repeated connection failures and exhausted retries still show an error,
as do authentication failures and uncertain order outcomes.

The Node/Bun relay sends native WebSocket pings every 15 seconds and reconnects
after 45 seconds without incoming traffic or a pong. These control frames never
enter the exchange's application stream. Bybit's engine separately sends its
JSON ping every 10 seconds on public and private sockets; its 20-second receive
silence deadline still recovers a stalled upstream. HOW keeps its existing
5-second RPC ping and 25-second silence recovery.

The `operational` / `info` / `warn` / `error` / `critical` taxonomy lives in
`apps/cli/src/log-classify.ts`; routing in `apps/cli/src/logger.ts`. View the
full log with the `logs` / `log` command, or tail `~/.tealstreet/cli.log`
(`yarn logs` in dev). The file is **cleared on every startup**.

Source: `apps/cli/src/listener/adapter-context.ts:13`,
`apps/cli/src/logger.ts`, `apps/cli/src/log-classify.ts`.

### Hook output

Each `cmd:output` frame produced by a webhook fire is **also** mirrored to
the local REPL with a `[hook:<name>]` prefix in the matching chalk color.

---

## JSON mode (`--json`) {#json-mode}

Commands run with `--json` keep stdout for exactly one JSON document. Everything
the CLI would otherwise print to stdout, including `console.log` from exchange
code, goes to stderr instead, and the background update notice is skipped. See
[Machine-readable output](./commands.md#json) for which commands support it.

## Status bar + footer {#status-bar--footer}

Changing login clears the old status and watch messages, stops its redraws and
loads the new user's footer settings. Late exchange events, command output and
webhook replies from the previous session are discarded.

Two pinned single-line surfaces frame the scrolling output, both reserved via
one shared ANSI scroll region (so they never fight each other or corrupt the
prompt):

- **Top alert bar — transient.** Mirrors the latest _notable_ event
  (warn / error / critical) with category colour, then dims a few seconds after
  it lands so it stays glanceable without nagging. Routine info / operational
  lines never appear here (they'd flicker it), so a real error still surfaces
  even if the scrollback has moved on. A dimmed divider separates it from the
  content once an alert has fired.
- **Bottom footer — always-on** (opt out with `footer off`). A customizable,
  glanceable status line: connection, account, symbol, balance, PnL, clock,
  CLI version, literal text, and custom-script segments. Arrange it with the
  [`footer` command](./commands.md#footer); the layout persists in the
  [`footer` config block](../operations/config-files.md). A dimmed divider sits
  above it.

Both **no-op on a non-TTY** (pipes / CI) and suspend/resume cleanly around
watch-mode alt-screen. Source: `apps/cli/src/scroll-region.ts` (the single
region owner), `status-line.ts`, `footer.ts`, `footer-segments.ts`.

Order and position sizes (`o`, `ps`, `p`, the prompt, charts, and the footer
`position` segment) are base-asset amounts, shown to the market's base lot step
(lot step × contract size); a step of 1 or more shows whole units.
Order tables, chart annotations (including SL/TP/trailing orders), and the chase
monitor's current-order row show the remaining base amount after partial fills.
Inverse contracts have a price-dependent base size, so fractional amounts use
up to eight significant digits rather than a fixed decimal count. Small nonzero
positions and remaining order sizes stay visible.

Unknown position valuation displays as `—` in position cards and position/margin
tables, including derived leverage, PnL percentages and incomplete account totals.
Known quantities and currency PnL remain visible. The footer also shows `—` for
the PnL percentage until a known nonzero valuation is available.

---

## Command audit & tracing {#audit-tracing}

Distinct from terminal output, the CLI can keep a durable, append-only record
of every **write-op** (orders / cancels / closes / leverage) — including
failures and script-driven writes — with a shared `runId` per typed line and
fills attributed back to the command that placed them. Off by default; written
to `~/.tealstreet/audit.log` (separate from `cli.log`). See the
[`audit` command](./commands.md#audit).

---

## Prompt

The REPL prompt is fully configurable. View it with `prompt`, set it with
`prompt set "<format>"`.

```text
prompt                          # show current
prompt list                     # show all named presets
prompt preset <name>            # switch to a preset
prompt set "{symbol} ${price} > "
prompt prefix <p>               # change the leading "> " marker
prompt preview                  # render the current format with live data
```

### Tokens

| Token        | Renders                             |
| ------------ | ----------------------------------- |
| `{symbol}`   | Focused symbol                      |
| `{account}`  | Account name                        |
| `{price}`    | Last price for focused symbol       |
| `{side}`     | Position side (`L` / `S` / `—`)     |
| `{size}`     | Base amount + asset (`240 DOGE`)    |
| `{entry}`    | Entry price                         |
| `{pnl}`      | Unrealized PnL (currency)           |
| `{pnl%}`     | Unrealized PnL (percent)            |
| `{liq}`      | Liquidation price                   |
| `{leverage}` | Current leverage                    |
| `{orders}`   | Open order count                    |
| `{conn}`     | Connection health glyph (see below) |

Alias `ps` works for `prompt`. Source: `PromptCommand.ts`.

---

## Watch modes &nbsp; _(standalone-only)_ {#watch-modes}

Live, full-screen views that refresh every **1000 ms**.

| Alias | Long forms                                          | Shows                             |
| ----- | --------------------------------------------------- | --------------------------------- |
| `wa`  | `watch account`, `watch acc`                        | Margin + positions + orders combo |
| `wp`  | `watch positions`, `watch pos`                      | Live positions table              |
| `wo`  | `watch orders`, `watch ord`                         | Live orders table                 |
| `wm`  | `watch margin`, `watch balance`, `watch bal`        | Live margin                       |
| —     | `watch chart` &nbsp; _(shorthand `watch c <args>`)_ | Live ASCII candlestick            |
| —     | `watch chase`                                       | Live chaser status panel          |

`wa`, `wp`, `wo`, `wm` all accept `--all` / `-a` for the multi-account view.

### Exit keys

| Context      | Keys                         |
| ------------ | ---------------------------- |
| REPL mode    | `Escape`, `Ctrl+X`           |
| One-shot CLI | `Ctrl+C`, `Escape`, `Ctrl+X` |

(In the REPL, `Ctrl+C` is reserved for readline line-cancel.)

### Terminal recovery

Watch modes use the alt screen + raw mode. On exit the CLI runs
`recreateReadline()` because readline state can corrupt across raw-mode
toggles. There's a 200 ms cooldown (`MONITOR_EXIT_COOLDOWN_MS`) where prompt
updates are skipped — that's intentional, not a freeze. Source:
`apps/cli/src/repl.ts:280-330`.

### Web parity

Watch modes are **standalone-only**. The web app has its own live panels
(positions table, orders panel, etc.) rendered as components, not as a
terminal canvas.

---

## Connection health

The `{conn}` prompt token and `accounts` listings render colored dots:

| Color  | Meaning                |
| ------ | ---------------------- |
| Green  | Healthy                |
| Yellow | Warning / initializing |
| Red    | Error / disconnected   |
| Gray   | Not connected          |

Driven by `getConnectionStatus(exchange)` from `ConnectionMonitor`.
Force a fresh socket with `conn reconnect`.

---

## Fill notifications

Every fill on the active exchange produces a one-liner:

```text
✓ Fill: Bought 0.01 BTC-PERP @ $50,124.5
```

When you're inside a watch mode, the fill is appended via `addWatchMessage`
to whichever panel is up. Switching back to an already enabled account still
prints each fill once. Source: `apps/cli/src/exchange-listeners.ts`.

---

## Critical error suppression

Per-account critical errors (geo-blocked, invalid API key, etc.) are
classified by `HealthMonitor` and **only printed once** per category.
If your error scrolled by an hour ago, check `cli.log` — it's still in there.
Source: `apps/cli/src/exchange-listeners.ts`.

---

## Network monitor + rate limiting

The CLI intercepts `console.*` (`apps/cli/src/logger.ts`) and applies, in
order: **operational-noise suppression** (operational lines go to `cli.log`
only — see [Output kinds](#output-kinds)), then a **5 messages / second** rate
limit with a **10 s dedup window** on whatever remains, then readline-aware
printing above the prompt. `print()` and the unwrapped `originalConsoleLog()`
bypass all of this and are what command handlers use for primary output. The
interceptor is the main defense against runaway third-party / WebSocket
logging shredding the prompt.
