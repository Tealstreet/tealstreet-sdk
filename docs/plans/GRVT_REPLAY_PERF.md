# GRVT replay audit cost

The GRVT profiling handoff in PR #1331 identifies repeated URL construction and
subscription-prefix scans in the replay socket audit. This focused change keeps
the audit's acceptance rules, errors, authentication, clocks and engine unchanged.
It does not complete the broader Rust engine performance work.

## Phase 1: plan and independent audit

Change only `sdk-rs/harness/venues.mjs`, its GRVT tests, and this plan. Cache a
string URL's hostname lazily at existing URL-parse call sites within each socket
generation. Do not cache non-string URL inputs or failed parses. Preserve parse
ordering: connected/closed, ping/pong reception and malformed JSON must retain
their existing behavior; request/session credentials remain live.

Maintain the existing exact subscription Set. On a newly added public key,
increment a Map count for every prefix ending in `@`; on successful deletion,
decrement those same counts, deleting zero counts. A public feed's membership
query uses the exact `${stream}:${instrument}@` prefix as its Map key. This is equivalent
to the old `startsWith` scan even with multiple delimiters, coercions, duplicate
subscriptions and overlapping selectors. Each generation owns its index. Private
stream membership, book selectors, auth ACKs and heartbeat checks remain intact.

## Phase 2: implementation and verification

Add differential tests against a frozen copy of the old prefix predicate covering
subscribe/unsubscribe, duplicate additions, overlapping/multiple `@` prefixes,
empty/null/numeric identifiers and generation isolation. Check URL parse counts,
non-string URL coercion and invalid-URL/JSON/heartbeat error ordering independently.
Retain existing private authentication and socket lifecycle regressions.

Run the focused GRVT test and full harness via `corepack yarn -s rust:test-harness`
(with serial test concurrency where needed), then obtain independent source and
test review. Repository typecheck/lint/tests and CI remain required before the PR
is ready. Cargo, fixture replays and performance measurements require the root
agent's next phase authorization; no CPU improvement is claimed from unit tests.

## Progress

- Plan prepared against landing `0063068241646a542ca0b936f75cb5ec6de2907c`.
- Independent plan audit passed before implementation.
- GRVT-only implementation and differential/error-order tests prepared.
- Focused GRVT tests passed: 14/14, including existing private auth/session checks.
- Independent source review passed; its same-generation URL-failure test improvement
  was applied and passed.
- Full concurrent harness attempt: 535/536 passed; existing Aster delayed-readiness
  subprocess hit its 25-second timeout. This failed attempt is retained in
  `/tmp/grvt-replay-focused.log`; a serial full run remains required.
- No performance gates or builds have run for this change.

## Phase 3: bounded measurement and final gates

Use one canonical combined release binary built from the unchanged landing Rust
source. Alternate only this file's before/after audit implementation. Freeze
fixtures, executable source, Node, Corepack/Yarn and all nine aggregate bindings;
check the installed dependency closure at the sequence's before/after endpoints.
Pause owned builds/tests/replays throughout measurements. Reserve a fresh gate
output directory atomically before importing the unchanged official gate.

Run exactly four pairs in AB, BA, AB, BA order, with one-run GRVT public gates
(two child replays each). No warmup, profiling or direction-based retries. Retain
numerical threshold failures; abort on output mismatch, invalid memory,
operational failure or incomplete results. Compare all non-metric outputs exactly
within each implementation. Report all four raw samples, ranges and the official
upper-middle medians for main/steady CPU and retained memory, including order
variation. These timings measure replay infrastructure, not an engine change.

Restore the candidate and require four final gates: GRVT public (three runs),
GRVT private (one), Binance public (three), Binance private (one). Private CPU and
memory remain informational. Then obtain fresh adversarial reviews, post them on
a PR into `feat/rust-rewrite`, fix findings, require green CI and merge. This
focused PR leaves the full batching/per-venue work on `feat/rust-perf` pending.

