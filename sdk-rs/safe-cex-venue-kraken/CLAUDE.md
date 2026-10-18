# Kraken Futures and Spot Rust port

Behavioral reference: [TypeScript venue guidance](../../packages/safe-cex/src/exchanges/kraken/CLAUDE.md).
Both variants remain in this family crate. `types.rs` maps `kraken.types.ts`;
`spot_types.rs` maps `krakenspot.types.ts`. `binding/` composes them with the
[shared exchange core](../safe-cex-exchange/CLAUDE.md) into the production
`KrakenExchange` (both names). The inherited `signWsAuth` row belongs to shared crypto.

## Exchange binding

`KrakenExchange::new` assembles `classes.rs` over the shared
`safe-cex-exchange/src/js_runtime` host: every class member target dispatches,
in `binding/members.rs`, to its ported body; the host traits are implemented
over `Js` in `binding/host*.rs` (REST family on the receiver scope) and
`binding/sockets.rs` (one `SocketHost` per socket object, owning the slots the
traits borrow). Every Exchange trait method either source class overrides calls
its assembled member (`--test trait_dispatch` drives each through the trait);
the five members Kraken inherits from base.ts reject with its `Not implemented`.
The host supplies two module-scoped values, never per instance: the Futures nonce
counter (`api::FuturesModule`) and the Spot client-ID generator (`spot_client_ids`).
REST clients come from the shared `client::factory` over the runtime provider:
`create_client` applies provider routing and the 20 s timeout, and keyed Futures
clients install the provider adapter (HOW when tunnelled); Spot never does.
`--test transport_binding` drives both through a scripted provider. Axios's
rejected response handler is reproduced by wrapping the client verbs, because
the shared client runs fulfilled interceptors only.

Gates: `--test exchange_binding` (scripted transport, every trait method, start
through private-socket auth), and `scripts/binding-controls.py` from `sdk-rs/`,
which requires one named failure per trait method and a restored pass.

Tables retain source own-key order as slices. Use `safe_cex_num::js_table_lookup`
when reproducing ordinary-object reads: `.find()` alone loses inherited values.
In particular `normalize_asset` retains the actual source's inherited non-string
results as `TableHit::Inherited`, instead of coercing them to strings. Its input
and mapped output are UTF-16 code units, preserving unmatched lone surrogates.
The tables represent the source module's initialized values and stock prototype;
no mutable module-global/prototype API is provided.

`KrakenBaseUrl` has explicit numeric-output registration. Future number-bearing
wire inputs must enter through shared `numeric_ingress`, with native output bits
observed before JSON. `INTERVAL` values are f64 literals, not an integer wire decoder.

The mandatory `tests/types.rs` oracle runs both complete actual TypeScript modules
and their real shared enum source. Set `SAFE_CEX_TYPESCRIPT_PATH` explicitly.
It compares every own entry and order, top-level and nested WS-map inherited
lookups, interval f64 bits, literal-union membership and decoded native variants,
and normalization output type/identity/UTF-16. It has no frozen shared fixture.
This slice has no interface-field census, record decoder, or opaque info input.

