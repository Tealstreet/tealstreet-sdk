# MEXC futures venue

Behavioral reference: [TypeScript MEXC guidance](../../packages/safe-cex/src/exchanges/mexc/CLAUDE.md).
Use the [shared exchange contract](../safe-cex-exchange/CLAUDE.md); variants stay within this crate.

REST signing and private WebSocket login take `current_time_ms` from their
construction dependencies. The native registry passes the same shared
`VIRTUAL_CLOCK.get_current_time_ms` getter to both; host wall time remains for
runtime accounting and retry/rate-limit timing. The same callback drives order
submit pacing/cooldown, optimistic timestamps, stop-order lookbacks and all
placement/cancel/rejection shields. Do not derive these source virtual-clock
reads from `ExchangeRuntime::now_ms`. The native registry regression
`native_host_mexc_clock` applies a 90-second sync offset and checks both signed
wire timestamps and HMACs over real loopback HTTP/WebSocket transports.

REST ticker funding timestamps use the required host local-wall-time converter,
just like other MEXC readers. A constructed REST regression compares the actual
TypeScript normalizer under America/Los_Angeles for offset-less dates, explicit
UTC, date-only and numeric times. Only offset-less date-times invoke conversion;
UTC/date-only/numeric inputs retain their source bypass behavior.

Leverage refresh uses stored symbol settings before deriving from positions,
matching the source short-circuit. Derivation reads positions/account mode only
when settings are absent. A constructed public-trait regression uses real
setLeverage to cache 50, seeds stale position leverage 20 and checks empty or
untyped leverage rows and missing markets against the actual TypeScript method.
Cached margin/position mode and both leverage sides survive the refresh.


Regular-order `externalOid` generation uses the shared crypto `uuid_with_entropy`
helper with runtime entropy: generate 21 nanoid characters, then strip `-` and
`_`, as `utils/uuid.ts` does. The runtime's generic `unique_id` contract does not
specify that format. The trait-level wire test pins length and separator removal.

The unused generic `private_session`, `public_subscriptions` and `ws_lifecycle`
socket bodies and their fake-host tests/oracles/controls were removed after the
wave2-vacuous-tests audit. Shared value traits and owned-method completion helpers
are still production dependencies; their bodies moved unchanged to `method_tasks`.
The old isolated-body manifest claims now point at production owners and retain an
explicit coverage finding rather than reporting those removed tests as proof.
`safe-cex-registry/tests/native_host_mexc_private_frames.rs` covers authenticated
regular orders, partial/full fills and cancellation, the emitted fill event,
positions and debounced balance refresh over real native HTTP/WebSocket servers.
Disconnecting all four production topic routes makes all four named tests fail;
restoring those routes and removing the duplicate bodies keeps them passing.

Private login-failure diagnostics use the shared JavaScript JSON stringifier.
The constructed venue's real private receiver covers ten numeric/key-order
profiles, independently checked against the maintained TypeScript handler.

Protocol tables are maintained in `src/types.rs` and compared in full to live
TypeScript by `tests/tables.rs`. The envelope uses a private map-only decoder;
absent optional fields differ from present null, while unknown data retains null.
The TypeScript compiler checks the envelope accepted-set corpus under strict
null and optional-property settings. No oracle falls back to root node_modules.

Run `yarn venue-oracle:mexc` with cargo/rustc on PATH and
`SAFE_CEX_TYPESCRIPT_PATH` pointing to the existing isolated TypeScript install.
`tests/tables.rs` spawns a child assertion test against an in-memory TS mutation.
`scripts/table-controls.py` independently mutates the Rust table, requires the
named assertion failure at exit 101, restores the source, and verifies green.
Implementation proceeds tables/types, then pure functions, then exchange and WS.

