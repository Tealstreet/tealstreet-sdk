# Field identity audit (in progress)

Two independent questions apply at every consuming boundary: equal fixture
values hide field swaps, and symmetric serde names can undo a wrong decode on
re-serialization. A complete roundtrip is retained as a shape check; direct
native field reads are required for identity.

The first live SDK schema/fixture census examined 60 records. This counts source
record shapes, not generated edge cases or decoder entry paths:

| Corpus | Records | Records with equal same-source-type fields | Equal groups |
| --- | ---: | ---: | ---: |
| common | 10 | 10 | 15 |
| orders | 13 | 12 | 14 |
| responses | 22 | 10 | 11 |
| transactions | 15 | 2 | 2 |
| Total | 60 | 34 | 42 |

These are exposures, not 42 proven gaps. The generated numeric corpora already
vary each numeric field and compare native f64 bits; direct record boolean axes
also exist for orders/responses/transactions. Their string/literal/array identity
and additional outer decoding layers still need the symmetric-mapping audit.

## Repairs and controls

- L2TxInfo: eight variants now assert native strings and independently varied
  native booleans alongside their existing direct/nested native numeric bits.
  Reviewer mutations swapping public/private serde names compiled and failed
  `L2CreateOrderTxInfo.price native string identity` and
  `L2CreateOrderTxInfo.reduceOnly native boolean identity false` at 101.
  Restored baseline 7/7. Its earlier post-decode bool swap passed because both
  fixture flags were true; the repair addresses both independent classes.
- SDK common: ten record shapes gain one-leaf-at-a-time string/number/boolean
  inputs and a native field projection. Child Balance and OrderBookLevel fields
  are read recursively through Account and OrderBook, not serialized as opaque
  child records. Numeric edge cases and Unicode fixtures remain separately.
  Isolated review returned MERGE: symmetric field renames preserving wire order
  failed `Balance.available native field identity` and the outer
  `Account.balances[0].available native field identity`, both compiling then
  failing at 101. Restored common baseline passed 3/3; scoped strict Clippy passed.
  Zero-flattening and finite-corruption controls independently failed
  `Balance.decimals existing Value signed zero` and
  `Balance.decimals native field identity` at 101.

- Generic wrappers: two records now decode source-side identity fixtures rather
  than generating their own expected input through Rust Serialize. Every native
  field is asserted, including distinct payload/metadata numbers, message/error,
  ordered item values and all four hasNext/hasPrev combinations. Isolated review
  returned MERGE: symmetric public/private names compiled and failed at101 on
  PaginatedResponse.hasNext native identity; restored4/4 and host Clippy passed.
- SignedTx: native identity crosses all eight transaction alternatives and the
  signature wrapper. Symmetric r/s serde names compiled and failed
  SignedTx.signature.r native identity at101; restored9/9, isolated MERGE.

- SDK orders: 13 native projections cover each field, and one-field cases vary
  strings, numeric wrappers, booleans and ordered string arrays independently.
  Isolated MERGE: symmetric public/private reduceOnly/postOnly names compiled
  and failed OrderInfo.reduceOnly native field identity at101. Restored3/3 and
  scoped strict host Clippy passed. Existing edge-bit cases remain.

- SDK responses: 22 native projections recurse through typed child records and
  exhaustively map closed enum variants. Identity inputs vary each leaf, including
  dictionaries and nested records. Baseline16/16 passed after nine identical Node captures became one per process.
  Isolated MERGE: symmetric names compiled and failed HealthResponse.database
  native field identity and nested FeeTier fields through FeeSchedule native
  field identity at101; restored16/16 and scoped strict host Clippy passed.

- SDK transactions: all15 original records now project native fields, including
  nested TransactOpts, with independent per-field values. Isolated MERGE: symmetric
  names compiled and failed CreateOrderTxReq.reduceOnly native field identity
  at101; restored9/9 and scoped strict host Clippy passed. Transaction capture
  calls fall4to1; generic capture calls fall2to1 with restored4/4 and scoped lint.
  Together with common/orders/responses this covers the60-record SDK census,
  plus the two generic wrappers, L2TxInfo and SignedTx outer boundaries. It does
  not complete the older venue/helper or shared-feeder audit.

