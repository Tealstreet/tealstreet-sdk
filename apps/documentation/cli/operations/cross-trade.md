---
title: Cross-trade engine
sidebar_label: Cross-trade
description: Run the Tealstreet cross-trade copier as a CLI daemon instead of a browser tab.
---

# Cross-trade engine

Cross-trade mirrors a master account's positions onto one or more copier
accounts in near-real-time. By default it runs inside the browser leader
tab. Power users can run the same engine as a CLI daemon instead — useful
when you want it to keep running with the browser closed.

> **v1 limitation — Chase execution mode is web-only.** The CLI engine
> supports **Market** and **LimitSlippage** execution modes. Configs
> with `Chase` copiers are accepted at `cross-trade add` (so the row
> still sits in `cross-trade.json`) but skipped at `cross-trade run`
> with a per-config warning. Use Market or LimitSlippage if you want
> the config to run on the CLI.

## Quick start

```bash
# 1. Add a config (interactive flags). Account names match `tealstreet account list`.
tealstreet cross-trade add --master my-master --copier my-copier --symbol BTCUSDT --mode Market --enabled

# 2. See what's stored.
tealstreet cross-trade list

# 3. Run the daemon (foreground; Ctrl+C to stop).
tealstreet cross-trade run

# 4. From another shell, confirm it's running.
tealstreet cross-trade status
```

`run` blocks. It boots the accounts referenced by enabled configs, prints
a banner once the engine is live, logs a 30s status line, and shuts down
cleanly on `SIGINT` or `SIGTERM`.

## Commands

| Command                    | What it does                                                                                                                          |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| `cross-trade list`         | Show every config in `~/.tealstreet/cross-trade.json` (tombstoned rows hidden).                                                       |
| `cross-trade add ...`      | Build a single-master / single-copier row. Required: `--master`, `--copier`. Optional: `--symbol`, `--mode`, `--weight`, `--enabled`. |
| `cross-trade enable <id>`  | Flip the top-level `enabled` flag. `<id>` accepts an 8-char prefix.                                                                   |
| `cross-trade disable <id>` | Same as above, off.                                                                                                                   |
| `cross-trade remove <id>`  | Soft-delete (tombstone). The web side picks this up via the listener bridge.                                                          |
| `cross-trade run`          | Foreground daemon. Boots accounts, starts the engine, blocks until SIGINT.                                                            |
| `cross-trade status`       | Read the heartbeat. Shows running/not, pid, uptime, active configs, last heartbeat age.                                               |

Execution modes accepted by `--mode`: `LimitSlippage` (default), `Market`,
`Chase` (saved but skipped at run-time in v1).

## How it works

`tealstreet cross-trade run` is the headless equivalent of the browser
leader tab's cross-trade engine. Same audit loop, same drift detection
(>5%), same per-(account, symbol) failure-mute on 2 consecutive failures
for 30s, same min-amount gate, same reduce-only bypass on residual closes.
The CLI swaps in its own placeOrder adapter that talks directly to
`exchange.placeOrder` for `Market` and `LimitSlippage` orders.

The daemon writes a heartbeat to `~/.tealstreet/cross-trade-running.json`
every 5s and clears it on graceful shutdown. The CLI listener
(`~/.tealstreet/listener.json` + bound on the REPL when active) reads
that heartbeat and broadcasts a `state:cross-trade` frame to every
connected web client. Heartbeats older than 15s are treated as dead.

## Web ↔ CLI safety check

When the web tab's cross-trade leader is active AND the CLI daemon is
running, **both engines place orders**. The system warns about this —
it does **not** auto-disable either side.

- **Web** shows a yellow banner above the existing leader-status alert
  on the cross-trade settings panel: _"Cross-trade is ALSO running on
  the CLI ({n} config(s))"_.
- **CLI** prints a throttled (≤1 per 60s) warning line whenever a
  connected web client reports `webCrossTradeActive: true` while the
  daemon is up.

The signal travels over the existing CLI listener bridge: the web tab's
`hello` frame includes `webCrossTradeActive: boolean` (default false),
and a follow-up `client:state` frame is fired whenever the value
changes. The CLI's `state:cross-trade` snapshot tells the web tab the
daemon's running state.

To silence the warning, disable one of the two:

- Web side: close the leader tab, or disable every cross-trade config
  in the settings panel.
- CLI side: Ctrl+C the `cross-trade run` daemon.

## Sync with the web

Configs at `~/.tealstreet/cross-trade.json` share their shape with the
web side (`StoredCrossTradeConfig` extends safe-cex's `CrossTradeConfig`
with `id`, `updatedAt`, and optional `deletedAt`). Edits flow both
directions via the listener bridge using the same LWW pattern as
account sync — newer `updatedAt` wins, tombstones propagate.

See [Config files](./config-files.md) for the on-disk shape and
[Listener](./listener.md#wire-frames) for the wire frames.

## Audit log

When `audit on` is set, the daemon emits `[cross-trade] start` and
`[cross-trade] stop` entries to `~/.tealstreet/audit.log`. Filter via:

```bash
audit tail | grep '\[cross-trade\]'
```

## Troubleshooting

**"No runnable cross-trade configs."** — At least one config must be
top-level `enabled`, have a `masterAccount`, and have ≥1 enabled copier
with an `account`.

**"Skipping N copier(s) using Chase execution mode."** — Expected on
v1; switch to Market or LimitSlippage to run on the CLI, or keep using
the web leader tab for Chase.

**`status` says running, web doesn't show the banner.** — The web side
only flips the banner when the local tab is leader AND has enabled
configs. If you're on a non-leader tab, the banner is suppressed by
design (only the leader can double-trade with the daemon).

**`status` shows last heartbeat >15s ago.** — Daemon was killed
without graceful cleanup. The heartbeat file may still exist on disk;
the staleness gate treats it as dead. Delete
`~/.tealstreet/cross-trade-running.json` manually if you want a clean
slate, or just start a fresh `cross-trade run`.

## Out of scope for v1

- Chase execution mode on the CLI (deferred).
- Supabase-backed config sync (configs are local-file-only for now).
- Auto-suppression of one engine when the other starts (intentional —
  warn-only).
- Multi-process safety: nothing prevents two `cross-trade run` daemons
  on the same host. Don't do that.
