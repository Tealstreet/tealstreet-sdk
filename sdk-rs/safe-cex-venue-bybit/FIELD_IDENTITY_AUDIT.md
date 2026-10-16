# Bybit fixture and decoded-field identity

The October 13 sweep covers 36 JavaScript oracle scripts and 50 Rust consumer/test
files (7 examples, 4 integration tests, 39 source/test files). The static scan
found 23 repeated literal-value groups in the 33 scripts outside the shape,
numeric-ingress and table-census entrypoints. Dynamic generator and consumer
inspection found the request params/data alias that the literal scan missed.
This is a field-pair inventory, not an exhaustive permutation proof.

## Fixture values

| Corpus | Equal fields and observation | Repair |
|---|---|---|
| `check-shapes` | Nested populated arrays were absent when sibling fields varied. | Preserve the 269 original cases byte-identically; append 170 labeled identity cases. Original corpus: 347 equal scalar-path pairs, 100 without an identity witness. Supplemented corpus: 627 pairs across eight source declarations, all distinguished; 280 now have a witness absent from the original corpus. |
| `numeric-ingress-fixtures` | Baseline numbers repeat, but each of 33 native paths independently receives nine lexemes. | Existing 297 path-specific cases already distinguish numeric identities; no numeric-value replacement. |
| `check-order-format` | triggerType/slTriggerType/tpTriggerType shared their selector in every supplied case. | Add 27 conflicting selector cases, including both stopLoss and takeProfit. |
| `check-exchange-state` | entryPrice/notional=10, unrealizedPnl/liquidationPrice=0, eleven cached ticker fields=0, cached id/symbol identical. | Differentiate these local-only fixtures; inspect six decoded position numbers, eleven cached ticker numbers and both records’ id/symbol before processing. |
| `request-fixtures` | params and data referenced the same object. | Add queryOnly=17 versus bodyOnly=29 through every method/route/signing mode; compare signed payload and transmitted body separately. |
| `check-order-params` | Shared optional fields often absent together. | Add twelve cases with distinct numbers, strings and trigger selectors and all reduceOnly/close combinations; inspect all sixteen native shared fields and extension payloads. |

The source-dependent supplement is not independent reference evidence and does
not retroactively strengthen a green result obtained from the original corpus.
`check-shapes --original-cases` exposes the unchanged original sequence;
`--identity-audit` emits the pair inventory and witness indices. Arrays include
singleton and reversed samples, so positions within arrays are distinguishable.

The shape generator feeds `numeric-ingress-fixtures` and the public record
accepted-set test **inside this crate**. A tracked exact-path and basename
consumer census found no executable consumer outside this crate: external
matches were manifest/documentation references and another venue's own
same-named script. No external feeder artifact was rewritten. Other local-only
fixtures above are edited in place.

Repeated values retained elsewhere have distinct per-field observations: ticker
events vary one field against unique initial values; candles use distinct OHLCV;
journal exit price/value vary independently; inverse-position Store fixtures
use distinct consumed mark/last values while their unused ticker fields remain
zero. Empty credentials have independent key/secret cases. Repeated protocol
table values are source data, not invented distinguishable fixtures.

## Assertion shape

| Gates / consumers | Observation after this repair |
|---|---|
| Shape records | Native checks on all fields of six record types, recursively through permissions and rate categories, before the retained roundtrip. |
| Union inventory / qualified public decoders | Accepted-set checks remain; 53 explicit spelling-to-native-variant assertions cover seven local enums. |
| Order-params | Native base and Plain/Bbo/TickSize fields before derived output and roundtrip. |
| Exchange-state | Native decoded position/cache fields before dedupe or ticker parsing. |
| Order-format | Native trigger fields before request formatting; conflicting selectors also compare resulting requests. |
| ResponseMeta / execution amounts / execution params | Direct decoded fields, including opposite response booleans, distinct price/amount and distinct execution strings. |
| Numeric ingress | Native field projections and binary64 bits on raw, owned and borrowed paths. |
| Tables/helpers, responses | Static data or functions consuming raw Value; no typed decode/reencode identity cancellation. |
| Inverse/market/order/history mapping | Raw Value inputs; output records compared against source, with numeric classifications/projections. Output-only comparisons are not claimed as decoded-input identity proofs. |
| Book/candle/journal/fill | Explicit native projections and state transitions. |
| REST fetch/submission | Request traces, errors, cancellation, result pairing; serialized result comparisons are not decoded-field proofs. |
| WebSocket/auth/dial/reconnect/constructor/getters | Native state, callback effects, frames, ownership and cancellation. |

Five previously acceptance/roundtrip-only or partially semantic gate families
now have native identity checks: shapes, unions, order-params, exchange-state,
and order-format. The response/execution record supplement extends the union
public-decoder gate rather than counting a sixth gate.

