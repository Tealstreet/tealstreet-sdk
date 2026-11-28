# WEEX write uncertainty

The engine seals dispatched HTTP 429, 408 and 5xx writes as unknown before
native authentication/refusal codes can override the outcome. Native code
`429` inside HTTP 200 is also unknown, in both envelopes and per-order rows.
These rate failures direct the caller to Check Open Orders while retaining
native diagnostics; they never authorize a resend or protective put-back.
Reads and documented refusals on non-ambiguous statuses keep their existing
classification.

Venue-decoded Unknown replies report `We couldn’t confirm this change. Check your open orders before trying again.`
in the terminal failure and correlated command diagnostic, and emit one
`order.outcome_unknown` event. The indexed journal retains accepted IDs, the
uncertain elements and the unsent suffix. It observes the existing dispatch
policy; payload sizes and write order still follow the legacy TypeScript.

Engine-sealed and venue-decoded Unknowns share the NX #9262 telemetry projection:
`command` is a `weex.<command ID>` label, `outcome` is `unknown`, and `request`
is the released decimal engine request ID. `placedOrderIds` and `unknownIndices`
are CSV strings capped at 16 entries, drawn only from indexed Placed and Unknown
journal entries. Native IDs must contain 1–32 ASCII digits; indices must fit
`u16`. Missing journals produce empty lists. Cancellation targets and client IDs
are excluded. Numeric `status` is included only for HTTP statuses 100–599.
No raw diagnostic, URL, endpoint, query, body or signed payload is projected.
Native bulk-cancellation telemetry waits for both independent families and yields
to engine-owned Unknown publication, so the command emits once. Placed-ID lists
also stop at a whole ID before the engine's 512-byte string cap.

Orders without a caller client ID send one rounded native payload even above
the market maximum, matching legacy `weex.exchange.ts:1041,1112`; the venue's
refusal is surfaced without splitting or retrying. Caller IDs retain the TS
base class's whole-batch maximum-size preflight (`base.ts:1211-1223`).

## Stop-entry brackets (AGT-115)

`/capi/v3/placeTpSlOrder` has no tp/sl fields, so a stop or take-profit entry dropped its
attached legs. `plan_orders` now refuses the whole command before any write with the engine's shared `stop_entry_legs_refusal` copy. A
`conditional` entry still carries them as `presetStopLossPrice`/`presetTakeProfitPrice`.
