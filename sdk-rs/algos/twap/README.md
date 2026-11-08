# Shared TWAP validation

Before admitting a schedule, TWAP materializes all remaining quantities on the
native amount grid, carries each slice's dust forward, and checks every resulting
write. A quantity plan that loses total size or violates a native minimum,
maximum or grid is refused before any slice or chaser starts. Restore and resume
use the same gate. Typed starts return `VenueError` with `invalidInput/notSent`;
the existing hosted operations continue emitting their refusal as an error log.

`plan::validate_write(&PlannedOrder)` is pure and matches the shared engine
`WritePlan` validator signature. Its rules include the final limit tick and an
adapter-supplied notional value per amount unit, so contract conversion, price
references and close exemptions stay native. Native market facts come from the
account store, never the caller's `activeMarket` object. Missing markets refuse.

The current account store supplies quantity bounds and precision only. It does
not expose venue-specific minimum notionals; the factory cannot populate those
rules yet. The notional gate requires the native adapter to supply both its
minimum and reference value. This validation phase does not complete the
[split-plan invariant](../../../docs/plans/SPLIT_PLAN_INVARIANT.md): production
notional wiring and the shared `WritePlan` outcome journal are pending.

Prices and fills can change after admission. Each scheduled release rechecks
the remaining plan with the current view and stops on predictable invalidity.
Existing uncertain placements are never replayed on restore. Runtime outcome
reporting will use the shared journal; TWAP has no local replacement journal.
