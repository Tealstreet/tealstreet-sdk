# Coinbase Rust integration

Both `coinbase` and saved-account `coinbasespot` load `RustEngineExchange` from
both SDK registries. The engine implementation is `sdk-rs/venues/coinbase/`;
`docs/plans/RUST_COINBASE.md` records the scope, protocol evidence and gates.
The frozen Advanced Trade TypeScript adapter lives only in
`src/__tests__/reference/coinbase/` for independent replay and old regression tests.

## Runtime contract

The authenticated `coinbase` profile discovers its permitted INTX perpetual,
CFM dated-futures plus spot, or spot portfolio. `coinbasespot` restricts discovery
to spot and never probes futures account endpoints. Keyless `coinbasespot`
public data uses Advanced Trade spot products; the main public profile retains
INTX instruments. The legacy `testnet` option selects the same live hosts as
TypeScript; it does not provide sandbox isolation.

`sign.rs` imports CDP ES256 P-256 PEM or Ed25519 seed/keypair credentials and
bounded downloaded-key JSON natively. Every REST dispatch signs a fresh JWT
for the real venue method/host/path; query strings are excluded. Every private
user/heartbeat subscription signs a distinct JWT without a URI. Credential
presence selects private startup; native validation rejects incomplete supplied
credentials. Explicit `publicDataOnly` overrides credential presence.
The host owns proxy routing and the `Authorization` to `Coinbase-Authorization`
alias required by the non-HOW proxy; venue code keeps standard authorization.

Every socket enforces connection-wide sequence numbers across all channels,
validated heartbeat liveness and generation isolation. Books require an initial
snapshot and recover gaps. Daily candles seed the full current day and replace
cumulative native five-minute buckets. REST lists stream through typed rows.
Failed or partially invalid snapshots preserve the previous account state;
user cumulative fills emit only positive increments at the incremental price.

Positions and store order amounts use base units. CFM placement and query sizes
remain native contract counts; contract size converts store sizes and quote
notionals. Spot leverage is one; derivatives retain the documented account
leverage and fixed cross/one-way model. Unsupported account mutations fail.
Market IOC, GTC/PostOnly limits and last-price stop-limit orders are supported.
Reduce/close requests require explicit size and observed opposite-position
capacity, reserved across queued and in-flight writes. This client bound cannot
prevent concurrent external account writes; no native reduce-only flag exists.
Caller IDs are nonempty strings of at most 128 characters without whitespace
or control characters, preserved verbatim for one plain Limit/Market order.
Spot edits require exact current zero-fill LIMIT decimal provenance. INTX
replacement requires authoritative zero-fill cancellation before a new client ID
is placed. Cancel acknowledgements mean initiation, and per-ID failures remain
visible. Writes are never automatically retried.

`getAccount()` returns the selected authenticated portfolio UUID through the
engine's account identity API. Typed fills, terminal order history and public
trades use bounded documented reads. Income and exact client-ID lookup fail
explicitly because Advanced Trade provides no corresponding reliable API.

## Preserved Starbase contracts

The `coinbase.starbase*.ts` modules below remain inert TypeScript contracts.
They are not imported by the Rust adapter, registries, routing or metadata;
this port does not activate the future Starbase migration. Their schema and
live-evidence tests remain in `src/__tests__/audit-regressions/`.

## INTX gateway migration status

The current official changelog retires the Advanced Trade `/intx/*` API paths
on October 1, 2026. This port preserves their legacy TypeScript compatibility
contract with deterministic fixtures; no live authenticated INTX support is
claimed. The Starbase modules remain inactive. The earlier migration plan below
records its historical schedule and unresolved schema gates.

Coinbase documents a hard September 9, 2026 INTX cutover (schedule subject to
change) with no parallel-running window. The published Starbase contract uses
JSON-RPC 2.0 at `https://drb.coinbase.com/api/v2` and
`wss://drb.coinbase.com/ws/api/v2`; Coinbase Advanced clients keep their CDP
keys and exchange a fresh signed JWT through `public/auth` with
`grant_type: coinbase_cdp`. Direct institutional Deribit accounts use a
different newly provisioned Deribit-key flow and must not be conflated with
this adapter. The implementation plan and cutover gates are tracked in
`docs/safe_cex_audit_july/exchanges/coinbase-starbase-migration.md`.

