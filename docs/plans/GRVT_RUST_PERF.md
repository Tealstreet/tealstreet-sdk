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

### Profile checkpoint

- Unchanged release WASM, three-run gate median: TS/Rust main 245.6/220.1 ms,
  steady 142.9/139.0 ms, retained 26.0/1.5 MB; identical output PASS.
- Fixture: 4,423 ticker deltas, 75 book snapshots, 196 subscription ACKs.
- Separate diagnostic replay, instrumented WASM bindings: 4,694 websocket
  calls total 17.854 ms, 42 frame calls 1.945 ms, HTTP startup 3.732 ms.
  These are wall timings with wrapper overhead, not gate CPU measurements.
- V8 sampling with release WASM names retained identifies typed feed/JSON
  decoding and numeric parsing inside public_message. Frame writing is minor.
- Engine perf lane note (`feat/rust-perf`): the existing GRVT harness audit
  `venues.mjs:createSocketAudit.received` allocates/scans all subscriptions for
  every public frame and parses URLs twice. It accounts for 368 samples,
  including 193 in its prefix-scan callback, versus 189 samples in WASM/host
  paths (100 microsecond requested interval, whole-process diagnostic profile).
  Whole gate CPU therefore obscures venue speedups. Do not remove or weaken
  these correctness checks to improve this lane's result; no harness or shared
  engine changes are planned here.
- Plan sub-agent review PASS: direct DeltaNumber visitor is the smallest
  candidate; preserve every numeric form, finite checks, signed zero, escaped
  strings, empty-string clearing, null/absent semantics and atomic rejection.
  Book allocation changes and public PnL changes are unsupported by evidence.
