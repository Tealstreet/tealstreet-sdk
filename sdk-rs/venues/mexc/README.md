# MEXC socket liveness

Private sockets use `safe_cex_engine::socket::Supervisor` with a ten-second
connection deadline and a separate ten-second authentication deadline. Native
heartbeats run every 8 seconds; 20 seconds without traffic closes the socket.
Pongs do not extend authentication. Rejected login closes and retires the socket
before scheduling capped exponential backoff with jitter. Stable authenticated
traffic resets backoff; merely opening a socket does not.

The existing public socket policy remains separate. Deterministic lifecycle
regressions are in `tests/socket_liveness.rs`; private replay tapes provide
scripted evidence, not live account connectivity evidence.

Private read refusals are decoded from success:false before list validation.
Startup, background account reads, identity, leverage and history retain the
native code and a bounded native message. Codes 401/602 are invalid-key or
signature refusals, 402 is expiry, and 406/511/701–704 are permission refusals;
background diagnostics use auth.rejected. A malformed envelope, an HTTP
status alone, or error-looking text cannot prove a key refusal.

MEXC's contract error table identifies 513 as the request-time-window error
and 602 as signature verification failure:
https://mexcdevelop.github.io/apidocs/contract_v1_en/#error-code-example
https://www.mexc.com/api-docs/futures/integration-guide

Only a current signed GET receiving 513 without data may issue one unsigned
GET /api/v1/contract/ping, then replay that same GET with the same selected
credential and original ownership bound. The server sample must be a positive,
safe integer from a successful envelope. Failed, malformed or late samples do
not calibrate or replay. A second 513 ends that attempt. Writes and 602 never
enter clock recovery. Signing already takes fresh protocol-clock milliseconds
at dispatch; calibration changes outbound timestamps, not engine deadlines.

## Protective edits

Known-class price edits use MEXC's native plan/TP-SL modify endpoints without a
cancel fallback. Quantity edits inspect the original, guard the destructive send
with a genuine fresh trigger reference, and require a terminal cancellation read.
Only a standalone stop with exact native provenance permits one put-back after a
definitive refusal; unknown outcomes never restore or resend. See
[`mexc-protective-edits.md`](../../../../docs/plans/callers/mexc-protective-edits.md)
for the endpoint classes, legacy signer-window limit and native controls.
