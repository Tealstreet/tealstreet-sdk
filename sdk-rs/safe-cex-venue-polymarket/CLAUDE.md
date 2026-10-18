# Polymarket source mirrors

The shared js_runtime utility table supplies the source Lodash throttle import;
this venue does not maintain a separate timer algorithm.

The concrete `binding::exchange::PolymarketExchange` assembles the existing source
body ports over shared `js_runtime` values, clients, sockets and store. Its native
ethers/V2 builder boundary is compared against the actual maintained TypeScript
signing helper and installed official SDK in 96 cases (all six tick sizes, four
signature types, both sides and both contract families). The SDK's MIT notice is
retained in `SDK_LICENSE.txt`. The production path never launches Node.

`tests/exchange_binding.rs` drives the concrete Exchange methods; run
`tests/binding_controls.py` sequentially after that target passes to measure all
16 entrypoint controls. Those controls must compile, fail their named E2E, then
pass after restoration. Existing method-body oracles and controls remain in use.
Measured concrete transport gates and controls are recorded in
[BINDING_EVIDENCE.md](BINDING_EVIDENCE.md).

The concrete `Scope<Extra>` receiver registrations cover 28 host methods using
exact compiler impl hashes. `binding/exchange.rs::receiver_tests` constructs
the real exchange with NativeRuntime and observes native cells, UTF-16 units,
throw identity, captured callbacks, class allocation and socket effects.
Run `cargo test -p safe-cex-venue-polymarket --lib receiver_tests --jobs 1` from
`sdk-rs`. `scripts/receiver-policy.mjs <fresh-rustdoc-json>` removes all 28 rows
to prove named guard failures, then restores the unchanged production inventory.
These receiver classifications do not claim native coverage of every alias.

The binding uses the shared `safeCexAxiosCreate` factory for proxy selection and
request outcomes. Registry `native_account_factory_routing` and napi
`native-account-factory-routing.test.mjs` exercise that production path.

Behavioral reference: [TypeScript venue guidance](../../packages/safe-cex/src/exchanges/polymarket/CLAUDE.md).

The first slice covers all 13 constants rows in `constants.rs`, plus four
authentication records in `types.rs`: credentials,
derivation request/response, and L2 headers. The declaration and method-body slices below record their narrower guarantees;
the concrete binding above composes their factory, signing, and socket bodies.
Future slices use the landed [shared core](../safe-cex-exchange/CLAUDE.md).

The constants oracle executes the actual constants module, shared enum module,
and logging enum module without replacements. Its one Node process returns all
13 exports, own-key ordering and pre-serialization numeric bits. Tables preserve
own-entry order; future dynamic lookups use the shared `js_table_lookup` so
prototype reads are not silently replaced by own-entry-only lookup. The constants
are declaration mirrors, not a mutable JavaScript object API.

Numeric entry goes through `safe_cex_num::numeric_ingress`, including f64 nonce
and timestamp. All output fields have explicit expression-macro registrations.
The private field decoders use the shared object guard, with no intermediate
Value buffer. Optional passphrase is absent or a string; present null is outside
the TypeScript declaration. The typed-record tests cover declared string/number
inputs, not arbitrary objects or unknown-property retention. All four rows cite
the CHOSEN shared UTF-8 boundary: actual TS retains escaped lone surrogates,
while Rust ingress refuses them. Scalar Unicode, astral pairs and whitespace
remain positive witnesses; silent surrogate replacement must fail the controls.

`tests/auth_types_oracle.cjs` resolves the actual TS interfaces with the compiler
checker (including inherited fields, refusing unresolved parents). It executes actual `deriveApiKey` using a
recording HTTP boundary, and actual `buildL2Headers` with installed ethers. Factory
creation and transport are not exercised; dependencies on those paths throw if
used, including the unexercised official order-builder dependency. Interface declarations are erased at runtime: schema checks are explicitly
separate from these actual source consumer executions. No fixture is shared. A OnceLock caches the complete corpus or its failure, so
all eight tests together launch Node once, including failing mutation runs.

Run from repository root, using `/opt/homebrew/opt/rustup/bin/cargo`:

