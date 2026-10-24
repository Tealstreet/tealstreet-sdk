# TxFlow steady ticker CPU

Base: actual `77f124089477a6aed502d9b02aa7f08752b7bfa5`. Owner: codex-5c57jl.
The request owner hcmq3f confirmed no overlapping TxFlow source claim.

## Measured target

Profile the public tape's steady interval, starting at its existing first-quarter
warm mark, with named WASM and the unchanged build settings. Separate profiles
from the uninstrumented before/after CPU samples. The tape contains 525 socket
messages, including 417 ticker updates. Initial steady samples include native
`on_message`, ticker Store lookup and allocation work. No profile, workload,
threshold, transport or socket authority change is part of this fix.

## Change

Reuse the catalogue instrument's borrowed symbol instead of allocating its name
on every ticker. Cache each instrument's ticker row after the existing complete
seed commits. This is a row-location hint, not publication or lifetime authority:
check the current Store row's symbol before using the index. A miss retains
`Store::update_ticker` and its existing missing-seed recovery rules. Metadata
replacement starts with empty hints; the subsequent seed rebuilds them. The
hint is skipped by metadata deserialization and does not alter venue wire data.

Keep the numeric decoder, ticker masks, live-before-seed overlay, all reducers,
HTTP builders, account hooks, socket checks and startup sequence intact. There
is no new shared engine API or venue lifecycle state.

## Verification and measurement

- Exercise real current-socket ticker updates, missing and unknown identities,
  table replacement and repeated catalogue/seed replacement. Preserve existing
  financial, public-policy, retirement, expiry and supervision controls.
- Build baseline and candidate from the same base with pinned Rust, bindgen and
  Binaryen; only the candidate source differs. Run nine alternating fresh-process
  TS/baseline/candidate triplets and retain every sample. Compare complete replay
  outputs excluding metrics and report CPU, memory and shipped size separately.
- These local Mac measurements diagnose the change; full CI retains the existing
  Linux CPU, memory, correctness and size gates. No favorable-run selection or
  threshold adjustment is allowed.
- Run scoped native tests, strict Clippy, fmt and normal bans; open a draft PR.

Results and exact source review receipts will be appended after execution.
