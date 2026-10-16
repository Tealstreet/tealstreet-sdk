# MEXC historical queries, signing and retries

This slice repairs historical query values before JSON projection, recursive query sorting/signing, fresh Store reads and exception precedence, configured local-date conversion, and zero-delay host scheduling. The public shared QueryProfile now selects Preserve or JsRoot key views; render_query is fallible. All fourteen prior profile literals select Preserve explicitly. MEXC selects the pinned root collation profile documented by [the data owner](../safe-cex-num/src/collation_data/README.md). Native retained values and insertion/index order are inspected before sorted signing and URL views.

The actual branch base is `333fd419fd`; reviewed open PR7921 (`d73b0a8fba70668ddabd31e152026d7c7bb8b08d`) entered through merge `1f19b07a50702b64f47389121d3cddeaa98aea0c`. It is an unmerged ancestor, not an integration-parent claim. Its shared ExchangeError origin API is required by the rejection witness. Separate fill/public/private-handler/extractRows PRs are not ancestors of the initial gates.

## Bounded native and maintained-source witnesses

- Historical rows: fourteen actual native method/GET calls and fourteen Source calls, with eighteen native versus nineteen Source market getters. Market getter two changes the real DefaultStore contract size, producing quantities 1 and 6. Empty rows perform zero getters; false, zero, string and array rows perform four getters without fills. Later null, symbol, profit and role errors precede the existing chosen commission-asset output refusal. That refusal is deferred only for its exact identified error; other failures escape immediately. The Market clone releases its Ref before the host date callback.
- The Source row containing symbol U+D800 returns an actual Execution whose native JS units are asserted. The direct native encoded response rejects with SyntaxError after one GET and before Store reads. Native BMP, astral and internal-whitespace symbols have independent Execution assertions. See [string boundary](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary) and [surrogate conflict](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict). This differs from the actual native factory response transformation to Null below.
- Historical rejection: one actual method/GET preserves the native typed RestFailure, original request URL/query number bits and key order, code HISTORY_FAILED, HTTP status418 and body phase. This fixture retains the typed RestFailure/request/response provenance; it does not prove a nested transport cause. Source retains the original thrown Error object. Updating the producer response from7 to42 changes the Source live reference while native retains7; see [ownership](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias). Source makes an eager GET before awaiting; native creates zero requests before polling. The retained borrowed-future and structured-error ABI costs are named in the manifest.
- Local date: one actual method/GET/fill invokes the configured native host once with civil milliseconds1704164645000 and produces UTC seconds1704157445. Source Date.parse is observed under fixed UTC+02. The date-specific native DefaultStore getter is not spied, so no native getter floor is claimed for this root.
- Rich factory: five native/Source factory roots, seven adapter attempts, native nine retry-clock reads/twenty-five runtime-clock reads/two zero-delay sleeps; Source nine Date.now reads/six signing virtual-clock reads/six observed HMAC updates/two actual zero timers. Source runs the actual factory against the real Axios child boundary. Native shared HTTP adds logging/stat clock calls: virtual6 and runtime25 are different scopes, not an equality claim. Native HMAC entry counts are uninstrumented; only actual header signatures and native inputs are asserted.
- Unicode factory: five Source actual factory/GET roots versus four native roots plus one whole ExchangeOptions ingress refusal before create_api. Actual Source AxiosHeaders.ApiKey exposes U+D800. Native positive headers independently cover BMP, astral, internal whitespace and whitespace-only trimming to empty. Actual Source factory parsing and normalizeExecution return a U+D800 symbol; actual native factory returns Transformed Null.
- Exhausted retry: one factory/four attempts, native seven retry-clock reads/twelve runtime-clock reads/three zero sleeps, consumed timeout19997 and original query/request/final typed network cause. Source observes four HMAC updates/seven Date reads/three timers. The pending-host scheduler test separately proves suspension; immediately-ready fake clocks prove call counts only.
- Recursive query: five native roots/five Source roots, ten outer Source helper invocations. This does not count every nested function entry. Roots cover arrays with index10 versus2, nested arrays/objects, nonfinite values, empty/undefined/null slots, booleans and numeric negative zero versus string "-0". The original native index/order/number bits are checked before wire expectations. Unicode stable ties also run through the actual factory.

