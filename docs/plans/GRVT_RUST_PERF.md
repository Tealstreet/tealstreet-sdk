# GRVT public fixture performance

Scope: improve GRVT perpetual public processing on the committed
`harness/fixtures/grvt-public.jsonl.gz`, preserving output and cadence.
No shared engine hot-path edits, fixture/threshold changes, or load tests.

## Phases

1. Profile: establish release WASM baseline and time decode, ticker/book apply,
   and frame emission on this fixture. Audit the measurement and candidate with
   a sub-agent. Temporary diagnostic artifacts stay outside commits.
2. Fix: change only the GRVT crate's measured bottleneck, add meaningful edge
   coverage, compare the same fixture and audit correctness with a sub-agent.
3. Gates and landing: run brief-required Rust checks, GRVT public/private and
   Binance control gates, post adversarial review on a PR to feat/rust-rewrite,
   resolve findings, merge only with green CI and cut a fresh follow-up branch.

## Progress

- Started from origin/feat/rust-rewrite on feat/rust-grvt-perf.
- Reading and profiling underway; no implementation changes yet.
