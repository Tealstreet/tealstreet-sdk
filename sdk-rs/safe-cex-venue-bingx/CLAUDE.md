Every concrete mapOrder caller uses the shared synchronized VirtualClock: REST
reads, private order frames, accepted placements and cancel-replace replies. The
real binding clock regression checks all four; fetch throttle clocks stay Date.now.

# BingX venue port

Behavioral reference: [TypeScript venue guidance](../../packages/safe-cex/src/exchanges/bingx/CLAUDE.md).

`constants.rs` mirrors the configuration and mapping declarations in
`bingx.types.ts`, using shared domain enums. Ordered pairs preserve own-property
order, including TRIGGER_MARKET before STOP_MARKET: actual `inverseObj` chooses
the last matching value. Ordinary object lookups must use shared
`safe_cex_num::js_table_lookup` to retain inherited identities; an own-only
search is not a replacement for TypeScript bracket access. These declarations
do not implement their REST, order, or WebSocket consumers.

`tests/constants_oracle.cjs` executes the maintained TypeScript modules with the
compiler selected by required `SAFE_CEX_TYPESCRIPT_PATH`. It does not stub enum
values or regenerate a shared fixture. Native typeof/String observations happen
before JSON serialization. A shared OnceLock captures the complete oracle corpus
once per test executable, never once per case. `tests/constants.rs` compares all runtime exports,
ordered pairs, inverse results and inherited identities; RECV_WINDOW also has a
native binary64 comparison. This is a constants witness, not a wire decoder or
consumer witness. Response decoders use shared numeric ingress as described below.

Scope all Cargo commands to `-p safe-cex-venue-bingx --jobs 1`. Run the crate's
live witness tests with `SAFE_CEX_TYPESCRIPT_PATH`, plus host and separate
wasm32-unknown-unknown strict clippy. `tests/constants_controls.py` temporarily
mutates one local source at a time, requires a named failure and a restored pass,
and restores source bytes in a finally block. Run it without concurrent edits.

Measured slice scope and mutation assertions are recorded in
[GATE_EVIDENCE.md](GATE_EVIDENCE.md). The scoped parity runner uses the actual
Cargo-reported executable, so target-directory overrides cannot select stale tools.

`types.rs` mirrors the declared wire records and string unions from
`bingx.types.ts` and the positions response from `bingx.api.ts`. Private serde
companions keep every public record's Deserialize behind `JsNumberInput` and the
shared strict map guard. All numeric fields are f64 and every public type has an
explicit, exhaustive expression-macro output registration. Optional R distinguishes
absence from present false and rejects present null. String unions decode strings
only; serde's externally tagged object form is not a TypeScript literal union.

The types witness uses the real TypeScript checker for erased declarations and
executes the maintained types module. Its inherited-field control must remain:
use resolved properties, not InterfaceDeclaration.members. Fixtures distinguish
same-typed fields, and native field observations are separate from serde metadata.
Wire assertions use shared JS numeric projection only after native-bit checks.
One Node invocation emits the complete corpus. Lone-surrogate witnesses compare
exact TypeScript code units and escaped output through both canonical retained
parsing and raw capture. Both shared and bare record decoding use numeric capture
and retain the record, replacing lone units with U+FFFD only in typed Strings.
Direct native member observations preserve every other distinct fixture field
and numeric bit pattern. All 883 source cases and 3659 typed route comparisons
remain pinned. No unchecked Axios runtime-validation equivalence is claimed.

Use `--test types` while working on types, and `tests/types_controls.py --only
'<row>'` for targeted mutation checks. The full runner proves one production
mutation per type row plus the inherited-field census control, restoring each
source in a finally block. Do not run another Cargo invocation during controls.

`api::sort` reuses the shared root collator. Its row explicitly chooses root/English
ordering rather than a host-dependent locale; UTF-8 keys have the shared boundary.
The sort oracle extracts and executes the exact source initializer with the actual
TypeScript compiler, with no replacement dependency modules. It pins the capture's
English environment and records Node, architecture, ICU and resolved locale.
The corpus compares native Ordering; `sort_controls.py` proves both reversed Rust
arguments and reversed actual TypeScript arguments fail `native sort ordering`.
One Node process handles all 784 pairs and three surrogate witnesses. The API
factory and query-signing behavior remain pending.

