# HyroTrader v1 on the Bybit family

The `Hyrotrader` variant supports Hyro-issued, per-challenge Bybit-style HMAC
simulator keys and linear USDT perpetuals. OAuth/session tokens, mobile and a
server-held credential lifecycle are outside v1.

## Setup

Keep the simulator address in deployment/local environment configuration. Never
commit the deployment IP or credentials. The browser SDK reads these public
endpoint variables:

- `NEXT_PUBLIC_HYROTRADER_SIMULATOR_REST_URL`
- `NEXT_PUBLIC_HYROTRADER_SIMULATOR_PUBLIC_WS_URL`
- `NEXT_PUBLIC_HYROTRADER_SIMULATOR_PRIVATE_WS_URL`
- `NEXT_PUBLIC_HYROTRADER_PROD_REST_URL`
- `NEXT_PUBLIC_HYROTRADER_PROD_PUBLIC_WS_URL`
- `NEXT_PUBLIC_HYROTRADER_PROD_PRIVATE_WS_URL`

Public/private WS overrides are optional when the deployment uses the matching
`/v5/public/linear` and `/v5/private` paths on the REST authority; the SDK derives
those URLs. Ordinary browser accounts default production/challenge REST to
`https://sim-tealstreet.hyro.site`; `NEXT_PUBLIC_HYROTRADER_PROD_*` values override
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
- Protective-edit proof pages can contain ordinary orders without a trigger
  source. The edited/cancelled stop still requires its exact known `triggerBy`;
  missing/changed target evidence refuses, and peer lineage remains mandatory.
- Private reconnects require the existing correlated auth/subscription handshake
  and REST snapshots. A timeout/transport failure is not proof of a rejected
  order; retain the existing identity-based reconciliation before retrying.

## Bybit REST write pacing

Ordinary writes share the engine's mutation release domain across commands.
Single writes have a 200ms floor, restoring the legacy transport's five-request
budget. A plan using create-batch or cancel-batch also shares a 1200ms floor:
Bybit counts orders within a batch, so two commands cannot each release ten
orders in the same second. Smaller batches and single-order fallbacks in such
a plan conservatively use that same batch floor. Existing per-plan batch waits
remain; unrelated reads and history's separate pacing remain unchanged.

The floors advance only on actual engine Claim release, not queueing or a
response. Held work retains its original owner, credential, deadline and
release-time fences. No venue timer, shadow queue, extended deadline or
rate-limit resend is introduced. A validated native `10006` is a known
`rateLimited` / `rejected` refusal, including on the LOCAL Demo worker path.
The known-refusal exception first validates both nested result objects; duplicate,
mistyped or unknown fields remain unproven even when lossy ID extraction is empty.
Accepted batch IDs remain accepted when another item is rejected. Neither
a known rejection nor Unknown grants a resend. Contradictory rows/IDs, malformed
replies, ambiguous HTTP statuses, native `429`/`10018`, and protective edits that
may already have cancelled a stop retain Unknown and Open Orders guidance.
This per-engine budget cannot account for another app or engine using the same UID.

## Browser reachability and beta gates

Hyro is admitted in development and preview by the existing metadata. Production
availability and mobile remain disabled, and transport defaults are unchanged.
HTTPS pages cannot use the separate simulator's
plain HTTP/WS URLs directly. Prefer Hyro-provided HTTPS/WSS and the required CORS
and WS-origin configuration. A Tealstreet TLS relay is a separate, conditional
proposal, not enabled by this SDK change. TLS to a relay does not encrypt a plain
HTTP/WS relay-to-simulator hop.

The default challenge deployment uses HTTPS and REST-derived WSS URLs. Before
expanding beta, verify authenticated REST CORS and private-WS Origin acceptance
from each app origin with provisioned challenge keys. Overrides and the separate
demo deployment require their own browser-safe route. A credentialed beta smoke
pass must cover validation, snapshots, chart/orderbook hydration,
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

## Order-history terminal retention

The default `/v5/order/history` walk keeps the caller's full range and existing
seven-day paging for filled orders. The additional `Cancelled`, `Rejected` and
`Deactivated` passes clip the lower bound to the last 24 hours on the calibrated
protocol clock; an older range skips these passes. Explicit upper bounds remain
unchanged, including valid future end times. Existing seven-day splitting still
applies, and wholly-future slices are omitted. This matches the
[V5 order-history retention rules](https://bybit-exchange.github.io/docs/v5/order/order-list)
and prevents an unavailable terminal slice from refusing the default history.
Terminal bounds use the shared integer wire-timestamp conversion after clipping;
fractional calibration, default query bounds and scheduling clocks are unchanged.
Caller deadlines, read-key selection, pacing, cursors and partial-row markers
remain shared with the original walk. This does not recover terminal rows the
venue no longer retains.
