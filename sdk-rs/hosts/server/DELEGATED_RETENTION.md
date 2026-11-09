# DELEGATED retention

Enabled Bybit DELEGATED accounts keep running for 48 hours after their last
admitted client disconnects. The session resume ID still has its normal 60-second
grace. After that grace a client uses a fresh verified handover to join the same
running account. Pending or refused handovers do not renew retention.

Any admitted client cancels the account's outstanding retention deadline. When
the final client disconnects again, a new 48-hour interval begins. Another user,
another account incarnation, and an expired resume ID cannot renew that interval.

Expiry stops the account's algos and records `no client for 48 h`, with disposal
kind `no_client_48h`. The existing retained stop-notice path tells the next
admitted client. The resident signing material is erased at disposal, including
when engine or host references remain. Removal or disabling through the existing
`dispose` operation takes effect immediately, including when another client is
attached. LOCAL and COSIGNED accounts retain their existing lifecycle.

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
