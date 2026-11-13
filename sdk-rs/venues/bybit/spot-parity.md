# Spot split and batch parity

Spot follows the legacy adapter in
`packages/safe-cex/src/__tests__/reference/bybit/bybitspot.exchange.ts`.
At lines 828–841, an oversized amount uses `ceil(amount / max)` equal main lots
and appends its rounded remainder. Main lots are rounded independently; this can
produce an undersized remainder or a rounded total different from the request.
The formatter sends these values without whole-plan lot validation.

TP/SL is attached to the first main lot and the nonzero remainder. Later main
lots omit it, matching lines 833–841. Existing native Spot execution fields remain.

Caller IDs and execution controls are checked for the whole command. Other
formatting refusals occur in caller order: a later missing market does not erase
an earlier sent order (lines 881–887). A definite native lot refusal permits the
rest of that caller order's lots, then stops later caller orders (lines 844–870).
Caught provider refusals retain their existing engine-owned handling.

The recorded stop-after-UNKNOWN exception (Sean/daxx, 2026-11-08) overrides TS's
payload catches at lines 844–856: Rust stops the action after genuine uncertainty,
with its placed-prefix evidence and the canonical Open Orders instruction.
The existing engine journal and CommandFailed telemetry retain those facts.

These partial-placement and protection-attachment behaviors are known legacy
issues, recorded under `bybit-spot` in the owner's known-issues report. This change
restores parity rather than introducing new admission rules.

KNOWN-ISSUE: legacy TS under-allocates (sum 0 of requested 0.002); kept for exact TS parity per Sam's decision.
KNOWN-ISSUE: legacy TS under-allocates (sum 2499.999 of requested 2500); kept for exact TS parity per Sam's decision.
KNOWN-ISSUE: legacy TS under-allocates (sum 4 of requested 5); kept for exact TS parity per Sam's decision.
The recording pins assert both the individual payloads and these totals.

Explicit cancellations retain one passive outcome per caller occurrence, including
repeated order IDs and unsent tails. Confirmed cancellations remain applied effects,
never created IDs. HTTP-envelope failures, transport loss and request/plan expiry
retain that accounting without granting another cancellation or retry.
