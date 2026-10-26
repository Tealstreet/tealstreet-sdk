# Bybit acknowledged leverage versus stale positions

Base actual landing e6cd8b5bd7a. Separate from private row admission #1620.
A successful leverage write updates settings and cached positions, but an older
in-flight positions read can overwrite them. Reproduce that overwrite for both
success and native leverage-not-modified ACKs. Reuse the existing positions
revision fence: advance its version on acknowledged settings and coalesce one
fresh read after an older active join settles. Preserve response/error policy,
valid concurrent financial state, native refused writes and Hyro behavior; do
not retry writes or invent a new settings publication channel.

Run Bybit plus Binance native regressions, strict Bybit Clippy/fmt/bans, an
independent review and full CI. Fresh actual landing integration, exact scope
guard as a separate call, merge on green after #1620; report both merge pins.

Verification: original regression reproduces acknowledged 20x overwritten by old
10x. Accepted/native-not-modified, fresh metadata/no write resend and refused-write
admission pass with 556 focused Bybit/Binance tests; strict Clippy/fmt/bans
pass. Independent review clear after excluding Hyro from this ordinary-read hook;
its existing atomic write/snapshot revision remains authoritative.
