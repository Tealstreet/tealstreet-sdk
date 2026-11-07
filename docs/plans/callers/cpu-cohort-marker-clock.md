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
Measured CPU ratio failures remain failures for changed Bitget bytes; diagnostic
reruns never become original-attempt certification evidence.


Under ddi's byte-identity ruling, independently built base and candidate Bitget
WASM and JS glue with matching SHA256 seals pass as "unchanged bytes, not
measured". Both tip and cumulative-anchor CPU witnesses are skipped. The
collector retains both packages and their source/run metadata; the planner
rechecks their actual hashes under the same original-attempt producer authority
and binds the candidate seals to the family runtime. No Q or readiness estimate
is invented. Changed bytes retain both measured witnesses, the pooled statistic
and the frozen 1.023110 bound. Correctness, process CPU and memory gates remain.

Original #2960 run 41604963351 measured tip Q=1.034397850 on identical WASM and
glue, a negative control above the bound. Original #2951 run 41600029141 had
anchor block Q values 1.016719664 and 1.002793106, yet pooled Q=1.024389402.
The estimator follow-up is recorded for ddi; this change does not alter it.
