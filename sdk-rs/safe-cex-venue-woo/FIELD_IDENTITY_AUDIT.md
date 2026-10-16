# WOO field identity audit

Baseline: `32827da93a`, 90 ported / 24 divergent / 420 pending. No status
changes are justified solely by improving an oracle. The WOO fixture and
assertion-shape sweep is recorded below; it does not establish merge-up readiness.

## Inventory scope

The initial recursive JSON scan examined all 26 owned JSON fixture files,
25,824 object records. It found 3,302 records with repeated same-typed scalar
values across 22 files (4,615 equal-value groups). Those are candidates, including metadata, not proven
production defects. `scripts/field-identity-inventory.py` emits exact JSON
pointers and field groups with `--json`. Its group count differs from the
record count because one record may contain multiple repeated groups. It does
not infer types for null/containers, or replace the separate inline-corpus and
native-assertion review.

## First repair: declaration feeders

`types.json`, `contracts.json`, and `public-records.json` feed WOO's
`numeric_ingress` test; the first two also feed `record_calls`, and contracts
feeds literal audits. A repository reference scan found no direct consumer
outside the WOO crate. Root CI invokes their WOO freshness oracles and Cargo
consumers. The original three artifacts are nevertheless preserved byte for
byte. The separately labeled `--field-identity` oracle mode emits supplemental
inputs in memory. No generated corpus dump is committed. These supplements are
post-audit evidence, not part of the original capture or a retroactive
independence claim.

The original account/alias post-decode swap COMPILED and the legacy declaration
test PASSED. Both fields held the same value. Distinct string/number samples and
per-field boolean cases now supplement all seven legacy and sixteen V3 contract
declarations plus six public records. Fixed literal-false migration gates stay
false; changing a required literal is not a legal field-identity witness.

`tests/types.rs` and `tests/v3_contracts.rs` retain roundtrip/acceptance assertions;
these are not native field identity proofs. The public-record shape test checks acceptance; native public fields are
checked independently in `v3_field_identity`. A proposed re-encoding assertion
was removed after it rejected equivalent integer/f64 spellings; it was never
evidence of native identity. `tests/field_identity.rs` separately traverses all
15 legacy native record types through seven outer response entries, directly
reading each field. It recursively follows nested records and rows rather than
checking inner types in isolation. f64 comparisons use bits. The explicit
wire-key assertions are fixed test code, not derived from Serde attributes at
runtime; a changed attribute cannot rewrite the expectation.

Six public records now have direct native-field checks, including distinct bid
and ask levels with separate price/amount/total values. V3 request flags and
optional strings, PageMeta and nested Rows metadata have native assertions.
The legacy fills query and output parameters also have distinct native samples.

All 12 V3 contract records now have direct checks for all 82 declared fields,
including explicit native enum matches and the nested generic payloads.
Fixture collision dispositions below are distinct from that record assertion
coverage. The numeric-ingress
gate already reads individual native f64 fields after independently replacing
one wire field; it consumes the original seeds and now also lists supplements.
Do not infer string/bool identity coverage from its numeric assertions.

## Architecture / NaN follow-through

New supplements were generated on darwin arm64, Node v24.20.0 (measured via
process.platform/process.arch/process.version). This does not retroactively
establish architecture metadata for every older artifact. Initial JSON scan
found semantic NaN tags in pricing.json, response.json and private-timestamps.json,
and no raw 16-hex NaN payloads. Capture-function and inline-bit-corpus results, signed-zero/finite controls
and header provenance are recorded in NUMBER_CAPTURE_AUDIT.md.

Original artifact SHA-256 (verified against committed bytes before this repair):

- types.json: `d3b3d7c8264d38bc456cbc7fd8e4d03ec206d207510f825eec552a061ab2fac1`
- contracts.json: `44a2a27ade047f2a6a430de476b0da3d2fc3002af35c177526c73aa42d356695`
- public-records.json: `591e5943df17470b15fc5f988921e2c5a9a9d5e26cb4b7db36697e2a116a25eb`

The first scoped positive run passes 13 tests: one native-identity, five numeric-ingress, one record-call rejection, one legacy declaration, two V3 contracts,
and three public decoder/mapper tests. This is not a full venue replay.

