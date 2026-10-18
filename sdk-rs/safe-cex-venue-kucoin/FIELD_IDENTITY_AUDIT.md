# Field identity audit

The cec0bc5ce9 merged-tree test run passed 109 tests across 24 binaries. This is not a
retroactive independence claim for earlier oracle results.

## Consumer boundary

Tracked-source searches for both golden filenames and the crate's scripts/tests
paths found no consumer outside this crate. The shared remote-stage oracle uses
its own KuCoin-labelled cases, not these fixtures. The numeric-ingress source
corpus is consumed by this crate's numeric-ingress test; it does not supply the
shared numeric corpus. Root CI commands execute these crate tests, rather than
feeding their output to another oracle.

No owned cross-crate feeder was found. Consequently the eleven crate-local test
fixture repairs are in-place repairs. Neither committed golden was regenerated
or edited. Their SHA-256 digests are:

- `api-capture.json`: `721669b61438f42d8222be728d1a9c22ce73a14a17884cc3cdd185ade12f9813`
- `api-retry-capture.json`: `efce1b0f3af17f37b4e1e411de78a35a359da7f8ac3d660fa63d0196e096b518`

If an outside consumer is subsequently found, preserve its original artifact
bytes and add a separately labelled supplement; do not rewrite the historical
artifact or claim that the supplement made its earlier greens independent.

## Measurement boundary

A Node wrapper captured actual inputs and outputs of the scoped crate test run.
The preliminary recursive sibling-scalar census covered 31 oracle scripts/golden files,
57,972 object occurrences and 361 equal-valued field-pair groups. Of these, 234
had no differing value at the same normalized record path. Those are candidate
pairs, not 234 defects: source tables, fixed output constants and diagnostic
booleans also appear in the output. A second pass decoding embedded JSON strings found 60,096 object occurrences
and 375 equal-pair groups, 244 without a differential at that encoded path.
That pass exposes field cases hidden inside raw JSON; array dimensions remain
separate. The run includes negative-control subprocesses;
the count is not a baseline-only corpus count. The second pass covers embedded raw JSON; a sub-agent also inspected Rust-only
fixtures and found two concrete consumer gaps (balance and fills), now listed
below. The supplemental cases do not retroactively validate earlier greens.

The two goldens have one candidate scalar pair: capture case `name` and
`wireBody`, equal in two cases but distinct in ten. The retry golden has zero
same-valued sibling scalar pairs under this method. Neither count claims that
embedded JSON strings or array dimensions were audited by that scalar pass.

## Repairs and controls

| Test target | Fixture changes |
| --- | --- |
| formatters | Position notional/contracts and unrealizedPnL/liquidationPrice differ. |
| order_helpers | Position numeric dimensions differ. |
| order_reads | Existing order timestamp differs from filled quantity. |
| sizing | Order timestamp, position dimensions, ticker identity and numeric dimensions differ. |
| spot_helpers | Market id/symbol and leverage limits differ. |
| spot_markets | Stored market precision and leverage dimensions differ. |
| spot_tickers | Market id/symbol, precision and leverage differ; stored ticker dimensions differ. |
| mappers | Four per-field postOnly/stop cases cover both venues and opposite booleans. |
| balance_reference | BTC with a nonunit ticker price separates amount/notional; currency and coin fields are asserted individually. |
| spot_fills_reference | Distinct symbol/orderId/side/feeCurrency replace absent string dimensions; output identities are asserted. |
| spot_validation | Two added raw balances vary used/upnl independently while retaining the original signed-zero case. |

All 45 tests across the first eight targets pass; the three additional targets
pass 14 tests. Swapping production ticker
`market.id` and `market.symbol` compiles, then fails `live Spot ticker output`
in `live_spot_tickers_numeric_bits_and_fallback_order` with exit 101. Restoring
production passes the same test. The unchanged source makes last/mark/index
identical in newly mapped Spot tickers; stored fallback fixtures can differ and
now do. This does not claim a swap of identical source constants is observable.

Swapping production `row.get("stop")` and `row.get("postOnly")` compiles and
fails `KuCoin mapper numeric field identity` with exit 101. Swapping the server
decoder's pingInterval/pingTimeout projections also compiles and fails
`direct KuCoin numeric ingress refused`, naming `$.pingInterval`. The shared
output guard catches that swap before a corrupted record can escape.

