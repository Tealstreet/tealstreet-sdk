# Bitunix and Blofin contract review, first batch

Source base: `49fd1ff259`. This batch adds 70 selected contracts without
changing existing classifications or production Rust. It does not establish
the complete package accepted set, feature matrix, local-owner coverage or
WASM coverage.

The Bitunix compiler review selects 66 owners: 61 concrete function APIs and
five wire owners. Independent expectations cover numeric bits and fallbacks,
response and row selection precedence, order flags, endpoints, signature
rejection, hashing, virtual order IDs, four string-only enums and native
pending-position fields. All 66 execute maintained production-native probes;
none uses a classification-only receipt. The selected response Vec alias also
executes the actual decoder and conversion-trait probes.

Three Blofin aliases execute native trait probes under explicit nondecoder
classifications: `LifecycleFuture<'static, bool, String>`,
`PrivateStreamFuture<bool>` and `PrivateStreamStart<bool, String>`. The latter
two add exact bounded specifications to the existing generic-alias machinery.
Compiler-authenticated roots and invariant native type equality preserve the
complete future/result types. The lifecycle alias retains
`ServiceError<String>`. These checks do not claim to execute lifecycle callbacks
or cover other generic arguments and borrow durations.

Validation uses maintained `runProbes` with native/default configuration and
`jobs: 1`, sequentially: 66 owner receipts, one decoder-alias receipt and three
nondecoder-alias receipts pass. Two compiled controls modify only the copied
production source used by that runner:

- Replacing a finite parsed number with its fallback fails the named
  `value_helpers::parse_finite_number` native expectations; restoration passes.
- Removing only `strict_decode::object` from `PendingPosition::deserialize`
  admits the complete valid 20-field positional array and fails the named
  native observation. Its matching full object decodes successfully. Restoring
  the guard passes both assertions. Optional-field omission and present-null
  cases are also observed directly.

Both mutations compile and execute; compiler failures are rejected as control
evidence. Real production source hashes remain unchanged. The contract cases
and exact selected APIs are checked in; generated compiler reports, temporary
workspaces and execution logs are not.

The original review inventory has 199 Bitunix owners plus two aliases and
163 Blofin owners plus 13 aliases. This batch leaves 133 Bitunix owners and
its function-pointer alias, and all 163 Blofin owners plus ten aliases, for
subsequent review. Those counts describe this source snapshot rather than a
passing package ratchet.

## Subscription parser follow-up

Five further native/default contracts execute the concrete OHLCV and order-book
parser owners and their field/float/level helpers. Candle fixtures observe all
six native number fields, negative zero, ticker-before-callback order, absent
tickers and callback errors. Book fixtures inspect both native sides, independent
cumulative amounts, original Rc identity and partial state when asks refuse
after bids have been replaced. Failure callbacks retain the completed book.

All five production-native receipts pass with jobs1. Two independently compiled
copied-production mutations replace candle open with close and accumulate book
prices instead of amounts. Each fails its named owner; exact restoration passes
both owners. These are runtime assertion failures, not compiler failures, and
the real production source bytes are unchanged. This follow-up leaves 128 of
the original Bitunix owners for subsequent review.

## Concrete helper and error-conversion follow-up

Another 47 Bitunix owners execute native/default probes: 46 concrete functions
and the three inventoried conversions on `adapter::Fault`. Their expectations
observe numeric precision and rounding, own-property order and Undefined/null
identity, first/last table precedence, historical UTF-16 selectors, REST field
selection, retry short-circuit order, response-body phases, TP/SL precedence and
native error class/message/response metadata. All 47 owner receipts pass.

Compiled copied-production controls remove inverse-table reverse precedence and
invert account-mode selection. Both fail their named native owner and pass
after exact restoration. Production source bytes remain unchanged. The query
fixture was also checked against the source's actual `qs` dependency: repeated
array keys and null's empty assignment remain observable.

