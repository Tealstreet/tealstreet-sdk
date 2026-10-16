# WOO Number capture audit

This is the capture audit, not a claim that every fixture has field-identity
coverage. Original artifacts are preserved. No old architecture is inferred
from the current machine; historical JSON captures have no architecture header.
The new in-memory field-identity and Number supplements carry platform,
architecture and Node version in their capture header.

## Bit capture sites

| Gate / corpus | NaN handling and scope |
| --- | --- |
| orders / order-numbers.json | Source `bits` and Rust native observation already canonicalize every NaN to the semantic string `NaN`; all non-NaNs retain hexadecimal bits. |
| public writers / public-writes.json | Raw DataView capture could retain host NaN payloads. It now calls `number-bits.cjs`; the Rust native observer uses the matching test helper. Only NaN becomes `7ff8000000000000`. All 87 existing source cases remain byte-identical. |
| numeric_ingress | Raw Number corpus is exactly `-0`, `-0.0`, `1e400`, `-1e400`, `-1e-4000`, plus native negative zero. These parse to zeros or infinities, never NaN. Individual decoded native fields are compared by bits. |
| field_identity / v3_field_identity | Source declaration samples are finite distinct numbers and strings. Native numeric comparisons remain bit-exact. |
| private_socket / private-timestamps.json | One native-NaN input has the semantic `NaN` tag and is explicitly excluded from JSON-input parity; successful timestamp results are finite. |
| pricing / pricing.json | Source and Rust encode NaN as the semantic `NaN` tag; signed zero has its own `-0` tag. |
| response / response.json | Source uses semantic `NaN` and `-0` tags. Native-NaN JSON-boundary exclusions are explicitly counted. |
| public oracle NaN sorting witness | Native NaN controls the comparator; the assertion compares the resulting close-value permutation, not NaN payload bits. |
| journal optional_num | Native NaN is asserted to produce absence. No NaN bits are captured. |

A recursive scan of all 26 original JSON artifacts found zero raw hexadecimal
NaN payloads. Semantic `NaN` tags occur in pricing, response and private timestamp
fixtures. Thus this audit found a susceptible capture function, not a reproduced
architecture-dependent mismatch in the frozen WOO corpus.

The `number_bits` supplement explicitly constructs positive, negative and
noncanonical NaNs, positive and negative zero, subnormal and adjacent finite
values, and both infinities. It observes source and Rust before JSON projection.
The writer freshness check was rerun on darwin/arm64, Node v24.20.0, and preserved
the original 87-case artifact. This is current recapture provenance; it does not
retroactively supply missing historical headers.

No direct consumer outside WOO was found for these fixture paths. Declaration
artifacts feed WOO numeric tests and remain untouched. The new supplements are
post-audit evidence, with no retroactive independence claim.

Both compiling controls fail at 101 on their named assertions:
`signed zero remains bit exact` and `non-NaN payload remains bit exact`.
After restoration, both Number capture tests pass. The positive source test
also checks the architecture header and all ten bit observations. Full
field-identity inventory remains in FIELD_IDENTITY_AUDIT.md.

Host and wasm32-unknown-unknown strict Clippy pass separately for WOO, including
test targets. No CI or browser execution is claimed. The process-spawn sweep
found no Node-per-case loop: bit and identity captures return whole corpora.

The public mapper `--mapper-identity` supplement also uses `number-bits.cjs`
before JSON serialization, with one invocation for all mapper cases and an
architecture envelope. This is new evidence: the original public.json erases
negative-zero percentage24h, so it cannot supply that assertion by itself.