All floors stop at the named bounded corpora. The added native witnesses assert native values before secondary wire/Source comparisons. Existing [Execution output choice](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary) remains a chosen whole-record refusal rather than a ported row; this repair changes evaluation order, not that output contract.

## Exact-SHA gate records

The adjacent HISTORICAL_QUERY_GATE_RESULTS.json retains each command, measured SHA, exit and sanitized output. Earlier baseline, focused and shared executions keep their own SHA labels; none is relabelled as final-head execution. Compiled negative controls retain explicit overlays and restoration checks. Evidence-only follow-ups do not claim newly executed gates.

PREPARATION FAILURE: the initial SDK workspace attempt exited101 with a native factory test Ref/move borrow error (subsequently fixed) and crypto lib-test incremental-cache I/O errors. That run also overlapped local input changes and is marked gate=false, sha=null. It is not a passing workspace gate. Final frozen-SHA checks are recorded separately.

NOT RUN: full venue/core/workspace test suites; host or WASM clippy; WASM builds/tests; TypeScript typecheck; live transport; CI reruns; exhaustive JS objects, all locales or all cancellation interleavings; native HMAC entry instrumentation. Scoped compilation and SDK workspace checking are not execution of those suites.

## Verification records

Source-only commands: seven at `c0889258c72fe3f7d6d6dc3a28b2d41d651dc65f`, all exit0. Focused native commands: five at `bf1ede9d476ad4998ab4cb90d94bd5cec1a4b947`, all exit0. Shared commands: eight at `1465286ec3`, all exit0, 53 selected tests. Original executions retain those SHAs. The retry-budget focused command executed three tests including its clock-contract compile-fail test; the later shared command explicitly skipped that unchanged contract test.

Compiled baseline defects at `3af8a7fc6f0768ed6ccfa6a64aff4f517d4ccd97` (historical query carrier), `dbc42c6d88` (zero-delay scheduler) and `c0889258c72fe3f7d6d6dc3a28b2d41d651dc65f` (configured date callback) each exit101 at named native assertions. The focused positive commands restore those behaviors to0. Two runner-preparation anchor errors stopped before editing their proposed mutant; they were corrected and are not discrimination results.

Nine compiled deliberate controls, stopped at iteration9 (`drop-api-key`):

| Control | Measured base SHA | Broken/restored/byte restoration exits |
| --- | --- | --- |
| `nan-collapse` | `5dfea26b4b766e8818a29ee69f1936fbaf9f6c5e` | 101 /0 /0 |
| `array-sort` | `5dfea26b4b766e8818a29ee69f1936fbaf9f6c5e` | 101 /0 /0 |
| `zero-schedule` | `5dfea26b4b766e8818a29ee69f1936fbaf9f6c5e` | 101 /0 /0 |
| `eager-batch-refusal` | `40b85ad3427fcd85a87f513893d2b0515581ce33` | 101 /0 /0 |
| `eager-field-refusal` | `40b85ad3427fcd85a87f513893d2b0515581ce33` | 101 /0 /0 |
| `flatten-origin` | `40b85ad3427fcd85a87f513893d2b0515581ce33` | 101 /0 /0 |
| `ignore-rich-signing` | `40b85ad3427fcd85a87f513893d2b0515581ce33` | 101 /0 /0 |
| `drop-restored-query` | `40b85ad3427fcd85a87f513893d2b0515581ce33` | 101 /0 /0 |
| `drop-api-key` | `40b85ad3427fcd85a87f513893d2b0515581ce33` | 101 /0 /0 |

Each broken control reached its retained named assertion. No survivor was dropped. The signing binary initially waited in OS `_dyld_start` before test startup, then completed without interruption; no early pass or timeout result is substituted for its measured101. Native HMAC invocation totals remain uninstrumented.

## Integration merge and fresh gates

