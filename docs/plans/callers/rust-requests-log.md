# Rust engine traffic in the Requests Log

**Change.** `transport/requestsLog.ts` wraps every Rust engine's transport (`RustEngineExchange`): the LOCAL worker, the COSIGNED or DELEGATED server, and the browser's main-thread page engine. Its traffic goes into the Requests Log through the events the TS paths already use:
- `grafana-http-request` and `grafana-http-response`: per HTTP request, the method, URL, body, status and timing (ms). Managed requests are logged at dispatch, and a streamed body once it completes.
- `grafana-pws` and `grafana-pws-sub`: per private socket, each received and sent frame, sampled by the same per-socket rate and size cap as `BaseWebSocket` (now the shared `WsDiagnosticSampler`). Heartbeats are skipped, and public sockets are not wrapped.

**Gate.** It uses the existing Requests Log toggle (`isWsDiagnosticsEnabled`, which NX sets from `enableRequestResponseAndPrivateWsLog` on main, in workers, and on the server through the verbose frame). While the toggle is off, each request and frame costs one flag check and builds nothing.

**Redaction.**
- Headers are never logged.
- URLs, bodies and frames go through `redactWsDiagnosticPayload`, which now also covers `sig`, `apiSecret`, `secretKey`, `privateKey`, `accessKey`, `authorization` and `proxyToken`.
- The test asserts that no API key, secret, signature, passphrase, proxy token or auth header value appears.

**Callers.**
- SDK: `isHeartbeatFrame` moves to `utils/wsDiagnostics.ts` and is re-exported from `base.ws.ts`.
- NX: none. The existing listeners put these events into `requestsLogAtom`, and worker and server events cross to main like the TS ones.

**Obligation.** None. **Deletion prerequisites.** None added.
