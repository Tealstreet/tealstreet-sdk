# Accepted-input contract review

The first batch adds 52 individually reviewed owners to
[the GRVT contract file](../safe-cex-parity/accepted-set/contracts/safe-cex-venue-grvt.json):
25 object decoders, eleven public and eleven private string enums, two free
functions, PublicSubscriptions and two constructor owners. Each contract contains
its own native expectations and exact selected APIs where applicable. This batch
does not complete GRVT: the fresh compiler inventory has 182 owners and nineteen
public aliases, beyond the original census's 158 owners and nineteen aliases.

The 25 record tests independently author two objects and a complete valid
positional array in the reviewed Serde declaration order. They compare every
native field before the selected presence, enum and binary64 observations, then
require rejection through text, owned Value, borrowed Value and qualified text
entrypoints. Removing each owner's concrete object guard separately compiled and
failed that owner's named complete-array rejection; restoring the source exactly
passed all 25 tests. An incorrectly ordered array cannot count as a shape control:
the admitted native fields must first match the object.

The 22 enum contracts observe native variants for all 44 literal tags and reject
tagged objects through the four paths. The eleven private decoder guard-removal
controls and live TypeScript literal comparisons are maintained by the existing
protocol enum tests. Parent filtering does not classify private decoders.

The presence function fixes its payload separately to Value and String; present
null remains Some for opaque values and fails the typed string path. The fee
conversion function compares binary64 bits before serialization, including
negative zero and multiplication overflow. JSON fixture transport normalizes a
numeric negative zero: six affected record contracts therefore separately decode
raw text containing lexical -0.0 and inspect its native bits.

PublicSubscriptions executes seven actual methods with fourteen independently
authored RPC/state expectations using a fixed u32 timer carrier. The error
constructor directly exposes native reason, optional cap, binary64 fields,
message and Display; NumberKey's constructor retains its explicitly typed role.
These claims do not establish arbitrary host objects or generic instantiations.

The five tests in accepted_aliases.rs are additional native evidence, separate
from these 52 contract rows. They exercise AddedMarkets<bool>, JsResult<bool,bool>
and the actual Fields<bool> alias with borrowed static string keys. They retain
source key order and native boolean payloads, distinguish borrowed Value's
decoded escaped keys from text's escaped-key refusal, and demonstrate the owned
Value limitation. The alias gate still requires independently reviewed compiler
identity and production-probe support; these tests do not waive it.

Selected production execution passes for all 52 owners on native and wasm:
51 runtime receipts and one explicit typed classification per target. Their
104 receipts pass the scoped ratchet, with the standing defect ledger unchanged.
These scoped results do not assert ordinary whole-crate source, alias or local
owner clearance. The ordinary package gate must still discover and classify
those remaining owners before the GRVT review is complete.

Run the maintained native tests from the repository root:

```sh
CARGO_INCREMENTAL=0 cargo +1.99.0 test --manifest-path sdk-rs/Cargo.toml \
  -p safe-cex-venue-grvt --test accepted_records --test accepted_aliases
```

The ordinary gate command is:

```sh
RUSTUP_TOOLCHAIN=1.99.0 CARGO_INCREMENTAL=0 node \
  sdk-rs/safe-cex-parity/scripts/accepted-set.mjs --package=safe-cex-venue-grvt
```

Re-run the individually compiled controls from the repository root:

```sh
python3 sdk-rs/safe-cex-venue-grvt/scripts/accepted-record-controls.py
```

No historical debt entry or production decoder is changed by this batch.

## Data helper batch

Eleven additional contracts execute the actual order status, creation-time, TIF,
trigger metadata, nullish field selection, instrument suffix and interval
functions, signing metadata validation, builder info construction, string field
admission and ordered Ethereum-address selection. Each raw API has independently
authored input/output expectations; observations inspect native enum identity,
optional presence, binary64 bits, borrowed-pointer identity and returned fields
before JSON projection. The alias selector distinguishes Some(null) from None.
Signing metadata reads raw JSON text to preserve negative zero. Builder info
retains an explicitly empty address, original fee text and numeric conversion.

