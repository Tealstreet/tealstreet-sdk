# WOO V3 private account, asset and futures reads

Fresh branch feat/woo-v3-private-account-models from **d5c0e7d58a7f03c8163680335e7b632ebb43ec86**.
Actual measured source/test/script/manifest SHA: **03f0a98f43ba777a76eb875980cfade316b51217**. Its immediate parent is
**eee1e92b8528608f2a563659296ac73a1e233fb6** (the source feature); the measured commit adds only
the corrected asset field-count notes. Both descend directly from the fresh integration base above.
No unrelated unreviewed branch is an ancestor; source feature plus its corrected asset-count note
are the only new work. Merge integration parent **95b56a79bc646156e1812b17428e0fb6295413a5** at
**a5de91a046ac99af29d01be35efb41185f7e369e**. Scoped source/tests/scripts/private-manifest survival comparison exited 0.
All checked direct dependency inputs remained byte-identical. The original source gate exits stand for the unchanged slice.
Post-merge extractor/census each exited 0 at the separate merge SHA, with the same 32 moves versus
the actual integration parent. This sheet/JSON are committed later as evidence only; source gates
are not claimed as executed at that documentation SHA.

## Completed source gates at 03f0a98f43ba777a76eb875980cfade316b51217

| Gate | Measured exit and result |
| --- | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-woo --all-targets --jobs 2 --offline --locked | 0; Finished dev profile |
| cargo test with same package/all-targets/jobs/offline/locked scope | 0; 41 targets, 114 passed, 0 failed, 0 ignored |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0; 7341 symbols/512 files; Would update 0, added 0, orphaned 0 |
| python3.14 scripts/ws-topics-targets.py | 0; scoped invention/status audit: 534 rows, 374 public Rust items |
| rustfmt --check --edition 2024 src/v3_private.rs tests/private_account.rs | 0 |
| node scripts/private-account-oracle.cjs | 0; actual whole private/types/safe-math/math/logging modules and pinned declarations |
| python3.14 scripts/private-account-census.py d5c0e7d58a | 0; 32 independently expected pending rows |
| python3.14 scripts/private-account-controls.py | 0; six compiling named native failures and six restored passes |
| git diff --exit-code after controls | 0; exact tracked source restoration |

Cargo used /opt/homebrew/opt/rustup/bin/cargo, serial invocations/jobs 2. Oracle/tests/controls used
SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript. The extractor output
text was inspected; parity:extract was the manifest writer after both manifest edits, and rust_files
came from the actual target union. The target audit scopes 22 WOO/journal manifests and dependency
symlinks, not workspace compilation. Full commands, SHA receipts, patches, call sites and named
failure/restored summaries are tracked in PRIVATE_ACCOUNT_GATES.json.

## Decoded native fields and finite floors

Six tests directly inspect all fields of PageMeta(3), AccountInfo(21), AssetBalance(16),
AssetBalanceReadModel(16), FuturesPosition(13) and PositionReadModel(16). Six declaration fixtures
include infinity, negative zero and an unvalidated decimal declaration string, separating number/string
declarations from strict runtime predicates. All three mode/side unions are decoded as their own
native variants, stopping at 2/2/3; 5 invalid kinds/literals per enum are refused. Fixed null/array/empty
record cases test declaration object shape. AST assertions independently pin six Readonly field names,
requiredness, full field types, account Main/Sub literals and all three gateway unions.

Every account/asset/futures decoded field and mapped numeric field is compared directly against actual
source values, using distinct decimal spellings and f64 bits. Output inspection includes typed owned
account info, independently identified free/frozen/pending-side values, signed holdings/native side/mode,
zero filtering, ADL/timestamp and infinity notional. There is no Rust decode/re-encode equality or numeric
serde_json::Value intermediary in this test target. Native assertions precede wire claims; no separate
wire-equivalence claim is made. Source balance.info alias identity/frozen containers and native typed
snapshot stability after changing the original account are observed separately.

Source oracle walks stop at helper 250 (10 fixed names x 25 kinds), allow-empty 25, Number-conversion 39
(13 fixed strings x 3 minimum settings), symbol 9, mode 2, side/mode 6, account 16, balance 8, asset 9, asset mapping 7,
futures 12, futures mapping 15, envelope 8, metadata 6, pagination 6 and declaration 6. These are finite floors,
not exhaustive JavaScript behavior, input/branch enumeration or exchange integration.

