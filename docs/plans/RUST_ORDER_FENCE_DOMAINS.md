# Store order fence domains

Owner: codex-uufub4; follow-up claude-fe7lgm. Status: follow-up done; no runtime change.
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
| `order_snapshot_epoch` | Explicit `invalidate_order_snapshots` at venue-selected boundaries, including absent-row ACKs and Binance's conservative stream/subscription paths | A logical read captures an issuer-bound `OrderSnapshotToken`. Publication checks it before native-cache and Store effects; Store row mutations and sibling snapshot publication do not themselves advance this fence. |

A broad counter used for both would invalidate a logical REST snapshot on each
WS row update or sibling endpoint publication. That defeats Bitunix's deliberate
row admission and snapshot progress policy. A selective counter used for both
would allow a late ACK to overwrite a newer WS row or completed snapshot when
no explicit write-boundary invalidation occurred. An absent-row write ACK can
also invalidate a snapshot without changing Store rows at all.

Preserve each venue's explicit triggers. Binance calls
`invalidate_order_snapshots` from `on_order_update` and `on_spot_order` on WS
ingress; Bitunix deliberately allows healthy WS peers through row admission.
The Store mutation API alone implies neither policy.

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
| Store row update after write release, without explicit invalidation (the Bitunix crossing) | Late ACK patch refused; snapshot token remains current. |
| Sibling logical snapshot publishes | Late ACK patch refused; the other endpoint's snapshot token remains current. |
| Explicit absent-row write invalidation | Snapshot token becomes stale without requiring a Store row mutation. |
| Another Store or exhausted epoch | Snapshot token refused; exhaustion never permits saturated equality. |

Runtime convergence is not proposed. Sharing a counter implementation in the
future would still require two independent domains and is outside this plan.

## Follow-up status

Done:

- Comments beside both Store fields and the `orders_revision` getter.
- Links from RUST_REQUEST_CONTEXT.md and RUST_FINANCIAL_SNAPSHOT_FRESHNESS.md.

Crossing controls:

- Crossings 1-3: `late_ack_and_snapshot_fences_cross_independently`
  (http_lifecycle.rs) drives a released write's late ACK through `Reconciliation`
  after each crossing. A row update or a sibling snapshot publish refuses the
  patch and leaves a held token current. An explicit invalidation stales the
  token and still lets the patch apply.
- Crossing 4 was already pinned by
  `order_snapshot_admission_rejects_before_native_rows_readiness_and_dirty_state`
  (store.rs): a foreign Store's token, an exhausted epoch, an empty token set,
  and no begin after exhaustion.

## Venue convergence (overseer ruling, 2026-10-27)

Nine venues fence order reads with a hand-rolled `orders_revision`. Each move
onto `order_snapshot_epoch` is a one-for-one swap:

- `begin_orders_snapshot` at read send;
- `commit_orders_snapshot` at publish;
- `invalidate_order_snapshots` at every existing bump site, adding none.

The local counter already fences the simple crossing, so each PR carries a
production-binding preservation control that passes before and after. It
carries a RED only where its owner finds a crossing the local fence misses.
The epoch exhausts permanently where a local counter wrapped or saturated. That
is accepted: it is a u64 and unreachable.

- Beta venues, in order: BingX, then BloFin (owner codex-mxb4mq), then Kraken
  (owner codex-9czdjh). Kraken's WS census keeps its additive fallback:
  `orders_snapshot_current` picks the atomic replace when current and today's
  additive apply when stale or exhausted. A successful census commit keeps its
  existing invalidation.
- Deferred until after beta, since they are not in the beta roster: bitrue,
  weex, txflow, grvt, and coinbase, including Coinbase's pure-read-fence check.
- Excluded: Hyperliquid. Its revision is scoped per dex, while the snapshot
  epoch is Store-global, so moving it would invalidate sibling dexes' reads.
