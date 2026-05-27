---
title: Recording
sidebar_position: 5
---

# Recording

A built-in market-data recorder that snapshots orderbooks (and, with caveats,
trades and funding) into a local SQLite database for offline analysis.
Available both as a top-level subcommand and inside the REPL.

---

## Surface

The recorder has the same subcommand shape in both places.

### Top-level (`tealstreet record …`)

| Invocation                                            | Result                                  |
| ----------------------------------------------------- | --------------------------------------- |
| `tealstreet record <exchange>`                        | Shorthand for `record start <exchange>` |
| `tealstreet record start <exchange> [flags]`          | Begin a recording                       |
| `tealstreet record stop <exchange> [--type --symbol]` | Stop a recording                        |
| `tealstreet record list`                              | Table of active + paused recordings     |
| `tealstreet record stats [exchange]`                  | DB size, row counts, per-type breakdown |

### REPL (`record`, alias `rec`)

Same subcommands as the top-level. `record <exchange>` is shorthand for
`record start <exchange>`.

```text
> record bybit
> record start binance --symbol ETHUSDT --interval 30s
> record list
> record stats
> record stop bybit --symbol BTCUSDT
> rec stats binance
```

---

## Flags

| Flag                                  | Default     | Notes                            |
| ------------------------------------- | ----------- | -------------------------------- |
| `--type <orderbook\|trades\|funding>` | `orderbook` | See [Types](#types) below        |
| `--symbol <SYMBOL>`                   | `BTCUSDT`   | Single symbol per recording      |
| `--interval <30s\|1m\|5m\|…>`         | `1m`        | Snapshot cadence                 |
| `--retention <7d\|14d\|30d\|…>`       | `7d`        | Older rows are pruned            |
| `--levels <N>`                        | `25`        | Orderbook depth (orderbook only) |

Recordings are keyed by `(data_type, exchange, symbol)`. Starting a second
recording with the same triple updates the existing config row rather than
creating a duplicate.

---

## Types

| Type        | Status                                                                                           |
| ----------- | ------------------------------------------------------------------------------------------------ |
| `orderbook` | Production — L2 snapshot at the configured cadence and depth.                                    |
| `trades`    | Verify — accepted by the parser; per-recorder behavior should be confirmed before relying on it. |
| `funding`   | Verify — accepted by the parser; per-recorder behavior should be confirmed before relying on it. |

The recorder keeps running for the lifetime of the CLI process as long as
the recording is in the `active=1` state.

---

## Database

Recordings live in **`~/.tealstreet/market-data.db`** — a single SQLite file
with WAL mode enabled. Inspect it with any SQLite client.

```bash
sqlite3 ~/.tealstreet/market-data.db
sqlite> .tables
sqlite> SELECT exchange, symbol, COUNT(*) FROM orderbook_snapshots GROUP BY 1,2;
```

### Schema (v1)

```sql
CREATE TABLE schema_version (version INTEGER PRIMARY KEY);

CREATE TABLE recording_config (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  data_type TEXT NOT NULL,
  exchange TEXT NOT NULL,
  symbol TEXT NOT NULL,
  interval_ms INTEGER NOT NULL DEFAULT 60000,
  retention_days INTEGER NOT NULL DEFAULT 7,
  config TEXT,
  active INTEGER NOT NULL DEFAULT 1,
  created_at INTEGER DEFAULT (strftime('%s','now')*1000),
  UNIQUE(data_type, exchange, symbol)
);

CREATE TABLE orderbook_snapshots (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  exchange TEXT NOT NULL,
  symbol TEXT NOT NULL,
  timestamp INTEGER NOT NULL,
  bids TEXT NOT NULL,
  asks TEXT NOT NULL,
  created_at INTEGER DEFAULT (strftime('%s','now')*1000)
);

CREATE INDEX idx_orderbook_lookup ON orderbook_snapshots(exchange, symbol, timestamp);
CREATE INDEX idx_orderbook_timestamp ON orderbook_snapshots(timestamp);
```

`bids` and `asks` are JSON arrays of `[price, size]` pairs, depth-bounded
by `--levels` at write time. `timestamp` is milliseconds since epoch.
Schema version `1` is the only version that exists today.

---

## Stats

```text
> record stats
Database: ~/.tealstreet/market-data.db   Size: 184 MB

Exchange   Symbol     Type        Rows       Oldest                Newest
bybit      BTCUSDT    orderbook   60,480     2026-05-10 09:00 UTC  2026-05-17 12:14 UTC
binance    ETHUSDT    orderbook   20,154     2026-05-15 18:22 UTC  2026-05-17 12:14 UTC
```

`record stats <exchange>` filters to one venue.

---

## See also

- [Commands → Recording](../reference/commands.md#recording) — REPL surface reference.
- [Config files](./config-files.md) — every other file under `~/.tealstreet/`.
- [Output](../reference/output.md) — how recorder messages are colored and rate-limited.
