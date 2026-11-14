# Rust harness: CPU gate policy

Sam, 2026-11-07: CPU objectives default to failing only when Rust > 1.00x TS after one re-measure;
missing the 10%-faster target is a warning; nightly certification files cpu-regression issues.

Sam, 2026-11-11 ruling: HyroTrader may run up to 1.20x TS raw steady CPU.
Only the exact `hyrotrader` venue's raw steady CPU uses this bound: 1.20x passes, above 1.20x fails.
HyroTrader main-thread/process CPU and every other venue's CPU objectives retain the 1.00x bound.

- Verdict: `performance-policy.mjs` (`performanceVerdict`, `remeasuredVerdict`). Each CPU objective (main-thread,
  steady, process) compares the paired same-runner medians. Retained memory keeps its 10% rule as a failure.
- Re-measure: `gate.mjs` re-runs the 16 interleaved pairs once, in the same job, only when a CPU objective
  exceeds its applicable failure bound: 1.20x for HyroTrader raw steady CPU, otherwise 1.00x.
  It decides only the CPU verdict: the first set's non-CPU failures are kept. Only the re-measured raw set is
  retained, so recorded revalidation never re-measures.
- Warnings: `cpu-annotations.mjs` emits `::warning` / `::error` plus a step-summary table; warnings never block.
- Bitget steady: the paired counterbalanced certificate accepts only when its candidate Rust/TS ratio is at most
  `1 + BITGET_STEADY_NOISE` (Sam, 2026-11-12: the observed baseline range from runs 43273706928 and
  43281701341) and its frozen creep bound `q <= 1.023110` also holds; the 0.90 readiness miss is a warning.
  Master push and nightly runs measure even identical bytes and retain the baseline and landed-master ratios.
- Checks: correctness is "Rust / replay correctness" and "Rust / replay measurement"; CPU is
  "Rust / CPU gate (<suite>)" (plan, certify, status). "Rust / required coverage" depends on no CPU job.
- Certification writes its per-suite results and reasons before failing when any suite is uncertified.
  Its error annotations name each refused suite; downstream status checks retain their existing guards.
- Each public performance replay logs `CPU_METRICS` records with exact TS/Rust CPU medians in milliseconds
  and Rust/TS ratios on success and failure. Private and correctness-only runs emit no measurement records.
- Nightly: `.github/workflows/rust-cpu-nightly.yml` and `cpu-nightly.mjs` dispatch rust.yml on master, then file
  or update one `cpu-regression` issue for suites whose CPU gate failed on two adjacent UTC nights (the last run each night).
  Run discovery paginates until two eligible nights are found or history is exhausted; manual dispatches and
  multiple runs on one night must not hide the previous night.

BingX Spot qualification adds independent Spot public/private cases to the `bingx` suite.
The nondefault `bingx-spot-replay` WASM feature exports a fixed-credential fixture constructor;
normal factories still refuse Spot. Shipped family size checks run before the fixture build.

WASM venue caps retain both parents' measured floors. Fresh compositions must
retain at least 10 KiB above their optimized web/Node measurement, rounded up
to KiB; boundary tests retain inclusive cap and cap+1 refusal checks.

TxFlow agent refresh #3314 at merge 9bb43250830d measures 2,147,684 bytes;
refusal fix e959e2a7dc2e measures optimized web/Node 2,147,758 bytes. Its
2,158,592-byte floor retains the required 10 KiB reserve.
WEEX in the same composition measures 2,028,105 bytes and retains a 2,038,784-byte
floor by the same rule. Pinned profiles and remaining family/global budgets are unchanged.

TxFlow construction-feature guards require dispatch signing, address/nonce setup
and its hosted-wallet codec, with independent missing-feature refusals. The HTTP
HMAC hosted-signing feature cannot substitute for that testnet wallet codec.
