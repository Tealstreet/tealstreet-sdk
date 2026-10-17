# ApexOmni port

Behavioral reference: [TypeScript venue guidance](../../packages/safe-cex/src/exchanges/apexomni/CLAUDE.md).

Protocol constants are ordered own-property tables; consumer lookup behavior is
not implied by the representation. Shared normalized order enums retain shared
identity. Venue wire enums use String-first deserializers and have no permissive
inherent decoder or conversion API. The live oracle executes the current TS
module and its shared enum imports, comparing every exported table value and
wire spelling; strict trait and concrete qualified probes reject extra shapes.

Run `yarn venue-oracle:apexomni`. Tests are also discovered by native Cargo CI;
`SAFE_CEX_TYPESCRIPT_PATH` is mandatory and selects its isolated compiler. The oracle executes only
Node builtins and repository source, requiring no extra npm runtime selectors.
Its in-memory TS mutation must fail the named Rust comparison at exit 101;
missing dependencies cannot satisfy that control. CI wiring is mandatory and
pinned in the workflow audit. The public records (TickerObject, DepthObject, TradeObject, KlineObject,
HistoryFundingObject, CurrencyObject, GlobalObject, PerpetualContractObject,
GetHistoryFundingOptions, KeyPair, ApiKeyCredentials and PerpetualCurrencyObject) have live compiler-derived field schemas and concrete
production decoder/serializer witnesses. Both the full positional sequence and
malformed nested fields must reject. The shared UTF-8 boundary is declared with
a double-escaped lone-surrogate witness; ordinary Unicode and astral pairs remain
positive. Remaining records, signing, mappers and exchange/socket operations stay
pending until their separate comparisons exist.

Record review independently found that fixed two-element arrays passed the first
corpus. Empty, singleton, longer and ragged recursive samples now reject that
narrowing at a named assertion. Numeric ingress now captures at every public record's Deserialize entry through
`JsNumberInput::decode_with`, then uses the shared map shape guard. Every record
has an explicit exhaustive `JsNumberOutput` registration. Serialize retains f64
native events; callers project to JavaScript JSON with shared `js_json_stringify`
when they need wire bytes. A field serializer that normalizes signed zero or
nonfinite values before ingress validation would hide a loss and must not return.
The live Node oracle captures typeof and binary64 bits before JSON serialization;
all 16 numeric fields are checked on raw, owned, borrowed and native Value entries.
Rust and TypeScript field-type mutations independently fail
at exit 101; compile failures are not accepted as controls.

Required boolean fields retain false and reject omission. Independent review
controls defaulting enableTrade or omitting its false output both compiled and
failed named assertions; live TypeScript boolean-to-string mutation also fails.

Optional record fields distinguish absence from present null. Omitted fields remain
omitted on serialization; independent Rust output-omission and TypeScript
optionality mutations each fail named assertions.

The inherited-field mutation adds a required parent field to TickerObject and must
fail the schema comparison. Native decoded field assertions use distinct values
and one-false-at-a-time boolean cases, independently of serialization roundtrips.

All six venue enums have native-variant assertions keyed by the actual executed
TypeScript member names; a symmetric decode/encode swap must fail those checks.

The interface now includes private-wire TimeInForce (GTT/FOK/IOC) and OrderStatus,
distinct from shared normalized enums. `Market` aliases `ApexMarket`; the type
alias preserves identity, while the value namespace deliberately excludes the
mutable JavaScript enum-object surface (the const row records this CHOSEN cost).
The protocol oracle executes the actual module and captures Market identity and
all members. Its successful corpus is cached once per test process. Per-row
comparisons continue after each panic so production mutations cannot mask a later
row. `scripts/mutate_protocol.py` mutates 21 rows in one compilation, requires
every named assertion, restores sources in finally, and verifies the restored
corpus in a second sequential --jobs 1 Cargo invocation. It is a mutation gate,
not a per-case oracle-spawning loop.

