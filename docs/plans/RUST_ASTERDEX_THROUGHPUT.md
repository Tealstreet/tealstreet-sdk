# Restore AsterDEX whole-family WASM throughput

Base: actual #1726 `e09e34486259f4efb44cc04c84a9eec6f37c5974`.
Fresh branch from current landing; plan committed before source changes.
Timing tally: AsterDEX 4 unrelated misses; a passing authorized retry does not
reset the count. Owner tally: KuCoin Spot4 (earlier memo records3), restored; WOO1 retains its original profile.
Old checkpoint54b72 evidence predates clock1722 and is not used.

1. Commit this plan and one README index row first. Four owned paths: new plan,
   README, existing RUST_WASM_ARTIFACT_SIZE note, harness/build.sh. No Cargo,
   source algorithms, fixtures, manifests, comparator, native or budget changes.
2. Follow the KuCoin Spot rollback: exclude Aster standalone/all-family builds
   from shared z defaults; restore host/engine/inherited dependencies to3 and
   override Aster venue package to3. Retain historical Binance z when linked.
   Preserve all other27 family argument arrays and combined arguments exactly,
   plus wasm-opt -Oz. Audit expanded arrays without executing cargo.
3. Final baseline: actual #1731
   `06b321192a65562db61489b2cbcdcfe4fce5d055`, including actual #1729
   `6e10b0e8a23dbb8d34f761e8199a83c6068fadb1`; fresh landing-push
   Rust run `39738818151` builds that exact source tree. The green #1731 PR
   head predates #1729 and is not whole-tree-equivalent to the actual merge.
   Authenticate the push event/ref/head/job/archive, source fingerprint and
   package hashes. #1726's 918473-byte baseline and the initial rollback run
   are historical/provisional only. This experiment's final four-file delta
   is measured against actual #1731, after both predecessors are real-integrated.
4. CI owns release WASM, size and timing. Require complete3 Aster workloads:
   public1 correctness+9 performance per implementation, privatev1/v3 one each:
   24/24 full raw outputs,12 Rust; exact stable hashes within each mode,
   only top-level metrics removed. Preserve full IDs/requests/receipts/checkpoints
   and exact known failure/fixer arrays. Authenticate GH ZIP digest/run/head,
   package/source fingerprint and every declared artifact/ledger slot.
5. Report normal stripped size delta/headroom under unchanged1343488-byte
   budget, and all public CPU/memory policy measurements plus nine-pair samples.
   No timing allowance increase, excluded output or unsupported speedup claim.
   Run shell/static/root checks, independent architecture/source/evidence review,
   full green CI. Integrate changed shared inputs, separate fresh exact4 guard,
   separate match-head merge, actual parents/tree audit and immediate reports.

Architecture gate: one build-profile selection policy; Aster-only restoration of
an unsuccessful optimization. No new per-venue runtime cross-cutting mechanism
or NX caller contract change.
