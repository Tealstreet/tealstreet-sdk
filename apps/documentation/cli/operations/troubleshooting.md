---
title: Troubleshooting
sidebar_position: 6
---

# Troubleshooting

Common failure modes and how to diagnose them. If something here doesn't
match what you're seeing, check `~/.tealstreet/cli.log` — full detail
lands there even when terminal output is rate-limited or suppressed.

---

## Connection health

Every `accounts` listing renders a colored dot per account, and the
[`{conn}` prompt token](../reference/output.md#connection-health) renders
one for the focused account.

| Color  | Meaning                |
| ------ | ---------------------- |
| Green  | Healthy                |
| Yellow | Warning / initializing |
| Red    | Error / disconnected   |
| Gray   | Not connected          |

Force a fresh socket on the focused account:

```text
> conn reconnect
> ws reconnect          # same — alias
```

Multi-account snapshot:

```text
> conn --all
```

See [`connection`](../reference/commands.md#connection) for the full
command surface.

---

## "Why didn't that error show up again?"

Critical errors (geo-blocked region, invalid API key, …) are classified
per account and **printed once per category**. Repeated occurrences of
the same class are suppressed at the terminal but still written to the
log file.

If you missed an error, look in:

```bash
tail -f ~/.tealstreet/cli.log
```

Or from inside the REPL:

```text
> logs            # print the log file path
> logs clear      # wipe it
> clearlogs       # same — alias
```

`cli.log` is **cleared on startup**, so a fresh session always starts
with an empty file.

---

## "The exchange rejected this API key"

The venue answered a credentialed request with an auth error, so the command
stops there instead of running against a half-dead session and printing a table
of zeros. Exit code is `1`.

Almost always the key was minted in a different environment from the account it
was added to. **Demo and live are separate key pages on the same venue** — a
Bybit mainnet key will not work on a `bybitdemo` account, and vice versa. Check
you created the key in the same place the account points at, then:

```bash
tealstreet account remove <name>
tealstreet account add -n <name> -e <exchange> -k <key> -s <secret> --default
```

Commands that do not need the venue — `alias`, `simulation`, `fatfinger`,
`whitelist`, `symbol` — still run normally with a rejected key.

## "HyroTrader production REST URL is not configured"

This is not an API-key rejection. It means the standalone CLI does not
know which HyroTrader production host to call yet.

Set the HyroTrader production REST origin in the same shell that launches
the CLI, then reconnect:

```bash
export NEXT_PUBLIC_HYROTRADER_PROD_REST_URL="https://<hyro-production-origin>"
tealstreet interactive --account <hyro-account-name>
```

Use the origin only, not a `/v5/...` path. The CLI normally derives the
public and private WebSocket URLs from that REST origin. If HyroTrader or
Tealstreet gives you separate WebSocket hosts, set the optional overrides
listed in [Config files → Exchange endpoint overrides](./config-files.md#exchange-endpoint-overrides).

The `NEXT_PUBLIC_HYROTRADER_SIMULATOR_*` variables are for Tealstreet's
shared demo/development simulator path, not ordinary challenge accounts.

## "Why is Bybit returning a 403 geo-block?"

That error means the CLI reached Bybit directly from a restricted
network route. Use a VPN, or add the account with Tealstreet proxy
routing from inside the REPL:

```text
> accounts add -n mybybit -e bybit -k <KEY> -s <SECRET> --tealstreet-proxy --proxy-region SG
```

This only changes how the CLI reaches Bybit. TradingView webhooks are set
up separately — see
[Webhook from TradingView](../recipes/webhook-from-tradingview.md).

---

## "Why did my typo do something weird?"

The REPL has one bit of magic: a single token that matches a market on
the focused exchange is treated as a `switch`.

```text
> BTCUSDT           # → switch BTCUSDT
```

A typo like `buyy 100$` doesn't match either a known command or a
symbol, so it falls through to **Unknown command** instead of being
absorbed silently. The shorthand only fires on exactly one token, and
only if it's a real market.

---

## "Why is my console flooded with the same line?"

The CLI's console interceptor is rate-limited to **5 messages / second**
with a **10 s dedup window**. If you're seeing the same warning over and
over from a third-party library, the interceptor will start dropping
duplicates after the first burst.

Primary command output goes through `print()`, which **bypasses the
rate limiter**, so trading output is never throttled.

See [Output → Network monitor](../reference/output.md#network-monitor--rate-limiting)
for details.

---

## Watch modes seem to freeze on exit

[Watch modes](../reference/output.md#watch-modes) (`wa`, `wp`, `wo`,
`wm`, `watch chart`, `watch chase`) use the alt screen plus raw mode.
On exit the readline state is rebuilt and prompt updates are skipped
for **200 ms**.

That short pause is intentional — it's not a freeze. If the prompt
still looks broken after a second, press Enter to force a redraw.

---

## "Where did my fill notification go?"

Every fill on the focused account produces:

```text
✓ Fill: Bought 0.01 BTCUSDT @ $50,124.5
```

If you're inside a watch mode when the fill lands, the message is
appended to whichever panel is showing rather than printed to the REPL
scrollback. Drop out (`Escape` or `Ctrl+X`) and check `logs` — every
fill is also written to `cli.log`.

---

## "My chaser is still in the book after `chase cancel`"

`chase cancel` cancels the chaser **and** the rest order. `chase cancel
keep` cancels the chaser but **leaves the working order in the book**.
If you used the `keep` variant and then wondered why the order is still
there — that's why. Use plain `chase cancel` to clear both, or
`cancel <id-prefix>` to drop just the order.

See [`chase`](../reference/commands.md#chase) for the full form table.

---

## "Why is `wait time 1200` blocking for 24 hours?"

`wait time HHMM` is a clock-time wait (UTC). An **exact match** to the
current time schedules for the **next** occurrence — i.e. ~24 hours
out. If you meant "the next time it's 12:00 UTC, but not right now",
that's exactly what you got.

To unstick a one-minute-from-now wait, target the next minute boundary:

```text
> wait time 1201
```

See [`wait`](../reference/workflow.md) for the full grammar.

---

## "The update banner won't go away"

A background updater check runs at REPL startup and prints a banner
when a newer version exists.

```text
> update -c       # version-check only — confirms what's available
> update          # fetch and replace the running binary
```

After `update`, restart the CLI. The banner only suppresses for the
current process.

---

## "Did my order go through? I was in `sim`"

When [simulation](../reference/safety.md#simulation) is on, no orders
hit the network. Every "execution" is a printed line, nothing more.

Toggle it explicitly:

```text
> sim                # show state
> sim off
```

Worth the half-second check before chasing a "missing" fill. The
[fatfinger](../reference/safety.md#fatfinger) and
[whitelist](../reference/safety.md#whitelist) guards can also reject
orders before they're sent — both print a `warn` line when they fire.

---

## Logs and where they live

| File                        | Contains                                     |
| --------------------------- | -------------------------------------------- |
| `~/.tealstreet/cli.log`     | Logger output + intercepted console messages |
| `~/.tealstreet/cli_history` | Newline-separated REPL history (cap 200)     |

`cli.log` is cleared on every CLI start. If you need to capture a
session, copy it before you exit.

---

## See also

- [Safety](../reference/safety.md) — simulation, fatfinger, whitelist.
- [Output](../reference/output.md) — output kinds, fill notifications,
  rate limiting.
- [Config files](./config-files.md) — every file under `~/.tealstreet/`.
