# BloFin setting refusal attribution

Delivered leverage, margin-mode and position-mode refusals now name their setting
in the error text, label the HTTP status, and include the selected BloFin code
and message when supplied. A nested refusal uses its own message rather than
an outer `success`. The engine journal already retained that selected code and
message as `venueCode`/`venueMessage`; these fields, classification and outcome
remain authoritative. HTTP status alone never becomes a vendor code. A non-JSON
HTTP failure names the setting and status without echoing an arbitrary body.

The exact old `BloFin setting rejected: Bad Request` string was shared by all
three setters. In both current source and NX build 99c547d4 it came from a 2xx
reply with a failing BloFin envelope, not the separate non-2xx HTTP branch.
BloFin's [error table](https://docs.blofin.com/index.html#errors) documents body
code `400` under HTTP 200. The two reported e2e bad-key events have no captured
request/body, so neither the failing setter nor a particular invalid parameter
can be established retrospectively from that string alone.

Request audit against the [official setters](https://docs.blofin.com/index.html#set-leverage):

| Setting | Existing request | Finding |
| --- | --- | --- |
| Leverage | Native catalog `market.id` (`BTC-USDT`), leverage clamped to catalog limits, account margin mode; long then short only for isolated hedge mode, otherwise no positionSide | Matches documented scope. No demonstrated request-format defect; bodies unchanged. |
| Margin mode | Global `marginMode: cross/isolated`, no instrument or position side | Matches documented global setter. Open exposure/orders warning remains; the venue may refuse a blocked change. |
| Position mode | Global `positionMode: net_mode/long_short_mode`; no invented multiPosition | Matches documented setter. BloFin prohibits switching with open orders/positions. |

Hosted lifecycle code-read (also checked against the deployed 99c547d4 blobs):

- NX `ExchangeServerWorker.reconcileMissingAccounts` sends saved init and topic
  subscriptions; it does not synthesize or resend a settings command.
- `Session::init` resnapshots an identical fingerprint. A conflicting live copy
  causes init refusal before replacing the held account or signer. A previously
  joined copy keeps its original valid account/settings; a new unjoined copy
  gains no command authority from the refused init.
- `Session::msg` calls `forget_disposed` before dispatch; a missing joined
  account's callMethod returns NotSent. `send_command` uses that joined account's
  handle/client, and the native actor ignores an absent peer or returns its
  terminal error before executing. Neither stale client nor payload chooses a
  replacement account/credential.

This code-read found no demonstrated stale-session settings bypass. It does not
prove the reported event sequence's cause without request/body or session audit
evidence. No Hosted lifecycle, credentials, request parameters, retry rules or
write facts change. SDK/NX/server exact-text matchers should use typed fields and
the setting label rather than the former generic sentence. No deployment
ordering or deletion prerequisite is added.