Market's erased type declaration is checked with the actual TypeScript compiler,
separately from its runtime value identity. Changing only the type alias target
must fail `ApexOmni protocol row Market type: TypeScript declared alias identity`;
transpilation and runtime object equality alone cannot witness this row.

Eight private records now correspond directly to the actual private interface:
WalletObject, OpenPositionObject, AccountsItem, AccountObject, WorstPriceObject,
FundingRateObject, HistoricalPNLObject, and AccountBalanceObject. AccountObject
retains optional accounts absence; a present null is rejected. All eight use
capture-before-decode numeric ingress and explicit exhaustive output checks.
The live compiler resolves the complete inherited field set. Native assertions
use distinct strings/numbers and one-false boolean supplements. The production
mutation runner swaps each row's first two string serde names symmetrically:
structural roundtrip assertions pass before all eight named native assertions
fail. Exact JSON property-order checks can additionally detect these mutations.
The record oracle is cached once per process for the entire normal corpus;
source-mutated corpora run once each. Oracle metadata records Node version,
major version and architecture. Raw JSON numeric cases do not include NaN;
signed zero is compared by exact bits.

Local continuation after 5d8164c530 adds MultiChainObject, SymbolObject,
PerpetualObject, SymbolInfoObject, UserObject and OrderObject. SymbolInfoObject
lists its own fields followed by the inherited PerpetualContractObject fields;
the live compiler census includes both and an inherited-base mutation must fail.
Opaque chains/userData/openTpParam/openSlParam are named passthrough fields;
shared retained input captures numeric events before constructing JSON Values.
Chains requires an array of arrays; userData distinguishes absent from present
null. JSON-only opaque payloads deliberately exclude JavaScript heap objects.
Four opaque paths have raw, owned, borrowed and shared ingress bit witnesses.

The six-row production corpus uses independent native paths: SymbolObject reads
its global feeAccountId, whereas PerpetualObject reads its usdc currency id.
This prevents a simultaneous nested-row mutation from substituting for proof of
the parent mutation. All other row axes touch independent record fields. Full
native field witnesses and one-false boolean supplements remain in the suite.
Origin is frozen at 5d8164c530 pending explicit release; subsequent work stays local.

Each config TypeScript mutation requires the precise affected row assertion.
The SymbolInfoObject heritage counter-control deliberately omits only a newly
introduced base field from that row's schema; unrelated nested-record failures
must not satisfy the inherited-field witness.

The remaining five interface rows are now mapped: ISO8601, WithPrice,
OrderWithClientId, CreateOrderOptionsObject and OrderResponseObject. Timestamp
strings remain unvalidated strings, matching the source union. The numeric
variant captures before decoding and supports integral visitor paths as well
as floating-point events. All three response timestamp fields have numeric-bit
witnesses. Native NaN comparison canonicalizes NaN only, records Node version
and both Node/Rust architectures, and keeps signed zero exact. Native-event
tests implement the transparent-newtype hook used by the shared capture layer.

Undefined-only optional properties use Option<()>: None represents absence,
Some(()) represents native present undefined, and both serialize without the
property. Present JSON null is rejected. WithPrice and OrderWithClientId each
have native-present omission assertions; a compiled wrong-serializer mutation
must fail the WithPrice assertion. Nullable response fields instead retain
absence versus present null through Option<Option<T>>. Actual TypeScript source
mutations cover all five rows and require the specific affected row assertion.
Create-order literal unions have native enum-variant comparisons, independently
of roundtrips. Earlier protocol and record witnesses remain in the scoped gates.

The utility corpus executes the actual apexomni.utils.ts module for getPrecision,
generateApeXRandomClientId and dropPrefix. The HMAC row stays pending; fail-on-access
crypto import placeholders make any accidental cryptographic execution fail.
Precision reuses the existing string-or-number union and shared coercion/formatting.
Randomness is host-injected through FnOnce, consumed exactly once; zero, subnormal
and exponent-spelled draws retain JavaScript slicing behavior. Prefix removal is
anchored and lowercase only. One Node invocation returns the complete corpus;
actual-source mutations each have their own corpus and exact assertion check.
The three production mutations compile together, fail three independent named
assertions, restore in finally, and pass. The runner uses two sequential Cargo
invocations and one Node process per complete corpus, never one per case.

