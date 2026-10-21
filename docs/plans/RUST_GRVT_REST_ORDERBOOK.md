# GRVT REST order-book parity

Fresh actual landing: `bcf9fc37afe4d570912a5bf5518fe0c4a7351306`.
Plan first, independent of other venue implementations. The precise standalone
book-command decoder from reviewed Backpack #1680 is a genuine prerequisite;
use its exact reviewed head before implementation and require actual landing
before merge. Preserve #1679 typed builder errors and NX structural migration.

1. Reproduce native capability/JSON/query refusal. Match TS market-symbol lookup:
   missing market returns an owned empty book without HTTP; mapped markets post
   unsigned MDG /lite/v1/book with {i:market.id,d:50}, no caller limit,
   session, account header or signature, on live/testnet roots.
2. Add an unstreamed command-owned public Book job. Bypass session refresh and
   response cookie/account evidence even for native code1000. Retain existing
   bounded read-network retry behavior without retrying writes or native errors.
3. Typed mapping preserves native side order, parsed positive finite prices and
   nonnegative sizes, zero rows and cumulative amount totals per side; omit IDs
   and sequence metadata as TS does. Support native full/lite field aliases.
   Missing/null result or sides are empty as TS; malformed non-null structures
   and financial data fail atomically rather than invent a successful book.
   Require actual objects at named record boundaries, never sequence surrogates.
4. Expose only GRVT book-query capability and precise standalone decoder;
   align combined-host/SDK fixtures and bridge regression. Public HTTP/native/
   exhausted transport errors remain owned typed NotSent and never account-auth;
   preserve native code/message and bounded usable Retry-After hints.
5. Regress live/testnet POST/native ID/body/limit, unknown markets, exact values
   and totals/zeros, full/lite and empty/malformed structures, concurrent owners,
   code1000/no refresh, bounded network retry, and existing live book/account/
   ticker independence through query success/failure and subsequent WS traffic.
6. Independent plan/implementation audits, GRVT+Binance native tests, default
   combined-host capability check, focused SDK tests/typecheck/lint and strict
   Clippy/fmt/bans. Full exact-head CI, separate exact scope guard and actual
   merge verification/report. Integrate shared build rollback #1684 first.

No new product enablement, NX writes, TS deletion, budgets, release automation,
local release WASM builds or full replay/timing matrices. Four local Cargo jobs.


Plan audit correction before implementation:
- TS's legacy instrument_id request and long-only response mapping do not match
  the actual lite protocol. The current official [Orderbook Levels schema](https://api-docs.grvt.io/market_data_api/#orderbook-levels) requires i and d,
  allows depth50 and demonstrates r.b/a with p/s. Use {i:nativeId,d:50}; preserve
  TS's ignored caller limit, market lookup, order and cumulative base quantities.
  Decode both lite and full side/level aliases; no decimal metadata rescaling.
- This is a documented narrow correction to TS, not new product support. Native
  rate code1006 is RateLimited even on HTTP200; auth-looking public code1000
  must never enter the existing account_refresh path. HTTP status has precedence.
- All seven original-source regression groups fail through actual venue calls;
  standalone JSON remains separately exercised before Cargo admission is enabled.
