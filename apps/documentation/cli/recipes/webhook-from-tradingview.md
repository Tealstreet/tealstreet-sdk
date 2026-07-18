---
title: Webhook from TradingView
---

TradingView-to-CLI webhooks are not currently available as a hosted
Tealstreet feature.

The CLI has local listener and hook primitives, but Tealstreet does not
yet run the public relay needed to receive TradingView alerts and route
them into a user's local CLI session:

```text
TradingView -> Tealstreet public relay -> user's local CLI listener
```

Until that relay is live, do not rely on TradingView alerts to trigger CLI
trades through Tealstreet.

## What exists today

The standalone listener can register a fixed local hook command:

```text
hook create alert "chase sell %all% 20% reduce"
```

Output:

```text
Created hook 'alert' (<prefix>...). Fire with:
  POST http://127.0.0.1:53219/hook/<id>
  X-Hook-Token: <secret>
```

The command is locked to the hook definition — request bodies are ignored
— so a malformed alert payload cannot coerce the CLI into running a
different command. Each hook gets a 32-hex token sent as `X-Hook-Token`;
verification is constant-time after a length guard.

The missing piece is the public route from TradingView to the user's local
listener. Advanced users can experiment with a self-managed tunnel to
`127.0.0.1:53219`, but that is not the Tealstreet-hosted path and is not
covered by support guarantees.

## Variations

```text
hook create take-profit "close longs" --account mybybit
```

Hook with an account scope — the command runs against `mybybit` regardless
of the REPL's current focus.

```text
hook list
hook show <id-prefix>
hook rotate <id-prefix>
hook revoke <id-prefix>
hook revoke --all
```

Manage the registry. `show` reveals the secret again (useful when you lost
the initial printout). `rotate` mints a fresh secret — update TradingView
right after. Prefixes must be ≥ 8 chars (it errors loudly on ambiguity).

```text
# Test the fire path manually
curl -X POST -H "X-Hook-Token: <secret>" http://127.0.0.1:53219/hook/<id>
# → 200 { "ok": true, "runId": "...", "command": "...", "account": "..." }
```

Verify a hook locally before experimenting with any self-managed public
tunnel. The hosted Tealstreet relay is not live yet.

```text
hook create cancel-stops "set ec; cancel; close"
```

The hook body accepts the full CLI chain grammar — `;`, `&`, `set ec`,
`retry`, etc. Bodies are quoted with `"` or `'`, no escapes.

## Gotchas

- **The listener is loopback-only.** TradingView's servers can't reach
  `127.0.0.1:53219` directly, and the hosted Tealstreet relay for that
  route is not live yet.
- The token check **short-circuits on length mismatch** before
  `crypto.timingSafeEqual` — an unequal-length call would throw a
  `RangeError` and return HTTP 500 instead of a clean 401. Don't strip this
  guard if you're patching the verifier.
- Hook fires return **200 immediately** — an external sender treats it as
  success the moment the request lands, before the command runs. If the
  command errors out, you see it in `[hook:<name>]` output in the REPL and
  via the `hook-output` topic on connected web clients. There's no way to
  surface command failure back through the HTTP response.
- **Kill switches** in `~/.tealstreet/config.json`:
  - `listener.enabled: false` → 503 on all hooks (listener doesn't bind).
  - `listener.remoteExec: false` → 423 on all hooks (binds but rejects).
- The command stored in the hook is **fixed at create time**. To change it,
  revoke + recreate (or use `mutate:hook` from the web side, which does the
  same thing).
- The hook secret is stored in `~/.tealstreet/hooks.json` in plain text.
  That file is local-only but treat it like any credential — don't sync it
  in dotfile repos.
- One hook = one command = one optional account scope. For multi-step or
  branched behavior, put it all in an [alias](../reference/aliases-and-vars.md#aliases)
  and have the hook call the alias.

## Related

- [Operations → Hooks & webhooks](../operations/hooks-and-webhooks.md) — full fire shape + error codes
- [Operations → Listener](../operations/listener.md) — kill switches + wire frames
- [Operations → Trusted clients](../operations/trusted-clients.md) — separate auth model for web clients
- [Recipe: Listener from web](./listener-from-web.md) — same listener, WS entry point
- [Workflow](../reference/workflow.md) — `set ec` / `retry` / `;` for hook bodies
