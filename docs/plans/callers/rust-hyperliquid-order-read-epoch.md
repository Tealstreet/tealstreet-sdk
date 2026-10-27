# Hyperliquid and Hyperliquid Spot order-read epochs

Both Rust venues capture `Cx::order_read_epoch()` when a logical Orders census
is created, before the weighted account-read queue or managed HTTP preparation.
The continuation retains that epoch through dispatch and response reduction.
A clean complete census publishes Store orders before calling
`orders_reconciled_since` for that census's DEX symbols. Global release remains
limited to a sole, fully discovered base DEX; epochs are never joined across DEXes.

An orders read queued before an uncertain placement cannot clear that placement's
fence even if its HTTP Context is admitted after the write. Reconcile re-asks
join the existing read without refreshing its epoch. A seal owes one successor;
that genuinely new census captures its own epoch and can release the fence after
clean publication. Invalid, lost or revision-obsolete snapshots keep the existing
retry and publication rules.

SDK, NX and server callers need no API or sequencing change. This closes the
venue-queue gap in the earlier Hyperliquid uncertain-placement fence adoption.
The production-binding tests use real weighted managed reads and the emitted
weight timer, a signed lost placement, a refused retry after the queued census,
and a successful retry after the fresh successor. They establish native engine
behavior, not browser transport cleanup or a legacy-engine replacement handoff.
Hosted signing remains post-beta.
