# Store order fence domains

Owner: codex-uufub4. Status: documentation plan; no runtime change.
Audited landing: `6c7f4fcf292d01b8c056127a070783e3fd72dcd2`.

## Decision

Keep `Store::orders_revision` and `Store::order_snapshot_epoch` as separate
invalidation domains. Do not merge their advancement rules. This closes the
design choice requested after integrating #1741 and #1759; documentation
integration remains pending.

The mechanisms landed separately:

- #1741: `b2fe97324763e1e2702c69ac5b839da37102928b`.
- #1759: `9bf754a01149d373c522728edbebb46abc5ae5b2`.

`RUST_FINANCIAL_SNAPSHOT_FRESHNESS.md` already distinguishes the domains.
The follow-up must make that distinction explicit beside the Store fields and
their callers, rather than leave two apparently interchangeable revisions.

## Why both stay

| Domain | Advances on | Consumer and required behavior |
| --- | --- | --- |
| `orders_revision` | Every `set_orders`/`upsert_order`, an existing-row update, or a removal that changes the rows | Managed write release captures the revision. `Reconciliation::new` denies a late ACK patch if any newer Store order mutation intervened. |
| `order_snapshot_epoch` | Explicit `invalidate_order_snapshots` at native write boundaries, including ACKs with no existing row | A logical read captures an issuer-bound `OrderSnapshotToken`. Publication checks it before native-cache and Store effects; ordinary WS row updates and sibling snapshot publication do not invalidate it. |

A broad counter used for both would invalidate a logical REST snapshot on each
WS row update or sibling endpoint publication. That defeats Bitunix's deliberate
row admission and snapshot progress policy. A selective counter used for both
would allow a late ACK to overwrite a newer WS row or completed snapshot when
no explicit write-boundary invalidation occurred. An absent-row write ACK can
also invalidate a snapshot without changing Store rows at all.

These are different predicates, not duplicate implementations of one ordering.
The additional HTTP write-release revision used by late reconciliation remains
unchanged; neither Store domain replaces it. Saturated broad revisions already
deny late patches, and selective Epoch exhaustion permanently denies tokens.

## Bounded follow-up

1. Add short comments beside the two Store fields/getter and link this decision
   from the existing financial-freshness and RequestContext integration notes.
2. Check existing controls for the four crossings below. Add a focused control
   only for a missing invariant; preserve current native admission policies.
3. Review with the financial snapshot and late-reconciliation owners. Carry the
   explicit keep-both rationale in the documentation PR; no venue migration,
   new revision, allocator, schema, or group-completion work belongs here.

| Crossing | Required result |
| --- | --- |
| WS row update after write release | Late ACK patch refused; snapshot token remains current. |
| Sibling logical snapshot publishes | Late ACK patch refused; the other endpoint's snapshot token remains current. |
| Explicit absent-row write invalidation | Snapshot token becomes stale without requiring a Store row mutation. |
| Another Store or exhausted epoch | Snapshot token refused; exhaustion never permits saturated equality. |

Runtime convergence is not proposed. Sharing a counter implementation in the
future would still require two independent domains and is outside this plan.
