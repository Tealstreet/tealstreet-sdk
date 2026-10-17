# KuCoin venue family

Behavioral reference: [TypeScript venue guidance](../../packages/safe-cex/src/exchanges/kucoin/CLAUDE.md).
Core contract: [shared exchange](../safe-cex-exchange/CLAUDE.md).

Spot production reads and cancellation use `spot_native_order_reads`, installed
by `spot_exchange_class.rs`. The unused typed `spot_reads` and `spot_writes`
modules and their exclusive tests/oracles/contracts have been removed. The
constructed `spot_io_binding` target observes market precision, distinct ticker
prices, current Store fallback, active-symbol/page requests, and all three
cancellation routes. Three compiled mutations of the live native implementation
leave the retired typed tests green and fail these constructed witnesses.
Existing `spot_native_order_reads` component/source tests remain authoritative
within their documented scope. Historical typed market/ticker/cancellation
refusal and oracle measurements in older evidence are not production coverage.
Shared output/identity limitations remain in the manifest; deletion widens no API.

`types` and `spot_types` contain protocol tables and strict spot token records.
`yarn venue-oracle:kucoin` compares every table against executing live source,
and checks record acceptance against the live TypeScript compiler. CI uses
`SAFE_CEX_TYPESCRIPT_PATH` from the existing isolated compiler installation.
Missing optional fields remain distinct from explicit null; private decode
companions sit behind the shared map-only guard. `account_mode` ports inert UTA detection and Classic-only selection; its JSON
input boundary and exotic-JS limitations are explicit manifest divergences.
`order_helpers` holds pure futures side, stop and mode decisions, exercised
against extracted live source methods. `broker` uses the core normalized string-header boundary and shared crypto.
`api_capture` pins live hashing plus actual Axios transforms before factory
implementation. `api` now composes the shared HTTP/interceptor, retry, crypto and
remote adapter operations; remaining exchange/socket runtime methods are pending.

See [gate evidence](GATE_EVIDENCE.md) for measured controls and current scope.

`sizing` uses the production Store for symbols, contract quantities and same-id
remainder reconciliation. `mappers` preserves native futures quantities beside
base units and distinct spot semantics; its live oracle tags nonfinite numbers
before serialization. Shared TableHit preserves inherited branches and later source overrides. The
closed Order fields refuse inherited outputs as whole records, with callable
omission and the separately retained __proto__ object witnessed before JSON.
Typed-output choices, prototype-object findings and input restrictions are
recorded in the two mapper manifest rows.

`balance_reference` pins the live Spot whole-object dictionary write behavior:
`__proto__` replaces the local dictionary prototype and loses its own serialized
entry, without polluting Object.prototype. Its nested-write control is explicitly
a different shape. Spot fetchBalance remains pending on the shared write operation.

Futures formatters use shared TableHit for standard inherited market names, including
source UUID consumption and undefined-symbol payloads. The live formatter oracle
instantiates DefaultStore and admits markets through its actual update; it must not use
Object.fromEntries for that dictionary. The two formatter manifest rows record the separately measured typed Store lookup
cost; their native component evidence is FUTURES_NATIVE_MAPPING_FORMATTERS_EVIDENCE.md.

`ws_helpers::send_channel_op` batches subscription frames using the shared JSON
encoder, with host callbacks for Date.now and the core socket send. Its live
source oracle pins exact bytes, per-frame clock reads and send-failure ordering;
it does not create a separate socket runtime.

Production futures normal/algo order reads use `futures_native_order_reads`,
installed by `binding/futures.rs` in the constructed exchange. The unused typed
`order_reads` copy, its decoder contracts and tests have been deleted. Run
`tests/order_reads_binding.rs` for the real HTTP/Store book witness and
`tests/futures_native_order_reads.rs` for the native component/source cases.
The binding witness observes both endpoints, scaled amounts and independent
failed-category preservation. A compiled live-reader mutation passes all eight
retired copy tests but fails the constructed witness; exact restoration passes.
Historical typed-reader findings and control counts in evidence documents are
not evidence about the production route. Shared typed projections remain bounded
by the manifest's representation choices.