## NaN capture

Three bit-capture corpora exist: constructor scalar defaults (12 cases), numeric
ingress (297 cases), and execution windows (222 cases). Constructor capture now
asserts finite defaults. The nine numeric JSON spellings can yield finite
numbers or infinities, never NaN; capture asserts that invariant. Both retain
bits unchanged. Each live capture records `process.arch` in its metadata.

Execution windows includes `['NaN', null]`, which returns an unbounded NaN
endpoint. Its JavaScript capture and Rust comparison canonicalize **only NaN**
to `7ff8000000000000`. Every finite value, both infinities and signed zero retain
their bits. Local captures were arm64; CI captures record their own architecture
rather than inheriting that label. Marker-based mappers use `NaN` classification
already, not a NaN payload snapshot.

Named capture controls are `capture preserves signed zero` and
`capture preserves finite payload`; replacing zero's sign or a finite payload
must fail before output. A negative-NaN assertion checks the canonical payload.

## Executed controls

All Rust mutations below compiled, failed at the named observation and were
restored to passing. Temporary mutant source is not retained.

- Symmetric `id`/`note` serde renames: `native field identity BybitApiKeyInfo.id`.
  Both decoding and encoding were swapped, so the old roundtrip alone would
  undo this corruption.
- SL/TP selectors swapped after outer option decoding:
  `formatter decoded trigger field identity`.
- Decoded entry_price/notional swap: `native position identity entry_price`.
- Decoded cached bid/ask swap: `native cached ticker identity bid`.
- Decoded reduce_only/close swap: `native reduceOnly`.
- Public Trade/Settlement conversion and reverse serializer arms swapped:
  `native variant identity: BybitTransactionType TRADE`.
- GET signing uses body with the same query encoding and request-id insertion:
  `payload case 16`, the first new conflicting params/data case. The first
  candidate left an unused variable and did not compile; it is not counted.
  Reversing the HTTP-method branch also failed, but that changes encoding and
  is not the claimed field-selection control.

The two JavaScript capture mutations independently collapsed signed zero and
changed finite payloads; both failed their named capture assertions and the
restored capture passed. The original shape sequence was compared byte-for-byte
against HEAD, not merely counted or parsed for semantic equality.

## Market precision axes

At 0b14758456, four production adjust sites in order_format.rs feed five fields:
price, triggerPrice, qty, and the shared stopLoss/takeProfit loop. Five independent
production mutations compiled against the existing 548-case formatter target.
The stopLoss-only price-to-amount mutation passed: a REAL FALSE GREEN. The price,
triggerPrice, takeProfit and qty swaps failed the existing comparison: ALREADY
COVERED by that current corpus only. That corpus included earlier field-identity
work; this is not evidence that the pre-sweep baseline was never blind.

The repair retains all 548 original inputs byte-identically (318645 JSON bytes;
SHA-256 cb3c61eb5bf9b1f31526d02b0fdf3c994210deaa315c9cd4b52f0be701dd360b).
A permanent hash assertion precedes all source evaluation. No existing fixture
values changed. Thirty-two labeled cases supplement them, giving 580 total:
price/amount steps 0.1/1, 0.05/0.001, 0.25/0.01 and 0.001/0.1; both sides;
limit, conditional, stop-market and take-profit-market. Every present field's
source adjustment is asserted to differ under the wrong step. The consumer reads
both native decoded precision fields and all five native payload fields before
serialization. The source-derived supplement makes no independence claim.

The exact-path consumer census found only this crate's live_data target consuming
check-order-format; it feeds no external gate. One Node invocation processes the
whole corpus and invokes one Rust example, rather than spawning per case. Capture
output records architecture and Node version (measured arm64, Node 24.20.0).

After supplementing, all five independent production swaps compiled and the Rust
example exited 101 at BYBIT-PRECISION price, triggerPrice, qty, stopLoss and
takeProfit respectively, each labeled precision:0.1/1:buy:limit. The outer Node
runner exits 1 when its Rust child exits 101. Production source was restored and
the 580-case target passed. These are controls over implemented formatting only;
new bracket formatters must add fractional distinct-step native payload cases
before claiming coverage. No parity status is promoted by this test repair.

The follow-up earlier-row filter parsed all 106 ported venue rows and checked
production market precision reads plus adjust calls. All five market-step output
fields belong to format_order_for_batch, whose row is already divergent. The two
ported helpers in that file select trigger names and ticker prices without reading
market precision; inverse order mapping adjusts at a fixed 1e-7, not either market
axis. No additional ported market-step consumer was found and no row was
reclassified. This does not claim pending formatters are swap-proof.

## Cardinality audit in progress