The first follow-on foundation is implemented in `coinbase.starbase.ts`. It
contains the published HTTP/WS endpoint constants, typed JSON-RPC request and
response handling, monotonic request IDs, an injected HTTP POST transport that
validates exact public/private method grammar before appending `/{method}` to
the HTTP base URL, CDP token exchange/refresh policy,
and unified WebSocket heartbeat primitives. Sanitized live-public and
official-schema fixtures and tests are under
`src/__tests__/audit-regressions/coinbase.starbase-*`.

The second follow-on is implemented in `coinbase.starbase-public.ts`. It is an
inert markets/public-data layer bounded by Coinbase's live public gateway and
published Starbase schemas: exact BTC/ETH linear-crypto-USDC-perpetual instrument
normalization with inverse/fiat/option rejection,
legacy `{BASE}-PERP-INTX` saved-symbol compatibility, ticker/book/candle
mappers, exact public request/channel builders, desired-subscription replay,
and order-book gap/reconnect recovery. Committed sanitized 2026-07-15 public
instrument/book/ticker/trade/WS evidence has exact byte hashes locked in tests.
Public trades corroborate `amount = contracts * contract_size` for BTC and ETH, book
and ticker sizes share that base-amount boundary, and the schema defines open
interest for the captured linear futures in underlying base coin. Mappers
therefore preserve these values directly as `bidBaseAmount`, `askBaseAmount`,
and `openInterestBase` while retaining the upstream schema contradiction in the
migration doc.
Raw-book `new`, `change`, and `delete` actions mean add, update, and remove a
price level. Contradictions are validated before a frame is applied; they clear
the book and require a new snapshot instead of silently becoming upserts.

The third follow-on is implemented in `coinbase.starbase-private.ts`. It is an
inert, read-only private-state layer with exact USDC/future request builders for
account summary, positions, open orders, ID-addressed order state,
currency/instrument order history, and user trades. Order-history requests
deliberately omit `with_continuation` and `continuation`: the current official
response schema publishes only an `order[]` result even though the optional
request parameters describe a different continuation object. Strict gateway
DTOs preserve legacy INTX symbol identity plus raw order/trade IDs, state,
amount, filled amount, and contract-unit fields. Account balance, equity,
available funds, withdrawal funds, initial margin, and maintenance margin are
explicitly named as USDC because the requests select `currency: USDC`. Position
`size_currency` is explicitly base coin and `size` remains explicitly the
gateway's published USD/quote size. Gateway margin and PnL fields stay
gateway-labelled instead of being silently asserted as safe-cex quote values.

Coinbase's migration guide says Starbase `amount` is base coin, but inherited
OpenAPI descriptions still say perpetual `amount` and `filled_amount` can be
USD. The private DTO therefore remains decodable for partial orders and trades,
while the safe-cex order mapper only emits an unfilled order and returns a
typed parity blocker for non-zero fills; historical-trade normalization stays
blocked entirely. The OpenAPI also describes `matching_id` as always null while
typing it as a string, so the decoder follows the published response example
and fails closed on non-null values. These upstream contradictions must be
resolved by a Coinbase schema correction or approved account capture before
write-path or reconciliation cutover.

The fourth follow-on is implemented in `coinbase.starbase-orders.ts`. It is an
inert, transport-free order-write contract layer for the published
`private/buy`, `private/sell`, `private/edit`, `private/cancel`, and
`private/close_position` methods. Exact builders use lower-case gateway order
and time-in-force values and preserve `{BASE}-PERP-INTX` as the saved symbol
while sending `{BASE}_USDC-PERPETUAL`. The technical migration guide takes
precedence for new request construction and explicitly defines `amount` as
base coin; every place-order builder therefore requires a typed total-base
amount whose source is that guide. The inherited OpenAPI parameter prose still
calls perpetual `amount` USD, so contract-unit conversion and all size edits
remain typed blockers instead of inferring total-versus-remaining behavior for
partially filled orders.

