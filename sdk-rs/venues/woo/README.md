# WOO X socket liveness

Private sockets use `safe_cex_engine::socket::Supervisor` with a ten-second
connection deadline and a separate ten-second authentication deadline. Native
heartbeats run every 6 seconds; 20 seconds without traffic closes the socket.
Pongs do not extend authentication. Rejected login closes and retires the socket
before scheduling capped exponential backoff with jitter. Stable authenticated
traffic resets backoff; merely opening a socket does not.

The existing public socket policy remains separate. Deterministic lifecycle
regressions are in `tests/socket_liveness.rs`; private replay tapes provide
scripted evidence, not live account connectivity evidence.

Ordinary orders and sized stops round the requested amount to the market step
before splitting into balanced native lots, matching the TS reference. An amount
of `1.0005` at step `0.001` becomes `1.001`; a native maximum of `0.3` produces
`[0.251, 0.25, 0.25, 0.25]`.

Multi-write placements keep an indexed reporting journal for every planned lot.
An uncertain lot retains accepted native IDs, marks the untouched suffix as
unsent, and reports `We couldn’t confirm this change. Check your open orders before trying again.` in the failure
and one command diagnostic, together with one `order.outcome_unknown` event.
The command stops after that lot without retrying or sending the suffix.
An accepted protective-parent cancellation records its cancel effect and target
parent ID in the journal before any replacement outcome.

## Stop-entry brackets (AGT-115)

Legs are placed as a separate positional TP/SL only after a limit/market entry. Stop,
take-profit (sized or whole-position) and trailing entries used to drop them;
`validate_places` now refuses the whole command before any request with the engine's shared `stop_entry_legs_refusal` copy.

## Unknown-outcome telemetry

Unknown-outcome telemetry uses the optional static engine hook and a common
closed projection for WOO's existing venue-decoded failures. The legacy request
driver is unchanged. Fields match NX #9262: `command` is a bounded `woo.<id>`
label, `request` is the decimal internal request ID, `outcome` is `unknown`,
`placedOrderIds` contains at most 16 numeric IDs (1–32 digits), and
`unknownIndices` contains at most 16 indices in 0–65535. `status` is omitted
unless it is an integer HTTP status in 100–599. Lists also fit the engine's
field byte cap, so logging cannot truncate them into an invalid NX value.
The projection never copies URLs, endpoints, queries, signed bodies or native
error messages. Native independent leg failures emit only at command completion.
Multi-lot placements keep the split journal's existing `order.outcome_unknown`
event instead of this projection, so each command still emits once.
