---
title: Web CLI vs standalone
sidebar_position: 8
---

# Web CLI vs standalone

Tealstreet ships two CLI runtimes:

- **Web CLI** — the panel inside the trading terminal. Runs in your browser.
- **Standalone CLI** — `tealstreet` binary on macOS / Linux / Windows.

The grammar, commands, and constants are the same. The differences below
are about **where** code runs and **which surfaces** are reachable.

If you can pick, the standalone CLI is the superset. Use the web CLI for
quick-fire trades while your eyes are on the terminal, and use standalone
for anything involving watch modes, recording, hooks, or being the listener
target for a tab that's about to be backgrounded.

---

## Where commands run (web CLI)

Every command in the web CLI runs in one of three places:

| Tier                   | Where                  | Commands                                                                                                                                                                                                                                                                                                                                                               |
| ---------------------- | ---------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Local registry         | Main thread            | `set`, `sets`, `track`, `untrack`, `unset`, `print`, `vars`, `alias`, `unalias`, `whitelist`, `unwhitelist`, `fatfinger`, `unfatfinger`, `simulation`, `sim`, `dry-run`, `history`, `wait`                                                                                                                                                                             |
| Worker (`safe-cex`)    | WebWorker              | Every trading command: `buy`, `sell`, `close`, `cancel`, `chase`, `twap`, `scale`, `swarm`, `nuke`, `stop`, `bump`, `move`, `chasers`, `tasks`, `kill`, `chart`, `position`, `positions`, `orders`, `margin`, `balance`, `bal`, `spot`, `last-price`, `mark-price`, `index-price`, `leverage`, `max`, `markets`, `echo`, `say`, `clear`, `cls`, `status`, `connection` |
| Listener (when paired) | Standalone CLI process | Anything you forward via the `listener` target (see [Pairing](../operations/pairing.md))                                                                                                                                                                                                                                                                               |

Source: `LOCAL_REGISTRY_COMMANDS` in `apps/web/src/components/cli/CLI.tsx`.

The split exists because the worker holds the live exchange connection; the
main thread holds the var / alias / setting state. `wait` is local because
it polls a local exchange store, not the worker.

---

## Standalone-only commands

These never run in the web CLI — either because they take over a terminal,
because they manage host-process state, or because they live in subsystems
that exist only in the standalone binary.

| Standalone-only             | Why                                                 |
| --------------------------- | --------------------------------------------------- |
| `watch *`                   | Alt-screen takeover                                 |
| `login` / `logout` / `auth` | OAuth flow via local port 9876                      |
| `update`                    | Updates the standalone binary                       |
| `hook *` / `hooks`          | Loopback HTTP listener — only the standalone has it |
| `clients *`                 | Same — listener subsystem                           |
| `export` / `import`         | Filesystem (`~/.tealstreet/cli-export.json`)        |
| `exit` / `quit`             | Terminates the standalone process                   |
| `logs` / `clearlogs`        | Filesystem-backed log file                          |
| `record *`                  | SQLite-backed market data recorder                  |

Top-level binary subcommands (`tealstreet account add`, `tealstreet exec`,
`tealstreet record`, `tealstreet tasks`, etc.) are by definition
standalone-only — there's no binary in the web context.

---

## `chart` is web-runnable but standalone-shaped

`chart` runs without throwing in the web CLI, but it's designed for a
terminal canvas and looks awful in a fixed-width DOM frame. Use the
terminal chart panel instead — it has the same data and a real renderer.

---

## Web-only constants

These constants exist **only** in the web CLI and throw "only available in
the web terminal" if you reference them from the standalone CLI:

| Token                 | Reads                                      |
| --------------------- | ------------------------------------------ |
| `$click`              | Last chart-click price from a linked chart |
| `$ordersize`          | Current size in the linked order form      |
| `$quick1` … `$quick7` | Quick-size slots 1 through 7               |

Resolved through `PreprocessContext.uiResolvers`
(`apps/web/src/components/cli/uiTokenResolvers.ts`).

---

## When to reach for standalone

A few cases where the web CLI is the wrong tool:

| Want                                                            | Use                                                                      |
| --------------------------------------------------------------- | ------------------------------------------------------------------------ |
| Watch a TWAP from a `wp --all` panel without leaving your shell | Standalone, `wp --all`                                                   |
| Receive a TradingView webhook                                   | Standalone (see [Hooks & webhooks](../operations/hooks-and-webhooks.md)) |
| Record an L2 orderbook to SQLite                                | Standalone (see [Recording](../operations/recording.md))                 |
| Leave the listener up overnight while the browser is asleep     | Standalone — pair it with the web tab                                    |

The reverse case — when web is better — is mostly speed of iteration: it
shares the live terminal's exchange connections, so you don't pay a
connect-and-warm cost on every restart.

---

## Pairing the two

You can route web CLI commands **through** a running standalone listener.
That's the `listener` target, configured per CLI tab in the web app. The
full pairing flow is documented under
[Operations → Pairing](../operations/pairing.md).