All eleven production runtime receipts pass separately on native and wasm. The
maintained actual-source tests for order helpers, order semantics, intervals,
instrument symbols and signing metadata pass all thirteen tests, including named
source mutation controls. These results supplement the first batch's 52 owners;
they leave 119 of the fresh 182 nominal/function owners and nineteen aliases
under review. No private constructor or compiler alias is cleared by these
helper receipts.

Two additional private construction rows review the actual Key and Host Clone
capabilities. Native tests assert retained Js receiver/object identity, shared
mutation and nonce state, signed-zero bits and SameValueZero behavior. These
construction rows do not classify the types' raw APIs. The alias suite now has
eleven tests covering all nineteen actual aliases, including callback errors,
polled futures, handler holes/undefined slots, ordered string/nonce containers
and native Market fields. The production alias gate obligations remain separate.

Ordinary discovery originally refused records.rs because its glob import made
attribute origins uncertain. Its eight explicit enum imports remove that
ambiguity without changing any decoder body or widening the shared scanner.

## Fixed alias batch

Eighteen alias contracts retain exact compiler source/expanded targets, generics
and actual root identity. Twelve individually reviewed callback, handler and
future capabilities have native decoder/conversion absence probes. Six real data
decoders execute independent native observations: AddedMarkets<bool>, StreamFeed,
JsResult<bool,bool>, NonceIds, NonceTimes and MarketRef. Their ordered contents,
variant identity, optional presence, binary64 bits and native fields remain
visible before JSON projection. Each complete alias passes production probes
on native and wasm (six runtime and twelve explicit classifications per target).

The actual borrowed Fields<bool> decoder is excluded from these eighteen rows.
Its expanded native tests preserve static text key pointers, retained escaped
Value key pointers, owned nonempty refusals, empty-map admission and bool-domain
refusal. The uninstalled route proposal is with the shared tooling owner; no
general borrowed guard, key ownership or lifetime has changed.

The stored total is 81 keys: 63 nominal/function owners and eighteen aliases.
Remaining GRVT work is 119 nominal/function owners and Fields, with nested/local
discovery tracked separately. The nested equality function in binding/private.rs
has an exact non-decoder comparison classification, pinned to the reviewed whole
file. It returns boolean equality or the original coercion error for already
supplied native operands; it does not admit a protocol model. The native numeric
comparison test executes nineteen trait calls, including signed zero, the source
code1000 cases, object coercion count and original thrown-object identity. The
complete Private owner retains its separate API obligations. This batch does not
claim whole-crate gate completion.

## Domain and interval helpers

The domain and websocket interval contracts add two separately executed helper
reviews. The EIP-712 domain observes both network choices, the three native
object fields in order, string identity and chain-id Number bits. The websocket
interval mapper observes all eighteen own mappings, five unchanged fallbacks and
all twelve stock inherited members by native ProtoMember identity, arity and
interpolation text. The existing builder_domain and public_subscriptions tests
compare the actual TypeScript functions independently. These two contracts do
not establish the caller's signing, subscription, or arbitrary host behavior.

Swapping only the two production domain chain IDs compiled and failed the two
network output expectations. Changing only the production 1m mapping to CI_3_M
compiled and failed that exact interval output expectation. Each control retained
the other helper's passing result; exact source restoration passed both owners.
Selected production execution also passes on wasm. No new classification-only
skip or historical debt row is introduced by these two helper reviews.

## SDK schema helpers

Three SDK schema-helper contracts execute twenty-four independently authored
cases against the native functions and actual vendored SDK. Metadata lookup
returns exact full/lite declaration tokens. BigInt conversion retains decimal
versus even-width hexadecimal formatting and non-BigInt identity, including the
source's negative hexadecimal spelling. Bidirectional mapping observes native
variants, ordered UTF-16 keys and strings, nested array lengths, Number bits,
primitive identity and own undefined fields before serialization. Carrier tags
construct these test values; they do not change the external input domain.

