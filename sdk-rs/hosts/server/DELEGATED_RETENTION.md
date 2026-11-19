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

## Interim proxy-authority limit

For a retained account whose resolved transport needs the socket-issued proxy
authority, that token is not renewed or extended by retention. On its expiry the
interim maintainer policy is a terminal actor stop and immediate key erase, with
recorded reason `proxy-token-expired`. Expired/missing authority cannot silently
fall back to a direct route. Direct accounts keep the independent 48-hour clock.
An admitted reconnect cancels the old timer and supplies its own checked token;
a later final disconnect starts a fresh interval under that authority.

**Open decision for Sam:** issuer-backed retained-token renewal versus a permanent
earlier-stop contract. The interim fail-closed guard chooses neither long-term
policy and makes no full-48-hour availability promise for a proxied account.
