# Unknown plain cancels on frame-fed writers

An Unknown plain cancel keeps its original command error and never replays.
It does not create a placement fence or turn an order into Canceled. For a
venue advertising `Venue::refreshes_orders`, the engine emits one
`orders.refresh { symbols }` source ask independently of `orders.held`.
Symbols are comma-separated; empty means the account. An unscoped command
asks for the account. A rate/credential deferral retains one bounded pending
ask, merges scopes, and respects the latest deferral. More than 64 names
coalesce to the account.

The SDK frame-fed adapter relays that event to `refreshAccountOrders` through
the existing worker/server bridge. It leaves any placement-hold grace timer
intact. The source's existing `RefreshOrders` scheduler bounds refresh work
and owes one successor when its current read predates the ask. Bybit now
retains an ordinary census in flight and sets its existing follow-up flag,
instead of abandoning that census and starting a competing physical batch.
Read epochs, cover validation, and malformed/partial-read rules are unchanged.

BingX and OKX also join an ordinary census when this source ask is pending.
Each retains one owed successor; starting a fresh census consumes that demand.
BingX keeps the successor Primary-signed, including when its normal poll
recovers a failed old read. OKX keeps the source debt separate from the
balance/positions re-read flag and preserves its existing failure retry wait.
Both production controls failed before these joins: an ask started a competing
ordinary census. One hundred asks now retain the original census and produce
one fresh successor, whose send supplies the account cover.

Callers forwarding engine events must retain this control event even when
the cancel's Unknown error has already been displayed. No caller should
resubmit the cancel or treat a later absence as proof that this cancel caused
it. Existing frame-only callers that discard engine events cannot request
the source refresh.

The real Bybit frame-fed regression failed on master with zero asks instead
of one. The mirror bridge regression independently failed before its event
handler existed. A real Bybit source/page control joins a pre-cancel census,
then requires one distinct successor and preserves the original Unknown.
Confirmed/rejected cancels, no replay, source read covers, and independent
placement holds have preservation controls. Bitget native target confirmation
is a separate slice after #3057; this change makes no native presence/absence
claim and does not alter cached algorithm retention.
