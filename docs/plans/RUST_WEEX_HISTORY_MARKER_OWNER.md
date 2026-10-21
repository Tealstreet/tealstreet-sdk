# WEEX incomplete-history command ownership

Base actual landing4592d985e8a. Reverify all five query-associated WEEX
HISTORY_INCOMPLETE sites. Replace unowned notices with events carrying the
actual command ID before completion/failure, preserving Warn/text, retained rows,
range continuation, opaque-cursor, budget and failure semantics. Do not infer
missing assets, relax fills, broadcast unowned markers or assign fake IDs.

Strengthen actual native history regressions for exact owner/code/ordering,
including cap, cursor, failed chunk and non-range failure. Verify the real emitted
event against host/worker command isolation if a narrow fixture path permits it.
Run WEEX plus Binance native tests, strict WEEX Clippy/fmt/bans, focused JS gates,
independent review and full CI. Source merges hold for entropy1624; integrate its
actual landing and rerun CI before exact separate green merge guard.