The numeric-ingress corpus already varies each numeric field independently,
including through the outer token and response decoders. Its literal `1` cases
intentionally coincide with the sibling sentinel; the other literals pin field
identity. Its original bytes remain unchanged. The sub-agent static inventory found no further concrete consumer gap beyond
the balance and fill cases above. Both Rust witness-consumer swaps compiled and failed with exit 101:
`balance coin field identity: BTC` (amount/notional) and
`live fill string field identity: orderId` (orderId/commissionAsset). These
controls mutate the test consumer of source output, not a nonexistent production
balance/fills mapper. Their restored tests are included in the merged-tree run.


## Semantic exclusions checked during review

Protocol table aliases are equal in the source itself. Replacing an endpoint or
side alias merely to make its value unique would change the reference. Full table
comparison pins each named entry, but swapping two identical aliases cannot
change an observable result. The same applies to the two constant false websocket
frame flags and the constant zero dimensions in fake Spot positions.

The pure Order fixture in the virtual-TP/SL predicate supplies equal numeric
values, but that predicate reads only id. It does not claim numeric projection
coverage. BaseExchange setup's equal fixture key/secret is similarly not used by
those helpers; the signing oracles use distinct authentication inputs.

Reference-only balance and fills witnesses do not implement their pending Rust
reads. Strengthening their identity assertions must not be reported as a new
production mapper or a parity-row completion.


## Post-repair census

A fresh capture labels negative-control environments and excludes them from the
baseline census. It covers the same 31 oracle scripts/golden files, with 54,095
object occurrences and 290 equal-valued field-pair groups. There are 163 groups
without a differential at their exact normalized path, including source constants
and diagnostic metadata. Comparing named path pairs finds 85 formerly
identical-only groups now have differing cases across 11 repaired test targets;
input and output paths count separately. This is not 85 production defects.

The independent static review additionally covered the 23 Rust integration test
files and factory unit cases, including absent rather than equal scalar fields.
That found the balance/fills assertion gaps. All five swaps compiled and failed
with their named diagnostics; all mutations were restored before the complete
109-test merged-tree run, which passed with zero ignored tests.

Owned oracle/golden files examined: 31. Outside-crate feeders identified: zero
by both tracked-source searches. Shared
feeders repaired in place: zero. Shared-feeder supplements: zero required.
Crate-local targets strengthened in place: eleven. Original golden bytes and
numeric-ingress corpus bytes remain unchanged.

## NaN capture and native decode follow-up

The later sweep covers seven bit-capture families: numeric-ingress, Spot symbol
reads, validation, markets, tickers, fills and orders. They share numeric-bits.cjs
and the Rust numeric_bits test helper. Only NaN is normalized to
7ff8000000000000, after DataView conversion on the JavaScript side; finite values,
infinities and both zeros retain their bits. Every live numeric capture records
captureArchitecture and captureNodeMajor. This local capture environment is
arm64, Node 26.7.0 (major 26); Rust reports aarch64. This is not an x86 CI run.

Markets, tickers and fills exercise NaN-producing coercions/query values. The
numeric-ingress JSON literal corpus admits infinities but cannot encode NaN;
the symbol-read and supplied-balance cases likewise do not synthesize NaN.
Orders has a numeric capture boundary that can receive NaN from malformed venue
numbers, even where a pagination NaN itself is not an output numeric field.
All seven boundaries use the same normalization, including those currently
without NaN output cases. The eight-pattern helper corpus independently pins
positive, negative and payload NaNs, both zeros, 1.5 and both infinities.

Two compiling Rust capture mutations fail at signed-zero capture identity and
finite and infinity capture identity, respectively, with exit 101. Restoring each
passes the same test. These assertions compare against literal expected bits,
not only against the other runtime's potentially correlated capture. Architecture
and positive Node-major metadata are asserted on the helper's source capture.

Five positive test gates across records, numeric_ingress, account_mode and
ws_helpers gained direct native-field or native-variant assertions. Re-encoding
remains supplementary. A symmetric endpoint/protocol serde-name swap in both
decode and encode compiles, then fails native server endpoint identity with exit
101; restoring it passes. Four scoped targets pass 14 tests. This control
specifically detects corruption that a serialize/decode roundtrip cancels.