Pure normalizers use shared JS coercion and return errors when coercion throws.
Numeric oracle serialization records IEEE-754 bits before JSON conversion,
including nonfinite results. `Balance.currencies` uses the shared JsObjectMap:
source insertion and canonical integer-key ordering now match. The committed
ordering finding flipped after repair and now requires live-source equality.
Query serialization consumes shared root collation and preserves qs repeat-array
semantics. Timestamp normalization uses shared Date.parse with an explicit host local-wall-time
to UTC callback; numeric positive timestamps bypass TimeClip, matching TypeScript.
createAPI composes the shared HTTP client, classified retry and signing owners.
Retry attempts retain the shared transformed body state; tests compare both HMAC
preimages and wire bytes across raw text, whitespace-valid JSON and empty bodies.
The response adapter delegates big-integer JSON decoding to the shared parser.
Shared HTTP header merging now matches Axios aliases and implicit Accept.
The former header and response negative-zero findings flipped after the shared
repairs; strict whole-header and pre-serialization numeric-bit comparisons now
cover all factory scenarios, including both overflow signs.
Runtime oracle packages resolve alongside the isolated compiler. MEXC selectors
include create-hmac, qs, axios and axios-retry because native cargo also finds
these tests. Only shared transport-factory and clock boundaries are injected
in the factory oracle; the venue factory and npm packages execute live. Both
MEXC test runtimes implement required `ExchangeRuntime::monotonic_now_ms` with
elapsed fake clocks advanced by their sleep callbacks; focused tests pin that
the runtime and rate-limit sleep paths share the clock.

The table lookup census is 27: ten utility reads are immune through numeric
coercion, one API BASE_URL read is a closed testnet/livenet choice, fifteen
exchange reads and one public socket interval read remain inventoried in the
live inherited-member oracle. The constant Market reverse-type lookup is also
closed; the cancel-reason caller numerically coerces its input. The two INTERVAL
guards and reverse time-in-force OR fallback can change control flow on
prototype names. Direct reverse-order type results retain functions; reverse
side property access yields undefined on an inherited function. These consumers
remain pending implementation and output-boundary decisions; shared TableHit
now matches all 52 lookup witnesses. No local
prototype-aware implementation is provided.

All pure mapper bodies now exist. Regular-order volume and sized-stop volume
re-coerce their numeric intermediate (canonicalizing negative zero); plan orders
convert the raw volume once and retain negative zero. Execution commissionAsset
can be arbitrary in TypeScript; this port explicitly refuses whole records with
non-string values under the closed-typed-output-boundary ruling, with five live
refusal witnesses. This is a declared CHOSEN boundary, not a pending shared fix.

Stateless exchange helpers are in exchange_helpers.rs; the oracle extracts their
actual maintained class field initializers and constants. extractRows currently
uses shared canonical numeric string-index access for nested arrays. Its remaining
FINDING is Array.prototype under __proto__; ordinary production orders/items
keys are unaffected. The array-prototype representation is explicitly held. No local prototype-aware map is introduced.

Cancellation shields are explicit ID-map operations with clock callbacks in
exchange_helpers.rs. Preserve the exact clock-read sites: a filter may cross an
expiry boundary between two orders, while missing IDs and short-circuit paths
do not read the clock. Response guards in exchange_errors.rs preserve complete
row filtering before any error-message coercion. Both have maintained live TS
oracles and compiling Rust mutation controls.

`MexcExchange.note_rejected_regular_order` ignores an empty ID, strips the
`__` suffix before recording its 30-second shield, cancels a matching
non-canceled Store order, and then emits the known or fallback venue reason.
The fallible Store API now propagates subscriber failures after the status
commit, preventing the later error emission. Its row remains a FINDING until a
targeted throwing-subscriber witness pins that order. The five-case live-source
oracle compares order state, shield keys/times, and error text; separate
TypeScript and Rust clean-ID mutations fail the named assertion.

`MexcExchange.cancel_orders` handles all three cancellation APIs in source
order, keeps the 50-item request batches, and propagates notification failures
after the Store status commit. Its six-case actual-source oracle pins request
payloads, status and shield changes, rejection, batching, and subscriber order;
Rust and TypeScript cancellation-kind mutations fail named assertions.

`validate_account` returns the fetched balance on success and emits the selected
error before returning an error result on failure. `get_account` validates first,
then hashes `options.key || "mexc"` with SHA-256 and keeps the first 24 lowercase
hex characters. The maintained oracle also pins that response.data.message/msg
outranks the transport message in TypeScript; Rust currently cannot recover that
detail after `fetch_balance` projects the REST failure, so both rows retain that
specific divergence.

The public socket's TypeScript `messageHandlers` is an ordinary object, but
venue-created registry keys are prefixed (`depth.<symbol>`,
`kline.<symbol>.<interval>`, and `trade.<symbol>`), so these add/remove/route
lookups cannot spell an inherited Object.prototype name. The incoming channel
used by `recordMessageHandlerStats` is wire-controlled and is a separate path;
it goes through the shared inherited-handler stats read implementation. Do not
replace it with a venue-local handler-count lookup.

