# DELEGATED retention

Hosted-supported DELEGATED accounts retain their protected provider for up to
48 hours after their last admitted client disconnects. The session resume ID still has its normal 60-second
grace. Within it, the socket that resumed the id may claim the parked label with a
`resume` init (DESIGN.md, Sessions and resume); the key stays memory-only and is never
re-sent. After that grace a client uses a fresh verified handover to join the same
running account. Pending or refused handovers do not renew retention.

Any admitted client cancels the account's outstanding retention deadline. When
the final client disconnects again, a new 48-hour interval begins. Another user,
another account incarnation, and an expired resume ID cannot renew that interval.
Settings successors sharing that provider renew only at successful publication;
superseded or refused builds leave the original signing deadline intact.

Expiry stops the account's algos and records `no client for 48 h`, with disposal
kind `no_client_48h`. The existing retained stop-notice path tells the next
admitted client. The resident signing material is erased at disposal, including
when engine or host references remain. A page `dispose` (removal, disabling, or a
device switching that account to LOCAL or COSIGNED) and a ping orphan detach only
that holder (AGT-79). While a live holder of another client registration that
requested DELEGATED remains, the account, its algos and the resident signer keep
running untouched. Otherwise the dispose stops the account and erases the key
immediately. COSIGNED tabs attached to the resident, parked holders and the
leaving registration's own stale sockets never keep it. A socket that has not
joined the account still removes it unconditionally. Expiry, signer failure,
lease fence, drain, settings rebuild and `Registry::dispose` still stop every
holder. LOCAL and COSIGNED accounts retain their existing lifecycle.

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
a bounded in-memory outbox of 1000 that logs every drop. When full it evicts
the oldest `fill`, then the oldest other low-priority notice (`orderRejected`);
`delegated.shutdown`, `delegated.shutdownWarning` and `algoStopped` are never
evicted for those, and an incoming low-priority notice is dropped instead.
Only a queue holding nothing but priority notices drops its oldest.
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
  stopped by drain/SIGTERM. The gateway treats it as a diagnostic: it logs it
  and never stores, pushes or replays it (Sam, 2026-11-21: notifications to end
  users are not telemetry; never push server-lifecycle events that are not
  user-specific). An ordinary roll waits for the user's chases and TWAPs, so it
  stops only idle residents; what a drain cuts short is reported per algo below.
  The drain waits for delivery beside the engine shutdown, bounded to 3 seconds
  in all: first for each algo host to queue its `algoStopped` notices, then for
  the outbox to empty.

The algo host (`src/algo_notify.rs`) emits, for DELEGATED residents only and
never from a snapshot or a rejoin replay:

- `fill`: live fills from flush frames, aggregated per order until 2 seconds
  pass without another fill. `filled` only when the order's own record (its
  current row, or the row right after its last fill) shows `filled` reaching
  `amount`; fill amounts are never compared with the order, since lot venues
  use different units. Anything unproven is `partiallyFilled`, including an
  IOC or capped market order whose remainder was cancelled in the fill's own
  batch, and a full fill whose row the venue removed in that batch. It collapses per order through an
  opaque hash; order ids, prices and sizes stay on the box.
  **Each fill carries a placement claim (AGT-196).** Two Tealstreet users can
  connect the same venue account; each gets its own resident and both private
  streams carry every fill on it. The box reports every fill, as before, plus
  `fillKey` and `placedHere`. `fillKey` is a domain-separated hash of the
  venue account, venue, symbol and order id: the same on every resident of
  that venue account and on no other, so per-symbol order ids (Binance USDⓈ-M)
  and caller-chosen ids (portfolio margin) cannot collide across unrelated
  accounts. The venue account (`accounts::identity::venue_account`) hashes the
  venue, network, Bybit site, subaccount, vault and API key, never the
  Tealstreet user; a second API key on the same venue account is another
  digest, so its fills stay unclaimed there. No id or key leaves the box. A key the venue-account hash cannot read
  falls back to that login's own account and logs `cause=unreadable_key`, so
  its fills fail open. `placedHere` is true when the native host's per-actor
  ledger (`safe_cex_native::own_placements`: engine-generated dispatch receipts
  and completion ids of this actor's placements, proven legs of a failed batch,
  and amends only of orders it already holds; a caller-chosen client id only
  through a proven placement, retired a minute after its order closes or a later order read omits it) names exactly the order id, or the row's client order id or
  parent, on the fill's symbol. A triggered child is linked at fill time from
  the venue's frame (`Notice::OrderTriggered`), so it is claimed even when it
  filled before any row was stored: OKX by `algoId` or `algoClOrdId`; Binance
  USDⓈ-M and COIN-M by `ALGO_UPDATE.ai`; BloFin by `algoId`/`tpslId`, unverified live.
  False means "not known here", never "someone else's": the ledger is
  memory-only (4096 newest identities, each for 24 hours), so orders from
  before a box roll, older resting orders, the venue UI, bots, liquidations and
  ADL are all unclaimed, as are Binance portfolio-margin triggered
  children and Bybit/BingX attached TP/SL, which carry no link. The gateway
  withholds a fill only when ANOTHER user's resident claimed the same key;
  unclaimed fills reach every login on the account exactly as before.
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
- `algoStopped` with `verb: stopped`, `cause: serverRestart` for each chase and
  TWAP a box stop cuts short: a drain or SIGTERM (`roll.sh --now`,
  `POST /admin/drain?now=1`, a host shutdown) or a forced user drain
  (`/admin/user-drain/force`, `exchange-roll --on-cap force`). Chases are never
  restored, so an absent user would otherwise never hear (AGT-119). One per
  algo, sent as the host receives the stop and before it settles; chases
  already stopping, TWAP slices, paused TWAPs and TWAPs with a client stop in
  flight are not reported. A chase's stop uses the chase's ordinary end `id`,
  so the gateway keeps at most one per chase. A TWAP is restored paused under
  its id and can still end for real later, so its stop gets an `id` of its own,
  minted once per stop (retries still dedupe), and shares only the collapse key
  with that end. The host reads the account's DELEGATED state through a weak
  signer, so a forced user drain holds each account until its host has queued
  these, bounded to 3 seconds.

Other stops caused by account disposal emit no `algoStopped`: the host's own
chase ends and TWAP removals during disposal are not ordinary ends. Retention
expiry is covered by `delegated.shutdown`; router-lease fences and revocation
disposals send neither notification (a server-lost notice from the router is
deferred). Fills still waiting out the debounce are sent when the algo host
shuts down or is fenced.

The lane adds no timing, admission, custody, signing or key lifecycle behavior
beyond the warning timer, and never cancels resting orders.
