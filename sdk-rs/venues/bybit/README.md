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
those URLs. All three final URLs must be present and valid before native startup.
The `NEXT_PUBLIC_*` origins are browser-visible; real challenge keys and secrets
come from the user's account, never a production build-time credential variable.
The shared demo account's disposable credentials belong only to its separate
web-owned configuration.

`extra.hyrotraderMode: 'demo'` selects the simulator map. The production mode
selects configured production origins, retaining the existing simulator fallback
when the production REST livenet URL is absent. The registry resolves that map
and testnet/livenet choice before construction. Neither mode falls back to Bybit
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
  absent/empty. Other supplied financial numbers retain their validation.
- A native `110001` cancel answer satisfies the cancellation intent without a
  resend. Existing fresh-read, fill-retention and unknown-outcome protections
  remain authoritative; it does not invent a cancellation or fill event.
- Private reconnects require the existing correlated auth/subscription handshake
  and REST snapshots. A timeout/transport failure is not proof of a rejected
  order; retain the existing identity-based reconciliation before retrying.

## Browser reachability and beta gates

Hyro remains development-only. Preview/production enablement, mobile gates and
transport defaults are unchanged. HTTPS pages cannot use the separate simulator's
plain HTTP/WS URLs directly. Prefer Hyro-provided HTTPS/WSS and the required CORS
and WS-origin configuration. A Tealstreet TLS relay is a separate, conditional
proposal, not enabled by this SDK change. TLS to a relay does not encrypt a plain
HTTP/WS relay-to-simulator hop.

Hyro must confirm the exact REST/public-WS/private-WS deployment endpoints, TLS
plan, allowed Tealstreet origins and challenge credentials. A credentialed beta
smoke pass must cover validation, snapshots, chart/orderbook hydration,
market/limit/conditional orders, chase/cancel-replace races, close, leverage,
TP/SL/trailing and reconnect. Synthetic tests do not certify that live contract.
Account UX, Requests Log diagnostics and the beta user document belong to the NX
web workstream. No preview/production promotion should happen before reachability
and that smoke pass are approved.
