# KuCoin socket recovery

Every detail, snapshot-shard, and private feed has its own engine `Supervisor`.
Reconnects back off exponentially with deterministic jitter and POST a fresh
role-specific bullet token. A 10-second acquisition/connect deadline retires
hung requests; per-feed generations reject late token replies. Failed token
requests no longer burst immediate retries.

The bullet response still selects the endpoint and heartbeat timings. Native
welcome and mandatory-pong deadlines remain independent of general data traffic:
an early pong cannot replace welcome, and ordinary data cannot satisfy an
outstanding ping. The Supervisor supplies the negotiated ping cadence; only
welcome marks a session authenticated and starts subscriptions. Futures REST
account reconciliation starts at launch and does not wait for the private socket.

KuCoin Spot HF uses the same supervision policy for its public and private
feeds. Each retry POSTs a fresh role-specific bullet token under a single-flight
acquisition with a generation fence. Token acquisition and transport opening
have separate ten-second deadlines; only a native welcome after opening marks
readiness. The bullet server's validated heartbeat timings drive the welcome
and mandatory-pong deadlines. Ordinary data and early pongs cannot satisfy
those deadlines. Late retired token responses/failures and socket frames are
inert, and welcome restores current ticker/book/candle or private account topics.

Private GET reads in both markets recover once from the native `400002`
timestamp refusal. They fetch public `/api/v1/timestamp` from their own REST
origin, calibrate the engine's protocol clock at receipt, then sign the exact
same path and query with the original selected identity. Normal startup adds no
clock request. The probe has a twenty-second cap; it and the resend remain under
the original read root and caller deadline. Invalid time, expiry, transport
failure, an HTTP failure on the resend, or a second timestamp refusal ends that
attempt without reopening outer retries. Existing periodic
account reconciliation can still start a new read on its normal schedule.

History and spot lookup continuations retain their original command root across
pages and retries. Background snapshots use a bounded root for each physical
page. Public reads stay unsigned when no identity was selected. Trading jobs
and POST socket-token requests do not enter this recovery path; writes are
never replayed on `400002`.

Ordinary Futures and Spot writes treat dispatched HTTP 408, 429, and 5xx replies,
and native `429000` rate limits, as `Outcome::Unknown`. Futures also recognizes
its [documented business-layer `200002` rate limit](https://www.kucoin.com/docs-new/error-code/futures)
before classification or cancel-family fallback; Spot retains its distinct native
code meanings. The write-only classifier weakens these failures before job
settlement, including after an accepted cancel or placement. User text directs reconciliation through Open Orders; these replies
cannot authorize a resend or a protective put-back. The ordinary Futures `400005`
signature retry excludes these ambiguous write statuses. A native rate limit on
HTTP 404 cannot authorize the ordinary cancel's order-family fallback. Protective
replacement text states uncertainty without claiming the position has no stop.
Managed protective writes retain the engine's unproven-fact boundary. Private
reads and documented definitive refusals on non-ambiguous responses keep their
existing classification.

When a later ordinary cancel leg is unknown, its uncertainty text and diagnostics
replace any earlier refusal in the terminal error. Ordinary Futures and Spot
write warnings use the normalized write error before command completion, so both
warning and terminal text direct ambiguous outcomes to Check Open Orders.

Ordinary Futures cancel-family fallback requires HTTP 404 with explicit native
`404` or `404000` not-found evidence. Native server errors, unlisted codes, and
malformed replies cannot release a second DELETE. Unknown write warnings and
terminals, including native `500000` and transport failures, direct callers to
Check Open Orders while retaining native diagnostics. An exhausted cancellation
confirmation read emits its warning after adding the same uncertainty guidance.

## Futures order size

Futures placement follows the legacy `kucoin.exchange.ts:881-925` formatter:
normalize the native contract count and send one payload, including above-max
sizes. The venue enforces the maximum and its refusal reaches the caller.
Minimum-size and positive-quantity checks retain their existing behavior.
Recording tests in `tests/trading/allocation.rs` pin the single payload, its
size and sum, and the surfaced native refusal.

## Hosted signing for Spot

KuCoin Spot shares Futures' dispatch-time HMAC-SHA256 recipe: timestamp, method,
path/query and exact body bytes, plus a separate passphrase MAC and optional
broker MAC. RemoteSigner resolves the whole `kucoin-batch` atomically on the
device, using the selected primary or secondary API-key slot. The origin keeps
only public identity and opaque refs. Private WebSocket authentication uses the
signed `POST /api/v1/bullet-private` request and the returned socket token.

Private `PayloadSigner` is unsupported (`unsupported` / `notSent`): signing one request
payload cannot produce the separate device-owned passphrase and broker MACs.
There is no agent/API-wallet signing mode in this integration. See
[`kucoinspot-hosted-signing.md`](../../../docs/plans/callers/kucoinspot-hosted-signing.md)
for caller impact and deterministic parity coverage. Explicit public-only accounts
ignore unused signing configuration and install no signing authority.
