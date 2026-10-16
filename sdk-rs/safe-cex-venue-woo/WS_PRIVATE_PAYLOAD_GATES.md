# WOO V3 private payload and data-frame gates

Branch: feat/woo-v3-private-payloads. Fresh base: 65e086a3c9039a79e4c7e9f354aba31d8f83009e.
Integration merge-down parent: 602630e279b6a3eb284950215db6197a7d750eec, merged at 8e73e95d8273a06b509468f8231b54481990b0bb.
Measured source/test/manifest SHA: **f76f5a71b5fde4ed1306f3f398bbc397bfa878c6**. This sheet and JSON are added in a later evidence-only
commit. Gates cover the measured source SHA; no gate is claimed as executed at the documentation commit.
No unrelated unreviewed lane branch is an ancestor; these source commits await the user's PR review.

## Completed measured exits

| Gate at f76f5a71b5fde4ed1306f3f398bbc397bfa878c6 | Exit and observed output |
| --- | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-woo --all-targets --jobs 2 --offline --locked | 0; Finished dev profile |
| cargo test with the same package/all-targets/jobs/offline/locked scope | 0; 38 targets, 100 passed, 0 failed, 0 ignored |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0; 7341 symbols / 512 files; Would update 0 manifests, 0 added, 0 orphaned |
| python3.14 scripts/ws-topics-targets.py | 0; scoped invention/status audit: 534 rows, 291 public Rust items |
| rustfmt --edition 2024 --check src/v3_ws_private.rs tests/ws_private.rs | 0 |
| node scripts/ws-private-oracle.cjs | 0; actual TS execution and declaration AST checks |
| python3.14 scripts/ws-private-census.py (fresh base and merge parent) | 0 each; 14 independently expected pending rows moved |
| python3.14 scripts/ws-private-controls.py | 0; six named compiling failures, each restored and passed |
| git diff --exit-code after controls | 0; exact tracked source restoration |

Cargo commands used /opt/homebrew/opt/rustup/bin/cargo and ran serially. Source tests and controls used
SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript. Full commands, exits,
SHA anchors, patches, call sites and assertion excerpts are in WS_PRIVATE_PAYLOAD_GATES.json.
Extractor output text was read; parity:extract was used as the writer after manifest edits.
rust_files was derived from the real target union, not hand-spelled. The target checker uses 22 selected
WOO/journal manifests with sibling dependency symlinks; it is a scoped audit, not a workspace claim.
The earlier check at 8e73e95d8273a06b509468f8231b54481990b0bb exited 0 before the boolean fixture addition;
its SHA-bound command/exit receipt is retained and does not cover that later test change. The initial
raw log was overwritten by the final check and is not retained; its obsolete log pointer was removed.

## Native assertions and finite floors

The oracle executes the actual TypeScript contracts/WebSocket modules and checks source type AST
field names, requiredness, types, literal sets and Readonly wrappers. Independent field floors are
account 2/9/10, balance 20, position 21, execution 30/5/3 and algo 44. Both decoded frame branches are
pinned including channel literals, topic Extract types and the empty public blocker tuple.
All native fields, enum variants, vector counts, blocker code/reason/requirement and numeric bits are
asserted directly before any wire assertion. No Rust decode/re-encode comparison appears in this target.
Source number tags are observed before JSON projection; raw ingress preserves -0 and infinity lexemes.

Fixed walks stop at declaration fixture 9, account case 13, balance case 11, position case 14,
execution case 20, algo case 13, private dispatch case 5 and frame case 16. Nine declaration fixtures
are also decoded through the nested private payload and frame unions, including an infinite frame
timestamp. Seven boolean basis cases each set one of three Fill or four Algo booleans true and all
others false. Separate allowed-empty cases accompany distinct baseline string values. These are finite
floors, not exhaustive enumeration of JavaScript values or field mutations.

Multiply-invalid fixtures compare the first returned source error message: account mode/value/common
fields precede branch fields; ordered row fields precede later invalid fields; execution message type
precedes variant selection; rejection order ID precedes client ID; frame topic precedes timestamp,
then payload. Source Error names are pinned to Error separately from native Result messages.
Source frozen records, row arrays, frames and blocker arrays are observed separately from native ownership.

## One discriminating control per new family

| Family | Named native assertion observed failing | Mutated exit | Restored exit |
| --- | --- | --- | --- |
| declaration-models | WS-PRIVATE native WooV3WsBalanceRow holding | 101 | 0 |
| account | WS-PRIVATE native WooV3WsAccount freeCollateral | 101 | 0 |
| table-rows | WS-PRIVATE native WooV3WsPositionRow pendingLongQuantity | 101 | 0 |
| execution | WS-PRIVATE native WooV3WsExecutionReport executedPrice | 101 | 0 |
| algo | WS-PRIVATE native WooV3WsAlgoExecutionRow triggerPrice | 101 | 0 |
| dispatch-and-frame | WS-PRIVATE native blocker code | 101 | 0 |

Controls overlay production source at f76f5a71b5fde4ed1306f3f398bbc397bfa878c6; each compiles and fails its named native assertion,
then restores the exact saved bytes and passes all five ws_private tests. The declaration control
symmetrically swaps holding/frozen Serde aliases, demonstrating discrimination that a roundtrip would
miss. Other controls redirect account free collateral, position pending long quantity, execution
executed price, algo trigger price, and balance blocker selection to distinct valid sibling values.
Stopped at mutation iteration 6 with zero survivors in this floor. No larger matrix is claimed.

A native review sub-agent checked source evaluation order, declaration graph, all 15 real targets for
14 rows, per-row notes and six unique anchors. It found proof gaps in optional strings, nested numeric
values, frame AST requiredness and boolean sensitivity; all were fixed before the final gates. Its
initial source oracle exited 0; no Rust gate, audit or control run is attributed to the reviewer.

## Independent census and adaptations

Python tomllib reads 22 selected manifests and independently asserts 534 rows and the separate
fourteen-symbol expectation. Fresh base and merge parent both start at 103 ported / 108 divergent /
323 pending. Final counts: **103 ported / 122 divergent / 309 pending**.
**14 pending rows moved: 0 ported, 14 divergent**, each with a distinct canonical CHOSEN note naming
its own real function/type. Fifteen targets include the supporting private decoder return record.

Source Readonly/frozen objects become owned native records/vectors; JSON literals are enforced while
public String/bool/f64 fields permit wider native construction. Source Error identity, executable
objects/accessors/prototypes, sparse arrays and lone UTF-16 surrogates are outside retained dense JSON.
The declaration private union canonically selects Balance for an ambiguous empty array; topic-directed
operation decoding preserves the requested variant. Both behaviors are asserted separately. Frame
JSON decoding enforces channel/topic branch membership and zero public blockers; direct construction
with the shared topic descriptor can be wider. Numeric declaration fields retain unrestricted f64
bits while runtime integer/decimal requirements follow the source validators in their evaluation order.
Existing private blockers remain attached; no undocumented lifecycle or replacement semantics are
invented. This slice does not register live stores/sockets and moves no HOW row.

Not run: workspace build/tests, full-workspace parity audit, monorepo suites/typecheck, dedicated HOW
tests, host clippy, wasm clippy, wasm build, doctests or CI reruns. No exits are claimed for those.
