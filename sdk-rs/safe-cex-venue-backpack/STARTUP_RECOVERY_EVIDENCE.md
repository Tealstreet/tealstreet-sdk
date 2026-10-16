# Backpack startup and recovery

Actual parent: `08e5cc0cce0cd909aedfa8d2dcde1a9ce448b2f5` (integration).
Branch: `feat/rust-backpack-startup-recovery`.
No earlier unreviewed lane work was inherited.

Corrected code SHA measured below: `188b8eba018198c6e4a07971a00e5ab30f0da91f`.
First implementation SHA: `1dbf2f76630baf5748fc72a2788205a88858b4be`.
Evidence-only commits do not acquire new gate exits.

## Census and scope

Python 3.11 tomllib compared each symbol with the actual parent over the
Backpack manifest directory: **9 pending -> ported, 0 -> divergent**.
Base 65 ported / 56 divergent / 26 pending; code 74 / 56 / 17, total 147.

Rows: BackpackExchange constructor, start, tick, subscribeToAllMarkets;
BackpackPublicWebSocket constructor, recoverOrderBook, terminateOrderBook;
BackpackSigningParams and generateWsAuthSignature.

The full exchange/socket classes, HTTP factory, public listeners and remaining
REST bodies are still pending. These rows port native bodies over configured
compatible super/factory/read/state/log/scheduling/signing operations, with
UTF-16 strings, native f64 values and opaque reference/error identity. Arbitrary
host prototypes and a full transport adapter are not claimed. The constructor
bridges retain the original construction context; public state initialization
includes the ticker factory with 1000ms/leading/trailing true. WebSocket auth
composition preserves subscribe/empty object, eager clock/signing factory,
5000 default window and four native ordered fields. Its crypto provider is
supplied; real Noble/dalek crypto is not claimed by this test target.

## Gates at the corrected code SHA

