# Hyperliquid /info reads ahead of candle and history reads

The shared Hyperliquid `/info` budget (`hyperliquidInfoBudget.ts`, about 1000
weight a minute across workers, pages and tabs) was strictly FIFO. Fence and
amend reads (`orderStatus`, `frontendOpenOrders`, `userRole`) queued behind
`candleSnapshot` (104) and history pages (120) and could miss their deadline:
"Hyperliquid amendment not sent: the order read did not answer".

- Candle and history reads are deferrable. They leave 120 weight of the bucket
  to every other read, and in the cross-tab dispatch queue the oldest
  non-deferring waiter goes first; deferrable reads keep FIFO among themselves.
- No starvation: a deferrable read that has waited 10 s competes in arrival
  order and may use the full bucket, exactly as every read did before.
- The total is unchanged: priority only reorders, it adds no capacity.
- `request.failed` from Hyperliquid account reads now carries `readKind`
  (`role`, `mode`, `state`, `orders`, `spot`, `asset`, `builder`, `fees`,
  `amendSource`, `amendRole`, `discovery`) beside the existing
  `lastFailedPrivateReadKind`.

Callers:

- NX (separate PR): Sentry keeps an engine event field only through
  `FIELD_RULES` in `apps/web/src/utils/sentry/engineEventTelemetry.ts`. Add
  `readKind` as a closed set of the values above, with tests that a listed
  value is kept and an unlisted one is dropped.
- Mixed tabs during a deploy: a dispatch row from an older tab has no
  deferrable flag and is treated as a priority read, so the old FIFO holds.

Deletion prerequisites: none added or cleared.
