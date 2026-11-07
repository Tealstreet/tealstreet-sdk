# Polymarket and Kraken fixed-setting admission

Owner: codex-hcmq3f. Base: actual #1989 `ce278e0b4a9`.

## Scope

Replace two numeric success occurrences with landed `Cx::done_local_setting`.
Polymarket's shared branch covers SetLeverage, SetMarginType and
SetPositionMode; preserve its no-op Store behavior and exact `[]` result.
Kraken Futures SetPositionMode(false) preserves its no-op Store behavior and
exact `null` result. The true-mode refusal and all physical/private paths stay
outside this slice. Both settings remain keyless and public-only compatible.

## Implementation and validation

Call done_local_setting directly at the genuine Setting entry, with an empty
Store projection. Let its Rejected result carry its existing typed notice;
Revoked produces no extra numeric terminal. Do not manufacture a Context,
credential, write attempt, timer or money fact.

Add real-venue controls for keyless and public-only settings, preserving Store
state and emitting one terminal with no I/O. Add NEW-CONTRACT invalid-clock
controls before cutover: the current numeric path is a native compatibility
control, not a claim that these no-op settings execute a remote effect.
The new seam refuses an invalid clock with InvalidInput/NotSent.
Expired-receipt behavior remains covered by the landed seam tests.
Prune exactly the two removed occurrence counts in the checker inventory.
Run both venue history/trading tests, the dedicated controls, strict Clippy and
both request-context checker commands; full SDK and WASM size gates run in CI.

## Caller-visible boundary

Results and Store behavior are unchanged. These two success branches now obey
the genuine command receipt, including expired/duplicate entry refusal, and
have no numeric callback completion authority. No signing or account-readiness
claim follows from this fixed-setting slice.

## Executed evidence

At plan/test head `5d74fe10618`, each real venue has one passing native
keyless/public-only compatibility control and one failing NEW-CONTRACT
invalid-clock assertion. The cutover changes only the two success calls;
all assertions are unchanged. Both dedicated controls and the existing
trading/history targets pass after cutover. Inventory removes exactly one
occurrence from Polymarket history and one from Kraken Futures trading.

## Actual landing integration

Plain-merged actual `59f50e4f37b` after #1995/#2000 landed. Source resolution
keeps the landed Polymarket local/physical read adoption and reapplies only the
fixed-setting call. Kraken removes its now-unused numeric command binding.
The remaining Polymarket history inventory occurrence reaches zero, so the
path itself is removed; Kraken trading drops one occurrence. The first merge
checkpoint incorrectly kept an empty manifest path; a forward fix removes it.
The checker independently rejects that historical direct-parent checkpoint;
current manifest validation succeeds, with no checker change or exemption.
