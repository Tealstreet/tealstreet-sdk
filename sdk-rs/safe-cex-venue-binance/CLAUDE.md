# Binance family port

Behavioral reference: [TypeScript venue guidance](../../packages/safe-cex/src/exchanges/binance/CLAUDE.md).
Core contract: [validated placement and lifecycle](../safe-cex-exchange/CLAUDE.md).
Futures, Portfolio Margin and Spot remain modules inside this crate.

`scripts/public-dispatch-contracts.mjs` uses the maintained compiler driver with
four exact owner filters for `public_dispatch::{Handlers,dispatch_binance_message}`
and `ticker_capture::{Capture,lookup}`. The three raw-key/frame operations have
direct native observations; `Capture::new` is a typed Store capability factory.
Its classification does not claim decoder execution. Keep retained metadata,
callback identity/order and inherited lookup observations before JSON projection.

The constructed NativeImports socket witness compares twelve prototype/capture
cases against the maintained TypeScript exchange classes. Capture assertions
must inspect retained cells before rereading the dictionary, so a lazy refresh
cannot conceal a broken Store subscription. `scripts/public-dispatch-controls.py`
requires four independently compiled production faults to fail their named
constructed socket tests, then restores each source file byte for byte.

## Source-owned tables

`scripts/types.mjs` evaluates all three live TypeScript type modules, resolving
shared enums from the real shared source, and generates `src/generated/*.rs`.
It also generates wire structs from the source AST, refusing unknown declaration
or field shapes. Do not hand-edit generated endpoints, hosts, mappings or types.
Regenerate with `node sdk-rs/safe-cex-venue-binance/scripts/types.mjs --generate`.

`yarn venue-oracle:binance` checks generator freshness and runs the compiled-table
comparison against live TypeScript, plus all crate tests including ignored Node
oracles. The mandatory workflow step uses the existing isolated TypeScript
installation. The committed workflow audit rejects removal, masking and conditions.
Default cargo tests remain hermetic.

## Integration boundaries

Binance uses shared `PlaceOrderOpts`; concrete venues declare it explicitly as
`Exchange::PlaceOpts`. Placement receives private venue-typed validated tokens.
The Portfolio Margin constructor owns separate PAPI, FAPI, unlimited and public
clients. Only PAPI gets the secondary-key wrapper; FAPI stays primary-only while
unused, matching the source constructor's comment. Both PAPI keys are validated.

Signed numeric payloads must use the shared JavaScript number formatter, and
query serialization must preserve insertion order and sign exactly the bytes
sent. A retrying client must use the source read classification; no write retries.

The type/endpoint and shared signing slices are implemented. API, exchange and websocket
manifest rows remain pending while those phases are implemented.

## Signing boundary

`signing::sign_binance_request` takes the structured Axios config before body
transformation, consumes the shared payload signer and service VirtualClock, and
returns authoritative params with data omitted. API composition must use that
returned config without restoring the old body. A retry calls this function
again, so it removes the previous signature and samples a fresh timestamp.
`serialize_query` is the same repeat-array codec used both for signing and the
core's post-signing transmission callback; numeric spelling uses js_number_string.

The live oracle executes the actual TS signer and payload helper with installed
Axios, qs and create-hmac. The normal corpus covers structured payloads, callback
headers/errors, bypass guards, integer key ordering and numeric thresholds.
Two chosen representation divergences are explicit in the signing manifest: UTF-16 string
spread can create lone units that Rust String replaces, and non-finite injected
clock/window numbers become their textual query spelling in the JSON config.
The former's actual qs bytes are pinned independently; do not silently describe
all arbitrary Axios data as equivalent. Normal venue requests use structured data.

Wire decoding uses string-only union and map-only record front gates. Records use
private Serde remote companions; they do not create another Deserialize owner.
The live union oracle compares the
accepted literal inventory (including Serde aliases) to the TypeScript AST;
qualified public Deserialize calls reject non-string and positional-array inputs.

The history numeric parsers use the shared fallible String(any) and parseFloat
primitives; callers must propagate their Result rather than panic or substitute.
Historical table reads use shared TableHit and preserve nullish fallback and
coercion order. BinanceHistoricalOrderError distinguishes source coercion errors
from chosen whole-record refusal at the closed enum output boundary; inherited
__proto__ output remains a separate FINDING, with live serialization witnesses.
User-trade query construction retains numeric values in a typed return record,
including NaN and infinities, and drops fromId whenever either time bound exists.
Its live oracle varies missing cursors as well as zero/negative/fractional IDs.

Orderbook snapshot loading preserves source order, duplicate levels, zero totals,
missing string-array cells (parseFloat(undefined) is NaN), and snapshot ID bits.
Reset clears both IDs independently. Buffer/apply use shared Number(any),
preserve sequence/coercion order and partial mutation on malformed updates;
no local coercion or trailing sort has been introduced.

Pure futures/PM hedge-side helpers are extracted with isHedged as an explicit
input, pending class integration. Algo classification includes only StopLoss,
TakeProfit and TrailingStopLoss, never Conditional. Already-set position mode
recognizes only nested response.data.code equal to numeric or string -4059.
The deprecated futures balance-issue detector intentionally remains false.

Spot user-data signing payloads preserve the source's raw API-key interpolation
and JavaScript numeric spelling. ApiType and BinanceOrderBookApplyResult use
the same string-only decoder discipline as the generated income union.

Spot USD valuation checks USDT, USDC and USD lazily; negative/infinite quotes
remain usable while zero/NaN fall through. Reconciliation helpers preserve native
f64 state, Math.max signed-zero/NaN behavior and the strict pause-time boundary.

Closed-position derivation consumes JSON records and uses shared Number for time,
parseFloat for monetary values. It preserves gross realized PnL and separate
fees, and returns an error for the whole call on failed coercion.

