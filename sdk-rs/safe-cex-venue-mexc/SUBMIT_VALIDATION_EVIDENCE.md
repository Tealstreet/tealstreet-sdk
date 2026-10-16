# MEXC submission validators

Moved **2 pending → ported**, **0 → divergent**: `MexcExchange#assertRegularOrderSubmitSucceeded` and `MexcExchange#assertRegularOrderBatchSucceeded`. MEXC triple: **92 / 40 / 53** (ported / divergent / pending). Census is separate from proof; see [census.json](evidence/submit-validation/census.json).

Branch `feat/mexc-submit-validation-2b2183` starts directly at requested integration parent `2b21833a6269dc56c29a2dd86918b70184868648`. Before these measurements, merged integration `89ad07ac80f6efc169430a2f94bd59898660e9f9` down. No other unreviewed lane branch is an ancestor introduced by this slice.

## Measured gates

All following measurements bind to **`f13a4729f047ffe7e8f59132f46f108eeff798af`**, not to later evidence-only commits. Exact commands/exits: [gates.json](evidence/submit-validation/gates.json); raw output resides alongside it.

| Gate | Measured exit / result |
| --- | --- |
| Scoped `cargo check -p safe-cex-venue-mexc --all-targets --jobs 2 --locked` | 0 |
| Scoped MEXC tests, actual TS oracle environment, `--jobs 2 --locked` | 0; 112 passed, zero failed; doc tests ran zero cases |
| Scoped `cargo fmt --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-mexc -- --check` | 0 |
| Official MEXC-only invention/status target check | 0; 185 manifest rows, 138 public Rust items, zero existing venue exemptions |
| `parity:extract:check` using actual Yarn script | 0; output inspected: would update 0 manifests; zero added/orphaned symbols |
| Production control script | 0; two compiled mutation controls below |

The first formatting invocation omitted `--manifest-path` and exited **1** before checking formatting. Both that attempt and the corrected exit are retained in gates.json and fmt-command-error.log. Earlier dirty-draft test compilations failed before the implementation commit; they are development attempts, not evidence covering this SHA.

## Native assertions and controls

The oracle executes the original TypeScript assertion/helper bodies, using the installed source TypeScript compiler. The Rust assertions inspect native error names/messages, conditional fields, ordered IDs, first failure index, native f64 count bits/NaN and cooldown calls. No native decode/re-encode or roundtrip comparison substitutes for these assertions.

Business errors retain the original `Rc<RefCell<Value>>` payload. Tests assert native pointer identity, mutate through the error and observe the caller payload, then mutate through the caller and observe the error. Strict `success === false`, truthy message fallbacks, nullish ID selection, complete failure-code filtering before ID/message coercion, duplicate IDs, absent metadata, and coercion TypeErrors are covered within represented JSON/native Value inputs. Both validators share `MexcOrderSubmitError`, mapped and explained in the manifest note.

| Production mutation | Named native assertion | Baseline / mutation / restored |
| --- | --- | --- |
| Copy response payload instead of sharing its original handle | `MEXC native original response handle` | 0 / 101 / 0 |
| Add one to first failed row index | `MEXC native first failure index` | 0 / 101 / 0 |

See [controls/summary.json](evidence/submit-validation/controls/summary.json) and each raw log. Both mutants compiled and reached their named assertion; source was restored byte-for-byte. No control survivor was observed in this two-control floor.

Independent expectations: 16 single cases and 20 batch cases, stopped at single index **15** and batch index **19**. Partial-ID cardinalities are the fixed 20-element fixture expectation, independent of produced arrays. The corpus is captured once per test process. The manifest census enumerates six MEXC files, stopping at `mexc.ws-public.ts.toml`; this does not establish completion of remaining venue rows. Mutation enumeration stops at `batch-failure-index`. A sub-agent statically reviewed source, oracle, native assertions and correspondence notes; its only finding (type target omitted from note) was fixed before gates. The reviewer ran no gates.

## Limits

This slice ports these assertion bodies. Placement/queue orchestration, arbitrary JavaScript accessors/functions, whole exchange-class behavior and real network execution are not proved. Remaining rows stay pending. No full workspace build/tests, host or wasm clippy, CI, or Polymarket gates were run for this MEXC slice.

Final fetch/merge at `8765d835ee89354d23cf43dc5e4f1cf056fd5fa9` found integration already included. [preservation.json](evidence/submit-validation/preservation.json) records nine comparison exits **0**, including the required MEXC src/tests comparison and all dependency inputs. These preserve the gates measured at `f13a4729f047ffe7e8f59132f46f108eeff798af` through this evidence-only head; they are not fresh gate executions.

Integration advanced again to `0f02e1219579430184111306d15e12e7f0864141` immediately before push; delivery merge head `1a87d90c0c06bc0b8fba790c419a7bdd3eb26239` preserves all nine input comparisons (exit 0), recorded in [final-preservation.json](evidence/submit-validation/final-preservation.json). A fresh global extractor check at that exact merge SHA exited **0**, with zero manifest changes; see final-extractor.json/log. Scoped Cargo/venue/control exits remain attached to the earlier measured SHA, not freshly rerun.
