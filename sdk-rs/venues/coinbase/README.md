# Coinbase derivatives foundation (S0)

## Advanced Trade create sizing

New Advanced Trade creates follow the legacy TypeScript formatter: normalize
the requested amount to the market step and send one payload, including above
the advertised maximum. Native refusals are returned to the caller without an
automatic resend. `KNOWN-ISSUE: TS sends above-max; venue refuses`.

Reference: `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts:1659`
(normalization), `:1772` (single request) and `:1573` (native edit maximum).
Native edit and INTX replacement checks retain their existing bounds.
Placement batches dispatch serially and stop on the first refusal or uncertain
response, matching `packages/safe-cex/src/exchanges/base.ts:1111-1114` and the
Coinbase reference's `placeOrder` at `:1612-1641`. A terminal failure preserves
accepted prefix IDs and an indexed outcome journal: accepted rows are `placed`,
a documented refusal is `notSent` with the venue message, and the untouched
suffix remains `notSent`. Uncertain rows say exactly "The outcome is unknown.
Check Open Orders", retain diagnostic text in `venueMessage` and emit an
`order.outcome_unknown` event attributed to the command and client order ID.
`tests/trading.rs::above_max_batch_*` pins `[1,26,1]` at max 5 / step 1,
including both refusal envelopes and a native rate-limit Unknown response.
`tests/trading.rs::legacy_ts_above_max_create_is_one_payload_and_surfaces_venue_refusal`
pins the six above-max cases and refusal propagation.

## Transport foundation

`drb::Transport` is the shared HTTP/main-WebSocket JSON-RPC transport owned by
`Coinbase::drb`. `drb_auth::Session` holds auth state for exactly one `Channel`
(HTTP or one socket ID). The existing venue flows are migrated by the separate
public, account and trading streams; S0 does not switch legacy routes itself.

Integration contract:

- Public order-book repair clears only the repaired book. In-flight changes are
  ignored until its replacement snapshot arrives, then normal sequence checks
  resume; other live books keep their state and shared socket.

- HTTP takes the caller's engine-issued `request_context::Context` and an optional
  engine `request_context::Session` handle. Private methods require that session;
  its `Venue::session_bearer` callback supplies the current token at preparation,
  and `session_bearer_revision` fences the same revision at release.
  Retain the context with the pending request, validate its delivery, record any
  Mutation result, and finish the context through the engine lifecycle. Production
  writes use genuine command/Plan authority. Catalogue and ticker polls use
  Background contexts; candle/trade queries retain their command context across
  pages. Reads deferred during discovery retain their original caller deadline
  and settle through `settle_caller_read_root` if the catalogue fails.
  No raw bearer strings or legacy `Cx::request` boundary are used by drb.
- Supported methods have a closed semantic classification, independent of rate
  pools. Orders, edits, cancels, position moves and account settings (including
  margin model, leverage and cancel-on-disconnect) require HTTP Mutation authority.
  Known queries, authentication, heartbeats and subscriptions use Read semantics. Unknown
  methods are rejected on HTTP and WS before admission; adding a method requires
  updating the classifier and its complete authority table in the foundation tests.
- HTTP returns `(RpcId, engine::RequestId)`; retain the RPC ID in the caller's
  pending context. Forward status/body and `cx.now` to `response_at` and transport
  loss to `failed`. WebSocket sends return an RPC ID; route responses by that ID.
- Subscription notifications are consumed by stream modules, not `response`.
- Call `expire` on a timer and `disconnected` when a socket closes. These return
  uncertain completions and never queue resends. Correlate each ID to its caller.
- Authenticate with fresh `Session::params`, send `public/auth`, then pass its
  decoded result and request-start time to `Session::accept`. Schedule refresh
  using `refresh_at`, gate private sends with `needs_refresh`, and invalidate the
  socket session on close and invalidate rejected credentials. Invalidation fences
  the latest issued or accepted authentication start, so pending completions cannot
  revive the cache. A fresh exchange must use a strictly later start timestamp;
  equal starts fail closed without changing the auth API signatures.
  The HTTP session-bearer callback can use `authorization`; WS authenticates
  in-band. Refresh always exchanges a fresh CDP JWT using `coinbase_cdp`; no refresh-token grant is assumed.
- Share one transport across streams so HTTP and WS consume the same credit
  pools. Local admission errors mean the request was not sent. A caller may
  schedule new admission later; released writes must never be resent implicitly.
- Use `to_instrument` / `from_instrument` only for legacy INTX perpetual aliases.
  Exact tradable names still come from `public/get_instruments`.

Protocol sources:

