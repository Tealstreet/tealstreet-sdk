# Native frame server and Binance variant integration

Merge landing `a7caec9314f21f0ec2b446df2255849c1fac354a` after the bounded
Bitrue compiler trial. That trial retained z and restored all nine bindings;
its negative data and independent audit are committed in `d0e1d2a12c`.

Resolve this integration in four reviewed scopes: core engine/wire, native
host/frame server, replay harness, and WASM/SDK/CI. Keep the nested workspace,
book-delta section and native command tracking, shared command decoder, exact
Binance product units and authentication/readiness contracts. Preserve the
batched text/binary dispatcher, raw arrival timestamps, fixed-prefix barriers,
frame reservations and per-family packages. The native account actor must
exercise the same batching and control ordering as the standalone host.

Verify the merged source with formatting, bans, native and WASM Clippy,
workspace Rust tests, harness and host tests, repository typecheck/lint/tests,
and independent scope reviews before committing the merge. No public timing
runs while owned checks/builds run. Previous release artifacts and controls
keep their exact historical source labels.

After this checkpoint, review any remaining singleton-encoding experiment
before editing or measuring it. Freeze the chosen source and complete all
family builds, real WASM host/SDK and consumer tests, genuine per-family size
caps, isolated Clippy and every current public/private profile. Derive the
roster from the merged CI and registry; do not assume the earlier 29/54 counts
still cover new Binance aliases. Build a matching original landing control and
run the declared balanced comparison with identical helpers/dependencies.
Retain every failure and report all before/after numbers. All required gates
and wider CPU/memory margins are still required before opening the PR into
`feat/rust-rewrite`, adversarial review comments, green CI, merge and a fresh
follow-up branch.

WIP checkpoint: source conflicts resolved, formatting/scoped diffs checked and
WASM scope reviewed. New native actor ordering and snapshot-state regression is
written but not yet compiled. Full native/harness/repository verification is
pending; this merge is not a ready-to-land claim. Fetch now includes landing
`0063068241646a542ca0b936f75cb5ec6de2907c` (signed smoke harness), to integrate
next before freezing source. Current scope is 32 public / 57 private profiles.

The user explicitly requested a smaller PR to land value sooner. Preserve this
whole performance branch by commit, then prepare a focused replay-audit PR from
latest landing: GRVT hostname/subscription-prefix indexing with differential
correctness tests and measured replay cost. Existing OKX indexing is not GRVT
work; Bitget-only tape parsing is already in landing. Keep the larger batching,
allocation and per-family build changes on this branch. The focused PR must
pass its local checks, adversarial reviews and CI; it does not complete the
full engine performance task or its final all-venue comparison.
