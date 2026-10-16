# Local adversarial sub-agent review

Measured implementation input: `cf64733d9bc9a5f58763e18cf5a885081935c4e9`.
Both reviewers returned **MERGE for the stated complete mapped callable body**.

`review_polymarket` inspected actual TS full-module loading, original prototype
getter and runtime-body provenance, native pattern-before-wire ordering, the
one-class completeness argument and all retained control logs. It verified all
12 recorded input hashes against committed blobs/restored worktree bytes and all
nine control-log/nine gate-log hashes. It confirmed the TS wrong-string fault
reaches the real runtime assertion, not merely an AST guard. Its provenance
finding was fixed before measurement: controls now reject dirty tracked inputs
and verify production baseline bytes against HEAD. It also corrected an
unsupported superlative about cost; this is a minimal calibration, not a census
of the cheapest possible functions.

`review_spot_contracts` independently inspected native import/module resolution,
real enum unit-variant construction, both structural kernels, full TS module
execution without stubs, independent 21-by-two runtime inventory and the four
control anchors/markers. Its call-boundary finding was fixed before measurement:
the theorem starts after successful call entry; caller setup and engine argument
limits are separate operations. Final review read all nine control logs and
verified restoration hashes, compile/check scope and completed extractor output.

The exact closed bodies prove that every successfully entered source call's
receiver/accepted arguments and mutable state are unread. They form one behavior
equivalence class, related to native unit input; the 42 witnesses do not enumerate
all JavaScript values. Trusted language/compiler/runtime and normal resource
semantics remain assumptions, not machine-checked meta-theorems. Construction,
dispatch, reflection, function-object identity and ABI are not covered.

Neither reviewer ran Cargo, source oracles, tests, controls or extractor, and
neither changed tracked files. Their verdicts do not add independent native
executions. The row is outside the frozen 242-row cohort; its 0/242 count does
not change. No full SDK approval is asserted.

## Post-merge review

`review_spot_contracts` returned static MERGE for measured merge input
`e9a093ba7979c49d0ce2184089f563ecb8f205b6`. It compared changed shared core
paths against the closed getter/unit enum return, finding no newly reached
dependency. It independently verified all 12 current input hashes against the
initial pins and merge sheet, all five merge log hashes, fresh name/spot runtime
passes, scoped check and extractor output. The unchanged kernel and control
sites plus fresh runtime evidence support retaining the old controls at their
actual `cf64733d9b` input; no four-control rerun or relabelling is claimed.
No new blocker was found. The reviewer reran no gates.


## Requested Clippy supplement review

Sub-agent review_spot_contracts: **MERGE**, measured input
3acd4d3fe1604238a95f328cb7fa9c2e55681766. Reviewed full local host and wasm logs,
matching receipt hashes, exact workspace/all-target and 38-package wasm policy
scopes, extractor zero-update text, twelve proof inputs and whole compiled-source
preservation. No reviewer gates were executed. Original controls and runtime
measurements retain their original SHAs; no wasm runtime claim.
