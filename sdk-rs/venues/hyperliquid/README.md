Hyperliquid writes follow the legacy TS serial/grouping behavior. The shared
WritePlan records outcomes and has no validation or dispatch authority.

Stop-after-UNKNOWN is a recorded parity exception (Sean/daxx, 2026-11-08;
doc #3355). A genuine Unknown halts further physical writes for that action,
including replacement after an uncertain cancellation. The engine duplicate-write
fence remains authoritative. There is no automatic retry or protective put-back.
Definite refusals retain the TS behavior; an already-terminal perp cancellation
can still proceed to replacement.

Unknown publications retain placed native IDs and indexed Unknown/NotSent rows.
The terminal error, Unknown elements and `order.outcome_unknown` telemetry carry
`The outcome is unknown. Check Open Orders`. The literal is intentionally copied
from the engine under the lead's venue-local wording rule. Telemetry identifies the
command and lists actual placed IDs and unknown indices as comma-separated fields.

Recording tests in `after_failure_parity.rs`, `write_plan_preflight.rs` and
`spot.rs` cite the TS continuation sites and pin this exception, accepted prefixes,
standard wording, journal rows, telemetry and absence of later writes.
