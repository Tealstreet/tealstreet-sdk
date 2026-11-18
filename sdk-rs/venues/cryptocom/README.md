# Crypto.com socket recovery

## Hosted signing

REST and private WebSocket authentication use HMAC-SHA256 with the account's
API key and secret. The canonical payload is `method + id + api_key + sorted
parameter names/values + nonce`, and the signature is lowercase hex. The nonce
and request id are the dispatch protocol timestamp in milliseconds. See the
[official authentication contract](https://exchange-developer.crypto.com/exchange/v1/docs/api/rest/crypto-com-exchange-api-v-1).

Local and hosted accounts share the existing `sign::Envelope` dispatch recipe.
Owned provider setup holds the secret; generic engine options contain only
opaque identities. RemoteSigner sends canonical bytes through the existing
`ws-auth`/`hex` callback for REST and WS, never signing material or opaque refs.
The device's hosted capture resolves its own primary identity. Failed signing
has no local-key or PayloadSigner fallback and cannot dispatch a private wire.

PayloadSigner can sign the same canonical HMAC payload for private REST reads
and writes. It uses the REST-only private-feed policy: reconciliation continues,
but private user sockets, authentication and reconnect upkeep are suppressed.
Public feeds are unchanged. RemoteSigner keeps private streaming.
Bounded callback metadata follows the declared HTTP header policy in both
native dispatch and hosted HTTP capture. The HMAC stays in the JSON `sig` field;
HTTP headers never replace it. WebSocket authentication refuses result headers.

Crypto.com subaccounts authenticate with their own API key/secret; this SDK
adds no wallet/agent signing or threshold-cosigning protocol. Hosted secondary
read credentials are refused with `unsupported` / `notSent`, because the raw
HMAC callback has no slot selector. Native private requests are POSTs and keep
their existing primary selection. Remove secondary credentials for Hosted use.

Golden fixtures cover every signed native method (account/identity, positions,
balance, ordinary/advanced open orders, trades/order/transaction history,
ordinary/advanced placement, cancellation, amendment, cancel-all and leverage)
and WS authentication. Existing native fixtures also compare their actual
dispatched signing results with device captures, including recovery and
ambiguous writes. SDK/WASM tests compare local, keyless Hosted and PayloadSigner
captures without network I/O. No WASM budget cap is changed here.

## Socket recovery

Market and user sockets have independent engine `Supervisor` instances with
10-second connection deadlines and 90-second receive watchdogs. Reconnects use
capped exponential backoff with deterministic jitter; short open/close flaps do
not reset the failure count.

The existing one-second delay before subscriptions/authentication remains.
Private login must succeed within 10 seconds of opening. A rejected login closes
immediately and retries after backoff. Heartbeats are answered with their native
request ID but cannot extend the authentication deadline. Retired sockets cannot
supply authentication acknowledgements to replacements, and duplicate successful
acknowledgements do not duplicate subscriptions. No active client ping is added.

Authentication requires an explicit numeric `code: 0` in the native reply.
Missing or malformed auth codes leave the login pending until its existing
deadline; heartbeat traffic still receives replies but cannot authenticate the
session or extend that deadline. Public data-envelope defaults are unchanged.

Candlestick subscriptions discard their entire first data response, including
the newest row, because it is a history snapshot. Only subsequent pushes enter
live candle frames. Reconnects, repairs, and a new subscription after the final
unsubscribe each discard history again. Requested history is available through
`FetchCandles`. Both live and fetched candles convert Crypto.com's millisecond
start times to Unix seconds, preserving fractional seconds.

Released writes answered with HTTP 408, 429 or 5xx remain `Unknown`, even when
the body carries a native refusal or signature code. Such a response cannot
authorize another physical attempt of that write or release its uncertainty
fence. Documented signature refusals at non-ambiguous statuses keep their bounded
retry; account and prerequisite read retries are unchanged.
