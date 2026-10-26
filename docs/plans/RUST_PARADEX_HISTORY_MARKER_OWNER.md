# Paradex incomplete-history command ownership

Reverify the bounded history PageBudget warning. Add actual request command to
its existing Warn event before partial completion; keep unbounded cap and other
stall failures unchanged. Native regression drives50 pages below requested limit,
checks retained rows/exact ownership/order at Error-only level, and fails original
code. Run Paradex/Binance native tests, strict Clippy/fmt/bans and independent
review/CI. Hold merge for entropy1624, then real integration/fresh CI/exact separate
guard before green merge/report.

Original regression fails on missing owner. Fixed Paradex/Binance suite passes
422 tests; strict Paradex Clippy/fmt/bans pass. Independent review CLEAR: bounded
partial behavior and unbounded/other-stall failures preserved.