All three complete APIs pass separately on native and wasm. Isolated production
mutations of a metadata selector, hexadecimal padding and the unknown-key target
each compile and fail the corresponding output expectations. Restoring the
source exactly passes all three again. The existing fifty-three-case source
oracle covers the consumed root and nested SDK maps. No shared decoder guard,
borrowed lifetime rule or historical debt entry changes. Remaining compiler
owners and local scopes retain their review obligations.

## Complete exchange owner and borrowed Fields

The GrvtExchange contract retains all eleven compiler-selected APIs. Its
constructor has a specific typed capability role; six raw methods observe native
results, Store state and HTTP requests. Four inherited rejections use only the
reviewed per-method source, signature, fixture and adapter policy. Each rejection
observes the actual Error name/message and unchanged HTTP/state effects.

Fields<bool> retains its Rc/RefCell/IndexMap identity and static borrowed string
keys. Its finite borrowed-map contract executes all 36 immutable route witnesses:
static text, retained static borrowed Value and owned Value. Native key order,
bool payloads, source-text offsets and borrowed Value key pointers are observed.
Escaped text and nonempty owned Values refuse; an empty owned map admits. No
caller projection, ownership change or permission for other generic arguments
is added. All nineteen reviewed GRVT aliases now have their own stored contracts.

Both new complete owners pass production execution on native and wasm. The
complete stored set executes together on each target, including all nineteen
aliases, and its receipts pass the scoped ratchet. A compiled
production position-size drift fails both fetch_positions observations while all
36 Fields routes remain green; exact byte restoration passes both owners again.
The merged Fields tooling additionally proves native identity, member-order and
removed-policy controls. These receipts retain the ordinary guards and do not
complete the remaining nominal, receiver or local-scope review.

## Composition owner batch

Four further contracts retain all five compiler APIs on ClientOrderIdRule and
the exchange, public-socket and private-socket composition owners. The validator
executes 29 independent UTF-8 string cases at canonical decimal and reserved
2^63 boundaries, including integers beyond Number's exact range. It does not
convert client IDs to Number.

Each member API uses Rc<Cell<i64>> values, with u32 socket timers and String
private fill keys. Eight cases per owner inspect original values, Some/None,
ordered keys, Rc pointer identity and mutation through the returned alias, while
checking unrelated native fields and state. These finite native instantiations
do not classify arbitrary generic values or JavaScript prototype behavior; the
owned registry representation remains the declared boundary.

The exchange's record_nonce wrapper has four independently authored cache cases.
They retain both Rc handles and inspect native time bits, unknown times, real-ID
sentinels, insertion order, earliest valid time and paired eviction at 200 entries.
The actual maintained TypeScript validation arrow and recordNonce body match
the corresponding 33 expectations. The source controls widen the reserved range
and cache limit in memory and introduce a late duplicate validator; each requires
a named failure and passes again with the original source.

The 57 cases execute production Rust before projecting selected observations.
Remaining owners and generic instantiations still need their own reviews.

Six separately compiled native production controls widen the reserved client-ID
boundary, remove each of the three member lookups, drop the real-ID delegate
argument and overwrite the retained earliest timestamp. Each fails the intended
owner with an observed-output mismatch; exact restoration passes all four owners.
The production files are restored unchanged.

The current 92-row stored union passes on native and target-specific Wasm: 79
production runtime results and 13 explicit classifications per target, including
all nineteen aliases. All 184 receipts pass the scoped ratchet without changing
the standing defect ledger. Both maintained source tests, strict host/Wasm clippy,
formatting and parity checks pass. The 109 remaining indexed nominal/function
owners still need individual review.

## Scalar, selector and error helper batch

Twenty additional function contracts review sixteen executed projections and four
output-only diagnostic factories individually. The production bodies retain their
behavior; the Rust additions are native factory witnesses under cfg(test).

