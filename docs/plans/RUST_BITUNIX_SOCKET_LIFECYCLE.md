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