- Eight signer/approval type-only records were examined. Seven have repeated
  same-typed values in the older edge corpus; cancellation has one field per type.
  New distinct-value construction/Clone cases cover all eight, with all eight
  create-order boolean triples. Isolated MERGE: Clone fee corruption compiled
  and failed LighterApproveIntegratorRequest.max_perps_maker_fee native clone
  identity at101. Restored tests and separate scoped host lint passed. These
  records have no request deserializer; no production decode/signing claim.

- Deployment profile: seven native fields and four proxy-flag combinations.
  Private-Wire-only proxy field renames compiled and failed
  DeploymentProfile.usesRestProxy native identity at101; public serialization
  and tables were unchanged. Restored1/1 and scoped host Clippy; isolated MERGE.

- Older scalar records: three live_pure checks now read native fields from
  distinct inputs. Symmetric minimum-field names compiled and failed
  LighterOrderMinimums.minBaseAmount native field identity at101; restored1/1
  and scoped host Clippy passed, isolated MERGE. The separate numeric audit
  already read those fields directly; this closes the local consumer gap.

- Close-classification records: two older decoders now exercise all 4 public
  and 64 private boolean combinations with native fields. Private handshake/
  rate-limit swap compiled and failed the named isHandshakeFailure native decode
  identity mask 1 assertion at101; restored1/1 and scoped host Clippy, MERGE.

## Shared feeder (original bytes preserved)

`tests/fixtures/signing-preimages.json` is consumed by the merged crypto
independent oracle. Its 92 cases retain SHA256
`afe399cc424ff3e4e5393ef9c2f8f642b770530328f6cc70d6aefd830383679a`.
All approval cases set maxPerpsMakerFee, maxSpotTakerFee and maxSpotMakerFee to 0;
revoke sets all fees to 0. A swap between those three dimensions is invisible to
that artifact. This is corpus exposure, not a finding that the signer is wrong.
The actual SDK hash-composition maker/spot-taker swap regenerated the original
92-case artifact byte-identically at cmp0. The later live supplement now feeds
its own mandatory venue gate:24 cases/48WASM calls with distinct valid fees,
direct fee-word identity and shared crypto comparisons. Generated JSON stays
in ignored .aimux/captures/signing-field-identity.json; maintained source and
tests are the portable gate. Shared crypto adopted the pinned generator at
74aa2775f1; integration and live CI are unclaimed, as is retroactive independence. Scoped3/3 passed; isolated review returned MERGE after a hex-decoder lint repair.
A Rust consumer fee-word8/9 swap compiled and failed the named distinct-fee
Poseidon assertion at101. The reviewer independently reproduced both the default
original SHA and cmp0 under the actual source fee swap. Strict scoped host
Clippy passed. The adoption document and nonignored shared test at 74aa2775f1
were inspected; the copied generator matches ab2aadb8f5 byte-identically. The
archived generated supplement remains unchanged; future local captures name
the adoption in their later header.

## Remaining scope before merge-up

Helper/state corpora, signing consumers and remaining older venue fixtures
remain in the sweep. No venue-wide audited-zero or completion is claimed.
The separate NaN capture audit must retain signed zero and finite bit controls;
JSON numeric-literal ingress cases cannot contain NaN, while semantic helper
oracles already tag it separately. This is not yet a complete corpus inventory.

## Consumer-shape inventory

All 24 top-level Rust test targets were inspected for their assertion shape.
This inventories consumers, not every generated equal-value group; the remaining
fixture sweep above is still open. A JSON projection below means explicit reads
of native fields, not Serialize of the tested record.

