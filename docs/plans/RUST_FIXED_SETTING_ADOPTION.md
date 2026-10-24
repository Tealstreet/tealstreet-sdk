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
