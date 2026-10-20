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
