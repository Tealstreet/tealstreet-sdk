# Coinbase venue parity port

The production private write-check timeout emits warning arguments as
`(reason, {forceToast: true})`; retain the second emitter argument and the
once-only latch until a received user frame resets it. Portfolio selection
projects only `uuid` and `type` into mutable `PortfolioState`, retaining unused
metadata as RetainedJsonInput. The declaration DTO remains strict; missing or malformed
`name`/`deleted` cannot reject runtime selection or balance reads. Production
regressions are in `tests/private_binding_frames.rs` and `tests/exchange_binding.rs`;
`scripts/health-portfolio-binding-controls.py` compiles each boundary reversion,
requires its named failure, and restores the source byte-for-byte.

Behavioral reference: [TypeScript venue guidance](../../packages/safe-cex/src/exchanges/coinbase/CLAUDE.md).
Core contract: [shared exchange trait](../safe-cex-exchange/CLAUDE.md).

The concrete API host uses the shared client factory and optional transport adapter
from the runtime provider. Preserve executable adapter and query-serializer handles
across the native signing carrier; neither is a JSON field. Real constructor/read
witnesses and eight maintained TS factory/API scenes cover provider selection,
no-adapter fallback, tunnel gating and Authorization header selection. Run
`scripts/transport-binding-controls.py` for four independent compiled production
reversions, and `scripts/private-binding-controls.py` for real private frame and
fill-dispatch controls. Private user orders map snapshots/updates into Store and
emit fills; the source does not dispatch balance or position topics.

`tables.rs` mirrors every exported protocol table in `coinbase.types.ts`.
`protocol.rs` mirrors the inert Starbase exported protocol constants, including
closed activation flags, refresh periods, timeframe resolutions, and evidence.
`starbase.rs::resolve_coinbase_starbase_endpoint` exhaustively maps the closed
transport-kind union; its live oracle compares both table values and resolutions.
`starbase_public.rs::is_supported_public_channel` implements the inert public
frame channel grammar; the maintained oracle checks the finite tables and invalid
segment/value cases before any websocket handler uses the classification.
`starbase_public.rs::has_exact_keys` uses `safe_cex_num::js_own_keys` and preserves
Reflect.ownKeys behavior for own JSON keys, including integer-index ordering.
The frame parser shares `starbase.rs::is_record` with the RPC parser; the two
TypeScript helpers have separately measured source corpora and explicit manifest
notes for the shared Rust predicate.
`types.rs` mirrors its finite JSON wire shapes, with separate enums for literal
unions. Optional fields omit absent values and reject explicit null. Struct
adapters require JSON objects because Serde's default struct decoder also accepts
positional arrays. Nested anonymous TS objects have named Rust types recorded
in the owning manifest row. Unknown object keys follow structural TS typing;
they are not preserved by these declared-shape DTOs.

The maintained oracle runs via root `yarn venue-oracle:coinbase`. It evaluates
the current checkout's TS tables and derives field/union cases from its AST,
comparing ordered table entries and checking missing/null/wrong-type inputs.
A bidirectional TS/Rust AST schema comparison also catches removed optional
fields and narrowed unions that example-only tests cannot see.
It requires Node and TypeScript (`SAFE_CEX_TYPESCRIPT_PATH` selects CI's isolated
compiler). The oracle never regenerates Rust or blesses a changed expectation.
See [gate evidence](GATE_EVIDENCE.md) for mutation checks and limitations.

Variants stay inside this crate. The production `CoinbaseExchange` binding composes the existing native class
and socket bodies; see [binding gates](BINDING_GATES.md). Starbase stays inert. Coinbase's TS read retry classification is `networkErrorOnRead()`;
placement tests must consume the centrally supplied Store implementation rather
than inventing a venue-local Store.