- `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-polymarket --locked --jobs 1`
- `cargo clippy --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-polymarket --all-targets --locked --jobs 1 -- -D warnings`
- `cargo clippy --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-polymarket --target wasm32-unknown-unknown --lib --locked --jobs 1 -- -D warnings`
- `cargo fmt --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-polymarket --check`
- `python3.14 sdk-rs/safe-cex-venue-polymarket/tests/scoped_parity.py` (indexes only this venue through source symlinks; no exemptions)

Tests and `python3.14 sdk-rs/safe-cex-venue-polymarket/tests/auth_types_controls.py`
require `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
The controls mutate this worktree sequentially, restore on failure, and require
named assertions plus a passing restoration. Do not run other source readers or
gates concurrently with the mutation runner.

`tests/constants_controls.py` performs one production mutation per constants row,
requiring the corresponding `constant:<TS symbol>:` assertion and a passing
restoration. It uses only the `constants` test target, one Node process per corpus
and sequential Cargo invocations with `--jobs 1`. During porting, use `cargo check`
or a single `--test` target; reserve the full package gates above for handoff.
All package scripts must use `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js`;
the Volta shim is not evidence. These Rust/oracle gates do not invoke Yarn.

The next types slice adds `PolymarketBalanceAllowance` and
`PolymarketOrderBookLevel`. These are wire records with two required string
fields each, not prediction-core balances, normalized book levels or payoff
models. Both use the same strict object decoder and exhaustive numeric-output
registration as the auth records. The shared CHOSEN UTF-8 boundary is explicit.

`wire_payload_records_actual_typescript` extends the existing single Node
capture, executes actual `getBalanceAllowance` and `getOrderBook` against the
recording HTTP seam, resolves both actual interfaces through the compiler, and
checks native fields independently from serialized output. Its 36 cases cover
each field separately: 28 accepted cases and eight escaped-surrogate refusals.
Blank/nonnumeric strings and exact decimal spelling remain strings. Prediction-core
owns numeric book parsing, liquidity, bounded payoff and risk calculations.
Each new row has a symmetric production serde-label swap requiring a named
native assertion at exit 101 and a restored pass.

Every oracle capture now includes `capture.architecture` and `capture.nodeMajor`
from the executing Node process. Preserve these with captured data; architecture
can change NaN sign bits. Both control runners accept `POLYMARKET_CONTROL_LOG_DIR`
to retain full failure/restoration logs for sub-agent review.

The local price slice adds `PolymarketPrice`, preserving both wire strings and
optional Number timestamp. `price_record_actual_typescript` executes actual
`getPrices`, checks compiler-resolved schema and native fields, covers 18 string
cases and nine native numeric cases through four shared ingress routes. Present
null is refused while omission remains None. `tests/price_controls.py` proves
a symmetric string-field swap, optional-null collapse and signed-zero loss;
only one scoped test target runs, with a single Node corpus capture. Full scoped
readiness gates must be measured before this slice is offered for merge.

`PolymarketOrderBook` composes the existing wire level record. Both sides are
required ordered arrays; timestamp/hash are optional, with present null refused.
The checker loads its actual lib.es5 Array declarations to resolve element types
and recursively validates source records. `order_book_record_actual_typescript`
executes actual getOrderBook; no Rust REST method is claimed. Its 54 string cases
cover all six top-level/nested string paths (42 accepted, 12 surrogate refusals).
Nine timestamp cases use four shared numeric ingress routes. `book_controls.py`
checks bid/ask serde swaps, reversed bids, truncated asks, optional-null collapse,
signed-zero loss and actual TS array-element drift. Wire records perform no
probability normalization, liquidity, payoff or price/amount precision formatting.

`PolymarketOrderResponse` has a required boolean success and six optional
fields. Present null is refused; an omitted hash array differs from an empty
array. The actual batch `postOrders` method supplies response witnesses through
a recording HTTP seam; no Rust writes or real signed order execution are claimed.
Native assertions cover every field and hash array identity, independent of wire
roundtrips. All six string paths carry 54 boundary cases. The checker now resolves
full boolean unions and string arrays while refusing narrowed boolean literals.

See `ZIP_CARDINALITY_AUDIT.md` for the field/site audit and baseline survivors.
Source totals and performed-comparison counts supplement independent native
length checks. `tests/cardinality_controls.py` covers empty/subset production
and source collections plus source-guard bypass controls. Oracle fixtures are
byte-identical across that repair; new slices must update explicit census
assertions deliberately when adding source cases.


`PolymarketClobMarket` mirrors five wire strings and two required boolean flags.
The actual `getMarkets` consumer supplies its one-process corpus. The type does
not normalize probabilities or derive precision: minimum_order_size and
minimum_tick_size remain distinct verbatim strings; prediction-core owns domain
mapping. `clob_market_record_actual_typescript` pins 2 sample rows, 45 string
cases (35 accepted), all 4 boolean pairs and 41 performed native record checks.
`tests/clob_market_controls.py` checks each field through four production swaps,
consuming-empty/subset shared Array mutations, a two-length-guard bypass failing
the independent sample comparison count, source omissions and actual TS drift.
No shared decoder mutation remains after the controls.


The utility slice adds normalizeTickSize and parseGammaMarketTokens only.
`tests/utilities_oracle.cjs` executes the exact AST-selected source declarations
and supported-tick Set in one Node invocation, recording architecture and Node
major. It does not instantiate the exchange or replace function bodies.
Tick strings are not normalized; Number formatting uses safe-cex-num. Gamma
pairing is a wire parser, not prediction-core outcome/probability/payoff logic.
Its private string-only JSON visitor consumes ignored containers without
materializing nested trees. Lone-surrogate array strings cause an explicit
whole-input refusal, matching the documented chosen UTF-8 boundary.
The utility target pins 41 tick cases / 99 native comparisons and 22 token cases /
19 source outputs / 15 native pairs. `tests/utilities_controls.py` includes a
consuming string-deserializer fault and removal of both length guards; the
independent performed-pair count must still fail. The utility slice does not map REST methods.


`precisionFromTickSize` now uses shared Number coercion and exponent formatting.
Below 1e21 it observes only the trailing-zero count of fixed-eight text. For an
exact binary halfway value, x=(2k+1)/(2*10^8) requires 2k+1=5^8*m with m odd;
therefore k ends in 2 or 7. Half-even and half-up differ only between nonzero
terminal digits, never changing that count. This local count projection is not
an implementation of generic toFixed. The original exponent-string behavior is
retained, including trailing-zero removal on exponent digits.
The same single-process utility oracle adds 3565 precision cases / 14159 native
comparisons, including 3000 halfway/adjacent cases and 512 deterministic finite
bit patterns. The six precision controls cover four production branches and
source cardinality with an independent performed-comparison guard.

The first REST body is `public_reads::get_server_time`, over an injected shared
HttpClient. This scalar slice alone does not assemble factory, constructor,
authentication, retry, or interceptors. This boundary accepts encoded JSON numbers and current native
numbers through shared numeric ingress, never guessing that transformed strings
should be parsed again. Non-number current data is an explicit CHOSEN refusal
(the actual TS getter returns it unchanged). HTTP failures retain the entire
shared RestFailure; decode refusal retains its response and method request.
The public-reads oracle extracts and executes the original method and ENDPOINTS
declaration once per corpus. Three body provenances cover each of 20 cases;
42 bit comparisons,18 refusals and three failure metadata comparisons are fixed
independently of capture length. This scalar slice introduces no collection
decoder or two-axis precision field.


Four further public REST bodies map `getMarkets`, `getOrderBook`, `getOrderBooks`
and `getPrices` through the same injected HttpClient and current-data decoder.
They preserve source endpoints, methods, token parameter spelling, comma-join
behavior and ordered batch bodies. This read slice alone does not assemble factory/constructor/interceptors; the
concrete binding above owns that assembly. Strict owned wire records deliberately refuse unchecked shapes and
lone surrogates, drop unknown fields and do not retain JS response aliases.
These methods do not normalize prices or map prediction-core domain objects.

`tests/record_reads_oracle.cjs` extracts the four original methods and ENDPOINTS
declaration once. The fixed floor stops at method 4 / iteration 9: 36 cases,
69 accepted provenance comparisons, 37 refusals, 102 native row checks and 135
native level checks, plus four full HTTP failure comparisons. Numeric bits are
captured before JSON serialization. All native response checks precede posted
request bytes. A lone-surrogate market response is tested only as encoded JSON;
Rust Value cannot represent the actual JS UTF-16 string. Other cases traverse
encoded, native untransformed and native transformed bodies.
`tests/record_reads_controls.py` supplies one native-value fault per new row:
market size/tick identity, book side identity, consumed-empty batch output and
price token/price identity. Its independent four-control floor ends at
`prices-token-price-swap`; every fault requires exit 101 at its named assertion
and an exit-0 restoration. Request and failure checks are in the oracle target;
this slice does not claim a control for every assertion.

Measured rows, exact gate SHAs and the four native fault/restoration results are
recorded in [REST_RECORD_READS_EVIDENCE.md](REST_RECORD_READS_EVIDENCE.md).

The spot contract slice mirrors six stateless PolymarketExchange bodies in
`spot_contracts.rs`: the name getter, OneWay mode, max leverage 1 and the three
unsupported leverage/position-mode/margin-mode warning overrides. Ignored
inputs stay ignored. Warning calls use an injected synchronous fallible logger
that can bind directly to shared BaseExchange::log; an original logger error is
returned unchanged rather than swallowed. No constructor, lifecycle, store,
HTTP or socket implementation is implied by these free method bodies.

`tests/spot_contracts_oracle.cjs` executes the six exact original members and
actual shared PositionMode/LogSeverity enum modules in one Node process. The
fixed floor stops at method 6 / input iteration 5 (indices 0–4): 30 native
returns, 15 native warning calls and three original logger rejection identities.
Numeric inputs/outputs are captured as binary64 bits before JSON serialization.
The Rust target compares native enum variants, numeric bits, warning text and
severity, plus opaque error identity and fields, without response roundtrips.
`tests/spot_contracts_controls.py` supplies one native-value fault per newly
ported row plus one eager-prefix control, seven controls ending at eager-prefix,
with named failures and restored passes. These rows have no identified behavioral
divergence and are mapped as ported; receiver projection names exact Rust targets.

Warning calls run synchronously before a Ready future is returned. The initial
await-only oracle could not distinguish lazy Rust async logging from the source
eager prefix; `warning_calls_precede_future_poll` now checks all three native
call sites before polling. The source capture records immediate call counts
before awaiting. A seventh control reinstalls deferred setLeverage logging and
must fail `spot_warning_native_eager:setLeverage`, then restore to a pass. The
control floor is seven, ending at eager-prefix. Native future-drop scheduling
beyond this body is not claimed.

The imported ethers `decodeBase64` native binding reuses the shared Node Buffer
decoder already used for Kraken WS auth. Supplied L2 secrets retain source
acceptance of noncanonical trailing bits, ignored characters and incomplete
quartets. `exchange_binding` compares actual signed balance requests for eight
secrets against the maintained TS helper with real ethers and independent Node
crypto. This measures the Node dependency profile, not a new browser-profile claim.
Private order frames assert Store orders and matched fills; trade frames assert
fill delivery and authoritative position refresh. The source has no private
balance topic, so those frames retain the REST account balance until its poll.

Private-read binding snapshot checks preserve every serialized store field except
request outcome counters, whose exact deltas are checked independently against
the maintained `safeCexAxiosCreate` interceptor running with installed Axios.
Each successful cursor page increments `ok`; a rejected HTTP 503 increments
`other`. These telemetry updates do not commit positions, orders, or balances.

The constructed ticker corpus in `tests/exchange_binding.rs` observes 24 cases / 40 price-change and last-trade frames with production token mappings
through the production socket protocol and DefaultStore. Independent null,
string, NaN-producing, object, overflow, signed-zero and false fields accompany
distinct good rows. The maintained TS constructor and actual DefaultStore provide
native numeric-bit expectations. Fresh TS receivers and advancing native clocks
isolate numeric mapping from throttle timing. `ticker-native-controls.py polymarket-store polymarket`
compiles a live publication fault, requires a native assertion failure, then
restores exact source bytes and requires the named test to pass.

Last-trade updates pass `{ id: tokenId }` as the Store selector. Passing the
raw token string silently leaves existing prices unchanged. The constructed
corpus fails with that old binding and passes with the object selector.