The sixteen runtime adapters execute 310 independently authored cases. Five
scalar helpers contribute 190 cases covering absent/null/boolean inputs, empty
and nonempty arrays/objects, noncallable coercion members, finite raw JSON Number
rounding, signed zero, overflow and underflow. Numeric outputs retain binary64
bits; only NaN is canonicalized. Strings can produce nonfinite results, while the
raw JSON carrier remains finite. This chosen carrier does not admit executable
coercion hooks, own undefined properties or arbitrary host objects.

The adjacent response selectors distinguish missing Option values from present
null, preserve truthy-first alias precedence, and compare length strictly with
numeric zero. Optional uppercase retains native text or the caller's diagnostic.
Signing fields keep pair order, duplicate names and empty strings. The recursive
signing integer carrier preserves the exact binary64 integer as decimal text,
including values beyond 2^53 whose shortest Number spelling differs. This is the
existing Alloy representation adapter, not an EIP-712 validation claim. Interval
projection checks mapped strings and all twelve stock inherited identities. Its
producer supplies a mapped fallback for misses, so Missing is outside this
internal call domain.

The direct nonce producer has four ordered-cache cases, separately from its
composition wrapper. They inspect native time bits, real-ID sentinels, earliest
time retention and paired 200-entry eviction. Thirteen cancellation cases inspect
native typed success, response message/code precedence, coercion before callback,
callback ordering and retained callback-error identity/metadata. Seven logging
catch cases retain the original native cause or replacement logger error, with
all four severities visible. These typed internal success cases do not extend the
source Exchange return type.

The four diagnostic factories perform no protocol admission. Twelve direct
native witnesses inspect their real error fields and fresh default metadata;
the JS error factory additionally checks UTF-16 fields and distinct object
identity. Their explicit classification receipts do not claim gate runtime
execution. Each factory has its own compiling name-drift negative control.

The maintained accepted-value-source.cjs oracle compares 190 cases with actual
ECMAScript String, Number and parseFloat operators, plus 85 independent selected
language/representation expectations. It also executes the two actual maintained
GRVT cancellation catch bodies against eleven error fixtures. Two typed success
cases and the uppercasing/logging native representation remain separate native
evidence. The existing composition oracle supplies four actual-source nonce
cases. Fresh JavaScript Error stacks are ambient; the oracle compares the
explicit callback stack only, rather than equating a fresh JS stack with native
None. In-memory nullish-fallback and source-prefix controls require named failures
and restore the baseline.

Run the maintained witnesses with the isolated TypeScript compiler path:

```sh
SAFE_CEX_TYPESCRIPT_PATH=/absolute/path/to/typescript/lib/typescript.js \
  CARGO_INCREMENTAL=0 cargo +1.99.0 test --manifest-path sdk-rs/Cargo.toml \
  --locked -p safe-cex-venue-grvt --test accepted_value_source
CARGO_INCREMENTAL=0 cargo +1.99.0 test --manifest-path sdk-rs/Cargo.toml \
  --locked -p safe-cex-venue-grvt --lib accepted_
```

Twenty individual production drift controls compile and fail their intended
native assertions before exact restoration passes. Sixteen runtime mutations
exercise coercion, parse/default selection, alias precedence, emptiness, text and
schema identity, integer conversion, nonce retention and callback/log ordering.
Four factory mutations each fail their named direct native test. Dependent
helper failures are checked explicitly; compiler failures do not count.

The full stored union now has 112 owners, including all nineteen reviewed
aliases. Fresh native and actual target-specific Wasm execution each pass 95
runtime receipts and seventeen explicit classifications with zero failures.
All 224 selected receipts pass the scoped ratchet, with the standing ledger
unchanged. The seventeen selected library tests and two maintained source-oracle
tests pass, as do strict host all-target/all-feature and Wasm library Clippy,
crate formatting, and the parity invention/status gate (7,459 rows and 17,214
public Rust items). The remaining 89 compiler nominal/function owners still need
individual review; this batch does not claim whole-package clearance or broader
generic, feature, local-owner or executable-host coverage.

## Configured Store and response helper batch