- https://docs.cdp.coinbase.com/coinbase-app/advanced-trade-apis/guides/derivatives/technical
- https://docs.cdp.coinbase.com/api-reference/coinbase-deribit-app-api/adv-starbase-openapi.json
- https://docs.deribit.com/articles/rate-limits
- https://docs.deribit.com/articles/errors

Rate admission starts at 5 matching requests/second with a burst of 20, and
20 general queries/second with a burst of 100. Instrument discovery,
subscriptions, transaction logs and position moves have dedicated credit pools;
position moves also enforce the rolling weekly cap. Supply the effective minimum
account/currency/trading/cancel limits to `set_matching_limits` when known.
Mass quotes have separate quotas and are rejected locally in S0. Server-side
quotas include other clients and remain authoritative.

`response_at` checks the 30-second deadline and starts rate-limit cooldown at
receipt time. The original `response` signature remains available for interface
consumers without a clock; its cooldown conservatively starts at the request
deadline. It requires the caller to expire pending requests on time. Correlation
is bounded to 256 pending IDs. A missing/mismatched ID, duplicate envelope field,
unlisted error, malformed reply, timeout or transport loss yields `Unknown`.
Native refusal codes never grant a resend. Error text/data are available in
`Failure::rpc_error`; Debug output omits those untrusted fields.

Auth refresh uses the earlier of `expires_in` and the transport policy (15 minutes
for HTTP, 49 minutes for WS), with a 10% margin capped at 30 seconds. Clock
regression and non-finite time fail closed. Cache tokens are erased on drop or
invalidation. Gateway JWTs use `iss: cdp`, a fresh nonce, and a 120-second lifetime
for both existing CDP signer algorithms. Existing spot JWT claims are preserved.

The interface was published first in `af5df7922`; implementation follows on the
same branch. All protocol and signer tests use offline fixtures and synthetic
keys. No private live calls or orders are used.

Validation: `cargo test --locked -j 2 -p safe-cex-venue-coinbase` from `sdk-rs`,
plus scoped Clippy and a `wasm32-unknown-unknown` check. The foundation suite was
run RED against the interface stubs before implementation; additional RED tests
caught legacy ES256 issuer reuse and delayed-response cooldown timing.

## Private derivatives reads (S2)

Selecting the existing INTX portfolio exchanges the primary CDP key on drb
before starting S1’s drb catalogue and derivatives account reads. Private
positions, orders and fills use the catalogue’s native instrument symbols.
A successful, correlated
`public/auth` response enables `Coinbase::drb_authenticated()`; a configured
key or failed exchange does not. Consumers of the INTX retirement notice should
suppress it only when this hook is true. Spot-only and CFM accounts retain their
Advanced Trade startup, signing, and socket paths.
After authentication, private startup uses S1's drb catalogue; positions, orders
and fills share its native `{BASE}_USDC-PERPETUAL` symbols.

`private_drb.rs` owns the HTTP bearer session, independent authenticated main
WebSocket session, and read contexts. It polls account summary, positions
(including isolated subaccounts), and open orders (including untriggered OTOCO
children) and subscribes to `user.changes.future.USDC.100ms`,
`user.isolated.changes.future.USDC.100ms`, and `user.portfolio.USDC`. It answers
heartbeat test requests, refreshes with fresh CDP JWTs, and authenticates each
new socket. Delayed REST snapshots cannot overwrite newer stream observations
or accepted writes. Malformed account snapshots preserve the last complete
state. Socket changes without a liquidation estimate or leverage retain the
last REST values. Isolated position IDs include their native subaccount ID.

The USDC summary exposes equity as total, available funds as free, their
difference as used, and futures session unrealized PnL as upnl. Position
contracts use explicit `size_currency`; notional uses base size times mark.
Linear perpetual order and fill quantities use base-coin units as specified by
Coinbase's technical migration guide. Nullable liquidation estimates map to the
engine's unavailable value (zero); absent leverage remains unavailable (zero)
until an authoritative value is reported. Portfolio margin models refer to
collateral/risk calculation; isolated positions use the explicit `isolated` flag.

`private_drb_history.rs` serves fills by order, instrument, or account and order
history by instrument or account. It retains the genuine command Read root
across recent and historical stages, bounds the walk to 50 pages, deduplicates
native identities, validates symbol/order filters, and applies inclusive time
bounds. Fill timestamps remain seconds and historical order times milliseconds,
matching the existing TypeScript interface. Histories never publish account
state or live fills. Fills follow native trade sequences/IDs and orders follow
continuation tokens or documented offsets. Exhausted page bounds are reported
through the engine's incomplete-history mechanism.

