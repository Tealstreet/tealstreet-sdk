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