Order fetching and reconciliation now live on MexcExchange. The orders hook
implements the venue override; both dyn calls and captured method references
must reach its disposal and placement-shield logic. Full snapshots place fetched
rows first and preserve shielded stops; regular-only snapshots retain existing
stops before recent regulars and fetched rows. Scoped full snapshots also retain
unfetched open regulars without a recency bound. Keep the source's separate
filter passes and clock sites. Actual DefaultStore constructors are used on both
sides of the live oracle, including disposal reset and notification counters.
The constructor and socket lifecycle remain pending. Reconciliation mutates no
venue state before the fetch await completes and has no later await or latch;
dropping an unpolled or pending fetch leaves the next snapshot usable.

Both reconciliation returns have the canonical owned-snapshot-vs-live-alias
choice: TypeScript returns order objects also installed in Store, so a later
mutation through the return changes the stored row. Rust returns owned orders.
The live-source oracle records both views after a returned lastUpdate mutation;
this is distinct from disposal, shield, and ordering parity.

Market, ticker, position and regular-order info retain raw-object aliases in TS;
plan/stop info shallow-spreads raw, retaining nested aliases only. Balance info
retains both the input array rows and per-currency row aliases. Rust owns these
snapshots. All seven rows cite owned-snapshot-vs-live-alias; the live oracle pins
both mutation directions, including the spread/non-spread distinction. Balance
currency ordering now matches the repaired shared map.

Balance also retains a separate prototype-write FINDING: a __proto__ currency
sets the JS currencies object's prototype to a Balance, exposing inherited free,
used, total and upnl values. Rust omits that key but cannot reproduce those reads
with the typed currency map. The live witness records reads before serialization;
matching Object.keys alone does not close this routed shared-write limitation.

MexcEnvelope numeric-safe ingress uses `decode_mexc_envelope` or
`decode_mexc_envelope_value` with `JsNumberInput` as its generic payload, then
decodes `data` into a typed output. Do not use direct `serde_json` envelope
deserialization for numeric parity: the maintained nine-case witness keeps its
prior failure as baseline while the checked raw and existing-Value paths compare
all code/data f64 bits with Node. This includes integer `-0`, `-0.0`, `1.5`,
`1e3` and both overflow signs. Paired spellings strongly implicate integer-
versus-float Number representation in the old existing-Value rebuffer, but do
not prove serde_json's internal cause. Other numeric mappers use source
`Number.parseFloat(String(value))` semantics rather than typed f64 ingress.
Numeric protocol-table decoding only targets enums/bool.

Pure symbol configuration helpers select the first live position in source order,
where negative and infinite contracts are truthy but zero and NaN are not.
One-way mode shares the first position's leverage only for a missing side;
present sides use their own rail and numeric fallback. Existing settings bypass
derivation in order leverage selection, even if position info would throw.
Leverage re-coerces through source toNumber, canonicalizing numeric negative zero.
The internal config has native numeric fields and no new wire decoder.
`find_open_position` preserves source order and selects only a symbol/side match
with `contracts > 0`; zero, negative zero, negative, and NaN contracts do not
select, while positive infinity does.

The MEXC precision audit is per field. The utility corpus checks
`Market.precision.amount` and `.price` with separate compiling identity
mutations. Rust order payload formatters are still unimplemented, so the
TypeScript volume/price `adjust` paths are not covered by those metadata checks;
their distinct-step cases are required when those Rust formatters are added.
`format_change_leverage_payload` selects the first positive matching position
and emits a `positionId` payload, optionally including `openType`; without a
usable position id it emits the market-id/position-type flat-rail payload.
It reuses `find_open_position` and retains leverage as native `JsJsonValue::Number`
until wire serialization, so pre-serialization signed-zero and non-finite bits
remain observable.

`MexcExchange.fetchLeverageForSymbol` overrides the shared hook through both
`Exchange` dispatch and a concrete method. It derives defaults before the
request, fetches leverage rows only for a known market outside public-only mode,
selects the first long and short rails, stores the per-symbol result, updates
the market maximum only for a positive changed cap, and falls back to derived
settings after request failure. Its source oracle extracts the maintained
TypeScript method and compares success, public-only, missing-market, and
request-failure paths; Rust and TypeScript mutation controls fail the named
buy-leverage assertion. `updateSymbolMetadata` also dispatches through the
override and awaits that fetch with the same symbol; a source-extracted wrapper
oracle and independent mutation controls pin that delegation.
The `fetchLeverageForSymbol` notification finding is closed: the live oracle
constructs the production TypeScript `DefaultStore`, commits the market cap
before the first subscriber throws, skips the second subscriber, catches that
error, then commits fallback leverage before the same subscriber throws again.
The Rust witness asserts the same market commit, caught failure and fallback
update. `updateSymbolMetadata` still needs a throw witness through its wrapper.
The same gap is noted separately on both already-divergent order snapshot
writers, `fetchAndSetOrders` and `fetchAndSetRegularOrders`: Rust now propagates
StoreMutationError after commit, but per-method Rust throwing-subscriber
witnesses are still needed. Their existing alias-choice findings remain
independent.

