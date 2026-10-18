# Breakout Prop DXtrade port

Behavioral reference: [TypeScript venue guidance](../../packages/safe-cex/src/exchanges/breakoutprop/CLAUDE.md).

The first slice provides the constants and seven response aliases from
`breakoutprop.types.ts` (nested records are named Rust structs), plus
`parseDxSessionTimeoutMs` and `toDxOrderType`. The portfolio read formatters are extracted into `portfolio.rs`. Exchange
construction, REST I/O orchestration, writes and polling listeners are now assembled by the production binding described below. There is no native
WebSocket in the source adapter.

Decode response records through `safe_cex_num::numeric_ingress::{js_from_str,
js_from_value, js_from_value_ref}`. Every record has an explicit recursive output
registration. `PortfolioOrder.cash_transactions` is an opaque JSON array:
`decode_cash_transactions` captures via `js_deserialize_passthrough` before
constructing Value, then checks the array shape. String fields retain the shared
UTF-8 boundary declared in the parity specification. Unknown record fields are
not represented; the future REST slice must retain raw responses separately for
`info`, including unknown fields. These codecs cover declared response shapes,
not arbitrary malformed JavaScript objects.

Each response record, including the internal combined-instrument pair, guards
its concrete `Deserialize` entrypoint with `strict_decode::object`. Private Serde
remote companions retain the original field names, order and custom cash-array
decoder for both serialization and decoding. The guard rejects complete positional
arrays; it does not add a second numeric capture or change the existing shared
numeric-ingress pipeline. Decoder mutation controls select the remote schemas so
the public data fields cannot mask a mutation of the actual decoder.

Constants representing source objects retain source entry order as key/value
slices. Open-string consumers must use `safe_cex_num::js_table_lookup`, including
its inherited-member result, rather than assuming absent own keys are undefined.

`tests/oracle.cjs` requires `SAFE_CEX_TYPESCRIPT_PATH`, compiles actual source
constants and helpers, and uses the actual response type AST to generate values.
Its schema walk refuses unsupported syntax and inherited interfaces. Native
observations in `tests/support/native_fields.rs` access decoded fields directly,
independently of serde names. Numeric bits are captured before JSON; NaN alone
is canonicalized and signed zero remains exact. No shared fixture is modified.

Scope checks to this crate, with `--jobs 1`. At slice boundaries run its parity
target, formatting and separate strict host/wasm clippy checks. Mutation evidence
belongs in `GATE_EVIDENCE.md`, not in production claims inferred from compilation.

The default `tests/mutations.py` inventory covers each constants/types manifest
row plus both utility rows. It requires a named runtime assertion failure at
exit 101 and a restored passing run; `--supplemental` retains the original six
cross-cutting controls. Constants compare ordered source entries as well as
values. Run mutation corpora sequentially, never alongside another Cargo process.

## Portfolio read formatters

`portfolio.rs` maps the four source methods `formatBalance`, `formatOpenPositions`,
`formatPositions` and `formatOrders` to named functions. They consume the shared
Store and an already captured `JsNumberInput`; logging and local-date-to-UTC
conversion are host inputs. Explicitly offset RFC3339 timestamps bypass that
local-date adapter through the shared parser. Do not substitute an unconditional
UTC conversion in a host whose local timezone differs.

Portfolio mapping reads only the properties used by each source formatter.
Unused declaration fields and unrelated balance/position/order collections never
validate a response; strict declaration codecs remain separate. Actual HTTP
binding regressions and eighty-two maintained-source cases cover this boundary.
Native position intermediates remain native through balance reductions and symbol
filtering; strict Order/Position projection follows complete source mapping.
Unknown side/type/status strings map to own undefined shared ModelField values,
matching the source lookup tables; JSON omits those fields. Inherited table
results that are functions or objects remain outside the final enum representation.
Mapping finishes before projection; later property errors still report and return
cached rows. Balance never projects side, entry price or contracts it does not return. Missing first portfolios
remain undefined (`None`) for open positions and orders, distinct from `[]`.
Balance catches its resulting reduce error; positions propagates its filter error.

Raw `info` is captured separately using `js_deserialize_passthrough` BEFORE Value
construction. Unknown fields and numeric lexemes remain represented; do not
rebuild info by serializing the typed response. Rust returns owned info and store
fallback snapshots; live source-alias witnesses disclose this shared boundary.