| Target | Identity assertion shape |
| --- | --- |
| accepted_records | Rejected positional inputs only; no positive identity claim |
| live_approval_types | Native construction/Clone fields; no wire decoder |
| live_builder_fee | Native config fields and explicit enum variants; scalar outputs |
| live_exchange_helpers | Scalar results, explicit error fields; input models owned by shared types |
| live_numeric_ingress | Native field bits across raw/owned/borrowed entries |
| live_pure | Scalar outputs; record decode and close-policy checks repaired separately below |
| live_sdk_common | Recursive native projection; roundtrip retained as shape check |
| live_sdk_constant_types | Explicit variant inventories plus serialization and accepted-set checks |
| live_sdk_constants | Direct named constant/table reads; no record decode |
| live_sdk_generics | Native wrapper fields and retained payload decoding |
| live_sdk_orders | Native fields and ID arrays |
| live_sdk_protocol | Direct named constant/table reads; no record decode |
| live_sdk_protocol_types | Native numeric wrapper bits and explicit variant inventories |
| live_sdk_responses | Native fields, nested children and dictionaries |
| live_sdk_transactions | Native record leaves, union alternatives and SignedTx outer fields |
| live_signer_helpers | Scalar/Value results and explicit error fields |
| live_signer_types | Native construction/Clone fields; no wire decoder |
| live_tier | Explicit tier variants and scalar results; opaque record projection |
| live_types | Native deployment fields plus table and accepted-set checks |
| live_utils_coercion | Native minimum/cancel result projections added; serialized checks retained |
| live_ws_state | Scalar results; opaque Value flattening rather than typed-record roundtrip |
| signing_capture | Source-only historical artifact comparison, with measured fee blind spot |
| signing_field_identity | Direct preimage word identity and shared crypto outputs |
| table_inheritance | Native TableHit identity/branches; production mapper remains pending |

Source-control child processes intentionally execute separately changed corpora.
Missing-compiler controls likewise have their own failing process. Neither is a
per-case Node spawn. The former 104 per-case numeric/Unicode spawns were replaced
by one capture; unknown-depth helpers already batch their six depth or nine
Unicode cases per record and remain separate corpora.

## Assertion-shape buckets

The 24 targets partition by their primary assertion boundary (mixed targets keep
all earlier checks). This is not a count of every generated fixture:

| Bucket | Targets | Can a differing-value swap be observed? |
| --- | --- | --- |
| Rejection only (1) | accepted_records | No positive identity claim; paired positive gates cover records |
| Constants/variant inventory (4) | live_sdk_constant_types, live_sdk_constants, live_sdk_protocol, live_sdk_protocol_types | Named constants, variants and bits; no multi-field decode roundtrip |
| Native record fields (11) | live_approval_types, live_builder_fee, live_numeric_ingress, live_sdk_common, live_sdk_generics, live_sdk_orders, live_sdk_responses, live_sdk_transactions, live_signer_types, live_types, live_utils_coercion | Direct native field reads; per-field inputs/controls recorded above |
| Scalar/opaque helper outputs (3) | live_exchange_helpers, live_signer_helpers, live_tier | Scalar or explicit Value comparison; broader equal-input census still open |
| Borrowed collections (2) | live_pure, live_ws_state | Native slices/items now compared directly; distinct-input supplement below |
| Signing feeders (2) | signing_capture, signing_field_identity | Historical fee blind spot reproduced; separate adopted supplement discriminates it |
| Tagged lookup (1) | table_inheritance | Native TableHit identity and branch predicates; mapper remains pending |

The borrowed-collection supplement is shared by two existing captures and does
not start another Node process. Eight public inputs and eight private inputs
cover four helpers: REST book/ticker extraction and both private flatteners.
Two records have distinct strings, numbers, opposite booleans, nested strings
and array items. Cases include empty/singleton/multiple collections, adjacent
and nonadjacent duplicates, integer-like object keys and competing nested aliases.
These inputs add to the prior corpus rather than replacing equal-value edge cases.
Assertions read native bid/ask slices and ordered Value references without using
Serialize. Value is already the helper's output type, so there is no typed field
rename followed by an encoder that can undo it at this boundary.

Reviewer reproduced the bid/ask output swap against pre-repair f3eb5d91a1:
it compiled and failed at101 on the old parseRestOrderBookLevels assertion
(bids [[1,2]], asks []). Thus bid/ask blindness was NOT present in that corpus.
The strengthened assertions also catch a compiling native-side swap and a
private flattened-order reversal. Collapsing the supplement's sides or booleans
fails both source captures at their named distinctness assertions. This shared
module feeds these two venue captures only, not the archived crypto oracle.

## Scalar and opaque-helper bucket

Three targets were inspected by their output shape: live_exchange_helpers,
live_signer_helpers and live_tier. Scalar outputs do not have two decoded record
fields for Serialize to swap back. Error comparisons explicitly retain name and
message. Tier enum results now use exhaustive native variant names, including
successful public Deserialize, rather than relying on enum Serialize for identity.
Opaque asRecord/getProvider results compare the borrowed Value directly.

