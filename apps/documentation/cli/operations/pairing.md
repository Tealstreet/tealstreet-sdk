---
title: Pairing web and standalone
sidebar_position: 4
---

# Pairing web and standalone

Pairing is the act of pointing the web CLI tab at a running standalone CLI
so commands typed in the browser execute inside the standalone process.
Under the hood it's the [listener](./listener.md) WS transport plus the
[trusted-clients](./trusted-clients.md) allowlist — this page is the
end-to-end walkthrough that ties them together.

---

## When you'd pair

| Goal                                                         | Why pairing helps                                                                                                           |
| ------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------- |
| Keep account WebSockets streaming on a dedicated machine     | Standalone is always-on; browser tabs can suspend, refresh, or close without dropping exchange connections.                 |
| Leave hooks alive across browser sessions                    | Hooks live inside the standalone listener (see [Hooks & webhooks](./hooks-and-webhooks.md)). The web app doesn't host them. |
| Survive browser backgrounding / tab suspension               | Backgrounded tabs throttle timers and may close sockets. A paired standalone keeps running regardless of what the tab does. |
| Quick-fire trades from a phone but execute from a real shell | The phone web tab gets the touch UI; the desktop standalone gets the real terminal, full keyboard, and proper logs.         |
| Run the recorder, watch modes, or chart panels               | All of those are standalone-only (see [Web CLI vs standalone](../reference/web-cli.md)).                                    |

---

## End-to-end setup

### From the standalone side

1. **Launch the CLI.** `tealstreet` (or `tealstreet -a <account>`). The
   listener auto-binds on `127.0.0.1:53219` on every start.
2. **Stay at the REPL.** The first connection from a new web client pauses
   here for your approval — you have to be looking at the REPL to grant it.

That's it from the standalone side. After the first approval, the web
client's `clientId` is in `~/.tealstreet/trusted-clients.json` and future
reconnects pass through silently.

### From the web side

1. Open the web CLI tab.
2. Set the **target** to `listener` in the tab's settings.
3. The web app opens `ws://127.0.0.1:53219/ws` and sends its `hello` frame.
4. Switch to the standalone, **approve** the prompt.
5. Web CLI is now wired through the standalone — every command you type in
   the browser ships as a `cmd:exec` frame and runs inside the standalone
   process.

For the per-tab settings UI and reconnect behaviour, see
[Web CLI vs standalone](../reference/web-cli.md). For the recipe-style
walkthrough see [Listener from web](../recipes/listener-from-web.md).

---

## What flows where

Once welcomed, the WS carries five conversation types. The full frame
catalogue is in [Listener → wire frames](./listener.md#wire-frames); the
flow at runtime looks like:

| You do…                                | Frames on the wire                                                                                                                                                         |
| -------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Type a command in the web CLI          | Web sends `cmd:exec { id, command, account?, symbol? }`. Standalone dispatches as if typed at its own REPL.                                                                |
| Standalone prints output for that run  | Each line streams back as `cmd:output { id, kind, text }` (`kind ∈ print \| success \| warn \| error \| info`).                                                            |
| Standalone finishes the run            | `cmd:done { id, success, error? }`. Late frames from `;` / `&` chains may still arrive — match by `id`.                                                                    |
| Web edits an account                   | Web sends `mutate:account { id, op, account?, name? }`; standalone replies `mutate:ack` or `mutate:err`. Account snapshot reflows via `state:accounts`.                    |
| A webhook fires while you're paired    | Standalone publishes `cmd:output` + `cmd:done` on the `hook-output` topic. Same frame shape — paired clients see the run in real time.                                     |

`state:accounts` and `state:tasks` are pushed to subscribed clients on
connect and after every mutation. Hooks are managed from the REPL only
(`hook create`), so their secrets never cross this socket.

---

## Failure modes

| Symptom from the web side                      | Likely cause                                                                                      | Fix                                                                                                                    |
| ---------------------------------------------- | ------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| Socket closes with code **4001**               | `clientId` was revoked, or you denied the approval prompt.                                        | At the standalone REPL: `clients` to inspect, re-approve on next attempt. See [Trusted clients](./trusted-clients.md). |
| `cmd:exec` returns an error frame              | `listener.remoteExec` is `false`.                                                                 | Edit `~/.tealstreet/config.json` → `listener.remoteExec = true`. Read fresh on every call, no restart needed.          |
| Webhook fires return HTTP **423**              | Same kill switch — `listener.remoteExec` gates HTTP too.                                          | Same fix. See [Hooks & webhooks](./hooks-and-webhooks.md#http-fire-shape) for the status table.                        |
| WS upgrade refused / HTTP **503**              | `listener.enabled` is `false`, or the standalone isn't running.                                   | Start the CLI; if it's already running, flip `listener.enabled` back to `true` in `config.json`.                       |
| Web prints "different CLI" warning             | The standalone's `listenerId` changed — different machine or wiped `~/.tealstreet/listener.json`. | Approve the new pairing, or restore the old `listener.json` if you wiped it by accident.                               |
| Web stops reconnecting after a long disconnect | Web caps backoff at 60 s. Standalone prints a refresh hint after 75 s of no clients.              | Refresh the web tab.                                                                                                   |

For the listener server itself (HTTP routes, kill switches, file paths),
see [Listener](./listener.md). For revoking trust after pairing, see
[Trusted clients](./trusted-clients.md). For the HTTP webhook variant of
`cmd:exec`, see [Hooks & webhooks](./hooks-and-webhooks.md).
