# Kucoin account response staging

Gate SHA: `b335fa74a56b69aad01eca5204ed2e7cc3480577`.
Branch: `fix/kucoin-account-response-staging`.
Parent: `025b497809c784ef9d1c015ebbd23eec8a43d4a7`, the reviewed but unmerged
PR #7850 validation repair. Integration
`1a43f3a8f25b7e5fe1c238794e79bad11e4c4788` was merged before pushing.
This branch carries that unmerged ancestor; its earlier measurements are not
reused as measurements of this repair.

## Change and remaining differences

`accounts::decode_body` now applies the permissive Axios default response
transform once to encoded/untransformed strings. Already transformed values
remain unchanged. Empty/non-JSON success and failure bodies retain the venue
fallback or transport message. Spot validation returns its fallback without
emission. No core query/response ABI changed.

A private iterative JSON builder avoids the serde DOM depth limit and private
number-key collision. It normalizes finite JSON numbers through the existing
JavaScript number formatter and normalizes ordinary object key order through
the existing shared helper. Full-document grammar validation precedes building;
malformed JSON containing a surrogate escape remains the original string.
Valid JSON containing unrepresentable string units still receives the explicit
whole-record refusal.

Two entire getAccount response-staging FINDING rows close. Tomllib remains
146 ported / 45 divergent / 0 pending: **zero status transitions**. Twenty
divergent rows still contain FINDING text; eight begin with FINDING. This is
not completion of the venue's remaining findings or owning-class assembly.
Spot validation's independent nonstring catch emission remains a FINDING.
A newly measured numeric rejection wire FINDING remains too: native Infinity
bits survive a raw `1e400` message, but the retained numeric carrier serializes
as `1e+400`, while actual TypeScript JSON.stringify returns `{"error":null}`.

Returned source userId values are observed before serialization: actual methods
return string, length one, and charCodeAt(0) D800 or DFFF. Both native methods
return the logged empty account on those inputs. The manifest keeps the
whole-record cost explicit alongside the existing typed-account choices.
Canonical policy references only:
`sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary`,
`sdk-rs/parity/MANIFEST-SPEC.md#forced-surrogate-conflict`,
`sdk-rs/parity/MANIFEST-SPEC.md#numeric-ingress-defect`,
`sdk-rs/parity/MANIFEST-SPEC.md#divergence-taxonomy`.

## Measurements

| Gate at the gate SHA | Exit and observed result |
| --- | --- |
| Scoped cargo check, Kucoin, all targets, jobs 2 | 0 |
| Scoped cargo test, lib + accounts + spot_validation, jobs 2, one test thread | 0: 1 + 8 + 10 passed |
| parity:extract:check | 0: Would update 0 manifests; 0 added, 0 orphaned symbols |
| node --test --test-name-pattern=KuCoin scripts/rust-ci-wiring.test.mjs | 0: three passed |

Cargo calls were sequential. The TypeScript compiler environment was supplied.
The writer ran after manifest reconciliation and reported zero additions and
zero orphans. The mandatory Kucoin command now includes the lib parser test;
its existing exact-command assertion was stale before this slice and is updated
to the actual full target list plus lib. Before/after the merge-down: one named Kucoin workflow step and forty integration-test targets each; lib changes from absent to present in this slice. No existing target was removed.

Native assertions run before source or wire assertions in the added witnesses.
Independent floors: eight empty/non-JSON account cases, eight account phase/depth
cases, twelve returned-userId observations across both methods, two validation
fallback cases, three rejected JSON number/object cases, and one parser group.
Parser enumeration ends at encoded/untransformed/transformed string handling;
source enumeration ends at the ordinary private-number-key rejection. Depth 170
is measured, not an arbitrary-depth safety claim for Value consumers/destruction.
Exotic prototypes/descriptors and the complete Axios adapter are not proved.

The first new fallback assertion failed before the implementation: exit 101,
`KUCOIN-STAGING native fallback`, representation diagnostic versus venue fallback.
An intermediate added wire expectation failed because the observed retained
number spelling is `1e+400`, not `1e400`. That diagnostic run was not a stable
SHA measurement because subsequent test edits overlapped it; none of its exits
are attached as covering the gate SHA. The fresh table above replaces it.

Six isolated semantic controls compiled and exited 101:

| Deliberate mutation | Observed assertion/test failure before source comparison |
| --- | --- |
| Restore unconditional json_value account decoding | KUCOIN-STAGING native fallback |
| Keep unrounded JSON integer spelling | KUCOIN-STAGING native exact rounded integer |
| Parse transformed strings again | KUCOIN-STAGING native phase/depth |
| Restore direct serde DOM parsing | KUCOIN-STAGING native phase/depth, empty user versus deep |
| Parse an ordinary private-number-key object through serde DOM | KUCOIN-STAGING native private-key object |
| Omit shared ordinary object-key ordering | native_axios_transform_floor, ordered native keys |

Enumeration ends at object-key ordering. No mutation survived this finite set.
The first decode and depth variants failed compilation because they orphaned
helpers under deny(dead_code); those exits 101 are invalid controls. Revised
variants retain a function reference, compile, and fail the named native tests.
Original four valid controls were followed by exact restoration and the same
lib/accounts/spot_validation command: exit 0, 1 + 8 + 10 passed. The revised two
controls were followed by a second exact restoration: exit 0, the same 1 + 8 + 10 tests passed. Both scoped src/tests/scripts preservation diffs exited 0 against the gate SHA.

Sub-agent review inspected the actual code, source observations, manifest notes,
and recorded gate output. It found the rounded-number test's f64 conversion
could hide unrounded storage; the independent exact integer assertion fixes it.
It also required returned-unit observations and MIXED/FORCED classification,
now present. Review independently counted 191 rows in twelve manifests, ending
at kucoinspot.ws-public. The reviewer did not run Cargo or other gates.

Not run: workspace compilation, the entire Kucoin target list, host or wasm
clippy, full monorepo suites, CI reruns, live exchange calls, other venue gates,
or complete class/Store prototype assembly.

The receipt commit changes only this manual evidence file. A full tracked diff against the gate SHA excluding this receipt exited 0; gates are measured at the named gate SHA, not claimed as rerun at its receipt descendant.
