# Hyperliquid write outcomes

Hyperliquid writes follow the legacy TS serial/grouping behavior. The shared
WritePlan records outcomes and has no validation or dispatch authority.

Stop-after-UNKNOWN is a recorded parity exception (Sean/daxx, 2026-11-08;
doc #3355). A genuine Unknown halts further physical writes for that action,
including replacement after an uncertain cancellation. The engine duplicate-write
fence remains authoritative. Unknown never triggers a retry or protective put-back.
Definite refusals retain the TS behavior; an already-terminal perp cancellation
can still proceed to replacement.

Protective put-back kept per Sam's decision, SPLIT_PLAN_INVARIANT.md exception 2
(doc #3355). After a definite full-position replacement refusal, master's existing
policy attempts the captured original protection once with a fresh identity.
Source, fill, position, newer-protection and fresh-mark checks remain unchanged.
The journal records this recovery without controlling dispatch.

Unknown publications retain placed native IDs and indexed Unknown/NotSent rows.
The terminal error, Unknown elements and `order.outcome_unknown` telemetry carry
`The outcome is unknown. Check Open Orders`. The literal is intentionally copied
from the engine under the lead's venue-local wording rule. Telemetry identifies the
command and lists actual placed IDs and unknown indices as comma-separated fields.

Recording tests in `after_failure_parity.rs`, `write_plan_preflight.rs` and
`spot.rs` cite the TS continuation sites and pin this exception, accepted prefixes,
standard wording, journal rows, telemetry and absence of later writes.

Engine-sealed HTTP503 Unknown retains halt, prefix IDs, journal and canonical
message assertions. Telemetry for that path awaits a later engine hook; its
telemetry assertion alone is deferred. Other Unknown pins require telemetry.

## Cancellation reporting

Decoded failed cancellation batches retain an indexed logical journal beside
the engine's native classification and partial metadata. Confirmed rows report
`applied` with effect `cancel` and their existing target IDs. Unproven rows
report `unknown` with the native diagnostic. Cancel targets remain absent from
created-order IDs; an empty created-ID list does not mean no cancellations applied.

A decoded Unknown row publishes `The outcome is unknown. Check Open Orders`
and one independent `order.outcome_unknown` event correlated by `commandId`.
Native error text remains available in `venueMessage`. The journal and diagnostic
do not authorize dispatch, retries, restoration or store changes.
Decoded cancellation facts settle the command's passive journal before terminal
publication; one correlated event retains the actual Unknown indices and placed IDs.

`tests/support/beta_batch_outcomes.rs` records the real cancel payload, checks
store effects and empty created IDs, and pins journal, terminal guidance and
diagnostics independently with the Unknown row first, middle and last. Confirmed
and already-gone cancellations remain successful without Unknown diagnostics.
