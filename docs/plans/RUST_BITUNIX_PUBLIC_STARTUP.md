# Bitunix public discovery and independent private startup

Reverify the whole-public-list invalid flag and ticker-dependent private_start.
Skip malformed typed catalogue/ticker rows with coded warning while preserving
valid peers. A nonempty unusable required seed must retry, never establish a
false authoritative empty list; malformed envelopes/streams remain failures.
Keep eligibility/API-support filters and all strict private/history mappers.

Start private initialization once the valid catalogue is available, independently
of ticker completion. Keep public ticker sockets gated on a real ticker seed;
ensure seed retries never restart private state/settings. Reuse shared startup
Backoff for separate catalogue/ticker retry keys (capped exponential/jitter and
Retry-After rate-limit floor), with request rebuilding and no write retries.

Regress mixed/all-bad/filtered/structural inputs, private reads while ticker is
pending/failing, no duplicate private start after seed recovery, and bounded
retry/reset. Run Bitunix/Binance tests, strict Clippy/fmt/bans, independent review
and full CI. Fresh source branch; integrate actual entropy1624 before merge with
separate exact guard. Socket reconnect/book/clock/read-key/private-row repairs
remain separate phases.

Plan audit: all startup helpers must retain private actions emitted with the
catalogue (shared helper, two account helpers and routing's real ticker seed).
Empty/filtered-only seeds cannot load Store readiness and must retry; they are
distinct from malformed rows. Private account valuation has no ticker dependency.
Implementation review found streamed native errors/malformed previews bypassed
on_response; public-only callbacks now preserve rate floors and sanitize previews
while retaining default private/history behavior. Two callback regressions fail
with overrides disabled; four primary regressions fail original source.

Verification: 22 public tests and 370 combined Bitunix/Binance tests pass, with
strict Bitunix Clippy/fmt/bans. Final independent correctness/adversarial review
is CLEAR after the stream-callback corrections. Actual history1633 must merge
and be integrated before this same-crate branch goes through final CI/merge.