The guide's OTOCO example also publishes a `take_limit` child with `price` and
`trigger` but no `trigger_price`; that more specific migration example takes
precedence over the inherited generic trigger prose for this exact child shape.
Stop children and `take_market` still require the OpenAPI `trigger_price`.
Top-level `take_limit` follows the generic OpenAPI request contract and still
requires `trigger_price`; the omission is scoped only to that specific OTOCO
child example.

The request enum admits `market_limit`, but the published response `order_type`
enum does not. The method parameter `valid_until` is an integer timestamp, but
Coinbase publishes neither its unit nor a range. Both remain typed write gates;
the layer does not claim a decodable market-limit lifecycle or emit a guessed
milliseconds/seconds deadline.

Starbase `order_id` is the only canonical write identity. Labels are decoded as
non-canonical display/correlation metadata and the contract layer intentionally
does not expose `edit_by_label` or `cancel_by_label`. A triggered stop-limit's
published `trigger_order_id` is modeled as its pre-trigger predecessor;
`oto_order_ids` are the primary OTOCO order's child IDs; and
`primary_order_id` links a child back to its primary. No missing lifecycle IDs
are manufactured. Filled write responses and returned trades remain parity
blocked until Coinbase resolves the response amount-unit conflict. This module
is not imported by the active exchange, WebSockets, API factory, routing, or
metadata.

The fifth follow-on is implemented in `coinbase.starbase-session.ts`. It is an
inert, transport-free Cancel on Disconnect contract layer for
`private/enable_cancel_on_disconnect`,
`private/disable_cancel_on_disconnect`, and
`private/get_cancel_on_disconnect`. The only published optional parameter is
`scope: connection | account`; omission preserves the gateway's default
connection scope. Enable and disable are explicitly recorded as WebSocket-only
and accept only the exact `"ok"` result. The getter strictly accepts the exact
`{ enabled: boolean, scope }` result object. No active runtime imports this
module, and it does not enable Cancel on Disconnect by itself.

The sixth follow-on is implemented in `coinbase.starbase-ws-session.ts`. It is
an inert lifecycle controller for a future unified WebSocket. A connection is
not ready for subscription replay until it has used a fresh connection-local
`public/auth`, enabled the published heartbeat, and—when an explicit Cancel on
Disconnect policy is supplied—mutated and read back that exact policy. It also
drives the existing 49-minute WebSocket re-auth policy and answers only an
already-classified heartbeat signal. A classified rate-limit disconnect enters
an explicit backoff gate; a future credit-aware transport must acknowledge its
own backoff decision before activation can run again, because Coinbase does not
publish a close code or universal retry delay that this layer could safely
guess.

This controller owns no socket, timer, reconnect loop, subscription method, or
raw-frame parser. Subscription replay is an injected callback so it does not
choose between the conflicting private-subscription examples/contracts in the
current schema. No active runtime imports the controller.

The seventh follow-on is implemented in `coinbase.starbase-cutover.ts`. It is
an inert, data-only cutover assessment. Its enforcement-facing selector accepts
no arguments: callers cannot inject evidence or claim that an account is INTX.
It consumes only repository-owned evidence and remains explicitly product-unbound
until a future integration derives INTX identity from a trusted runtime
portfolio. Therefore spot, CFM, and unclassified traffic all remain on the
existing Coinbase paths; public reads, private reads, and writes stay legacy.

The only synthetic evaluator is named `ForTestsOnly`. It can describe an atomic
HTTP-only comparison plan for a test-classified INTX portfolio after the public
and private HTTP response contracts, HTTP authentication contract, and
credentialed HTTP authentication evidence are ready. That plan consumes only
JSON-RPC method results and always disables writes. It contains no WebSocket
session, subscriptions, or private notifications.

