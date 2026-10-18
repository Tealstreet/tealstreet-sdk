# Builder contract review

This batch reviews twelve original native/default census owners: four concrete
builder records and eight pure helper functions. It adds twelve individual
runtime contracts, preserving all previously reviewed contracts. It makes no
production Rust change and adds no policy classification or exemption.

The maintained references are `asterdex.builder-fee.ts` and
`asterdex.builder-approval.ts`. The governing boundaries remain
[strings](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[typed outputs](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary),
[snapshots](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias) and
[numbers](../parity/MANIFEST-SPEC.md#numeric-ingress-defect).

| Concrete owner | Reviewed native behavior |
| --- | --- |
| `AsterBuilderFeeConfig` | Three required strings, preserving empty address and unnormalized fee text; decoding adds no address or cap validation. |
| `AsterBuilderApprovalCheckResult` | Boolean, finite status variant and address string; optional error/max-fee fields preserve omission and present empty strings while refusing present null. No extra relationship is imposed between the boolean and status. |
| `AsterBuilderApprovalResult` | Required message string and success boolean, retaining empty text and false. |
| `AsterBuilderApprovalStatus` | Exactly four string tags, observing the actual named native variants and refusing each valid tag's object form. |

Record observations call owned Value, borrowed qualified Deserialize, JSON text
and a qualified JSON-text deserializer. Every accepted path checks all declared
native fields or actual enum variants and actual Clone equality. Complete
positional arrays use each existing Serde declaration's order, including present
optional values. The public data fields introduce no private-construction gate;
fresh compiler inventory must confirm that rather than adding unnecessary policy
rows. Ordinary lexical `Wire` decoders retain independent local obligations.

| Helper | Reviewed behavior |
| --- | --- |
| `get_base_url` | JS trim, endpoint fallback and removal of one trailing slash. |
| `extract_builder_rows` | Outer/data array choice, preserving entries and order without filtering. |
| `parse_fee_rate` | Number coercion, finite filtering, signed-zero bits and coercion-error class. |
| `get_provider_error_code` | Nonnull object with a numeric code, observing native binary64 bits. |
| `get_provider_error_message` | Preserved Error message, string selection, object message and fallback, including empty messages. |
| `parse_scaled_decimal` | Canonical unsigned decimal syntax, eighteen-place limit and exact BigInt result. |
| `normalize_decimal_string` | JS trim, decimal validation, normalized zero and trailing-zero removal. |
| `assert_fee_rate` | Decimal validation before the exact cap, retaining field-specific error text. |

The helper contracts contain exact compiler APIs and 55 fixed expectations
authored after reviewing these bodies. JSON adapters call the existing typed
functions; they add no production raw-input API. The source checker executes
the maintained TS bodies against these expectations. It also checks the four
erased TS schemas against 27 independently authored record inputs. These schema
checks do not claim that TypeScript interfaces provide runtime decoders.

`scripts/accepted-builder-contracts.mjs` checks source expectations, performs
fresh compiler discovery, uses the split `loadContracts()`/`loadPolicy()`
loaders, and runs canonical production-native probes for precisely these twelve
owners. It requires runtime execution and zero classification-only receipts.

`scripts/accepted-builder-controls.py` compiles six isolated faults: the three
object guards removed separately, the status String front door removed, loss
of a present optional error string, and refusal at the exact supported fee cap.
Each fault must fail its exact named owners at native output assertions. The
status fault is also caught by the approval record's nested tagged-map witness;
its required failure set contains both owners. Each other fault fails one owner.
Compiler errors and stale result files are rejected. Every exact-byte restoration must
pass all twelve owners. Run the scripts serially with one Cargo job.

This scope excludes other census owners, aliases, local owners, features and
WASM contract execution. After this batch, the original Aster census still has
154 owners and eight aliases to review; the four-venue queue item remains open.
