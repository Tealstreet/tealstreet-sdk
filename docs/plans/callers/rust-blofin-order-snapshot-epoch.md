# BloFin order snapshot freshness

BloFin's pending-order census uses the Store's `order_snapshot_epoch` instead of
its venue-local `orders_revision` (class D to B). SDK callers keep the same
commands, credentials, request cadence and reconciliation API.

Capture one `OrderSnapshotToken` before the first managed census request. Move
that same token through every normal, TP/SL and algo page; later pages never
renew it. The existing row/source merge runs synchronously inside
`commit_orders_snapshot`, using its current-row argument. No snapshot publication
invalidates sibling snapshots. A stale whole census keeps its existing bounded
retry and owed-reconciliation behavior.

Exactly seven old revision-bump sites now call `invalidate_order_snapshots`: the
existing private orders batch, place preparation, cancel preparation, retired
trade cleanup, released placement failure, rejected/unproven placement answer
and each accepted or absent cancel item. No new invalidation site is added.
Store epoch exhaustion refuses the read permanently before setting busy state
or consuming owed reconciliation; no unfenced request is sent.

Keep the first-issuance `order_read_epoch`, clean-row requirement, Primary
reconciliation selection, uncertainty generation and re-ask join guards. Only a
clean committed census with the original matching uncertainty generation may
call `orders_reconciled_since` with its original read epoch.

Startup cumulative execution-prefix evidence remains before the freshness gate,
exactly as before: complete cumulative rows can prove execution prefixes even
when newer WS state makes row publication stale. That path emits its existing
fills and diagnostics; it is independent of the guarded order-row/source merge.
Its reducers and all old financial fixtures remain unchanged.

Production-binding controls pin the Store write-fence crossing before any
response, a crossing between pages, normal atomic multi-family publication and
an accepted-cancel crossing. The first two fail on the parent at publication
assertions; the accepted-cancel and normal-publication controls already pass and
are preservation evidence. The stale-read control also drives the actual retry
timer and proves that a new census can publish.

No caller migration, inventory allowance or deletion prerequisite is added.
Landing requires reviewer CLEAR, correctness/bans/family/replay CI and exact-tree
auditor GO. This slice does not change socket policy or financial conversion.
