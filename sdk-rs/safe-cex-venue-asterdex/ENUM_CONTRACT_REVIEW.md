# Accepted enum contracts

This batch selects eleven enum owners from the original native/default Aster
census: API and FAPI versions, STP mode, the position-mode string alternatives,
the boolean/string position-mode union and the six public/private stream event
tags. The existing ten string decoders already have concrete String frontdoors.
This review adds evidence and contracts without changing production behavior.

The 173 expectations are independently authored JSON text fixtures. Each actual
native owner is decoded through owned Value, borrowed qualified Deserialize,
raw text and qualified raw-text Deserialize. Native variant, ordinal and actual
Clone identity are observed; the union also observes boolean/string payload
identity. Decoded owners are never serialized to derive expected observations.
Single-tag event enums use canonical and whitespace-surrounded raw text as two
distinct successful inputs, following the maintained enum witness pattern.

Every valid string tag has an explicit forbidden `{tag:null}` witness. Wrong
case, whitespace inside the tag, arrays, booleans, numbers and other objects
remain refused. The union accepts exactly native booleans and the two lowercase
string tags, and refuses both Serde-style variant maps and maps named by the
valid string tags. Its native Boolean/String branches remain distinguishable.

`scripts/accepted-enum-source.cjs` checks every fixed input against the actual
maintained TS type schema. It uses the existing schema reader, with the exact
complete tag sets checked against the independently reviewed literals. The
position-mode schemas come from the actual parent field union, rather than the
older oracle's hardcoded nested aliases. These are erased TS declaration
witnesses, not a claim that TypeScript performs runtime input validation.

`scripts/accepted-enum-contracts.mjs` runs fresh native/default compiler
discovery and canonical `runProbes` with one Cargo job. It requires exactly the
eleven selected enum owners and eleven actual native runtime receipts, rejecting
classification-only results. Existing contract/policy shards are loaded through
the maintained readers. No receiver, construction or shared-helper permission
is added, and ordinary lexical `Wire` owners remain separate obligations.

`scripts/accepted-enum-controls.py` compiles one explicitly grouped mutation:
only the ten String frontdoors are removed, feeding each original deserializer
to its same inner Wire. Each of the ten own enum probes must separately fail a
direct native observation; the dependent position-mode union must fail too.
The control requires exactly those eleven failed owner receipts and refuses
compiler errors as evidence. Exact-byte restoration must pass all eleven owners.
This grouped control does not claim ten isolated mutation builds.

The measured grouped mutation reached all eleven native owner assertions with
zero classification-only receipts. Each owner reported a direct native
observation mismatch for a formerly refused tagged map. For example, API version
`{"v1":null}` changed from refusal to native `V1`/ordinal zero. The assertion
failure followed actual Rust execution; no compile failure counted as evidence.
Exact-byte restoration passed all eleven owners again, with zero failed owners
and zero classification-only receipts. All 173 maintained-source shape
observations also passed before baseline, mutation and restoration.

Master `68af3fa734` is integrated after decision PR #360 merged. All 46 existing
Aster contracts are preserved beside these eleven additions (57 total). The
fresh native refresh passed eleven runtime owners with zero failures or
classifications. The frozen install and whitespace checks passed;
`parity:check` passed 7,459 rows and 17,214 public Rust items with zero findings.
The selected enum bodies and native probe emitter remain unchanged.

The scope is the selected native/default enum owners under the canonical
[string boundary](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary) and
[divergence taxonomy](../parity/MANIFEST-SPEC.md#divergence-taxonomy). It excludes
parent records, local decoders, other owners, aliases and feature/WASM coverage.
After the prior thirty-eight owner contracts (including merged decision #360),
this batch leaves 135 original Aster owners. Queue ddi2il-32 remains open.