`utils.rs` ports USD/USDC alias precedence and order-state/type switches. Console
errors are injected callbacks so unknown-value logging remains observable. The
resolver retains nullish-vs-truthy handling for every own JSON value. Its sole
CORRECTED divergence rejects inherited Object.prototype market keys; both TS
and Rust outcomes are pinned by the live oracle and explained in the manifest.

When implementing the exchange, preserve the empty capability override and
register it as `empty_override`; an unbranded implemented-empty response is the
TS defect, not a request to invent exchange support. Retry classification remains
central; do not invent TransportError network retryability locally.

`orders.rs` preserves REST configuration precedence and uses the central
`number_from_string` for Number coercion, plus safe-cex-num arithmetic. It keeps
raw RetainedJsonInput under Order.info, including unknown fields and their insertion order.
The mapper's manifest documents inherited-status rejection and the core Order
serializer's extra-last field layout. Root-order inputs otherwise follow the
declared LIST_ORDERS_RESPONSE shape; malformed whole-frame validation belongs
to the response boundary.

`starbase_session.rs` keeps the inert cancel-on-disconnect builders and strict
result decoders. Absent scope differs from explicit null; get results require
exactly enabled/scope keys. Live TS checks schema changes in both directions,
request values, error precedence, and the protocol error name/message/display.

`starbase.rs` ports transport-free method, record, safe-ID and RPC-error guards.
ASCII method matching retains the TS regex grammar without trimming, Unicode
character classes, or a regex engine. RPC error data preserves missing/null;
integer codes retain JS binary64 semantics. Finite JSON is the wire boundary.

`starbase_cutover.rs` keeps closed status/blocker unions, false-only activation
fields, and ordered blocker collectors. Its oracle enumerates every valid
combination of the eleven evidence fields. The evidence validator uses shared JS own-key enumeration and preserves the
first-empty-unknown-key truthiness bug. Test-only readiness accepts object options;
the production selector takes no arguments and consumes only repository constants.

Accepted-shape gates inventory all 21 legacy and 21 cutover enums against live
TS and execute trait plus concrete qualified decode calls. The 41 string unions
use string-only front gates and private function-local derived Wire enums; the
false-only singleton remains a boolean decoder. Every one of the 116 decoded
records has a map-only front gate, with complete positional-array rejections.
Five protocol/session records and the session scope are serialize-only; their
explicit parser functions own validation. No remote Self decoder is generated.

Starbase auth results and the bearer singleton are serialize-only. The explicit
validator keeps nonempty untrimmed tokens, arbitrary string scopes and positive
integer expirations without imposing a safe-integer bound. Its maintained live
TS oracle covers rejected values and positional record arrays too.

`starbase_public.rs` begins the inert public-data contracts and validators. Its
closed enums are serialize-only with explicit decoder classification. Validators
retain source comparison direction, including NaN minimum behavior, and use the
shared JS formatter in diagnostics. Its live oracle checks rejected shapes,
empty strings, BOM/NEL, integer boundaries and exact error payloads.

`CoinbaseStarbaseSequencedOrderBook` wraps the shared sequenced-book operation.
Its `order_book()` returns the persistent Arc/Mutex handle. Release that lock
before calling adapter methods; the adapter releases its own read locks before
calling the shared operation. External handle mutations remain observable.
Action validation deliberately precedes duplicate/stale sequence filtering,
including the marked replay-new-level recovery defect in the TypeScript.