WebSocket session activation and the unpublished private notification envelope
are represented by separate, closed gates. Deterministic repository facts use
`implemented | not-implemented`; credentialed/live facts use
`verified | unverified`, so implemented code cannot be mistaken for live proof.
The HTTP-shadow, WebSocket, and authoritative runtime activation flags are all
literal `false`. Unit parity, credentialed HTTP read parity, credentialed
WebSocket lifecycle, the notification envelope, write canary, and cutover
runbook remain unresolved. Enabling anything requires trusted portfolio binding
and a reviewed code change, not caller input or configuration. The assessment
has no endpoint, transport, store, registry, metadata, or routing import.

The eighth follow-on is implemented in
`coinbase.starbase-public-frames.ts`. It is an inert public-only classifier for
independently captured live envelopes from 2026-07-15. A subscription frame is
accepted only as exact JSON-RPC `method: subscription`, without `id`/`result`,
when `params.channel` exactly matches a desired, acknowledged public channel;
the captured `params` boundary is passed to the existing `{ data }` decoder.
Heartbeat frames accept only `params.type: heartbeat | test_request`, without
an ID. A `public/test` response requires a known positive pending request ID
and exactly one valid `result` or `error`. Its only optional top-level keys are
the proven gateway metadata `usIn`, `usOut`, `usDiff`, and `testnet`; timestamps
must be positive safe integers, the difference non-negative, and jointly
present timing values must agree. Every other own key is rejected.
Canonical error codes must be safe integers; integer-looking values outside
JavaScript's exact range fail closed.

Before `JSON.parse`, a bounded JSON grammar scan rejects duplicate object keys
at every depth after decoding escapes, including spellings such as `id` and
`\u0069d`. This prevents last-key-wins parsing from bypassing exact envelope or
result/error checks while preserving arrays and escaped string contents.

The classifier does not infer private channels from public evidence. Every
non-acknowledged subscription, including private-looking names, remains
explicitly unsupported and unclassified. The published AsyncAPI still omits
the outer envelope, so the source capture hashes are pinned in code and the
migration doc. The classifier owns no socket, dispatch, subscription state,
store, routing, or active import, and its activation constant is literal
`false`.

`private/get_user_trades_by_order` is also deferred: its published response
example contains a trade array, but `PrivateGetUserTradesByOrderResponse`
defines no `result` property. The inert reader uses the coherent
`private/get_user_trades_by_currency*` response family until Coinbase publishes
an exact by-order result schema.

All eight modules are intentionally inert: no current exchange, WebSocket, API
factory, metadata, or shared-routing file imports them. Do not wire them into
the active adapter until read-only private state passes source-of-truth parity
and all normalization blockers are resolved. The public mapper intentionally
supports only BTC/ETH linear USDC perpetuals; discovery requests USDC futures
and filters those exact names before strict decoding because the live result
includes additional perpetuals and dated futures. Expanded product support,
an exact amount increment, and maximum base amount remain gated; live
`contract_size` and `min_trade_amount` are positive decimals but neither is
treated as an unpublished amount step. The inert market-parity assessment
returns typed blockers for both missing market fields instead of manufacturing
a safe-cex `Market`. Coinbase has not published
the inbound notification envelope in its Starbase OpenAPI or AsyncAPI schema.
The inert public classifier is therefore bounded to the pinned 2026-07-15 live
public captures; the controller still accepts an already-classified signal
because the two modules are not wired together. Public stream mappers start at
the AsyncAPI channel payload's published `{ data }` shape, reached through the
captured notification `params` boundary.

The private read layer currently consumes only JSON-RPC method results.
Although AsyncAPI lists private portfolio/order/position channels, Coinbase has
not published the unified socket's outer inbound notification envelope.
Authenticated notification classification and live snapshot/delta
reconciliation therefore remain deferred; do not infer Deribit envelopes from
channel names. The pure read-state holder validates a complete result before
replacement, so malformed or duplicate REST snapshots preserve the
last-known-good account, positions, open orders, ID-addressed order state,
order history, or trade page. Order-state results must echo the requested
canonical `order_id`, and instrument-history rows must all match the requested
Starbase instrument. Account decoding also preserves Coinbase's optional
`total_*_usd` cross-collateral fields under explicit USD names; that does not
resolve the unlabelled PnL-unit blocker.
