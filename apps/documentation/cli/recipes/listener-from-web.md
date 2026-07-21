---
title: Listener — drive the standalone CLI from the web terminal
---

Run trading commands from the in-app web CLI but have them execute in the
standalone CLI process on your local machine. The standalone keeps the
account streams open, holds the chasers, runs the hooks; the web tab is
just a remote keyboard. Use this when you want the standalone to host the
session (persistent connections, hooks, simulation state) and the web tab
for the convenience of the browser UI.

## Minimum viable

In the standalone REPL — nothing to do. The listener binds
`127.0.0.1:53219` automatically at startup (provided `listener.enabled` is
not `false`).

In the web CLI tab, set the target dropdown to **`listener`**. The first
connection from this browser triggers a one-time approval prompt in the
standalone REPL. Approve and every subsequent command typed in the web
tab runs in the standalone process.

## Variations

```text
# In the standalone REPL — see what's connected
clients
```

Lists trusted browsers (one per `clientId`). Revoke via
`clients revoke <id-prefix>` (prefix ≥ 8 chars).

```text
# In the standalone REPL — confirm the listener is up
> # no command needed; the listener is implicit
> # check via /health from outside:
$ curl http://127.0.0.1:53219/health
{ "ok": true, "pid": ..., "port": 53219 }
```

```text
# In the standalone REPL — disable the bridge entirely
> # edit ~/.tealstreet/config.json:
{ "listener": { "enabled": false } }
```

Kills the loopback server. Web tabs targeting `listener` will fail with a
connect error.

```text
# In the standalone REPL — keep streams but reject commands
> # ~/.tealstreet/config.json:
{ "listener": { "remoteExec": false } }
```

State frames (`state:accounts`, `state:tasks`, `state:hooks`) continue;
inbound `cmd:exec` is rejected. Useful as a panic-button toggle.

```text
# In the standalone REPL — turn off account sync
> # ~/.tealstreet/config.json:
{ "listener": { "accountSync": false } }
```

Stops broadcasting/accepting account snapshots — web tabs no longer mirror
the standalone's account list.

## Gotchas

- **Listener is loopback-only.** The standalone binds `127.0.0.1:53219` —
  it's not reachable from the LAN. To run the listener on a different
  machine from the browser, you need a tunnel (Cloudflare, ssh -L, etc.).
- The 75-second **client-hint timer**: if no client connects within 75 s
  of listener startup, the standalone prints a one-line refresh hint. This
  is timed to be slightly above the web reconnect ceiling (60 s) so it
  doesn't fire on every soft-reload.
- Approval is **per browser tab namespace**, keyed by a clientId UUID
  stored in the browser. Wipe browser storage → approval prompt fires
  again on next connect.
- The first frame from the web client must be a `hello`. If you're poking
  at this from a script, see [Listener wire frames](../operations/listener.md)
  for the protocol.
- When you switch web targets back to `worker` (the default), the web tab
  goes back to running commands in its own safe-cex worker — the standalone
  keeps running but no longer sees the web's keystrokes. Streams it owned
  (chasers, TWAPs, hooks) keep going.
- Commands run via the listener use the **standalone's accounts**, not the
  web's. If your standalone has account `A` selected and your web tab
  thinks the active account is `B`, the `cmd:exec` frame can carry an
  `account` override; without it the standalone uses its own focus.
- Output streams as `cmd:output` frames — for `;` and `&` chains, late
  frames can arrive after `cmd:done`. Don't treat `cmd:done` as a hard
  end-of-output marker.

## Related

- [Operations → Listener](../operations/listener.md) — wire frames, kill switches
- [Operations → Pairing](../operations/pairing.md) — the approval UX
- [Operations → Trusted clients](../operations/trusted-clients.md) — revoke / list
- [Web CLI subset](../reference/web-cli.md) — what runs where
- [Recipe: Webhook from TradingView](./webhook-from-tradingview.md) — current hosted webhook status