Reviewed remote feat/rust-venue-apexomni-next is pinned at
10afdcdb2b938a64b2860d3247763089be87db01. Later utilities are isolated on
feat/rust-venue-apexomni-utils. Counts at the pinned head are 21 ported,
35 divergent, 112 pending. The fetched integration subscriber identity field is
retained by both shared notification conversion branches; this venue currently
has no Store writes or boxed errors, established by reading its production modules.

Local signer utilities now include timestamp, assert, four type assertions and
assertSubAccountId. Clock injection consumes one draw; floor and division retain
native f64 behavior. Type assertions consume shared JsJsonValue prepared data,
including undefined, BigInt, NaN and infinities. This is not a JS heap model:
foreign array realms, prototypes, executable values and symbols remain outside
the boundary. The live oracle demonstrates the foreign-array mismatch and the
UTF-8 message limitation. Result errors retain source messages, not Error object
identity or stack; assert.address is still outside the function port.

The subaccount assertion remains the source no-op. Production controls mutate
all seven rows in one compilation/corpus and require seven named failures. The
four type helpers first check independent successful cases, so a simultaneous
base assert error-message mutation cannot masquerade as a predicate mutation.
Every actual-source control checks its exact affected assertion. Normal captures
use one cached Node process for the entire corpus; each source-mutated corpus
uses one further process. NaN alone is canonicalized; signed zero stays exact.
PR 7256 pins the utility branch at 3c5cbc19a050905c35dddf484e39ecd4ccceacdc;
signer continuation stays local on feat/rust-venue-apexomni-signer-utils.

The bracket-only precision-swap audit at d50b9f8441 found zero implemented
price/amount formatter sites, not immunity. See [PRECISION_SWAP_AUDIT.md](PRECISION_SWAP_AUDIT.md)
for the inspected module closure and the distinct-step, direct-native-payload
mutation obligations on pending placement, update and TP/SL formatter rows.

`calculate_tp_sl_price` applies the source side multiplier before shared `adjust`.
Its actual-source corpus uses distinct fractional price steps; direct binary64
assertions precede any wire conversion. This does not cover placement bindings.

Eight range/string validators now mirror source guards and messages. Numeric
strings remain non-numbers, fractional values remain permitted where source
permits them, and the timestamp assertion compares against the constant epoch
threshold rather than validating the input's decimal digit count. Pubkey hashes
check only lowercase prefix and UTF-16 length; nonhex/astral values are preserved.
Display projection delegates to shared String(any); it does not coerce predicate
inputs. Nested arrays retain nullish empty slots and BigInt/nonfinite spelling.
Own noncallable toString errors retain their message, while Result<(), String>
deliberately projects away Error versus TypeError class, identity and stack.
The validator supplement leaves prior type/clock corpus values unchanged. Eight
compiled production guards each fail their own named direct-result assertion,
then restore and pass, with one Node invocation for each complete corpus.

Cardinality audit: record/protocol witnesses use whole-Value comparisons, but the
original utility and signer-validator loops accepted empty source subcorpora.
Source totals are now assertions, and scalar loops separately assert the number
of comparisons executed. Native record field inventories include inherited and
undefined-only properties; numeric subcorpora have independent execution counts.
The collection supplement covers 13 native record fields, including nonempty
nested arrays in chains, userData, openTpParam and openSlParam. It checks source
per-case size, decoded native length, a fixed native comparison total, and exact
values. Each axis catches its own failure so sibling mutations cannot mask it.
Protocol tables retain exact whole-value comparisons, with fixed source totals,
independent native length equality, and a fixed 19-row comparison census.