`starbase_private.rs` keeps private-state contracts and raw-value guards inert.
Its DTOs are serialize-only; the explicit source decoders remain separate rows.
Price is a number-or-market_price union, and parity results retain the ready and
blocked fields separately. The private positive-number diagnostic differs from
the public minimum guard, so their validators are not interchangeable. Missing
optional inputs differ from present null; collection guards borrow their inputs.
`CoinbaseStarbaseReadOnlyState` mirrors the seven TS snapshot operations: each
fully decodes and validates before replacing its field, so a failure leaves the
previous snapshot intact. Order history by instrument replaces the same history
field as unfiltered history; the state oracle checks the sequential results.
`normalize_coinbase_starbase_order` preserves blocker order and constructs a
shared Order for ready inputs. Its CHOSEN row records that TypeScript `info`
and blocked gateway retain source insertion order and runtime-only own fields;
ready `info` aliases the exact gateway object, so later gateway mutation updates
`info` while mapped Order fields stay snapshots. The Rust typed gateway DTO and
copied retained map preserve declared fields and numeric bits while losing alias
identity and caller-only fields. Native `info` numbers retain NaN, infinities and
negative zero. The live oracle bit-checks each direct
float output and every present numeric `info` field before JSON erases -0; both
take-market `take_profit` and stop-market `stop_loss` trigger branches are
covered by tagged negative-zero witnesses.
The Store notification reachability audit follows the TypeScript call graph.
The three private-health handlers preserve the original `StoreMutationError`,
including Store-local subscriber identity and source error cause. Degraded/auth
handlers use the existing `PortfolioFailure` carrier for Store and callback errors. Their
regression test pins commit-before-error, ordered fanout stopping at the first
failure, and skipped log/warning effects. Other Coinbase Store writers remain
pending or divergent with a `FINDING` note until their handlers propagate the
fallible result.

`starbase_orders.rs` starts with serialize-only write-contract literal unions.
The write-orders oracle executes the actual TS protocol tuples and checks strict
literal inference and every emitted value. The basic place-order and OTOCO
builders are exercised by `write_order_builders_live_typescript_corpus` against
the executed TypeScript factories. Rust serialized parameter field order follows
those factories. The three blocked place-order builder rows are CHOSEN
divergences: TypeScript returns the caller intent object with its insertion
order and any caller-added own properties, while Rust serializes the typed
intent by declaration order and drops unmodeled properties. The corpus pins
both differences. Preserving arbitrary caller object details would replace the
public typed contract with a generic insertion-ordered map.

`tests/numeric_ingress_types.rs` audits 13 public numeric record fields in
`MarketResponse`, `IntxInstrumentsResponseItem` and
`PortfolioBreakdownResponseBreakdownSpotPositionsItem` at raw JSON and owned
`Value` entry paths. Raw `from_str` and in-memory floating `Value(-0.0)` retain
negative-zero bits, while parsed integer-token `-0` becomes `+0` after conversion
to `serde_json::Value`; those 13 parsed-Value results are recorded as findings.
Compare bits before serialization. Raw exponent overflow is measured separately.
No DTO production caller was found, and this evidence excludes shared numeric
record graphs listed in MANIFEST-SPEC section 17.

`private_health.rs` ports the private-stream message-received, degraded, and
auth-failed transitions against the production `DefaultStore`. Store failure
propagates after commit and before logging. Log and warning callbacks are fallible:
a log failure prevents the warning latch and emission; a warning failure escapes
after setting the latch. Constructed private-write timers and an authentication
error frame retain the original native callback cause and source retry behavior.
The three existing health rows remain ported; other notification findings retain
their independent scope.

The Store notification reachability audit follows the TypeScript call graph and
allows only the three health rows whose Rust handlers propagate the fallible
Store result. All other direct writers must remain pending or divergent with a
`FINDING` note until their handlers propagate the result.

`decimal.rs` ports the two order-submission string guards without coercing JSON
values. They accept only strings and implement the source's closed ASCII decimal
patterns; Unicode digits, BOM and whitespace remain rejected. Their live oracle
reads the patterns and executes the TypeScript predicates.

`private_order_snapshot.rs` ports the private websocket snapshot reset, ordered
merge, and batch-application helpers. The batch threshold uses the current raw
batch while quarantine uses the accumulated raw count; Store notification
failures propagate after the snapshot state and Store are committed. The live
oracle executes the three production TypeScript methods in one Node process.
The websocket event handler remains pending and must propagate the returned
Store error when it is implemented.

