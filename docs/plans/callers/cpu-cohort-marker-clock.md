# Buffered CPU marker delivery

The CPU cohort validator orders execution by the exact BEGIN/END payload
clocks, retaining the positive-duration requirement. Actions log receipt clocks
bound delivery: each marker must arrive after its payload clock, receipt order
must agree with marker order, and every payload/receipt must lie inside the same
successful public step of the authenticated original attempt-1 producer.
A BEGIN receipt may occur after the END payload when stdout is buffered.
Exact marker uniqueness, source, job/run/runner, archive and log hashes remain.

Original #2946 run 41598460629, paired job 125791670432, recorded a BEGIN payload
at 07:53:52.087692 and an END payload at 07:54:28.709793 UTC. Actions received
both at 07:54:28.710 UTC. The old check rejected that delivery although the
paired CPU job passed. The regression retains those literal timestamps, with
negative controls for reordered, duplicated, substituted and out-of-bound
markers. Numeric CPU bounds, pairing, eligibility and required coverage do not
change. The failed-certification wrapper remains exit 1 and now says so plainly.

This changes only harness validation and diagnostics; no venue, dependency,
Cargo.lock, WASM budget, artifact policy or numeric measurement gate changes.
Separate measured CPU ratio failures remain failures; this fix does not waive
them or turn diagnostic reruns into original-attempt certification evidence.
