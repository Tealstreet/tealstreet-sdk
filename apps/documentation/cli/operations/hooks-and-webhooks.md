---
title: Hooks & webhooks
sidebar_position: 2
---

# Hooks & webhooks

There are two ways a webhook reaches the CLI:

- **Hosted webhooks** — TradingView (or any sender) posts to your
  Tealstreet webhook URL, and Tealstreet relays the alert to whichever
  listener you designated: this CLI, or a browser tab. **The alert message
  is CLI commands.** This is the supported path.
- **Local hooks** — a fixed command fired by `POST /hook/<id>` on the
  loopback listener. Only reachable from this machine.

---

## Hosted webhooks

### How it works

```text
TradingView alert --POST--> your Tealstreet webhook URL
                              (Settings -> Webhooks)
        |
        v
Tealstreet relay --> the ONE listener you claimed (this CLI or a web tab)
        |
        v
the alert message runs here as CLI commands
```

The CLI connects to the relay automatically when the REPL starts, as long as
you are logged in (`login`) and `listener.remoteExec` is not `false`. It runs
nothing until you choose it: no listener receives your webhooks until you run
`webhook claim` (or choose one in Settings → Webhooks). That is deliberate — a
TradingView URL you set up before this existed may still be posting.

### The alert message is CLI commands

Put the commands you want to run in the TradingView alert's **Message**
box. The full grammar works — `;` / `&` chaining, aliases, `@vars`,
`retry`, `sleep` and `set ec`. To try an alert without trading, turn
`simulation on` in the REPL first (a webhook cannot change it):

```text
buy $250 SOLUSDT at 150
close longs; buy $500 BTCUSDT
my-entry-alias
```

**Aliases resolve on the listener that runs the alert.** An alias you
defined in this CLI exists here; one defined in a browser does not.

Each alert runs in order behind anything you are typing, **against the
account the REPL is currently on**. Name the symbol in each order
(`buy $100 SOLUSDT`); a webhook cannot switch the account or symbol.

### Which listener runs it — the claim

Exactly one listener receives your webhooks. You can run the CLI and the web
app at the same time, possibly on different accounts, so Tealstreet never
picks for you after the first one:

```text
webhook            # who receives your webhooks, and every listener you have
webhook claim      # make THIS CLI the receiver
webhook release    # stop receiving here; alerts expire until something claims
```

You can also move the claim from **Settings → Webhooks** in the web app.
Whenever it moves, the CLI tells you.

If the claimed listener is offline, an alert waits (60 seconds by default,
set in Settings) and then expires **without running**. It is never sent to a
different listener.

### What a webhook may not run

A webhook message is text from whoever holds your webhook URL. It may trade;
it may not reconfigure the CLI. These are refused, and the refusal is
recorded as the alert's result:

`login`, `logout`, `auth`/`whoami`, `update`, `exit`, `quit`, `clients`,
`hook`, `webhook`, `logs`/`log`/`clearlogs`, `audit`, `footer`, `export`,
`import`, `script`/`scripts` (running `script:<name>` is allowed), the watch
modes, and these commands under any of their aliases: `account`, `accounts`,
`symbol`, `set`, `sets`, `unset`, `track`, `untrack`, `alias`, `unalias`,
`when`, `simulation`, `fatfinger`, `unfatfinger`, `budget`, `whitelist`,
`unwhitelist`, `history`, `wait`. Reading a var you set yourself
(`buy @size SOLUSDT`) works; changing one does not, and neither does a capture
(`@o = buy …`).

A webhook also may not hold the CLI for long, since it runs in line with what
you type: at most 50 steps, at most 60 seconds of `sleep` in total, and one
`repeat`. An alert that waits behind your own commands past its expiry is not
run.

These rules are for hosted webhooks only. Local hooks (`hook create`) run a
command you saved yourself and are not restricted.

### Delivery guarantees

- **At most once.** The CLI confirms with the relay before running an alert,
  and the relay confirms each alert to one listener exactly once. A dropped
  connection can cost an alert; it can never run one twice.
- **Results are recorded.** Each alert's output and done/failed status show
  in the delivery log under Settings → Webhooks.
- **Your URL is the secret.** Anyone with it can trade on the receiving
  listener's accounts. Rotate it from Settings → Webhooks if it leaks.

---

## Local hooks

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
hook create alert-long "buy $250 SOLUSDT at 150"
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
{"ok":true,"runId":"r_01HK…","command":"buy $250 SOLUSDT at 150"}
```

---

## TradingView (and other senders)

`127.0.0.1` is unreachable from the public internet, so external senders
cannot hit a local hook. Use [hosted webhooks](#hosted-webhooks) instead —
see [Webhook from TradingView](../recipes/webhook-from-tradingview.md).

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
