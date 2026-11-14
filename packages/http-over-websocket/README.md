# HTTP over WebSocket request deadlines

Every request has one bounded deadline covering token lookup, tunnel readiness,
sending and response decoding. TypeScript's `src/requestBudget.ts` is the parity
reference for the Rust engine/native host.

| Class | Default budget |
| --- | ---: |
| discovery/catalogue | 60 s |
| history page | 30 s |
| order read | 20 s |
| order write | 20 s |
| other read | 20 s |

GET requests accept closed `requestClass` hints. Non-GET methods always keep
the short `orderWrite` label, ignoring hints for longer classes; managed posted
reads retain their explicit remaining engine deadline. Legacy GETs classify exact path components (e.g. `instruments-info`,
`contracts`, `exchangeInfo`, `history`, `candles`, `orders`), ignoring queries and
bodies. Unknown GET paths use `read`; other methods use `orderWrite`. Labels only
select budgets and diagnostic facts; they never authorize retries.

Explicit positive finite Axios timeouts, including 20 s, remain authoritative.
Missing, zero, negative or nonfinite timeouts use the class default. Budgets are
integers between 1 and 2147483647 ms. Socket/time-sync/auth readiness has a 30 s
cap inside the request's original deadline. Native/TS preparation and binding
never renew that deadline. Existing engine startup discovery remains 90 s total
(60 s discovery plus a 30 s readiness allowance); explicit venue and Plan call
windows may impose shorter bounds.

Timeouts and readiness failures identify `class`, `budget` and `elapsed` in
milliseconds, with sanitized URLs only. Missing-token failures retain their
separate authorization classification. A post-send write timeout still means
outcome unknown and is never an instruction to replay the write.

Run `yarn test` here; the JSON fixtures in `src/requestBudget.fixtures.json` also
serve the Rust parity tests. Rust's `sdk-rs/scripts/check-bans.sh` remains the CI
gate against unbounded native socket opens/writes.

## Relay keepalive and reconnect diagnostics

HOW keeps its existing 5-second RPC heartbeat and 25-second pong silence
recovery. After the relay handshake, Node/Bun sockets use native WebSocket
ping/pong every 15 seconds, with a 45-second receive silence deadline checked
on timers and ingress. Cleanup removes both the timer and pong listener.
Browsers retain the venue heartbeat; relay RPC is never injected into the
upstream application stream.

Routine reconnect diagnostics use info output. The CLI retains them in
`cli.log` only; repeated failures, exhausted retries and authentication errors
remain visible. URL providers are never printed, and resolved URL diagnostics
omit queries, credentials and fragments. Socket debug output omits outgoing
frames and protocols.