Nine Blofin callback/factory aliases separately pass their actual native
Deserialize/conversion trait probes under specific nondecoder classifications.
Eight use the existing fixed `serde_json::Value` error-parameter callable policy;
`CommonHttpFactory` is concrete. Exact compiler targets and authenticated roots
are retained. These are capability classifications, without callback execution,
other generic arguments or permission for neighboring host methods.

This follow-up has 56 receipts: 47 runtime owners and nine explicit alias
classifications. On the original compiler inventory, 81 Bitunix owners and its
retry function-pointer alias remain, alongside all 163 Blofin owners and its
API-factory function-pointer alias. The existing Future specifications and every
previous contract remain unchanged.

## Historical result mappers

The trade-to-income, closed-position and historical-order functions now have
three further native/default runtime receipts. Expectations inspect canonical
enum variants, all mapped numeric bits, IDs, account/asset/provenance fields,
optional presence and source null/empty guards. The closed-position cases keep
closed quantity distinct from peak quantity, with separately chosen close prices.
The trade cases distinguish commission from rebate and verify PnL-before-fee
ordering. Historical orders retain zero-as-undefined price/time fields.

All three actual production owners pass. Three compiled copied-source controls
invert fee sign, replace notional's close price with quantity, and invert the
market-order test. Each fails its named owner; exact restoration passes all
three. Real production source bytes are unchanged. This leaves 78 original
Bitunix owners, alongside the previously listed pointer and Blofin scope.

## Blofin wire records and concrete helpers

Another 49 Blofin native/default owners execute directly: twelve response/data
records and 37 private numeric, property, error, response and stream helpers.
Record observations inspect every native field individually, including nested
row order, distinct string fields, optional omission versus null, and all three
batch-side enum variants. Complete valid arrays in Serde declaration order
refuse the map-only boundary. These classify the maintained schema; erased TS
types do not validate arbitrary runtime values.

Helper observations distinguish number bits, nullish and truthy selection,
error class/message, first-element selection, stream sequence/level fields and
nonempty book levels. All 49 runtime owner receipts pass with fresh compiler
inventory and jobs1. Compiled copied-production controls drop the parsed levels
and remove only FetchMarginModeResponseData's map guard. Both fail their native
owner; exact restoration passes both. Real production source bytes are unchanged.

Two additional receipts classify the exact Bitunix RetryCondition and Blofin
BlofinApiFactory pointers as nondecoders. The shared reviewed pointer registry
checks their compiler signatures, authenticated roots and invariant native type
equality, plus actual Serde/conversion traits. This makes no callback-execution,
generic, local, feature or WASM claim.

Together with the preceding batch-side repair, 50 of the original 163 Blofin
owners now have these selected native contracts, leaving 113. All thirteen
Blofin aliases and both Bitunix aliases have bounded native classifications.
The remaining 78 Bitunix owners and complete package/matrix gates remain owed.

## Retained inputs, selectors and native state

A further 66 native/default owner contracts cover 17 Bitunix and 49 Blofin
owners. Forty concrete helper functions observe timestamp units, number bits,
property selection, first-match order, query fields, native order overlays and
reference equality. Cursor expectations preserve the maintained source's
length-then-lexical ordering without normalizing leading zeros. Query numbers
are inspected through their native binary64 bits.

Six Blofin retained-input owners inspect actual Value variants, nested payloads,
property order and number bits. Their borrowed accessors execute separately.
The topic owner observes its actual ordered keys and values; three error owners
execute coercion conversions at String host errors. These specific open-JSON
contracts do not grant the downstream mappers an open schema.

Eleven Blofin mapper/store owners observe actual income, closed-position and
historical-order native fields and enum variants, resolver/raw-symbol fallback,
production Store indices, lot-size truthiness and mode callbacks. Five Bitunix
state owners observe retained info, live-ID/cache precedence, pre-format callback
ordering, canonical zero-position defaults and account-mode REST requests.
Native callback futures use bounded deterministic fixtures, without a claim
about real transport scheduling.