BingX double-parse behavior is UNMEASURED. Constants, declared-type and comparator
witnesses do not measure the transport/API parse chain; do not infer either the
presence or absence of a double-parse defect from these rows.

Store inventory at ed82bf3301 is AUDITED ZERO: no Store dependencies, no production
Store call sites, no FINDING rows requiring migration; existing map_err calls
convert only deserialization failures. Future Store consumers must preserve
notification identity when propagating. Verified at fetched integration
9af82bfeaefa3496cd8eea0a29b9ce7d3c1741a7: ExchangeErrorMetadata.store_subscriber_ids
is Vec<u64>; both shared conversion branches append notification.subscriber_id()
inner-to-outer. The named shared-core structural-retention FINDING is CLOSED at
that integration SHA. The utility branch now includes integration 710e663156c8;
no current BingX consumer depends on the field.
No local sidecar or boxed Store error exists to remove. Ids remain Store-LOCAL,
never globally unique.
The two error-message
helpers in errors.rs perform no Store or transport error conversion.

`ticker::derive_ticker_bid_ask` mirrors the synthetic quote helper using shared
MarketPrecision and safe-cex-num add/subtract/math_max. The return tuple is (bid,
ask); optional precision covers the source optional lookup. The actual-source
oracle executes the maintained helper and safe-math/isNotNumber modules together.
One Node invocation emits 168 native-bit cases, canonicalizing only NaN. Each
bid/ask precision swap has its own named mutation assertion. The historical zero
precision audit is superseded for these two fields only; order formatters remain
pending. The preserved original capture is evidence, never a replacement oracle.

`market_utils` extracts classification and symbol resolution from the maintained
class. It consumes shared Market values and ordered index entries; it does not
construct or mutate a Store. Market objects remain truthy even with empty IDs.
Shared js_table_lookup preserves standard inherited property hits (which suppress
the second index) and last-own-entry wins. Prototype mutation is outside that
shared lookup contract. The witness invokes actual source arrows with real plain
object indexes, including own __proto__ properties made via Object.fromEntries.
Its 75 classification and 138 resolution cases have independent source and
executed-comparison counts. Both outputs are scalars: there is no native output
collection or precision operand in these two helpers. The permanent controls
exercise empty/subset/extra source corpora with and without admission guards.

`precision::parse_precision` retains `_parsePrecision`'s String coercion,
parseInt inferred radix and literal zero-appending loop, using shared numeric
helpers. JsPrimitive Number/String are the source's declared input union; other
variants return None. It does not clamp enormous positive precisions or introduce
a resource limit absent from TypeScript. The witness executes the actual method
body and parameters, with fixed 27-string/18-number comparison counts. Numeric
cases use native bits, NaN-only canonicalization, and capture Node/architecture.
This is a scalar precision-string utility, not a price/amount adjustment site.

`account_reads` ports primary balance retrieval and account identification over
an injected configured request callback. It builds shared RestRequest and receives
shared JsNumberInput / ExchangeError; no local transport or error representation
is introduced. This seam is after the pending API factory's response transform,
so it does not measure the double-parse chain. Shared passthrough decoding retains
runtime response fields and numeric lexemes; it does not enforce the erased TS
wire interface. Request rejection propagates unchanged; coercion TypeErrors retain
name and unprefixed message. The crate now depends on exchange and transport
(shared Store is transitive); the historical zero-dependency inventory is not a
claim about this graph. There are still no venue Store mutation call sites here.
The account-read witness retains its original 68-case capture as evidence and
adds ten raw-input numeric cases. Observe primary-row userId bits before JSON
projection and getAccount's userId string directly; JSON alone erases signed
zero and infinity. These are helper-input observations, not measurements of the
still-pending API response-transform chain.

Account-read input admission also compares the entire method/input sequence against
`tests/fixtures/account-reads-inputs.json`: 68 ordinary, 10 numeric-lexeme and two
UTF-8 boundary inputs. Runtime metadata and expected results are excluded; expected
results always come from live TypeScript execution. The archived capture digest is
historical evidence, not itself an executable content guard. Same-length replacement
and reversal controls exercise all three input guards. Cardinality bypass controls
remove both the length and content admission guards, retaining native comparisons
and fixed executed-method counts.

## Production binding

