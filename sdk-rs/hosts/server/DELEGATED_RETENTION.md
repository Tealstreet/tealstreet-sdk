# DELEGATED retention

Hosted-supported DELEGATED accounts retain their protected provider for up to
48 hours after their last admitted client disconnects. The session resume ID still has its normal 60-second
grace. After that grace a client uses a fresh verified handover to join the same
running account. Pending or refused handovers do not renew retention.

Any admitted client cancels the account's outstanding retention deadline. When
the final client disconnects again, a new 48-hour interval begins. Another user,
another account incarnation, and an expired resume ID cannot renew that interval.
Settings successors sharing that provider renew only at successful publication;
superseded or refused builds leave the original signing deadline intact.

Expiry stops the account's algos and records `no client for 48 h`, with disposal
kind `no_client_48h`. The existing retained stop-notice path tells the next
admitted client. The resident signing material is erased at disposal, including
when engine or host references remain. Removal or disabling through the existing
`dispose` operation takes effect immediately, including when another client is
attached. LOCAL and COSIGNED accounts retain their existing lifecycle.

A poisoned resident lock is recovered only to discard its provider and erase
the protected material. It remains unavailable for signing or admission;
revocation and owner drop still erase it while other provider references remain.

Restart loses the resident copy. The router can deliver the authenticated
interruption marker with reason `waiting-for-key` through the existing upgrade
header. A DELEGATED init receives `waiting for your key` even before another
handover is admitted. Replies use that init's current `instanceId`; clients
without one retain the existing envelope format. Restored TWAPs use the existing
interruption pause and explicit-resume guard. No key, algo state, or deadline is
persisted by this module, and chases are never automatically restored.

Retained running or paused jobs continue to refuse idle release after the session
resume grace ends. Drain and router-lease fences remain authoritative throughout
retention.

## Proxy-authority limit

For a retained account whose resolved transport needs the socket-issued proxy
authority, that token is not renewed or extended by retention. On its expiry the
actor stops terminally and the key is erased immediately, with recorded reason
`proxy-token-expired`. Expired/missing authority cannot silently fall back to a
direct route. Direct accounts keep the independent 48-hour clock. An admitted
reconnect cancels the old timer and supplies its own checked token; a later
final disconnect starts a fresh interval under that authority.

**Decided (Sam, 2026-11-20, AGT-37):** there is no issuer-backed retained-token
renewal. The earlier stop at proxy-token expiry is the permanent contract, so a
proxied account has no full-48-hour availability promise. Expiry stops algos and
erases the key; it never cancels resting orders or closes positions.

## Host notification lane

`src/host_notify.rs` posts DELEGATED lifecycle and trading events to the
gateway (`POST ${TEALSTREET_HOST_NOTIFY_URL}`), which pushes them to the user's
devices. The wire contract is shared with the gateway and clients (Linear
AGT-37). Startup environment:

| Variable | Meaning |
|---|---|
| `TEALSTREET_HOST_NOTIFY_URL` | Gateway ingest URL (production `https://news.tealstreet.io/internal/host-notifications`). Unset: the lane is off, nothing is queued and no warning timer is armed. `https` only; `http` only for loopback. |
| `TEALSTREET_HOST_NOTIFY_TOKEN` | Bearer secret, required when the URL is set. Never logged; the header is marked sensitive. |
| `TEALSTREET_BOX` | Sent as `X-Tealstreet-Box`; `TEALSTREET_ROUTER_BOX_ID` is the fallback. One of them is required when the URL is set. |

Delivery uses its own HTTP client (never venue egress), a 5-second timeout and
a bounded in-memory outbox that drops the oldest past 1000 and logs the count.
Batches carry at most 50. Network failures and non-2xx statuses retry the same
batch in order with backoff from 1 second, doubling to a 10-minute cap; the
gateway dedupes the at-least-once delivery on `id`. 400/413/422 drop the batch
(logged), since a resend cannot succeed. Nothing is persisted; a restart loses
the queue.

`id` is `sha256(user \0 accountDigest \0 kind \0 naturalKey)`. `accountDigest`
is a domain-separated hash of the owner, venue and registry name (the identity
digest or the label), so a label never leaves the box. Every field is typed or
shape-checked; keys, API key ids, tokens, order ids, client order ids, prices,
sizes, balances, PnL, labels and venue text cannot serialize. `clientPresent`
is true while any live admitted holder holds the account; the gateway skips
pushes then.

Retention emits for DELEGATED residents only:

- `delegated.shutdownWarning` at `deadline - 1 h`, by the same task that owns the
  retention deadline, so cancelling retention also cancels the warning. It
  rechecks the account incarnation, timer generation and live holders first.
  An interval shorter than one hour (a nearly expired proxy token) has none.
- `delegated.shutdown` on expiry with cause `noClient48h` or `proxyTokenExpired`.
- `delegated.shutdown` with cause `serverRestart` for each DELEGATED resident
  stopped by drain/SIGTERM. The drain waits for delivery beside the engine
  shutdown, bounded to 3 seconds.

The algo host (`src/algo_notify.rs`) emits, for DELEGATED residents only and
never from a snapshot or a rejoin replay:

- `fill`: live fills from flush frames, aggregated per order until 2 seconds
  pass without another fill. `filled` when the order has left the store with
  its size met, otherwise `partiallyFilled`. It collapses per order through an
  opaque hash; order ids, prices and sizes stay on the box.
- `orderRejected`: a chase or TWAP placement the venue refused outright
  (`Outcome::Rejected`). Client writes, exec and nuke legs use the untapped
  port and never emit; unknown and not-sent outcomes never emit. At most one
  per algo, symbol and side per 60 seconds, so a retrying chase pages once.
  A chase placement on a market whose live chases are all TWAP slices reports
  `algo: "twap"`.
- `algoStopped`: a chase end whose recorded reason is not an explicit client
  stop (`User cancelled`, kept order, `chaser:terminateAll`, nuke), or a TWAP
  leaving the state map without a client `twap:stop`/`twap:cleanup` or nuke.
  Reasons are compared before any `; ...` retried-cancel suffix the chaser adds.
  `Completed`/length-condition chase ends and TWAPs without a slice-failure or
  shortfall report are `finished`/`algoFinished`; the rest `stopped`/`algoError`.
  TWAP slice chases report through their TWAP. Exec has no `algoStopped`: it
  is a one-shot client command answered on its own socket.

Stops caused by account disposal emit no `algoStopped`. Retention expiry and
drain/SIGTERM are covered by `delegated.shutdown`; router-lease fences, forced
user drains and revocation disposals send neither notification (a server-lost
notice from the router is deferred). Fills still waiting out the debounce are
sent when the algo host shuts down or is fenced.

The lane adds no timing, admission, custody, signing or key lifecycle behavior
beyond the warning timer, and never cancels resting orders.