The original 31-artifact census above describes cec0bc5ce9, not the expanded
capture machinery. Later live capture metadata and assertions are new evidence;
they do not retroactively establish independence of earlier runs. The current inventory has 33 scripts plus two goldens (35 artifacts). A fresh
tracked-source consumer search for the script/test paths, numeric-bits helper
and both golden names found zero outside-crate consumers. Both golden SHA-256
digests still match the originals above. No shared-feeder regeneration is part
of this change.

## Oracle process batching

Eleven functional per-case oracle loops are now one invocation per corpus:
accounts has three (32 baseline invocations to 3), order_reads has three (24 to
3), and spot_markets has five (24 to 5). Total: 80 to 11. Missing-dependency
controls are separate environments and are not included in that baseline count.
The same 24 tests across those three targets pass. No production Rust mapper or
pending row is introduced by this test-execution change.

## Derived precision identity

The bracket-only precision audit reproduced a false green: replacing the
triggerStopDownPrice price step with the amount step compiled and passed the
original formatter target. Its stopLoss=12.01 fixture rounded to 12 with both
price step 0.1 and amount step 1. Distinct step fields alone were insufficient.
The original bracket-up mutation failed; the bracket-down survivor is retained
as failed-instrument evidence, not relabelled as a caught mutation.

Twelve added live-source cases cross three amount/price step pairs (1/0.1,
0.25/0.01 and 0.01/0.5), both sides and limit/stop-market types. Amount 1.37,
price 12.37, stop loss 11.76 and take profit 13.28 distinguish derived results.
Native payload string fields are compared directly with independently captured
source fields before Rust serialization. This is an in-place crate-only corpus
repair; neither golden nor any shared feeder is regenerated.

All three price-to-amount mutations now compile and fail with exit 101, followed
by a restored pass. The previously surviving bracket-down mutation fails the
native triggerStopDownPrice identity assertion; bracket-up and algo-stop fail
the existing whole-payload precision identity assertion. The source-only corpus
and Rust payload consumer retain independent field comparisons.

## Per-field precision census

Six implemented Rust adjust sites feed six formatter fields: create price, size, triggerStopUpPrice
and triggerStopDownPrice; algo size and stopPrice. Independent production swaps
now compile and fail at KUCOIN-PRECISION create.price, create.size,
create.triggerStopUpPrice, create.triggerStopDownPrice, algo.size and
algo.stopPrice respectively. Every restored target passes. Native presence is
also checked: wrong amount precision can introduce a size the source omits.

The 235 original cases are retained unchanged. Their canonical JSON input capture
(before special-number injection) is 129321 bytes, SHA-256
 d97568d830dce51d1ca81cff6d1c573497b1f0a01139090e659b54afff18005f.
This is canonical JSON captured now, not a claim that a historical raw-stdin
artifact existed. The ignored capture is retained locally; no generated corpus
is committed. The earlier twelve labeled fractional cases are unchanged;
eighteen more extend the supplement across market, take-profit-market and
trailing-stop-market. Total: 265 cases, including thirty labeled cases across
three step pairs, both sides and all five source order types. Native Store step
fields and native payload fields are asserted separately.

Historical claims remain per field: bracket-down survived the old corpus;
bracket-up and algo stopPrice failed it. Create price, create size and algo size
are proven against the present corpus only. No never-blind claim is made for any
field, since earlier general identity work already affected the fixtures.

A source-and-Rust census finds seven futures source adjust sites mapping to these
six fields: separate stop-loss and take-profit source branches share the Rust
algo stopPrice site. Both source types are in the supplement. Spot placeOrder
and updateOrder add four source price/amount sites with no Rust operations yet;
those rows remain pending and have no swap-proof claim. Future implementations
must supply their own fractional field controls.

## Cardinality audit in progress

Initial textual census: 60 `.zip` sites across 22 Rust test files, followed by
inspection of capture helpers and oracle-driven loops. This is an inventory,
not a whole-venue proof. Recursive JSON comparisons already compare array lengths;
whole-Value golden equality compares entire arrays. Neither fact proves every
capture executes the independently expected number of native comparisons.

A reproduced false green exists in `live_spot_tickers_numeric_bits_and_fallback_order`:
the 108 input cases still reported success when the source output was emptied
with `out.splice(0)` and when only its first capture survived `out.splice(1)`.
The source script was restored byte-identically. Seven capture helpers now assert
output count against their caller's input count; websocket batch and Spot order
reference capture add the equivalent guard. These changes are separate from the
reviewed precision commit `7940a03b6437e4841b7b8be11daaa104d53eebb7`.

