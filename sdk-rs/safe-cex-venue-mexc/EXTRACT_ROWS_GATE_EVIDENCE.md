# MEXC extractRows gate evidence

Code/fixture SHA: `a1825c2732dcdc3d9cd776ae904f060bbf791511`. Actual parent: `8bed4b60947d05aebac5586eb39d2ef0181062ba`. Fresh integration base: `d74a1c01f77439f9df61fa330063be194aab7576`. No unmerged repair branch is an ancestor; other MEXC repair PRs are outside this census.

`extractRows` now uses the existing shared exact array-property primitive, returning the pristine stock Array.prototype before a later items fallback. Canonical index0 differs from00/-0. The old552-case regression exception that explicitly accepted native[7] versus source[] has been removed. The shared primitive's visibility changes; its existing enum/type and algorithm do not. No shared type or trait changes in this slice.

One leading FINDING becomes MIXED: specific FORCED returned-row ingress loss and two specific CHOSEN prototype costs. Statuses remain **145 ported /40 divergent /0 pending**; **0 pending rows moved**,0 divergent-to-ported. Leading buckets:14 CHOSEN /5 MIXED /21 FINDING. Independent tomllib census covers six manifests/185symbols, stopping at `mexc.ws-public.ts.toml`; it does not aggregate the other open MEXC branches or claim every inline finding closed.

## Native-first witness floor

The focused test executes **ten actual-source helper invocations acrossnine output roots**, **nine native helper invocations**, and **one native whole-input Value refusal**. The refusal invokes no helper and is not counted as a method call. These are outer fixture calls, not a census of all nested property operations. Native decoded values and nonempty input-row owner pointers are asserted before Node/source comparisons.

The first four calls pin0→[1,2],00/-0→[7],__proto__→[]. Two additional calls retain and reread the pristine prototype. Source mutates its actual realm-owned Array.prototype by pushing marker42, then verifies both the earlier returned array and a fresh lookup see the same mutated owner. Native repeats real helper calls against its pristine immutable model; this fixture does not fabricate a native prototype-mutator API. Retained static-prototype observation is an explicit cost ([MANIFEST-SPEC](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias)). Separately, fresh native lookups omit already-mutated realm prototype contents: keep the shared JSON array primitive and borrowed slice ABI rather than add realm-owned mutable prototype state. This second difference is neither fixed nor absorbed into the ownership anchor. Ordinary native input rows remain borrowed read-only views, not owned snapshots.

The source helper also returns a row whose symbol's program-visible charCodeAt(0) isD800. It proves the returned original-array owner, string type and exact UTF-16 units before JSON. Native refuses the whole input at Value parsing, before helper invocation ([MANIFEST-SPEC](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict)). Three native BMP/astral/whitespace positives invoke the actual helper, assert returned units and borrowed input-row ownership; matching source positives run separately. Refusal is not mislabeled invalid JSON or a zero-call runtime probe.

## Discrimination and interrupted attempt

At baseline `9dd0eacbf88acb0c5fc2443e0c5a3c96d80602f2`, the original fixture compiled then exited101 at **extractRows native exact array property precedence __proto__**, actual[7]/expected[]. It stopped at native invocation4 of its then-six planned calls; later reads and Node comparison were not reached.

At `8bed4b60947d05aebac5586eb39d2ef0181062ba`, writer, original six-call native fixture and552-case helper regression exited0. Sub-agent review then found the missing program-visible returned Unicode witness and incomplete CHOSEN taxonomy. The own shared-number cargo invocation was intentionally interrupted with measured subprocess exit**-2**; the runner exited1. Those are not passed gates. No scoped checks, controls or extractor check completed in that interrupted pass. Its exact partial records are retained separately in JSON. The new fixture/note commit adds the witness and uses MIXED; all twelve final gate records below bind to the revised SHA.

Two deliberate compiled controls fail with exit101 at named native assertions, then pass restored with exit0:

1. Replace shared ArrayPrototype result with Undefined: **extractRows native exact array property precedence __proto__**, stopped on call4 before source comparison.
2. Replace canonical index conversion with u32 parsing, admitting00: **extractRows native exact array property precedence 00**, stopped on call2 before source comparison.

Stopped after control2; no survivors in this two-control floor. Compiler-only failures are rejected. The Unicode boundary/positive cases are asserted, not separately mutated proof claims.

## Measured gates at the code SHA

| Named gate | Exit |
|---|---:|
| mexc-extract-rows-writer | 0 |
| mexc-extract-rows-native | 0 |
| mexc-extract-rows-existing-helpers | 0 |
| mexc-extract-rows-shared-unknown | 0 |
| mexc-extract-rows-safe-cex-num-check | 0 |
| mexc-extract-rows-safe-cex-venue-mexc-check | 0 |
| mexc-extract-rows-control-1-red | 101 |
| mexc-extract-rows-control-1-restored | 0 |
| mexc-extract-rows-control-2-red | 101 |
| mexc-extract-rows-control-2-restored | 0 |
| mexc-extract-rows-restored-diff | 0 |
| mexc-extract-rows-extract-check | 0 |

The focused test is one test; the existing MEXC helper test checks552cases and the selected shared-number regression checks2961rows plus its two refusal witnesses. These are existing bounded regression floors, not complete behavioral enumerations or additional native-first proof. Native-first ordering is claimed only for the focused fixture. Scoped all-targets checks cover safe-cex-num and safe-cex-venue-mexc separately. Cargo runs serially with jobs2. Writer and check output text reports0 added/0 orphaned; this is not inferred from Yarn exit alone. JSON retains commands, SHAs, measured exits, named assertions and excerpts.

Local adversarial sub-agent review caught the omitted Unicode case; the committed fix and updated MIXED note received no blocking finding by inspection. Reviewers ran no cargo.

Not run: full venue/num/core/workspace test suites, SDK workspace compilation, host/WASM clippy, WASM builds/tests, TypeScript typecheck, live transports or CI. No proof of a mutable native realm-prototype owner or full production MEXC class composition is claimed.

## Integration merge follow-up

MergeSHA `b8e922138b9b139a70c46508068e66eb1b49f56f`, actual first parent `e0574a4850f22ae73169b4e144ac6c396417a6fc`, merged integration `fada0ab29c9534bf4b51ec6e9c9385cf490b5b4c`. Incoming changes: Bybit gate evidence and inverse-read test only. Own measured Rust/source/fixture/Cargo/manifest/extractor inputs are byte-unchanged from `a1825c2732dcdc3d9cd776ae904f060bbf791511`; scoped git diff exits0.

Fresh measured gates at this merge SHA:

| Named gate | Exit |
|---|---:|
| mexc-extract-rows-postmerge-input-diff | 0 |
| mexc-extract-rows-postmerge-writer | 0 |
| mexc-extract-rows-postmerge-extract-check | 0 |

Native tests, existing regressions, both scoped cargo checks and both controls were not repeated: measured inputs are unchanged and original exits retain their original SHA. This merge contains no unmerged MEXC repair ancestry. Evidence-only follow-up commits do not relabel any gates as rerun at their SHAs. No additional workspace, clippy, WASM, live or CI coverage is claimed.
