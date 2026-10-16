# MEXC mapper Unicode boundary evidence

Six audited explanation findings are now MIXED: normalizeMarket, normalizeTicker, normalizePosition, normalizeOrder, normalizePlanOrder and normalizeStopOrder. Their existing alias costs remain explicit under [ownership](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias). The materialized symbol and whole-record refusal costs cite [surrogate conflict](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict) and [string boundary](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary). These notes document observed differences; no production Rust or maintained TypeScript behavior changes in this slice.

Actual integration parent: `539ac8165b8e36ef009dc467123e69fe14e002e2`. This fresh branch has no other unmerged slice as an ancestor. The adjacent JSON retains sanitized commands, measured exits, outputs and exact original SHAs. Final native/scoped/writer/extractor gates are bound to `8687cef962caf2c5fe1c65f6614564e06a4e2840`. Earlier controls retain `82ea59f904913707349227ae6f76dc727063aadd`; they are not relabelled as final-head execution. The later Source fixture formatting changed no assertions, and its control/restoration were measured again at8687cef962.

## Native and Source floor

The new native test directly asserts decoded symbol UTF-16 units and independent numeric fields for28 admitted mapper calls, plus4 direct Market fixture children for Ticker. It measures7 whole serde_json::Value ingress syntax refusals, before calling any mapper for those records. Source calls the seven actual maintained exported helpers over five symbol cases:35 subject calls, plus5 direct Market fixture children. Source symbol units are asserted before JSON output. The common tests/source.mjs loader is unchanged.

The five cases are ASCII, BMP U+03A9, an astral rocket, internal U+FEFF/U+3000, and a lone U+D800. All native admitted results and numeric assertions precede invoking the Source subprocess or comparing its secondary JSON observations. For U+D800 the Source returns actual mapped symbols, while the corresponding native encoded records fail before a mapper can run. The test does not claim a native lone-surrogate helper invocation or partial mapped result.

These are independently fixed direct-call floors, not a census of every nested helper entry. Enumeration stops at normalizeExecution/lone (a native ingress refusal), after the native admitted normalizeExecution/internal-space case. Execution provides a seventh subject witness; this slice does not reclassify that row because PR7983 owns its separate historical/factory explanation repair. Aliasing, timezone behavior, all other fields and all inputs are not re-proven by this bounded corpus.

## Discrimination

- Native control: compile normalizeMarket with U+03A9 removed from its symbol. It exits101 at `unicode native materialized symbol normalizeMarket/bmp`, reporting actual units without937. Production bytes are restored, the scoped git diff exits0, and the restored native test exits0. This is a compiled assertion failure, not a compiler error.
- Source control: wrap the actual loaded fromMexcSymbol export to remove U+D800. Actual normalizers use that child export. It exits1 at `SOURCE materialized mapper symbol units normalizeMarket/lone`, reporting actual units without55296; the default restored Source run exits0. This export-level fault injection does not edit maintained TypeScript or the common loader. The distinct control is measured at both original and final fixture SHAs; that is not two independent controls.

Both selected controls discriminate; neither survived. Control enumeration stops at these two fault shapes. This is not exhaustive mutation coverage.

## Gates and remaining work

- `mexc-mapper-source-baseline` at `82ea59f904913707349227ae6f76dc727063aadd`: exit0.
- `mexc-mapper-native-baseline` at `82ea59f904913707349227ae6f76dc727063aadd`: exit0.
- `mexc-mapper-source-drop-lone` at `82ea59f904913707349227ae6f76dc727063aadd`: exit1.
- `mexc-mapper-source-restored` at `82ea59f904913707349227ae6f76dc727063aadd`: exit0.
- `mexc-mapper-native-drop-bmp` at `82ea59f904913707349227ae6f76dc727063aadd`: exit101.
- `mexc-mapper-native-restoration-diff` at `82ea59f904913707349227ae6f76dc727063aadd`: exit0.
- `mexc-mapper-native-restored` at `82ea59f904913707349227ae6f76dc727063aadd`: exit0.
- `mexc-mapper-final-native` at `8687cef962caf2c5fe1c65f6614564e06a4e2840`: exit0.
- `mexc-mapper-final-source-control` at `8687cef962caf2c5fe1c65f6614564e06a4e2840`: exit1.
- `mexc-mapper-final-source-restored` at `8687cef962caf2c5fe1c65f6614564e06a4e2840`: exit0.
- `mexc-mapper-scoped-check` at `8687cef962caf2c5fe1c65f6614564e06a4e2840`: exit0.
- `mexc-mapper-writer` at `8687cef962caf2c5fe1c65f6614564e06a4e2840`: exit0.
- `mexc-mapper-extract-check` at `8687cef962caf2c5fe1c65f6614564e06a4e2840`: exit0.
- `mexc-mapper-diff-check` at `8687cef962caf2c5fe1c65f6614564e06a4e2840`: exit0.

Writer and extractor completion text is checked, including zero updated manifests and zero added/orphaned symbols, in addition to their measured exits. A scoped crate check is not workspace compilation. The new test target is included by the existing mandatory venue-oracle:mexc all-target invocation; CI was not rerun.

Tomllib over six manifests/185 rows, stopping at mexc.ws-public.ts.toml: **145 ported /40 divergent /0 pending**, with3 CHOSEN/7 MIXED/30 leading FINDING notes. **Six leading findings reclassified, zero pending rows moved, zero divergent-to-ported changes.** Remaining API, account, scheduling, socket-composition and Store notification findings remain work. Other reviewed open slices are not ancestors of this branch, and their fixes are not attributed to it.

Not run: full venue/core/workspace test suites; workspace compilation (no shared type or trait changed); host or WASM clippy; WASM builds/tests; TypeScript typecheck; live transport; CI reruns; exhaustive strings, fields, objects, timezone or cancellation interleavings. Existing alias witnesses are referenced, not claimed freshly executed here.

A fresh integration fetch followed by the named integration merge command exits0 with Already up to date at8687cef962. Integration remains the exact parent539ac8165b8e36ef009dc467123e69fe14e002e2; no merge conflicts, input changes or CI-step reductions occurred.
