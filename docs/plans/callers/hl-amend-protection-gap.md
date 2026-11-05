# Hyperliquid amend prerequisites before cancellation

SDK direct, native/hosted and frame-fed Hyperliquid engines finish identity
resolution and the existing fresh orderStatus sizing read before cancelling an
amended order. Both reads share an exclusive absolute deadline five seconds
after command receipt. A reply or physical cancel claim at the deadline or later
refuses NotSent,
regardless of timer callback order; a cancel released in time still completes its
replacement after that deadline. Failure or timeout refuses NotSent with the
original protective order untouched. Partial fills still reduce the replacement
to the freshly read remainder.

Both identity and sizing continuations retain the opaque engine preflight owner
and original deadline. Retry and deadline timers carry that same binding. A
retired generation cannot resolve identity, issue another read, change sizing or
clear a successor's wait, even for the same native order ID. Queued retired reads
are discarded before HTTP; late success and failure only retire their own read.

Source account bootstrap already warms maxBuilderFee and exports the approval
to frame-fed engines. Each amend captures its optional builder pair before its
cancel. Unknown approval chooses the existing permitted no-builder fallback
without poisoning the account cache. A late approval applies to future orders,
not this amendment; a later failed/refused approval omits the captured fee.
Neither a fee query nor its eight-second waiter runs between cancel ACK and
replacement. Standalone placements retain their existing optional fee wait.
This supersedes the amendment portion of the historical builder-wait description
in RUST_HYPERLIQUID_TRADING_CONTEXT.md; it adds no mandatory fee or approval API.

Native custody, same-order sequencing, nonces, no write replay, the 334 ms
mutation pacing and transport deadlines stay in force. Cancel or replacement
network failures retain their existing uncertain/unprotected outcomes; this
change removes the avoidable optional-fee gap, not physical network latency.

The existing host transport.http.timing derives its closed stage once from the
final bound packet, after native signing materializes the body. It gains labels
for identity, builderApproval, orderStatus, cancel, place and modify. Prerequisite
reads emit individual queued/wire samples; other info reads keep the minute
aggregate. No URL, request body, account address, order identity or key is logged.
These samples measure host preparation/admission and wire time, not native FIFO
time before the request is prepared. PR-B/C own further admission/intent timing.

No NX edit or host rollout is required for the SDK behavior. Family and combined
size qualification belongs to CI; any CI-measured cap adjustment must keep the
Hyperliquid and Hyperliquid Spot siblings equal. No merge or deployment is part
of this change.
