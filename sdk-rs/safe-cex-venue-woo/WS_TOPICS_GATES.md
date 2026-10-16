# WOO V3 WebSocket topics gate sheet

Branch: feat/woo-v3-ws-topics. The delivery commit adds only this evidence and its reproducible scoped target-audit script.
Measured post-merge source/test/manifest SHA: `7de83b11ab54fd5ee38044c4efe3b08dae048f25`. Gates below are attached to that SHA, not the later evidence-only commit.
Mutation SHA: `c0a204a22df1b5d66c838323de656c986668e1bf`. The four new topic source/test/oracle/control files are unchanged between that SHA and the measured post-merge SHA; git diff --exit-code on those paths exited 0.

## Actual parents and scope

Fresh branch base: d7233c152349b3076a7469c481f69289589ec702.
First integration merge: 4e0ca1accd6fc8a81fb6d163581861c9860f8da9 at 2149cca68a9bc1f54823134d1374fa561385ef52.
Final merge-down before delivery: 1181108d2dfeee7dd6cb1d21a3a39922e1e13b9e at 7de83b11ab54fd5ee38044c4efe3b08dae048f25.
The source tested here includes that final integration parent. No unrelated unreviewed lane branch was used as a base;
our own slice commits remain subject to PR review. Both merge-downs were clean.

Fifteen source rows map to 42 real supporting Rust items: five literal-union declarations, four constants,
one topic descriptor declaration, two validation helpers, and three topic operations.
The public/private builders and parser are pure; no socket or live exchange lifecycle is introduced.
Four frozen constant rows and the descriptor record disclose CHOSEN ownership/alias/write-refusal boundaries.
The JSON/UTF-8 input and Result string-error boundaries are also CHOSEN. The new private coercion case preserves
its source message but does not claim to retain JavaScript Error identity/name in the native String error value.

## Completed gates

| Gate | Measured SHA | Exit / output |
| --- | --- | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-woo --all-targets --jobs 2 --offline --locked | 7de83b11ab54fd5ee38044c4efe3b08dae048f25 | 0; Finished dev profile |
| cargo test with identical package/all-targets/jobs/offline/locked scope | 7de83b11ab54fd5ee38044c4efe3b08dae048f25 | 0; observed 33 target runs, 82 passes, zero failed/ignored |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 7de83b11ab54fd5ee38044c4efe3b08dae048f25 | 0; Would update 0 manifests; 0 added, 0 orphaned |
| Production parity-check binary, narrowed WOO workspace and 22 WOO/journal manifests, no exemptions | 7de83b11ab54fd5ee38044c4efe3b08dae048f25 | 0; invention/status gates passed, 534 rows and 214 public Rust items |
| rustfmt --check on new module/test and four updated WOO runtime fixtures | c0a204a22df1b5d66c838323de656c986668e1bf | 0 |

All venue tests used SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript.
Cargo invocations were serial. Extractor output text was read; its writer ran after every manifest edit.
The target audit executed /tmp/woo-scoped-targets.py; its exact source is preserved at scripts/ws-topics-targets.py.
It uses the production checker binary previously built by the independently gated Orderly rescue;
it does not build other venue crates. Each mapped target resolves; target existence is separate from behavioral proof.

## Discrimination, stopped after iteration 3

| Family | Mutation | Named native failure | Mutated exit | Restored exit |
| --- | --- | --- | --- | --- |
| Contracts | maximum_topics_per_command 20 -> 21 | WS-TOPIC native maximum command topics | 101 | 0 |
| Literal unions | first bbo parse mapping Bbo -> Bborpi | WS-TOPIC native public enum | 101 | 0 |
| Topics | includes_rpi comparison uses orderbookupdate instead of orderbookupdaterpi | WS-TOPIC native includes RPI | 101 | 0 |

Every mutation compiled, failed a named native assertion, restored exact bytes, and then passed the focused three-test target.
No survivors were observed in this three-control floor; no broader mutation matrix is claimed.
The source oracle extracts actual source declarations/functions. Its independent operation floors are
44 public builds, 11 private builds, 56 parses, eight string validations and eight symbol validations (127 total).
The native tests use those fixed index walks and assert every decoded descriptor field and literal enum variant directly.
There are no native re-encoded-byte comparisons in the new target. Existing wire tests in the wider venue suite are separate.
The new private coercion JSON case {"toString":null} observes source TypeError and message;
the native String error is asserted against that message. Source descriptors are observed frozen;
mutable native copies are a declared boundary. These enumerations are floors, not exhaustive runtime behavior.

## Independent tomllib census

Python 3.14 tomllib read all 22 WOO plus journal manifests (534 rows), compared to integration parent 1181108d2d:
96 ported / 47 divergent / 391 pending -> 101 / 57 / 376.
**15 pending rows moved: 5 ported and 10 divergent.**
WS_TOPICS_GATES.json preserves each changed source row, target list, measured command, SHA and exit.
Four older history rows received exact extraction-target notes only and add zero row movement.
The monotonic-clock changes repair four venue test fixtures for the merged shared runtime interface and add zero movement.

Earlier attempts are not final passing gates: initial scoped check at 83d8c687ce exited 101
(missing output eligibility and merged monotonic test-runtime methods, fixed); the initial target-audit scratch workspace
exited 1 for a missing dependency symlink, then an audit exposed supporting targets and four inherited target-note gaps
(all repaired without exemptions). An extractor invocation started during final merge and recorded the pre-merge SHA;
it is not counted as a post-merge gate. A separate clean post-merge extractor check passed at the SHA above.
The earlier full venue suite also passed at c0a204a22df1b5d66c838323de656c986668e1bf, but the post-merge suite above supplies the delivery result.

Not run: workspace build/tests, full-workspace parity checker, host clippy, wasm clippy, wasm build,
TypeScript monorepo typecheck, CI rerun, or dedicated HTTP-over-WebSocket tests. No HOW row movement is claimed.
Sub-agent review examined the actual source and final changes; its coercion-message and frozen-copy findings were fixed.