Run scripts/mutate_cardinality.py and scripts/mutate_protocol_cardinality.py
with SAFE_CEX_TYPESCRIPT_PATH set. They batch production empty/subset/extra
mutations, require named runtime assertion failures (compilation failure does
not count), restore all source files in finally, and run restored targets.
Source omission controls also bypass source guards and require independent
native/execution-count failures. Every invocation consumes one complete Node
corpus, not a subprocess per case. Original oracle script/fixture bytes remain
unchanged; the native collection supplement lives only in the Rust tests.

The decoder mutations consume input: Fields::deserialize finishes before the
production assignment drops decoded rows. scripts/mutate_consuming_cardinality.py
keeps an empty-wallet mutant installed while removing both source and native
length-guard functions. The expected remaining failure is the exact whole-native-
value assertion: this suite has redundant detection, not length-only protection.
Protocol controls additionally empty both source and native tables while bypassing
the source total; fixed native totals must still reject len == len == 0.

The signer wire `TxEthSignature` record now retains its two signature-kind
literals through a String-first native enum. The separate signer-record oracle
uses the actual TypeScript compiler and resolves inherited properties; it does
not add samples to any earlier record corpus. Ten samples cover both variants
and five signature strings through raw, owned and borrowed decoding, with sixty
fixed native field comparisons. The symmetric production enum control preserves
roundtrip output and fails the subsequent decoded-kind assertion. Transaction
signing adapters and the separate TxSignature packed/wire conversion remain
named pending work; this record does not claim to implement them.

Transaction declarations live in `transaction_records`: numeric flags retain
f64 (including -0) and validate 0|1; feeRates is exactly a two-number tuple.
The wire TxSignature is separate from the packed crypto primitive. Optional
properties reject present null, and all records capture numeric ingress before
decoding. The declaration oracle compiles the actual TS interface/type closure
without loading wallet/WASM imports; native field checks precede wire checks.
`mutate_transaction_records.py` uses one compiled control per declaration;
numeric controls compensate serialization so shared ingress checks cannot mask
the direct native assertions. Wrapper controls use independent child fields.
Transaction signing, packing and wallet/provider calls remain separate work.

Runtime bodies use shared BaseExchange/Store and explicit HTTP, socket and zkLink
host capabilities. The host owns imported SDK handles and polling; callbacks
must propagate shared ExchangeError metadata. Date-parser failures are forwarded,
not flattened into strings. Keep one SdkInitialization per host SDK module.
Hash/HMAC helpers take UTF-16 units only at TextEncoder input and preserve its
unpaired-surrogate replacement; this does not introduce a second Store model.

Native mapper gates inspect every returned field and enum variant before wire
serialization. Their object-field comparison does not claim property insertion
order or mutable JS alias preservation. Required shared enum/string/bool fields
refuse wider runtime source values visibly; retained info refuses native NaN.
Cancellation mirrors the source duplicate-base request when a regular order and
its composite brackets appear together. Blacklisting and Store removal precede
network calls, with ten-request lots and a one-second delay between lots.

`pure.cjs`, `signing.cjs` and `exchange.cjs` execute actual source declarations.
Their compiled controls are scoped to those new bodies; caller and helper
mutations run separately to prevent a helper failure masking caller evidence.

## Shared runtime binding (October2026)

The root `ApexOmniExchange` now assembles the existing typed bodies through
`binding` on shared ScopeShared, Axios, Store, globals, timers and BaseWebSocket.
The host owns both socket bodies; they retain their parent whose HostLink is
weak, avoiding a parent/socket/host cycle. API interceptors retain Weak<ApexApi>.
Callbacks release field borrows before reentrant disposal. Dispose cancels owned
timers/tasks; dropping the host cancels them too.

All16 required trait methods are wired. The source inherits exact
`Not implemented` rejections for fetchTickers, max leverage, position mode,
margin type, scheduled cancellation and public trades (base.ts759/799/832/836/989/1138);
those hooks make no HTTP calls. Both public listeners are also bound. The caller
client-ID guard is the first placement statement. Client IDs are preserved in
both wire fields and the contract nonce. The actual TypeScript placeOrder body,
its real signContract helper and the pinned npm SDK match the complete native
URL-encoded placement body, including its128-hex signature, fee and expiration.

