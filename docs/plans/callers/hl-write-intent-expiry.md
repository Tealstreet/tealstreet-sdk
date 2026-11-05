# Hyperliquid queued write intent

SDK native, worker and hosted callers must supply the effective caller wait
through the existing engine command receipt. Native Event::Command uses the
engine's existing 120 s default; EngineHost forwards its configured timeoutMs.
No new host cancellation or signed-wire field is introduced.

Hyperliquid captures the remaining wait once in genuine preflight custody,
floored to whole milliseconds by CommandWait. Queued jobs park with that bound.
Builder and pre-cancel prerequisite waits retain its remainder. At dequeue and
at the first physical HTTP Claim the same exclusive deadline is checked, so
delayed timer delivery cannot authorize a late send. The original timer also
revokes a held first binding independently of a wall-clock rollback. A timer
delivered after physical release leaves the response and continuation intact.
Expiry before any physical write emits one typed Timeout/NotSent and retires
only that owner. Late callbacks
or duplicate expiry timers cannot reactivate it or retire a peer.

The caller wait does not revoke an already released mutation or invent a resend.
Its response window and owned serial continuation remain valid. In particular,
an amend whose cancel has landed retains replacement custody after the original
caller timeout; expiring that continuation would leave the order unprotected.
PR-A #3098 still resolves and bounds all amend prerequisites before the cancel.

Only queued independent cancels and protective placements receive priority.
Promotion requires disjoint markets and native/client order identities from
every older job; same-market work and scheduleCancel remain barriers. An active
owner is not preempted, and global nonce custody and 334 ms mutation pacing are
unchanged. This does not promise parallel writes or priority over an already
active builder/read/response wait.

Per-order transport.http.timing remains the host timing seam: accepted-to-prepare
includes native queue time, and the existing final-Bind stage identifies writes
and prerequisite reads. There is no additional private telemetry or rollout.
Family and combined size are qualified by CI; any Hyperliquid cap change keeps
Hyperliquid Spot equal. No old caller boundary is removed by this change.