All 66 selected native owner receipts pass. Four compiled copied-production
controls change event timestamp units, the strict history threshold, live-ID
truthiness and closed-position absolute fees. Each fails its named owner; exact
restoration passes all four, with real production sources unchanged.

This leaves 61 original Bitunix owners and 64 Blofin owners for subsequent
review. Complete discovery, locals, features and WASM matrix coverage remain
separate obligations; none of the 66 receipts is classification-only.

## Concrete hosts and complete Bitunix discovery

Three concrete Bitunix adapters now execute every inventoried raw-input method
on the real BitunixVenue constructor. PlaceFrame observes case-sensitive matching,
Unicode lowercase and UTF-16 output; BatchFrame observes property/own-key selection,
nonmutating omission and actual POST/response data; CancelFrame observes actual
POSTs and selected native Store order-status changes. The existing scripted
runtime/transport fixture is included directly, with only its heading made
include-compatible. No host receiver receives an exemption.

Seven Blofin helpers observe encoded/native/transformed body provenance, exact
configured REST requests, optional presence and supplied-value String decoding.
Already-native strings remain data. Native helpers execute immediately-ready
adapters through the actual production futures, including their explicit yields.

Complete Bitunix discovery exposed ambiguous macro imports and two nested
decoders. Exact type imports replace the history mapper glob; JSON macro calls
use their canonical binding, while the ordinary adapter JSON function has a
distinct import alias. The recursive query converter and event precision helper
move unchanged to module scope and receive their own native contracts. Their
actual QueryValue variants, nested fields/order, number bits and precision
results are inspected directly. Only the obsolete block-local precision review
is removed; no discovery or classification guard is broadened.

After these structural changes, complete native/default inventory passes with
five Deserialize and 196 other owners. This is discovery, not a complete runtime
or matrix verdict. All twelve selected runtime owners pass after the changes.
Four compiled controls invert concrete message matching, reparse an already-native
body string, erase query number values and replace the nullish precision default.
Each fails its native owner by value or bit observation and passes after exact
restoration. Real production source bytes remain unchanged during controls.

Of the original census, 58 Bitunix and 57 Blofin owners remain. Both newly lifted
helper owners also have explicit native contracts. Features, locals and WASM
matrix runtime coverage remain separate obligations.

## API callbacks, stream dispatch and retained adapter hosts

Another 24 owners have native/default contracts: sixteen Bitunix and eight
Blofin. Every receipt executes production code; none is classification-only.

Bitunix API witnesses observe native factory configuration, retry count,
conditional transport installation and registration order. The registered
request/response callbacks are actually invoked. Signing expectations use
independent double-SHA256 calculations on explicit fixture plaintexts, preserving
already-serialized body spaces, header positions, numeric variants and bits.
Response cases retain original pointer identity and resend/dedupe/header/report
order, including suppressed and falsy resend paths.

Nine public websocket owners retain listener/super disposal order, per-step
short circuits, cooldown timer identity and delay bits, ticker field-read order,
individual number bits, missing-ticker guards, market-filter/map/track phases,
open-time listener/timer order and all message routes. Kline callbacks assert
that both handlers receive the same actual parsed Rc allocation. These are
bounded host callbacks; real scheduling is not inferred.

Concrete FillFrame, Session and Constructor methods use the existing native
venue fixture. Their observations include actual GET requests, warning events,
Store changes, read-result variants and socket tasks. The constructor's real
scripted public socket emits price versus ticker subscriptions under secondary
versus primary selection. The history query helper separately preserves optional
zero values, field order and native number variants/bits. No receiver gains an
exemption from these method witnesses.

Blofin helpers retain emitted value pointer identity, missing/null differences,
Error versus host-failure variants, warning severity and truthy response-message
precedence. Book admission observes the actual borrowed payload and bid bits.
Listener entry construction checks callback Rc identity and actually invokes the
retained callback, alongside native subscription fields and key.

