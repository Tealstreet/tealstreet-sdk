# MEXC native error origins

Code gate SHA `4e95ad677d54f84aac352dd174e91586b3a7db93`; actual parent `854ec5f502118601fdad890f7905d4fb5505ec95`; initial integration base `1a43f3a8f25b7e5fe1c238794e79bad11e4c4788`; merged integration `d19d2874e0265c85602b9240d618612c65984c02`. Earlier branch commits `3f2c62543d` and `f66b0f6c39` contain the failing Store-origin witness and its fixture correction. Implementation commit `ed68e263f0`, native assertion-order commit `a6d815f7d4`, and warning-count commit `854ec5f502` are ancestors of the final gate SHA. Separate fill and public-handler PRs are not ancestors of this gate SHA.

REST rejections now retain the native RestFailure with its request, response phase/body, context, code and transport chain. Previously flattened Store failures retain the whole StoreMutationError and original opaque subscriber cause. The existing recovered-core-error branch keeps prior metadata/origin and appends the subscriber ID; it does not claim preservation of the outer Store error object. MEXC balance, position-mode and cancellation reject through this shared conversion. The metadata wrapper witness pins the second subscriber error after the fallback write; public-only pins the first error. The new origin witnesses complete their native assertions before source trace comparisons. Existing cancellation and position-mode corpora are rerun as regression floors.

The added Rc origin makes every ExchangeError **not Send or Sync**. This is a public API cost: the workspace check verifies in-repository consumers, not compatibility of downstream clients. The known full metadata literal in the webworker test was updated in the same implementation commit. Origin Debug omits retained request payloads.

The observed response snapshot difference is retained under [MANIFEST-SPEC](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias). Actual source requests/subscriber effects start before awaiting the Promise; the existing borrowed native futures start on polling. Notes name this remaining CHOSEN behavior and why the common interface is retained. Custom JavaScript Error names/extra properties outside RestFailure are not represented by this repair.

| Named gate at code SHA (or explicit control overlay) | Exit |
| --- | --- |
| `mexc-error-origin-writer` | 0 |
| `mexc-error-origin-source` | 0 |
| `mexc-error-origin-metadata-source` | 0 |
| `mexc-error-origin-core` | 0 |
| `mexc-error-origin-methods` | 0 |
| `mexc-error-origin-cancel` | 0 |
| `mexc-error-origin-metadata` | 0 |
| `mexc-error-origin-mode` | 0 |
| `mexc-error-origin-scoped-check` | 0 |
| `mexc-error-origin-workspace` | 0 |
| `mexc-error-origin-drop-store-origin-red` | 101 |
| `mexc-error-origin-drop-store-origin-restored` | 0 |
| `mexc-error-origin-drop-code-red` | 101 |
| `mexc-error-origin-drop-code-restored` | 0 |
| `mexc-error-origin-metadata-swallow-red` | 101 |
| `mexc-error-origin-metadata-swallow-restored` | 0 |
| `mexc-error-origin-restored-diff` | 0 |
| `mexc-error-origin-extract-check` | 0 |

Three compiled controls, stopped after iteration 3: dropping the whole Store origin fails `shared conversion retains opaque subscriber origin` at 101, restored test exits 0; removing REST code metadata fails `REST metadata code view` at 101, restored test exits 0; swallowing the actual metadata child rejection fails `metadata native wrapper propagates original final failure` at 101, restored test exits 0. Restoration diff exits 0. No control survived. The earlier compiled Store-origin baseline also exits 101 at `shared conversion retains opaque subscriber origin`; its SHA and separate fixture failures/interrupted preparation attempts are preserved in [machine evidence](ERROR_ORIGIN_GATE_RESULTS.json). Preparation failures are not reported as final commit coverage.

Coverage floors: two actual REST method rejections/two requests; two actual metadata wrapper/child/DefaultStore roots/three first-subscriber calls/zero later-subscriber calls/one request/one warning-log call. Nine existing position-mode cases and six cancellation scenarios are bounded corpora, not a completeness claim. Native witnesses inspect downcast originals, enum variants, f64 bits, identity, committed fields and call counts before secondary JSON views. Expected counts are independently fixed.

Tomllib over six MEXC manifests/185 symbols: **145 ported /40 divergent /0 pending**, leading buckets 17 CHOSEN/4 MIXED/19 FINDING. Three former leading FINDING rows now carry specific CHOSEN notes; no divergent-to-ported changes and **0 pending rows moved**. Twenty-two divergent notes still contain FINDING when inline findings are counted; this is not closure of all MEXC findings. Enumeration stops at `sdk-rs/parity/manifest/exchanges/mexc/mexc.ws-public.ts.toml`.

Not run: full venue/core/workspace test suites; ignored core source-notification test; host/WASM clippy; WASM builds/tests; TypeScript typecheck; live transport; CI. The all-targets SDK workspace compilation is a check, not those test suites.

Native sub-agent `context_review` completed stable-source and evidence reviews with no blocking findings. The reviewer ran no gates; measured exits above come from the named runner. The selected native tests passed 16 tests with one ignored, followed by three restored single-test passes.

## Integration merge follow-up

Integrated `f853d56fd227c2106dba609141b1fdd44bcab125` at code SHA `59499469d171b7caa5eb91ed6065347512a7a2b3`, whose actual first parent is `4c01557402b25d8a85f568b45263cf8adaa73e79`. The scoped own-input diff against original code gate SHA `4e95ad677d54f84aac352dd174e91586b3a7db93` exits 0: repaired source, tests, mapping notes and exemptions are byte-unchanged. Incoming audit notes and BingX/Blofin test changes altered workspace/extractor inputs, so those checks were rerun. Original selected tests and three controls retain their original SHA; they are not relabelled as new-head executions.

| Fresh merged-head gate | Exit |
| --- | --- |
| `mexc-error-origin-merged-own-inputs-diff` | 0 |
| `mexc-error-origin-merged-writer` | 0 |
| `mexc-error-origin-merged-workspace` | 0 |
| `mexc-error-origin-merged-extract-check` | 0 |
| `mexc-error-origin-merged-restoration-diff` | 0 |

Native sub-agent `context_review` also reviewed the merged-head evidence delta: clean, with original and merged gate SHAs kept distinct. No reviewer gates were run. This evidence follow-up changes no Rust, TypeScript, fixture, manifest or build inputs.