Trailing-stop formatters use shared adjust and math_round (ties toward positive
infinity). Preserve each family's ID field/prefix and priceProtect casing;
only mint the injected UUID after the source's missing-state checks pass.

Signing oracle packages resolve relative to SAFE_CEX_TYPESCRIPT_PATH in CI;
ci-deps.cjs selects their real source-package versions under the copied lockfile.
The mandatory signer control test rejects source mutations, no-op output and
missing runtime imports. Never fall back to ambient packages when an isolated
compiler path is supplied.

Response-shape helpers use shared js_own_keys before the six-key cutoff. Array
validators borrow the original rows and preserve TypeError from message coercion.
The live corpus carries ordered pairs so JSON transport cannot hide key-order bugs.

The response-shape accepted-set review separates three executed outputs (both
borrowed-array validators and the qualified native error conversion) from four
diagnostic non-decoder roles (three descriptors and the typed error formatter).
Classification receipts do not execute those diagnostic functions. Each has its
own direct named native test, including both private helpers. Preserve nonempty
slice identity, native element fields/bits, error name/message and Display before
serialization. `scripts/response-shape-controls.py` requires seven independently
compiled failures and exact-restoration passes; fresh production observations and
classification receipts are checked by `scripts/response-shape-contracts.mjs`.
All five Binance accepted-set runners read reviewed policy shards through the
shared `loadPolicy` API; never restore deleted flat maps. See the
[policy storage guide](../safe-cex-parity/accepted-set/POLICY-FILES.md).
This selected native/default scope does not cover other owners or the feature
matrix. Keep the existing [string](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary)
and [typed-output](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary) boundaries.

Spot order formatting consumes BaseExchange's client-ID resolution and final
native-count assertion directly. Its market snapshot is cloned before errors
can emit through the store. Preserve attached-order field inheritance, post-only
LIMIT_MAKER, zero-max split guard and per-payload UUID timing.

Public-data startup bodies consume BaseExchange and the production store. Preserve
the disposed guard and the log before reading loaded flags and updating private
state; synchronous log listeners can change the state before that snapshot.

Public websocket subscription helpers snapshot shared JS own-key order and read
each count immediately before sending. Native-count callbacks preserve NaN and
infinities; nonnumeric counts are None. Bind the fallible send callback to the
core socket when composing classes. Send errors abort the remaining keys.
All five concrete PublicState kinds use subscribe_binance_public and its private
encoder. Keep the live-source cases on that actual shared route; do not recreate
unused family-named forwarding wrappers. The native subscription receipt test
opens every kind with native_dependencies and checks outgoing bytes, reentry and
send failure. See PUBLIC_SUBSCRIPTION_REVIEW.md for individual classifications
and compiled body controls.

Public onOpen bodies use BaseWebSocket send/reset rather than local socket or
orderbook replicas. Market variants keep their books; public/Spot variants reset
before the explicit subscribe callback. The disposal guard runs only at entry: a
reentrant disposal during send does not skip the source's remaining reset/subscribe.

The futures and PM private onOpen extractions call the production ping operation,
then read caller-owned resync/opened cells. They clear resync before invoking the
fallible throttled-fetch callback and mark opened only after success. Reentrant
disposal during send does not add a second source-absent guard. Concrete class
composition and throttled timer ownership remain pending.

Spot private onOpen is an extracted async operation over the core socket, a
caller-owned Number attempt counter and a subscription-parameter future. It
checks disposal/staleness after successful signing but still logs and closes on
rejected stale/disposed signing, as the source does. Concrete WsProtocol wiring
remains pending; that integration must retain core-owned task lifetime.

Signing also has a declared owned-snapshot boundary for nested params. Source
shallow copies retain aliases in both directions after return; Rust returns deep
copies. See the signing manifest's canonical alias citation and live witness;
this is independent of its string/non-finite configuration choices.

Futures and PM position-mode readers await the supplied configured GET operation
once and consume decoded response data. They use different endpoint paths and
strict boolean true; undefined/null bodies return TypeError. This extracted
callback surface does not claim the still-pending concrete class/client wiring.

The public factory remains held on response phase/type provenance. Its live oracle
executes the production HOW adapter over a recording socket, then the actual
Binance response transform. JSON text `"123"` returns a string directly but a
number through HOW; byte-only RestResponse cannot distinguish those phases.
Do not infer response origin from bytes or add a venue-local response wrapper.

The futures worker-result hook reconciles only changePositionMode, using shared
JS truthiness on the first argument and retaining only a string second argument
(including empty string). The confirmation callback remains concrete class
composition; its errors propagate without coercion or swallowing.

PM changePositionMode uses production Store and a configured POST. The typed
StoreMutationError enters the source catch after the setting has committed;
notification stops at the first failure. Only this catch projects the error to
its truthy response.data.msg or message for emission. A throwing emitter escapes.
The eight production-Store source cases now compare against the Rust operation.
The POST still settles before mutation, and unchanged settings notify nobody.

PM margin-mode rejection is an extracted async operation with a fallible error
emitter. It emits exactly once, then returns the unsupported-margin Error; an
emitter failure takes precedence and retains its metadata. Unused isolated and
symbol inputs are omitted; concrete class/emitter composition remains pending.

The three `leverage` function contracts pin typed command APIs, including the
shared setter reached by both live exchange classes. Their classification receipts
do not execute the commands or admit raw records. The separate `leverage` test
target compares 138 maintained source cases and four inherited-market guards,
observes native finite/signed-zero bits and nonfinite classes before projection,
and checks both families cannot write Store state while POST remains pending.
Use canonical accepted-set `--measure --module=leverage` for the scoped receipts.

