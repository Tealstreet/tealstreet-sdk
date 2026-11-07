# Bitunix explicit history completeness

Reverify native history responses/failures and query-only cap ownership against
actual landing. Failed first/later HTTP, native, malformed or transport pages
must reject rather than complete as clean empty/partial history. Preserve strict
financial validation; no failed-page rows may escape. Cap and full duplicate/
nonadvancing walks may retain validated rows only with owned semantic Warn before
done. Actual empty end pages and proven requested-start boundaries stay complete.
Use existing request.command; no fabricated owner for background account markers.

Regression first: first/later-page failure matrix, duplicate/nonadvancing pages,
cap retained rows/exact ownership at Error-only level, clean empty success and
existing time/dedupe/fee rules. Run Bitunix/Binance tests, strict Clippy/fmt/bans,
independent review and CI. Hold source merge for entropy1624; integrate actual
landing/fresh CI, separate exact guard then green merge/report. No trading writes,
private lifecycle, native identity or owner-gated policies in this phase.

Seven native tests fail original source (missing ownership, silently successful
failures and missing stall evidence). Fixed history16 and combined Bitunix/Binance
368 tests pass; strict Bitunix Clippy/fmt/bans pass. Two independent correctness/
adversarial reviews CLEAR. RustEngineExchange history methods directly propagate
native command rejections, so no caller fallback can mislabel a failed read clean.
