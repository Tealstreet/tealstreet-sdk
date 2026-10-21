# Port-review and wire-contract follow-up sweep

Start from landing `687fe0c1e08925a774c64e8bbb736f56052af206`.
Scope: still-open findings under Port reviews and Wire contract in
`RUST_AUDITS.md`; production impact governs each disposition.

1. Reverify every open or partly open finding against current source, existing
   regressions and landing history. Record stale findings with the responsible
   merged PR/commit. Collect those strikes in one documentation PR.
2. For remaining production defects, use fresh landing branches and one batch
   per venue or shared component. Reproduce each defect with a regression before
   repairing it; preserve intentional behavior and nearby caller documentation.
3. Independently review each batch, run focused venue plus Binance gates,
   formatting, strict Clippy and bans, and relevant SDK tests/typecheck/lint.
   Push for full CI, integrate landing as required, then run the exact scope
   guard separately before merging on green. Report every merge to the overseer.

Do not change owner decisions, infer undocumented live behavior, invent financial
data or silently discard incomplete history. List owner-decision and live-capture
dependencies explicitly for the overseer. Do not change budgets or run local
release WASM, full replay sweeps, timing gates or load tests. Use four Cargo jobs.
