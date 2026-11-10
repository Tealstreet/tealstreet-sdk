# Rust harness: CPU gate policy

Sam, 2026-11-07: Rust must not be slower than TS. Fail only when Rust > 1.00x TS after one re-measure;
missing the 10%-faster target is a warning; nightly certification files cpu-regression issues.

- Verdict: `performance-policy.mjs` (`performanceVerdict`, `remeasuredVerdict`). Each CPU objective (main-thread,
  steady, process) compares the paired same-runner medians. Retained memory keeps its 10% rule as a failure.
- Re-measure: `gate.mjs` re-runs the 16 interleaved pairs once, in the same job, only when a CPU objective is over
  1.00x. It decides only the CPU verdict: the first set's non-CPU failures are kept. Only the re-measured raw set is
  retained, so recorded revalidation never re-measures.
- Warnings: `cpu-annotations.mjs` emits `::warning` / `::error` plus a step-summary table; warnings never block.
- Bitget steady: the paired counterbalanced certificate accepts only when its candidate Rust/TS ratio is at most
  1.00x and its frozen creep bound `q <= 1.023110` also holds; the 0.90 readiness miss is a warning.
- Checks: correctness is "Rust / replay correctness" and "Rust / replay measurement"; CPU is
  "Rust / CPU gate (<suite>)" (plan, certify, status). "Rust / required coverage" depends on no CPU job.
- Nightly: `.github/workflows/rust-cpu-nightly.yml` and `cpu-nightly.mjs` dispatch rust.yml on master, then file
  or update one `cpu-regression` issue for suites whose CPU gate failed on two adjacent UTC nights (the last run each night).
  Run discovery paginates until two eligible nights are found or history is exhausted; manual dispatches and
  multiple runs on one night must not hide the previous night.

Bitget CPU investigation uses `bitget-cpu-profile.mjs` only in an original CI
attempt on `fix/bitget-cpu`. It profiles production modules separately from the
certificate, retains steady-window markers, and accepts WASM symbol names only
after proving identical optimized non-custom sections. Diagnostic profiles never
certify a gate or change its fixed comparison policy.

`bitget-cpu-composition.mjs` separately measures the clean automatic composition
with pinned #3426 against the fixed anchor using the unchanged gate and cohort
order. Its synthetic commit is diagnostic only and cannot issue a certificate;
#3426 still needs its own authenticated certification after incorporating the fix.
