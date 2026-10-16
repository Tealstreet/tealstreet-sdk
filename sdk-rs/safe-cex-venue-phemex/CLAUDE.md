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
BRAND gates it, not supportsExecutionsHistory. This read slice does not alter it.

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