Credential parsing and normalization use one live TypeScript subprocess. The
Rust tests pin all nine parse inputs and all 54 key/secret normalization pairs,
with independent outcome-branch counts; same-length case substitutions fail
the named input-identity assertions. Public-frame tests similarly pin every
channel, exact-key, and record-predicate input rather than relying on corpus size.

The write-edit oracle gives `reduceOnly` and `rejectPostOnly` opposite-valued
cases and checks the native `EditParams` fields directly. Bit captures record
`process.arch` and the Node major; NaN payloads canonicalize to
`7ff8000000000000`, while finite values and signed zero remain bit-exact.
Coinbase's local numeric corpus is consumed only by this crate's tests; it is
not the separate KuCoin corpus.

Field-identity audits preserve shared TypeScript fixtures and evidence byte for
byte, adding separately labeled supplements in Coinbase's Rust oracles. The
21 legacy and 8 cutover generic wire checks recursively assert decoded native
fields before comparing re-encoded shape. A per-type execution count lives inside the passed identity closure, so omitting
its invocation cannot increment the count. All 29 generic wire corpora pin their
full content with frozen SHA-256 expectations captured at `1857224642`; labels,
values, validity flags and order are included. Recursive field checks compare
native strings, booleans, f64 bits and exhaustive enum matches, including absent
optionals, before the wire roundtrip. All 42 legacy/cutover enum types have direct
native variant checks. Six bit-capture
corpora can contain NaN; `special-numbers.cjs` canonicalizes only NaN to
`7ff8000000000000`, retaining the host architecture, Node major, signed-zero
bits, and all finite bits. Numeric-ingress and private raw-ingress inputs were
checked separately and contain no NaN.

