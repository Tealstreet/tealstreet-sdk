# Hyperliquid amend prerequisites before cancellation

SDK direct, native/hosted and frame-fed Hyperliquid engines finish identity
resolution and the existing fresh orderStatus sizing read before cancelling an
amended order. Both reads share the existing five-second amend bound. Failure or
timeout refuses NotSent with the original protective order untouched. Partial
fills still reduce the replacement to the freshly read remainder.

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

The existing host transport.http.timing gains only closed stage labels for
identity, builderApproval, orderStatus, cancel, place and modify. Prerequisite
reads emit individual queued/wire samples; other info reads keep the minute
aggregate. No URL, request body, account address, order identity or key is logged.
These samples measure host preparation/admission and wire time, not native FIFO
time before the request is prepared. PR-B/C own further admission/intent timing.

No NX edit or host rollout is required for the SDK behavior. Family and combined
size qualification belongs to CI; any CI-measured cap adjustment must keep the
Hyperliquid and Hyperliquid Spot siblings equal. No merge or deployment is part
of this change.
