# Backpack private updates and fetch scheduling

Actual parent: `18c971aca05c65d1bf3d7526121b2a1d6b612df1` (integration).
Branch: `feat/rust-backpack-private-events`. No earlier unreviewed lane work was inherited.

Code measured below: `f451e0a4b42f2ac0c38857e472ab557da4d6fe2a`.
First implementation: `1398b29f49162f73758032cdd336f5bde04ef74c`.
Later commits containing only this evidence do not acquire new gate exits.

## Rows and scope

Python 3.11 `tomllib` census over `parity/manifest/exchanges/backpack/*.toml`,
compared per symbol with the actual parent: **5 pending -> ported, 3 pending ->
divergent**, eight rows total. Base 60 ported / 53 divergent / 34 pending;
code head 65 ported / 56 divergent / 26 pending, total 147.

Ported: constructor state initialization, handleOrderUpdate,
handlePositionUpdate, throttledFetchPositions, throttledFetchBalance.
Divergent: executePositionFetch, executeBalanceFetch, fetchThrottledOrders.
Each divergent row has its own **CHOSEN** note: return original reconciliation
failure through native Result rather than source silent catch. Successful
updates, eager state changes, disposal guards and fetched-value identity follow
the source. Orders deliberately has no post-await disposal guard.

The full socket class remains pending. These bodies use native UTF-16 strings,
f64 values, captured host methods, state references and compatible supplied
operations. Arbitrary host prototypes and overridden mapping methods are not
claimed. Position arrays preserve captured length, live sparse slots and kept
record identity; event/market references remain live across the mode await.

## Measured gates

All three exits below were measured at the code SHA above, with one Cargo
process at a time:

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --all-targets --jobs 2`: **0**, finished dev profile; zero errors.
- `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --test private_events --jobs 2`: **0**, 5 passed, 0 failed; 264 source cases.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: **0**; output says 7,341 symbols / 512 source files, would update 0 manifests, 0 added, 0 orphaned. Output text was inspected.

The extractor WRITER was run after manifest changes; its output reported zero
updates/additions/orphans. No rust_files arrays were hand-authored.
At the first implementation SHA, scoped check, five-test target and extractor
also exited 0 (251 cases before the later capture fixtures). These older exits
are not evidence for the corrected code SHA.

## Independent corpus and native assertions

The oracle transpiles and executes actual backpack.ws-private.ts, types.ts and
createPositionId.ts. Base socket, store, clock, timer and deferred promises are
compatible supplied hosts. Rust asserts decoded native fields, enum values,
IEEE-754 bits, UTF-16 units, captured callee/reference identity, retained
position records, eager state and operation order. Output roundtrips are not
used. Source input/cardinality pins precede use of each case; there are no wire
output assertions.

Fixed Rust expectations: constructor 2; orders 12 inputs x 10 modes = 120;
positions 12 x 8 = 96; throttles 2 kinds x 12 = 24; fetches 3 x 7 plus one
orders-capture case = 22. Input fields and mode/kind/iteration order are pinned
independently of the oracle output. These are finite floors: stopped at
constructor iteration 1, order 119, position 95, throttle 23, fetch 21. They
are not a completeness proof for every possible host or numeric input.

## Compiling mutation proofs

Each control started from the corrected code SHA. Its specific test was run
with the test command above plus the named function and `-- --exact`. Each
broken run compiled, ran one test and exited **101** on the named assertion;
then the file was restored byte-identically and that test exited **0**.
Stopped after control iteration **9**; no controls survived this enumeration.

| Iteration | Compiling mutation | Test | Named failure | Broken / restored |
|---|---|---|---|---|
| 0 | Initial last-fetch timestamp becomes -0 | private_initialization | CONSTRUCTOR native positive-zero timestamp | 101 / 0 |
| 1 | Swap native order amount and filled | private_order_updates | ORDER amount native number | 101 / 0 |
| 2 | Zero-position filter preloads slots into a snapshot | private_position_updates | POSITION native filtered records 21 | 101 / 0 |
| 3 | Change inclusive throttle edge >= to > | private_fetch_throttles | THROTTLE native eager calls 1 | 101 / 0 |
| 4 | Swallow awaited position/balance rejection | private_fetch_execution | FETCH chosen visible failure 3 | 101 / 0 |
| 5 | Resolve order-update callee after loaded getter | private_fetch_execution | FETCH native captured orders update identity | 101 / 0 |
| 6 | Resolve trigger classifier after symbol getter | private_order_updates | ORDER native captured classifier identity | 101 / 0 |
| 7 | Resolve position-update callee after filter | private_position_updates | POSITION native captured update identity | 101 / 0 |
| 8 | Delete final order corpus case | private_order_updates | ORDER independent corpus size (119 versus 120) | 101 / 0 |
| 9 | Fill position corpus with its first case, keeping length 96 | private_position_updates | POSITION independent corpus mode, iteration 1 | 101 / 0 |

Restored SHA-256: private_events.rs
`c1cf37e94174526e469afc7bca1ce35bcc656f840118f5044e3a52a269f8d0ae`;
private_events_oracle.cjs
`cc3df352a97294f87d2cb58a613d311257e71ea3881142eb6764096f2186404e`.
Post-proof scoped git diff against the corrected SHA exited 0 for all crate
src/tests. Raw logs, oracle JSON and proof runner stayed outside Git in /tmp.

## Review and development findings

Read-only sub-agent review reproduced source witnesses (Node exit 0) for
trigger-callee capture, live filter slots and store-update capture. All were
fixed. A review found the first store-capture fixture conditioned its mutation
on prior capture, making a late capture potentially pass; the corrected fixture
mutates independently on the second ID read and logs reads. Control 7 above
actually failed. No survivor was measured before that instrument correction.
Constructor +0 now has bit assertions and source bit observations.

Before either code commit, a development test exited 101 because JSON float
state values differed structurally from JSON integer oracle values; corrected
to decoded native f64/bool assertions, then five tests passed with exit 0.
System python3 lacked tomllib; the census ran with /opt/homebrew/bin/python3.11.
These are development observations, not SHA-bound mutation proofs.

Not run: workspace compilation, other venue suites, prior Backpack test targets,
host/wasm clippy, wasm build, full monorepo tests, live exchange/transport tests
or CI. No shared type fields or shared runtime traits were changed.

## Integration preservation before push

Fetched and merged integration `3a957718fe445a833bb10a464e16e7cc6c585276`;
merge commit `73b2b5c9272beb53c2c52c7ef5bd632d1bddbcb0`.
At that merge SHA the following separately measured diffs against the corrected
code SHA exited **0**:

- Entire Backpack crate src/tests.
- Workspace Cargo.toml/Cargo.lock, venue Cargo.toml, and src/Cargo.toml of all internal build dependencies: num, types, crypto, exchange, store, transport and candle-cache.
- Actual TypeScript oracle inputs: types.ts, createPositionId.ts, backpack.ws-private.ts.

The code/test/build/source inputs are byte-unchanged. Cargo and mutation exits
remain attached to `f451e0a4b42f2ac0c38857e472ab557da4d6fe2a`, not relabeled
as runs at the merge SHA. Other venue manifests changed, so extractor check
was run at the merge SHA: **0**, inspected text says 7,341 symbols / 512 files,
would update 0 manifests, 0 added, 0 orphaned. No Cargo rerun was needed.

Repeated tomllib census at merge HEAD and against both actual parent and
merged integration: each comparison moves **5 pending -> ported, 3 ->
divergent**; resulting Backpack census **65 / 56 / 26**, total 147.
This subsequent commit changes only the evidence file.