`numeric_ingress` audits record decoders against live declarations and JSON.parse,
comparing f64 bits before serialization. Each Spot token projection and the
account-mode detection decoder captures `JsNumberInput` inside its public
Deserialize entry before calling the private map-only/tagged Wire decoder through
`decode_with`. This repairs bare serde callers as well as the shared
`js_from_str`, `js_from_value` and `js_from_value_ref` APIs; exhaustive expression
registrations check every output field. All seven routes retain overflow,
underflow and signed zero. The two open-string Spot type rows retain the shared
UTF-8 boundary, with live lone-surrogate refusals and scalar/astral/whitespace
positives. Capturing only outside the public decoder would leave direct callers
unrepaired; compiling bypass controls pin the capture in both decoder families.

The seven private Serde companions live at module scope and use `serde(remote)`;
they introduce no additional `Deserialize` entrypoints. The public entries retain
their map/string admission checks and correlated account-mode checks.
`accepted_native_guards` observes native fields, optional presence and numeric bits,
and rejects complete valid sequence or tagged-object forms through owned,
borrowed, qualified and text decoding. Run the six compiled guard-removal controls
serially with `node safe-cex-venue-kucoin/tests/accepted-native-controls.mjs` from
`sdk-rs`; the runner restores production bytes and reruns all seven tests.

`accepted_exchange_members` exercises connected native book callbacks and cleanup.
The futures case proves a valid change emits before cleanup, then suppresses the
next valid change. Run `node safe-cex-venue-kucoin/tests/accepted-book-cleanup-control.mjs`
alone for the compiled handler-removal failure and exact restoration proof.

Account-mode Serialize emits native fields directly, without an intermediate
`json!` Value that would erase infinities before the ingress output guard.
The wire serializer still gives JavaScript's null for non-finite JSON values;
the ingress guard sees the native numeric events before that projection.

Response consumers use RestResponseBody::json_value for the current model view.
Fixtures explicitly construct Encoded HTTP bytes; native HOW data is read without
a serialize/reparse round trip. This accessor does not install a response parser
or claim completion of the factory's separate response-transform behavior.

`spot_helpers::set_leverage` returns the shared StoreMutationError unchanged.
State commits before ordered notification; the first failure skips later
subscribers and retains its opaque subscriber id and cause. Its live oracle
executes the production Store constructor and update.

The Store notification audit also traces fixed callees, not only direct writes.
The 30 remaining ported callables and 32 data/type rows add no notification path.
Already-divergent `send_channel_op` has a separate core-composition FINDING:
closed-socket non-heartbeat send starts an unawaited reconnect that publishes
health and rejects on a Store subscriber throw, while the venue helper returns
normally. The source witness executes that method chain and the real Store with
explicit diagnostic/watchdog fixtures; it is not a full socket runtime oracle.

`api` runs venue signing inside `HttpClient::execute_with_interceptor`, after
defaults and before the shared transform. Remote echo adapter values are compared
before byte projection, so an object cannot masquerade as its JSON string. The
read classifier admits GET-like reads and the two bullet POST endpoints; order
writes cannot enter retry. Each retry and the one signature-rejection resend
re-sign with a fresh timestamp over the serialized string body; only a signing
capture keeps the server's timestamp. Retries share a derived timeout budget.

Factories own option snapshots. Source futures captures authentication/key
defaults but reads live broker config; Spot also reads live key/privacy changes.
These directions, Axios false-header suppression outside the string header ABI,
and the UTF-8 boundary have maintained source witnesses and CHOSEN notes. The
factory oracle executes real Axios/retry/nanoid/remote adapter and venue code,
while its safeCexAxiosCreate/transport-install seam is injected: it is not a
production routing, authorization or telemetry acceptance claim. Pending signing
and transport cancellation tests drop twice, including after a retry; no venue
latch, task registry or timer is introduced. Default retry delay is zero, so
there is no venue retry-sleep future to own.

The validation oracle uses actual Axios default response transformation: empty
and non-JSON bodies remain strings in source and cause a non-emitting fallback.
Current shared byte responses instead produce a logged decode diagnostic;
this separate response-staging FINDING awaits core's response provenance ABI.

`spot_fills_reference` holds the pending Spot historical-fill read on the shared
request query boundary. The live method and its actual factory serializer execute
with installed Axios, qs and omitUndefined. Native NaN and infinities are query
text, not JSON values; capture query bits before JSON erases them. Do not route
those options through json! or synthesize NaN JSON. Clock calls and later-row
failures are captured without claiming a Rust mapper or transport implementation.

