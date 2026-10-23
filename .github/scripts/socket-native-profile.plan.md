# Temporary native socket diagnostic

This branch is diagnostic only and must never merge. It adds one opt-in
`workflow_dispatch` job to the existing Rust workflow; normal gate jobs skip
only for that explicit input. No gate, timing target, build policy or package
source is changed by the diagnostic machinery.

One isolated Ubuntu runner builds three pinned source trees: landing SafeIDs
`7641650e7b1`, the immediate pre-change Phase H candidate `d8dcb8e6e62`, and the
reviewed Phase J candidate `1c243e48d2a3886033aa1c69b2cda6f24be366c8`. Each uses the exact same native driver, dependency lock,
Rust 1.99.0, fat LTO, one codegen unit, frame pointers, and release profiles
used for local diagnosis. Preserve Bitunix's size profile. The driver, tape,
source heads/trees, lock and protocol are frozen by a checked contract.

Builds finish before measurement. Run both complete fixed ABBA cohorts in one
serial job: landing/before/before/landing, then landing/current/current/landing.
Each process warms up three times and replays for 40 seconds. Attach Linux perf
for 20 seconds after READY, sample user CPU with frame pointers, retain every
raw perf file, stack export, self report, process CPU accounting and all logs.
Do not retry, discard samples, select a favorable cohort or automate a passing
CPU disposition. Native Linux remains a diagnostic, not the final WASM gate.

Persist each record as soon as it completes, and always upload diagnostics on
failure. Retain source/build provenance, exact binaries, machine/tool versions
and a hash inventory. Refuse changed inputs, dirty source, incorrect trees,
different final facts, missing profiles, unsuccessful processes or incomplete
cohorts. The immutable script is copied outside the checkout before sequential
source checkouts, and no build runs during either measurement cohort.

Review the workflow and runner before dispatch. Exercise fail-closed contract
tests locally without a build or replay. Final result readers independently
authenticate the actual workflow/job/artifact and inspect the complete perf
profiles before any further fix or fresh full CI. Final performance proof still
requires the separately authorized same-runner WASM experiment.

This is a NEW retained diagnostic cohort. The `before` role now denotes Phase
H, not original a31; the old original-candidate cohort remains historical. Only
the two case pins/trees and checked contract hash change in the runner inputs.
The source gates and both source/order reviews passed before freezing Phase J.
Use fresh run/job/artifact/output namespaces; never copy old results/binaries.
Verify the normal workflow base is byte-identical before reusing the opt-in
diagnostic additions. This branch and its diagnostic commits never merge.
