# BingX public subscription demand

The SDK, NX and server may change subscriptions while connecting or while an
interest replacement is queued. BingX must retain the complete subscriber model,
including duplicate book/candle references and add/remove reversals. Use the
engine's existing full Desired-set FIFO on every admitted change. No venue-local
proposal queue, pending-change slot or open-only gate is needed. Synchronous
admission failure reports and rolls back only that change; asynchronous capacity
keeps the latest model, retires the affected public epoch and rebuilds on Ready.
Private refusals cannot touch public demand. A failed Ready proposal must not
discard a later admitted public change. Permanent errors stay affected-feed
fail-closed; financial outcomes and private listen-key policy do not change.

Production-bound controls first cover all three BloFin review shapes. Existing
BingX Ready reproposal protects pre-open demand; account-feed routing protects
against private refusals. Meaningful negatives must isolate the real pending
change/refcount loss and public opening-refusal ownership loss. Capture actual
feeds, epochs and proposal callbacks, without manufactured authority. Preserve
all existing financial fixtures and wire reducers.

This beta-release gate clears only after the REDs pass on the source fix, scoped
native/strict/fmt/bans and current CI family/replay pass, reviewer CLEAR and
8x5tm1 exact-tree GO. Record this obligation only in this fragment; no legacy
inventory or shared-engine change is needed. 55ao7t confirmed no new beta API
seam is required; internal engine coalescing would remain compatible.
