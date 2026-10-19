# Unused V3 ticker/quote retirement

Specific-claims assignment row 66: `WooV3WsTrade`. Baseline mirror master
`4d7247f9b0`. The Trade declaration is consumed only by the inert
`v3_ticker` composer and its exclusive `v3_request_ticker` facade.

A tracked-workspace caller census finds no Rust consumer outside
`v3_ticker.rs`, `v3_ws_quotes.rs`, their module declarations and that facade.
A TypeScript-AST scan of 929 tracked application/package source files finds no import,
re-export, literal require or dynamic import of `woo.v3.ticker` outside tests.
The ticker module's type-only BBO/Trade import is not a shipped caller. Existing
maintained-class/import-graph tests construct both WOO receivers, load no V3
module, and retain all eight hard-false migration gates.

Remove the composer, its ticker/info records, BBO/Trade declarations and private
serde/validation helpers, plus the exclusive Rust facade, source oracle and
mutation runner. Seven extracted TS identities remain visible as not-applicable;
no source row is hidden and no scanner, alias rule or exemption is relaxed.
This is closure by removal, not a fabricated behavior FIX/RED->GREEN claim.

The public stats mapper is not exclusive to the deleted facade: its own public
mapping, numeric ingress and field-identity targets still call it. Keep that
independent module and corpus unchanged. Shared Market and numeric conversion
helpers have independent consumers too. This retirement claims no liveness or
API equivalence for other inert V3 declarations.

`fix_audit_tests::inactive_v3_ticker_tail_has_no_constructed_registration`
checks all seven retired identities against the actual WOO receiver and runtime
globals, with a live fetchTickers positive control. Existing source import-graph,
legacy public startup/ticker and actual private execution-report/fill tests
verify the shipped paths independently. All-target Clippy checks remaining
compiler consumers. Full index and extractor check mappings; cancellation,
override, fmt and CI wiring remain required. Execution results belong in the PR.