Generated wire records capture shared JsNumberInput at their public Deserialize
entry, then call the private remote companion through strict_decode::object. Keep
all field/default/presence attributes on that schema and keep the explicit
exhaustive JsNumberOutput impls emitted by the generator, including the income
union's registration. This repairs raw JSON overflow refusal without trading away
signed zero on already-parsed Value input. Never replace it with Value-first
rebuffering or remove the map/string front gates. Numeric ingress tests compare
bits before serialization over raw str/slice, owned/borrowed Value and the three
shared js_from_* entrypoints. The separate vanilla Value-rebuffer witness still
records an unsafe bypass outside these opted-in decoders.

`tests/accepted_record_shapes.rs` authors each complete valid positional array in
declaration order and reads every decoded native field across those seven routes.
Run `python3 sdk-rs/safe-cex-venue-binance/scripts/record-shape-controls.py` from
the repository root to remove each owner's object guard, require its named array
assertion to fail after compilation, restore the source and require it to pass.
Do not derive the negative array from record serialization.
`node sdk-rs/safe-cex-venue-binance/scripts/record-contracts.mjs` runs fresh
ordinary inventory and the four records' production contract probes. Run accepted
probes serially; this focused command excludes other owners and the feature/wasm
matrix.

The income, API-type and orderbook-apply string unions use private Serde remote
schemas after their existing String admission. Preserve both actual native
variant identity and the complete Serde rename/alias inventory. The 24 valid
tags must each reject a tagged-null object through owned, borrowed, qualified
and text decoding. `tests/accepted_string_shapes.rs` supplies those witnesses;
`python3 sdk-rs/safe-cex-venue-binance/scripts/string-shape-controls.py` removes
only one String frontdoor at a time, requires the named valid-tagged-object
assertion to fail after compilation, restores exact bytes and requires a pass.
`node sdk-rs/safe-cex-venue-binance/scripts/string-contracts.mjs` performs fresh
ordinary inventory and production native probes for those three enums only.
It excludes other owners, aliases, local scopes and feature/wasm coverage.

`order_logic`'s already-set position-mode error recognizer, deprecated balance
predicate and post-only rejection classifier each have a specific reviewed
non-decoder reason. Their actual Boolean/error results are witnessed directly in
`tests/accepted_predicate_roles.rs`, supplemented by the maintained live
`order_logic` and `rejection` targets. Run
`python3 sdk-rs/safe-cex-venue-binance/scripts/predicate-role-controls.py` to
replace one result at a time with an incorrect Boolean, require its named native
test to fail after compilation, then restore exact source and pass. The separate
`node sdk-rs/safe-cex-venue-binance/scripts/predicate-role-contracts.mjs` runs fresh
ordinary compiler discovery and those three production classification receipts.
Those receipts classify the functions; they do not execute their predicate logic
or discharge another function, owner, alias, local or feature configuration.

The Futures/PM response-message functions and their private shared helper have
observed native String/error-name contracts; the websocket rejection mapper has
actual Option<OrderRejectReason> variant/absence witnesses before serialization.
These functions preserve their fallback behavior: non-object response payloads
produce empty text, and the typed Map API for rejection reasons is exercised with
actual maps. Do not replace those output contracts with string-only/map-only
admission checks. `tests/accepted_error_outputs.rs` supplies public native
witnesses. `scripts/error-output-contracts.mjs` performs fresh ordinary compiler
discovery and four actual production output probes, including a direct call to
the private helper. `scripts/error-output-controls.py` changes one result at a
time, requires the changed owner's compiled native output mismatch, restores
exact bytes and requires all four outputs to pass. A private-helper mutation
also affects its two public callers; the private owner's own result must fail.
Compiler errors do not count. Run these probes serially and keep other owners,
aliases, locals and feature/wasm claims outside this focused evidence.

The public-factory response witness now pins the repaired shared provenance
boundary: direct quoted HTTP text parses once; HOW custom-parser strings pass
through the venue parser again (source ts-bug). The original equal-response
finding flipped by name. Twenty-two actual factory captures compare typeof and
recursive numeric bits, including object/array/null, signed-zero, overflow and
negative-underflow controls. This is shared
boundary evidence, not an implemented Rust public factory or completed row.

Public connectAndSubscribe extractions use BaseWebSocket.start_websocket with
omitted options, retaining core-owned dialing, event dispatch and cancellation.
The entry disposal guard precedes even the options read. Futures/PM public sockets
start their own socket before invoking the companion-market connect callback;
its error propagates. URL table keys are closed source choices (channel and
testnet/livenet), not venue-controlled keys. Concrete protocol/class composition
remains pending; these extractions do not introduce a second task registry.

Futures/PM public socket disposal invokes the companion-market disposer first,
then BaseWebSocket.dispose. A child error skips all parent teardown and propagates
unchanged. There is no source entry guard: already-disposed parents still call
the child. Keep cancellation in the core; concrete class composition is pending.

The three _startPublicDataOnly operations propagate log failures before updating
state. Store.update failures propagate after commit, retaining StoreMutationError
inside a boxed standard Error, including notification subscriber id and opaque
cause identity. ExchangeError now retains inner-to-outer Store-local subscriber
IDs, but its conversion still loses the Store variant and arbitrary callback
cause. Keep the box for those two observables; an ID list alone is insufficient.
Remaining subscribers are skipped. The native test crosses all three wrappers
with both recovered ExchangeError metadata and an opaque callback cause.

The notification audit follows outgoing production-core edges, not only direct
Store writes. Three private ping extractions call sendWithLog -> send -> an
unawaited reconnect when no socket is open -> processClose -> real parent health
publication -> Store.updateSocketHealth. A subscriber throw commits health, skips
subscriber2 and reconnect continuation, and rejects the discarded reconnect
promise; ping itself still returns normally. These three rows retain FINDINGs pending whole-path native remeasurement; the
shared Store and health-publication error channels and subscriber IDs now exist. The
12-case send-store oracle uses actual venue/core methods and production Store;
a pre-existing onCloseCount=100 bounds the positive control at the source guard.
Caller-supplied callbacks terminate the extraction audit by declaration. Other
open/connect/dispose outgoing edges remain under audit; this is not a fleet-wide
or whole-venue clean claim.