All 24 selected runtime receipts pass with jobs1. Seven compiled copied-production
controls change retry count, cooldown delay, Store market updates, secondary socket
forwarding, query limit, warning text and timestamp admission. Each fails its named
native value/bit observation; exact restoration passes all seven owners. Real
production source bytes remain unchanged throughout the controls.

These rows leave 42 original
Bitunix owners and 49 Blofin owners for subsequent review. Complete-package
runtime, local-owner and feature/WASM matrix gates remain separate obligations.

## REST and historical-read orchestration

Six more Bitunix owners execute native/default contracts with bounded immediately
ready futures and explicit callbacks. OHLCV observes request order, date callback
inputs, optional limit presence and complete ordered Candle fields as native
number bits, including signed zero. Its two distinct candles also detect partial
output loss. Account validation retains strict numeric-code admission, original
error variants, account-before-balance order and balance-failure presence. Symbol
metadata retains set-before-read order, margin/leverage presence, matching-only
overlays and null-response short circuits.

The three history wrappers observe the actual selector function pointers, native
UTF-16 ID and timestamp results, exact endpoint/options, public-only short circuits
and propagated errors. Income preserves one-to-many mapping and empty results;
closed PnL reads each account value before mapping, including a skipped row.
Complete native result vectors retain descending order and number bits. These
bounded callbacks do not replace the separately mapped pagination, REST, Store
or mapper implementations.

All six native owners pass. Five compiled copied-production mutations alter a
candle field, strict validation code, margin mode, history selector precedence
and descending sorting: all six observations fail and all restored owners pass.
An additional keep-first candle mutation fails the full two-row observation,
then passes after restoration. Real production bytes remain unchanged.

This batch leaves 36 original Bitunix owners and the previously measured
49 Blofin owners. The original Blofin census stopped at receiver drift, so its
empty missing-owner array does not prove runtime completeness. Full-package,
local-owner and feature/WASM matrices remain separate obligations.

## REST read and write hosts

Another six Bitunix owners execute native/default witnesses. The three writes
retain both position-mode literals, complete position preflight before warning,
POST-before-setting order, full UTF-16 symbols including isolated surrogates,
leverage clamp bits and updates to both long/short IDs after successful POST.
Transport, setting, warning, rejection and report failures remain distinct.

Market and ticker reads independently inspect two complete native outputs,
including every declared number bit and optional-field absence. Unknown and
filtered rows remain distinct. Their fallback fixtures return two distinct
cached rows and inspect the full vectors. Balance checks native arithmetic,
account-mode updates, zero-value precedence and the complete two-position
borrowed slice, including pointer identity. Ready callback hosts bound this
profile; arbitrary transport scheduling and executable host objects remain
separate obligations.

All six actual native owners pass, with no classification-only receipts. Eight
compiled copied-production mutations alter the mode literal, UTF-16 symbol,
short-position update ID, market amount precision, ticker ask, frozen-balance
arithmetic, both cached fallback vectors and the returned position slice. Each
fails the intended native observation, then passes after exact restoration.
The two cached-vector and position truncation gaps were found by independent
review and repaired before publication. Production source bytes are unchanged.

After the six earlier REST/history owners, this batch leaves 30 original
Bitunix owners and the previously measured 49 Blofin owners. Complete-package,
local-owner and feature/WASM matrices remain separate obligations.

## Position records and multi-page histories

Three additional native/default owners now execute their actual production
bodies. PositionRecord observations cover every string, enum identity, optional
presence and numeric bit. Long/short and hedged/one-way rows keep closed size
separate from maximum size, rebate separate from cost and the explicit funding
choice. Zero versus null position IDs retain distinct key/fallback behavior.

