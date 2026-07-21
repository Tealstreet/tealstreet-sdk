---
title: Hooks & webhooks
sidebar_position: 2
---

# Hooks & webhooks

:::caution Hosted webhook relay is not live

TradingView-to-CLI webhooks are not currently available as a hosted
Tealstreet feature. The CLI has local listener and hook primitives, but
Tealstreet does not yet run the public relay that would receive a
TradingView alert and route it into a user's local CLI session.

The details below describe the local building blocks and are useful for
development or self-managed tunnels. They should not be treated as a
supported Tealstreet-hosted webhook path.

:::

A hook is a registered command body that fires over HTTP when the local
listener receives a `POST /hook/<id>` with a matching `X-Hook-Token`. The
listener is local to the machine running the standalone CLI, so external
senders need a route to that machine before the hook can fire.
Standalone-only — hooks live inside the listener subsystem, see
[Listener](./listener.md) for the underlying transport.

---

## REPL commands

| Command                           | Syntax                                              | Notes                                                                                  |
| --------------------------------- | --------------------------------------------------- | -------------------------------------------------------------------------------------- |
| `hook` / `hook list` / `hooks`    | bare                                                | List registered hooks. Secrets shown as `<hidden>` — use `hook show` to reveal.        |
| `hook create`                     | `hook create <name> "<command>" [--account <name>]` | Mint a new hook. Prints the fire URL + `X-Hook-Token`. Quotes: `"` or `'`, no escapes. |
| `hook show <id\|prefix>`          | `hook show <id\|prefix>`                            | Reveal the stored secret and re-print fire instructions.                               |
| `hook rotate <id\|prefix>`        | `hook rotate <id\|prefix>`                          | Mint a new secret in place. Old secret stops working immediately.                      |
| `hook revoke <id\|prefix\|--all>` | `hook revoke <id\|prefix\|--all>`                   | Delete one or all hooks. Prefix must be **≥ 8 chars**; ambiguous prefixes are refused. |

Quoting rule for the command body: use `"…"` or `'…'`. There is no escape
syntax inside the quotes — pick whichever quote your body doesn't contain.

```text
hook create alert-long "buy $250 SOLUSDT at -0.5%"
hook create flatten "nuke positions" --account mybybit
hook revoke 4f3a1c8e
hook rotate 4f3a1c8e
```

---

## Token model

| Field    | Shape                          | Purpose                                                         |
| -------- | ------------------------------ | --------------------------------------------------------------- |
| `id`     | 16 random bytes → 32 hex chars | Goes in the URL path: `/hook/<id>`. Non-secret.                 |
| `secret` | 16 random bytes → 32 hex chars | Sent as the `X-Hook-Token` HTTP header. Stored in `hooks.json`. |

Verification uses `crypto.timingSafeEqual`, but the implementation
**length-checks first**. The unequal-length call to `timingSafeEqual` would
throw `RangeError` and bubble up as HTTP 500; the length guard keeps the
wrong-token path returning a clean **401**. Don't remove the guard if you're
touching `verifyHookToken()`.

---

## Per-account scope

`hook create … --account <name>` pins the hook to a specific account. When
the hook fires it sets `accountOverride` on the run's AsyncLocalStorage, so
the command executes against that account regardless of what the REPL is
currently focused on.

Hooks created without `--account` execute against whatever account the REPL
has focused at fire time. That's fine for hooks you want to follow your
manual context, but explicit pinning is the safer default for unattended
fires.

The web app can create hooks via `mutate:hook` over the WS (see
[Listener → wire frames](./listener.md#wire-frames)) — same `account` field,
same semantics.

---

## HTTP fire shape

```text
POST http://127.0.0.1:53219/hook/<id>
X-Hook-Token: <32-hex secret>
```

The request body is **ignored** — the command is whatever was registered at
`hook create` time. Useful because most webhook senders (TradingView,
alerting tools) only let you customize URL and headers, not control which
command runs.

| Status | Body                                          | Meaning                                                                                        |
| ------ | --------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| `200`  | `{ ok: true, runId, command, account? }`      | Accepted. Command runs asynchronously; HTTP returns immediately.                               |
| `401`  | `{ ok: false, error: 'invalid token' }`       | Token missing, wrong length, or wrong value. Length-guarded before constant-time compare.      |
| `404`  | `{ ok: false, error: 'unknown hook' }`        | No hook with that `id`.                                                                        |
| `423`  | `{ ok: false, error: 'remoteExec disabled' }` | `listener.remoteExec` is `false`. See [Listener → kill switches](./listener.md#kill-switches). |
| `503`  | `{ ok: false, error: 'listener disabled' }`   | `listener.enabled` is `false` or the server isn't bound.                                       |

Fire-and-forget: the `200` returns the moment the hook is accepted. The
actual run streams `cmd:output` frames on the `hook-output` WS topic, then a
final `cmd:done`. Web clients subscribed to that topic see the run as if it
were a `cmd:exec`.

```text
$ curl -X POST http://127.0.0.1:53219/hook/4f3a1c8e... \
       -H "X-Hook-Token: 8b9d2e4a..."
{"ok":true,"runId":"r_01HK…","command":"buy $250 SOLUSDT at -0.5%"}
```

---

## TradingView (and other senders)

The supported Tealstreet-hosted route is not live yet:

```text
TradingView -> Tealstreet public relay -> user's local CLI listener
```

`127.0.0.1` is unreachable from the public internet, so TradingView (or any
external sender) cannot hit the standalone listener directly. Advanced
users can run their own public tunnel to `127.0.0.1:53219`, but that
tunnel, auth boundary, and operational risk are self-managed.

Current status: [Webhook from TradingView](../recipes/webhook-from-tradingview.md).

---

## Local mirror

Every `cmd:output` frame a hook produces is **also** printed to the local
REPL with a `[hook:<name>]` prefix in the matching chalk color. You see
exactly the same output a paired web client would see, without needing one
attached. Useful for debugging the command body without flipping back to
the web tab.

---

## Lifecycle

- Hooks persist to `~/.tealstreet/hooks.json` on every mutation. The file
  **contains shared secrets** — back it up the same way you'd back up any
  credential file.
- Hooks survive REPL restart. Revoking a hook is the only way to invalidate
  it; restarting the CLI does not rotate secrets.
- Hook fires are gated by `listener.remoteExec`. Toggling that flag to
  `false` is the fastest way to disable every hook at once without deleting
  them — the kill switch returns HTTP 423 and the hook records stay intact.

Related: [Listener](./listener.md) for the underlying server,
[Trusted clients](./trusted-clients.md) for WS-level access control,
[Pairing](./pairing.md) for the end-to-end web setup.