The existing corpora already compete nonce code/message, tier aliases, reference
price candidates, margin aliases and numeric helper inputs. The tier maker/taker
fee helper is deliberately symmetric: exchanging its arguments preserves the
source predicate, so that swap is not a defect to detect. This is separate from
the signing fee dimensions whose historical blindness was reproduced.

Two distinct nested records from collection-identity.cjs now feed asRecord and
getProvider in their existing captures. Provider cases also compete the selected
field against a different shadow object. This module therefore feeds four venue
captures, not an external gate. No capture process was added. The signer corpus
contains 992 nonce cases and 66 provider cases; tier contains 27 normalized,
4 eligibility, 958 payload, 99 error and 81 fee cases plus 3 literal variants.
Inventory guards now cover these arrays and all 27 exchange-helper arrays, so
an omitted comparison corpus cannot silently pass. These are maintained case
counts, not a claim that every repeated value was removed.

Precision scope: existing utils-coercion inputs have amount step 0.01 and price
step 0.1, plus a 6-by-6 cross-product of precision fields. Their amount/price
helpers return separate scalar results, checked against live TypeScript. Runtime
order formatters remain pending, so this evidence must not be generalized to a
production price/amount formatting pipeline that is not implemented here.

### Bracket precision audit

The implemented Rust has exactly two direct precision-field reads, both scalar
fallbacks in utils.rs: get_lighter_amount_decimals reads precision.amount and
get_lighter_price_decimals reads precision.price. The independent reviewer
exchanged those reads in compiling production code against the UNCHANGED
live_utils_coercion corpus. It failed101 at getLighterAmountDecimals: amount0.01,
price0.1, expected2, received1. No surviving precision mutation was measured.

The source formatter consumers remain PENDING: resolveConditionalExecutionPrice
(line2371 price adjustment), placeOrder (lines2387-2390 and2450-2455, including
conditional triggerPrice), and placeOrders (lines2607-2610 and2663-2664).
They have no Rust operations or native payloads to mutate yet. Single-axis
scaling takes supplied decimals; it does not choose price versus amount. This is
not formatter immunity: adoption must add fractional distinct-step payload cases
across sides and limit/stop/trigger branches and mutate each actual consumer.

Per-site refinement at 77cf56f03c: the earlier combined swap failed at the amount
assertion first and therefore did not independently establish price coverage.
Two fresh independent production mutations now distinguish the sites:

| Implemented scalar site | Mutation | Existing named failure |
| --- | --- | --- |
| utils.rs:334 get_lighter_amount_decimals fallback | amount to price only | getLighterAmountDecimals, got1 expected2, exit101 |
| utils.rs:359 get_lighter_price_decimals fallback | price to amount only | getLighterPriceDecimals, got2 expected1, exit101 |

Each mutation compiled, each was restored before the next, and the restored
target passed1/1. Both used amount0.01/price0.1 with empty info. Zero precision
fixture-value repairs; the unchanged generator SHA256 is
`dbb555b0b4e7afa91d2625cd70e56c0747aaea4c263e06e03cc8ac35ca348aeb`.
This is ALREADY COVERED NOW per scalar site, not a claim that either corpus was
never blind before earlier sweeps, and not a venue-level payload coverage verdict.

Pending adoption must independently mutate execution price, triggerPrice and
amount outputs, including split lotAmount and intermediate rest/lotSize rounding
in placeOrder, batch price/amount in placeOrders, and conditional slippage price.
No one passing field may stand in for another. These source rows remain pending;
there is no compiled native payload formatter on which to perform those controls.

## Cardinality audit

The six-shape census covers the same 24 targets above. Inspection and mutation
proof are separate: five zip sites already had a native length, fixed table or
complete-array guard, but that did not establish independent corpus totals.
The expanded inspection found free helper subcorpora in live_utils_coercion,
live_pure, live_builder_fee, live_signer_types and live_approval_types. Fixed
counts now guard those arrays. Existing ws-state corpus counts remain pinned.

Cancel-target comparison additionally pins 13 source cases, 31 total outputs,
the 13 per-case output counts and 62 native field comparisons. Its independent
native length assertion remains before zip. Six production mutations compiled
and failed at `resolveLighterCancelTargets inventory`: empty, shortened and extra
outputs, each both with source guards present and with them bypassed. This is
current mutation evidence, not a claim that the old subcorpus loops were safe.

