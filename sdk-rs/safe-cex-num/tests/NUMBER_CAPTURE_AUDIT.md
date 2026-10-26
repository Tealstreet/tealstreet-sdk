# Number capture audit

35 num/GRVT files contained bit capture/comparison operations. Seven NaN-capable
output pipelines used raw payload bytes: round, log10, pow, extrema, Date,
lodash.random, and GRVT builder errors. All seven now capture via
`support/number-bits.cjs`, using `7ff8000000000000` for NaN only. This new shared
test helper also serves GRVT; it does not rewrite an existing feeder artifact.
Its stderr capture header records actual architecture, platform and Node version.
Local captures are darwin arm64; no x86-64 execution is claimed.

The other six previously accepted any NaN pair in Rust; GRVT's builder-error
capture had strict payload equality. Number-string tests carry arbitrary NaN
bits as inputs and assert the resulting string, not an output payload.
Arithmetic/P4/selection/coercion/hook oracles already encode NaN symbolically.
GRVT numeric ingress accepts only JSON numeric tokens; builder successful fee
conversion rejects nonfinite operands but deliberately retains derived infinity;
trigger prices are captured only after a finite-result guard. None can capture
NaN output. Balance-prototype witnesses assert Number.isNaN, not payload bits.
Signed zero, subnormals, finite neighbors and both infinities remain bit-exact.

Controls changed the capture function to normalize both zero signs and then to
replace finite nonzero values with 1. Each failed its named SIGNED ZERO CAPTURE
or FINITE CAPTURE assertion; both were restored, and all three capture tests pass.
No change to production numeric semantics is made.
