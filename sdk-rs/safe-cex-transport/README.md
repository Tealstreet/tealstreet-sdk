# Transport

Behavior and architecture: [safe-cex guidance](../../packages/safe-cex/CLAUDE.md#proxy--tunnel-architecture).

Run `yarn routing:generate` here after an intentional TypeScript routing snapshot
update; run `yarn routing:check` to verify freshness and cross-language equivalence.
The generator consumes `routing.equivalence.test.ts`'s golden snapshot for REST
and all three WS channels, checks every result against the live TS resolver, and
rejects incomplete exchange/option coverage. All 24 combinations per exchange,
including the Brazil site-id branch, come from the golden snapshot. HOW tokens come
from `http-over-websocket/egressRegion.ts`, and relay URLs use the derived region.
Region pinning comes from differing JA/SG snapshot results. No routing host or
Tokyo exchange list is maintained by hand in Rust.

`TransportRegistry` is held by one runtime realm. Hosts pass an instance provider
as a separate in-process value; it takes precedence over the registry default.
Unregistered realms use `DirectTransport`, whose REST resolution refuses either
Tealstreet routing flag before a request can reach a venue. Standalone helper
calls use the same registry with no instance override.

HTTP adapter/authorization callbacks return object-safe boxed local futures. This
permits browser JS handles; these futures cannot be spawned on an executor that
requires `Send`. Host I/O is supplied through `TunnelHost` and `TunnelSocket`; HOW protocol
handling and relay negotiation live in this crate. P2c signing and retry contracts
are linked below.

P2b behavior: [HOW guidance](../../packages/http-over-websocket/CLAUDE.md)
and [fallback trigger](../../packages/safe-cex/src/utils/wsDirectFallback.ts).
`how::HowRestAdapter` implements P2a’s `RestAdapter` and shares the serde-only
HTTP frame definitions with `infra/tradetunnel/src/frames.rs`. `relay` exposes
one event consumer per socket. Host receive futures must be cancellation safe.
`socket::TunnelDialOptions` rejects incapable native bindings before dialing;
its options must be applied by the host binding, including HTTP/1.1 ALPN.

Regional clients are shared by host identity while adapters or callers own them.
The runtime cache holds weak references and prunes dead entries on lookup;
`dispose_how_websockets` closes them and cancels their host tasks. Hosts must
schedule `spawn_local` futures and implement cancellation through `TunnelTask`.
Static dial, timer and receive futures must own their I/O handles, rather than
retain the host/client or borrow either. Monitor waits hold only weak client
leases, so dropping the final caller cancels tasks and releases the host.
The monitor starts on first connect, sends idle pings, consumes frames, and
reconnects after a missing pong or a disconnect. Raw adapters negotiate v4
gzip/binary frames and accept legacy raw and v3 gzip/base64 text responses.
The monitor rechecks the socket after reconnect backoff; stale receives are
ignored by connection generation so they cannot close a newer connection.

Preparation cancellation and deadlines only release the caller’s preparation
lease; they never close the shared socket or fail other nonce waiters. The
connection tracks whether a token was sent, and token sends retry only
`WebSocketNotReadyError` five times with 500ms waits under the caller’s deadline.
Preparation is serialized, so a joiner starts its own bad-auth counter after the
owner exhausts instead of advancing concurrently as in TS. This can cause extra
authentication attempts, bounded by each caller’s complete-request deadline.
The loop also prepares writes, but sends only rpc.auth before constructing the
HTTP request; extra attempts cannot replay an order.

Dial options are sealed and expose only borrowed headers/ALPN accessors. Host
bindings must apply those options unchanged. They must derive runtime capabilities
from the actual execution environment: Node/Bun/native hosts must never declare
Browser or BrowserWorker, even if a global WebSocket exists. The browser variants
are reserved for browser-controlled upgrades where JavaScript cannot set headers.
Rust cannot independently inspect `process.versions.node` through the host ABI.

Request-body truthiness is applied to the original bytes before JSON parsing.
Ordinary requests retain JSON.parse’s binary64 rounding; raw requests protect
nested large integer lexemes as strings before parsing and transmission, including
values beyond u64. The raw custom parser returns null for non-JSON input.

Hosts implement `TunnelSocket::close(code, reason)` as a close request and return
errors only for synchronous close failures. Receive errors represent error events;
close events are separate and must carry the actual code, reason and cleanliness.
Relay local close waits for that event, fabricating 1006 only when close throws.
Protocol handshake failures retain the TypeScript fallback close of 4000.

An immediate relay close is driven by `next_event` even before the first dial
poll; it applies the requested close to the host socket and receives its final
close metadata without sending authentication or relay negotiation frames.

`RestRequest.timeout_ms` supplies each call’s complete-request deadline. Positive
values select that budget; absent, zero and negative values select 20 seconds,
matching axios config.timeout. Adapters carry no construction-time timeout.

`RestRequest.params` is the query channel. Outbound data and params are populated
in the shared `HttpRequestArgs<Value>` before serialization; the server’s default
`HttpRequestArgs` still accepts map bodies only. This preserves TypeScript’s
batch-body wire shape and its current server-side parse rejection. Broker-ID
verification remains server-side in `broker::check` and outside P2b’s scope.

Signed JSON bodies retain JavaScript property order through parsing and frame serialization: canonical integer-index keys come first in numeric order, followed by other keys in insertion order. Ordinary response numbers also round through binary64; raw response bytes retain their original digits.
Request headers use IndexMap and query params use an order-preserving serde Map.
Signing and REST request headers share the ordered header type. Optional signer
query values remain supported; HOW accepts object query values and assigns them
to the explicit shared map type. Signing preserves each request’s timeout.
Shared fields are assigned from explicitly typed maps, so changing them to a
HashMap breaks the client build. Both workspaces pin identical serde_json version
and features; a hermetic configuration test guards against one-sided drift.

Native HTTP execution is supplied by the host through `RestAdapter`.

P2c contracts and behavior: [read retry guidance](../../packages/safe-cex/CLAUDE.md#automatic-retries-are-for-reads-only),
[remote signing and secondary credentials](../../packages/safe-cex/CLAUDE.md#proxy--tunnel-architecture),
and [polling slowdown](../../packages/safe-cex/CLAUDE.md#server-pushed-polling-slowdown-tickintervaloverride).
Run `yarn p2c:gates` here for each scoped gate, its negative control, and its
restored pass. The runner prints the directory containing full gate logs.
`yarn retry:types` checks each retry and secondary-access doctest independently, then removes its
specific invariant and rejects unrelated-error substitutions. Error-code
annotations on `compile_fail` do not enforce the failure reason on Rust 1.99.

Run `yarn warning:wasm` here to execute the warning probe as compiled wasm and
verify console output, deduplication, reentry, and direct routes. It requires the
locked wasm-bindgen CLI (`yarn ffi:setup` from the repository root).

The site-id doctest uses plain `compile_fail`. The ignored
`typed_site_doctest_reports_e0308` gate compiles that exact example against the
actual Cargo library and requires the compiler’s JSON diagnostic to be E0308.
It rejects unrelated errors and a removed type guard; no rustdoc error-code
annotation is relied upon. CI’s
`-- --include-ignored` runs it alongside the live routing freshness gate.

The dial-options doctest is checked through `scripts/site-id-diagnostic.mjs --dial-options`, which requires E0451 for both private fields. Wrong imports or renamed fields cannot substitute an unrelated compile failure.

Response headers pass through an explicit shared HashMap assignment before host conversion, so a server-side header type change breaks the client build.

Remaining HOW APIs live in `how_config` and `tunnel_endpoints`. Configuration
patches shallow-merge callbacks and explicitly clear optional callbacks with
`Some(None)`. Explicitly configured token/expiry/clock callbacks take precedence
over the existing host callbacks; unconfigured runtime hosts retain their current
behavior. `get_how_config` returns an immutable cloned snapshot rather than JS’s
mutable configuration object. URL helpers use WHATWG parsing and preserve the
generated endpoints with the shared production/staging selector. Existing host and adapter signatures are unchanged.

Domain latency monitoring initializes known domains from configured metadata on
connect. HTTP sends that do not throw enroll known domains, including closed
frames silently dropped by the ping/pong substring rule. Domains are queried
immediately once and subsequently on one-second ticks after strictly more than
ten seconds without an update. Close cancels the newest latency handle; orphan
chains from repeated starts remain until disposal. Caches and used domains
survive reconnect. Regional snapshots return Singapore then Tokyo and rely on
the existing weak client ownership. Latency wire values retain TS’s unchecked
field shapes; Rust restricts domain keys to strings.

Relay property callbacks are optional observers invoked before queued-event
delivery; the existing single `next_event` consumer remains unchanged. Callbacks
are cloned before invocation so reentry does not retain a RefCell borrow.
`buffered_amount` reads host network-buffer bytes, never the pre-open relay queue.
The new defaulted `TunnelSocket::buffered_amount` returns zero for hosts without
that optional metric. Handshake cancellation is represented by state-scoped
deadline futures, without a redundant standalone timer flag. Binary views copy
only their supplied slice, and base64 decoding accepts browser atob whitespace,
unpadded inputs and noncanonical trailing bits.

Native verbose logs honor DEBUG=1/true and the public logging flag, omit headers
and URL query values, redact sensitive JSON keys, and bound strings/arrays/objects
and recursion depth. The Rust JSON tree does not represent cyclic objects, DOM
FormData/URLSearchParams or distinct BigInt values; those conversions belong to
host inputs. UTF-16 truncation substitutes U+FFFD if a cut splits a surrogate
pair, because Rust strings cannot hold lone surrogates. Scalar-string secrets
remain visible, preserving the TypeScript key-only redaction limitation.

`HowRestAdapter::with_options` binds a custom parser and/or forced JA/SG region.
Custom parsers run on request text and decoded raw responses; request parser
throws use the same raw-string/form fallback as TS. `execute_with_context` adds
typed caller-config snapshots and named timeout/method/URL/route metadata while
retaining the response-bearing RestAdapter failure inside its contextual error.
Opaque Axios keys, JS exception prototypes and originalError objects remain
unrepresented; structured REST responses and context are preserved.
Simulation uses the explicitly configured direct RestAdapter under the same
request deadline and sends no tunnel frames; simulation bypasses constructor
tunnel-capability validation, while real dials still validate fail-closed. Its tests distinguish constructing/handing a dependency
request from polling its actual I/O future. Request parsing/logging consumes the
deadline too, with a final check before a stale frame can be sent.

`RelayWebSocket::with_connection_options` extends `new(host,url)` with
`UpgradeHeaders::Static` or `UpgradeHeaders::Fresh(FreshHeadersProvider)`,
protocols and authentication-only `headers_required`. The provider remains lazy
until each inner tunnel dial; callers reuse it when constructing a replacement
relay after disconnect. Required authentication headers return
`HeaderCapableWebSocketUnavailableError`'s message before dialling when a
header-capable constructor is unavailable. The check runs both at construction
and at lazy dial. Browser/wasm best-effort overlays and protocols are inert.
The sealed Origin/UA/ALPN policy survives caller overlays. Rust removes
case-insensitive conflicting policy names; JS spread preserves differently cased
keys. This deliberate difference is declared in the constructor manifest row,
as is the typed surface's omission of arbitrary `unknown` Node options.

Run `yarn warning:wasm` to execute the relay overlay probe as wasm in addition
to the existing warning probe. The overlay probe uses a host that declares native
capabilities, so it measures the wasm branch independently of browser tagging.
See [HOW gate evidence](GATE_EVIDENCE.md) for measured controls and limitations.
The case-conflict rule is forced by the inherited BTreeMap header ABI, which
cannot preserve ordered application of duplicate HTTP field names (even though
it can hold differently cased string keys). Keeping the required Origin value
deterministically preserves WAF policy. Restoring conflicting case variants
would allow callers to shadow that policy through downstream normalization.

Raw request parsing reproduces the TypeScript big-integer regex bug inside quoted
strings: corrupted JSON becomes outbound arg.data = null. A live TypeScript
oracle deliberately fails if that upstream bug is corrected. The shared safe-cex-num
whitespace shim matches all Unicode scalars against Node trim. Numeric spelling
also uses its js_number_string primitive.

Tunnel endpoints select /tunnel-staging only when NEXT_PUBLIC_TRADETUNNEL_STAGING
is exactly 1. Native clients capture the process environment once at first
endpoint use; wasm captures it at build time, like browser bundle substitution.
The same selector supplies HOW dials, relay URL helpers and generated-route relay
URLs. The public endpoint signatures are unchanged.

Wasm verbose logging remains disabled by an accepted implementation simplification.
A JS environment bridge is feasible; this is a chosen omission with no parity
argument, not a restriction forced by Rust.

Review clarifications: repeated latency starts reproduce the source's orphan
chain, and stop cancels only the newest tick. Dispose prevents that orphan from
rescheduling; the review's stronger perpetual-after-dispose claim was disproved
by the live TypeScript lifecycle test. Closed HOW frames containing ping or pong
anywhere in their serialized JSON are silently dropped while domain tracking
still runs (a source bug, including HTTP URLs and unrelated body strings).

Simulation close is terminal for the retained client, unlike a real disconnect.
Disposal keeps each waiting request's original deadline and timeout error.
The weak regional cache is still a chosen lifetime adaptation: final release
allows recreation and loses auth/latency history, unlike eager TS module exports.
HOWConfigPatch.undefined_fields applies explicit-undefined resets by TS property
name; typed snapshots expose failing closures/false for required callbacks/bool.
These snapshot representations are not live JavaScript object identity.

Relay close accepts an omitted status, retains buffered bytes after fail until
the inner close callback, and cancels an already-parked handshake deadline on
transition. Rust Drop closes its final owned socket without a status; TS has no
deterministic finalizer. Dead inner sockets are released on their close callback.
Host callback configuration remains usable without global configureHOW, unlike
TypeScript's mandatory module-level configuration. The per-row notes identify
these ownership and host-injection choices rather than claiming exact parity.

TunnelSocket.ready_state exposes DOM readyState for the closed-send precheck.
Its compatibility default is OPEN because connect returns an opened socket.
Bindings whose underlying state can change before receive delivers close must
override it; otherwise the host cannot preserve the silent ping/pong drop during
that interval. This additive method is claimed by HOWWebSocket.validateAndSend.

RestResponse.reason is optional direct HTTP statusText; HOW leaves it None,
including frames containing status_text. Bybit's interceptor appends nonempty
direct reasons, inheriting the health-pattern collision documented in
[the SDK guidance](../../packages/safe-cex/CLAUDE.md). This is source behavior,
not permission to add canonical reason phrases in appendHttpFailure.

The source-package JSON parsers are public as
safe_cex_transport::how::big_int_as_string_json_parse and json_parse. The first
shares the complete rewrite/parse/null contract used by HOW raw bodies; quoted
string corruption is deliberately preserved. Both take UTF-8 str and return
owned JSON values. Overflow retains its arbitrary-precision Number lexeme so
shared JS coercions observe Infinity rather than null before venue interpretation.
Negative zero, including negative underflow, also retains its lexeme to preserve
the binary64 sign that Number-to-String would erase. Use JS serialization when a JSON wire projection is needed;
serde serialization retains that lexeme. Arbitrary non-string JS input and lone
UTF-16 surrogates remain outside this chosen typed boundary.

Envelope decoding also uses json_parse, before typed status decoding and latency
storage; ordinary finite values collapse through binary64, with signed zero
and overflow retaining lexemes so venue interpretation sees the source value. Relay queued-send failures retain OPEN and discard the
drained tail, matching the source bug. Heartbeat-send failures are consumed
inside onMessage so pending requests retain their deadlines.

The numeric clock callback and host fallback are chosen simplifications, not
forced Rust constraints. Missing/non-numeric serverTime does not invoke the
callback; the TypeScript invokes it even with undefined. The row notes disclose
this omission along with the omitted structured not-ready error properties.

REST adapters now return `RestFuture<RestResponse>` with a `RestFailure` retaining
message, response (status/reason/headers/body), request, context and leaf transport
provenance. Do not flatten a venue rejection to `Host(message)` or return it as a
successful response to preserve its body. Clock, socket and signer leaf operations
continue to use `TransportFuture` and `TransportError`. Core `HttpClient` itself
implements `RestAdapter`, so secondary-key routing needs no lossy wrapper.

`TransportError::Network` holds opaque `NetworkFailure::from_code(message, code)`.
Hosts forward the originating error code; they cannot choose retry eligibility.
`RestFailure::is_retryable_network` requires no response, a nonempty code, neither
ERR_CANCELED nor ECONNABORTED, and the installed is-retry-allowed deny list (DNS,
unreachable-network and TLS errors). ETIMEDOUT is not ECONNABORTED and remains
eligible as in axios-retry. The live installed-package oracle runs in core CI.
Signature retries inspect rejected responses only; resolved 2xx responses never
retry even if a batch-leg message contains signature text. The Bybit exact-path
network exclusion does not disable retries of rejected signature responses.

The live parser and staging oracles use the native CI job's isolated TypeScript
installation through `SAFE_CEX_TYPESCRIPT_PATH`; local runs may resolve
`typescript` normally. They transpile and execute the current TS sources, without
`tsx` or a root workspace install. Missing tooling is an error, never a skip.
The ordinary Cargo test suite also runs the parser oracle in a child process
with an in-memory TS corruption fix and requires an assertion failure at 101,
so CI exercises the correction detector as well as the positive oracle.

All live TypeScript oracles, including the closed-send/latency and relay lifecycle witnesses, resolve the compiler through `SAFE_CEX_TYPESCRIPT_PATH` in CI. Validate the whole transport test crate in a source mirror without ancestor `node_modules`; a passing parser oracle alone does not establish that later lifecycle oracles execute. Missing compiler dependencies remain hard failures.

`RestRequest.body` uses `RestBody`, with explicit `Untransformed(Value)` and
`Encoded(Vec<u8>)` arms and no Default. Secondary routing preserves the arm.
The shared post-signing transform creates a third, opaque `Transformed` state.
Failures retain that JS data and signed headers for retry, so a later signer
sees the prior transformed string while the wire transform remains identity.
Encoded bytes never acquire inferred provenance. A retry revalidates the failed
configuration against its original read classifier and refuses a changed write
target with the terminal error and no replay. Query parameters remain in the
pre-serialization configuration so a retry does not append the query twice.
Signing uses `untransformed()` and refuses encoded input without a send; it never
reconstructs body origin from bytes. After signing, `wire_bytes()` preserves raw
strings and delegates other JSON to the shared ECMAScript encoder. HOW transforms
untransformed inputs at its final wire boundary before applying its existing
source parser. Remote signing still applies TypeScript's explicit tryParseJson
operation to untransformed config values; that source operation is distinct from
and does not authorize decoding arbitrary encoded request bytes for signing.

Remote signing is one exhaustive `RemoteSignerRequest`/`RemoteSignerResponse`
HTTP-or-WS union. HTTP payloads are `HttpSigningRequest`/`HttpSigningResponse`;
WS payloads are the canonical types from `safe-cex-types::ws_auth`, shared with
crypto without a lateral service dependency. The union owns the serialized type
(`http` or `ws-auth`); WS retains id, wsAuth, extraData and wsAuthSignature.
Decode an untagged wire response with `request.decode_response` so an error-only
reply cannot be misidentified. Unsupported WS signing returns an explicit error;
it must not fall through to HTTP. HTTP execution rejects a WS response before send.