The general history pager executes a real 100-row first page and short second
page, asserting all 103 admitted native rows. First-ID deduplication, repeated
empty IDs, time bounds, UTF-16 endpoint/symbol units and offset bits remain
distinct. A later fetch failure retains the first 100 collected rows; report
failure propagates. Historical orders separately assert all 100 mapped outputs,
including a skipped mapper row and duplicate ID, descending timestamp order and
fractional limit truncation. Their cursor follows oldest creation time rather
than mapped timestamp; a later fetch failure clears the earlier outputs.

All three owners pass actual native execution. Three compiled copied-production
controls replace closed size with maximum size, offset 100 with 99 and the
initial to cursor with from. Each fails its named output observation and passes
after restoration. Independent review found no issues. Production bytes remain
unchanged. Ready callbacks bound this profile; concrete REST/mappers and the
unexecuted 100-page/50-page warning caps are not inferred from these receipts.

Together with the separate six read/write contracts, this leaves 27 original
Bitunix owners and the previously measured 49 Blofin owners. Complete-package,
local-owner and feature/WASM matrices remain separate obligations.

## Socket callbacks and login signing

Eleven additional Bitunix owners execute native/default witnesses. Debounce and
all three wrappers retain fresh timer rereads before clear, literal field/handler
identity and native 1000ms bits. Three asynchronous reconciliation functions
retain timeout reset before disposal, complete two-row payloads, existing loaded
entries, exact new flags and fetch/loaded-read failure boundaries.

Futures open preserves three subscription frames, both active topics and their
order, fresh timer tokens and the second disposal check before heartbeat setup.
Ping-loop scheduling retains exact 15000ms bits. Message ingress checks count
increment bits, blob success versus report-and-original-data fallback, disposal
after the await, complete sparse handler traversal and the same parsed Rc
allocation delivered to both handlers.

Login expectations independently compute double SHA256 from explicit fixture
plaintext with hashlib. Explicit native timestamp bits preserve signed zero
while signing text remains zero. Native property order, string/numeric remote
signature variants and signer Rc identity remain observable. A truthy remote
error suppresses even a present signature; sync, sleep, sign, report and send
failures retain separate boundaries. Review found the missing precedence case
and it now has an actual condition-removal control.

All eleven native owners pass, with no classification-only receipts. Eight
compiled copied-production edits cover delay values, reconciliation flags,
handler truncation, secret trimming and remote-error precedence. They produce
twelve negative owner receipts (login has two distinct controls); every restored
owner passes. Real production source bytes remain unchanged. Ready callbacks
bound these profiles; real transport, timer ownership, browser Blob behavior,
concrete Store integration and cancellation are not inferred from these probes.

Together with the three position/history contracts, this leaves 16 original
Bitunix owners and the previously measured 49 Blofin owners. Complete-package,
local-owner and feature/WASM matrices remain separate obligations.

## Concrete order formatting and amendment fields

Four order-model helpers execute against the real native OrderFrame, constructed
venue, Store and error listener. Rejection retains original error text, complete
error-event payloads and listener-failure precedence. Create formatting inspects
complete ordered JsJsonValue payloads, distinct fractional amount/price steps,
raw TP/SL prices, market-price omission, both split payloads, reduce-only position
IDs and native TIF/reduce caches. The generated-client-ID path is selected.

TP/SL formatting separately covers partial TP and SL quantities and full-position
updates. Full-position TP retains the raw opposite SL value; cached position ID
precedes live info and prevents copying an unmatched counterpart. Append-leg
observations distinguish trigger, quantity and LIMIT execution price, preserve
untouched opposite-leg snapshots, and skip quantity/execution fields for position
updates. Absent snapshots preserve the original payload. Validation failures
retain the exact partial payload mutation and error-event sequence.

All four actual native owners pass. Nine compiling copied-production mutations
remove rejection emission or independently swap amount/price axes at every
selected create, partial/full TP/SL and append-leg precision site. Each produces
a named output mismatch. The six TP/SL swaps run one at a time, followed by both
owners' complete restored native cases; the rejection/create swaps each have
separate restored runs. Three additional post-construction controls discard all
create payloads, retain only the first of two create payloads, or discard the
single TP/SL payload. They preserve complete input consumption and cache effects;
each fails the named output assertion. The complete four-owner native run passes
after restoration. These twelve compiled controls cover nine rejection/precision
faults and three output-loss faults. Real production source bytes remain unchanged.
Independent review found no issues. These concrete bounded fixtures do not
classify the OrderFrame receiver itself or infer arbitrary caller IDs/hosts.

