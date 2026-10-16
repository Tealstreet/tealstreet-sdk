# Orderly utility/REST rescue gates

Measured source/test commit: `b619891eb085a5200b4c722a79639d0ac40d676c`.
Measured manifest/target/control commit: `3557c7c2758037e12405ce1ab69af0b7c46287ef`.
The only change between these commits is the exact Rust target text in one manifest note;
`git diff --exit-code b619891eb0 3557c7c2758037e12405ce1ab69af0b7c46287ef -- sdk-rs/safe-cex-venue-orderly/src sdk-rs/safe-cex-venue-orderly/tests` exited 0.
This sheet is a later evidence-only commit. No gates are claimed as executed at that later SHA.

Rescued branch: `feat/orderly-utility-rest`, originally `113738148d53dbd5a937a8e185547fd95146aec5`.
Merge-downs: `45ff4e3dd2941ae338dd783a6177e94d00eccce0` at `fd0e9489a33133b0ef1c228dd2b19d95d671e898`,
then `d7233c152349b3076a7469c481f69289589ec702` at `9422c648c496595c2f2a635a66609e3c8ac0a615`.
Both merged cleanly with ort. The inherited dead-lane implementation had no trusted gate narrative;
this rescue established its own gates and sub-agent review. Those inherited commits are ancestors,
and remain subject to PR review.

## Completed gates

| Gate | Measured SHA | Exit and observed result |
| --- | --- | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-orderly --all-targets --jobs 2 --offline --locked | b619891eb085a5200b4c722a79639d0ac40d676c | 0; Finished dev profile |
| cargo test with the same scoped package/all-targets/jobs/offline/locked flags | b619891eb085a5200b4c722a79639d0ac40d676c | 0; eight integration targets passed, no failed/ignored tests |
| python3.14 sdk-rs/safe-cex-venue-orderly/tests/check_manifest.py | 3557c7c2758037e12405ce1ab69af0b7c46287ef | 0; production invention/status checker passed, 95 rows and 36 public Rust items, empty exemptions |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 3557c7c2758037e12405ce1ab69af0b7c46287ef | 0; Would update 0 manifests; 0 added, 0 orphaned |
| One compiled mode-branch mutation, focused position_side_parity test | 3557c7c2758037e12405ce1ab69af0b7c46287ef plus documented mutation | 101; POSITION-SIDE native/hedged/buy/undefined panicked |
| Exact bytes restored, focused position_side_parity test | 3557c7c2758037e12405ce1ab69af0b7c46287ef | 0; actual_typescript_position_side_corpus passed |

All venue test invocations used SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript.
Cargo invocations were serial. Extractor output text was inspected. The manifest writer ran after edits.
Mutation: utils.rs `position_mode == PositionMode::Hedged` changed to `PositionMode::OneWay`.
Stopped after control iteration 1; zero survivors in that floor. The inherited two-control script was not run.
The corpus floor is 12 static branch cases, two rejection identities, one pending/resumed getter,
and one explicit live-options/eager-invocation source witness. Native Option values and error Rc identity
are asserted directly; no native encode/re-encode comparison supplies the expected results.

## Census and scope correction

Python 3.14 tomllib census across all 13 Orderly manifests, including nested exchange manifests,
compared to d7233c1523: 95 rows; 13 ported/15 divergent/67 pending -> 13/16/66.
**One pending row moved to divergent; zero pending rows moved to ported.**
The older two utility rows already exist in integration and are not counted as new movement.
All 29 off-pending rows have non-empty real Rust targets accepted by the production checker.
This is a target-existence proof and census, not new behavioral proof for all 29 rows.

Review found that source accepts Pick<side,reduceOnly>, while the Rust helper requires full PlaceOrderOpts;
source observes mutable options after await and invokes the getter eagerly, while Rust retains an immutable
borrow and starts on poll. The row is now CHOSEN/divergent with those boundaries and an exact extraction target.
The source mutation witness yields LONG; the native immutable-options witness yields SHORT.
Configured getter and REST composition remain separately pending.

Earlier attempts are not passing gates: an initial check was terminated (-15) while files changed;
check at dddf042bd0aae1967b95c370d4f49430076fc0f1 exited 101 for a shadowed test helper (fixed);
target checker at b619891eb0 exited 1 because the revised note omitted the exact target (fixed).
No workspace build, workspace tests, full parity checker, host clippy, wasm clippy, wasm build or CI was run.