REST collection checks independently count source and native books/bids/asks/
tickers at 424/425/251/3286; native per-case lengths precede complete slice
comparisons. Private flatten checks pin source totals 704/840 and independently
count compared native payload/order items, retaining complete ordered equality.
Fifteen independent output mutations compiled and failed101 with source-total
guards bypassed: empty/subset/extra for bids, asks, ticker items, flattened
payload and flattened orders. Each failed its own native count assertion.
These are post-computation output controls, not consuming-deserializer controls.

The signing supplement now requires the 24 deployment/operation identities and
exactly one hash plus one signature call in every case, rather than allowing
48 aggregate calls to conceal an empty case. The original 92-case artifact and
both signing generators are unchanged by this audit; only consumer assertions
change. Shared crypto adoption of these later cardinality assertions is not
claimed. SignedTx per-variant/location distributions remain an open census item
beside the separate uncommitted TxReceipt slice.

A real older false green was reproduced against 8ad81895ba: assigning
`scalars.length=0` in the utility capture compiled and passed1/1. The repaired
target fails101 at `UTILS-CARDINALITY scalars source cases` (0 versus456).
This is an oracle-derived loop that executed zero scalar comparisons; no live
utility-production defect is claimed. Empty and shortened scalars are now
permanent source-capture controls alongside cancel expected-output truncation.

The inherited-table gate additionally pins all12 distinct member identities;
17 aggregate rows could otherwise hide a duplicate inherited name. Its pending
mapOrder source witnesses still do not claim an implemented Rust mapper.
The consuming serde-sequence audit is recorded below; a non-consuming parser
error does not count as proof on that separate axis.

### Consuming response collections

Nine of the 15 public SDK Vec fields now have consuming-sequence proof:
MaintenanceWindow.affectedServices, AccountApiKeys.apiKeys, ExchangeInfo.markets,
ExchangeInfo.tokens, ExchangeInfo.rateLimits, FeeSchedule.makerTiers,
FeeSchedule.takerTiers, SystemStatus.components and
SystemStatus.scheduledMaintenance. Each source array has four items, checked
independently of the decoded native length, with 24 item comparisons across
bare/shared raw, owned and borrowed paths. Other root arrays are emptied to
isolate each field; the source capture and its existing fixtures are unchanged.

The injected shared Array::next_element_seed drained its iterator before
returning None. It compiled and all nine tests exited 101 at named
`SDK-CARDINALITY <Type>.<field>` assertions after successful decoding on every
path: native length zero versus four, empty native identity and zero versus 24
comparisons. No trailing-element error supplied the failure.

With that consuming mutant retained, removing the native length and comparison
count guards still failed all nine through native collection identity on all
six paths. The count guards are redundant for this mutant; this is not a claim
that removing them makes the target green. Shared source and test were restored
byte-identically and all nine baselines passed. These are injected test controls,
not a finding that the production decoder drops elements.

The remaining six fields use a separate, later source supplement:
Account.balances, OrderBook.bids, OrderBook.asks, PaginatedResponse.items and
BatchCancelOrdersParams.orderIds/clientOrderIds. Each has two distinct items.
The oracle checks those six source collection declarations, not full schemas.
Account, OrderBook and batch cancel exercise bare raw/owned/borrowed decoding;
PaginatedResponse additionally exercises shared raw/owned/borrowed decoding.

For these fields the injected private Wire field decoder first deserializes the
entire Vec and then returns an empty Vec; optional fields retain Some(empty).
This is consuming field-deserializer corruption, distinct from the shared Array
mutation and from clearing an already-decoded record. All six compile and fail
101 at their named SDK-CARDINALITY assertions on each covered path. Keeping
that mutant while removing native length/comparison guards still fails native
identity: the guards are redundant for this mutant, not generally redundant.

The maintained scripts/check-consuming-collections.py runner performs both
mutations, both guard-removal variants and restored baselines serially. It pins
15 field identities and their path inventories independently of the source
capture, rejects compile/decode errors as proof, and restores original bytes.
Its logs and generated result sheet stay ignored. No historical fixture or
shared crypto generator changes, and no live production data-loss defect is
claimed. This closes the consuming-empty axis for these 15 fields only;
it does not imply every subset or extra-output mutant has been run here.
