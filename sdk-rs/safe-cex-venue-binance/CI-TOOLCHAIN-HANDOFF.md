# Urgent formatter CI repair handoff

PR7851 is draft. Its original integration parent is
ac0f5fccf24207b26c279e67da2a88f2c72a7aae. The merge in this preservation step
incorporates integration333fd419fdbf70f081039610f22da9ee653bc96f.
It retains prior unmerged owner/notification/prototype work as ancestors.

At bdb3ac57417143976f03075310eb91873f975631 the complete local Binance oracle
passed0:289passed/0failed/0ignored across73binaries, matching the separate source
expectation through tests/types.rs. Workspace fmt0, workspace build0 and wasm
policy0 completed. Host clippy finished101 at numeric_ingress items_after_test_module;
the identical capture helper was subsequently moved before that module. The wasm
build was interrupted without a measured completion exit. Doctests, wasm clippy
and the remaining final audits were not completed by that stopped pipeline.
These numbers do not cover this merge.

The formatter default-cwd regression failed1 before repair at8778fe2778 and
passed0 atf79adb2241. Removing cwd deliberately failed1 at the named assertion;
restoring it passed0. Explicit relative RUSTFMT semantics are preserved. Root
freshness with RUSTUP_TOOLCHAIN unset passed0 atbdb3ac5741. The portable records
are under gate-evidence/integration-gates.

The merge-down attempt hit conflicts, and the shell incorrectly continued to
push4e83571a66. That push was reported as not ready. The accidental dirty-merge
fmt1 and interrupted host -2 results are marked dirty and are not final gates.
No success is inferred from them. Future dependent mutations must inspect each
command exit before proceeding.

The urgent CI-only repair will be based freshly on integration, without this
PR's unmerged owners/prototype work as ancestors. It must run its own Binance,
host clippy, wasm clippy and workspace build gates; old measurements will not be
retagged to that branch. The remaining inherited-length finding stays visible
at the canonical anchor and in the affected rows.

Preservation checks at31740e204c02e384eeb52e6072ac8bdccbf678e2:
scoped safe-cex-exchange all-target Cargo check0, extractor check0 (zero updates,
additions and orphans), actual-source length witness0. The venue census was
263ported/23divergent/0pending across20files through binancespot.ws-public.ts.toml,
with8literal FINDING rows. This is a census, not a whole-venue parity result.
Integrationd4e20845a85cc4e2711c99648ef7547f8aa806df was subsequently merged without
conflicts. Earlier gate numbers are not retagged to that later merge.