Multiply-invalid cases assert success before timestamp/data, metadata total before per-page/current,
account field sequence, asset/position row sequence, and side compatibility before holding/zero filtering.
Position sign validation precedes mark threshold, mark conversion precedes symbol, and zero rows skip
unused invalid price/symbol fields. Minimum positive mark is Number.MIN_VALUE (f64 bits 1), not the
minimum normal float; overflow notional is returned. Account used balance is ordinary IEEE subtraction,
not safe-math rounding. Asset interest/PnL/fees permit negatives while trading/staking/price quantities
keep their distinct nonnegative minima. Pagination records callback paths, comparing to independently
captured source calls BEFORE errors/fields: invalid metadata makes zero calls; all row validation precedes
page-cap refusal. This observes which ran first, not merely two errors being present.

## One compiling control per family

| Family | Named native assertion | Mutated exit | Restored exit |
| --- | --- | --- | --- |
| declarations | PRIVATE-ACCOUNT native total | 101 | 0 |
| validators | PRIVATE-ACCOUNT native number bits | 101 | 0 |
| account | PRIVATE-ACCOUNT native leverage | 101 | 0 |
| assets | PRIVATE-ACCOUNT native tradingFree | 101 | 0 |
| futures | PRIVATE-ACCOUNT native position side | 101 | 0 |
| pagination | PRIVATE-ACCOUNT native metadata-before-callback order | 101 | 0 |

Controls overlay production at 03f0a98f43ba777a76eb875980cfade316b51217: symmetrically swap total/currentPage Serde labels; abs() erases
integer negative zero; decoded account leverage+1; tradingFree reads frozen; reverse one-way side sign;
move metadata decode AFTER row collection. Each mutation compiled and reached its named assertion.
Saved production bytes were restored and the exact single family test passed (1 pass, 5 filtered) each time.
Stopped at mutation iteration 6, zero survivors in that floor. No further matrix or broader proof claimed.
Swapping both Serde labels can mask the error in a decode/re-encode comparison; this control
asserts the decoded native total directly. No wire roundtrip was run. Callback-order mutation explicitly changes the observed trace before any response comparison.

Read-only sub-agent review found no production mismatch in the stated domain, checked all 32 unique
notes/34 real primary/supporting targets and all six unique mutation anchors. It identified two wrong
17-field prose counts, corrected to 16 before gates. A draft negative-fixture Value reparse would have
projected/rejected overflow numbers; that draft loop was removed before gates. During oracle preparation,
actual LogSeverity re-export needed its real module registered (node exit 1 then 0, working-tree only).
These preparations are not native discrimination results. The reviewer ran no cargo/mutation/oracle/
audit/extractor gate and edited no files; no exits are attributed to it.

## Census and intentional interface boundaries

Tomllib reads 22 WOO/journal manifests and independently asserts 534 known rows and 32 explicitly named
pending-symbol moves. **32 rows moved: 4 pending->ported, 28 ->divergent**.
Before 109 ported / 164 divergent / 261 pending; after **113/192/229**.
All32 notes are distinct and name their own target. Three exact gateway unions and mapPositionMode
are ported. One UTF-16/UTF-8 String adaptation is FORCED; remaining divergences are per-row CHOSEN
ownership,typed input/output or exception-interface adaptations. Two supporting types are explicitly
mapped: generic owned paginated result and operation-only typed account balance. The 28 order/algo/
transaction declaration/decoder/mapper rows in this private file remain pending with no invented target.

Dense retained JSON and typed native model inputs exclude executable getters/prototypes/boxed objects,
sparse arrays and lone UTF-16 surrogates. Optional captured absence is None; explicit null still refuses.
Declared numeric f64 fields are not silently restricted to runtime safe-integer minima. Source frozen/
readonly identities become owned public native fields and Result messages. Account type deserialization
pins Main/Sub while direct public String construction is wider. Typed mapper inputs exclude arbitrary
extra property graphs. WooV3AccountBalance is an operation-only output with typed owned info, not shared
Balance integration/standalone declaration ingress; WooV3PaginatedRows is likewise operation output.
No live adapter/store mutation/HOW row or shared type/trait changed. Futures outputs stop before a shared
Position: leverage and marginMode must be joined from later account configuration, never inferred.

Not run: workspace build/tests,full-workspace parity audit,monorepo suites/typecheck,dedicated HOW tests,
host clippy,wasm clippy,wasm build,doctests or CI reruns. No exits are claimed for them.

Final read-only sub-agent evidence review found no material issues and independently checked raw
logs, 32 mappings, six named failure/restoration pairs, DTO boundaries and merge survival. The reviewer
ran no gates or edits. Final fetch at a8e9d3882d1e2690136e27eb24be9926f073092f exited 0; integration still pointed to
95b56a79bc646156e1812b17428e0fb6295413a5, merge reported already up to date (0), and source/tests/scripts/manifest plus
all listed dependency comparisons again exited 0. This supplement changes documentation only.
