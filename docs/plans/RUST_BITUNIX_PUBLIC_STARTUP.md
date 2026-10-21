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
