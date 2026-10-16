# MEXC private handler repair evidence

Code and fixture SHA: `c5dd833119ebcf3c1069ab9f84204d4c9473c7f8`. Actual parent: `66187a4f131eb566132308a5ce6211d001d429b1`.
Integration base: `f853d56fd227c2106dba609141b1fdd44bcab125`. **Reviewed but unmerged dependency:** PR7921 at `d73b0a8fba70668ddabd31e152026d7c7bb8b08d` supplies retained native error origins. This branch includes that work; it is not an independent integration-only slice.

Five leading FINDING rows become method-specific MIXED rows: handleOrder, handlePosition, handlePlanOrder, handleStopOrder and applyStopLikeFrame. Statuses remain **145 ported /40 divergent /0 pending** across six manifests and185 symbols; **0 pending rows moved**,0 divergent-to-ported. Leading buckets:17 CHOSEN /9 MIXED /14 FINDING. The separate fill/public-array repairs are not ancestors. This is not closure of every MEXC finding.

The repair reads markets for each source row, releases the Store borrow before clock callbacks, preserves symbol-only outer lookup and TypeError names, makes null rows abort the handler's batched order/position write and subsequent hooks, uses the position normalizer's source-default mode and propagates fallible seen/shield hooks. Existing constructor callers remain accepted. `handle_fill` is byte-identical to the baseline parent; this slice does not replace its separate repair.

Native stored info remains an owned snapshot ([MANIFEST-SPEC](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias)); the actual producer mutation witness sees source42/native7. A source stored Order/Position symbol can contain a lone surrogate, whereas native whole-frame Value ingress refuses it ([MANIFEST-SPEC](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict)). BMP, astral and whitespace positives run as real handler calls. Opaque source Error.code313 is directly readable; native retains typed cause identity and313 under Error::source but does not project that field at top level. That is a separate documented CHOSEN interface cost, not covered by either representation anchor.

## Measured floor

Four focused Rust tests execute **86 actual-source roots**, with **80 native fixture entrypoint invocations and6 native whole-frame Value parse refusals**. Store12, rows32, references8, parenthooks10, Unicode24 source roots. Native Unicode runs18 real entrypoint invocations; six refusals invoke no handler and are not counted as method calls. These count fixture calls into mapped methods; they are not a census of every nested normalizer/helper invocation. Getter, subscriber and hook counts are recorded separately below. All native decoded fields, identity, bit/value, ordered-effect and independent call-count assertions execute before their secondary source comparisons. Enumerations stop at these fixtures; no exhaustive input claim.

Store witnesses count17 first/9 later subscriber calls per runtime, exact write sequence and committed state; source strict Error identity and native typed opaque identity313 are independently pinned. Rows count28 market getters per runtime,4 first/4 later subscribers and16 named TypeErrors. Live-market callbacks replace actual Store contract size .5 with3: both getters remain observable and normalized amounts/contracts must be[1,6]; aliases use marker7→42. Reference totals per runtime:12 getters/8 real date callbacks. Parent-hook failures count20 getters per runtime/6 first/6 later/3 rejection/14 seen/6 shield calls and preserve exact committed prefixes. Unicode source counts24 getters/24 first/24 later/16 seen/16 shield, native positive calls18/18/18/12/12. Six unrepresentable native frames do not contribute fabricated zero-call assertions.

The committed baseline at `66187a4f131eb566132308a5ce6211d001d429b1` compiled and failed with exit101 at **private native per-row market getter calls** (1 vs2), the **first root, order/failAt1, of12**. Later roots and source comparison were not reached on that negative run. Uncommitted positive preparation exited0, recorded separately with sha=null; it is not final-SHA coverage.

Five deliberate compiled controls each fail at a named native runtime assertion with exit101, then pass restored with exit0:

1. Keep two market getter calls but reuse the stale first selected market: **private native each order uses newly returned market**.
2. Reverse only terminal status writes: **private native exact Store write sequence order/2**.
3. Flatten the first symbol TypeError: **private native exact TypeError name**.
4. Swallow the seen hook error: **private native original parent hook error propagates**.
5. Swallow the shield hook error: the same propagation assertion at the shield failure.

Stopped after control5; **no survivors in this five-control floor**. Compiler-only failures are explicitly rejected. Default mode, Unicode, null cases, primitives and plan/stop info flags are positive assertions, not separately mutated claims.

## Gates at the code SHA

| Named gate | Measured exit |
|---|---:|
| mexc-private-writer | 0 |
| mexc-private-native | 0 |
| mexc-private-existing-regressions | 0 |
| mexc-private-scoped-check | 0 |
| mexc-private-control-1-red | 101 |
| mexc-private-control-1-restored | 0 |
| mexc-private-control-2-red | 101 |
| mexc-private-control-2-restored | 0 |
| mexc-private-control-3-red | 101 |
| mexc-private-control-3-restored | 0 |
| mexc-private-control-4-red | 101 |
| mexc-private-control-4-restored | 0 |
| mexc-private-control-5-red | 101 |
| mexc-private-control-5-restored | 0 |
| mexc-private-restored-diff | 0 |
| mexc-private-extract-check | 0 |

The writer/check outputs both report0 added /0 orphaned, inspected from output text. JSON contains exact commands, actual exits, assertion names and excerpts. Cargo processes ran serially with jobs2. Sub-agent read-only review of the committed changes found no blocker and confirmed handle_fill unchanged; the reviewer ran no cargo.

Not run: full venue/core/workspace test suites, SDK workspace compilation for this slice (no shared type or trait changed), host/WASM clippy, WASM builds/tests, TypeScript typecheck, live transports or CI. The dependency's workspace evidence belongs to its own measured SHAs and is not relabeled as private-slice coverage.

## Integration merge follow-up

Merge SHA: `661fc942dfca96c12f3c00778ffeac1834bc6124`, actual first parent `cbb2a19028b0050ce480021539c0b2f5f0480667`, merged integration `d74a1c01f77439f9df61fa330063be194aab7576`. Original16gates remain bound to `c5dd833119ebcf3c1069ab9f84204d4c9473c7f8`. The incoming12files are CI recovery documents/artifacts, the types CI-loader helper/test and a CI-wiring test. No own Rust/TS/fixture/Cargo/manifest input changed; scoped byte-diff exits0 against the tested code SHA. The MEXC source loader uses its explicit isolated compiler/runtimeRequire binding rather than the changed types CI-loader helper.

Fresh measured exits at the merge SHA:

| Named gate | Exit |
|---|---:|
| mexc-private-postmerge-input-diff | 0 |
| mexc-private-postmerge-writer | 0 |
| mexc-private-postmerge-scoped-check | 0 |
| mexc-private-postmerge-extract-check | 0 |
| mexc-private-postmerge-restored-diff | 0 |

The original four focused native tests,13 existing private regressions and five compiled controls were not repeated at the merge SHA; their tested inputs are byte-unchanged and their earlier SHA coverage remains explicit. Reviewed, unmerged PR7921 remains an ancestor dependency. Evidence-only commits after this merge do not relabel these gates as rerun on their SHAs. No additional workspace, clippy, WASM, live or CI coverage is claimed.
