# BloFin managed sockets and order census epoch

One venue-only slice from master5065bbf9b4d, after BingX2304. Both feeds declare
managed_sockets and register before either connects; no mixed raw lifecycle.
No engine, signer, registry, host or checker changes. Every merge needs an exact
8x5tm1 GO on a fresh attempt-1 run containing master3dae492.

Public retains connect20s, idle20s, ping5s and backoff cap16s. Private retains
connect10s, authentication10s, idle20s, ping5s and default30s backoff cap.
AnyText refreshes liveness; pongs never prove authentication. Public AuthNone,
private AuthRequired, native controls and Reconnect recovery fit existing APIs.
The unchanged installed-Primary SessionAuth recipe signs one fresh login at each
genuine Opened. Native ping follows. Only definitive code0 login ACK opens the
four private channel interests and account refresh; duplicate/stale ACK/open and
old signing completions cannot replay them. Rejected or malformed proof retires
only private; permanent pre-start key refusal retains native startup failure.
No signed login is retained as a replayable interest.

Canonical public desired interests cover tickers/funding for every market,
refcounted books/books5 and candle/books5 union. Encode the existing subscribe
and unsubscribe JSON in bounded4096-byte batches. Provisional additions/removals
commit local demand only with the exact proposal; synchronous refusal retains
committed maps. Asynchronous Capacity retains desired work, invalidates public
books and uses shared Closed/backoff, preserving private. Permanent encoding or
invalid-interest defects and inherited initial Ready replay errors fail closed
on the affected feed. Sequence-gap book resync uses genuine repair interests,
never raw frames or new socket authority. No HTTP book handoff is needed.

Retirement clears sequence/book/resync before replacement rows and publishes
book invalidation. Engine owns connect, auth, liveness, ping and retry identities;
remove both legacy supervisors. Suppressed public must not block real private.
Preserve native URLs/testnet, provider wire, financial reducers and HTTP hooks.

The full order census spans three families and pages. Capture order_read_epoch
ONCE when the first page is issued and retain that minimum through every
continuation. Only clean current Primary census publication calls
orders_reconciled_since(None, captured_epoch). Keep orders_revision and the
uncertainty_generation guard: reasks join actual busy/retry work and never move
the census origin. A first-page-before-mark/last-page-after-mark regression must
remain fenced until an entirely fresh clean census.

Controls use real Blofin binding, installed provider, actual socket/timer IDs and
Prepare/Bind/Claim. First pin duplicate public/private Opened and required ACK,
plus explicit declaration. Extend with genuine reconnect/fresh signature, stale
and nonfinite full Store/action checks, sibling isolation, public suppression,
refcount/books5 union, sequence repair, real valid-key pressure and retained
recovery. Preserve old financial/assertion/payload bodies; fixture observation
changes are explicitly documented. Remove only BloFin's three fingerprint rows
(four occurrences) and family after the actual scanner proves zero raw sites.
No request/signing/credential inventory changes. Local scoped tests/strict/fmt/
bans, reviewer CLEAR, fresh correctness/CI family/replay and auditor GO gate it.
No local family matrix or combined build under the venue speed policy.