The first two compiling controls fail the intended native assertions:
`AccountResponse.account native field identity` for the post-decode string swap
and `AccountResponse.account_mode native field identity` for the symmetric
Serde wire-name swap. The numeric swap instead fails the earlier shared
`numeric ingress changed Number bits at $.totalCollateral` guard. That
interception does not establish sensitivity of the later native assertion; the
runner records the actual failure rather than relabeling it.

Two V3 post-decode swaps also compile and fail by name: futures index/mark at
`index_price native field identity`, and request reduce/post-only flags at
`reduce_only native field identity`. Original fixture bytes are not changed.
Referenced enum fields additionally receive per-field union-arm supplement
cases, so the two migration HTTP-method fields are not always both GET.

## Gate assertion-shape ledger

This distinguishes the assertion mechanism from completion of the value census.
A native assertion on one field does not establish coverage for its siblings.

| Gate | Assertion shape / remaining identity work |
| --- | --- |
| constants | Projects named native constants/tables, with no record decode/re-encode cycle. |
| journal | Retains native-record comparison and adds direct field assertions against source JSON without decoding the expected Execution. |
| legacy_fills | Existing request transformation compares re-encoded params. New direct query and params assertions cover all five fields each. |
| literal_calls | Canonical accepted-set checks; intentionally does not claim record field identity. |
| numeric_ingress | Individual native numeric field bits. Equal-valued nested Rows/book witnesses now have distinct-value companion checks. |
| order_history | Every successful historical mapper output now has native field assertions before the retained re-encoded comparison. |
| orders | Ordinary limit/algo outputs now assert every native field before serialization; numeric bits, alias, error and clock witnesses remain independent. |
| pricing | Direct scalar function results, with semantic NaN/signed-zero encoding. Distinct baseline and fallback cases are recorded below. |
| private_socket | Direct timestamp results and observable send/diagnostic effects; collision dispositions are recorded below. |
| public_heartbeats and support dispatch | Frames, calls, diagnostics and latency; collision dispositions are recorded below. |
| support public_writes | Direct named ticker fields before serialization; ordinary samples distinguish all writer fields. Repeated edge-value cases remain as numeric semantics witnesses. |
| record_calls | Map-only acceptance/rejection, not field identity. |
| response | Direct dynamic helper outputs; collision dispositions are recorded below. |
| signing and support factory_wire | Hash preimages and actual adapter wire output; these test the requested wire boundary, not a record roundtrip. No equal scalar sibling candidates; the two signed/wire views stay separate. |
| types | Roundtrip/acceptance retained, supplemented with native assertions across all 15 legacy record types. |
| utils | Direct scalar/string/list helper output, not a typed record roundtrip. |
| v3_contracts | Roundtrip/acceptance retained; all 12 records and 82 fields have direct native assertions, including enum variants. |
| v3_public | Six native-record checks plus all four mapper families inspecting native outputs against a separate live pre-serialization bit capture. |

The two new native-identity targets supplement existing gates; no earlier
roundtrip result is relabeled as having established field identity. This ledger
also leaves the shared transport oracles outside this WOO-only inventory.

Restored changed-target verification: 11 tests pass across legacy_fills,
v3_contracts, v3_field_identity and v3_public. The earlier native
legacy, numeric-ingress and record-call targets also passed; no full venue suite
was replayed. Original freshness checks pass at 242 legacy declaration cases,
273 contract cases and 2,184 public behavior cases using the existing worktree
TypeScript install. This is not clean-install or CI evidence.

Scoped strict Clippy passed separately on the host and wasm32-unknown-unknown,
both with --all-targets, --locked, --offline and --jobs 1. No browser execution
or CI result is claimed. Those historical lint results are not rerun claims for this extension; no merge-up readiness is claimed.


## Contract and mapper extension

The contract supplement's migration-method and balance-string swaps compile
and fail at `WooV3MigrationRoute.legacy_method native field identity` and
`WooV3Balance.holding native field identity`. The mapper high/low and limit
id/symbol swaps compile and fail at `high native field identity` and
`Order.id native field identity`.

The stronger mapper assertion exposed a limitation of the old artifact:
JSON.stringify erased a negative-zero percentage24h. The new `--mapper-identity`
mode captures every numeric output before serialization and canonicalizes only
NaN, retaining signed zero and every other bit. It runs once for the entire
mapper corpus and labels platform, architecture and Node version. Original
fixture bytes remain untouched; their historical green is not upgraded.
The separately declared NaN candle-sort permutation remains outside this
ordinary mapper identity comparison and retains its existing dedicated witness.

