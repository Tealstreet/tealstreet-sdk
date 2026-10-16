# WOO V3 WebSocket command slice gates

Branch: feat/woo-v3-ws-commands. Fresh base: 2c04dafa36dd53b183f10b974fcd0a4adf2ada1a.
First integration merge-down parent: 0f02e1219579430184111306d15e12e7f0864141 at 17c35c7a426c0ff65edd6efa95ec93386c8ab925.
Final integration merge-down parent: ccc6b553a45033e903488df180dd9ac1523b4edb, merged cleanly at b72e500c03eae469206607d852e998beaa3725f6 before delivery gates.
Measured final source/test/manifest SHA: b72e500c03eae469206607d852e998beaa3725f6. This file and its JSON are added by a later evidence-only commit;
no gate is claimed as executed at that later SHA. No unrelated unreviewed lane branch was used as an ancestor.
Our own slice remains subject to PR review.

## Completed gates and actual exits

| Gate | SHA | Result |
| --- | --- | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-woo --all-targets --jobs 2 --offline --locked | b72e500c03eae469206607d852e998beaa3725f6 | 0; Finished dev profile |
| cargo test with the same package/all-targets/jobs/offline/locked scope | b72e500c03eae469206607d852e998beaa3725f6 | 0; observed 34 targets, 85 passes, zero failed/ignored |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | b72e500c03eae469206607d852e998beaa3725f6 | 0; Would update 0 manifests; 0 added, 0 orphaned |
| python3.14 sdk-rs/safe-cex-venue-woo/scripts/ws-topics-targets.py | b72e500c03eae469206607d852e998beaa3725f6 | 0; production invention/status audit, 534 WOO/journal rows, 230 public Rust items; empty exemptions |
| rustfmt --check on v3_ws_commands.rs and tests/ws_commands.rs | 290c462dad6bd33ab6cd2875c97da2212a4384d9 | 0 |

The scoped check and full venue suite above were both measured at the final merged source SHA.
The command source/test/oracle/control files are byte-for-byte unchanged from the mutation SHA,
290c462dad6bd33ab6cd2875c97da2212a4384d9; git diff --exit-code on those paths exited 0. The earlier pre-merge full suite
also passed at that mutation SHA but is not used as the delivery suite result.
All venue tests used SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript.
Cargo runs were serial. Extractor output text was inspected; the writer ran after manifest edits and generated formatting.
The target audit uses the existing production checker binary and narrows workspace members to WOO;
it does not build other venues. Target existence is a separate claim from behavioral parity.

## Native assertions and once-per-family discrimination

The source oracle executes the whole actual TypeScript contracts/WebSocket module, exporting its actual private helpers;
no helper is rewritten. It also extracts the actual four-member command-name type alias.
Independent floors: four command literals, three wire records, two command records, 34 command operations,
nine inputs each for record predicate/record/array checks, nine enum cases, ten topic-list cases and seven channel checks.
The helper/build operation floor is 87 (34 + 27 + 9 + 10 + 7). Every walk stops on those fixed indices;
no enumeration is presented as exhaustive runtime coverage.
Decoded native enum variants, id, cmd, optional params content/presence/order, channel and nested wire fields are asserted directly.
Borrowed native object/array references and independent 0/1/2 sizes/content are asserted. Successful source list values
and the source channel validator's undefined return tag are independently pinned. There are no native re-encoded-byte
comparisons in the new target. Existing wire tests belong to the broader venue suite.

| Family | Mutation | Named assertion failed | Mutated exit | Exact bytes restored exit |
| --- | --- | --- | --- | --- |
| JSON helpers | is_record switches object predicate to array predicate | WS-COMMAND native record predicate | 101 | 0 |
| Declaration codecs | SUBSCRIBE parse maps to Unsubscribe variant | WS-COMMAND native literal | 101 | 0 |
| Command routing | unsubscribe wrapper emits Subscribe | WS-COMMAND native command kind | 101 | 0 |

All three controls compiled and failed named native assertions; each restoration passed the three-test ws_commands target.
Stopped after mutation iteration 3, with no survivors in that floor; no additional matrix is claimed.
Control SHA: 290c462dad6bd33ab6cd2875c97da2212a4384d9. The source file was restored exactly and git diff was clean before delivery.

## Row census and intentional boundaries

Python 3.14 tomllib read all 22 WOO plus journal manifests (534 rows), compared with the final ccc6b553a4 integration parent (the fresh 2c04dafa36 base has the same WOO counts).
101 ported / 57 divergent / 376 pending -> 102 / 70 / 362.
**14 pending rows moved: 1 ported and 13 divergent.** The JSON preserves all changed rows and their real Rust targets.
The command-name literal union is ported. The other rows carry CHOSEN notes: JSON/dense arrays/UTF-8 exclude host
prototype/executable/sparse/nonfinite-leaf/unpaired-UTF16 cases; immutable Rust input borrows exclude live mutable
JavaScript aliases; owned native outputs replace source frozen records/lists; string errors replace Error identity.
Within the admitted domain, validation order is ID, count, indexed topic/parser/channel checks, then duplicates.
Original noncanonical depth text and absent params are retained. Both runtimes snapshot caller params; source freezing
versus mutable native output copies is observed separately. These pure helpers do not register live exchange/socket methods.

Sub-agent review found two missing successful source-value checks (channel void tag and topic-list values); those were fixed.
The reviewer ran the source oracle (0) and inspected real mappings; it did not run native gates or mutations.
No workspace build/tests, full-workspace parity audit, TypeScript monorepo typecheck, dedicated HOW tests,
host clippy, wasm clippy, wasm build or CI rerun was run. No HOW row movement is claimed.
