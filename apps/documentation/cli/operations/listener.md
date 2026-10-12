---
title: Listener
sidebar_position: 1
---

# Listener

The local listener is an HTTP + WebSocket server that the standalone CLI binds
on `127.0.0.1:53219`. It exposes three things to localhost: a health endpoint,
a per-hook webhook fire endpoint, and a WebSocket upgrade that lets the web
app drive the standalone CLI as a remote target. Standalone-only — there is no
listener in the web CLI runtime. Relevant when you want to pair the web tab to
a running CLI or keep accounts streaming on a dedicated host while you switch
browsers. Hosted TradingView webhooks do not use this server: they arrive
over the CLI's outbound relay connection — see
[Hooks & webhooks](./hooks-and-webhooks.md#hosted-webhooks).

---

## HTTP surface

| Method + path     | Auth                            | Purpose                                                                  |
| ----------------- | ------------------------------- | ------------------------------------------------------------------------ |
| `GET /health`     | none                            | Liveness probe. Returns `{ ok: true, pid, port }`.                       |
| `POST /hook/<id>` | `X-Hook-Token: <32-hex secret>` | Fire a registered hook. See [Hooks & webhooks](./hooks-and-webhooks.md). |
| WS upgrade `/ws`  | origin allowlist + double-trust | Bidirectional channel for the web `listener` target.                     |
| anything else     | —                               | `404`.                                                                   |

`GET /health`:

```text
$ curl http://127.0.0.1:53219/health
{"ok":true,"pid":42101,"port":53219}
```

---

## WebSocket handshake

When the web app's CLI tab targets `listener`, it opens
`ws://127.0.0.1:53219/ws` and runs through this sequence:

1. **Origin allowlist.** The Sec-WebSocket-Origin header is checked against
   the configured allowlist before the socket is accepted.
2. **`hello` frame.** Client must send `{ t: 'hello', clientId, label? }` as
   its first frame. `clientId` is a UUID the web app generates and persists
   per browser-tab namespace; max 64 chars. `label` is an optional human
   string for the trust prompt.
3. **Trust check.** The CLI looks up `clientId` in
   `~/.tealstreet/trusted-clients.json`. Known clients pass through silently.
   Unknown clients pause the WS handshake and trigger a REPL prompt asking
   the operator to approve or deny.
4. **Accept or reject.** On approval the CLI replies
   `{ t: 'welcome', listenerId, user: { email } }`. On rejection it replies
   `{ t: 'rejected' }` then closes the socket with code **4001**.
5. **Topic subscribe.** Welcomed clients receive snapshots for the topics
   they care about: `accounts`, `tasks`, `hook-output`.

Once a client is welcomed, all further interaction happens via the wire
frames below.

---

## Wire frames

Every frame is JSON with a discriminator field `t`. Listed in handshake-then-
runtime order.

| `t`                         | Direction    | Payload                                                                            | Purpose                                                                                                                                                                                                                  |
| --------------------------- | ------------ | ---------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `hello`                     | client → CLI | `{ clientId, label?, webCrossTradeActive? }`                                       | First frame after socket open. Identifies the web client. `webCrossTradeActive` defaults `false` and signals that this tab is the cross-trade leader with enabled configs — drives the CLI dual-engine warning.          |
| `client:state`              | client → CLI | `{ webCrossTradeActive }`                                                          | Post-hello update to the per-session feature flags carried in `hello`. Fired when the web tab's leader/lock state changes mid-session.                                                                                   |
| `welcome`                   | CLI → client | `{ listenerId, user: { email } }`                                                  | Acceptance. `listenerId` is the CLI's persistent UUID — the web app pins it to detect "different CLI" on the next connect. Email is display-only.                                                                        |
| `rejected`                  | CLI → client | (empty body)                                                                       | Trust denied. Always followed by close code `4001`.                                                                                                                                                                      |
| `cmd:exec`                  | client → CLI | `{ id, command, account?, symbol? }`                                               | Execute `command` as if typed at the REPL prompt. Optional `account` / `symbol` scope the run.                                                                                                                           |
| `cmd:output`                | CLI → client | `{ id, kind, text }` — `kind ∈ print \| success \| warn \| error \| info`          | Streamed output. Same `PrintKind` values as the REPL renders (see [Output](../reference/output.md#output-kinds)). Webhook fires also stream on the `hook-output` topic.                                                  |
| `cmd:done`                  | CLI → client | `{ id, success, error? }`                                                          | Run finished. Stray late `cmd:output` frames from `;` / `&` chains may still arrive after this — match by `id`.                                                                                                          |
| `mutate:account`            | client → CLI | `{ id, op: 'upsert' \| 'delete', account?, name?, updatedAt? }`                    | LWW merge of an account from the web side into the CLI's `accounts` list.                                                                                                                                                |
| `mutate:ack`                | CLI → client | `{ id, data? }`                                                                    | Mutation succeeded.                                                                                                                                                                                                      |
| `mutate:err`                | CLI → client | `{ id, error }`                                                                    | Mutation failed.                                                                                                                                                                                                         |
| `state:accounts`            | CLI → topic  | `{ accounts: [...] }`                                                              | Full snapshot. Pushed on subscribe + after each write (debounced).                                                                                                                                                       |
| `state:tasks`               | CLI → topic  | `{ tasks: [...] }`                                                                 | Snapshot of `tasks.json`.                                                                                                                                                                                                |
| `state:cross-trade`         | CLI → topic  | `{ running, pid?, startedAt?, configCount, configs: [...] }`                       | Engine running-state snapshot fed by `~/.tealstreet/cross-trade-running.json` (daemon heartbeat) + the `cross-trade.json` registry. Stale heartbeats (>15s) report `running:false`. See [Cross-trade](./cross-trade.md). |
| `mutate:cross-trade-config` | client → CLI | `{ id, op: 'upsert' \| 'delete', config?, configId?, updatedAt? }`                 | LWW merge of a `StoredCrossTradeConfig` row from the web side. Same shape as `mutate:account` — incoming wins on strict `updatedAt` >.                                                                                   |

---

## Auth model

Two opaque identifiers cooperate to gate the WS.

| Identifier   | Lives where                                            | Used for                                                                                                              |
| ------------ | ------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------- |
| `clientId`   | Web app (browser-tab namespace UUID; max 64)           | Sent in `hello`. Looked up in `trusted-clients.json` for double-consent.                                              |
| `listenerId` | `~/.tealstreet/listener.json` (generated on first run) | Sent in `welcome`. Web pins it on first approval and surfaces a warning if a later `welcome` returns a different one. |

Layers on top:

- **Double-consent.** First connection from a new `clientId` always triggers
  a REPL prompt. The operator must approve before the socket completes. See
  [Trusted clients](./trusted-clients.md) for the persistence shape and
  revocation commands.
- **Origin allowlist.** Enforced before the handshake even starts.
- **JWT email.** The `welcome.user.email` is pulled from the standalone
  CLI's stored OAuth token. Display only — there is no incoming JWT and the
  email is not used as an auth credential.

---

## Kill switches

All three default to `true`. They live under `listener.*` in `config.json`
and are read fresh on every call — no memoization, no restart needed.

| Flag                   | What it gates                                          | Effect when `false`                                                         |
| ---------------------- | ------------------------------------------------------ | --------------------------------------------------------------------------- |
| `listener.enabled`     | Whether the loopback server binds at all.              | Server never starts. `POST /hook/<id>` returns **503**. WS upgrade refused. |
| `listener.accountSync` | `state:accounts` broadcasts + `mutate:account` accept. | Accounts topic stops emitting; `mutate:account` returns `mutate:err`.       |
| `listener.remoteExec`  | `cmd:exec` frames and `POST /hook/<id>` fires.         | `cmd:exec` returns an error frame; webhook fires return **423**.            |

To set them, edit `~/.tealstreet/config.json` directly:

```json
{
  "listener": {
    "enabled": true,
    "accountSync": true,
    "remoteExec": false
  }
}
```

---

## Files on disk

All under `~/.tealstreet/`. Override the directory with the
`TEALSTREET_CONFIG_DIR` env var if you need a non-default location.

| File                   | Contents                                                          | Sensitive?                        |
| ---------------------- | ----------------------------------------------------------------- | --------------------------------- |
| `listener.json`        | `{ listenerId, createdAt }`                                       | no                                |
| `trusted-clients.json` | `{ clients: [{ clientId, label, origin, approvedAt }] }`          | no                                |
| `hooks.json`           | `{ hooks: [{ id, name, secret, command, account?, createdAt }] }` | **yes — contains shared secrets** |

See [Config files](./config-files.md) for the full per-file map.

---

## Port and reconnect

- Default port `53219`, hardcoded.
- **75 s client-hint timer.** If nothing connects to the listener within
  75 s of CLI startup, the REPL prints a hint reminding you to refresh the
  web tab. The web app caps its reconnect backoff at 60 s, so 75 s is
  enough headroom to assume the web side has given up.
- **Topics**: `accounts`, `tasks`, `hook-output`. A client opts in
  per-topic after `welcome`.

## Web auto-connect gate

The web app only probes `ws://127.0.0.1:53219` if it has reason to
believe the CLI is running on this machine. Without that gate, every
web user (the vast majority of whom never run the CLI) would see
`WebSocket connection failed:` errors in their browser console on
every page load.

The CLI fires a single `POST /api/cli/checkin` on every startup with
its `process.platform`. The web mounts its loopback worker if
**either**:

1. It already has a `trustedListenerId` for this device (from a prior
   successful pairing).
2. The current web platform (`darwin`/`win32`/`linux`) has a fresh
   checkin (< 14 days) in `users_app_settings.settings.cli.checkins`.

Mobile browsers and the iPad desktop-mode UA are hard-skipped — they
can't reach loopback anyway.

**First-run experience**: launch the CLI once. On the next web page
load, the gate flips on and the pairing flow runs as documented in
[Pairing](./pairing.md). After that the trustedListenerId fast-path
takes over and the worker stays live across reloads.

For the end-to-end web ↔ standalone setup, see [Pairing](./pairing.md). For
the current webhook status and local hook primitives, see
[Hooks & webhooks](./hooks-and-webhooks.md).
