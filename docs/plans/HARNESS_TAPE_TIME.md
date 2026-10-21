# Private replay tape-time sweep

Landing branch: `feat/rust-rewrite`. Harness changes only; preserve strict
FAIL results for documented TS/Rust differences.

1. Finish the parked OKX family replay: virtual private clock, exact observed
   diagnostic keys bound to each tape SHA, and the post-setLeverage USDT
   margin-in-use rationale at checkpoints 9–11.
2. Move remaining wall-clock private replays to tape time, one venue/family
   per PR in the handoff order. Remove only unobserved declarations; new
   Rust-ahead differences require exact keys, tape SHA and a specific reason.
   Keep Phemex startup time rewinding for signed request validation.
3. Extend `weex-replay-clock.test.mjs` to assert the covered venues.
4. Clarify WEEX client-ID declaration wording to describe TS-null IDs.

For each PR: review the plan and diff, run focused harness checks, require at
least five identical strict runs using Node 22 and fresh record/verify outputs,
review independently, and merge only after green CI. Use CI-built artifacts
or CI's five-pair mode on `wip/harness-tape-time-<suite>` branches. That
branch flag applies only to the matching replay matrix job; ordinary branches
keep one private pair. Compare all five raw outputs per implementation after
excluding measured metrics and record run ID and output hashes in the PR.
Do not build local release wasm or run public timing sweeps. Merge pinned landing SHAs with porcelain Git and inspect the complete
landing-to-head diff before merging. Report every merge to the overseer.

## OKX verification

The PR body records current-head CI run IDs, tape identities and all raw
artifact hashes. Each of the four family tapes must reproduce its complete
nonmetric output five times per implementation, with no unexpected or
unobserved private declarations, before merge.

## WOO phase

Enable the virtual private clock on WOO only. Use the matching sweep branch
to collect five strict CI pairs, compare every complete raw output except
measured metrics, and narrow only the exact unobserved tape-bound declaration
keys. Inspect order censuses, poll/command ties and snapshot staging in the
artifacts; do not change production behavior or broaden declarations. Run
focused harness tests, independent review, full CI and the fail-closed merge
guard, then report the merge. OKX landed in PR #1557 (`3d73437e589`).