The separately named ticker cardinality supplement leaves the original corpus
unchanged and expects one case with two outputs. It checks the source output total,
then independently requires two native rows, iterates fixed indices `0..2`,
checks their symbols against literals as well as source fields, and asserts two
comparisons. Empty and partial source controls now fail
`KUCOIN-CARDINALITY spot_tickers source case count`. Independent production
`take(0)` and `take(1)` mutations compile and fail
`KUCOIN-CARDINALITY ticker native retained rows`; restoring yields 11 passing
ticker tests. The supplement is local to this crate and carries no retroactive
independence claim. Other targets' empty/partial production controls remain to
be measured; no whole-venue cardinality verdict is claimed.

The six-shape extension adds extra-native-output and bypassed-source-guard
controls. Duplicating ticker outputs, or removing all three source guards while
returning zero/one native rows, each compiles and fails the independent native
retained-row assertion. Restoring passes the supplement. This is seven executed
ticker mutations: two source, two native truncations, one native addition and
two native truncations with source guards bypassed.

A read-only sub-agent audit found the inherited-market formatter target also
needed its oracle helper's total asserted; that guard is now explicit. The audit
returned DO NOT MERGE as a completed sweep. Remaining production control inventory:
market and symbol reads; normal/algo/combined order reads and fallback categories;
parent/attached order mapping; the singleton create-order payload vector; remainder reconciliation;
selected positions and retained leverage-map entries; shard symbols and emitted
frames; instance-server vectors and both account-mode subaccount vectors.
Scalar-only targets need corpus/comparison coverage and nested-event inventories,
not fabricated collection-return mutations. API, balance, fills and Spot orders
reference-only captures require source controls and carry no Rust-production
cardinality claim. Tables have their own constant collections. Inspection of a
length guard is not an executed mutation result.

The next measured set adds five controls apiece for parent/attached order mapping,
remainder reconciliation, selected positions, retained leverage-map entries,
shard symbols and emitted frames: empty, partial, extra, and empty/partial with
source guards removed. All thirty compile, fail their named independent native
cardinality assertions, and restore to passing targets. Together with the prior
32, this is 62 executed controls, not proof for the remaining inventory.
Sizing now pins 255 inputs and 510 retained orders; Spot helpers pin 18 inputs,
30 positions and 48 retained leverage entries. Existing fixture values remain
unchanged. The frame and shard supplements each pin two fixed native indices.

Decoder mutation semantics are explicit: the server and classic/unified vector
mutations run AFTER Wire::deserialize has consumed the full input into Vecs.
Their failures are native-count assertions, not serde trailing-element errors.
The maintained `scripts/check-decoder-cardinality.py` also removes both fixture
and native length guards with consuming truncation still active, expecting a
named fixed-index assertion. This does not claim a mutation of the shared
Array::next_element_seed implementation, and does not allege a production bug.
The maintained command has now run all 15 variants across server, classic and
unified: each compiled and failed its named assertion, then each restored target
passed. The six variants removing both fixture and native length guards fail
`KUCOIN-CARDINALITY server/classic/unified native index coverage`; the earlier
15 decoder controls removed source guards only. These are separate proof scopes.

Scalar capture totals are fixed independently at 137 broker, 352 parser, 281 query
and 870 order-decision cases. Each target executes its native comparison at every
fixed index and checks the comparison total. Twelve capture-output controls
(empty, retained-first-only and extra row per target) fail 101 at each target's
`KUCOIN-CARDINALITY <target> source total`; all four baseline/restored targets pass.
The preload is `scripts/capture-cardinality-control.cjs`, selected by
`KUCOIN_CARDINALITY_SCRIPT` and `KUCOIN_CARDINALITY_CAPTURE`, through Node's
`--require`. Oracle source and fixture bytes are unchanged. These are capture
controls, not production collection mutations or a completed seven-shape sweep.

The maintained `scripts/check-order-cardinality.py` now proves five production
paths independently: normal book, algo book, combined book, normal Store fallback
and algo Store fallback. Each has empty, partial, extra, bypass-source-empty and
bypass-source-partial controls, for 25 compiling mutations. All fail 101 at
`KUCOIN-CARDINALITY book native category 0/1/2/3` for the affected category; each
restored target passes. The supplement fixes four inputs, per-case outputs
2/2/4/5 and 13 native comparisons, with literal IDs as well as source checks.
Production and test bytes are restored by the runner. These are output-projection
mutations after the operations execute, not shared serde visitor mutations.