Twenty-two further function contracts execute 158 independently authored cases:
nine data-read helpers, three snapshot delegates, three market helpers and seven
order-control helpers. Each retains its complete compiler-selected function API.
The fixture supplies native values, ready futures, field projections and callback
observers; every adapter invokes its indexed production function directly.

Native observations include all produced market and ticker fields, Number bits,
Option/Cow variants, borrowed slice pointers, ordered cache contents, input-slice
identity, retained callback/cleanup capabilities and original error origins.
Eager callback effects are captured before polling and compared with completed
request, logging, emission and notification events. These are explicit native
callback and owned Store domains; they do not establish arbitrary executable
JavaScript objects, concrete SDK/routing behavior or wider generic lifetimes.

Market projection distinguishes source evaluation errors from the existing
closed typed-output refusal. A later null row must take precedence over an
earlier representation refusal, while an isolated representation refusal follows
successful fetched logging. Refresh captures its old set before the fetch hook,
replaces metadata even when membership is unchanged, and commits Store state
before change emission and logging. Reentrant replacement and separate callback
failures observe both the returned error and already committed native state.

Leverage limits retain the borrowed original slice on skipped or caught paths;
a valid unmatched response still produces an owned vector. Default leverage
reads and cache notifications remain independently visible. The setter reads
account identity twice and notifies its cache update before looking up the first
position afresh. A subscriber replaces the original positions during that
notification; the patch must target the new live position. The standalone
cache_notification_precedes_fresh_position_lookup regression additionally checks
actual Store fields, account reads and worker flags before fixture projection.

Cancellation guards, endpoint waits and emitted errors run through each actual
entrypoint. Batch cancellation continues after ordinary request failure; batch
placement stops on the authentic native Builder variant and preserves its reason
and cap bits. A spoofed error name remains ordinary. Retained order-book callbacks
and the exact returned Rc cleanup capability are invoked after the delegate
returns. The finite native C/R witnesses grant no permission to other generic
instantiations.

The existing data_reads, markets, snapshots and order_controls integration suites
compare their maintained source corpora with native fields and effects. They are
separate evidence from the newly authored contract cases and do not expand their
bounded input domains. Run those witnesses and the direct Store regression with:

```sh
SAFE_CEX_TYPESCRIPT_PATH=/absolute/isolated/node_modules/typescript \
  CARGO_INCREMENTAL=0 cargo +1.99.0 test --manifest-path sdk-rs/Cargo.toml \
  --locked -p safe-cex-venue-grvt --test accepted_store_helpers \
  --test data_reads --test markets --test snapshots --test order_controls
```

Use the TypeScript package directory with axios and axios-retry installed beside
it: the complete fetchMarkets source oracle executes the real request factories.
All 34 targeted native/source tests pass, including the direct reentrant Store
regression; these are separate from the 22-function contract baseline.

Twenty-two individual production mutations compile and fail their intended
native output assertions; exact restoration passes each complete function
contract. They cover lookup/cache keys, independent default reads, book prices,
singleton filters, ticker fields, late account reads, market casing/endpoints,
cancellation payload and scope, ordinary-error continuation, authentic Builder
termination, retained listeners, candle/book ordering, metadata-only refresh,
leverage values and the cache-before-position-read sequence. Compiler failures
do not count as controls, and all four production files are restored exactly.

The complete stored union now contains 134 owners, preserving the previous 112
rows unchanged. Fresh native and actual target-specific Wasm execution each pass
117 runtime receipts and seventeen explicit classifications, including all
nineteen reviewed aliases, with zero failed owners. All 268 selected receipts
pass the scoped ratchet; the standing ledger is unchanged. The remaining 67
compiler nominal/function owners still require individual review, alongside the
previously stated local, feature and executable-host obligations.

Strict host all-target/all-feature and Wasm library all-feature Clippy pass, as
do crate formatting and the parity invention/status gate (7,459 rows and 17,214
public Rust items). These checks followed the fresh compiler inventory and both
complete stored-contract runs after merging current master.