Integration `8fcf323edfc6747fcdf4427ce19267a7147d018b` was merged at `412bbf669cf00b83774c0fe84cde902e5b0e373f`; actual first parent `de463f9b95`. The only merge conflicts were createAPI and normalizeExecution mapping notes. Reconciliation preserves the independent audit findings and resolves these two explanation gaps with actual native/Source Unicode witnesses, citing their frozen audit anchors. No Rust or fixture input owned by the MEXC repair changed in this merge.

The scoped MEXC source/test diff against5dfea, Source fixture/reference diff againstc088 and owned shared-file diff against146528 each exit0. These are explicit path scopes, not a claim that all dependencies are unchanged. Incoming HOW/core source and tunnel-test changes required fresh selected regressions and workspace checks. The tunnel-input diff exits1 as expected. Replaying the merge whitespace check over incoming Crypto.com evidence exits2; those immutable evidence logs are not altered by this repair. The PR diff against the merged integration parent exits0. Scoped rustfmt over the twenty Rust files changed by this repair exits0. Exact paths, commands and outputs are in JSON.

Fresh selected regressions at `412bbf669cf00b83774c0fe84cde902e5b0e373f` execute 71 tests in12 serial commands, including four history tests, the legacy historical method test, seven client tests, six utility tests and the53-test shared floor. The unchanged retry clock-contract compile-fail test is skipped in this merged runtime rerun; its original positive execution retainsbf1ede9.

| Fresh gate | Exit |
| --- | --- |
| `mexc-rich-merged-regression-0` | 0 |
| `mexc-rich-merged-regression-1` | 0 |
| `mexc-rich-merged-regression-2` | 0 |
| `mexc-rich-merged-regression-3` | 0 |
| `mexc-rich-merged-regression-4` | 0 |
| `mexc-rich-merged-regression-5` | 0 |
| `mexc-rich-merged-regression-6` | 0 |
| `mexc-rich-merged-regression-7` | 0 |
| `mexc-rich-merged-regression-8` | 0 |
| `mexc-rich-merged-regression-9` | 0 |
| `mexc-rich-merged-regression-10` | 0 |
| `mexc-rich-merged-regression-11` | 0 |
| `mexc-rich-final-scoped-check` | 0 |
| `mexc-rich-final-transport-check` | 0 |
| `mexc-rich-final-sdk-workspace` | 0 |
| `mexc-rich-final-writer` | 0 |
| `mexc-rich-final-extract-check` | 0 |
| `mexc-rich-final-diff-check` | 0 |

The final SDK workspace all-targets keep-going compilation exits0; this supersedes the failed preparation only as a separate frozen-SHA measurement. It is not a workspace test-suite run. Writer and extractor outputs were checked for their completion footer in addition to recording exits.

Tomllib over six manifests/185 rows, stopping at mexc.ws-public.ts.toml: **145 ported /40 divergent /0 pending**, leading buckets6 CHOSEN/4 MIXED/30 FINDING. **0 pending rows moved** and0 divergent-to-ported changes. Relative to current integration, this slice resolves historical-query behavior and two audited explanation findings; reviewed unmerged ancestor PR7921 additionally resolves three earlier error-origin findings. The incoming audit increased the known finding floor, so this is not closure of all MEXC findings.

Native sub-agent context_review and account_value_review found no new blocker in committed5dfea source. Evidence review caught and corrected error code/status attribution, an unproven nested transport-cause claim and a prose/control-count mismatch. Reviewers ran no gates; their code review is not execution evidence. Remaining whole-class composition, raw account error/reporting and other open findings are not claimed fixed.

## Final evidence review

Native sub-agent context_review reviewed PR7983 at `e8063dcb9c59a8bbb1991bd65c6cf2cb6d2f1751` and found no blocker. The corrected metadata/cause/count claims, original control SHAs, fresh 71-test floor, ancestry warning and bounded limits were checked. The reviewer ran no Cargo, tests or oracles and made no edits. The SDK-input diff from the frozen gate SHA412bbf669c to e8063dcb9c, excluding only these two evidence files, exits0; the exact command and empty output are retained in JSON. This follow-up records review and unchanged inputs, not newly executed runtime gates.