Futures and PM private onOpen also cross that outgoing edge through their actual
ping calls. The background reconnect rejection does not reject onOpen: the venue
continues to its supplied order-fetch callback, clears resync and sets hasOpened.
The expanded20-case notification oracle pins that separate continuation; these
two rows are additional FINDINGs, not a claim that onOpen itself throws.

All five public onOpen extractions likewise reach the notification through their
initial BaseWebSocket.send. Forty added cases pin that a failed background
reconnect does not stop local reset/subscription, and pin the Futures/PM public
publicDataOnly branch that skips the initial bookTicker send. That suppression
avoids this edge only in that branch; it is not whole-row immunity. The supplied
subscribe callback remains the declared boundary. These five rows are FINDINGs;
source notification corpus is now60 cases including the earlier private paths.

Spot private async onOpen crosses the same send edge after its supplied signer
resolves. Its own promise resolves while the discarded reconnect promise rejects;
the signing-error catch does not handle that background failure. Four added
cases and an await-reconnect control pin the two-promise distinction, bringing
the notification corpus to64. That row retains a whole-path FINDING; the shared error channels now exist.

The disposal notification probe executes actual vendored ReconnectingWebSocket
construction and teardown, actual BaseWebSocket.dispose and both public venue
overrides. terminate removes underlying close listeners before host termination
or fallback close, so synchronous and delayed host-close delivery do not publish
health in these32 cases. This is a source-call boundary measurement, not browser
event-loop coverage. Connect registers future incoming handlers; registration
alone is not an outgoing notification call. Supplied child/subscribe callbacks
remain outside the extracted operation's scope.

Store error-conversion audits start from mutation callees, including aliases and
UFCS. public_start binds Result<(), StoreMutationError> and boxes the original;
pm_position_mode binds that same type and projects only at the source catch.
Spot private open propagates emit_and_log_error's ExchangeError before force_close,
so a throwing emitter skips closure. Its indirect background reconnect finding
remains unclaimed by this direct-call repair.

Numeric capture also imposes the shared128-level recursion budget on unknown
fields before the private Wire can ignore them. All four generated records refuse
a140-deep unknown array accepted by Node, across raw/owned/borrowed and shared
ingress routes. This is a separate CHOSEN acceptance cost, not a reopening of
the repaired overflow defect or a loss of the signed-zero regression guard.
Keep shallow unknown-field acceptance and numeric bits pinned independently.

The five public recordHandlerStats bodies share public_stats' extraction, which
calls the actual BaseWebSocket stats operation without a disposal guard. Keep
core's caller-visible reconciliation Result, undefined versus zero inherited
arity and own-key shadows. Source stats writes mutate inherited native function
referents or Object.prototype; core deliberately omits that global mutation, so
these rows remain CHOSEN FINDINGs, including later alias effects. The1280-case
source oracle runs each wrapper in fresh realms and observes typeof and writes
before serialization; ordinary own stats and immediate error identity match.

Closed-position history reads use the shared calendar splitter, sequentially await
configured GET pages, then map all accumulated rows and stably sort descending.
A malformed early row must not prevent a later GET, whose error takes precedence.
The supplied fallible emitter receives a truthy response.data.msg or the request
message; its rejection escapes rather than being swallowed with the read error.
Configured decoded GET and emitter remain explicit class-composition boundaries.

Field identity is measured after public decoding, not by whole-record roundtrip:
Serialize can undo a matching Deserialize rename. The separately labeled
field-identity source supplement gives every sibling string/number a distinct
value and reads all51 public fields through seven ingress routes. Preserve the
original numeric feeder bytes; its depth consumer and earlier green reports are
not retroactively evidence of independent string fields. FIELD_IDENTITY_AUDIT.md
records original-corpus inventory, source-invariant equalities and compiled swaps.

Numeric observation captures canonicalize NaN payload/sign only when NaN is
reachable; retain all non-NaN bits, including negative zero. Capture architecture
and audited NaN-free domains are recorded beside the corpus inventory.

Union acceptance tests must compare decoded native variants independently of
Serialize. Params and depth-operation tests likewise inspect public fields and
returned variants before checking wire output; symmetric renames must not cancel.

The subprocess audit traces49 Rust launch sites across38 original test files;
none launches a child per semantic case. Source corpora are batched; mutation
controls may launch distinct whole-corpus runs. Keep that boundary when extending
an oracle. Native closed-PnL provenance and Futures regular/algo routing are
asserted before record serialization so paired serializer errors cannot hide them.

Futures changePositionMode extracts configured POST, fallible confirmation and
error emission. Keep its first confirmation inside the catch boundary: a -4059
from that callback triggers a second confirmation, whose error escapes directly.
Concrete applyConfirmedPositionMode Store writes remain a separate pending row.

Main-price and split-quantity identity use a separately selected eight-case
supplement per formatter. Keep default oracle output unchanged, and pin each
supplement input by its fixed index before native assertions. Native price,
quantity, side, payload count and distinct precision steps precede wire equality;
corpus hashes record provenance and do not replace these admission checks.
Precision controls select the whole main-price map closure: PM's attached-order
rounding also spells `adjust(price, market.precision.price)`.

Futures and PM setLeverage bodies use production Store and ModelPatch numeric
fields, retaining NaN and signed zero. Preserve the first matching position's
unchanged guard, both canonical long/short IDs and post-await mutation. Request
and Store notification failures enter the source catch; emitter failures escape.
Configured xhr versus papiXhr POST and fallible emission are explicit class
composition inputs; the native leverage request stays numeric until transmission.
An absent own market with a stock Object.prototype symbol passes the market
guard: use shared TableHit, then preserve the position guard and missing-limits
TypeError. Own Market entries shadow inherited names normally.

