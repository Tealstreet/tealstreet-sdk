# WEEX selected-symbol metadata refresh

Base: `8f7b0cee3ff` (landing after #1665). Re-verified Cutover audit:
`RustEngineExchange.updateSymbolMetadata` dispatches native rails only for
onMetadata/onReadAndMetadata; WEEX reports onRead, so the selected-symbol hook
silently does nothing although explicit fetchLeverage already works.

1. Commit this short plan first. Reuse the existing native fetchLeverage / signed
   symbolConfig path and shared SDK capability policy; no venue-name fallback.
2. Add regressions reproducing a stale cached/flat symbol on metadata refresh,
   repeated refreshes and independently fresh explicit reads. Keep public-only
   no-private-I/O behavior and other onRead venues unchanged. Exercise actual
   native capability/host behavior in compiled consumer CI where practical.
3. Report WEEX as onReadAndMetadata and update the mirrored capability fixture;
   retain the existing native parsing, revision fences and failed-read behavior.
   Update nearby guidance and audit/caller status only for this repaired hook.
4. Run focused SDK/native checks and typecheck with local Rust jobs capped at4;
   CI owns release WASM, WEEX/Binance gates and full checks. Independently review,
   fix, push/PR and integrate current landing under lane rules. Do not merge
   shared source until Kraken #1663's priority hold is lifted. Fresh separate
   guard, exact-head green merge and actual-pin reports to ddi2il/cxxyqr.

## Before-fix evidence

The actual native/host regression ran against green #1666 CI run39669643997's
`ci-wasm-weex` artifact (WASM SHA256
`7992f411c20a00395dca82dc1f25fecb03ba3fe75a1bea57e4099b59f4bc4add`).
Both refresh/failure tests fail because no scoped request is dispatched; the
public-only control passes. The initial local package contained only Binance
and failed constructor setup; that attempt is not regression evidence.
No local release build was run. Kraken #1663 landed at `e033ec44f2e`; the
priority hold is released and that landing is integrated before implementation.

Independent review caught a stale all-venue native host capability expectation.
Its focused parity test failed after the capability change (WEEX still expected
onRead); update that mirrored expectation as well as the JS fixture. The
existing generic onRead test now uses GRVT with every assertion retained.

## Integration and focused checks

The committed implementation integrates #1670 WEEX empty-fill history and
#1671 audit docs at `fb96c9d5d4b` conflict-free, preserving incoming tests and
B11 text. Local checks: 20 shared SDK policy tests, 39 native WEEX account
tests, native all-venue capability parity, SDK typecheck, scoped Clippy and
format checks pass. Final after-fix WASM proof is required from fresh CI; the
local artifact remains the known pre-fix baseline, not rebuilt silently.