`tests/portfolio_oracle.cjs` executes the actual exchange module and actual
createPositionId utility with constructor-only host stubs. Networking helpers
throw if reached. It does not prove HTTP requests, startup, or polling. Fixtures
use different quantities/notional values, mark/last/open prices, order fill and
remaining values, and stored fallback balances. Native field observers preserve
NaN/signed-zero bits before JSON. Both venue oracles record Node version, Node
major, V8 version and architecture; only NaN is canonicalized.

Closed-enum witnesses observe inherited constructor/toString callables and
__proto__ objects before JSON serialization, then assert Rust refusal. Three
inherited-status-before-missing-leg cases protect source fallback precedence;
the targeted inherited-deferred-refusal mutation must fail portfolio_formatOrders.

## Portfolio read wrappers

portfolio_reads.rs extracts the four public portfolio reads over an injected
raw-request factory. Invoke that factory before constructing the async block:
TS calls fetchPortfolioRaw immediately, before suspending. Read stored fallbacks
only after the future settles. Generic raw errors propagate unchanged; do not
route them through the formatter-error conversion callback. Request creation,
authentication and HTTP remain pending in fetchPortfolioRaw, not hidden in this
hook. The shared Store is only read by this production slice.

portfolio_reads_oracle.cjs imports the existing private portfolio source loader
in-process, then executes the actual async source methods. Existing direct oracle
execution retains its output. Scope controls with portfolio_reads_mutations.py;
all subprocesses are sequential, one Node invocation per complete corpus.

The precision-swap audit is in PRECISION_SWAP_AUDIT.md. Current implemented paths
have no paired step consumers; this says nothing about future placeOrder work.
Use distinct fractional step outcomes and native payload assertions for all three
source adjust calls, including stop/trigger price, when that pending row is ported.

The read-wrapper factory call is eager, but a lazy raw future starts work only when
polled. Dropping an unpolled or pending wrapper releases that raw future and stops
formatting; discarding a TS Promise does not. The four read rows declare this
CHOSEN host-driven lifecycle. Detached continuation would require a task runtime
and owned Store lifetime outside these borrowed orchestration functions. The live
abandoned-Promise witness contrasts both drop states with a completed-future
positive control; it does not claim cancellation parity.

## Raw reads and cardinality

`raw_reads.rs` supplies the three raw REST reads over session, account, request,
and error-reporting hooks. Session factory invocation is eager. Account lookup,
GET and filtering happen after the preceding await. Reporter failures propagate
with the original ExchangeError metadata. Concrete auth/API setup remains pending.
The raw portfolio method preserves successful JSON null as Some(null); the four
read wrappers apply shared JS truthiness before choosing a stored fallback.
Twenty composed source cases cover null, false, 0, -0 and empty string through
actual raw and wrapper methods. Do not conflate Option branches in observations.

All four source corpora have a pinned census in tests/support: source totals,
per-case array lengths, identities and expected method inventories. The native
array-length checks are independent of that census, recurse by indexed access
only after equal lengths, and accompany full-value assertions and fixed main
comparison counters. `cardinality_mutations.py` bypasses the source census while
mutating production to empty/subset/extra vectors. The included source-cardinality test
drops whole cases and subcorpora before the census, reusing the same live corpus. Every invocation runs one
Node process for its complete corpus, including supplements. Original portfolio
fixtures stay in portfolio_oracle.cjs; the two-valid-orders supplement is separate.


Formatter errors use shared ExchangeError. The balance and order reporting
callbacks are fallible and `?` propagates their complete original error before
any fallback is returned. Generic read wrappers retain original raw error E;
the separate formatter conversion takes the FULL ExchangeError, including
subscriber IDs and cause metadata. Identity conversion is used by hosts whose
raw error is ExchangeError. Never convert via `.message` in production.
`logger_mutations.py` proves both discarded reporter errors and wrapper metadata
flattening fail the direct/composed actual-source witnesses.

The response-codec census is 13 typed Vec fields plus opaque cashTransactions.
`consuming_decode_mutations.py` consumes every element before returning empty.
Seven finite source profiles also decode directly through serde before the three
numeric-ingress paths, so native cardinality assertions are reached without
relying on serde trailing-element checks or numeric-ingress checks. The final
control bypasses source census and native-length checks while keeping the mutant;
full decoded-value equality must still detect the loss. This is independent
coverage, not a claim that the production decoder is defective.

