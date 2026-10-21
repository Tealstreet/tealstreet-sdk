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