The market read now applies the same refusal-timing rule as tickers: template
interpolation precedes numeric coercions, and typed String refusal is deferred
until the entire source batch finishes. Later source errors keep their original
message. The market oracle uses installed Axios response transformation, not
JSON.parse(raw); empty/non-JSON bodies retain a separate response-staging finding.

The older account, futures order-read and Spot symbol-discovery oracles also use
the actual installed Axios response transform, including raw failure bodies.
Empty/non-JSON account/order responses retain separate provenance findings: the
source emits a fallback/transport message where Rust diagnoses JSON decoding.
Discovery returns an empty array in both languages and adds no such finding.
The numeric-ingress oracle's JSON.parse remains intentional source decoding.

`FIELD_IDENTITY_AUDIT.md` inventories equal-valued fixture dimensions and the
consumer boundary. Market id/symbol and independent numeric/boolean fields must
have distinct cases; source constants are documented separately. Keep cross-crate
feeder bytes unchanged and add labelled identity supplements if any are introduced.
The balance/fills reference-only witnesses assert fields before their pending
production reads exist; their controls do not claim a production mapper.

`spot_orders_reference` executes the live Spot pagination, mapper and discovery
methods with real Store and Axios. It pins concurrent launch, symbol-order
flattening, first-rejection fallback and sibling effects after that return.
The production row stays pending on shared owned aggregation; a raw runtime
spawn is not a venue-owned lifecycle contract. Numeric identities are asserted
before serialization, and raw bodies use Axios rather than JSON.parse fixtures.

Spot cancellation reads the current RestResponseBody model view through
json_value; fixture responses explicitly select Encoded. This compiler-forced
response migration does not change the separate fallible-emitter boundary.

Spot mapOrder evaluates size, filled, remaining and time-in-force before the
record expressions, then status, type, price and timestamp before refusing typed
id/symbol or inherited enum outputs. A later source coercion error must precede
a representation refusal. Shared mapper coercion helpers expose error.message;
futures callers are covered separately because they use the same helpers.
This per-record rule does not claim whole-page or concurrent-read refusal timing.

Account and order reads now return ExchangeError when error reporting fails.
A bad-code emission inside the source try is caught and reported again; an
emission from the catch escapes immediately. Spot cancellation retains its
existing boxed error contract and returns the listener failure when reporting
fails. Raw non-string error input remains a separate shared finding.
The notification oracle executes live venue/reporting methods and installed
TinyEmitter, with all cases returned by one Node process.

Numeric capture helpers canonicalize NaN only, after JavaScript DataView coercion.
Signed zero, finite numbers and infinities remain bit-exact; each live numeric
capture records architecture and Node major. Decoded records and variants have
direct native identity assertions in addition to any re-encoding comparisons.
Account, order-read and market oracle cases are batched once per corpus to avoid
per-case Node processes on the shared machine.

Formatter precision fixtures use fractional inputs and distinct amount/price
steps for both bracket directions and algo stops. Native payload fields are
asserted before serialization; different step values alone do not prove identity
when the chosen input rounds to the same result under both steps.

Oracle capture helpers compare source case counts against caller inputs before
pairwise comparisons. The ticker cardinality supplement separately fixes native
output count and comparison indices; neither oracle length nor a printed corpus
size may determine how many native results receive assertions.

Spot order-reference concurrency traces compare complete literal event sequences
at return and after settlement, including post-return transport-error suppression.
The trace controls preserve capture length when reversing or replacing events.

Alias-array captures assert whole records before selected fields; reference
identity remains a separate assertion because equal JSON cannot prove aliasing.

Table tests inspect native pair counts and unique keys before serialization.
The supplemental table-pairs capture compares ordered entries alongside the
original object projection, which alone would erase duplicate native keys.