## Combined instrument read

`market_reads::fetch_combined_instruments` calls both raw factories eagerly,
then polls both pending children. Details determine output order; the first
matching instrument wins; unmatched details are omitted. Raw numeric info is
retained via shared passthrough capture, independently of typed validation.
Joining reads only the source symbol properties and retains the complete raw rows.
Declaration codecs do not validate fields before the market mapping catch.

The borrowed future does not detach children: dropping or rejecting it releases
remaining child futures. Actual source Promises continue. If two errors become
ready between polls, native selection is left-first rather than chronological
Promise settlement; the manifest and `combined_host_poll_order_boundary` witness
state this chosen host scheduling boundary. Do not claim detached Promise parity.
The actual-source corpus pins input duplicate order/counts and output counts;
full decoded output equality independently detects empty/subset/extra output.

## Markets and tickers

`quotes.rs` maps fetchMarkets, fetchTickersForSymbols and fetchTickers. Market
mapping uses first USD replacement, first margin tier and separate amount/price
increments. Only source-accessed fields are projected. Mapping property errors
report their cause and return cached markets before inspecting later rows. Raw
info retains unknown fields, signed zero and overflow lexemes.
Ticker mapping reads only symbol/bid/ask; unused type/time fields never validate
a response. Mapping property errors report and return cached tickers before strict scalar
projection. Twenty-two additional source cases and four actual HTTP regressions
cover these paths.
Ticker events absent/null produce source undefined (`None`), while empty events
produce `Some([])`. Session/account/HTTP failures report and return current stored
tickers; reporter errors retain their original ExchangeError. The market combined
request is awaited outside its mapping catch, so its rejection propagates.

The symbol vector is owned across awaits. Source callers may mutate their array
before login completes; quote_symbols_owned_input witnesses this chosen boundary.
Fallbacks are read after awaits and owned: quotes_deferred_owned_fallback covers
both timing and alias differences. Pending and unpolled future drops release
child resources rather than running detached source Promise continuations.

The quotes corpus executes 45 actual source cases in one Node subprocess. It
pins 193 array counts across main cases and supplements, independently compares
native lengths, compares all decoded output fields, and asserts 45 main case
comparisons. Amount and price precision assignments now exist: the older zero-site
precision finding applies only to its named historical revisions.

## Candle read

`candles::fetch_ohlcv` uses eager session, then account, then POST. It forwards
truthy date bounds to a host Date.toISOString adapter; it does not implement that
calendar formatter. Shared js_date_parse handles response timestamps and shared
js_json_stringify handles the prepared payload. Undefined interval mappings and
bounds survive until wire serialization; zero stays zero, NaN becomes null. The
encoded body explicitly carries application/json so the transport does not infer
form encoding. Authentication and provider construction remain separate rows.

The source ignores limit and maps every event. Native fields are observed directly
before JSON. Original request/reporter errors retain complete ExchangeError
metadata. Declared response validation, UTF-8 refusal, owned input options and
borrowed future cancellation are witnessed CHOSEN boundaries. The ISO fixture
adapter proves forwarding and error propagation, not native ISO implementation.
The 28-case main corpus runs in one Node subprocess with three supplements for
NaN bounds, an ignored lone surrogate and discarded Promise continuation.

## Account/auth reads

`account_auth.rs` extracts validateAccount, getAccountCode and invalidateSession.
Session acquisition remains its own pending row. Invalidation borrows only token
and expiry, so it cannot clear the separate in-flight request. Cache lookup and
session factory invocation occur eagerly; an empty cached string is not a hit.
Native account discovery reads only first-user accounts, accountStatus and the
selected account code. Unread metadata and later users never validate a response.
Accounts filter FULL_TRADING and use the first substring match, not equality or
the last match. Both cached values and mutations are observed directly.

Validation logs response.data.message but RETURNS response.data.msg; preserve
that difference. Logger failures propagate original metadata. Logger arguments,
returned error values and options are owned snapshots, with actual callback
mutation and late-option-change witnesses. All candidate counts and identities
are pinned independently. Twelve additional maintained-source profiles cover
missing/poisoned unused fields, array-like users and consumed-field TypeErrors.
Dropping a redundant matching account can leave the
selected result unchanged, so result equality alone is insufficient coverage.

