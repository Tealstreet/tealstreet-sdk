# Initial branch-push secret scans

New Rust rewrite branches currently scan all history since master, repeatedly
timing out on hundreds of commits already on the landing branch.

Keep PR base/head and ordinary push before/after ranges unchanged. For an
initial branch push, use its common ancestor with `origin/feat/rust-rewrite`
when that ancestor contains history outside master. Otherwise retain the
master baseline, including branches with no common ancestor with landing.
Unexpected Git failures must stop the job. Keep the scanner version, checksum,
redaction, failure status, timeout and allowlists unchanged.

Landing history becomes the trusted scan baseline for its feature branches;
ancestry alone is not proof of a completed scan. If landing already contains
the pushed head, there are no branch-only commits to scan.

Add a focused CI regression test using temporary Git histories covering
current/older landing branches, landing advancement, master branches, merges
from master, ordinary pushes, PRs, manual runs, missing landing refs, unrelated
histories and Git errors.
Audit independently and require full green PR-head CI and a separate guard
before merging this workflow-only change.
