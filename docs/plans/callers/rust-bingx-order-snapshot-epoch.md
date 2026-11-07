# BingX order snapshot freshness

BingX account reads use the Store's `order_snapshot_epoch` instead of a venue
`orders_revision`. This changes the freshness implementation from class D to B.
SDK callers keep the same commands, credentials, routing and reconciliation API.

Each open-orders read captures one `OrderSnapshotToken` before managed admission.
The single-request census carries it through signing and response streaming.
Publication and native baseline/observation updates run inside
`commit_orders_snapshot`; only a clean committed census may call
`orders_reconciled_since` with its original `order_read_epoch`.

The seven existing invalidation sites remain exactly the same: forced uncertain
reconciliation, private-session invalidation, private open, validated stream
trade progress, other admitted stream order updates, trade response effects and
trade command entry. Each former revision bump calls
`invalidate_order_snapshots`. Ordinary snapshot publication adds no invalidation.

Read reservation identity uses a checked issue sequence, never publication
authority. After a clean commit, a separate unconsumed Store witness suppresses
unnecessary replacement reads only while that witness remains current. New
issues retain that witness; only a clean complete commit replaces it. Signing
failures still release only their own reservation. Store epoch or read-identity
exhaustion permanently refuses further
reads instead of wrapping; no unfenced read is sent.

Production-binding controls cover a read crossing an explicit Store write fence
(RED on the parent), accepted cancel followed by an older read (preservation,
already GREEN on the parent), normal publication and sibling Store publication.
An incomplete later read preserves the last clean witness, suppressing a
redundant refetch when an older stale response arrives (RED before the witness
retention correction; preservation on the parent).
Existing forced-reconciliation, sign-failure ownership, malformed-row retention
and native financial controls stay in place. The explicit Store-fence RED does
not claim that the parent's accepted-write revision fence was broken.

No caller migration, inventory allowance or deletion prerequisite is added.
Landing requires reviewer CLEAR, correctness/bans/family/replay CI and exact-tree
auditor GO. This slice does not change socket policy or financial conversion.
