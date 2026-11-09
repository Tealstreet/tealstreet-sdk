# WebSocket mutation authority

`Cx::send` and `send_signed` are for control traffic: subscriptions, session
authentication and heartbeats. A venue must classify orders and setting changes
as writes, just as it declares `Operation::Mutation` for HTTP. The engine does not
infer operation semantics from JSON or endpoint names.

Construct those intents with `Cx::websocket_write` using an engine-issued
mutation `Context`, then queue them with `send_websocket_write`. The opaque
`WebSocketWrite` cannot be constructed directly or passed to control sends.
Only command-owned mutations can construct one; Read, Background and Session
contexts cannot. Foreign, expired and revoked contexts fail before an intent is
returned, and submission validates again. Only live Private or Business socket
identities are accepted.

The managed request lifecycle holds the intent until signing completes, then
emits an opaque `WebSocketWriteBinding` in the existing outbound envelope. The
binding retains a process-unique issuer stamp for its creating engine. Readiness
and final claim reject a foreign binding before admission, even when a replacement
engine repeats every request, socket and permit counter. HTTP wire tokens are unchanged.
The native transport reserves its single-use permit through the same
`Venue::http_release` registration, `http_release_veto`, pacing and write journal
as HTTP mutations. Admission callbacks run before the physical handoff, outside
the socket-state mutex. `WebSocketHandoff` revalidates the socket and uses its
single-use `WebSocketCommit` only around `start_send`. A refused handoff aborts
as `NotSent`; pacing, nonce release floors and write fences advance only with
an accepted handoff. `on_signing_release` runs afterward outside the socket lock.
Both drain and final claim check the socket; final claim requires its completed
handshake. The shared lifecycle's HTTP packet carries the frame text; it never
reaches an HTTP executor. Only admitted reservations reach the physical send.
Signing recipes see `WsSend` and cannot replace its socket identity. A credentialed
context requires a signing recipe; an unsigned context cannot attach one.

The returned `RequestId` identifies the managed attempt. A future adopting venue
must correlate its socket acknowledgement to that ID and deliver the decoded
reply through `Event::HttpResponse` (or loss through `HttpFailed`). Its existing
HTTP write-fact, reconciliation and completion hooks then record the result.
The same original deadline bounds preparation, signing and the response; loss
after release remains an uncertain write. There is no implicit socket retry.
If pacing is not ready at the final claim, the intent fails before send.

Hosts must register their final claim handoff with `enable_websocket_write_host`.
The native host implements it through `Transport::connect_socket` and its
connection-owned `WebSocketWriter`. The writer checks an open socket and writer
readiness before invoking the claim callback, then hands off the frame without
an async queue or yield. Connecting, busy, closed and unsupported writers fail
as `NotSent` without committing. Socket retirement or replacement during
admission also remains `NotSent`; uncertain physical send or flush failures are
`Unknown`. Legacy string queues carry control traffic only. Custom native
transports default to no mutation writer and must opt into this contract.
WASM and custom hosts without that registration refuse submission before any
signing or preparation; the existing browser
bridge has no WebSocket mutation handoff.

Managed feed sockets and HTTP Mutation groups are refused: their current
contracts authorize subscription interests and HTTP batch handoffs respectively,
and supply no socket mutation handoff. Explicit HTTP egress is also refused because
an existing socket cannot inherit an HTTP route. No venue currently adopts this API;
venue transport behavior is unchanged.
