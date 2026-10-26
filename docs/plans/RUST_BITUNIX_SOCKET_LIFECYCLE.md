# Bitunix socket recovery and disconnected books

Reverify immediate reconnect and stale disconnected depth on current landing.
Replace per-connection watchdog/reconnect loops with one persistent shared socket
Supervisor per stable feed (private, futures and each public shard). Retire old
socket epochs before retry, cancel queue/login timers, and schedule exactly one
capped exponential retry with jitter. Preserve URLs, channel/feed/recover routing,
headers, subscriptions, public batching and private bootstrap. Timeout recovery
keeps recover=true; ordinary closes keep recover=false.

Use a 10-second connection bound and 15-second pings, 45-second public inactivity,
and existing private compatibility: opened private connections do not become
idle-expirable before the first real pong. Do not synthesize received traffic to
reset retry failures; reset only from actual traffic on a stable connection.

On any futures retirement, immediately invalidate subscribed books and publish
book_changed so consumers clear before retry/open. Keep interests, replay each
once at the new open, and reject retired messages/opens/closes/queue timers. Fence
same-socket login retry on retirement without changing native auth classification.

Regress repeated refusal/cap/reset independently across all feed families,
connection timeout, silent public recovery and private first-pong compatibility,
old socket/timer events, batching cancellation, book-unavailable/clear-before-open,
replay once and recovered one-sided depth. Prove original failures, preserve all
prior routing/heartbeat tests, run Bitunix/Binance native + strict Clippy/fmt/bans,
independent plan/implementation review, CI and separate exact merge guard.

Fresh actual landing branch; do not carry unmerged public-startup work. Integrate
that actual merge if it lands before this phase. Private rows/auth/read credentials,
host clock and forced routing remain separate phases.

Plan audit CLEAR: private supervisor uses finite f64::MAX idle (shared watch
clamps to five seconds), with a venue-owned 45-second deadline enabled only by
real pong. Real traffic alone resets failures; no fake received(), supervisor
reconstruction or ignored timeout. Test late pong, reset epoch gate, retired
queue/login timers and interests changed during backoff explicitly.

Verification: two root regressions fail original production code (immediate
reconnect and stale disconnected book); all six lifecycle regressions pass after
the fix, including every feed/shard, capped/reset retries, private pong timing,
retired queue/login timers, connection deadlines and current-interest replay.
374 combined Bitunix/Binance native tests and strict Bitunix Clippy/fmt/bans pass.
Two independent implementation/adversarial reviews are CLEAR.