The formatter extension fixes 265 inputs (235 original plus 30 labelled precision
supplements), 530 native formatter comparisons, and 120 inherited-market inputs
with 240 comparisons. Fixture values are unchanged. Six capture-output mutations
(empty, partial and extra for each target) fail at the oracle case-count assertion.
Four independent production-vector controls (empty, extra, and both with source
guards removed) compile and fail `KUCOIN-CARDINALITY formatter native payload rows`;
restoration passes. A singleton has no nonempty proper subset, so none is claimed.
The maintained runner is `scripts/check-formatter-cardinality.py`. This closes
these order-book/formatter controls only, not the whole seven-shape audit.

The balance/fills reference extension pins 14 balance captures/comparisons,
21 fill-query captures/comparisons with one call each, and three mapper captures.
The successful mapper capture has exactly one fill, one numeric-bit row and one
key row. Seventeen capture controls in `scripts/check-reference-cardinality.py`
fail at named assertions: nine outer empty/partial/extra controls and eight
nested empty/extra controls for query calls and mapper fills/bits/keys. Six
baseline tests pass and all five restoration runs pass. Original fixture and
source bytes remain unchanged; nested singleton arrays have no claimed nonempty
proper subset. These are reference captures only, not production Rust reads or
shared decoder mutations. The whole cardinality sweep remains incomplete.

The Spot order-reference slice fixes outer totals at 7/2/2/6/7 for pagination,
concurrency, discovery, later pages and raw/identity cases. Pagination compares
100 capped requests using a fixed loop; discovery compares ten symbol calls.
Literal per-case inventories pin calls and wire/bits/types row counts at return
and after sibling settlement, with five numeric fields per retained bit row.
The maintained `scripts/check-spot-order-cardinality.py` measures 36 named capture
failures: 15 outer controls and 21 nested controls (empty/partial/extra), followed
by seven restoration passes. All seven existing tests pass. Inputs and source
bytes are unchanged. This is reference-capture evidence only, not a Rust
aggregation implementation, disposal proof or a consuming shared decoder mutant.
Event traces, alias snapshots and other remaining gate families are not covered
by this bounded count audit; the completed-sweep verdict is still withheld.

The Spot concurrency trace supplement pins six complete event arrays: success,
post-return bad-code emission, and suppressed post-return transport rejection,
each at return and after settlement (43 expected events). On the pre-supplement
Rust test, reversing or same-length replacing the success at-return capture
both still passed. These are two measured false greens, not production defects.
Thirty capture controls (empty/partial/extra/reverse/replace per trace) now fail
101 at the corresponding `KUCOIN-TRACE ... event identity` assertion; three
restoration runs and the seven-test baseline pass. Host and wasm strict scoped
Clippy are measured separately. `scripts/check-order-trace-identity.py` retains
the controls; the source oracle and fixture inputs are unchanged. Assertions
compare full live event content, not an archived digest or a length-only pin.
This covers the six traces only: no Rust aggregation, disposal, alias-snapshot
completion, source-input admission or completed-cardinality-sweep claim.

The alias-array supplement compares all fields and exact singleton cardinality
for mutated Store orders, the retained result after Store replacement, and the
replacement Store orders. It retains the separate reference-identity booleans.
Nine capture controls (empty/extra/same-length null replacement for each array)
fail named `KUCOIN-ALIAS ... complete record identity` assertions; restoration
and the seven-test baseline pass. `scripts/check-order-alias-identity.py` owns
these controls. Source and input bytes are unchanged. Singleton arrays carry
no nonempty proper-subset or reversal claim. This does not cover info objects,
production Rust aggregation, disposal, or the complete cardinality inventory.

