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
CDP issuer claims without changing the S2/S3 session transport under development.
Engine-owned signer slots select by request purpose: account/history reads can
use the read key; catalogue, permission checks, amend proofs and private socket
authentication use the primary key. A refused read key pins subsequent reads to
the primary signer and egress through the same selection boundary.
See `docs/plans/callers/coinbase-hosted-signing.md` at the repository root for
wire formats, typed refusals, algorithm selection and NX integration.