## Equal-value candidate dispositions

The 4,615 groups are not 4,615 defects. Original corpora retain their edge cases;
identity is pinned by distinct companions or one-field variations, not by deleting
the repeated values. No original artifact is regenerated by this extension.

| Artifacts | Candidate disposition |
| --- | --- |
| types, contracts, public-records | Declaration samples repeated field values; the three labeled live supplements differentiate fields, and native assertions now cover legacy, V3 contracts and public records. |
| fills | Zero fromMilliseconds/page edge retained; the distinct query/params companion reads all five native fields. |
| history | The source oracle changes each input field independently over its value set. Native HistoricalOrder assertions now prevent the output serializer from concealing field mismatches. |
| journal | Filled/amount and price/averagePrice have independent source variations. Native Execution assertions no longer rely solely on deserializing expected records. id/orderId equality is source-mandated: both are the same source order.id. |
| public | Independent one-field decoder/mapper variations already exist. Direct mapped fields now use a separate live bit capture; equal index/open values in the baseline are distinguished by the one-field variants. |
| order-output, order-numbers | Lookup-key and numeric fields have independent variations. Direct Order assertions cover successful outputs. amount/remaining equality when filled is zero is an arithmetic edge, with nonzero-filled cases retained separately. |
| algo-batch-priority, algo-timing, order-alias | Purpose-specific ordering/alias witnesses, not field-identity evidence. The ordinary and numeric order corpora supply independent field cases; these clock and alias artifacts remain unchanged. |
| pricing | Equal mark/index/last is the all-zero fallback edge. Distinct 100/102/103/104 baseline and fallback-removal cases exercise price-source priority independently. Outputs are scalar, not roundtripped records. |
| public-writes | Equal bid/ask and last/volume/quoteVolume occur in numeric edge witnesses. Distinct ordinary writer inputs plus named native ticker observations and field-routing mutations provide identity coverage. id/symbol is the fixture store's lookup identity, not two independently mapped output fields. |
| public-heartbeats, private-heartbeats | disposed/fail/diagnostics are separate input dimensions, including unequal combinations. Frames, clock changes, diagnostic counts and reconnect counts are independently asserted. |
| public-dispatch | own/had are input ownership and observed membership; equality is the behavior being checked, not a decoded field pair. |
| private-timestamps, utils, history-assets | input/output or lexeme/asset equality records scalar identity behavior; no multi-field record decoder. |
| market-alias | mutatedSymbol/sourceAfter equality is the alias witness itself, paired with the unchanged snapshotAfter view. |
| constants | Repeated inherited-member classification tags are lookup results; canonical table entries are checked independently. |
| factory-options-alias, fills-schema, response, signing | No equal scalar sibling candidate in the inventory. Signing/factory asserts actual hash preimages and adapter output, not a decode/encode roundtrip. |

This disposition is about this committed corpus and its live companion paths,
not a claim that equal-valued fixtures are intrinsically safe. Shared transport
artifacts are outside this WOO audit. No direct external WOO-fixture consumer was
found by the earlier repository reference scan; originals were preserved anyway.


The extension adds native assertions to the remaining five gate families:
V3 contracts, V3 public mappers, legacy limit/algo mapping, historical mapping,
and journal projection. Earlier legacy declarations and fills supplements remain.
No original fixture record was edited; improvements are companion assertions and
live supplements, not 3,302 relabeled defects. Six new compiling controls cover
contract method/string swaps, candle high/low, limit id/symbol, history price/
average price, and journal price/quantity. Each fails its named native assertion.

The algo timing earlier-row repair already landed at 32827da93a. This extension's
restored orders target passes all six tests, including the 12-case 2/1/0 clock
witness and whole-filter ordering. Its prior five mutation proofs are historical
evidence from that repair, not newly replayed controls. Primitive RHS message
parity remains a routed shared get-key FINDING; no local in-operator shim exists.

Read-only sub-agent review at `ccfd79d1c5` independently enumerated all 12/82
contract fields and the four mapper output families, checked the saved named
swap-control logs, and found no remaining omission in that requested scope.
The review also inspected the 12-case algo timing witness and five historical
control logs, confirming whole-filter ordering and 2/1/0 clock observations.
It ran no Cargo tests and did not relabel earlier verification as a fresh replay.