Verification checkpoint: all 536 harness tests pass serially with unchanged
timeouts. Repository typecheck 44 tasks, lint 28 tasks and tests 27 tasks pass.
The earlier concurrent harness timeout remains recorded above; no timing samples
have run. Independent implementation and measurement-plan reviews passed.

## Results at `499a2cb10d`

The fixed sequence completed: eight one-run paired gates and all four final
gates passed (32 child replays total), with no extra samples. Every GRVT
non-metric output matched exactly across versions within each implementation.
Dependency endpoints and source/artifact guards passed; candidate restored.

Same Node v26.7.0, Yarn 1.22.21 and canonical combined release WASM on both sides:
10,752,252 bytes per target, SHA-256
`42d504638655e206434af2661fcf11e7c3fe016753b541a087b7635529a0da50`.
Rust/build sources match landing `0063068241646a542ca0b936f75cb5ec6de2907c`;
only the audit file varied. The build preserved historical aggregate and
per-family artifacts.

| Four-sample upper-middle median | Before | After |
| --- | ---: | ---: |
| TS main CPU (ms) | 197.115 | 129.436 |
| TS steady CPU (ms) | 123.061 | 79.974 |
| TS retained bytes | 26,467,922 | 26,488,818 |
| RUST main CPU (ms) | 165.083 | 118.656 |
| RUST steady CPU (ms) | 101.772 | 63.617 |
| RUST retained bytes | 2,472,908 | 2,477,460 |

Rust main/steady CPU fell 28.1%/37.5%; TS also improved. The main TS/Rust
ratio narrowed from 1.194 to 1.091, and retained Rust memory increased by
4,552 bytes in these medians. This is a replay-audit optimization, with no
claim of wider engine margins or lower memory. All four adjacent pairs favored
the candidate's main and steady CPU for both runners; shared-machine variation
remains visible in the raw samples.

Raw samples in each version's execution order (AB, BA, AB, BA):

- TS before: main `[235.238, 197.115, 194.04, 168.683]`, steady `[125.099, 123.061, 121.063, 108.301]` ms; retained `[27348994, 26446738, 26467922, 26339554]` bytes.
- TS after: main `[129.413, 134.511, 129.436, 126.145]`, steady `[79.499, 81.642, 79.974, 73.949]` ms; retained `[26526498, 26488818, 26436026, 26360290]` bytes.
- RUST before: main `[165.083, 165.077, 165.31, 150.685]`, steady `[102.556, 98.816, 101.772, 91.474]` ms; retained `[2456500, 2500468, 2472908, 2470604]` bytes.
- RUST after: main `[118.24, 118.656, 124.29, 103.673]`, steady `[62.717, 65.935, 63.617, 51.626]` ms; retained `[2477460, 2460540, 2478572, 2462164]` bytes.

Final unprofiled three-run public gates:

| Fixture | Main CPU TS/Rust | Steady CPU TS/Rust | Retained memory TS/Rust | Result |
| --- | ---: | ---: | ---: | --- |
| GRVT, 4,695 frames, 194 markets/tickers, 2 books | 129.8/104.4 ms | 79.5/54.0 ms | 26.4/2.5 MB | PASS |
| Binance, 873 frames, 917 markets, 783 tickers, 3 books | 258.1/98.5 ms | 143.7/35.3 ms | 17.1/3.6 MB | PASS |

Private correctness: GRVT 18 frames and Binance 14 frames, both PASS. Private
CPU/memory are informational. These final gates are validation separate from
the fixed comparison. No fixtures, thresholds, clocks, engine or TS code changed.

Local raw evidence: `/tmp/perf-replay-pr-measure/result.json`, per-run logs/raw
JSON paths and hashes recorded inside it; canonical build proof
`/tmp/perf-replay-pr-build.json`. Runner and wrapper were independently reviewed
before execution. Reproduce correctness with the ordinary `rust:build-wasm`,
`rust:private-tape-grvt`, `rust:private-tape`, and GRVT/Binance `rust:gate` scripts;
use each fixture's books above, public three runs and private one run/empty books.
PR reviews and green CI remain pending. The full engine performance task remains
on `feat/rust-perf` at WIP checkpoint `8e7e8ea805`.
