# Phemex venue port

Behavioral reference: [TypeScript venue guidance](../../packages/safe-cex/src/exchanges/phemex/CLAUDE.md).

The constants slice transcribes the twelve runtime exports in phemex.types.ts
and BROKER_ID from phemex.api.ts.
`constants.rs` retains own-entry order, including duplicate endpoint values in
Grafana lists. Ordinary object lookup uses the shared `lookup` re-export;
consumers must preserve `TableHit::Inherited` until their source branch decides
how to handle it. No own-only lookup fallback is implied by the tuple storage.
Runtime mutation of exported tables is outside this immutable configuration slice.

`tests/constants_oracle.cjs` transpiles and executes the actual venue types module,
shared types module and LogSeverity dependency. It refuses unexpected runtime
imports and requires SAFE_CEX_TYPESCRIPT_PATH. Every exported constant is compared,
including ordered entries, and lookup observations capture typeof before JSON.
RECV_WINDOW is checked by binary64 bits. BROKER_ID executes the actual AST-selected
source declaration without loading the API factory's unused runtime dependencies.

Response types decode through shared numeric ingress, with f64 Number fields and
exhaustive JsNumberOutput registrations. The compiler-based type census reads the
actual aliases (including the generic intersection) and traverses inherited fields.
Native field assertions cover all 49 declared account/position fields with distinct
values; the live Node corpus also exercises each numeric field through text, owned
and borrowed inputs, directly and through the flattened envelope. Keep the
symmetric serde-name controls: a roundtrip alone would miss their defects.

The two type rows declare CHOSEN boundaries in their manifest notes. In particular,
PhemexApiResponse<T> reserves code/msg outside T as a CALLER PRECONDITION; it does
not universally reject optional/defaulted overlapping payload types. Unknown fields
are omitted by these declared-shape models. A later mapper returning the whole
wire row as info must retain raw ingress separately, not serialize this projection.
The account read body is implemented below; remaining REST and WS are pending.

Scoped gates use cargo with --jobs 1: check, test -p safe-cex-venue-phemex,
host clippy --all-targets -- -D warnings, and wasm clippy --target
wasm32-unknown-unknown --lib -- -D warnings. Run tests/constants_controls.py with
SAFE_CEX_TYPESCRIPT_PATH to prove the named assertions fail under production
mutations and pass after each restoration. Its tomllib inventory requires one
production mutation per ported constant row, plus two inherited-read controls.
Use --only BROKER_ID when validating that newly added row; the full inventory is
still checked before filtering. Run tests/types_controls.py for type-row and
inherited-census controls. This corpus has no external consumers.

The manifest gate is an isolated tooling workspace so the venue has no forbidden
runtime-to-tooling dependency. From repository root, run:

```sh
CARGO_TARGET_DIR="$PWD/sdk-rs/target" /opt/homebrew/opt/rustup/bin/cargo run --locked --manifest-path sdk-rs/safe-cex-venue-phemex/tests/manifest-gate/Cargo.toml --jobs 1
```

It resolves the real shared source index, filters both indexed sets to this crate,
and checks only the five Phemex manifests with no exemptions. Keep the tool's
separate Cargo.lock committed. The oracle's observation dictionaries must have
null prototypes: an ordinary object's `__proto__` setter discards that case.
Both sides assert the observation census so the missing case cannot pass silently.

The first utility body, calculate_price_diff, has explicit inputs and delegates to
shared safe-cex-num subtract. Its live oracle extracts the actual class field
initializer and executes actual safe-math plus actual PositionSide dependencies.
One Node invocation returns all 722 numeric cases plus symbol cases; only NaN is canonicalized. Run
tests/utilities_controls.py for the production branch-inversion fail/restore check.
format_symbol_in compares every Unicode scalar plus multi-character strings; its
UTF-8 boundary is declared CHOSEN. format_symbol_out consumes shared Market slices,
selects the first match before truthy fallback, and preserves JSON value type and
numeric lexemes. Its JSON/UTF-8 domain is declared CHOSEN. Numeric witnesses check
the native Number variant before bits: coercion alone can hide numeric-string drift.
The four production controls cover branch reversal, uppercase, first-match ordering,
and nonfinite Number-to-string drift. Other REST and WS remain pending.

`account::get_account` ports the complete getAccount method body with configured
read and fallible error-emitter callbacks. The read callback supplies already
parsed xhr.get response.data; createAPI/signing is still pending. Reuse shared
ExchangeAccount, ExchangeError and js_string rather than inventing venue copies.
The method preserves read/emitter failure identity and emits before constructing
the missing-user-ID error. The live actual-method oracle compares 30 cases in one
Node invocation, including exact endpoint/currency, native user_id, falsy values,
nonfinite/rounded numeric lexemes, coercion failures and missing chains. This row
is CHOSEN for the shared JSON/UTF-8 input domain and Rust future abandonment;
callers must retain/poll the method future to completion to mirror JS continuation.

