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
