---
title: Config files
sidebar_position: 7
---

# Config files

Everything the CLI persists lives under **`~/.tealstreet/`**. All of it
is plain text or SQLite — readable with `cat`, `jq`, or `sqlite3`. No
binary blobs, no hidden state stores.

---

## Files

| File                   | Shape                                                                                                                                                                                                                                                                                                                                                                                                                        | Lifetime                                                                                                                                             |
| ---------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| `config.json`          | `{ defaultAccount, defaultSymbol, accounts: [...], auth: {...}, listener: { enabled?, accountSync?, remoteExec? }, footer: { enabled?, intervalMs?, segments?: [{ type, group?, text?, name?, color? }] } }`                                                                                                                                                                                                                 | Persistent. Main config + creds.                                                                                                                     |
| `tasks.json`           | `{ version: 1, tasks: PersistedTask[] }`                                                                                                                                                                                                                                                                                                                                                                                     | Persistent. Top-level `tealstreet tasks` view.                                                                                                       |
| `cross-trade.json`     | `{ configs: StoredCrossTradeConfig[] }` — each row extends safe-cex's `CrossTradeConfig` (`{ enabled, masterAccount, masterSymbol?, masterWeight, copiers: CrossTradeCopier[] }`) with `id` (uuid), `updatedAt` (ms), and optional `deletedAt` (tombstone). Read/written by `tealstreet cross-trade {list,add,enable,disable,remove,run}`. Same shape as the web side so the listener bridge can sync edits across runtimes. | Persistent. LWW + 30d tombstone retention.                                                                                                           |
| `vars.json`            | `Record<string, string>` — static `set @x` only                                                                                                                                                                                                                                                                                                                                                                              | Persistent.                                                                                                                                          |
| `aliases.json`         | `Record<string, string>`                                                                                                                                                                                                                                                                                                                                                                                                     | Persistent.                                                                                                                                          |
| `aliases-sync.json`    | `{ cursor, pending[] }`                                                                                                                                                                                                                                                                                                                                                                                                      | Alias changes not yet accepted by the server, and the server revision seen. Deleting it re-syncs from scratch (the account's aliases win conflicts). |
| `whitelist.json`       | `string[]` (symbols)                                                                                                                                                                                                                                                                                                                                                                                                         | Persistent.                                                                                                                                          |
| `fatfingers.json`      | `Record<string, number>` — symbol → max contracts                                                                                                                                                                                                                                                                                                                                                                            | Persistent.                                                                                                                                          |
| `simulation.json`      | `{ enabled: boolean }`                                                                                                                                                                                                                                                                                                                                                                                                       | Persistent.                                                                                                                                          |
| `budget.json`          | `{ usd: number }` — aggregate notional cap (0 = off). Spend is session-only, not stored.                                                                                                                                                                                                                                                                                                                                     | Persistent (cap only).                                                                                                                               |
| `audit.json`           | `{ enabled: boolean }` — write-op audit log on/off (off by default).                                                                                                                                                                                                                                                                                                                                                         | Persistent.                                                                                                                                          |
| `audit.log`            | Append-only JSONL — one `{ ts, runId?, account, command, ok, orderIds?, error? }` per write-op while auditing is on; `runId` groups all writes from one typed line / `repeat` / script run, and `[fill]` lines attribute fills back to the placing command/run.                                                                                                                                                              | Append-only; size-capped (~5 MB) then rolled.                                                                                                        |
| `audit.log.1`          | Single rotated backup of `audit.log` (the previous ~5 MB). Replaced on each rotation. `audit tail` dips into it when the live log is short after a roll.                                                                                                                                                                                                                                                                     | Overwritten on each rotation.                                                                                                                        |
| `cli-export.json`      | `{ version: 1, vars: {...}, aliases: {...}, binds: {} }`                                                                                                                                                                                                                                                                                                                                                                     | Written by the REPL `export` command.                                                                                                                |
| `listener.json`        | `{ listenerId, createdAt }`                                                                                                                                                                                                                                                                                                                                                                                                  | Persistent. Identity for the loopback listener.                                                                                                      |
| `trusted-clients.json` | `{ clients: TrustedClient[] }`                                                                                                                                                                                                                                                                                                                                                                                               | Persistent. Paired web clients.                                                                                                                      |
| `hooks.json`           | `{ hooks: Hook[] }` &nbsp; **— contains plaintext secrets**                                                                                                                                                                                                                                                                                                                                                                  | Persistent.                                                                                                                                          |
| `cli.log`              | Log lines                                                                                                                                                                                                                                                                                                                                                                                                                    | **Cleared on every CLI startup.**                                                                                                                    |
| `market-data.db`       | SQLite — orderbook snapshots + recording config                                                                                                                                                                                                                                                                                                                                                                              | Persistent. See [Recording](./recording.md).                                                                                                         |
| `cli_history`          | Newline-separated commands (cap 1000)                                                                                                                                                                                                                                                                                                                                                                                        | Persistent.                                                                                                                                          |
| `scripts/`             | Directory. One `.ts` file per script + a seeded `tsconfig.json`, `types.d.ts`, and `hello.ts` example.                                                                                                                                                                                                                                                                                                                       | Persistent. See [Scripts](#scripts) below.                                                                                                           |

---

## Session-only state

These are **never written to disk**. Restart the CLI and they're gone:

- **Live vars** (`sets @x …`) — reactive variables that recompute on
  every read.
- **Tracked vars** (`track max @x …` / `track min @x …`) — ratchet
  trackers.
- **In-process chasers and TWAPs** — running background tasks. The
  top-level `tealstreet tasks` view reads `tasks.json` instead, which
  is a separate persisted record. See [Tasks](../reference/commands.md#tasks-in-process).

If you want to round-trip your vars + aliases between machines, use
[`export` / `import`](#export--import) below.

---

## Overrides

Two environment variables relocate the state directory:

| Variable                 | Effect                               |
| ------------------------ | ------------------------------------ |
| `TEALSTREET_CONFIG_DIR`  | Move the entire `~/.tealstreet/` dir |
| `TEALSTREET_CONFIG_FILE` | Override just the main `config.json` |

Useful for sandboxed test setups (`TEALSTREET_CONFIG_DIR=/tmp/ts-test
tealstreet`) or for keeping multiple identity configs on one machine.

---

## Exchange endpoint overrides

Most exchange URLs are built into the CLI through safe-cex. HyroTrader is
different: its challenge endpoint is not stored in `config.json`, so the
standalone CLI reads it from environment variables at process start:

- `NEXT_PUBLIC_HYROTRADER_PROD_REST_URL` — HyroTrader production REST
  origin. Required for ordinary `hyrotrader` challenge accounts.
- `NEXT_PUBLIC_HYROTRADER_PROD_PUBLIC_WS_URL` — optional public
  WebSocket URL. Defaults to REST origin + `/v5/public/linear`.
- `NEXT_PUBLIC_HYROTRADER_PROD_PRIVATE_WS_URL` — optional private
  WebSocket URL. Defaults to REST origin + `/v5/private`.

Use the REST origin only, for example `https://example.hyrotrader.invalid`,
not a `/v5/...` endpoint path. If the REST origin is missing, the CLI
cannot connect and prints:

```text
HyroTrader production REST URL is not configured
```

These variables must be set in the shell that launches `tealstreet`, or
persisted in your shell profile if you want them available every time.
`NEXT_PUBLIC_HYROTRADER_SIMULATOR_*` is reserved for the shared Tealstreet
demo/development simulator account and should not be used as a production
challenge template.

---

## Crash reporting

Released binaries report crashes (uncaught exceptions and unhandled
rejections) to Tealstreet's error tracker so we find breakage that never
makes it into a bug report.

| Variable                 | Effect                   |
| ------------------------ | ------------------------ |
| `TEALSTREET_TELEMETRY=0` | Turn crash reporting off |

Also accepts `false` / `off`. Development runs (`bun run src/index.ts`)
never report — only versioned release binaries do.

What is sent: the exception type, message, and stack trace. Values that
look like credentials are redacted before the event leaves your machine,
and the hostname, OS username, request bodies, and account details are
not collected. Nothing from `config.json` or `hooks.json` is read by this
path.

---

## Backing up

`config.json` and `hooks.json` are the sensitive ones — `config.json`
contains exchange API credentials, `hooks.json` contains webhook
secrets. Treat both like credentials. Don't sync them through anything
unencrypted. The CLI writes both, and `account export` output, as owner-only
(`0600`).

The rest of the directory (vars, aliases, whitelist, fatfingers, sim
state, history, market-data.db) is safe to back up wholesale:

```bash
tar -czf tealstreet-backup.tar.gz \
  --exclude='cli.log' \
  --exclude='hooks.json' \
  --exclude='config.json' \
  ~/.tealstreet
```

Restore by extracting back into `~/.tealstreet/`.

---

## Export / import

Two distinct round-trip flows exist. They do not overlap.

| Command                                          | Surface                  | Content                           | File                            |
| ------------------------------------------------ | ------------------------ | --------------------------------- | ------------------------------- |
| `export` / `import`                              | REPL only                | Vars + aliases (`binds` reserved) | `~/.tealstreet/cli-export.json` |
| `tealstreet accounts export` / `accounts import` | Top-level binary or REPL | Exchange account credentials      | Path you pass (or stdin/stdout) |

Use the first for moving your workflow setup between machines. Use the
second for moving exchange creds — typically when seeding a new machine
from the web app's exported account JSON.

See [Aliases & vars → export/import](../reference/aliases-and-vars.md)
for the workflow round-trip and
[Commands → Accounts](../reference/commands.md#accounts) for the
credential round-trip.

---

## Scripts

`~/.tealstreet/scripts/` holds your custom Tealstreet scripts — one
`.ts` file per script, plus three seeded files the CLI manages.

On first launch the CLI creates the directory and writes:

- `tsconfig.json` — TypeScript configuration so your editor picks up
  the ambient types. Only-if-missing; safe to edit.
- `types.d.ts` — ambient declarations for the script-facing API
  (`api`, `args`, `ScriptMeta`, `Capabilities`, etc.). **Rewritten on
  every CLI launch** so the surface stays in sync with the CLI version
  you're running — don't bother editing it, your changes will be
  overwritten.
- `hello.ts` — example script you can rename, edit, or delete.
  Only-if-missing.

Author a script by dropping a new `.ts` file into the directory.
Filenames must match `/^[a-zA-Z_][a-zA-Z0-9_-]*$/` (letters, digits,
underscores, hyphens; no spaces, no dots). The filename sans `.ts` is
the script's name. Subdirectories are ignored. Names that collide with
REPL builtins (`buy.ts`, `tasks.ts`, etc.) are legal — see "Invoking a
script" below.

Each script must export:

- `meta: ScriptMeta` — declares the `targets`, `capabilities`, and
  optional API versions the script needs.
- `default async function (args, api)` — the actual script body. `api`
  is the dispatcher's gate-checked surface, `args` carries the parsed
  REPL arguments.

### Invoking a script

The REPL routes scripts via the `script:<name>` prefix:

```
> script:hello
> script:hello arg1 --flag=value
```

The prefix is required. Bare `hello` is never resolved to a script —
it falls through to cli-core's regular command dispatcher, where it
either matches a builtin / user-defined alias / capture-variable name,
or finally errors as unknown if nothing matches. The prefix-only model
means filenames like `buy.ts` are perfectly legal: invoke them as
`script:buy` and the builtin `buy` keeps working unchanged.

`script:` composes with the rest of the chain syntax: `retry N
script:foo`, `@x = script:foo`, `set ec; script:foo`, webhook fires
(`hook create alert "script:foo 0.001 BTC"`), and chain operators
(`;` and `&`).

After editing a script, run `script reload` in the REPL to re-scan
the directory. The CLI does **not** auto-reload on file save.

## See also

- [Recording](./recording.md) — `market-data.db` schema.
- [Listener](./listener.md) — `listener.json`, `trusted-clients.json`,
  `hooks.json`.
- [Pairing](./pairing.md) — how `trusted-clients.json` gets populated.
