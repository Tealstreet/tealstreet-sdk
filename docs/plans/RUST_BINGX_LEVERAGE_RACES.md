# Preserve BingX leverage values during settings races

Restore #1522 read semantics when metadata discovery overlaps newer settings or
an in-flight settings write: retain newer cached rails, skip stale metadata
publication, and return current cached rails with read values as the fallback.
Neither overlap should become a command failure or UI null. Preserve atomic
leverage/margin publication for reads that remain current.

Add regression tests for a settings-revision race and a pending settings write,
including fresh returned values and unchanged newer store settings. Run focused
BingX/WEEX/Binance native tests, fmt, strict Clippy and bans; use fresh CI and the
separate revert guard before merging. Coordinate wording with codex-cxxyqr.
