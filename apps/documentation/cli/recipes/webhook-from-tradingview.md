---
title: Webhook from TradingView
---

Run CLI commands from a TradingView alert. TradingView posts to your
Tealstreet webhook URL, and Tealstreet relays the alert to the listener you
designated — this CLI or a browser tab — where the alert message runs as CLI
commands.

## Setup

1. **Log in and start the REPL.**

   ```text
   tealstreet login
   tealstreet
   ```

   The REPL connects to the webhook relay on its own, but runs nothing until
   you choose it.

2. **Choose this CLI to receive your webhooks.**

   ```text
   webhook claim
   webhook          # confirm: "This CLI receives your webhooks."
   ```

3. **Copy your webhook URL** from Settings → Webhooks in the web app.

4. **Create the TradingView alert.** Paste the URL into **Webhook URL**, and
   put CLI commands in **Message**:

   ```text
   buy $250 SOLUSDT at 150
   ```

When the alert fires, the CLI prints the run as `[webhook] …` and records the
result in Settings → Webhooks → delivery log.

## Message examples

```text
close longs; buy $500 BTCUSDT
```

Chains run like they do at the prompt: `;` waits for the previous step, `&`
runs alongside it. The result reported to Settings is recorded once the whole
chain has finished.

To test a new alert without trading, run `simulation on` in the REPL before
it fires: write commands are logged and skipped. A webhook cannot turn
simulation off.

```text
my-entry
```

An [alias](../reference/aliases-and-vars.md#aliases). Aliases sync between the CLI and
your browsers on the same login, so one defined anywhere works on whichever
listener runs the alert.

TradingView placeholders work like any other text: `buy $100 {{ticker}}`.

## Gotchas

- **One receiver.** Exactly one listener runs your webhooks. If you also run
  the web app on other accounts, check `webhook` before relying on an alert.
- **Offline means expired, not rerouted.** If the claimed listener is offline
  longer than the hold window (60 s by default, Settings → Webhooks), the
  alert expires unrun.
- **The URL is the secret.** Anyone holding it can trade on the receiving
  listener's accounts. Rotate it in Settings → Webhooks if it leaks.
- **It trades on the REPL's current account.** A webhook cannot switch account
  or symbol, so name the symbol in each order (`buy $100 SOLUSDT`).
- **Some commands are refused** — anything that reconfigures the CLI or
  changes saved state (`login`, `account`, `set`, `alias`, `hook`,
  `simulation`, `fatfinger`, …). Full list:
  [Hooks & webhooks](../operations/hooks-and-webhooks.md#what-a-webhook-may-not-run).
- **Kill switch:** `listener.remoteExec: false` in `~/.tealstreet/config.json`
  stops this CLI receiving webhooks at all.

## Related

- [Operations → Hooks & webhooks](../operations/hooks-and-webhooks.md) — hosted webhooks, claims, local hooks
- [Workflow](../reference/workflow.md) — `;`, `&`, `retry`, `set ec` in alert messages
- [Aliases & vars](../reference/aliases-and-vars.md) — reusable alert bodies