Order-fetch privacy gates call Exchange::is_public_data_only, preserving the
venue getter dispatch contract rather than bypassing it through the base helper.

The response interceptor uses RestResponseBody's current-data transform. It parses
strings with the source big-integer parser and passes native object/array/null
values through; it must not serialize HOW data back to bytes before parsing.
Model reads use json_value without pretending another interceptor ran.

WebSocket protocol work uses `safe-cex-exchange::BaseWebSocket` for lifecycle
and host I/O. `ws::subscription_key` matches source `JSON.stringify` property
order and the distinction between an absent `param` and explicit null. The
notification callsite inventory in `STORE_NOTIFICATION_AUDIT.md` records every
MEXC store mutation. Ticker and private-position witnesses verify that state
commits before a subscriber error propagates and later subscribers are skipped;
other mutation sequences retain their findings until individually witnessed.

Collection oracles pin fixed source totals and iterate fixed source indexes; never
derive a native comparison bound from oracle length. Their consuming-empty
deserializer control consumes every array element before returning an empty
array, avoiding serde's trailing-element rejection. It fails named native
assertions both with source counts active and with source-count and native-length
guards bypassed, leaving direct identity checks to catch lost rows.

The envelope corpus asserts each decoded native field (`success`, `code`,
`message`, `msg`, and `data`) before its serialization-shape comparison. Its
compiling swapped-wire-name control must fail at the named message identity
assertion; byte roundtrips alone do not establish field identity. Numeric-bit
captures canonicalize NaN to `7ff8000000000000` at capture only, retain signed
zero and finite bits, and record `process.arch` on each bit-bearing scenario.
The Rust capture helper applies the same NaN-only rule. The ingress controls
prove that `-0` remains distinct from `+0` and that finite payload bits remain
compared.

Fixture identity review scans every test `.mjs` object literal for repeated
same-expression values and reviews generated Rust `json!` records separately.
Repeated protocol input values are differentiated where they could mask a
wrong field lookup; equal alias-direction results on empty market cases,
compiler-option booleans, source-census counts, and all-present optional-field
presence checks are setup or symmetric-presence observations. Full market alias
cases assert both mutation directions, and full fill cases assert distinct
field values by name. The historical-fill corpus separates `side` from `role`;
a compiling Rust input-key swap failed at `execution maker field identity`, and
the in-memory TypeScript swap control fails at `MEXC TypeScript execution role
field identity`. A separate Rust control swaps commission and realized PnL and
fails at its named commission assertion. The fetch-orders corpus has a distinct
`orderId` identity scenario and a compiling Rust preference-swap control. The
numeric corpus gives `code` and `data` different spellings on each record and
checks both fields independently; none of these MEXC corpora feeds a gate
outside this crate.

The bit-exact NaN audit covers 19 TypeScript capture files: each canonicalizes
only NaN at capture and records `process.arch` and the Node major version; finite
values and signed zero remain bit-exact. The Rust support encoder applies the
same NaN-only canonicalization.
The ingress corpus has named signed-zero and finite-bit mutation controls.

Private fill delivery uses `MexcFillEvent` at the stored hook and both handler
constructors. Keep its eight native fields through emitter invocation: an empty
order ID is an own field, and binary64 amounts must reach native assertions before
any JSON serialization. The consumer-specific execution normalizer excludes the
unused commission-asset projection without widening the public Execution model.
The public execution path evaluates realized PnL and maker before returning an
asset projection error, preserving the source coercion failure order.

Order, position and stop-like handlers reread markets for each row and release
the Store borrow before invoking timezone callbacks. Private positions use the
normalizer's default Hedged mode; the constructor mode argument remains accepted.
The fallible-hook constructor propagates seen/shield failures at their original
call sites. Preserve these hooks when changing the fill emitter contract.