This leaves 12 original Bitunix owners and the previously measured 49 Blofin
owners. Complete-package, local-owner and feature/WASM matrices remain separate
obligations.

## Position read and concrete amendment orchestration

Three native helpers cover position fetching, standalone TP/SL amendments and
regular/attached amendments. The position probe reads every native Position
field, numeric bits and optional presence. Two live rows distinguish qty from
maxQty and live from cached leverage; complete output contains five ordered
live/default positions. Separate one-way defaults deduplicate to two positions.
Transport failure reports before returning two distinct cached native positions.
Hex inputs reconstruct cached negative-zero fields after JSON transport.

Amendments execute on the real OrderFrame, venue, Store and HTTP transport.
Complete bodies distinguish quantity, main LIMIT price, both TP/SL triggers and
both LIMIT execution prices. Attached amendments retain the parent quantity and
main price, change the selected trigger and suffix the acknowledged ID. Explicit
acknowledgment and fallback IDs, standalone routing, identity and already-filled
quantity refusals remain separate outcomes. Truthy response messages retain both
actual interceptor and amendment-catch error events.

All three actual native owners pass. Fifteen compiling copied-production edits
fail intended output assertions: five position controls cover consuming empty
and partial output loss, contracts value, ID dedup and fallback ordering; ten
amendment controls cover the six independent precision fields, acknowledgment ID,
attached suffix and complete loss of each single-result ID vector after request
and response consumption. Full restored native cases pass. Real production
source remains unchanged. Independent review corrected the cached signed-zero
fixture; no contract findings remain.

This leaves nine original Bitunix owners and the previously measured 49 Blofin
owners. OrderFrame itself, complete-package/local-owner and feature/WASM matrices
remain separate obligations. These finite selected runtime profiles make no
arbitrary callback, source-object or complete lifecycle claim.

## Concrete OrderFrame receiver

All 23 inventoried OrderFrame methods execute through 61 native cases, including
both no-argument Store snapshot methods. The actual constructor, Store, HTTP
lanes and BaseExchange listeners supply the receiver. Declared returned JSON is
inspected recursively with numeric bits; the probe does not serialize a decoded
owner to manufacture its expected result. Snapshots retain complete distinct
positions/orders. The profile observes native cache mutation, caller-ID admission,
missing/null/false presence, identity and trigger/snapshot fields, and standalone
HTTP delegation.

GET/POST cases forward distinct nonempty inputs with false/null fields and
numbers; actual transport params/body and GET custom headers/timeouts are checked.
Independent review found this initial forwarding hole before publication.

Eight compiling copied-production controls produce named output failures:
GET/POST forwarding loss, empty and partial position snapshots, empty and partial
order snapshots, flipped reduce-cache truthiness and lost own-null presence.
Snapshot loss occurs only after constructing the complete native output. All 61
restored native cases pass, and real production source bytes remain unchanged.
These finite concrete profiles do not classify the other venue/socket receivers
or replace the separate shared model, generic helper, full-package/local-owner
and feature/WASM obligations.

## Socket Parent, book constructor and historical fills

Three native owners execute through twenty cases. The concrete socket Parent
uses the real venue, BaseExchange and Store: distinct event names/messages,
severity, error prefixes and suppression metadata retain actual events/reports;
a missing weak owner refuses before forwarding.

The book constructor uses String callbacks and usize parser/timer tokens. All
native returned subscription fields and initially empty book vectors are checked,
with exact live handler/topic mutations and sent frame, or deferred 100 ms wait.
Precision lookup failure occurs before parser allocation. The bounded host refuses
unused parsing/cancellation hooks; concrete Feed and lifecycle behavior stay
separate.

