# Accepted wire record contracts

This batch selects twenty ordinary record owners from the original Aster
native/default census. All twenty concrete Deserialize bodies and their TS
declarations were reviewed. These are the ordinary records in `types.rs`,
excluding the six flattened records, the kline tuple and the PlaceOrderOpts
intersection. No production behavior, dependency or shared policy changes.

The reviewed records contain 184 declared native fields, including forty direct
numeric fields. Independently authored examples use distinct strings and
decimal numbers, native booleans, populated/empty vectors and complete nested
account/order payloads. A second full example supplies raw negative zero for
every numeric field. Optional auth, open-interest and user-trade fields have
explicit omission and null witnesses. All four position-mode union alternatives
are observed. The single-field listen-key owner observes the actual string and
its UTF-8 byte length, including an empty valid string.

All 707 expected observations were authored independently of the Rust decoder
and the source oracle outputs. Each native case decodes the concrete production
owner through owned Value, borrowed qualified Deserialize, direct raw JSON text
and qualified raw-text Deserialize. Every actual field is observed, with numeric
bits, enum variant/payload identity and nested data projected directly. Actual
Clone is projected again and must preserve all fields, presence and bits. No
decoded owner is serialized to construct its expected observation.

`scripts/accepted-record-source.cjs` checks each fixed input against the actual
maintained TS schema and checks that the sample covers every source field in
declaration order. It also checks Node JSON.parse retains the negative-zero
fixture signs. These are erased TS declaration witnesses, not runtime TypeScript
input validation. Unknown-property retention is outside these ordinary modeled
records; the flattened records keep their separate native extension obligations.

Each owner has an explicit complete, valid positional array containing every
field in reviewed Serde declaration order. It must be refused on all four native
routes. Required-field omission, null fields, wrong kinds and nonobject outer
shapes also have explicit cases. The numeric examples execute direct raw-text
decoding under the workspace's arbitrary_precision configuration, rather than
depending only on an owned Value path.

`scripts/accepted-record-contracts.mjs` runs fresh native/default rustdoc and
canonical production probes with one Cargo job. It compares the complete fresh
compiler field list to the native-observation field list before probing, so new
fields cannot silently inherit incomplete evidence. It requires exactly twenty
native runtime owners and refuses classification-only receipts. All selected
fields are public, so no construction permission is added. Existing policy and
contract shards are loaded through the maintained readers.

`scripts/accepted-record-controls.py` compiles one explicitly grouped mutation:
only the twenty concrete object guards are removed. It requires every owner's
own complete-array case to fail a direct native observation, with exactly twenty
named failed owners. Compiler errors do not count. Exact source bytes are
restored and all twenty owners must pass again. Each run's native receipts and
logs are retained outside the checkout. This is one grouped build, not twenty
isolated builds.

The measured baseline passed twenty runtime owners, zero failures and zero
classification-only receipts. Removing the twenty guards produced exactly
twenty named failed owners, each with a direct native mismatch for its own
complete valid positional array. Exact-byte restoration passed all twenty again.
All 707 maintained-source shape observations passed before every native run.
The per-run JSON receipts preserve every owner's individual failures.

After integrating master `4b7d081d69`, all 57 master contracts remain unchanged
beside these twenty additions (77 total). The final source/native refresh passed
20 owners / 707 observations, twenty runtime receipts and zero classifications
or failures. Frozen install, script syntax and whitespace checks passed;
`parity:check` passed 7,459 rows and 17,214 public Rust items with zero findings.
Enum PR #366 is merged and its eleven rows are preserved.

The scope remains twenty native/default owners under the canonical
[string boundary](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[typed output boundary](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary)
and [numeric requirement](../parity/MANIFEST-SPEC.md#numeric-ingress-defect).
Ordinary lexical Wire decoders and the generic present helper remain separate
obligations. This does not cover the flattened records, other owners, aliases,
features or WASM. After the prior batches, these twenty leave 115 original Aster
owners for further review. Queue ddi2il-32 remains open.
