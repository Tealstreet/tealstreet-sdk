# GRVT public fixture performance

Scope: improve GRVT perpetual public processing on the committed
`sdk-rs/harness/fixtures/grvt-public.jsonl.gz`, preserving output and cadence.
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

- Started from origin/feat/rust-rewrite (`5d7589f296`) on feat/rust-grvt-perf.
- Profiling complete; the speculative decoder change was discarded after paired
  measurements failed to establish a material gain. No runtime changes remain.

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
  every public frame and parses URLs twice. Its stack contains 368 inclusive
  samples, including 193 self samples in its prefix-scan callback. Separately,
  189 self samples have WASM or hosts/wasm source URLs (100 microsecond requested
  interval, whole-process diagnostic profile).
  Startup, idle and post-measurement GC are in this profile too; their totals
  cannot be attributed to gate CPU. Whole gate CPU obscures venue processing. Do not remove or weaken
  these correctness checks to improve this lane's result; no harness or shared
  engine changes are planned here. The lane can cache socket hostnames, index
  subscription membership while preserving unsubscribe/generation semantics,
  and avoid Bitget-specific tape parsing on GRVT. Preserve every audit check.
- Plan sub-agent review PASS: direct DeltaNumber visitor is the smallest
  candidate; preserve every numeric form, finite checks, signed zero, escaped
  strings, empty-string clearing, null/absent semantics and atomic rejection.
  Book allocation changes and public PnL changes are unsupported by evidence.

### Rejected candidate

- Replace DeltaNumber's untagged enum with the existing finite numeric visitor,
  parameterized only for ticker empty-string clearing. No arithmetic, Store,
  sequence, book, flush, routing, signing or transport behavior changes.
- Added regression coverage for long/full and lite fields, exact parsed bits,
  integer limits, signed zero, escaped strings, nullable fields, strict book
  numbers and malformed-message atomicity including sequence preservation.
- Implementation sub-agent review PASS, no findings. Added its suggested raw
  JSON negative-zero and strict book-number empty-string cases.
- Supplemental native release diagnostic was deliberately stopped (SIGINT,
  exit 130): it rebuilt the full signing dependency graph. The completed WASM
  sampling/boundary profile is the profiling evidence; no native timings are
  claimed. Removed the temporary native diagnostic test.
- Important limit: all timed WASM work was only ~24.7 ms of the diagnostic
  replay. A large whole-gate speedup cannot follow from this tiny venue change;
  report actual handler and gate measurements separately, without subtracting
  wall instrumentation or whole-process samples from gate CPU.

### Paired result and decision

Three alternating replays of each release WASM artifact used per-method thread
CPU counters. Instrumented ticker CPU was 63.341, 50.845, 54.003 ms before and
48.370, 52.870, 53.825 ms after (medians 54.003/52.870). Frame medians were
2.520/2.589 ms; book handling 3.289/2.978 ms. All three output comparisons,
excluding metrics only, were exactly equal. Instrumentation adds substantial
cost relative to the original boundary profile; these are not gate numbers.

The 2.1% ticker median difference is below the observed spread. A later
candidate gate passed at TS/Rust main 443.5/405.3 ms and steady 297.1/262.8 ms,
but machine load differed from the earlier baseline. It cannot demonstrate a
speedup. One diagnostic pair overlapped that gate; it is not the final gate.

Independent sub-agent review agreed to discard the speculative decoder change
and its experimental tests. The requested clear main/steady CPU improvement
was not achieved within the permitted crate scope. This PR preserves the
profiling evidence and hands the measured audit/tape costs to `feat/rust-perf`;
it makes no claim that shared engine hot paths are slow. Fixtures, thresholds,
harness behavior and engine/venue code remain unchanged.

A named release WASM sample can be reproduced from the repository root:

```sh
corepack yarn -s rust:build-wasm --venue grvt
wasm-bindgen --remove-producers-section --target nodejs \
  --out-dir sdk-rs/hosts/wasm/pkg/node \
  sdk-rs/target/wasm32-unknown-unknown/release/safe_cex_wasm.wasm
corepack yarn -s node --expose-gc --import tsx --cpu-prof \
  --cpu-prof-interval=100 --cpu-prof-dir=/tmp \
  sdk-rs/harness/replay.mjs --impl rust --venue grvt \
  --tape sdk-rs/harness/fixtures/grvt-public.jsonl.gz \
  --books BTC_USDT_PERP,ETH_USDT_PERP --out /tmp/grvt-profile.json
```

The ordinary WASM build restores canonical artifacts before gates. Temporary
profiles, timing wrappers and test output are outside the repository.

### Gates before landing integration (`7536117347`)

Canonical combined default-feature release WASM build PASS. Bans, GRVT fmt,
Clippy (all targets), 85 native tests and wasm32 host Clippy PASS. All four
replay gates PASS with original fixtures and thresholds:

| Public fixture (three runs) | TS/Rust main CPU | TS/Rust steady CPU | TS/Rust retained memory | Rust linear memory |
| --- | --- | --- | --- | --- |
| GRVT: 4,695 frames, 194 markets/tickers, 2 books | 463.9/457.8 ms | 329.3/294.5 ms | 26.1/2.3 MB | 1.0 MB |
| Binance: 873 frames, 917 markets, 783 tickers, 2 books | 250.8/147.9 ms | 140.0/72.9 ms | 15.9/3.1 MB | 1.9 MB |

Private correctness: GRVT 18 frames, Binance 14 frames, both PASS. Private CPU
and memory are informational. The final GRVT main CPU margin still reproduces
the original concern. These final combined-build gates are not before/after
comparisons against the earlier venue-only profile.

Final documentation audit: clarified inclusive versus self sample counts and
added exact named-WASM profiling/rebuild commands. Runtime diff is empty.

Landing integration: merged `origin/feat/rust-rewrite` at `74b0a7dd14`
(BreakoutProp) as `c3a5051ce2`. The diff against the new landing branch remains
only this document and its roster link. Follow-up integration checks and
independent review results are recorded on [PR #1331](https://github.com/Tealstreet/tealstreet-sdk/pull/1331).
