# WOO runtime route declaration evidence

Moved **10 pending rows: 3 ported, 7 divergent**. Python tomllib census against actual fresh parent
`a07e971bb08f867925ff18ca8b13d2f288d0deba` checks a fixed independent ten-symbol expectation.
WOO at final gate: **158 ported / 249 divergent / 125 pending**.
Assigned two-file partition: **3 ported / 10 divergent / 37 pending**.
Other WOO lanes' merged rows are not claimed as our movement. No edits to their native/TS/manifest hunks.

Final gate SHA: **8c234d506dd11236ebd51b0bc6889c70867e013c**. This report is committed later; it does not relabel exits as measured at its documentation commit.
Production control SHA: **49b8cceed74da74ea35b63ff0dd580f620c1c34f**. Source/test assertions changed later only by adding the independent unit-array count;
production runtime_selector.rs and control/domain/original oracle scripts compare byte-equal from that control SHA.
Actual integration parents merged down: `4afb5dbf48e5f071608ee23c74e367a107695439` then
`51a76599506b631dbe122bcc00a259e4a8aeb259`, then `9c663a908f40c2955e27b1ce58f97fb5c08fb331`. No unrelated unreviewed feature ancestor was used;
our own feature commits await the user's review. No PR opened, merge to integration or history rewrite.

## Final executed gates

All cargo commands ran serially, jobs 2. Oracle env:
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Commands, actual subprocess exits, complete output and SHA binding are in RUNTIME_PLAN_GATE_RESULTS.json.

| Gate at 8c234d506dd11236ebd51b0bc6889c70867e013c | Actual exit |
| --- | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-woo --all-targets --jobs 2 | 0 |
| cargo test, same package/manifest/jobs, --lib --test runtime_selector --test legacy_socket_control --test private_socket | 0; 26 passed across 4 targets |
| git diff --exit-code restoring control source/test scope | 0 |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract | 0; Updated 0 manifests; 0 added, 0 orphaned |
| same node/yarn parity:extract:check | 0; Would update 0 manifests; 0 added, 0 orphaned |
| python3.12 scripts/runtime-plan-census.py | 0; fixed ten-row transition expectation |

Earlier actual gates, separately bound:
- Full WOO --all-targets test run at `83ba64fcac19585cb371b86066eca54f43e9eb23`: exit 0, 144 passed, 45 targets, zero ignored. This is NOT a full-suite claim at final head.
- At `323b6c0a71a16dc2ee8d5224fb97080344754d97`: fresh scoped check 0, library/runtime/added legacy_market_reads targets 0 (23 tests), writer/check 0/0.
- At `49b8cceed74da74ea35b63ff0dd580f620c1c34f`: fresh scoped check 0; library/runtime targets 0 (16 tests); writer/check 0/0; all three controls below.
- Initial dirty preflight check and writer exited 0 before the first source commit; they are diagnostics, not committed-head gates.
- One preservation helper attempt exited 1 because it mistakenly used ba65de95b1 across our later domain/test changes.
  Corrected fc5264802f-to-final merge proof exited 0; the failed attempt is not claimed as merge preservation.
- The earlier 8P/2D declaration classification was corrected after reviewing §§14/19; its census is superseded.

## Native proof floors and disclosed loss

Readonly native structs/enums now carry actual selector results; the JSON compatibility API delegates to them.
Private fields plus borrowed accessors preserve readonly access; source helper/source selector frozen outputs are observed separately.
All native case assertions precede any new wire projection. Wire property order additionally matches actual source.
Direct enum decoding compares 26 independently listed named route variants and three named modes, before string/wire checks.
Source walk stops at route 26 / route-mode case 544 (32 IDs times 17 modes), and standalone helper target 8.
Outside-union refusal stops at mode 4 / ID 4; source declaration loss stops at record 6 / slot 7;
scalar, astral-pair and uninterpreted backslash-u positives stop at 3. These are finite floors.

ID, RuntimeMode and Blocked declarations are ported: their declared strings are closed ASCII literals.
Five target-bearing declarations are FORCED under the required String/Value surface: source path U+D800 is
program-observable, and native shared JSON ingress refuses the WHOLE record. Target uses the actual source helper;
enclosing records use valid typed frozen declaration fixtures, not a claim that every fixture is emitted by the selector.
InvalidMode and the union are MIXED: observable surrogate refusal plus chosen retained JSON/snapshot/host-object restrictions.
Scalar, paired Unicode and literal ASCII escape text are preserved without replacement. No malformed-JSON claim or lossy substitution.
The V3 constructor and source declaration fixture remain dormant: all eight real gates stay false; no override or runtime reachability proof.
Annotations are inspected against source declarations, not a TypeScript typecheck claim.
Existing target/select function notes were corrected to MIXED with their own actual-source returned-unit observations;
their statuses were already divergent and add zero to moved rows. requireRoute diagnostic consumer taxonomy was not broadened.

| Control at 49b8cceed74da74ea35b63ff0dd580f620c1c34f | Named native assertion | Mutated / restored |
| --- | --- | --- |
| Paired enum serde/as_str markets/tickers swap, production source | typed native decoded route ID discriminant markets | 101 / 0 |
| Swap both shadow target paths at production construction | typed native shadow authoritative path | 101 / 0 |
| Simulate U+D800-to-U+FFFD normalization at test shared-parser ingress | native whole-record surrogate representation refusal target | 101 / 0 |

The third is explicitly a TEST-INGRESS overlay, not a production mutation. All compile and fail assertions rather than compiler errors.
Each restoration passed all five runtime tests and restored exact bytes; stopped after mutation iteration 3, no survivors in this floor.
The existing historical seven-control runtime-selector matrix was not rerun.

## Merge preservation and review

Both integration merges were conflict-free. Own selector source/test/oracle/manifest scope: scoped git diff --exit-code 0.
Whole crate src/tests: diff 1, first for legacy_market_reads and its lib export, then legacy_order_control and its lib export.
All-target compilation and affected WOO targets were refreshed accordingly. Third merge: whole WOO src/tests diff 0, shared websocket.rs diff 1 (private before-close hook, default None; WOO has no setter call). Fresh scoped check plus library/runtime/socket targets passed at the final gate SHA. Earlier shared dependency/TS/Cargo inputs diff 0;
481-package WOO locked closure is identical, despite the unrelated lighter-sdk lock change. Reports retain exact comparison commands.
Review by context_review: no remaining blockers at fc5264802f; reviewer ran no gates/builds.

Not run: workspace build/test/sweep (no shared type or trait change), full WOO suite at final head,
host/wasm clippy, wasm build, monorepo suite, TS typecheck, live exchange/network/browser integration,
CI reruns, diagnostic consumer audit, other lanes' mutation matrices, doctests. No exits claimed for these.

## Final metadata merge before push

Integration `d1652df92999c41782adbd5be274485279db548b` merged cleanly at
`c9f118b23e94f9d6e9269556300d7af7fe5b4640`. Scoped git diff --exit-code from
8c234d506d over WOO src/tests/scripts and shared native/Cargo inputs exited 0.
Native exits at 8c234d506d stand under that exact-byte preservation; cargo/tests/controls were NOT rerun here.
Fresh extractor check at c9f118b23e exited 0 with Would update 0 manifests, 0 added, 0 orphaned;
fresh tomllib census exited 0: 3P/7D moved, WOO158/249/125, assigned partition3/10/37.
This following evidence-only commit changes no gate inputs. Final native test floor is 26 tests across four selected targets.
