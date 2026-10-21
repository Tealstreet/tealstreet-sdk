# Fill execution identity

Landing: feat/rust-rewrite, after #1581. Reference:67e4e1c30a and
claude-kuhwxi Codex handoff, excluding financials already landed in #1430.

Add optional native Fill.id, populating Hyperliquid perp/spot from tid. Other
venues retain None. Keep FillFinancials, fee signs and realized-PnL conventions.
Emit FILLS revision2 unconditionally: existing base/symbol columns, execution-ID
string refs (NONE when absent), aligned realizedPnl[n] and fee[n] columns.
Preserve finite filtering and event-only, once-only delivery.

Accept revision2 only for FILLS in JS, while preserving revision0 and revision1
readers with/without #1430 financial tails. Add optional fillId accessor; SDK
fills use fillId ?? orderId, with orderId separately retained for attribution.
Harness Rust capture keeps order id in id and adds executionId plus finite fee.

Remove only replay-proven executionId groups from two Hyperliquid records
(perp and merged-spot) and two HyperliquidSpot records. Retain every unrelated
client-order/order/settings/position allowance and every other venue declaration.

Tests: native encoding offsets/revision/alignment and once-only snapshots;
HL perp/spot distinct execution IDs with preserved financials; legacy revision1
without/with financials and revision2 decoder fixtures; missing/present/invalid
ID refs, malformed lengths, revision3 rejection, multirow alignment; FrameApplier
execution-id vs order-id attribution; Rust harness capture of identity/fee.
Update affected Phemex/MEXC/WEEX/HL wire tests, frame contract fixtures and docs.

Verify focused engine, HL, Phemex, MEXC, WEEX and Binance tests, compile all venue
Fill sites via registry, strict clippy/fmt, focused wire/SDK/harness tests and
SDK typecheck/lint. No local WASM release builds: CI owns compiled frame tests,
replays, family size and timing gates. Require green CI proving all four removed
identity exceptions resolved. Read-only adversarial plan/implementation audits,
contract-owner notification, separate landing guard, merge and overseer report.