`binding::BingxExchange` (also re-exported at crate root) assembles the landed
`exchange::BingxExchange` bodies on shared `safe-cex-exchange::js_runtime`: one
ScopeShared/Store, Axios client with raw-text bigint parsing, common intrinsics,
and public/private BaseWebSocket facades. `BingxDependencies` supplies the HTTP
wire host, WsRuntime and optional external-public-data factory. Secondary keys
route GETs to the secondary client and writes to the primary. Hosted HTTP signing uses the shared native host capability; the BaseExchange
signer view and API skip-local-signing branch receive the same capability.
Payload-signing options remain unavailable without a host capability.

The secondary read client's proxy is the first truthy secondaryProxyUrl or the
provider's public CORS proxy. It always replaces the primary overrideProxyUrl,
including clearing it for direct reads when neither resolves. Primary writes
retain their own proxy and credentials. The production binding proxy regression
checks routed URLs through DirectTransport for explicit, empty and absent
secondary proxies, both with and without a public proxy.

All 16 Exchange trait methods are bound. The source inherits the base rejection
for fetchSymbolMaxLeverage, scheduleCancel and fetchPublicTrades; their tests
assert the exact Error and zero requests. Other hooks delegate to the existing
venue bodies rather than duplicating their algorithms. Existing manifest
divergences remain priced at their original rows. This binding does not claim
whole-venue equivalence or resolve the remaining API helper rows.

`tests/binding.rs` drives each of the 16 methods with a scripted HTTP/WS host,
including caller order IDs, distinct amount/price steps, purity, and disposal.
The registry native-host proof exercises startup, signed placement and cancellation
over real loopback HTTP and WebSockets.

`scripts/binding-controls.py` compiles one deliberate failure for each of the 16
required Exchange overrides plus both public listener hooks, requires each named E2E failure, restores the source in finally,
and proves the restored suite. Run without concurrent Cargo/source changes.
Additional binding tests cover raw serialized POST signature resends (HTTP
200 and 400), the one-resend guard, ambiguous write refusal, and fresh signatures
on retryable reads. Signature decisions and payload preparation reuse the shared
signature-rejection port; the host wires its per-call stamp into Axios retries.

The shared binary socket bridge delivers native Buffer/browser Blob objects. The
binding retains a leased frame handle and delegates decompression to the shared
Store read_blob helper, preserving its pako byte widening. The typed handlers keep
their disposal guards after that await. The binary E2E proves both gzip and zlib
Ping/Pong on public and private protocols. Frame leases are released on completion,
error and cancellation. `scripts/registry-control.py` proves removing the actual
constructor entry breaks native startup and restores a pass.

Candle/book trait listeners dispatch to the existing public subscription receiver.
The shared OrderBook callback carries a value snapshot; unlike the source live
book object, an earlier native callback value retains its levels after cleanup.
That owned-callback boundary is priced on the listenOrderBook manifest row.

`scripts/binding-precision-controls.py` independently swaps the production amount
and price precision operands. Fractional wire assertions must fail separately;
integral placement inputs do not establish this protection.

The HTTP bridge uses shared `client::factory::{create,install_transport_adapter}`
for fixed20-second timeout, Axios Accept defaults, provider-selected routing and
proxy authorization. Source createAPI's installTransportAdapter call uses the
Default variant; raw-text response preservation still protects bigint parsing.
The selected adapter handles both initial requests and one-shot signature resends.
`tests/support/binding_factory.rs` checks non-default receive windows, Accept and
a real custom provider/adapter. `scripts/binding-factory-controls.py` independently
restores the old timeout, removes Accept and chooses BigInt; each compiled change
must fail its named E2E and restore to pass. Run this controller serially.

Native hosted-account coverage is in registry tests/native_host_bingx.rs. It starts
with no local API secret, authenticates its private socket through the remote-signed
HTTP listen-key flow, places/cancels, and checks the browser signer's HMAC over the
transmitted order fields. A stray local secret must not trigger local signing.
A refusing signer sends no HTTP request. hosted-signing-controls.py independently
removes HTTP forwarding, BaseExchange signer attachment and API local-signing
bypass; each must compile/fail the named native E2E and restore to pass.

The production batch-placement capability dispatches to BingxPlacement.place_orders,
matching the source whole-batch formatting and submission loop. A formatting
failure sends nothing; a submission rejection still allows later payloads to be
attempted before the final error is thrown. Real binding regressions verify a
rejected take profit does not suppress the stop loss, and a later invalid full
stop prevents an earlier entry from being submitted.