## Production binding

The existing constructor now installs shared js_runtime class functions and
promises, and the Exchange trait dispatches all 16 methods through them. Existing
source ports remain the bodies for session, portfolio, order and polling work.
ProductionHost supplies real API clients, the shared five-RPS scheduler, owned
login/order/loop continuations, and a BaseWebSocket ancestor that never dials.
BaseExchange is shared by Rc with external data providers; constructor and API
oracles retain their original hosts and independent source comparisons.

Create a fresh ProductionHost from BreakoutDependencies for each account. Its
HTTP host supplies terminal I/O. The shared client factory selects transport
and records outcomes; the source tunnel-install point installs its adapter. local_to_utc is
an explicit host policy for offset-less legacy dates. ProductionHost rejects
reuse for a different live account. No source product enablement is changed.

The typed native order-result ABI carries a scalar source orderId as one string,
and an array as its numeric/string scalar entries. Nonscalar IDs and lone UTF-16
surrogates visibly refuse. Source undefined array returns likewise cannot enter
required native Vec outputs. Existing helper snapshot/strict-codec choices still
apply; shared typed conversion captures native Number events before JSON.

Run tests/binding.rs for the production constructor and all 16 trait methods.
Run python3 scripts/binding-controls.py from this crate (or repository root) for
16 independent compiling dispatch failures and restored 14/14 passing tests.
These are dispatch controls, not full JavaScript semantics or live API parity.
Run them alone, never alongside another Cargo command; logs stay outside the repo.


The registry's opt-in Rust path constructs this host with options-aware native
HTTP and the shared gateway factory. Native DXtrade login refuses remoteSigner
until that capability is implemented. Its JS date realm explicitly uses UTC for
offset-less local dates; other hosts must supply their own local_to_utc policy.
The live native HTTP witness is registry/tests/native_host_breakoutprop.rs.

Four real HTTP binding regressions cover malformed margin/max-size mapping and
unused declaration fields. The combined and quote source comparisons accept the
source's omitted unused fields; failures outside the mapping catch still reject
and reporter failures retain their metadata.

The production UUID host removes both nanoid separators before the order path
adds its `tealstreet-` prefix. The binding regression supplies runtime entropy
containing both separators and observes the actual POST order code.


The native Exchange now forwards book/candle subscriptions to the existing
polling container and returns its original cleanup through the shared single-use
ABI. Owned typed callback values retain the shared native callback boundary;
the source polling algorithm and handler table remain canonical. fetchOHLCV
invokes the existing candle body over the actual account/session/client and the
shared num Date ISO formatter. It preserves both returned rows despite limit=1,
the encoded JSON candle request and original HTTP refusal metadata.
Five `real_trait_` binding witnesses exercise the real polling loop, table cleanup,
REST request, HTTP error and pre-POST invalid-date refusal. The four initial
hook regressions fail the original defaults. Run
`scripts/stream-binding-controls.py` exclusively for compiled dispatch, cleanup,
empty/subset collection and TimeClip faults with exact restored passes. The
runner checks disk space and restricts any required clean to this worktree target.


Authentication is installed in the native client's request interceptor before
constructor rate limiting. It reads the current session after dequeue and on
every network retry; the one-shot 401 recovery remains in DxApi. The three
`auth_pipeline_` production binding regressions fail on the previous early-auth
placement and pass with the native pipeline hook.

Amendment copies retain Order.clientOrderId into internal PreparedOrderOpts.
Both fresh and replacement placement enter the same caller-ID preflight before
market/session work, outside the placement catch. Source updateOrder cancels
first, then unsupported caller IDs (including the empty string) emit one refusal
and prevent replacement HTTP. The binding regression compares absent, own
undefined, nonempty and empty IDs against actual source updateOrder/placeOrder
and BaseExchange guard bodies, preserving valid cancel-then-place controls.

The production binding fixture drains ready promise reactions before advancing
its virtual timer. `tests/binding_scheduling_source.cjs` executes the five actual
BaseExchange rejection bodies and the maintained loop/sleep bodies: rejection
needs no timer, and a complete ready-turn drain includes the loop's zero-delay
callback. Clock/state/handle assertions remain native and unchanged.