The native zkLink host uses safe-cex-crypto's proven ContractBuilder/seed-only
signer. It performs WASM ToInt32/unsigned narrowing before building the contract.
No wallet provider or other transaction signing is supplied; unused capability
kinds return explicit errors. Missing/invalid seeds are visible to account
validation. Hosted HTTP/private WS signing uses the supplied remote capability;
L2 contracts still require the source Omni key seed. External
public data requires an injected factory.

The shared HTTP factory owns default timeout, headers, tunnel routing and adapter
selection. Signing preserves callable adapter/serializer values and unchanged
config-field identity; the typed legacy response bridge restores those client-default
capabilities before resend.

Network failures retry reads only. Apex's signature code20016/401/message
rejection in an HTTP200 envelope resends once through the existing legacy
signing body. HTTP failures do not enter that resend. Native errors retain HTTP
status, response envelope and config. Private login is awaited before subscribe;
both public/private callbacks use shared socket lifecycle and diagnostics.
Buffer text conversion is explicit at this venue's typed protocol boundary;
public browser Blob input preserves the source data.includes TypeError.

P4 disposal guards intentionally stop fetched positions/orders after the HTTP
await and stop the private position timer before publishing after disposal.
The source continues in these cases; the row notes price this difference.
Position reads retain the source's zkLinkConfig initialization while leaving
Store positions untouched. Order-book callbacks own snapshots; retaining the
callback result after cleanup differs from source's cleared shared object and
is declared in the row note.

`tests/binding.rs` has27 assembled tests, including all16 methods, both listeners,
legacy replay limits, HTTP metadata, write non-replay and disposal. Run
`scripts/binding-controls.py`, `binding-precision-controls.py`,
`binding-capability-controls.py` and `registry-control.py` serially: they temporarily mutate production bodies and
restore in finally. The18 method/listener controls and two independent precision
swaps compile/fail their named assertion; missing registry membership fails the
real native-host test and restores to pass. Two capability controls independently
remove callable preservation before initial signing and legacy resend; each
compiles and fails the custom-adapter witness before restoration passes.

Hosted native accounts now pass the shared account capability to HTTP, core
canSign and private WS auth. The browser keeps the API secret; the server keeps
the source-required Omni key seed for L2 contracts. Account-name absence remains
undefined without falling back to local HMAC. The WS bridge preserves the typed
request envelope and awaits its response before login/subscription. Relay refusal
and transport errors reconnect without private frames or an auth-invalid message.
The open callback runs eagerly to its first suspension, then the host owns its
continuation so disposal/drop cancels pending signing. This extends the declared
P4 native cancellation choice to a source Promise that would keep running.

`native_host_apexomni` covers local and hosted HTTP/WS, remote-secret wire HMAC,
request account/metadata, delayed login, relay failure and pending-auth disposal.
`scripts/hosted-signing-controls.py` independently removes capability edges and
callback ownership; each must compile and fail its named native test, then restore.


Placement follows lodash `times` for out-of-range lot counts, including an empty
submission when the market maximum is zero. It still schedules order refresh.
Trigger-price fallback preserves an adjusted zero in the final `price` operand.
All split payloads are submitted before response IDs are read and projected;
a numeric ID cannot interrupt the remaining submissions or reconciliation.
The approved placement-result policy applies JS String to every provided ID
after all submissions; missing IDs remain visible errors. Reconciliation is
scheduled before ID conversion, so a result error cannot hide submitted orders.
`tests/support/placement_parity.rs` drives all three cases through the real
constructed binding and L2 signer, observing the signed HTTP requests.

## Accepted-set map records

`public_records`, `private_records` and `config_records` use 26 private Serde
remote schemas at their existing concrete `Deserialize` boundaries. Preserve
all original field/rename/default/presence/custom-passthrough attributes, numeric
ingress, Serialize and exhaustive JsNumberOutput registrations. The previous
ordinary local derived Fields decoders have independent obligations; their
parent guards do not exempt them. A remote companion must remain private and
must not acquire independent trait/conversion implementations or extra APIs.