All three validateAccount bodies use configured decoded GET and fallible emission;
Futures/PM read secondary-key truthiness only after primary success. Futures uses
shared parseFloat/String, retaining total -0 while falsy free/upnl become +0.
PM/Spot forward generic decoded balance payloads without copying and swallow
best-effort fetch/projection errors. AccountValidation::Valid always represents
an own balance property (None is undefined); Invalid has error only. Concrete
class/client/fetch composition and arbitrary executable callback behaviors remain
outside these extractions; preserve this boundary when wiring the classes.


## Concrete class closure

The concrete Futures, PM and Spot wrappers implement the shared Exchange trait
through exchange_trait.rs. Capability brands and supported method routing follow
their actual source constructors.
Shared socket registration and imported owner observers are weak. Constructor
watchdogs bind to the stable core parent rather than a temporary wrapper.
Listener setup/cleanup, Store notifications and worker confirmation retain typed
errors and their source catch boundaries. See
[the class closure evidence](CLASS-CLOSURE-GATES.md) for bounded native/source
witnesses and [the canonical lease policy](../parity/MANIFEST-SPEC.md#owned-exchange-lease-policy)
for retained-handle lifetime choices. These classes remain divergent; completing
this closure is not a whole-venue equivalence claim.

All 16 `Exchange` trait methods are overridden on all three classes. Methods the
TS class inherits from `BaseExchange` (Futures: fetchSymbolMaxLeverage,
scheduleCancel; PM adds fetchPublicTrades; Spot adds getAccount, setLeverage,
changePositionMode, changeMarginType) go through `base_exchange_rejection`, which
reproduces the base `Error('Not implemented')` with no request and no emit. Do not
"implement" one without a TS override to port. The SDK path decodes typed values,
so its projections must match that decoder: an absent TS key is omitted rather than
written as undefined (Futures/PM `isSpot`), `class_decode` passes newtype capture
through to the native value (position `info`), and `capture_is_algo` accepts the
decoded order-type string as well as the native enum. `tests/private_classes.rs`
pins every method's outcome and transport routes per class, constructed via each
`::new` over a route-scripted `request` import.

`native_host` (feature `native`) is the production `ExchangeImports`. Each
factory call builds an axios-equivalent client over the shared `HttpClient`:
the ported request interceptors run inside `execute_with_interceptor` (so a
public endpoint's header removal stands), response stages run FIFO (factory
interceptors, then weight guards), reads retry on network failure, a -1022
signature rejection is re-signed once (a non-2xx through retry, a 2xx through
the interceptor's resend, sharing one stamp per call), and the
secondary-key split routes `get` to the secondary client. The private base is a
core `BaseWebSocket` with id 3 whose protocol forwards open/message to the
registered listeners; close stays with the core. `PrivateBase::install_hooks`
routes watchdog ping and reconnect back into `PrivateState`. `bind_owner` gives
the host the exchange weakly: response interceptors emit through it, `paginate`
runs the shared `income_paging` pager over the venue's rethrowing page fetch,
and `fold_closed_positions` runs `build_records` over the exchange's own fills
and income dispatch. A hosted remote signer arrives through `native_dependencies`:
clients get it as the HTTP signer (the venue interceptor leaves requests unsigned
when `options.remoteSigner` is set) and `remote_sign` relays ws-auth to it; the
options token is stripped before typed decoding, since typed options cannot hold it.

`ExchangeImports::request` args are the query params for GET/DELETE and the body
for POST/PUT — never an axios config object; every call site passes that shape.
Binance ledger ids arrive as JSON numbers and the class keeps them raw as TS
does; `income_records` projects them through `String()` only at the typed
`IncomeRecord` boundary (trait income methods, pager, fold).

The family value graph is the canonical `js_runtime::value::Js`: `EventValue`
is an alias and `EventOps` retains the decoded-data operations without separate
storage. Closed categorical constructors now create source string primitives.
Fresh decoded objects capture the existing native `PrototypeRealm` through
`Js::Builtin`, retaining live intrinsic referents used by core statistics.
These native referents support the existing data/reference profile, not arbitrary
intrinsic execution or full descriptors. Typed DTO conversion uses the shared
native bridge. See [shared value evidence](SHARED_VALUES.md); the separately
planned typed JS-string cutover remains outside this change.

The native host's `uuid()` must use `safe_cex_crypto::uuid::uuid`, the maintained
source `nanoid().replace(/-|_/g, '')` implementation. RFC UUID-v4 is36 characters
before the10-character broker prefix and exceeds every family's ID limit.
Stripping the default21-character nanoid yields at most21 alphanumerics; do not
pad it to a fixed length. The production dependency/owner witnesses in
`tests/private_classes.rs` cover actual host entropy, minting, regular/split and
attached/standalone/trailing payloads, and preserve caller IDs. Run them with
`--features native`; a default-feature filtered run cannot execute this host.
The same native host fixtures deliver private orders/fills/positions/balance
frames through the assembled private binding and assert real Store and emitted
fill contents, including the source's ignored derivative balance rows and Spot's
absence of a derivative-position topic.


The unused `private_order_logic` JSON classifier and the opt-in typed historical
mapper are removed. The production socket and its event tests share
`private_events::get_order_reject_reason`; history orchestration tests consume
the same native mapper as the actual exchange. Keep the canonical history source
corpus in `history_native` and the production private-topic regressions in
`private_classes --features native`; do not restore a parallel typed mapper to
make a test host easier to implement.

Signature-predicate coercion failures retain their TypeError name and unprefixed
message through the shared ExchangeError conversion. The real native receiver
regressions cover both resolved and rejected HTTP responses without a resend.

Native HTTP dispatch retains the original params/data until the real request
interceptors run. Query BigInt leaves take qs decimal spelling; signed bodies
are moved into query fields by the source signer before JSON transformation.
Public/unsigned BigInt bodies reject with TypeError before terminal I/O, retaining
the original refusal when signing is skipped. Never turn a conversion error into
an absent params/body field. `private_classes::native_projection_*` uses actual
native imports, factories, signing and terminal HTTP; four regressions compile
and fail on master. Signed GET/POST wire bytes and HMACs compare against actual
TypeScript with real qs/create-hmac in one two-case capture. Maintained typed
exchange callers still supply ordinary string/number payloads; these BigInt cases
cover the reachable native-host boundary, not a new typed DTO admission claim.

Portfolio Margin account updates keep the source body's `info: position` reference
until the concrete native Store adapter. That adapter applies the approved
representation choice: a depth-one snapshot of the updated position's own fields,
excluding `info`. Include the current price/contracts/PnL/side patch before
capturing it; a previous-state snapshot has stale readable fields. Identity and
recursive `info` reads remain an accepted divergence from TS's cycle. Never nest
the previous owned info on every frame. The native `private_classes` growth and
current-field regressions use the real socket and Store;
`SAFE_CEX_TYPESCRIPT_PATH=<typescript> node scripts/pm-position-info.mjs` observes
the maintained TS handler with the actual TS Store across sixteen changing frames.


Concrete market reads apply the approved strict-asset row policy after complete
source REST projection. Non-string base/quote/symbol or nonnumeric leverage-max rows emit their
field and original value through the existing error path; valid siblings remain available to typed
fetch and Store refresh. Raw REST helpers retain their source-owned values.

Native depth application converts coercion failures through ExchangeError::from,
retaining the source error name and cause. The production socket regression covers
all three families, subsequent valid deltas and sequence-gap snapshot recovery.

The constructed ticker census in `private_classes --features native` uses actual
NativeImports and ExchangeOwner/PublicSocket dispatch for all three families.
Futures and PM each cover80 field-independent packed frames through market and
book sockets; Spot covers48. All native numeric fields compare by bits against
the constructed maintained TS exchanges and DefaultStores after finite resets,
with distinct good rows surrounding malformed input. The shared control runner
misroutes the live last-price publication and requires all three family tests
to compile and fail, then all three pass after exact restoration. Default-feature
filtered runs cannot execute these native-host witnesses.

COIN-M inverse sockets live in `src/inverse/ws_public.rs` and `ws_private.rs`.
Their constructor receives the actual parent Scope; field arrows retain their
lexical socket while lifecycle and task ownership remain in the shared core.
`tests/inverse_socket_binding.rs` exercises that constructor, real Store and HTTP
clients with primitive runtime ports. Its mandatory source test runs maintained
whole socket classes and imported algorithms, including inherited book reset.
The eleven source groups cover topics/tickers, candles, depth sequencing, native
fill contracts/large IDs, inverse positions, capped listen-key retry, keepalive
recovery, disposal, stale REST suppression, escaped-surrogate frame handling and native warning units. This is a selected corpus, not exhaustive equivalence.

Run `python3 sdk-rs/safe-cex-venue-binance/scripts/inverse-socket-controls.py` from
the repository root to compile five isolated socket behavior faults and require
their named tests to fail, then pass after exact restoration. It runs Cargo from
`sdk-rs` with one job. The public book bridge reuses canonical family depth
algorithms and retains the family's typed, owned callback snapshot boundary;
it does not claim mutable JavaScript graph aliases across that boundary.

The inverse public dispatcher retains the documented family UTF8 parser profile:
TS accepts an escaped lone surrogate in an otherwise usable frame while native
Serde refuses that frame. The constructed refusal/plain-frame control and actual
TS acceptance are explicit witnesses; public class/onMessage rows remain divergent.
Connection/keepalive warning concatenation must retain native UTF16 before the
existing shared reporting/log boundaries; tests read the actual Base console Js
argument before any diagnostic display projection.

The eleven inverse socket raw-string helper contracts use the separate
`scripts/inverse-socket-helper-contracts.mjs` runner and 79 native cases.
Their scope, primitive capability seams and actual-class source coverage are
reviewed in [INVERSE_SOCKET_HELPER_CONTRACT_REVIEW.md](INVERSE_SOCKET_HELPER_CONTRACT_REVIEW.md).
The runner keeps rustdoc artifacts in `target/accepted-inventory`, separate from
normal native builds. The companion Python control runner compiles isolated
production faults and restores exact bytes; source fixtures are never substitutes
for those native receipts.

Native HTTP requests use the existing eager class settlement. Dropping a snapshot
waiter after Promise.all rejection leaves its started sibling owned by the runtime,
including response interceptors. Registry native_account_proxy_errors holds each
orders/positions sibling response until after the first rejection to prove this.

Ticker-capture aliases name retained state handles and implement Deserialize
in the production dependency graph. Their decoder contracts check map admission
and every decoded Ticker field/bit/presence through the actual aliases. The
separate constructed native witness establishes runtime reference behavior.
`private_classes::production_native_ticker_capture_aliases_retain_cells_and_replace_generations`
(`--features native`) constructs all three real native venues and dispatches
book-ticker frames into their Store. It checks dictionary/cell identity, signed
zero and replacement generations before serialization. The separate
`scripts/ticker-alias-controls.py` corrupts dictionary reuse and retained-cell
updates independently, requires the named native assertion to fail, restores exact
source bytes and requires a pass after each fault. The selected alias inventory
runner is `scripts/public-alias-contracts.mjs`; it does not waive other owners.

The account leverage aliases are native computation capabilities. The constructed
Futures/PM overlay witness checks signed bracket requests, Store publication and
unchanged market fields. `scripts/account-leverage-alias-contracts.mjs` measures
their actual native traits with the maintained authenticated compiler context.

The two `margin_mode` free functions are typed asynchronous commands, with exact
compiler API classifications rather than decoder-admission claims. Their separate
production-function witnesses in `margin_mode` and `margin_dispose` compare 64
maintained source cases and poll both futures margin modes across a pending POST.
Run the canonical accepted-set `--measure --module=margin_mode` for classification
receipts; those receipts do not execute the commands or cover arbitrary JS inputs.

### Position snapshot callback alias receipts

`position_snapshots::{Emit,Log}` are executable capabilities captured from the
actual ExchangeState. `node scripts/snapshot-alias-contracts.mjs` (cwd this
crate) uses fresh authenticated compiler discovery and runs the two native alias
trait/type probes. These are classification receipts, not callback execution.
The concrete execution witnesses are in `tests/support/snapshot_aliases.rs`,
included by the native `private_classes` target. They construct the production
native dependencies/ExchangeOwner, drive signed Futures/PM positions failures
and Futures account balance aggregation through native HTTP, and observe Base
error/log callbacks, one-time logging, native balance bits and listener errors.
Run them with `cargo test --locked --jobs 1 -p safe-cex-venue-binance --features
native --test private_classes snapshot_aliases` from `sdk-rs`.
`python3 safe-cex-venue-binance/scripts/snapshot-alias-controls.py` independently
faults Emit forwarding and Log severity in the production capture bodies,
requires the corresponding compiled native assertion to fail, then restores the
exact file and requires GREEN. This cohort makes no claim for other aliases or
Wasm callback execution.

`public_rest::RestFuture` is the pinned request continuation returned by the live
`RestContext::get`, with a reviewed `'static` compiler instantiation. Its specific
non-decoder classification does not classify the resolved JSON or other lifetimes.
The constructed native `private_classes` test
`production_native_public_rest_future_waits_and_preserves_candles_and_rejection`
holds terminal HTTP for Futures, PM and Spot, checks pending before release and
real GET path/query, then reads both Candle rows' native field bits or the retained
HTTP rejection status/data. Settlement must not resend. The scoped
`scripts/public-rest-alias-contracts.mjs` authenticates compiler artifacts and
source claims before running ordinary alias probes. Separate
`scripts/public-rest-alias-controls.py` faults fulfilled and rejected continuation
paths independently; both must compile and fail the named native test, then pass
after exact source restoration. Other owners and generic instantiations remain
outside this finite evidence.

Native HTTP weight guards receive canonical shared AxiosHeaders on both successful
responses and reconstructed rejection responses. NativeImports reads and invokes
the actual get method with the original header receiver. The native
`production_native_request_weight_callbacks_observe_success_and_rejection` test
compares five signed request outcomes, including mixed-case weight/retry headers,
against actual maintained TS guards with installed Axios. Its only request stub
is the terminal RestAdapter; the factories, signer and response guards are live.

The request-weight `Failure` and `HeaderGetter` aliases retain native error and
bound header-reader capabilities. Their compiler receipts classify the actual
alias traits; they do not execute a decoder or the callback. The existing native
`production_native_request_weight_callbacks_observe_success_and_rejection` test
constructs the real parent and compares five signed HTTP outcomes to TS/Axios.
`scripts/header-alias-controls.py` independently corrupts the production header
argument and rejection-response forwarding, requires compiled named failures,
and restores exact source bytes before requiring a pass.

The twenty history/API/lifecycle capabilities have individual source-role records
and authenticated native compiler probes in `scripts/lifecycle-alias-contracts.mjs`.
Their real native owner/HTTP/socket/signing/Store witnesses and compiled controls
are described in `LIFECYCLE_ALIAS_REVIEW.md`. Income paging uses the canonical
native typed bridge on both sides of the shared pager, preserving native number
bits while retaining the existing numeric transaction/trade ID String policy.

### Retired duplicate order throttle

The former `orders_throttle` generic module had no production caller; its only
Rust caller was its standalone oracle target. The assembled private constructor
already installs `PrivateState::start_fetch_throttled`, and its live
`throttled_orders` method selects and schedules that callback. The two TypeScript
private-socket source methods remain mapped to those canonical class bodies.
Do not restore `FetchOrdersCallback`/`LoadedProperties` or their duplicate helper
module to satisfy an alias inventory.

`tests/support/order_throttle_binding.rs` exercises the production native
Futures/PM constructor, 10000ms timer, both signed order requests and one loaded
Store publication with the ticker-loaded sibling retained. Run the native
`private_classes` target with filter `order_throttle_binding` from `sdk-rs`.
`scripts/live-order-throttle-controls.py` separately corrupts the real class delay
and loaded flag, requires compiled named native failures, and restores the exact
production file after each control. Historical `ORDERS_THROTTLE*_EXITS.json`
receipts describe the retired oracle-only helper; they are not current class
execution evidence. No package script directly names the deleted target/oracle;
the existing all-targets Binance oracle command continues to discover live tests.

The private/order alias review covers thirteen live capabilities with individual
compiler contracts and actual native binding/fault witnesses. See
`PRIVATE_ORDER_ALIAS_REVIEW.md` and `scripts/private-order-alias-contracts.mjs`.
`RefreshCallback` and `KeyFuture` were retired with test-only scheduling and key
wrappers. Keep the live receiver's native key future, retry/refresh methods and
single shared detach helper; do not restore parallel helpers for the census.


The private REST `SnapshotCall` capture preserves each source console argument
as a native value and forwards the explicit log severity. Caught errors use the
canonical `Thrown` materialization, so their name/message reach the console sink
as an Error rather than a message string. `snapshot_call_binding` in the native
`private_classes` target executes actual signed order requests and observes the
captured severity, original data bits/presence and caught Error before projection.
`scripts/snapshot-call-controls.py` independently corrupts those three production
forwarding paths, requires compiled named assertion failures and exact-restoration
passes. `scripts/snapshot-call-alias-contracts.mjs` authenticates fresh compiler
metadata and checks only the selected alias classification; it does not replace
the native callback execution witnesses or classify the continuation aliases.

The public ticker handlers consume canonical native EventArray values from the
constructed public classes. The old PublicEvent/PublicEvents JSON adapter and its
oracle-only test/control tail are removed; they had no production caller.
The actual native ticker census in private_classes retains source comparisons for
all three families; its production field-routing control requires three compiled
failures and exact-restored passes.

The remaining five public stream aliases have a finite authenticated compiler
runner, `scripts/public-stream-alias-contracts.mjs`. Failure, TickerFields,
CallbackList and CoreInvoke are specific non-decoder capabilities; their native
trait receipts do not claim callback execution. The constructed native ticker
census and inverse socket topic/depth tests execute their live consumers.
BinanceDepthUpdate remains a live map decoder. Its two native observations read
all keys, nested values, row cardinality, UTF16 units and numeric bits. Run the
runner with `--controls` to corrupt opaque-member retention and signed zero
independently, require named native failures, then restore the exact contract.
These five rows do not classify other function owners or feature combinations.

Keep production imports explicit. The canonical accepted-set scanner must prove
macro provenance across each source scope; wildcard imports can hide that proof
even when the current business logic compiles. Import changes also require review
of any affected source pins; nested helper bodies keep their existing contracts.

The public frame RAII guard lives at module scope so compiler discovery covers
its lifetime and Drop implementation. The native `public_frame_guard` test in
`private_classes` checks packet release after success, rejection and unwind,
original failure identity and the next real Store ticker update for all families.
`scripts/public-frame-guard-controls.py` removes the actual pop, requires a compiled
packet-retention failure and restores the exact source before requiring GREEN.

The private-construction Clone reviews in the Binance construction policy cover
three reusable capabilities: the inverse listener retry context, shared class
settlement observations and captured private arrows with their receiver/base
leases. The disconnected candle-listener witness executes the actual retry
clone before a real socket frame and cleanup; the native private-clone witness
holds a listen-key request, drops the original settlement and observes both
clones before invoking a cloned reconnect arrow. These reviews do not classify
raw-input decoders or widen accepted representations.

LiveFlag constructor receipts classify typed callback/Cell state. Native retry
proofs live in tests/support/live_flag_binding.rs; live-flag-controls.py requires
independent compiled reader/conversion failures and exact source restorations.


The private REST continuation aliases have finite native evidence in
`tests/support/snapshot_future_binding.rs`. The actual PM exchange starts the
regular signed request, awaits its member result before invoking the conditional
member, and keeps the aggregate pending until that second HTTP response settles.
The witness reads ordered native fields, signed-zero/NaN bits and own-undefined
from both results, and separately checks rejected conditional settlement against
the source Store fallback and Base error emitter. Only terminal HTTP is held.
`scripts/snapshot-future-controls.py` independently corrupts member order and
request-result forwarding, requires successful compilation plus the named native
failure, restores exact production bytes and requires GREEN after each fault.
`scripts/snapshot-future-alias-contracts.mjs` authenticates compiler artifacts and
classifies only `SnapshotFuture`/`SnapshotRequestFuture`; those classification
receipts are distinct from executing the real continuations and do not classify
the response payloads or unrelated aliases.
The seven `public_listeners` aliases have a finite compiler-authenticated review
in `scripts/public-listener-alias-contracts.mjs`. `Book` is an actual map decoder,
with native per-level binary64 observations and Rc identity; the six callable,
future and sequence-state capabilities retain their individual role reviews.
`tests/support/public_listener_aliases.rs` constructs the real native dependencies
for Futures/PM/Spot. It advances the virtual timer before subscription, holds
terminal snapshot HTTP while two frames buffer, checks the exact endpoint and
flat query, then checks one retained book through live updates and cleanup.
`NativeListenerOperations::get_snapshot` passes raw query fields to `run_request`:
the latter installs GET args into config.params, so callers must not wrap again.
Separate compiled semantic controls require named native failures and exact
source restoration for every reviewed alias.

Private REST shape checks, field aliases and diagnostics consume the canonical
native graph directly. Never restore the recursive `json_data` projection.
Existing borrowed-JSON array validators delegate to the same native guard.
The executed function contracts and constructed native HTTP witnesses are
reviewed in `PRIVATE_REST_CONTRACT_REVIEW.md`; snapshot callable aliases remain
a separate cohort. Refresh the two reviewed nested-helper source pins after
reviewing any change to `private_rest.rs`.

The snapshot rejection fixture settles rejected HTTP through `RestFailure`, not
a fulfilled response with status 400. Its exact Base-formatted emitter assertion
distinguishes transport rejection from malformed fulfilled-body validation.
`scripts/snapshot-rejection-fixture-control.py` compiles the old fulfilled fixture,
requires that assertion to fail and restores the genuine rejection path. This is
a fixture-semantics check, separate from the two production continuation controls.


The legacy `public_message`, `private_message`, `spot_public_message` and
`public_candle_delegate` callback extractions have no production callers and were
removed with `message_entry` and its standalone oracle/control receipts. All
nine mapped source members retain their existing concrete native class bodies.
Use `private_classes --features native` for actual native private frame/Store,
public ticker generations, candle delegation/cleanup and depth recovery proofs.
Registry/N-API construction reaches those concrete classes. The current wasm FFI
exports only `parity_probe`/`version`, with no legacy message entry.

The standalone `delegated_methods` extraction layer has no production caller and
is retired with its options alias and mock-only oracle. Keep the six TypeScript
rows on the live ExchangeState listener/dispatch members. The native candle,
depth and historical-position witnesses and compiled live history flag control
are documented in DELEGATED_ROUTE_REVIEW.md; do not recreate forwarding helpers
to satisfy accepted-owner discovery.

The ten `signing` accepted-set owners are outbound codecs and request-signing
transformations, with individual non-decoder role records and exact compiler APIs.
Their classification receipts do not execute signing or establish arbitrary-JS
parity. `tests/signing.rs` and `scripts/signing.mjs` separately execute the typed
signer against the actual TS signer with installed Axios, qs and create-hmac.
Keep the existing UTF-16 spread, nonfinite-config and owned-snapshot limits.
