# Coded logging completion

Landing: `feat/rust-rewrite`. No deployment or tealstreet-next edits.

1. PR5: expose structured engine events through SDK emitters and the worker
   bridge, retain Error delivery for existing auth/reload consumers, add
   code-based health classification and authoritative `getHealth()` snapshots.
   Keep text classification as the legacy TS fallback. Record downstream callers
   and deployment ordering in RUST_CUTOVER_CALLERS.md.
2. PR6 (merge after PR5): migrate remaining engine log sites to stable codes,
   delete Cx::log, LogLevel and Notice::Log, and update host protocols and tests.

For each PR: focused regression tests (including changed text, misleading auth
text, event metadata forwarding and filtered counters), package typecheck/lint,
scoped Rust checks with CARGO_BUILD_JOBS=4 where relevant; broad replay/wasm
checks in CI. Independent correctness review, fixes, green CI and the pinned
landing revert guard precede a merge commit. Report each merge via Aimux.