`tests/accepted_map_records.rs` independently observes every native field,
Option presence, nested collection and f64 bit pattern before owner serialization.
The authored fixture bases and explicit deltas preserve raw numeric tokens,
including -0 and overflow, and contain one complete valid literal positional
array per owner in reviewed Serde declaration order. Expected values come from
the TS declarations, never from owner Serialize. All seven ordinary, qualified
and shared numeric paths run for each case. Existing live-source record tests
remain required; these selected contracts do not replace them.

Run the selected test and `scripts/accepted-map-record-controls.py` serially from
sdk-rs with its pinned toolchain and one Cargo job. Each control removes only one
owner's object guard, requires the named compiled complete-array admission
failure, restores all source bytes exactly and reruns all 26 native tests. `scripts/accepted-map-record-controls-test.py` checks exact-span
isolation for similarly named owners and wrapped calls without compiling or
mutating production.
`scripts/accepted-map-record-contracts.mjs` performs fresh native/default compiler
inventory and actual runtime probes for the selected 26 owners; it separately
inspects every actual nonblanket companion implementation and the exact returned
owner identity. New traits/conversions/inherent APIs must refuse. Other records,
functions, aliases, receiver policies, feature and wasm behavior remain separate
accepted-set obligations. Follow ../safe-cex-parity/accepted-set/MAP-ONLY-INGRESS.md and
MANIFEST-SPEC#numeric-ingress-defect rather than adding a new primitive.


The transaction/order/signer map cohort contains all19 transaction structs,
CreateOrderOptionsObject, OrderResponseObject and TxEthSignature. Their private
remote schemas preserve numeric0|1 flag validation, the exact two-number fee
rate tuple, optional/nullable distinctions, native enum/union/timestamp variants
and all existing numeric ingress/output behavior. Signing/packing/range checks
remain separate APIs. WithPrice and OrderWithClientId still have independent
undefined-only admission obligations; invalid null array slots are not evidence
of a complete valid positional array.

`tests/accepted_transaction_records.rs` uses independent raw-token fixture bases
and deltas, actual native fields/variants/presence/bits and fixed per-owner case
and route counts. Its complete array literals follow each original Serde field
order. Run `scripts/accepted-transaction-record-controls.py` serially; it reuses
the reviewed exact-span `guard_removed` selector, requires one isolated compiled
named array-admission failure per owner and restores all source bytes with all22
native tests passing after each control. The fresh production runner is
`scripts/accepted-transaction-record-contracts.mjs`. ChangePubKeyOnchain has one
actual field; its maintained ordinary observation directly matches that native
variant before the wire audit. Do not invent a second native field or exempt it.


## Accepted-set scalar and validation helpers

The selected23 helper bodies in signer_utils, exchange_utils and utils::drop_prefix
are reviewed individually. Prepared-value admission/projection, numeric string
parsing and fee coercion use actual native decoder output contracts. Typed Boolean
assertion, ignored subaccount arguments, literal byte indexing, injected random-ID
generation/formatting, typed TP/SL arithmetic and literal prefix stripping each
have their own non-decoder reason; those classification receipts do not execute
decoder coverage. All23 still require direct named native behavior witnesses.

The cfg(test) modules use tests/support/accepted_scalar_helper_fixtures.json and
accepted_scalar_helper_support.rs. Expected outcomes come from reviewed source
contracts and literal known answers, not production serialization or proposal
reports. Fixed per-owner case counts and actual execution counters cover634 cases;
callbacks expose argument bits, consumption count and ordering/error behavior.
Prefix stripping additionally checks the exact borrowed pointer offset.

Run scripts/accepted-scalar-helper-contracts.mjs for fresh native/default compiler
discovery and16 actual decoder outputs plus7 explicit classifications. Run
scripts/accepted-scalar-helper-controls.py serially:23 isolated reviewed full-body
mutants must each compile/fail their own named native test; the16 decoder mutants
also require the selected production function's output mismatch. Exact restoration
passes all23 native tests each time; restored decoder runs repeat all16 outputs
and7 classifications. Literal whole-function uniqueness is checked before any
mutation; new source drift must refuse. Other functions, aliases, receivers,
construction, feature and wasm accepted behavior remain separate obligations.

