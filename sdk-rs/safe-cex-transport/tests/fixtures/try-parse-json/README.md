# Numeric capture provenance

The original `vectors.json` capture did not record its architecture or Node
version; both are unknown. Its bytes are preserved. On 2026-10-13 the actual
source capture was rerun on **darwin/arm64, Node v24.20.0**, and matched every
byte (SHA-256 `9ba72718e015427f36ba2d2300f3e2f200ccd0de2697c83a2ce72b4c39b3d14d`).
`capture.cjs` prints the verification runtime and architecture on every run.

The closed input list contains JSON numeric literals and nonnumeric parse
controls. Numbers can become finite values, signed zero or infinities, but never
NaN. The observer now refuses NaN so extending this corpus cannot accidentally
start pinning a CPU-specific NaN payload. If NaN-producing inputs are added,
canonicalize only their captured NaN bits; keep all other bits exact.

Native assertions distinguish `signed-zero capture retains its observable sign`
from `non-NaN capture retains every payload bit`. The live parser tests use the
same source runtime as their Rust consumer invocation; they are not frozen
architecture-specific snapshots. This supplement does not regenerate the
existing corpus or claim provenance for its original capture.