The constant-table census examines 13 native pair tables (145 pairs); all native
count and unique-key assertions pass with zero duplicate keys in those tables.
This excludes scalar constants and endpoint-list arrays. An injected duplicate
`limit` entry in futures ORDER_TYPE compiled and passed the old table target:
its native pair count increased from 12 to 13 while object construction retained
only 12 keys. This is a latent test-instrument gap, not a measured production bug.
The repair preserves `tables.cjs` and its original object projection; a separate
`table-pairs.cjs` supplement captures live ordered entries alongside it. Native
counts and unique keys are checked before any serialization or source comparison.
The maintained `scripts/check-table-native-cardinality.py` checks compiled test
failure, exit 101 and the named assertion. Its 19 controls comprise one duplicate
per table (13 count failures), ORDER_TYPE duplication with the count bypassed
(one unique-key failure), and ORDER_TYPE duplicate/empty/partial/extra/reverse
with both native guards bypassed (five ordered-pair failures). These are injected
mutations, not observed production defects. The deeper bypass proof is confined
to ORDER_TYPE; no complete seven-shape table or venue sweep is claimed.
The first complete matrix iteration finished with all 19 named failures and a
passing restored target; both production table files and the original oracle
were byte-verified unchanged. The two-test baseline (including source mutation)
also passes. Host and wasm strict Clippy are separate scoped table-target runs.


## Production order-reader cleanup (2026-10-18 SGT)

The old src/order_reads.rs implementation, its eight tests, source-only oracle
and cardinality control are retired. Earlier measurements of that copy above
are historical, not production coverage. The constructed Exchange installs
futures_native_order_reads through binding/futures.rs. Its retained component
tests remain; tests/order_reads_binding.rs additionally drives actual HTTP
requests and the real Store. Redirecting the live normal reader to the algo
endpoint compiles, leaves all eight old copy tests passing, and fails the new
binding witness at duplicated algo IDs. Exact restoration passes. The new
witness also checks ordered endpoints/params, native contract-size scaling and
failed-normal/empty-algo Store category preservation. This deletion changes no
production algorithm or shared representation policy.


## Production Spot I/O cleanup (2026-10-18 SGT)

The parallel typed spot_reads/spot_writes implementations, their four test
targets and exclusive source oracles are retired. Earlier measurements of those
copies are historical. Production already installs spot_native_order_reads
through spot_exchange_class; its native/source component cases remain. New
tests/spot_io_binding.rs drives the actual constructed Exchange, HTTP factory
and Store for market precision, distinct ticker prices and fallback, active
symbol discovery/paging and three cancellation routes with exact params. Three
compiled mutations independently change the live ticker bid field, discovery
endpoint and cancel-all endpoint. Each leaves its corresponding retired-copy
tests passing but fails the constructed witness. Exact restoration passes all
three constructed tests. This is deletion of unused code, not an algorithm
cutover or a resolution of shared typed projection/identity policy.


## Production account-copy cleanup (2026-10-18 SGT)

The unused typed accounts module and its exclusive response_default parser are
retired, with their three account/validation/notification test targets and
source-only oracles. Earlier measurements of those copies are historical. The
constructed Futures and Spot exchanges already use the retained native account
modules; their existing component/source cases remain. The paired package and CI guard now include exchange_trait in the venue oracle command. Changing only the production futures userId read
to referrerUserId compiles and leaves all eight old typed account tests passing,
but fails futures_binds_all_sixteen_defaults at empty ID versus user-1. Exact
restoration passes. The actual constructed test and its runtime fixture are
unchanged, preserving concurrent private-topic work. Canonical shared transport
parsing and all shared body/API/representation policies remain unchanged.

## Typed trading-copy retirement (2026-10-18)

Production class dispatch uses the native mapping/formatter/helper bodies. The
unused typed mappers, formatters, order_helpers and seven sizing functions are
removed with five exclusive test targets, four source oracles and formatter-only
cardinality controls. Nineteen decoder contracts owned only by those copies are
removed; all surviving contract objects are unchanged. Live typed to_api_symbol
and api_margin_coin stay, and the sizing source extractor retains the two methods
needed by ws_helpers. No native implementation or shared model API changes.

Before deletion, a compiled actual native amount mutation leaves all ten typed
mapper tests green but fails constructed order_reads_binding. A compiled native
buy/sell mutation leaves all four typed formatter tests green but fails the
constructed futures exchange_trait request payload assertion. Both witnesses pass
after exact byte restoration. Existing native/source helper, mapping/formatter,
order-read and constructed exchange targets remain wired. The Order snapshot
supplement retains every assertion while dropping an empty-Store identity call
to the retired helper. Earlier typed-copy counts and refusal findings in this
file are historical evidence, not coverage of the installed production route.
