# Kraken venue adapters

## Futures hosted signing

Kraken Futures (`kraken`) admits RemoteSigner through its explicit construction
policy. Spot remains outside this activation. Local and hosted registry engines
use provider-owned custody for private REST reads/writes and WS challenges;
signing failures before transport remain NotSent, and ambiguous writes stay
Unknown. The public `Kraken::new` local compatibility constructor is retained;
registry construction uses `Kraken::with_signing` with opaque refs only.

Native public-only startup discards unused primary identities before validation,
so saved blank or key-only primary fields do not block unauthenticated market
data. Private primary and secondary read credentials retain their validation.
The SDK's shared explicit-public setup also ignores unused signing callbacks
and grants no signing authority.

Trusted native and private-capsule setup preserve construction refusal ordering:
incomplete raw remote accounts refuse before secret decoding, and malformed
private primary secrets retain Kraken's base64 diagnostic before provider
installation. Explicit public setup discards unused primary material before
decoding; secondary read validation remains active.

REST signs the exact encoded form, optional frozen decimal nonce and path (without the
`/derivatives` prefix) with SHA256 followed by HMAC-SHA512. WS signs SHA256 of
the challenge with the same MAC. See Kraken's [REST authentication](https://support.kraken.com/ca/articles/360022635592-generate-authentication-strings-rest-api-derivatives)
and [WS challenge signing](https://support.kraken.com/articles/360022635652-sign-challenge-websocket-api-derivatives).

The device wire uses `ws-auth` / `kraken-sha512` and a closed
`{slot: "primary" | "secondary", payload: string}` descriptor. Payload is the
canonical preimage, never secret material or a key ref. The device resolves its
own slot and returns `{credential, signature}` as JSON, where signature is
canonical base64 of 64 MAC bytes. The origin checks the selected public
credential and complete result before returning the atomic Present/hash/MAC
batch. The existing `kraken-challenge` format remains a primary challenge API.
Wrong-account replies, foreign refs, partial graphs, unavailable devices,
cancellation and callback errors refuse without local-secret fallback.

SDK/native registry custody uses the existing engine-owned opaque slot refs;
caller-assigned `stableId` metadata and account migration are unnecessary.
Local setup decodes base64 secrets. Provider REST omits the optional Futures
`Nonce` header and signs `form + path`. [Kraken's Futures guide](https://docs.kraken.com/exchange/guides/futures/rest)
describes supplied nonces as incrementing with only brief out-of-order tolerance,
not as scoped to a connection or session. Independent per-engine counters would
therefore be unsafe when sending a nonce on the same key. Omitting it avoids
duplicates, arrival-order races and shared nonce state across local and hosted
engines, including separate processes/WASM memories. This is a nonce-free mode,
not a claim that generated nonces are unique across engines.

The compatibility HTTP signing API preserves a supplied decimal `Nonce` header
byte-for-byte, including u64 values beyond JS safe integers. If absent, it omits
the optional nonce instead of creating a competing JS counter; [Kraken permits
nonce-free Futures REST authentication](https://support.kraken.com/articles/360000906023-what-is-a-nonce-).
Callers explicitly supplying nonces still own key-wide coordination: our managed
nonce allocator shares state only within one native process or WASM module.
The existing explicit-nonce recipe API and Spot nonce behavior are unchanged.

Private PayloadSigner remains typed unsupported/NotSent: that callback's current
`hmac-sha256` string contract cannot express Kraken's binary SHA256-to-HMAC-SHA512
graph. Kraken Futures uses symmetric API keys rather than this SDK's wallet-agent
cosigning modes. [Subaccounts have separately controlled API keys](https://support.kraken.com/ca/articles/360042809671-understanding-derivatives-subaccounts);
configure that subaccount's matching device key instead of a separate agent key.

NEEDS-NX: carry the new `kraken-sha512` format through to the updated SDK on the
device; no new identity assignment or forwarding is required. The refused list
derived from construction-rules by NX #9032 follows the explicit policy
automatically. No ExchangeName or account-key field is added. WASM size caps are
left to the lead's exact-head CI budget commit.

## Futures placement size parity

Futures placement follows the legacy TS `kraken.exchange.ts:452-462`: round the
amount to its step and send one payload, even above the market's amount maximum.
It does not split or cap that size. A native `invalidSize` reply remains a
definite venue refusal, as in TS `:514-529`.
KNOWN-ISSUE: TS sends above-max; venue refuses.

## Futures account history

Kraken's [account-log schema](https://docs.kraken.com/api-reference/account-history/get-account-log)
allows a null `contract` on account-level entries. Null, missing and empty
contracts retain the existing empty-symbol income mapping, including transfers
derived from reported old/new balances. They do not fail the streamed history
page or discard later trade rows. Nonnullable ledger identity, asset, date and
description fields remain strict; no economic values are invented for nulls.

## Kraken Spot hosted signing

Kraken Spot remains local-only, with hosted/remote signing deferred and the existing
typed refusal retained. Its strictly increasing nonce per API key must be shared
across local and hosted instances. Enabling hosted signing requires a caller-owned
stable nonce identity (`localSigner.primary.stableId`) and an account migration;
deriving that identity from key hashes or slot names is forbidden. If Kraken Spot
demand appears, this identity and migration work is the path to hosted signing.

## Position liquidation prices

Futures [`open_positions`](https://docs.kraken.com/exchange/api-reference/futures-websocket/open_position)
publishes `liquidation_threshold` in the instrument's quoted price units.
Only finite, positive numbers and numeric strings are usable prices.
Missing, null, empty, malformed or nonpositive thresholds are unknown. Some
Futures accounts publish `0.0` when no per-position threshold exists; a zero
or negative price cannot be used as a position's liquidation price. Normalize
these sentinels to unknown without preventing the position from updating.
This deliberately corrects the TS reference's preservation of explicit zero.

REST [`openpositions`](https://docs.kraken.com/api-reference/account-information/get-open-positions)
does not publish that threshold. It preserves a previously received WS value;
without one, the price is unknown. [`accounts`](https://docs.kraken.com/api-reference/account-information/get-wallets)
has different wallet shapes: `flex` / `multiCollateralMarginAccount` publishes
portfolio and margin totals, while single-collateral `fi_*` margin accounts can
include account-level `triggerEstimates.lt`. The TS reference uses accounts for
balances, not liquidation-price derivation; this adapter likewise does not
invent a per-position estimate from those totals.

Unknown prices use the packed frame's NaN representation and serialize to JSON
`null`. The SDK frame applier preserves that as `Position.liquidationPrice: null`;
finite positive prices remain numbers. Spot currently exposes wallet
balances and an empty positions table, without margin-position REST/WS mapping.

## Futures socket liveness

Every public and private socket subscribes to Kraken's documented
[`heartbeat` feed](https://docs.kraken.com/exchange/api-reference/futures-websocket/heartbeat)
on open, including reconnects and before private authentication. Futures rejects
the Spot-style JSON `event: ping`; it is not a WebSocket control ping. The
heartbeat feed supplies periodic receive evidence without that invalid request.

Both Futures sockets expire after 20 seconds without accepted receive evidence,
matching the TS reference's base watchdog. The managed supervisor checks expiry
at the actual callback time, including a monotonic jump after sleep. It retires
the old socket before accepting a late frame, applies bounded reconnect backoff,
then replays ticker/book interests or obtains a fresh private challenge and
subscribes fills, open orders and positions. Account reload is not required.
Connect/auth deadlines, retry caps and Spot policy remain unchanged.

Heartbeats prove socket liveness only. Separate venue-owned clocks watch data:

- Each subscribed ticker symbol must produce a decoded payload with at least
  one ticker value within 30 seconds (the documented
  [delta throttle](https://docs.kraken.com/exchange/api-reference/futures-websocket/ticker)
  is one second). Silence retires the public socket so its ticker/book interests are
  replayed; a busy sibling symbol, empty envelope or subscription ack cannot
  refresh that symbol. New interests start fresh clocks; removed ones stop.
- Each subscribed book retains its existing validated-data clock: after more
  than 120 seconds without a snapshot/delta it requests unsubscribe/subscribe,
  retrying at 30-second intervals until a valid snapshot arrives.
- Private fills, open orders and positions can legitimately stay quiet after
  their initial snapshots. Heartbeats and the 20-second socket receive watchdog
  maintain that connection; inactivity does not unsubscribe or reconnect it.
  Keep the live subscription uninterrupted while it is healthy.

The first valid `fills_snapshot` establishes startup history without emitting
live fills. Its valid, mapped IDs are retained in a separate bounded history
cache, so repeating that history after a reconnect stays quiet. They do not
enter the emitted-ID cache or block a later live delivery of the same execution.
If the first socket closes before delivering that snapshot, the replacement
snapshot recovers executions instead of treating them as startup history.

Subsequent `fills_snapshot` messages recover valid, mapped executions absent
from both caches. Live `fills` messages deduplicate only against the emitted-ID
cache. Both caches survive reconnects; no timestamp cutoff can discard a
previously unseen recovery execution, including one without a timestamp.
Snapshot and live replays of an emitted ID do not emit again. Invalid or unmapped
executions enter neither cache. Execution timestamps still guard order-progress
updates against stale data; emitting an older fill cannot roll progress backward.

The existing five-second venue timer checks these clocks independently of the
scaled account REST poll. Ticker expiry is also checked before accepting late
data. Initial private authentication and the 180-second first-payload bound
remain independently enforced; heartbeats cannot satisfy missing initial data.

The deterministic `socket_supervision::futures_wake_recovery_measurement` test
models paused timers, a clock jump, 50 ms socket handshakes and 50 ms server/auth
response stages. It measures first frames separately from complete private
snapshots; its five-second assertion is a simulation bound, not a network SLA.

Kraken Spot treats a dispatched write's HTTP 408, 429, or 5xx response as
`Outcome::Unknown`, even when the body contains a native signature, credential,
or rate-limit error. HTTP 429 retains `ErrorKind::RateLimited` for diagnostics.
These responses cannot authorize a resend or terminal credential rejection;
uncertain placements trigger an `OpenOrders` read before another placement.
An unknown result also refuses already-queued placements and amendments as
`Outcome::NotSent`; they were admitted before the uncertainty was known.
Queued cancellations remain available. Reconciliation never replays refused
writes: the caller checks Open Orders before explicitly retrying.

Definitive signature refusals retain the single re-sign attempt. Private reads
also retain their existing bounded signature and transport retry policies.

Futures writes likewise keep HTTP 408, 429, 5xx, and native `apiLimitExceeded`
responses unknown, including a bracket leg after an accepted entry. Their user
text says the outcome is unknown and directs the caller to Check Open Orders.
These failures never resend or place a replacement stop. Definitive refusals and
private reads retain their existing behavior.

As on Spot, an unknown Futures result refuses already-queued placements and
edits as `NotSent`, including after partial acceptance. Queued cancellations,
account settings and leverage reads remain available; refused writes never
resume automatically after reconciliation.

All Futures Unknown trading failures include reconciliation guidance, including
HTTP 200 native server errors and unproven cancellation statuses. Batch errors
retain an unknown leg's message and native diagnostics together when a later
leg is definitively refused.