The pure signing helpers mirror URL-safe base64 of bytes and UTF-8 strings,
ASN.1 tag/length conversion (including the source's truncated two-byte length),
and SEC1-to-PKCS8 wrapping. The live signing-helper oracle asserts native strings
and byte vectors at length boundaries and malformed PEM inputs. PEM decode failure
returns a unit `CoinbasePemDecodeError`; its CHOSEN row records the discarded
platform DOMException metadata. Production key import and JWT signing are supplied by `binding/crypto.rs` over shared ES256/Ed25519 primitives; native/WebCrypto tests cover the assembled signing host.
The native identity inventory compares all 116 record field sets against the
production Rust AST, including multiline nested types.

The inert `starbase_rpc.rs` module ports the JSON-RPC client, shared params/request handles, HTTP transport factory, RPC rejection fields, and heartbeat operations from `coinbase.starbase.ts`. It does not activate Starbase in the current exchange. Local callbacks run before the returned future is polled; the response parser observes transport changes to the same request handle. DTOs are outbound only. `tests/rpc.rs` compares native fields and branches against the maintained live-source oracle `scripts/rpc.cjs`; the scope is finite owned JSON, not arbitrary JavaScript object identity in returned response frames. Auth session remains pending.

The JSON duplicate-key grammar now maps to `json_key_scanner.rs` method by method. Keep raw strings and decoded keys as UTF-16 units, including lone surrogates, and preserve the source 128-depth check, invalid-before-duplicate precedence and repeat-scan state. The scanner does not port the still-pending frame classifier. Gates are recorded in `SCANNER_GATES.md`.

`starbase_auth.rs` and `starbase_ws_controller.rs` port the inert auth and WebSocket lifecycle controllers. Native completion Promises preserve identity and queued reactions; polling a returned handle cannot drive its producer. Host spawners must poll producers to their first suspension, enqueue wakes as FIFO microtasks, and run without requiring callers to await. Promise reactions share a registration-ordered queue across observer callbacks and Future waiters. Source await continuations and async Promise adoption are explicit. Auth keeps raw validated JSON and shared cache aliases; activation retains mutable policy references and checks its generation after each awaited stage. The live `auth-session.cjs`/`ws-controller.cjs` oracles cover cache boundaries, ignored calls, overlap, policy readback and queued disconnects. No current exchange or socket is activated.

Coinbase create/cancel and REST fallback bodies are mapped individually in `order_create.rs`: amount/price precision formatter, optimistic native Order carrier, own typed market resolver, hot Promise placement/cancellation, order polling/backoff and fresh WS/REST merging. Production Exchange assembly lives in `binding/`; its gate scope and remaining notification findings are recorded in `BINDING_GATES.md`. Explicit receiver capabilities preserve operation boundaries; transport returns native hot Promises. `CoinbaseOrderResponse` retains a shared root and shallow object/array field identities; primitive info fields copy. Tests use the actual installed p-iteration implementation, decoded native values and source error/timing/alias probes. String response IDs and own genuine Market records are the declared typed boundary; inherited non-Market prototype outputs are excluded.

Public frame classification is complete in `starbase_public_frames.rs`, using `json_key_scanner.rs` before a guarded native JSON.parse bridge. Retain UTF16 strings/keys, f64 overflow/underflow/signed-zero, shared array/object payloads and primitive copies; serde Value cannot replace this data carrier. Context remains a plain typed JSON record. This module owns no socket or dispatch. Gates: `FRAME_CLASSIFICATION_GATES.md`.

JWT signing and private user-order `lastUpdate` read the synchronized shared
virtual clock, matching their source call sites. Polling, private health and
optimistic REST timestamps retain the source Date.now runtime clock. The native
clock regression syncs server time, signs real REST JWTs and delivers an opened
private socket frame for both Coinbase variants with an unrelated host clock.

`PlaceOrderRequest` serializes `leverage` before `order_configuration`, matching
the source formatOrder object literal. The production transport regression
observes exact encoded POST bytes for Coinbase and CoinbaseSpot; it drives the
real class graph, signers, client factory and installed transport adapter.

CDP key-name validation requires exactly one organization segment and one key-ID
segment, matching the source regex. Slashes inside either segment are refused
before local JWT signing or HTTP dispatch; Unicode and interior spaces stay
accepted. The constructed credential binding witness checks eight actual-source
profiles through WS signing and rejects an invalid REST key before transport.

Concrete order generation, client-ID minting and JWT nonce capabilities use the
shared source UUID helper (`safe_cex_crypto::uuid::uuid_with_entropy`) with runtime
entropy: generate nanoid21, then remove both separators. Generic runtime.unique_id
has a separate host contract. Constructed UUID regressions exercise both variants,
exact POST client IDs, optimistic parent IDs, mint entropy consumption and signed
JWT headers; Node-derived Ed25519 public keys verify the actual wire token.


The native ticker census in `tests/exchange_binding.rs` verifies the maintained
source has no active ticker_batch handler: its mapper and registration are
commented out in TS. For both constructed Coinbase variants, 56 malformed
three-row batches preserve all ticker numeric bits and produce one visible
unsupported-channel report per frame. The shared control runner changes the
actual dispatcher diagnostic and requires a compiled native assertion failure
followed by exact-restored success. This does not invent ticker WS support.

Coinbase runtime HTTP response data and raw read/order roots use RetainedJsonInput
through mapping and DefaultStore. The native loopback regression in
`tests/transport_binding.rs` compares real reqwest ingress with maintained source,
including overflow, negative zero, lone UTF-16 and native own undefined/NaN.
String fields actually consumed by typed models keep their existing UTF-8 boundary.

PortfolioState captures the canonical retained object directly; Serde flatten
buffers must not inspect opaque metadata. Metadata keys retain UTF-16, while
uuid/type remain strict native strings. The local Frame graph keeps own and array
undefined values during real merge/read operations, alongside native numeric bits.

The exported ListAccountsResponse and IntxInstrumentsResponse aliases keep their
wire-decoder roles. `scripts/wire-alias-contracts.mjs` uses the maintained compiler
context and native probe engine for exactly those two aliases; it reads every
nested field, row order/cardinality and f64 bits before reporting. Its `--controls`
mode independently truncates each decoded native vector and requires failure,
then exact restoration. These finite UTF8 JSON cases do not classify the other
aliases or remove the generic-registration guard in the full default inventory.

Thirteen generic capability aliases have exact compiler target registrations.
Boolean outcome/error instantiations, the real retained ReadItem order roots and
the production Host bound-method instantiations are reviewed separately; unknown
generic aliases still fail the existing identity path. Host and its associated
SocketHost are visible only within this crate for authenticated probes, while
Host.shared keeps its original binding-only visibility.

`generic_alias_native` in the library test target executes the production native
Promise spawner, retains pointer identity and opaque info/number bits, and captures
bound arrows from actual Advanced/Spot constructors. It does not claim every
callback body or every generic argument. `scripts/generic-alias-contracts.mjs`
authenticates those thirteen fixed types and classifies their non-decoder roles;
its controls independently remove the signing and bound-method registrations,
require the unchanged monomorphization refusal, then restore exact policy bytes.

Six portfolio/balance aliases have individual capability records: three real
Deserialize decoders and three non-decoder roles.
`scripts/portfolio-alias-contracts.mjs` authenticates their fresh compiler/artifact
identities and executes the three alias decoders; missing-contract mode requires
all six named failures. The native
`portfolio_alias_native` tests capture the actual PortfolioState through the JS
codec, inspect live array/row references, opaque metadata and numeric bits, and
preserve native balance snapshots plus the venue's error-cause/message carrier.
The approved JSON error view and JavaScript input-object identity stay separate.
`portfolio-alias-controls.py` removes production metadata retention, requires a
compiled native assertion failure and restores the exact source before GREEN.

Seven public-stream aliases retain explicit non-decoder capability roles. The
finite `scripts/public-stream-alias-contracts.mjs` runner authenticates compiler
identities and refuses any future Deserialize capability hidden by a role record.
`public_stream_alias_native` uses constructed Advanced/Spot classes, production
frame dispatch, the actual TickHost callback adapter and the native book adapter;
it observes callback/frame/book roots, live iteration, errors and numeric bits.
`public-stream-alias-controls.py` changes the real dispatch iteration and book
price projection independently, requires named compiled failures, restores exact
source bytes and reruns the corresponding witness. This is a finite capability
census, not full transport or all-callback coverage.

Eight configuration/signing aliases have specific non-decoder role records.
The finite `config-alias-contracts.mjs` probe authenticates exact current compiler
identities and refuses a new Deserialize capability. `config_alias_native` uses
constructed classes and actual request/private-option/account/candle/event
adapters, inspects shared native input roots and presence flags, imports the
maintained RFC signing key through CryptoHost, checks cache identity and verifies
actual websocket tokens with the Node/WebCrypto oracle. Amendment-root fields
are observed directly; full amendment and remote-signer flows are not claimed.
`config-alias-controls.py` independently changes actual key-cache identity policy
and event-array projection, requires named compiled failures, restores exact
source bytes and reruns the same witnesses.

Lifecycle timer roots retain the original shared-runtime Js Timeout handle.
The native host passes that exact value from setTimeout to clearTimeout; JSON
views cannot preserve its cancellation identity. Both Advanced and Spot native
class witnesses cancel a real timer and retain an independent uncancelled timer.
`lifecycle-timer-controls.py` restores the JSON-roundtrip defect, requires both
named native failures, restores exact source bytes and reruns both witnesses.
The fixed timer alias has a specific non-decoder capability receipt.

The shared retained/numeric typed decoder now replaces isolated UTF-16 units
with U+FFFD in materialized String fields instead of rejecting the entire record.
`tests/string_boundaries.rs` retains the live source corpus and exact unit/path
pins, measures that accepted typed-view cost, and checks that ignored strings
leave declared fields intact. Standalone serde_json::Value parsing and the
separate bounded recursion behavior retain their own explicit checks.