This is a targeted census and mutation record, not a whole-venue clean claim.
Five Rust zip sites were inspected: candle state, book state, ticker numeric
fields, journal buckets, and history numeric serialization. Indexed JS response
and shape consumers were inspected separately. Source-total and nested-corpus
inspection additionally covers the touched helper, data, formatting, union,
request, constructor, auth, dial, fill, error and public-WS tests.

Measured prior false greens: empty outcomes and a removed tail outcome passed
all 1236 candle cases and all 987 book cases; deleting the ticker key tail passed
both ticker tests including 1458 live cases. Candle/book now evaluate all input
frames independently, pin source totals 2172/1746 and each input/outcome length,
then compare independent native lengths and exact comparison totals. Empty and
subset source corpora or outcome arrays fail their source-total assertions.
Bypassing both source guards still fails expected/native case 0 for each target.
Ticker now pins all eleven ordered keys, 1458 cases, independent native field
length and 16038 comparisons. Empty/subset keys and source cases fail their
named guards. Journal and history already rejected compiling production empty
and subset mutations: bucket count and complete result equality respectively.

Shape and response indexed walks now check source totals 439/8488 and native
length equality. Compiling native example empty, subset and extra output
mutations fail SHAPES/RESPONSES-CARDINALITY expected/native outputs. These two
walks previously lacked rejection of extra outputs by inspection; this record
does not claim a prior extra-output survivor was executed.

Fixed source totals also cover origins63495; helper configs45/messages1030/sites91;
position cases1041/ticker cases3213; order-params6588. Each new JS total was proved
with empty and subset source mutations, failing its named source guard. No corpus
values were rewritten. Shape records still feed the numeric and record gates,
and their generator output is unchanged: guards do not rewrite historical bytes.

Constructor subloops now use independent fixed bounds16/4/5 for statuses,
placements and reinitializations; expected per-case lengths and comparison
totals80/20/25 accompany the five successful and seven failed constructors.
A misplaced dial720 assertion in remote-auth1150 was caught in review and moved
to its actual dial target; both targets then passed. Remaining source guard
controls and review are in progress, so this document is not merge readiness.


The constructor controls now independently empty and shorten each of its three
subcorpora: all six fail their named source-length assertion. With those guards
removed, an empty statuses, placements or reinitializations array still fails
its corresponding CONSTRUCTOR-CARDINALITY native index coverage assertion.
All nine return Rust test exit 101; the restored constructor passes 1/0/0.
These are fixture-splice controls, not consuming-deserializer controls.

Further empty/subset source controls return 101 at each new guard: four scalar
corpora (fills, error messages, requests and dial), six public-WS subcorpora,
local-auth, expiry and category-symbol subcorpora, and all eleven union case
sets. That is 48 independent source mutations, with each script restored after
its pair. The six positive JS gates (data, helpers, exchange-state, order-params,
shapes and responses) exit zero. No historical corpus bytes were regenerated.

## Consuming collection controls

The initial ContractTrade-only probe has now been extended to ten independent
fields: ContractTrade, Spot, Wallet, Options, Derivatives, CopyTrading, BlockTrade,
Exchange, NFT and ApiKeyInfo.ips. The permanent runner is
scripts/check-consuming-collections.mjs; optional arguments select native field
names for a focused rerun. Each mutation changes only that private wire field,
using a SeqAccess visitor that drains every element as IgnoredAny before returning
an empty vector. Shared production files and corpus inputs remain unchanged.

All ten mutations compile and fail the selected native field identity assertion:
BybitApiKeyPermissions.<field>, or BybitApiKeyInfo.ips. The Rust example returns
101 and the outer Node runner returns 1. For each field independently, retaining
the consuming decoder while removing only its native equality assertion still
fails the outer comparison. The runner requires both the affected record's
sample label and the selected array's added-empty diff line, not a field name
that could occur unchanged elsewhere in the printed record.

These fields have native whole-array equality and outer whole-record equality,
not standalone length checks or element zips. The ten guard-removal outcomes
prove redundant independent coverage; they do not prove either guard alone is
indispensable. No fixture-value repair was necessary. This is evidence about
the current identity corpus, not a claim that earlier corpora were never blind.

The strengthened runner exits zero after twenty controls and a restored positive
439-case shape run. Source and example are restored byte-identically in finally;
process termination is not covered by that restoration guarantee. Run only in an
otherwise idle owner worktree. Review caught and tightened an initially weak
outer-failure check that merely searched for the field name; the whole twenty-
control run was repeated after requiring the changed empty-array diff line.

This closes consuming-drop proof for these ten typed permissions/IP collection
fields only. It does not establish the same property for every Value-based book,
candle, fill or ticker path elsewhere in the venue. Corpus preservation and the
shape generator's external numeric/record consumers are unchanged.