The PR 7424 rescue restores the alias oracle initial probe seeds to 1/1,
matching the independent native assertions; 8/2 was a left-over broken fixture.
Account validation success now checks native free/used/total, and cancellation
checks native ordered OrderStatus variants before any status serialization.
The cancellation row is FINDING divergent: rejected HTTP requests are rewritten
by post_cancel_payload and lose response/code metadata, unlike source identity
propagation. The transport supplement asserts both current native and source
views; the earlier ported classification is not retained. Validation fallback
and error-emission coverage are not claimed by the five-case account oracle.
The rescue's tracked gate sheet names exact measured SHAs and separately records
inherited unmerged shared-core owned-timer WIP; scoped MEXC gates do not imply
core-wide or workspace readiness.

Response parsing and request-attempt wrappers forward the inner success receipt.
Typed venue-refusal errors retain its effective request configuration. That native
provenance field is distinct from a claim that the source attaches error.config
when constructing its venue error.

The native registry supplies a hosted account to configured HTTP, the existing
BaseExchange signer and MexcSocketDependencies.remote_account_name. Keep all
three connected: public HTTP bypasses hosted signing, private HTTP signs remotely,
and WS login uses the same account with trimmed API key plus request time. A
signer error or absent signature preserves the source “Remote WS signing
unavailable” message and sends no login or private filters. Native registry
E2Es and compiled omission controls are documented in HOSTED_SIGNER_EVIDENCE.md.


The concrete `Host::for_of` uses the shared GetIterator/next protocol for JS
values, including empty and live Map iterator objects. It keeps the existing
opaque symbol-settings projection separate. Source disposal captures timer
values before clearing the current map; its iterator must remain live while
handles are cancelled. Do not restore raw-array admission or a map snapshot.

`ITERATOR_CONSUMERS.md` records the cross-venue for-of/spread/Array.from review.
The two native map-iterator witnesses cover empty and non-empty iteration across
values/keys/entries and raw Map, including insertion/deletion, native numeric bits
and reference identity. `scripts/iterator-consumer-controls.py` compiles an
iterator-object rejection, requires both named failures and restores exact bytes.
Its Cargo subprocesses have deadlines and private temporary logs.

The production private asset handler debounces `fetchBalance` for 1000ms and
swallows both fetch and Store-update failures, matching the TypeScript callback's
empty catch. `socket_assembly::registered_private_asset_refresh_swallows_balance_fetch_failure`
drives a real registered private frame, advances the host monotonic timer, and
requires one attempted balance read with no host exception report.

The two remaining generic socket copies in runtime_assembly (private_on_message
and public_listen_order_book) and their fake-host scenarios are removed.
Production private dispatch remains in sockets::PrivateProtocol; public book
subscriptions remain in ws_public::MexcPublicSocket. The assembly oracle now
covers only its seven live exchange-body groups (61 source cases). Manifest
rows cite those live owners and retain explicit coverage findings.

The constructed ticker corpus in `tests/exchange_trait.rs` covers 160 cases / 480 frames with every numeric alias, distinct fallback values and per-market contract sizes. Source Date parsing and native local-time conversion use the explicit UTC host profile.
It drives the production constructor/socket protocol and actual DefaultStore,
checking native number bits against maintained TS classes and DefaultStore.
Independent null, string, NaN-producing, object, overflow, signed-zero and false
inputs accompany distinct good rows, with finite Store resets before every case.
The shared `ticker-native-controls.py` compiles live field-publication faults,
requires named native failures, restores exact bytes and requires passing tests.

Reporting regressions retain the actual Error at the production worker post sink:
read `name`/`message` before JSON projection, and compare the shared JS stringifier
separately against actual source `JSON.stringify`. Numeric reports compare IEEE
bits rather than serde's integer/float storage variants. The 120 reporting cases
and six throwing-listener cases retain their original call and fallback floors.
`examples/reporting_worker_host.rs` constructs the real MEXC venue and drives
all three failing REST readers through `install_exchange_event_bridge` and
`WorkerDataPost::for_browser`. Build that example for wasm32, run wasm-bindgen
with `--target nodejs`, then run `tests/reporting-worker-host.mjs <generated-js>`
with `SAFE_CEX_TYPESCRIPT_PATH` set. Its six cases compare actual source readers
and worker registration to the native browser post and UI structured clone.
A cloned Error keeps its message but resets a custom name, matching JavaScript.