`ws_ping` ports all four futures/Spot public/private ping entrypoints using
BaseWebSocket disposal, mark_ping_sent and send/send_with_log. The Date.now
callback runs only after the shared monotonic-clock update. Heartbeat JSON uses
the shared JavaScript encoder with id before type. No venue-local heartbeat
state or timer is introduced. Native decoded id/type and core pong latency are
asserted before source wire comparisons; disposing the parent skips both clocks.
See [ping evidence](WS_PING_EVIDENCE.md) for the four ported rows and one
compiling native-id control per method.
`position_metadata` composes the shared get_or_fetch_positions_for_symbol
operation for futures position mode and margin type. It selects the first
position, rejects empty results or missing metadata with the source message,
and propagates dependency errors unchanged. It owns no separate fetch task or
Store write; any fetching and notification belong to core. Native enum identity
is asserted before wire comparison. Its oracle injects the position dependency
to isolate these two wrappers, while Rust also tests the actual core fetch path.
See [position metadata evidence](POSITION_METADATA_EVIDENCE.md) for the two
new mappings, first-position controls and shared string-boundary witness.

`spot_private_class` binds all eight Spot private arrow bodies and prototype
`dispose` directly to Rust functions over a live receiver. `PrivateClassOps`
supplies language operations; `frame` and `orders` must be pure views of that
same receiver. Async arrows execute eagerly and retain completion independently
of the caller's returned promise. The focused `spot_private_class` target
compares native state and identity first, then executes the maintained whole
TypeScript class with dependency fixtures. This does not prove a full socket
transport or a concrete JavaScript engine implementation.

`futures_private_class` binds the ten futures private arrows using the same
`PrivateClassOps` language contracts as the Spot class. It reuses native setup,
routing and order algorithms, while receiver fields own subscription and timer
state. Reconnect retains the previous socket across listener removal and close;
its async arrow runs eagerly and retains completion. Dispose is a lexical arrow
with the class home retained separately from the instance. The two class tests
share a finite language fixture, not venue method factories.

`spot_public_class` installs eleven native public arrows over the same language
capabilities, with live subscriptions/routing and existing market-stream bodies.
Its three-group oracle finishes native assertions before wire/source comparison;
see `SPOT_PUBLIC_CLASS_EVIDENCE.md` for the finite composition floor and controls.

`futures_public_class` binds the explicit constructor and seventeen public
arrows, with native sharding/channel/message logic and existing market-stream
algorithms. Its four-group proof counts clocks independently; the earlier
constant-clock oracle survivor and its repair are recorded in
`FUTURES_PUBLIC_CLASS_EVIDENCE.md`.

Production account/validation operations use `futures_native_accounts` and
`spot_native_accounts` through their constructed classes. The unused typed
`accounts` module and its private `response_default` parser are removed, with
exclusive account/validation/notification tests and oracles. Shared transport
parsing stays canonical and untouched. Existing native/source account cases
remain, and the paired package/CI guard now includes `exchange_trait` in the venue oracle command for real
constructed HTTP/account field observations. A compiled native user-ID mutation
leaves all eight retired copy tests green and fails that actual binding test;
exact restoration passes. Historical typed-account parser/refusal measurements
in evidence files are not production coverage. Shared typed account projections
retain their separate declared limitations.

`spot_native_policy` adds native value bodies for Spot pricing, position
projection and leverage/mode policy. Existing typed helpers remain mapped;
these components are not yet installed by the owning exchange class. Its
finite language fixture now distinguishes own definition from prototype
assignment after a production mutation survived the earlier instrument.

`KucoinDependencies.hosted_signer` supplies the same optional account capability
to the shared BaseExchange bridge and concrete per-options HTTP host. Native
registry hosts retain it for primary and unlimited clients. Private WS auth
continues to obtain a bullet-private token through HTTP; no separate hosted WS
protocol is introduced. Existing local hosts supply None. Actual-source/native
evidence and omission controls are in [HOSTED_SIGNER_EVIDENCE.md](HOSTED_SIGNER_EVIDENCE.md).

The concrete StreamOps for-of host consumes Js::iterator, captures next once,
validates step objects, and closes on a body failure while preserving that throw.
It does not replace iterable inputs with an array snapshot. The real connected
`native_iteration` socket witness compares sixteen raw orderbook update frames
with the maintained listener and actual orderbook utilities. String payloads,
including astral Unicode, retain source iteration and native NaN levels; truthy
non-iterables report one listener exception. Callback count and every price,
amount and total are observed before JSON numeric projection. The test-only
commit compiles and fails on master at the unexpected string-input exception.
