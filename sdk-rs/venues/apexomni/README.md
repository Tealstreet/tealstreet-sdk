# ApeX Omni socket liveness

Private sockets use `safe_cex_engine::socket::Supervisor` with a ten-second
connection deadline and a separate ten-second authentication deadline. Native
heartbeats run every 5 seconds; 30 seconds without traffic closes the socket.
Pongs do not extend authentication. Rejected login closes and retires the socket
before scheduling capped exponential backoff with jitter. Stable authenticated
traffic resets backoff; merely opening a socket does not.

ApeX keeps the production-compatible login → immediate three-channel subscribe
→ ping ordering. A successful login acknowledgement or a validated native
`ws_zk_accounts_v3` snapshot/delta establishes readiness, including wallet-only
updates for flat accounts. Wallet proof requires its account ID, supported token
and finite balance; supplied pending amounts must be nonnegative. This proof
does not change balance mapping. Malformed private rows,
empty envelopes and unrelated messages cannot cancel the authentication deadline.
An unrelated subscription rejection does not retire an authenticated socket.

The existing public socket policy remains separate. Deterministic lifecycle
regressions are in `tests/socket_liveness.rs`; private replay tapes provide
scripted evidence, not live account connectivity evidence.
