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
unsent, and reports `The outcome is unknown. Check Open Orders` in the failure
and one command diagnostic, together with one `order.outcome_unknown` event.
The command stops after that lot without retrying or sending the suffix.
An accepted protective-parent cancellation records its cancel effect and target
parent ID in the journal before any replacement outcome.