Scoped gates (from the repository root; cargo is available at
`/opt/homebrew/opt/rustup/bin/cargo`):

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kraken --jobs 1`
- `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kraken --jobs 1`
- `cargo fmt --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kraken -- --check`
- `cargo clippy --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kraken --all-targets --jobs 1 -- -D warnings`
- `cargo clippy --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kraken --target wasm32-unknown-unknown --all-targets --jobs 1 -- -D warnings`
- `python3.12 sdk-rs/safe-cex-venue-kraken/scripts/types-controls.py`

The controls runner requires exactly one production mutation for every row in the
two type manifests, measured with real `tomllib`. Each sequential mutation must
fail its named runtime assertion, restore original bytes in `finally`, and pass
a restored run. `--supplemental` also runs the five previously proven TypeScript,
surrogate, and capture controls. `--start-at` resumes an interrupted run without
repeating already proven rows; its output reports only the selected suffix.
Each run uses the single scoped `--test types` target and one Node invocation for
the entire corpus. Exit 126 never satisfies a negative control. Do not run it concurrently with other gates
or edits of the same files. Review is by sub-agent only; merge integration down
and repeat the scoped gates before handing the owner a SHA.

Structural gates for the types slice:

- `node sdk-rs/safe-cex-venue-kraken/scripts/types-manifest-check.mjs` invokes
  the maintained TS `extractFile` on the seven source files containing implemented rows and compares
  identities parsed by Python `tomllib` in one subprocess for the entire corpus.
  This is not a package typecheck.
- Build the maintained indexer with `cargo build --manifest-path sdk-rs/Cargo.toml
  -p safe-cex-parity --bin parity-check --jobs 1`, then run
  `python3.12 sdk-rs/safe-cex-venue-kraken/scripts/types-parity-check.py`.
  It retains the complete workspace source index for canonical reexport resolution,
  then gates this crate's public invention obligations against its owned rows in
  every committed Kraken manifest, with no exemptions. Scalar and split mappings
  retain all original targets and notes; provenance comes from committed
  rust_files declarations. It does not validate the inherited shared-crypto row
  or another crate's invention obligations. The list-mapping regression runs with
  `python3.12 sdk-rs/safe-cex-venue-kraken/scripts/types-parity-check-test.py`.


`fills.rs` and `income.rs` map all ten utility rows individually; their chosen
wire domain is declared once in [MANIFEST-SPEC](../parity/MANIFEST-SPEC.md#kraken-utility-wire-domain).
The private wire enum captures shared numeric ingress before untagged buffering,
registers all variants, and delegates retained construction and coercion to shared
crates. Mapper outputs are shared Execution/IncomeRecord values. There is no
opaque passthrough. Provide a fallible symbol resolver and an explicit host
local-time adapter. The UTC identity adapter is appropriate only for a UTC host.

`tests/utilities.rs` runs one Node subprocess for the complete actual-source
corpus. Private TS helpers are exported by appending an export declaration to
the unmodified source body. Captures record Node, V8, CPU architecture, platform
and timezone; only NaN is canonicalized and signed zero is observed by native bits.
Rust assertions read native output fields and enum variants before Serialize.
Distinct price/quantity/PnL, four separate ledger amounts, distinct identifiers,
and nonzero balance-delta supplements prevent same-valued swaps and collapsed
branches from satisfying these assertions. No local record-field Serde renames
are involved: the input enum keeps keys and the output is constructed directly.
This is not an interface-field AST census and makes no inherited-field claim.
Paired source acceptance/Rust refusal witnesses cover lone surrogates and the
shared depth budget. Host callback exceptions and resolver order are exercised.

Utility porting gate: `cargo test --manifest-path sdk-rs/Cargo.toml
-p safe-cex-venue-kraken --test utilities --jobs 1` with the same explicit compiler
path. `python3.12 sdk-rs/safe-cex-venue-kraken/scripts/utilities-controls.py` requires
one production mutation per each of the ten TOML rows, with named failure and a
restored pass. It serializes Cargo runs; each run captures the complete corpus in
one Node process. At slice readiness run full scoped tests and host/wasm clippy
separately. A gate sheet and sub-agent verdict attach to the exact reviewed SHA;
once accepted for merge-up, freeze the remote branch until released.


`helpers.rs` maps the four remaining module utilities. Their shared-error ABI,
collation profile and nanoid generator boundary are declared once in
[MANIFEST-SPEC](../parity/MANIFEST-SPEC.md#kraken-module-utility-adapters).
The client-ID factory must be created once per SDK runtime/module and shared
across Spot instances. Its caller supplies a fallible CSPRNG; the fixed-size pool
is not a random generator. Pending constructors must retain that lifetime.

`tests/helpers.rs` starts Node once for the complete corpus. Its oracle uses AST
selection of unchanged declarations (no copied function body), actual shared TS
enums, and actual installed nanoid 6.0.1 with a pinned source checksum. It asserts
native enums and error fields, actual comparator results, and two 1320-call ID
schedules with and without entropy failure. Source acceptance witnesses retain
the explicitly chosen error alias/Display, UTF-8 and custom-size differences.
Run the single `--test helpers --jobs 1` target while porting, then
`scripts/helpers-controls.py` with the explicit TypeScript compiler environment.
The structural gate selects implemented Kraken-owned rows mechanically, excluding
pending methods and the inherited shared-crypto row. The TS source census still
checks the full manifests of all seven selected files against the maintained
extractor, including the untouched pending rows in those files.

The scoped index wrapper retains committed `[file].rust_files` declarations,
filtering only other-crate paths. `scripts/manifest-controls.py` removes a real
Kraken declaration, requires the maintained source-provenance gate to fail, then
restores original bytes and requires a pass. Never derive declarations from rs
targets: that would make the proof true by construction.


`reads.rs` implements five read method bodies at the
[parsed read boundary](../parity/MANIFEST-SPEC.md#kraken-parsed-read-boundary).
It uses shared Store read views only; no notifying write is discarded. Boxed read
errors propagate unchanged. Pending runtime assembly must drive the returned
futures and provide already-parsed Axios data; API factories and signing are not
implemented by this slice. `tests/reads.rs` runs all actual-source cases in one
Node process, with native query, response, position and error-identity assertions.
Run its single `--test reads --jobs 1` target during development, and
`scripts/reads-controls.py --supplemental` for the three row mutations, symmetric
query-field rename and six constant-position numeric-field mutations.

Integration `9af82bfeaefa3496cd8eea0a29b9ce7d3c1741a7` includes structural
subscriber identity in shared ExchangeErrorMetadata. The read-error witness
passes distinct inner/outer IDs through both adapters and asserts their order.
The existing Box is the generic callback error ABI, not a notification sidecar:
it retains the caller's original error identity and also admits numeric-ingress
errors. These five bodies contain no Store write or StoreMutationError conversion.

Pager ownership: Bitget owns the one shared `paginateIncomeRange` operation.
The Kraken source has one import at `kraken.exchange.ts:44` and one direct call
at `:886`; the public wrapper calls the range method at `:914`. Both dependent
income rows remain pending with FINDING notes, empty layer and empty Rust targets.
The scoped status gate includes these two pending FINDINGs instead of hiding them.
No venue-local pager exists. The shared manifest already has six pending rows at
integration `710e663156c8` (four exports plus two private functions), and its source
regression corpus remains unchanged. The experimental public-wrapper body and its
8 proven controls remain in local commit `cfe6aefa095044f04a2bea55ba1973ae9dc78f7b`
for reuse after the shared pager lands; they are not active mapped production code.

See [precision audit](PRECISION_AUDIT.md) before porting the pending order writers.
The current audit found zero production amount-step or price-step reads; this is
not a claim that future formatters resist precision swaps.


The [cardinality audit](CARDINALITY_AUDIT.md) pins independent source corpus and
per-case output counts for all four implemented witness targets. Native collection
lengths are checked before capture; mandatory loops use committed bounds, including
2640 native ID comparisons across two scenarios. The read oracle wrapper adds live
nonempty array-token supplements without editing the original oracle. Run
`scripts/cardinality-controls.py` for production empty/subset and source omission
controls; bypass controls prove the native guard does not depend on source census.

`scripts/consuming-controls.py` separately probes the shared array decoder through
Kraken's live corpora. It consumes discarded elements before returning None,
requires named native-length failures, then removes both outer length checks while
keeping the mutant active and requires named fixed-index failures. It restores
shared production and witness bytes; never commit its temporary shared mutation.


`fetch_leverage_settings` preserves the source success/truthiness guards, clear/set
ordering, partial cache effects, and catch/console behavior. Its cache and console
host boundary is declared in [MANIFEST-SPEC](../parity/MANIFEST-SPEC.md#kraken-leverage-cache-host).
The pending constructor must retain the cache; this function does not create a
venue-local Store or pager. The original read oracle remains byte-identical;
`reads-leverage-supplement.cjs` adds 33 actual-source cases in the existing single
Node process. Equal-shaped object/array keys remain distinct in the native witness.
Source totals, per-case output counts, native lengths and 33 completed comparisons
are independently asserted. `scripts/reads-controls.py --start-at
kraken.exchange.KrakenExchange.fetchLeverageSettings --leverage-supplemental`
runs eight production controls and one cache-witness control.
`scripts/consuming-controls.py --leverage-only` consumes all decoded elements, then
repeats with native length guards removed; direct native effects still must fail.
See [leverage evidence](LEVERAGE_EVIDENCE.md) for this slice's measured controls.


`fetch_historical_positions` calls the shared journal operation with symbol scope
false. Its typed input/future boundary is declared in
[MANIFEST-SPEC](../parity/MANIFEST-SPEC.md#kraken-journal-delegation).
`tests/support/history.rs` compares ten cases executing the actual Kraken and base
TypeScript arrows within the existing single Node corpus. The imported journal
engine is a recording dependency on both sides; its algorithm remains shared-owned.
Checks observe native from/to bits, originating exchange identity, quote fallback,
record order/count/allocation and error metadata allocation/subscriber IDs.
A numeric-account source acceptance/Rust refusal pair records the chosen type
boundary. Seven serial production controls in `scripts/history-controls.py` cover
scope, bound identity, empty/subset/extra records, length-guard bypass and error
flattening. This path adds no decoded collection: native record vectors come from
the shared folder and are returned directly. No new consuming-array control is
claimed for the scalar options decoder.


`spot_reads.rs::fetch_ohlcv` ports the Spot candle read at the
[Spot candle host boundary](../parity/MANIFEST-SPEC.md#kraken-spot-candle-host).
It reads shared Store market info, preserves inherited interval identities with
shared TableHit, and uses shared numeric ingress/coercion and Candle output.
The callback receives native query components; pending factory assembly must
retain pair/interval/since semantics. Numeric infinite timestamps are retained;
the explicit numeric-timestamp profile refuses source string/null timestamps.
The oracle supplement executes the unchanged actual source in the existing one
Node process. It pins 55 cases, 93 native records and 558 native field comparisons,
plus two paired timestamp boundary cases. Original oracle files remain unchanged.
`scripts/ohlcv-controls.py` runs 14 venue production mutations, two shared
consuming-decoder controls, and two source omission controls. Guard bypass keeps
native index coverage and completed comparison totals independent of source size.
No market precision adjustment, Store notification or websocket path is added.


`tickers.rs::fetch_tickers` implements the Futures ticker read at the
[Futures ticker host boundary](../parity/MANIFEST-SPEC.md#kraken-futures-ticker-host).
The existing single Node corpus executes the source arrow and real get-key module.
It pins 49 cases, 63 records and 693 direct native numeric comparisons, alongside
request/trace ordering, original error metadata and current Store fallback.
Typed Store pseudo-market exclusion and owned fallback snapshots have paired
source witnesses. `scripts/ticker-controls.py` runs sequential production swaps,
collection controls, trace identity and consuming shared decoder controls.
No notifying Store write or market precision adjustment is added.


`validation.rs` ports both `validateAccount` bodies at the existing parsed read
and shared UTF-8 boundary. Native `ValidationRead` forwards Futures GET without
body and Spot POST with an empty object. Error-message and status precedence,
malformed response handling, account own-key guards and best-effort balance
presence retain the source behavior. A synchronous balance callback failure
enters the outer catch; an asynchronous rejection resolves without a balance.
`tests/validation.rs` observes 76 cases per method in one source process, including
continuation-time balance callback replacement and catch-path TypeError rejection.
Two paired Futures UTF-16 witnesses record the shared representation refusal;
that row is FORCED divergent, while Spot is ported within the existing boundary.
`scripts/validation-controls.py` proves one named production failure per new row.
Measured code SHA, exits and non-claims are committed in `GATE_EVIDENCE.md`.

`futures_candles.rs::fetch_ohlcv` implements the Futures charts read at the
[Futures candle host boundary](../parity/MANIFEST-SPEC.md#kraken-futures-candle-host).
The existing one-process read oracle has 47 cases and 74 source records. It
observes inherited interval lookup, clock use,
request range, sorting, missing-data reporting, catch reporting and emission.
Both error callbacks remain fallible and preserve original metadata on failure.
NaN timestamps are refused per row with a visible row/field/value diagnostic;
good rows survive and sort normally, without claiming NaN comparator ordering.
The corpus observes 73 admitted native rows and 438 numeric fields plus the
refused row. The constructed `exchange_binding` regression covers mixed,
multiple-bad and all-bad batches against unchanged maintained-source capture.
`scripts/rest-read-controls.py` proves one production mutation per new read
with a named native assertion failure and a restored pass.
No Store write, timer, or two-axis precision adjustment is introduced.


`spot_balance.rs::fetch_balance` ports the Spot balance read at the
[Spot balance host boundary](../parity/MANIFEST-SPEC.md#kraken-spot-balance-host).
`tests/spot_balance.rs` executes the unchanged source arrow in one Node process
and asserts native balances, alias overwrites, currency order, continuation-time
tickers, Store fallback timing and emitter metadata. The 29-case corpus compares
49 currencies and includes a paired UTF-16 source acceptance/native refusal.
Porting gates are the two named read tests, scoped all-targets check and maintained
extractor writer/check; broad control matrices are unnecessary for this slice.


`ws_helpers.rs` ports the public websocket's two pure utilities individually.
`get_ticker_product_ids` captures shared Store market symbols before the fallible
active-symbol callback, preserves duplicate/order semantics and retains the full
list when the active set is empty or has no overlap. `map_timeframe` preserves
its complete twelve-entry Futures table, nullish default and stock-prototype
identities using shared TableHit; it accepts UTF-16 keys including lone surrogates.
`tests/ws_helpers.rs` selects unchanged source members by AST, observing nine
product cases (27 native IDs) and 32 timeframe cases (20 numbers and 12 inherited
identities) in one Node process. `scripts/ws-helper-controls.py` proves one named
production failure per new row. Socket lifecycle methods remain pending.


The concrete Spot API host binds the source's per-error signature retry condition,
full-client `request` resend, and form replay. The typed Spot venue emits flat
read/order form fields; its native parser decodes those fields and the formatter
re-signs them with one fresh nonce. `tests/spot_form_oracle.cjs` runs the actual
qs dependency for six stringify/parse/re-sign cases, including encoded Unicode,
plus signs, repeated fields and prior nonces. This binding does not expose a
public general-purpose qs parser.

`tests/exchange_binding.rs` checks successful and repeated signature rejections
under HTTP 200 and 401, including complete unsigned order fields, fresh signatures
and the two-request ceiling. `scripts/spot-api-binding-controls.py` independently
mutates each new native API-host method, requires a named runtime failure, restores
exact bytes and requires the test to pass again. Shared retry-config preservation
is covered by `safe-cex-exchange/tests/js_runtime.rs` and the same native E2E.

Native hosted accounts enter through the shared Js HTTP host, which injects the
signer into the live parent options. Private challenge auth must read that live
`parent.options.remoteSigner.accountName`, not the original typed constructor
options. The native registry HTTP and WS witnesses and compiling controls live
in `safe-cex-registry/tests/native_host_kraken_signer.rs` and
`safe-cex-registry/scripts/kraken-hosted-controls.py`.


`KrakenBaseUrl` is a string-only decoder for the two source literals. Its private
remote Serde schema retains lowercase serialization; `String::deserialize` guards
all admission paths before variant decoding. `tests/accepted_base_url.rs` checks
both native variants before serialization and refuses each valid tagged-null map
through owned, borrowed, qualified, JSON-text and shared numeric-ingress routes.
Run `scripts/base-url-string-control.py` with the explicit compiler environment
for a compiled guard-removal failure and exact-source restored six-test pass.
`node sdk-rs/safe-cex-venue-kraken/scripts/base-url-contracts.mjs` from repository
root inventories and executes only this native/default production owner, including
an independent native-variant observation. This selected probe leaves the 25
whole-crate receiver refusals and all other owners/aliases/locals/construction
open; it is not a whole-venue accepted-set pass. The compiler check also requires
the private remote companion to have no independent Deserialize or public API.

`tests/accepted_helpers.rs` directly observes all four module-helper APIs for
accepted-contract review. The trigger decoder matches actual native enum variants,
absence and coercion errors before serialization. Comparator operands, literal
error construction/metadata snapshots and the entropy-backed ID generator retain
three separate non-decoder roles; their classification receipts are not execution.
Run the new native target together with `--test helpers` (one live TS process),
then `scripts/helper-contract-controls.py` with the explicit TypeScript path.
Its three role controls must fail named compiled native witnesses; the trigger
control must fail its own actual production output probe. Every exact source
restoration must pass. `scripts/helper-contracts.mjs` selects only these four
native/default owners. Other owners, the 25 receiver refusals, aliases, local
functions, construction and feature/wasm accepted behavior remain independent.
The host profiles are already defined in MANIFEST-SPEC#kraken-module-utility-adapters.


`tests/accepted_scalar_helpers.rs` observes six distinct fills/income scalar
decoders: finite numbers, two presence-sensitive optional-number functions,
seconds and millisecond date functions, and the raw-field position-size-leg
predicate. Compare native f64 bits, Option presence, Boolean values and coercion
error names before Serialize. Date fixtures retain the explicit local-time host
boundary and pre-epoch flooring. The selected production gate
`scripts/scalar-helper-contracts.mjs` runs all six actual APIs against independent
cases; it is not a classification-only receipt. Run the native fixture with
`--test utilities`, then the serial `scripts/scalar-helper-contract-controls.py`.
Each control must fail its named compiled native witness and only its own
production output probe; every exact restoration must pass the named native
witness and all six production probes. Other owners, aliases and all 25 receiver
refusals remain independent, under MANIFEST-SPEC#kraken-utility-wire-domain.

The four private request records in `futures_candles`, `spot_reads` and `reads`
use their concrete Deserialize boundary with the existing shared numeric backend
and `strict_decode::object`, followed by private Serde remote companions. Preserve
Serialize, wire renames, declaration order, optional/null rules, ignored fields
and every numeric-output registration. The companions return the original owner;
they must not gain independent Deserialize/conversion traits or new inherent APIs.

Their cfg(test) native witnesses use `tests/support/accepted_record_shapes.rs` to
observe every field, Option presence and f64 bits through seven ordinary,
qualified and shared numeric routes, then reject complete valid declaration-order
arrays. Run `cargo test -p safe-cex-venue-kraken --lib accepted_shape --jobs 1` from
sdk-rs, then `scripts/private-record-shape-controls.py` serially. Each removal of
only the object wrapper must fail its exact named full-array admission assertion;
all four tests must pass after each exact restoration. The production gate
`scripts/private-record-contracts.mjs` inventories the four actual owners and
separately inspects complete compiler companion implementations. New traits,
conversions or inherent APIs require review; private companion syntax alone is
not proof. Other owners, aliases and receiver refusals remain independent.

REST signing nonces use the shared `VIRTUAL_CLOCK.get_current_time_ms()` for
both Futures and Spot. The shared JS virtualClock facade reads that same owner;
Date.now and transport timing keep their runtime clocks. Futures cliOrdId uses
the source utils/uuid separator removal, independently of Spot's existing
client-ID generator. `tests/native_clock.rs` checks actual signed Futures/Spot
requests under a pushed server offset and a different host wall clock;
`tests/native_identity.rs` checks both nanoid separators in the native cliOrdId.
The shared Bitunix native audit control runner checks both regressions.


Unused `construction::futures_constructor` / `spot_constructor` wrappers and
their generic constructor-only corpus are removed. Production construction stays
on `classes::futures_exchange` / `spot_exchange`, sharing `construct_with` for the
actual API-client assignments. Constructor manifest rows target those live class
factories. `exchange_binding` covers startup plus production private orders,
fills, positions and balance frames with native Store/event assertions.
A compiled xhr-assignment mutation fails the production startup regression;
exact restoration passes. Retained composition tests cover live class factories
and polling rather than the deleted wrappers.

Spot websocket candle dates use shared ToPrimitive before choosing UTF-16 date
parsing or numeric TimeClip, matching the one-argument Date constructor. The
constructed candle binding test compares eleven raw JSON input shapes to the
maintained handleOHLC body, including array dates, invalid dates and fractional
milliseconds; output timestamps and all price/volume fields are checked natively.

The concrete book host uses the shared Js iterator after the source falsy-to-empty
fallback. String sides are iterable: Futures/Spot snapshots skip their NaN quantities
and emit an empty side, while Spot deltas retain a NaN level. The constructed
public-book regression compares those actual source cases through real subscriptions
and raw socket frames, retaining native price/amount bits before JSON projection.

The constructed ticker corpus in `tests/exchange_binding.rs` covers 224 Futures cases / 672 frames and40 Spot three-row frames, including every numeric get-key alias and distinct fallback values. Futures uses fractional relative funding, while Spot last also updates mark/index.
It drives the production constructor/socket protocol and actual DefaultStore,
checking native number bits against maintained TS classes and DefaultStore.
Independent null, string, NaN-producing, object, overflow, signed-zero and false
inputs accompany distinct good rows, with finite Store resets before every case.
The shared `ticker-native-controls.py` compiles live field-publication faults,
requires named native failures, restores exact bytes and requires passing tests.

Futures ticker and order reads capture native response data before coercion. Their
raw info retains own Undefined, unpaired UTF-16 and nonfinite number bits; the
constructed REST regression observes all of these before any JSON output.

Configured REST ticker and order numeric fields use canonical retained parseFloat
before any UTF-8 String admission. The actual-source and HTTP binding regression
`retained_numeric_prefixes_match_source_before_utf8_string_admission` preserves
a numeric prefix followed by a lone surrogate; independent compiled ticker and
order controls fail before their respective cutovers. Symbol and ID Strings keep
their explicit final shared output admission.

The retained input admits lone UTF-16 surrogates. The read/utility boundary
witnesses inspect those units before invoking the existing consumers: token and
leverage wire conversion, fill/income model conversion, and scalar String
admission still refuse their closed representations. Leverage reports that
refusal once before any cache mutation. Depth and numeric-query refusals remain
separate; all twelve utility source boundary cases are retained.

### REST read receiver coverage

`READ_RECEIVER_REVIEW.md` documents the first seven concrete `ReadHost` receiver
classifications. `tests/support/accepted_read_receivers.rs` reaches only the
actual Scope of a constructed `KrakenExchange`, reusing terminal transport
fixtures. The inherited-key regression follows live `utils/get-key.ts` through
shared `in_operator`; do not restore an own-key-only lookup.
`scripts/read-receiver-controls.py` requires seven named compiled runtime failures
and exact-source restoration. Shared Store array identity is separately owned;
these receiver tests claim only the facade's current slot/value behavior.

### Native diagnostic receiver boundary

`NATIVE_DIAGNOSTIC_REVIEW.md` records the polling/trace native Error cutover.
Keep original error values and console argument counts through `console_values`;
terminal rendering belongs to the host. The actual constructed-Scope witnesses
live in `tests/support/accepted_diagnostic_receivers.rs`; their two original-body
regression controls are in `scripts/diagnostic-receiver-controls.py`. Refresh
reviewed impl/file pins whenever those enclosing bodies change.

### Trading error receiver members

`TRADING_ERROR_RECEIVER_REVIEW.md` covers the three concrete ErrorHost methods.
Resolve the native case member once and invoke it with its original receiver;
do not restore the String-only guard or stringify the returned cell. The native
constructed-Scope witnesses and compiled controls are named
`accepted_trading_error_receivers` and `trading-error-receiver-controls.py`.
`trading-error-receiver-source.cjs` executes the live TypeScript callers directly.

### Cancellation, account and class receiver coverage

`ADDITIONAL_RECEIVER_REVIEW.md` records eight concrete Scope receivers and the
Spot cancellation native-error cutover. `accepted_additional_receivers` drives the
production constructor, signed client, Base reporter and socket prototypes.
Keep original `Thrown`/message cells until the post-report fresh Error ToString;
never reintroduce an intermediate typed UTF-8 error. The existing fourteen-case
Spot cancellation corpus and nine independent compiled controls remain scoped
to these paths. Module-array and shared Store identity are separately owned.

### Trading protocol and tick receiver coverage

`RUNTIME_RECEIVER_REVIEW.md` covers TradingHost iterable/send and TickHost's typed
error emission. Spread must use the native iterator protocol, retaining one
method lookup and the original values/errors through shared IteratorRecord.
`tests/support/accepted_runtime_receivers.rs` reaches the constructed Scope;
`runtime-receiver-source.cjs` executes the live spread/tick source and
`runtime-receiver-controls.py` compiles one semantic fault per receiver, requiring
named failures and exact restored passes. Keep limited/unlimited client selection
observable through the real signed HTTP pipeline.

### API retry receiver coverage

`API_RETRY_RECEIVER_REVIEW.md` records the native read/signature retry predicates.
Preserve source short-circuit/property access order and strict native code checks;
reuse the shared transport deny-list data without a second String conversion.
`accepted_api_receivers` reaches actual constructed Scope callbacks; the source
script executes the live utility and exact Spot retry arrow. The three serial
`api-receiver-controls.py` faults must fail named compiled witnesses and restore
exactly. Array.join UTF16 behavior is shared-owned and repaired separately in #1037.

### Public async wrapper timing

The shared `invoke`/`invoke_unit` helpers must invoke the assembled member before
returning a Rust waiter. That member already starts a canonical eager Promise;
keep its existing runtime-owned continuation and do not wrap it in another spawn.
`EAGER_WRAPPER_REVIEW.md` describes the real HTTP/drop/rejection regressions and
live-source witness. Pumping physical Axios interceptor jobs is separate from
polling the returned SDK waiter.


The three `rest_account` aliases have finite profiles at the actual constructed
`binding::rt::Rt` host (and explicit `bool` payload for ReadResult), with native
Serde/conversion detectors and independent invariant expected types. Run
`RUSTC_BOOTSTRAP=1 node safe-cex-venue-kraken/scripts/handle-alias-contracts.mjs`
from `sdk-rs`; discovery stays in its separate accepted-inventory target. The
constructed-Scope tests and three compiled consumer controls are described in
[READ_ALIAS_REVIEW.md](READ_ALIAS_REVIEW.md). These contracts do not classify
other hosts, payloads, following aliases, or runtime method behavior by default.

`PAGE_ALIAS_REVIEW.md` covers the following two income-page aliases at the same
actual Scope host. `page-alias-contracts.mjs` measures their finite native types;
`accepted_page_aliases` drives the real signed account-log client and pager with
held completion, native error identity, wire parameters and native income fields.
`page-alias-controls.py` requires named compiled faults and exact restored passes.
These continuation contracts do not classify other Kraken aliases.

`CONSTRUCTION_ALIAS_REVIEW.md` covers the two finite Factory/RateLimit callback
profiles over the real Scope. `construction-alias-contracts.mjs` measures their
actual compiler identities and decoder traits; `accepted_construction_aliases`
observes signed client bases, native thrown values and source throttle timing.
The two compiled constructor faults in `construction-alias-controls.py` must fail
named witnesses and restore exact bytes before readiness.

`POLLING_ALIAS_REVIEW.md` covers the four finite OHLCV continuation capabilities.
The `polling-alias-contracts.mjs` native probes authenticate actual compiler
identities; `accepted_polling_aliases` uses the constructed Scope, real chart
client and timers. `polling-alias-controls.py` requires four independently named
compiled runtime failures and exact-source restored passes.

`MEMBER_ALIAS_REVIEW.md` covers three finite class dispatch/continuation aliases.
`member-alias-contracts.mjs` authenticates their compiler identities; the native
`accepted_member_aliases` witnesses use constructed receivers, real chart HTTP
and the initialized arrow. `member-alias-controls.py` must fail three named
compiled runtime assertions and restore exact source bytes before readiness.

`binding::rt::SpotIds` is the module-owned mutable generator capability, not a
wire decoder. `CLIENT_ID_ALIAS_REVIEW.md` records its source lifetime and exact
native proof. `--lib accepted_client_id_alias` uses the real generator shared
by two constructed Spot clients, signed AddOrder requests and entropy refusal;
`scripts/client-id-alias-controls.py` requires named compiled semantic failures
and exact restored passes. `scripts/client-id-alias-contracts.mjs` uses fresh
compiler-authenticated identity/Serde/conversion probes for this one alias.

`LOOP_SOCKET_ALIAS_REVIEW.md` covers the concrete socket event and periodic tick
callback capabilities. `loop-socket-alias-contracts.mjs` runs authenticated native
alias probes; `accepted_loop_socket_aliases` drives the actual socket/Store and
signed timer/HTTP paths. Three named compiled consumer faults and exact restored
passes are required by `loop-socket-alias-controls.py`.


Historical fill pages in the constructed ReadHost map directly over the canonical
Js graph, using the shared map/filter and iterator operations. Only fields read
by kraken.fills.ts are reached; never serialize or capture the whole row to
produce a utility DTO first. Unrelated surrogates, nonfinite numbers and deep
properties must not erase a page. Native IDs retain their exact units until the
existing final Execution projection. tests/fill-ingress-source.cjs executes the
maintained mapper for twelve wire cases; exchange_binding checks both surviving
fills, every numeric bit and signed unwindowed/cursor requests. The constructed
Scope test also checks exact native units and an unread throwing property.
The standalone fills utility's separately documented profile is not broadened
by this production binding fix.

`TOPIC_ALIAS_REVIEW.md` records the UTF-16 collection decoder and physical public
subscription boundary. Keep `Topic` units in canonical retained JSON until wire
escaping; do not reintroduce a lossy String conversion. The `accepted_topic_alias`
native witnesses, live-source script and compiled lossy-consumer control cover
both Futures and Spot. `topic-alias-contracts.mjs` probes only this concrete alias.

`STREAM_ADAPTER_RETIREMENT.md` records why the duplicate typed stream adapters and
three callback aliases were removed. Their old class target strings are native
dispatch identifiers, not Rust calls. Stream manifest rows map to the live member
dispatcher. Preserve constructed book/candle tests and the actual Futures socket
no-op witness; `scripts/stream-binding-controls.py` requires four compiled named
failures and exact-source restored passes.

`BOOK_LEVEL_ALIAS_REVIEW.md` records the concrete Level/Levels decoders. Their
native probes inspect all record field bits before JSON; constructed Futures/Spot
socket witnesses compare both sorted sides to the unchanged live source emitter.
`book-level-alias-contracts.mjs` authenticates exact compiler identities and traits;
`book-level-alias-controls.py` requires two independently compiled consumer faults
and exact restored native passes. Keep NumericChanges and JS callback-array
identity separate; these two records do not classify either obligation.