The binding witnesses now cover a replacement listen-key request held across an
old socket Close event: only one replacement request/socket is created. The shared
close-listener ownership provides this behavior; no venue fallback
or replacement reconnect path is needed. Private text frames cover order Store
writes, terminal fill emission/status, and position writes before REST healing.
Balance-only account rows remain ignored as in TS; this socket has no balance
handler and the REST balance stays unchanged. Approved strict-asset market row
refusals include base/quote and nonnumeric leverage maxima. They emit the row,
symbol, field and value while retaining valid siblings.
Source/transport failures retain the stored snapshot; all refused rows return
an empty fresh result.

REST signature timestamps read the shared synced `VIRTUAL_CLOCK`, matching
`bingx.api.ts`. Registry `native_host_bingx_clock` offsets server time by 90 seconds,
performs a real signed account read, verifies the transmitted HMAC, and requires
the timestamp to fall inside the synced interval. Raw runtime time remains the
clock for source `Date.now()` paths.

The concrete API serializer forwards the source's repeat-array and pinned
localeCompare sort options to the shared qs binding for both signing and final
wire serialization. The final sort includes the newly appended signature key.
`safe-cex-registry/tests/native_host_bingx.rs` proves the account GET sends
signature before timestamp and independently verifies its HMAC. Shared option
corpora cover nested and 12-element array key sorting and comma/form encoding.

Signature-predicate coercion failures retain their TypeError name and unprefixed
message through the shared ExchangeError conversion. The real native receiver
regressions cover both resolved and rejected HTTP responses without a resend.

Historical fill ISO dates without an offset use the required
`BingxDependencies.local_wall_time_to_utc` callback; explicit offsets bypass it.
The native registry supplies its UTC profile explicitly. The real binding test
compares actual TS executions in Singapore and New York (winter and summer).

Amendment copies preserve Order.reduceOnly presence through PreparedOrderOpts;
undefined is not reconciled as false. The real native updateOrder test supplies
own undefined without a raw reduce-only alias, exercises position-side lookup,
and compares the entire signed cancel-replace body to the maintained TypeScript
class plus actual API signer/qs/HMAC. Undefined reduceOnly is omitted at the wire.

The constructed native ticker corpus in `tests/binding.rs` covers 140 three-row
market.contracts frames, including every camel/snake numeric alias and distinct
later fallback values. Ten independent inputs include null, strings, malformed
objects, NaN, overflow and signed zero; large finite values exercise derived
prices. All eleven stored numeric fields are compared by bits with the maintained
TS constructor and actual DefaultStore. The shared fault runner misroutes mark
to index and requires a compiled native assertion failure and exact restored pass.

Held HTTP fixtures use the shared wake-aware Release gate. The registry native
listen-key witness holds a real reqwest response across the old socket Close
acknowledgement and verifies one replacement dial after release.

Opaque info uses RetainedJsonInput from HTTP/private-frame ingestion through
mapping and Store. Data reads, amendment, placement and order-history responses
share this retained carrier; only consumed typed fields perform native coercion.
The real NativeRuntime WebSocket regression compares the stored raw order with
maintained TypeScript using numeric bits and UTF-16 units. Per-row asset/leverage
admission and visible refusal remain unchanged.

`public_stream::HandlerList` owns live callback registrations, not a raw decoder.
`scripts/handler-list-contracts.mjs` measures its actual compiler alias through
the maintained accepted inventory. The constructed production socket witness
`native_callback_registry_keeps_live_lists_and_original_errors` observes append
to a captured list, next-frame registry replacement, original parsed payload and
callback error/code referents. `tests/handler-list-controls.py` replaces the live
list capture with a copied list; the named native dispatch assertion must fail
before exact source restoration passes. No probe-family relaxation is used.


The typed-native public async family uses shared `eager::start_eager` for all 18
implemented SDK async methods, retaining the existing runtime and native receiver.
`tests/divergence_timing.rs` covers call-prefix errors/logs and a held mode-change
continuation after unpolled waiter drop. The lifecycle child probes cover live
refresh/recovery and full silent teardown on final exchange-handle drop.
`scripts/divergence-controls.py` compiles eleven independent production faults,
requires named failures, restores exact source bytes and replays GREEN. Run it
serially with no concurrent source edits or Cargo commands. Always run the full
workspace parity index before marking a PR READY. See DIVERGENCE_TIMING_AUDIT.md.