Historical fills retain complete ordered native JsJsonValue fields and number
bits, two surviving rows, newest-first order and replacement of an earlier
duplicate. Distinct symbol/order/side/price filters, millisecond timestamps,
limit clamp/truncation, exact suppression request data and warning failure are
selected. A surviving negative-fee row proves positive commission conversion.

All three actual native owners pass. Eleven compiling copied-production controls
fail intended output assertions: Parent event-name/severity/prefix forwarding,
book handler case/topic count/deferred wait, and fill consuming empty/partial
output loss, sort reversal, lost duplicate replacement and removed commission abs.
Every owner passes its complete restored cases; production source bytes remain
unchanged. Independent review strengthened event-name and surviving-fee fixtures
before publication. Complete-package/local-owner and feature/WASM matrices, full
telemetry/console, concrete socket integration and source-object boundaries retain
their independent obligations.

## Blofin configured reads, services and projections

Twenty specific native owners use 78 independently authored finite cases. The
127 existing Blofin classifications are preserved. No production source, source
corpus, manifest or shared policy changes are part of this batch.

The two account-mode readers distinguish Store-before-cache precedence, explicit
refetch, strict string-zero responses and the different zero-contract position
rules. Leverage reads observe both native sides, mode/margin cells, the committed
Store record and preservation of an unrelated symbol. Symbol cancellation checks
the complete selected native snapshots and numeric bits before the single awaited
delegation. Cancellation requests forward two distinct payload items and complete
native responses; application responses remain outside the transport-only catch.

Ticker and market projections inspect every native field, optional presence and
number bit, plus recursively retained values. Two distinct successful outputs and
two distinct cached fallback outputs prevent partial-loss false greens. The real
shared Store strips truthy ticker info but retains false, matching the maintained
TypeScript Store. Market filtering, lot-size fallback, contract/base-size axes and
source error traces stay explicit.

Position projection and its configured readers use the real Store, modes and
PositionReadHost. Price step 0.25 and amount step 0.01 are distinguished independently
at both notional and unrealized-PnL sites: 4 contracts at mark 5.123 produce 20.5,
and PnL 0.37 produces 0.25. Complete native position fields include raw contracts,
retained info, leverage/liquidation fallback and every optional. The orchestration
profiles exclude a live BTC row when only ETH is requested, retain the first of
two same-ID records with unequal leverage/PnL, select cached positions after auth
failure and construct both metadata-derived zero-position sides. Metadata mode
filtering, disposal, warning and detailed original-error paths execute separately.

Raw fills, active-order pagination and closed-position rows retain complete
ordered multi-element outputs, exact query spreads and cursor/error behavior.
Closed rows use historyId rather than their shared positionId. Executions inspect
all native fields and aliases, native commission/position-side presence, timestamp
units and explicit contract scaling, including a sizing argument unequal to the
market field. Income range reads retain all source PnL,
commission and rebate types even when incomeType is supplied: the maintained source
does not apply that filter. Complete records retain stable time ordering, first
trade-ID deduplication and client-side bounds. PositionRecord mapping separately
checks optional explicit scale, fee sign, history identity, settlement fallback,
callback order and unattributed funding; it assigns no journal grade.

All twenty selected native owners pass. Sixty-one independent compiling mutations of copied
production source fail their intended native output assertions, then all
78 restored cases pass. Native receipts, independent review and these
mutation controls are scoped to these selected default/native owners. Collection
controls consume complete inputs before returning empty or partial outputs.
Fallback-specific partial-loss, both precision swaps, symbol filtering,
first-record retention and outgoing cancellation-batch truncation have their own
controls. Restored complete native cases pass with real source bytes unchanged.
Complete-package/local-owner, feature/WASM, long-page bounds, arbitrary callbacks,
source-object aliases and encoded-ingress obligations remain separate.