Existing history capability finding is carried forward, not re-derived:
`docs/plans/TRADE_DATA_TRIAGE_v1.md:359` and
`docs/plans/HISTORICAL_DATA_v1.md:504` record Phemex as symbol-scoped with no income
ledger. The fills axis reports unsupported and is never called; the capability
BRAND gates it; there is no metadata flag. This read slice does not alter it.

`fetch_tickers_raw` uses the shared get_key::v helper and keeps the strict null
check: missing error emits undefined and returns null (named ts-bug); only an
explicit null error returns raw data. Primitive data throws the source in-operator
TypeError before emitting. JSON values retain their numeric lexemes; witnesses
capture native type and Number bits before stringify. Rejection identity survives
both the read and emitter seams. The row declares the JSON/UTF-8, reference-identity
and future-lifetime boundaries; configured API construction remains pending.
Run tests/ticker_reads_controls.py for null-return, endpoint and emitter controls.

Cardinality witnesses pin source groups and native comparison totals independently.
See CARDINALITY_AUDIT.md for the measured baseline truncation survivor, preserved
oracle hashes, and per-target scope. The supplement wrappers execute the original
oracles in the same Node process. Run cardinality_controls.py for native
empty/subset/extra-output and type-count-bypass controls; run
source_cardinality_controls.py both normally and with --bypass for source-group
controls. Each script refuses compile-only failures and restores original bytes.
Do not replace fixed native walks with oracle-derived loop bounds or zip alone.

`validation::validate_account` ports the actual validateAccount try/catch body.
Configured reads supply parsed response.data or a caught JSON error value.
Reuse shared js_string/parse_float, Balance, ValidateAccountResult and
AccountValidationError. The total's finite check precedes used coercion. Source
`parseFloat(used) || 0` normalizes negative zero and NaN only; Infinity survives.
Emission uses response.data.message; the returned error uses response.data.msg.
Do not conflate these fields or stringify truthy runtime error values. Emitter
failures escape the catch body unchanged. The live 62-case witness counts every
read/emitter call and result branch and directly checks all native balance fields.
Run tests/validation_controls.py for twelve production fail/restore controls.
JSON/UTF-8, JS own-property/alias identity and future-lifetime boundaries are
explicitly CHOSEN in the manifest; this is not API construction or a live request.

`ticker_parser::{parse_tickers, fetch_tickers}` continues the REST read path using
shared Store reads and shared Ticker numbers. Source lookup uppercases the symbol,
but settlement suffix checks use the original symbol; lowercase suffixes remain
excluded even when their uppercase market exists. Fallback clones the shared
Store ticker snapshot (CHOSEN alias boundary). Non-array truthy results and invalid
symbol/coercion values preserve the source TypeError rather than silently skipping.
The actual-source oracle has 41 parser cases and six composed read cases. Fixed
case/per-case output counts and 13-field inventories constrain both sides before
comparison: 55 native tickers, 715 fields, 18 errors, six reads, three emissions,
and two completed rejection-identity checks. Run ticker_parser_controls.py for
compiled mutations with named failures and restoration; its output records the
specific fields proved, not a whole-venue precision/decoder coverage claim.
The parser consumes JSON arrays, not a new venue deserializer; its empty/subset
controls run after the whole input has been processed and test returned cardinality.
Existing collection-decoder proof remains in CARDINALITY_AUDIT.md.

The production `binding::exchange::PhemexExchange` now constructs on the shared
`safe-cex-exchange::js_runtime` and dispatches the Exchange trait into the existing
native REST/order/session/socket bodies. Dependencies supply HTTP, a WS runtime,
an optional external-public-data factory, and a tunnel installer when requested.
The constructor keeps the source secondary-key client and 3 RPS limiter distinct
from unlimited order creation. Source inherited unsupported methods still reject
with the BaseExchange message; historical fills keep their symbol-scoped route.
`tests/exchange_binding.rs` exercises all sixteen required methods, batch placement,
history, disposal during a pending read, public-only startup and remote signing
failure/acknowledgement ordering against a scripted transport. Amount and price
steps differ in the fixtures so selecting the wrong axis changes the payload.

Cancellation also uses the unlimited client. Single and batch replies validate
the response envelope's code/msg, leaving live Store orders untouched on a
rejected batch. The assembled placeOrders member checks all caller IDs before
placing its first order, matching the inherited source batch preflight.
The order_regressions unit tests run the real assembled binding/clients;
they cover both batch outcomes, a full read-limiter window, and duplicate IDs.
Symbol cancellation checks the actual Store filter, selected IDs in the signed
DELETE query and unrelated orders staying open. Its dead callback helper and
fake-only test were removed; the manifest points to binding/methods.rs::async_method.

