# HyroTrader v1 on the Bybit family

The `Hyrotrader` variant supports Hyro-issued, per-challenge Bybit-style HMAC
simulator keys and linear USDT perpetuals. OAuth/session tokens, mobile and a
server-held credential lifecycle are outside v1.

## Setup

Keep the simulator address in deployment/local environment configuration. Never
commit the deployment IP or credentials. The browser SDK reads these public
**origin-only** variables:

- `NEXT_PUBLIC_HYROTRADER_SIMULATOR_REST_URL`
- `NEXT_PUBLIC_HYROTRADER_SIMULATOR_PUBLIC_WS_URL`
- `NEXT_PUBLIC_HYROTRADER_SIMULATOR_PRIVATE_WS_URL`
- `NEXT_PUBLIC_HYROTRADER_PROD_REST_URL`
- `NEXT_PUBLIC_HYROTRADER_PROD_PUBLIC_WS_URL`
- `NEXT_PUBLIC_HYROTRADER_PROD_PRIVATE_WS_URL`

Public/private WS overrides are optional when the deployment uses the matching
`/v5/public/linear` and `/v5/private` paths on the REST authority; the SDK derives
those URLs. Ordinary browser accounts default production/challenge REST to
`https://sim-bybit.hyro.trade`; `NEXT_PUBLIC_HYROTRADER_PROD_*` values override
that default. Explicit demo accounts still need their separate simulator map.
All three final URLs must be present and valid before native startup.
The `NEXT_PUBLIC_*` origins are browser-visible; real challenge keys and secrets
come from the user's account, never a production build-time credential variable.
The shared demo account's disposable credentials belong only to its separate
web-owned configuration.

`extra.hyrotraderMode: 'demo'` selects the simulator map. Explicit `production`
selects only production origins on the requested network; a missing selected
endpoint refuses with typed `invalidInput` / `notSent` before I/O. Configured
production testnet does not require livenet URLs. Omitted mode defaults to
production; only `demo` selects simulator. Neither mode falls back to Bybit
live, testnet or demo hosts. Native callers can instead supply `Options.endpoints`
with `Variant::Hyrotrader` through the normal signing-provider setup.

## Supported contract and limitations

- Cross margin and one-way positions only. Isolated/hedged mode changes return
  typed `unsupported` / `notSent` errors before any network write; selecting the
  already-supported modes is a local success.
- Signed writes use exact JSON bytes and `Content-Type: application/json`.
  Conditional `triggerDirection` is a JSON number. Bybit variants retain their
  existing wire representation.
- Public subscribe/unsubscribe frames contain at most 10 topics per request.
  This is a request batching limit, not certification of unlimited topics on one
  physical simulator connection; confirm the deployed simulator's connection cap
  with Hyro before a large beta workload.
- Flat position clears can omit side/leverage. Live positions still require a
  valid side and positive leverage, and accept `entryPrice` when `avgPrice` is
  absent/empty. Flat rows with missing, empty or nonpositive leverage retain the
  known position/setting leverage, defaulting to 1 when unknown, so unchanged
  leverage requests remain local. Supplied non-finite numbers still refuse.
- A native `110001` cancel answer satisfies the cancellation intent without a
  resend. Existing fresh-read, fill-retention and unknown-outcome protections
  remain authoritative; it does not invent a cancellation or fill event.
- Private reconnects require the existing correlated auth/subscription handshake
  and REST snapshots. A timeout/transport failure is not proof of a rejected
  order; retain the existing identity-based reconciliation before retrying.

## Browser reachability and beta gates

Hyro is admitted in development and preview by the existing metadata. Production
availability and mobile remain disabled, and transport defaults are unchanged.
HTTPS pages cannot use the separate simulator's
plain HTTP/WS URLs directly. Prefer Hyro-provided HTTPS/WSS and the required CORS
and WS-origin configuration. A Tealstreet TLS relay is a separate, conditional
proposal, not enabled by this SDK change. TLS to a relay does not encrypt a plain
HTTP/WS relay-to-simulator hop.

Hyro must confirm the exact REST/public-WS/private-WS deployment endpoints, TLS
plan, allowed Tealstreet origins and challenge credentials. A credentialed beta
smoke pass must cover validation, snapshots, chart/orderbook hydration,
market/limit/conditional orders, chase/cancel-replace races, close, leverage,
TP/SL/trailing and reconnect. Synthetic tests do not certify that live contract.
The SDK emits opt-in `ws-diagnostic` events for Hyro and every Rust venue:
`{ venue, channel, direction, message }`, with sanitized text for public/private/
business sockets. Enable `setWsDiagnosticsEnabled(true)` in the exchange's realm
or send the existing `__workerConfig.wsDiagnosticsEnabled` worker setting. The
default is off. Fields and values are both allowlisted: only closed protocol/
status/error enums, bounded integers and success booleans survive. Free-form
IDs, topics, scalar data, messages and nested echoes become fixed placeholders.
Connection URLs, headers and binary contents are omitted. `WsDiagnosticEvent` is exported from
the SDK, and the worker posts `{ op: 'ws-diagnostic', data: [event] }`.

Account UX, consuming that event in the web Requests Log, and the beta user
document belong to the NX web workstream. Production promotion requires separate
approval after reachability and that smoke pass are accepted.
