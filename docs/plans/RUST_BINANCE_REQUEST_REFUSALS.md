# Binance request refusals

Owner: claude-kuhwxi (overseer assignment, 2026-10-26). Reviewer: claude-4yjlw0 or claude-dazzy3. Speed mode applies: this is a venue-only PR whose inventory changes are removal-only.

## Scope

Binance's 7 request legacy sites at base were all `cx.fail` numeric completions of pre-admission refusals:
- `trading.rs` (5): unsupported commands, Spot derivative settings, stop-limit amend, keyless trading, invalid plan;
- `lib.rs` (2): candle interval, candle range.

Each becomes `cx.refuse_command(operation, error)`: the engine admits the command's own Context, fails it with the native error, and emits one typed terminal. A full registry or invalid clock gets the engine's typed admission refusal instead. The operation is Read for the read commands (FetchStpMode, FetchAccount, FetchOrderBook, FetchLeverage, candles) and Mutation otherwise.

Native text, kind and NotSent are unchanged. **Better than TS:** an unsupported candle interval is now typed `Unsupported` and NotSent; it was a bare message.

Request inventory for Binance: 7 → 0. Controls: `tests/local_refusals.rs`.