Private position frames preserve the source mapper's socket receiver: unlike the
exchange receiver, the socket has no `log` method. An unknown market therefore
raises TypeError after the mapper's earlier reads, before any position updates,
refresh timestamp/timer or later same-frame pong. The shared mapping body accepts
its receiver's logging capability so REST mappings retain exchange warning logs.
The production binding tests cover mixed valid/unknown rows, unchanged Store and
skipped pong, plus a known-market socket-frame control that updates Store.


The binding dispatches private snapshot refresh through the Apex lifecycle override,
so reopen commits positions and orders before balance, retaining the WebSocket
grace merge. Private keepalive schedules into the core Ping slot shared with pong,
close and disposal; there is no separate shadow timeout. Real binding regressions
stagger pong deadlines and hold replacement dialing after close. Private order,
fill, position and wallet frames cover Store, fill notification and execution
history behavior; wallet-only frames leave balance owned by REST, as in source.
The socket oracle supplies no invented `log` method: unknown-market position rows
compare the source TypeError and unchanged state.

Missing order type/status/side table entries retain own undefined via ModelField.
They cannot reject an entire REST snapshot or stop later rows in a private frame.
Unknown private statuses still follow the source’s ignore branch after mapping;
unknown type/side with an OPEN status reach Store. Inherited callable/object
lookup results retain their separately documented closed-field refusal.
The unmapped_order_enums target exercises both production entry paths.

Native HTTP auth, socket URLs/login and fallback pong, mapped model timestamps,
private snapshot grace and order expiration read the shared synchronized clock.
Raw Date.now remains separate for cancellation caches, fill freshness, client IDs
and zkLink transaction timestamps; ping timing remains monotonic. The isolated
native_clock target syncs the actual singleton and covers signed REST/placement,
socket authentication and a terminal private order frame after a clock advance.


## Retired oracle-only transaction helpers

The exemption reachability audit keeps the live `signContract`/`initSdk` route.
The SDK index has no export for standalone order, transfer, withdrawal or
changePubKey helpers, and repository production TS/Rust callers do not use them.
Their Rust executable counterparts and withdrawal-only precision helper were
removed, together with eight unused SDK host operations and four selector variants.
The TypeScript source modules are test/oracle-only and remain tealstreet-next owned.
Payload declarations retain their independent schema/accepted-set obligations.

The maintained signing oracle keeps all six Contract cases, seven exchange
signOrder wrapper cases and initialization; the retained outputs match the
pre-deletion corpus. Twenty-four fake transaction orchestration cases and one
hundred withdrawal-only precision cases were retired. Existing native placement,
private frames, hosted signing, lifecycle, packing and signature proofs remain.

Truthy non-boolean Order.reduceOnly is a maintainer-approved per-row refusal.
The mapper warns with its field and original value, returning no orders for that
row so valid surrounding REST/private rows continue; it never coerces the flag.


Market batches retain the source config update, filter and complete numeric map
before strict output admission. A refused id/symbol/base/quote/active/info row
warns with its index, field and original value; valid siblings remain available
to fetch and Store refresh. Source evaluation and reporting failures propagate.


`binding::production_ticker_coercion_matches_constructed_typescript` adds
64 field-independent cases through the constructed production socket/Store:
three-row instrumentInfo frames, truthy guards and mark/index fallback. Each case resets both native and
maintained TS Stores to finite baselines, then checks all eleven native numeric
fields after malformed and good rows. Null, numeric/prefix/NaN strings, objects,
overflow, signed zero and false follow the actual TS exchange and DefaultStore.
The shared ticker census records the witness. The compiling
`parity/scripts/ticker-native-controls.py apexomni` fault must fail that test and
pass after exact source restoration.
