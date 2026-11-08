# Hyperliquid cancellation reporting

Decoded failed cancellation batches retain an indexed logical journal beside
the engine's native classification and partial metadata. Confirmed rows report
`applied` with effect `cancel` and their existing target IDs. Unproven rows
report `unknown` with the native diagnostic. Cancel targets remain absent from
created-order IDs; an empty created-ID list does not mean no cancellations applied.

A decoded Unknown row publishes `The outcome is unknown. Check Open Orders`
and one independent `order.outcome_unknown` event correlated by `commandId`.
Native error text remains available in `venueMessage`. The journal and diagnostic
do not authorize dispatch, retries, restoration or store changes.

`tests/support/beta_batch_outcomes.rs` records the real cancel payload, checks
store effects and empty created IDs, and pins journal, terminal guidance and
diagnostics independently with the Unknown row first, middle and last. Confirmed
and already-gone cancellations remain successful without Unknown diagnostics.
