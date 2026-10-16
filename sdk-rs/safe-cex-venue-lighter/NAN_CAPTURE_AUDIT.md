# NaN capture audit (in progress)

Capture environment for this census: darwin arm64, Node as installed in this
worktree's host. All 21 live Node producers ran successfully. The separate
`probe-powf.cjs` Rust/libm measurement was not rerun as part of this Node-only
census; its frozen report already records platform, architecture, Node/V8 and
rustc versions and compares result NaNs using the string `NaN`.

Seven live producers emitted canonical `$number: "NaN"` payloads:

| Producer | NaN tags |
| --- | ---: |
| approval-types | 4 |
| exchange-helpers | 5575 |
| pure | 15802 |
| signer-types | 6 |
| tier | 18 |
| utils-coercion | 1 |
| ws-state | 943 |
| Total | 22349 |

The census recursively inspected `bits` and `$number` hexadecimal payloads using
IEEE exponent/fraction masks, and found zero NaN bit patterns. Semantic helper
capture already uses the text tag independently of the separate `-0` tag.
SDK numeric-ingress spellings are parsed numeric JSON literals: overflow can
produce infinity, but none can produce NaN. Constant/protocol bit captures were
finite in the measured source. This is a current-source measurement, not a
claim about all future constants or every Rust-side comparison.

The original signing artifact is a shared feeder and remains byte-identical.
Its later identity supplement records capture architecture/platform explicitly;
no old header is rewritten. The powf report is historical evidence and is not
regenerated to relabel a previous gate result.

The Rust bit-comparison inventory covers 13 test files: SDK common, orders,
responses, transactions, generics and numeric ingress consume JSON numeric-literal
bit captures; constants and protocol captures currently contain only finite
source values; protocol types additionally test in-process native f64 event
preservation; signer/approval/builder fixtures compare in-process native fields,
using the canonical NaN tag when sourced from Node. Pure helper comparisons use
is_nan for computed NaNs; clampDecimals returns finite values for its non-finite
inputs. No captured Node NaN bit identity is asserted by those current corpora.

The historical powf report is the exception at its INPUT metadata: baseBits and
exponentBits could expose the sign of a literal NaN, even though result NaNs were
already canonical text. Its future capture helper now emits 7ff8000000000000 for
NaN only. The existing report is unchanged and is historical measurement evidence,
not a CI snapshot regenerated in place. This one capture writer is hardened;
seven live semantic producers were already canonical.

The isolated common-record reviewer independently compiled zero-only flattening
and finite corruption, observing exit101 at Balance.decimals existing Value
signed zero and Balance.decimals native field identity respectively; restoring
both returned3/3. These prove finite and signed-zero checks remain active.
Focused capture tests pass 2/2: seven exact cases plus child mutations requiring
exit101 at pow capture signed zero exact and pow capture finite exact. The
capture helper executes without rerunning the full libm measurement. Isolated review returned MERGE: 16 independently probed IEEE cases passed;
removing canonicalization failed the payload-NaN witness. Scoped live_pure
strict host Clippy passed at jobs1. No wasm result is claimed for this test-only
change.
Signed zero and all finite payloads must stay bit-exact. No corpus-wide completion
claim is made by this partial capture census.