The production symbol helpers and raw ticker reader use the shared Js carrier.
`utilities::{format_symbol_in_value, format_symbol_out_value}` retain lone UTF-16
surrogates and raw info values, while `ticker_reads::fetch_tickers_raw_value`
retains response identity. The older UTF8/JSON convenience helpers remain for
existing typed callers. `tests/binding_values.rs` compares the actual TypeScript
helpers for all 2048 lone-surrogate units, raw info output and raw response identity.
The private reconciliation prefix resets its timer, snapshots/clears its symbol
queue and begins the account read before returning a future. A detached AccountRead
keeps the continuation from borrowing mutable host state across that boundary.

The Exchange trait binds candle history and public candle/book listeners to the
venue readers and production public socket, applying source symbol formatting.
The socket reads its parent's timeframes and xhr. Snapshot await continuation
yields before consuming buffered deltas, including an already settled response.
The bound_chart regressions cover REST candle rows, socket frames, snapshot-plus-
delta merging and unsubscribe cleanup through the real assembled binding.

Five preexisting typed-edge FINDING rows remain: response-envelope msg,
account.currency, getAccount.userId, validateAccount.error, and parseTickers' symbol
and Store key. These require the coordinated shared string model assigned to core
(ddi2il-10); they are not source-carried defects and are not marked ported. The
production raw helpers and eager reconciliation bug are repaired in this binding.
Run `scripts/binding-controls.py` for each bound trait method and each repaired
helper; it requires compilation, a named runtime failure, and exact restored pass.

The Rust registry now constructs Phemex on the production native HTTP/WS host
with hosted signing and the shared gateway provider. Its native construction
regression reads markets and verifies the HMAC on an account query over local
HTTP. The native host retains the shared caller-owned limiter argument.

The production dispose wrapper delegates to the source body, which invokes
BaseExchange.dispose once before its public/private socket cleanup. The wrapper
does not add an earlier duplicate base call. The source-super regression and
compiling skip-super mutation verify the flag and inherited listener cleanup.

The native registry supplies the shared default tunnel adapter when requested.
The registry tunnel witness verifies public/private reads over real rpc.http,
proxy-token authentication, signed query bytes and zero direct HTTP. Missing
and no-op installer mutations must compile and fail before exact restoration.

`PhemexApiResponse<T>` streams `code`/`msg` directly from the original map and
forwards payload fields to `T` through a filtering MapAccess. This avoids Serde
flatten's intermediate Content buffer, which misreads arbitrary-precision JSON
number tokens in a typed payload. The flat serialization schema and original
`T: Deserialize<'de>` bound remain intact, including borrowed payloads. Metadata
is checked even if a custom payload finishes early. Existing numeric-ingress
frontdoors retain their ECMAScript Number and nonfinite-output checks.

The original 44-owner native/default accepted-contract census has a closed
compiler-selected runner, `scripts/accepted-census-contracts.mjs`, backed by the
per-crate Phemex contract shard. It executes 30 decoder results and 14 individual
classifications; independent native witnesses and mutation controls accompany
non-decoder/typed roles. See safe-cex-parity/accepted-set/PHEMEX-REVIEW.md for the
scope and commands. This is not full current discovery: post-census binding
owners, aliases, local scopes, feature combinations and WASM remain explicit
obligations rather than inheriting the selected contracts.

Order-side and reduceOnly formatting use BaseExchange's
getOrFetchPositionsForSymbol with a scalar symbol. Stored rows avoid REST;
missing rows fetch and populate the cache only after loaded.positions is true.
Before positions load, only order-side raw-mode recovery reads the account,
and the empty positions result keeps reduceOnly omitted as in the source.
The production binding tests cover cached placement/cancellation, unloaded
formatting, and the single cached recovery read after positions load.


`tests/support/private_binding_topics.rs` drives authenticated frames through the
real constructed private socket and shared Store. Separate cases cover order
snapshots replacing Store without fill notifications, terminal deltas emitting
the execution price, supported-market position updates, and per-currency balance
merging with current position PnL. These are coverage of existing source behavior,
not additional venue fixes.


Post-merge #418 review found snapshot failure warnings dispatching to the socket's
missing log instead of parent.log, and stringifying Error rather than reading its
message. The real constructed public-book regression proves both failures with
compiled REDs. It compares the maintained TS subscription, asserts the warning
text/severity, next-delta retries, 30-second warning throttling/reopening and later
snapshot recovery with the buffered delta. Error instances read message; other
thrown values retain String(value). The source supplement includes plain object
and string rejections. The real read limiter remains active in this regression.

The constructed ticker corpus in `tests/exchange_binding.rs` drives 72 packed
three-row frames through the live public socket. Each of nine numeric inputs
varies independently across eight malformed/coercion values, while every native
Ticker number is compared by bits with the maintained constructed TS exchange
and DefaultStore. The control runner swaps last/mark in the live packed mapper
and requires a compiled native assertion failure followed by exact restoration.
