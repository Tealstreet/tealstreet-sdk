# Orderly port-review lows

Branch: `fix/rust-orderly-port-lows`, from current `origin/feat/rust-rewrite`.

1. Re-verify each low against landing. Strike stale discovery/order isolation
   findings with current evidence; retain cancellation safeguards when an ID
   alone cannot identify a safe normal/algo/sibling cancellation.
2. Route bounded candle reads to documented `/v1/tv/kline_history`, using
   seconds for `from`/`to`, typed parallel arrays, range/shape/OHLC validation,
   and ascending output. Preserve `/v1/kline` for unbounded latest reads.
   Reject intervals the history endpoint does not support before I/O.
3. Verify the trailing display formula against official API semantics. Fix
   display-only mapping if the required fields are unambiguous; record risky
   or unsupported cases rather than guessing a protective price.
4. Reproduce each fix with focused regressions, then run Orderly + Binance
   native tests, touched-crate fmt/Clippy and bans with CARGO_BUILD_JOBS=4.
   Independently review the diff, publish findings on the PR, wait for green
   CI, merge a pinned landing SHA with porcelain Git, run the revert guard,
   and merge into feat/rust-rewrite. Report the merge to claude-ddi2il.

Official history contract:
https://orderly.network/docs/build-on-omnichain/restful-api/public/get-kline-history

## Verification and review

Both bounded-endpoint selection and trailing display regressions failed against
previous behavior before implementation. Orderly + Binance native tests: 375
passed. After correcting one-sided bound defaults and adding future/no-data/WS
cases, the two touched test targets pass all 51 cases. Touched fmt, strict
all-target Clippy and banned-pattern checks pass; public/private replay,
whole-repository TypeScript gates and WASM builds run in CI per lane rules.

Independent review found the history API requires both bounds despite docs
omitting that label. Read-only calls to the configured EVM host confirmed
from=0 returns newest-N and absent to/from rejects with -1005. Implemented
from=0 / to=engine-time defaults and empty future-from results; final review
reports no remaining actionable correctness findings. Trailing display uses
the official SDK formula instead of TS's incorrect BUY/rate calculation.
