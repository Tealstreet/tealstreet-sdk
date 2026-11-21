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
  missing/changed target evidence fails the proof. A live or executed old stop
  ends the edit without a replacement, and other failures get one final orderId
  read before the replacement (exactly one stop; see `write-plan.md`).
- Private reconnects require the existing correlated auth/subscription handshake
  and REST snapshots. A timeout/transport failure is not proof of a rejected
  order; retain the existing identity-based reconciliation before retrying.

## Bybit REST write pacing

Ordinary writes share the engine's mutation release domain across commands.
Single writes have a 200ms floor, restoring the legacy transport's five-request
budget. A plan using create-batch or cancel-batch also shares a batch floor of
120ms per order in its largest batch, so a full ten-order batch keeps 1200ms:
Bybit counts orders within a batch, so two commands cannot each release ten
orders in the same second. Charging every batch the whole 1200ms paced a burst
of one-order placements (each a one-order create-batch) at one per 1.2s, which
users saw as DELEGATED capping their clicks (AGT-124); they now run at the 200ms
single-write floor. A floor is set by the batch just released, so small batches
followed at once by a full batch can briefly exceed a 10/s budget; Bybit then
refuses only the excess orders with native `10006`, a known rejection. Single-order
fallbacks in a batch plan share that plan's floor; an unrecognised batch leg is
charged the full 1200ms. Existing per-plan batch waits remain; unrelated reads
and history's separate pacing remain unchanged.

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

Create-batch may acknowledge a reduce-only peer resize alongside the new order.
Immediately before dispatch, the native plan snapshots bounded open, nonvirtual,
same-symbol/same-side reduce-only peer IDs, only for an entirely reduce-only batch.
Aligned response/status rows may subtract those known peers; requested echoes
match their targets, and a single remaining unnamed identity is unambiguous.
Multiple unnamed new identities, foreign/duplicate IDs, conflicting echoes,
misaligned arrays or a rejected peer remain Unknown without resending. The peer
is never a caller result or algo-owned order; its private stream owns its state.
Scope decoding shares the acknowledgement field codec, and duplicate checks
borrow identities before moving caller results. Membership checks copy no ID strings.
This follows Bybit's documented [reduce-only resizing](https://www.bybit.com/en/help-center/article/Reduce-Only-Order?category=5f2fb74e9c8b771130).

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

## Closed-history lifecycle evidence

Transaction-log rows with an execution reference retain optional native decimal
strings `positionAfter` and `executionQuantity` beside `tradeId`. They preserve
signed post-event position size and exact execution quantity. Missing, empty or
invalid evidence is omitted, never converted to flat. A source event whose
explicit cash flow and fee are zero retains a balance-neutral `other` record so
its flat-position evidence is not lost. Existing financial components, signs
and identities are unchanged.

The SDK joins these facts to executions, validates their quantities using decimal
arithmetic, and independently establishes the initial position size. This keeps
an add and partial close of a pre-window position from becoming a round trip.
Income history retains its existing read-only cursor, owner and pacing path.
The source contract is [Bybit's transaction log](https://bybit-exchange.github.io/docs/v5/account/transaction-log).

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
remain shared with the original walk.

A successful order-history result whose effective start precedes that 24-hour
boundary at completion (including time spent fetching pages) carries
`incompleteReason: "retentionLimit"`, including an empty result.
The diagnostic names the unfilled `Cancelled`/`Rejected`/`Deactivated` population;
older filled orders still return. Omitted starts use V5's seven-day default before
the supplied end (or now). Ranges still inside retention at completion retain
their usual completeness.
A failed later page preserves its more specific incomplete reason.
Frequent reads while an authenticated engine exists can collect those terminal
rows before expiry. This does not recover expired rows or provide durable capture
while no engine is running; deeper backfill cannot close that retention gap.


## Later-page history failure diagnostics

Closed-PnL and query history capture their closed native operation when binding
an owned read. If a later page fails after accepted rows, the original result
remains incomplete with reason `laterPageFailed`. Its existing command-attributed
marker also carries `historyOperation`, `historyFailureKind`, and an optional
allowlisted `historyVenueCode`; unlisted native codes become `other`. Messages,
URLs, cursor tokens, credentials and row values are not diagnostic metadata.
The non-default engine feature `history-page-diagnostics` is enabled by Bybit;
other families do not opt into these additional native fields.

The browser host and SDK completeness helpers retain those facts as optional
`failure: { operation, kind, venueCode? }` beside the positive incomplete marker,
through mapping and worker/hosted JSON. This distinguishes a closed-PnL failure
from an auxiliary order-history failure without treating a retained prefix as
complete coverage. An earlier missing-coverage reason remains authoritative;
first-page failures and serialization failures still reject. These diagnostics
do not change requests, deadlines, retry policy, pacing or writes.
## HyroTrader auth refusal during admission

A current, correlated `auth` acknowledgement with explicit `success:false` and
native `10003` reports a generic typed `rejected` event. Hosted admission can
refuse that attempt promptly without guessing a key/IP cause, disabling the
credential, changing resident ownership, or authorizing any write. The numeric
code is retained without interpreting its cause; raw reply text is not copied.
Missing/null success, other codes, subscription responses, stale request IDs and
transport/handshake timeouts retain their existing uncertainty and retry behavior.

## Execution-history fees

Historical fills preserve the signed `execFee`: positive is a cost and negative
is a maker rebate. The transaction-log income row uses the opposite cash-flow
sign. Missing, empty or invalid execution fees stay absent; an explicitly
reported zero remains zero. Timestamps, pagination and order identity are unchanged.

## Stop-entry brackets (AGT-115)

A trailing stop, and a stop/take-profit that closes a covering position, go to
`/v5/position/trading-stop` with only their own price, so attached legs were dropped (both
live, 2026-11-21). `plan_place` (and `hyro_stop`) now refuse those routes when the order
carries a non-zero leg, failing the whole command before any write with the engine's shared
`stop_entry_legs_refusal` copy (`HyroTrader` names Hyro). The check sits at the routing point,
so it sees the same position the route does. A stop with no covering position goes through
order create and keeps its legs (live: a `__stop_loss` row came back), as does `conditional`.