One Cargo process at a time:

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --all-targets --jobs 2`: **0**, finished dev profile, zero errors.
- `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --test startup_recovery --jobs 2`: **0**, 8 passed, 0 failed.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: **0**, inspected text says 7,341 symbols across 512 source files; would update 0 manifests, 0 added, 0 orphaned.

The extractor WRITER ran after manifest changes and reported zero updates,
additions and orphans. No rust_files arrays were hand-authored.

## Corpus and native assertions

Actual TypeScript exchange/public socket/signing bodies, source enum tables and
LogSeverity are transpiled/executed. lodash sumBy is the actual dependency.
Base constructors, XHR/socket factories, timers, microtask/loop scheduling,
store and deferred readers are compatible supplied hosts. The Ed25519 stub
returns fixed bytes and records the actual signing payload/decoded seed.

Independent fixed cardinalities: constructors 9; start 2 x 16 = 32; tick
3 numeric samples x 19 = 57; subscriptions 12; recovery 15; termination 10;
WebSocket auth 6 numeric inputs x 4 = 24. Total **159 runtime cases**.
Additionally the actual signing type declaration's two alternatives and native
object/batch variants, record/row/outer-array/value aliases and UTF-16 keys are
asserted. All group mode/kind/iteration order and signing scalar/key/secret
inputs are pinned independently of the oracle output.

Decoded native fields, enums, float bits, loaded flags, same fetched object,
balance mutation, source operation order, eager calls and original errors are
asserted. Live market iteration and live terminal Set additions/deletions are
included. Native auth fields/arguments for every case precede the deferred
source payload/seed byte checks. There are no output roundtrips. Payload byte
expectations use independent literal number spellings; decoded seed expectation
is [7;32], not a value obtained from the output under test.

Enumeration floors: stopped at constructor iteration 8, start 31, tick 56,
subscription 11, recovery 14, termination 9, auth 23 and native type variant 1.
The bounded promise/future drivers allow 12 steps and require completion for
each fixture; the periodic callback was invoked once (tick case 0). These are
finite observations, not an exhaustive proof for all host behavior.

## Compiling discrimination controls

All controls began at the corrected code SHA. Each used the scoped test
command above plus its named function and `-- --exact`, ran one compiled test
and exited **101** on the named assertion; the file was restored byte-identically
and the same test exited **0**. Stopped at control iteration **11**.
No deliberate mutation survived this completed enumeration.

| Iteration | Compiling mutation | Test | Named assertion | Broken / restored |
|---|---|---|---|---|
| 0 | Rate limit10 -> 9 | constructors | CONSTRUCTOR rate native bits | 101 / 0 |
| 1 | Ticker factory1000 -> 999 | constructors | CONSTRUCTOR native ticker delay native bits | 101 / 0 |
| 2 | Defer initial markets factory until first poll | runtime_start | START native eager calls 0 | 101 / 0 |
| 3 | Balance upnl becomes sum+1 | runtime_tick | TICK balance patch upnl native bits | 101 / 0 |
| 4 | Capture subscriber after live market-id getter | subscriptions | SUBSCRIBE native invocation count | 101 / 0 |
| 5 | Exhaustion threshold3 -> 4 | recovery | RECOVERY eager attempts native bits | 101 / 0 |
| 6 | Terminal Set traversal freezes initial length | termination | TERMINATE native callback sequence | 101 / 0 |
| 7 | Swap native auth timestamp/window fields | ws_auth_composition | WS AUTH native field 2 | 101 / 0 |
| 8 | Object params wrapper copies into a new record | signing_params | PARAMS native object identity | 101 / 0 |
| 9 | Delete final termination corpus case | termination | TERMINATE independent corpus size (9 versus10) | 101 / 0 |
| 10 | Fill start corpus with its first case, keeping length32 | runtime_start | START independent corpus mode, iteration1 | 101 / 0 |
| 11 | Captured signer seed changes under unchanged signature stub | ws_auth_composition | WS AUTH independent actual-source decoded seed | 101 / 0 |

Restored SHA-256:
startup_recovery.rs `7c6f23f9521e0475d7b9a396d31e6ad1ac427d190e8e4c53518f2e763431cc08`;
signing.rs `1f6e7815c90d9954c2976cb5e113fc96adcd87405f232682df776124f3a9e88e`;
startup_recovery_oracle.cjs `52d5fd810040119fc1059f90fa5006667fe99e6c09fe4005b7b7740dc471a25a`.
Raw outputs/corpus/proof runner remain in /tmp, outside Git.

## Findings and review

Read-only sub-agent review closed a real eager tick RefCell panic: the logging
borrow must end before synchronous-account-error recovery schedules the loop.
The first uncommitted development target exited101 (6 passed/2 failed) on that
panic and a test-host guard which conflated a disposed boolean with a throwing
getter. Both were corrected, then eight development tests passed with exit0.

**FINDING:** the fixed-signature stub masked a Rust secret fixture differing
from the actual-source secret at tests/startup_recovery.rs::ws_auth_composition.
Review identified unasserted oracle payload/seed captures. New independent
source key/secret/seed/payload pins detected the bad fixture: the first code SHA
had scoped check0 and extractor0, but its test target exited **101** (7 passed,
1 failed) on `WS AUTH independent secret input`. The corrected secret derives
its exact Base64 spelling independently from the declared [7;32] input.
Only the corrected SHA's exits support this delivery. Control11 then proved
that the new decoded-seed assertion detects changed capture content even when
the stubbed signature remains identical.

Three initial standalone Node development oracle runs exited1 due to incomplete
constructor receiver, LogSeverity dependency and replacement-error fixtures;
corrected actual-source oracle exited0. These development observations are not
SHA-bound mutation controls.

Not run: workspace compilation, other venue or prior Backpack test targets,
host/wasm clippy, wasm build, full monorepo tests, live exchange/transport tests,
real crypto tests or CI. No shared type fields or shared runtime traits changed.

## Integration preservation before push

Fetched and merged integration `91bdde133d2f407dffa8f2140f94e87eac7c1d5f`;
merge SHA `274c86a9f3985d1cc8df89aba62ce66bd622cad1`.
Separately measured diffs against the corrected code SHA:

- Entire Backpack crate src/tests: **0**, byte-unchanged.
- Actual oracle inputs (exchange/public socket/signing/types/venue tables, LogSeverity and safe-cex package.json): **0**, byte-unchanged.
- Workspace/venue Cargo manifests, internal dependency source/manifests and Cargo.lock: **1**, only the OKX package's dependency references changed in Cargo.lock; no Backpack code or dependency source/manifests changed.

A Python3.11 tomllib traversal compared the old and merged lock records reachable
from the independently named safe-cex-venue-backpack root and its seven direct
package names. The first traversal assertion expected unqualified dependency
strings and failed; corrected to resolved package names. Corrected comparison
exited **0**, stopped after **481 reachable records** in each snapshot: every
record is equal, and OKX is outside that closure. This is dependency-preservation
evidence, not compilation of OKX or the workspace.

The relevant source/test/build inputs are unchanged; Cargo and mutation exits
remain attached to `188b8eba018198c6e4a07971a00e5ab30f0da91f`, not relabeled as
runs at the merge SHA. No Cargo rerun was needed. Other manifests changed, so
extractor check was run at the merge SHA: **0**, inspected text says 7,341 symbols
across512 files, would update0 manifests,0 added,0 orphaned.

Repeated tomllib census against both actual parent and merged integration:
**9 pending -> ported,0 -> divergent**; resulting **74 /56 /17**, total147.
The subsequent commit changes only this evidence file.