All requests use S0's closed Read method table; this module introduces no
mutation methods or mutation authority. Trading migration belongs to S3;
`drb_http_session()` exposes only the opaque engine binding while valid, and
`drb_ensure_http(cx)` starts/refreshes authentication without consuming a
trading command's retained root. Trading integrations resume their own waiters
after the HTTP auth success branch and drain them on definitive auth refusal.
Provider-backed accounts retain only public CDP bindings and opaque signer
references. HTTP and WebSocket logins prepare fresh JWTs at dispatch through
the installed provider, with the RPC ID and physical channel fixed before the
signing wait. Retiring a socket drops its pending authentication result. Both
the login and subsequent bearer reads use primary account egress even when a
separate read key serves legacy account discovery. JWT claims use protocol UTC;
session expiry and invalidation retain scheduling time.
The old account/trading integration fixtures now run as internal unit fixtures
with an explicit test-only legacy selector, preserving their shared Spot/CFM
regression oracles. This selector is absent from production and public Options;
the new private tests exercise the production drb startup route. Provider graph
parity, route authority and delayed socket replacement controls are in
`tests/support/drb_provider.rs`.

Offline protocol fixtures follow the [technical migration guide](https://docs.cdp.coinbase.com/coinbase-app/advanced-trade-apis/guides/derivatives/technical),
[OpenAPI](https://docs.cdp.coinbase.com/api-reference/coinbase-deribit-app-api/adv-starbase-openapi.json),
and [AsyncAPI](https://docs.cdp.coinbase.com/api-reference/coinbase-deribit-app-api/adv-starbase-asyncapi.json).
No live private authentication, account request, or order is part of validation.

The CI private replay for the `intx` profile (`harness/coinbase-drb-tape.mjs`) runs the
WASM engine through drb auth, private reads and the private stream. The TS INTX
implementation is retired and cannot model drb, so that tape is replayed by Rust
alone and compared against the tape's declared expected account. Its strict result
stays FAIL with one declared known failure (the retired TS oracle), never parity.

Private read failures retain S0's classified kind and native error fields in
command failures and startup diagnostics. Rejected credentials terminate the
startup through `startup_read_refused` and stop further gateway authentication
attempts; transient network/rate failures retain their classification. Expired
sessions on an already-loaded account can refresh. This read-error projection
provides no write resend authority.

Order decoding validates the optional `original_order_type` against the native
`market`/`market_limit` enum. When the gateway represents one of these orders as
an internal limit, both live state and history retain the original market type;
ordinary limits and conditional stop/take/trailing types keep their own type.

## Trading (S3)

`trading_drb` intercepts USDC perpetual commands using the discovered native
instrument name (or S0's legacy INTX alias mapping). Spot and CFM retain their
existing trading codecs. All gateway writes use genuine command-owned Mutation
contexts with `RetryPolicy::Never`; authentication and reconciliation use Read
contexts. Each trading method takes its Read/Mutation authority from S0's
closed classifier rather than a second local table. HTTP bearer tokens are exchanged with the primary CDP key, refreshed
before expiry, and attached only by the engine's session callbacks.

Market and limit entry support IOC/FOK; limits also support GTC and post-only
with `reject_post_only`, so the gateway cannot silently move a post-only price.
`amount` is in base-coin units for these linear USDC perpetuals. Native
`reduce_only` also implements explicit closes. Amend uses `private/edit`,
preserving the native ID and cumulative fills. Cancel supports one or more IDs,
all orders on an instrument, and `VenueAction::CancelAccountOrders` for the
gateway account. Bulk cancellation acknowledgements prove a count; they do not
invent per-order terminal states.

An entry's attached TP/SL uses OTOCO with incremental fill handling and opposite,
reduce-only market exits. A single attached exit uses OTO. Standalone protective
orders use native reduce-only `stop_market` / `take_market` triggers, with last,
mark or index price. Generic conditional orders, GTT, slippage/BBO/STP controls,
and edits of linked/protective legs without native leg identity are refused
before I/O. There is no silent cancel-and-recreate fallback.

Instrument leverage uses `private/set_leverage` and `private/get_leverage`.
Side-specific leverage and hedged-position mode are unsupported. The gateway's
account-wide SM/PM margin-model change cannot safely implement Tealstreet's
per-symbol isolated/cross toggle, so that command returns an explicit refusal.
Testnet commands are refused rather than sent to the production gateway.

The optional custom leverage may be null; reads use the native maximum as its
documented default. Malformed custom values cannot silently select that default.
Native internal limits preserve documented original market intent in account,
acknowledgement and recovery decoding. Linked identities survive refresh and
snapshot readmission independently of displayed exit prices. A queued amendment
rechecks linked identity and instrument uncertainty after authentication.

Client IDs map to `label` (at most 64 bytes). **The gateway does not guarantee
label uniqueness or server-side idempotency.** This engine reserves each label
once, including after a rejection or loss, and never repeats a released write.
Reservations are bounded to 4096 labels per engine lifetime; reaching the bound
requires an engine restart and fresh IDs. This does not deduplicate submissions
made by another client or a previous engine instance.

Timeouts, transport loss, mismatched acknowledgements and unlisted RPC errors
finish as Unknown. Reconciliation reads the primary account by label/native ID,
or reads leverage/open orders for settings and bulk cancels. Empty/duplicate
label matches preserve uncertainty and are read again later. New placements and
edits on an uncertain instrument wait for read-back, including when a new client
ID is supplied. Recovery cannot reopen the failed command, grant a resend, roll
back cumulative fills, or overwrite newer terminal state. Accepted writes
invalidate older order-snapshot tokens.
An unchanged open-order read does not resolve a lost edit or cancellation;
recovery keeps polling until it sees the edited values or a terminal order.
Dispatched HTTP 408, 429 and 5xx responses also finish as Unknown, even if their
bodies look like a native refusal, as does a gateway rate-limit error (10028) on
a released write. This covers place, OTOCO, amend, cancel, bulk cancel and
leverage. The engine cannot resend or put those writes back.

Offline fixtures in `tests/fixtures/drb-trading.json` are adapted from the
[Coinbase OpenAPI](https://docs.cdp.coinbase.com/api-reference/coinbase-deribit-app-api/adv-starbase-openapi.json)
and the [technical migration guide](https://docs.cdp.coinbase.com/coinbase-app/advanced-trade-apis/guides/derivatives/technical).
The first 10 command tests were run RED against the legacy route; further RED
tests caught acknowledgement mismatch, stale recovery, uncertain-ID fencing,
settings recovery and absent-label polling bugs. No live private calls or orders
are used.

## Public perpetual data (S1)

Keyless derivatives startup discovers active linear USDC perpetual instruments
and then fetches their book summaries. It uses two HTTP calls for the full
catalogue, rather than one ticker call per market. Prices and funding update
through public ticker subscriptions, with a five-second summary refresh.
Spot-only startup continues to use Advanced Trade spot endpoints.

`public_drb.rs` consumes the shared `Coinbase::drb` transport. Pending public
requests retain their RPC IDs and settle through `public::Request::Drb`.
The separate account migration can call `Coinbase::drb_start` after determining
that an authenticated account uses global derivatives; legacy private account
discovery is outside S1's scope.

Instrument IDs and symbols use the exact `{BASE}_USDC-PERPETUAL` name. Linear
trade and book amounts are base currency amounts, without contract scaling.
Market amount precision comes from `contract_size`, with `min_trade_amount`
kept as the minimum order size. Summary `estimated_delivery_price` seeds the
index; ticker notifications supply `index_price`. Funding uses the projected
`funding_8h` rate as a fraction, with settlement times in milliseconds.

Books require a snapshot before accepting changes, and every change must link
its `prev_change_id` to that instrument's last `change_id`. Gaps clear the book
and reconnect. Duplicate or older frames cannot rewind a book. Chart channels
carry the requested supported minute/day interval; subminute candles aggregate
deduplicated public trades. Chart history converts wire milliseconds into SDK
candle seconds and returns chronological, half-open ranges, paged in blocks of
1,000 bars. Unsupported native chart resolutions are rejected explicitly.
The chart HTTP route uses GET: live POST calls return `-32601`. Its response
omits `id`, so the shared transport correlates that specific read through the
engine HTTP request while retaining credit admission and the response deadline.
Explicit wrong IDs still fail. Other RPC methods retain strict ID correlation.

REST and drb share the existing Coinbase HTTP emission and public command
completion boundaries. Public sockets also use one existing connection site;
its scoped inventory replacement only changes the URL selection for drb.
The frozen HTTP/completion and socket inventories gain no new call sites.

See [the live tape and offline replay gate](tests/fixtures/drb-public.README.md).

## Hosted CDP signing

SDK/registry construction consumes local credentials into provider custody and
uses the same dispatch JWT recipes as keyless RemoteSigner accounts for private
REST and WS authentication. ES256 and the existing Ed25519 input formats retain
their canonical claims and signature bytes. REST-only PayloadSigner can sign
canonical JWS bytes and returns a 64-byte signature in unpadded base64url;
private WS auth requires RemoteSigner. Wallet/agent and OAuth credentials are
separate protocols, not CDP JWT substitutes. The signing-only DRB codec accepts
CDP issuer claims through the shared private-session and trading transport.
Engine-owned signer slots select by request purpose: account/history reads can
use the read key; catalogue, permission checks, amend proofs and private socket
authentication use the primary key. A refused read key pins subsequent reads to
the primary signer and egress through the same selection boundary.
See `docs/plans/callers/coinbase-hosted-signing.md` at the repository root for
wire formats, typed refusals, algorithm selection and NX integration.
