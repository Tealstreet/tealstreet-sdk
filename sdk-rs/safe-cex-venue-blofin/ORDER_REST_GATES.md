# Blofin eleven-row order REST slice

Code, gate and final static-review SHA: `dac09b909ff85bacd360997aef3856e408ee389b`.
Original integration parent: `c5842fe5e72e2d3eb2a734a5c2ad1c82b92bab6e`.
Integration merged before these gates: `82274b95ecbae81c5c8b20c4d3f65d60cf64a360`
via `208530a0a9612fc75ef0cfa2467fae6840b935ca`. No private lane branch was merged; inherited integration commits were not
re-reviewed here.

The seven-file Blofin tomllib census is14 ported /40 divergent /71 pending at the
original parent and14 /51 /60 at the code SHA. Eleven independently selected
pending methods moved to divergent, zero to ported. Enumeration stopped after
all seven Blofin manifests (125 rows); this is no whole-project census.
Each row has its real Rust target and a specific canonical CHOSEN note. Class,
constructor, API construction and websocket rows remain pending.

Implemented methods: updateOrder, cancelOrders, setLeverage, placeOrder,
placeOrders, changeMarginType, placeOrderBatch, placeAlgoOrderBatch,
cancelNormalOrders, cancelAlgoOrders and cancelTriggerOrders. The functions use
shared RestAdapter/Store and typed owned host capabilities. Tests isolate
formatter/mode/placement capabilities; full exchange wiring is not claimed.
Pending native futures drop on failure instead of continuing as independent JS
promises. Source object aliases, arbitrary executable fields, malformed updater
values and encoded non-JSON bodies follow the documented typed/ingress boundary.
Timers receive exact350/500/1000ms arguments; wall-clock timing is not proven.

## Measured gates at the code SHA

| Command scope | Exit | Evidence |
| --- | ---: | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-blofin --all-targets --jobs 2` |0| `order-rest-evidence/check.log` |
| `cargo clippy` with the same package/all-targets/jobs scope and `-- -D warnings` |101| `host-clippy-with-dependencies.log`; two inherited transport Arc warnings |
| Same host clippy, additionally `--no-deps` |0| `host-clippy-crate-only.log` |
| Scoped `cargo test`, `--test order_rest --test order_services --jobs 2 -- --nocapture` |0| `tests.log`;14 new tests plus10 existing service tests |
| `rustfmt --edition 2024 --check` on order_rest.rs and its Rust test |0| `fmt.log`; changed files only |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` |0| `extractor.log`; read output:7341 symbols/512files, would update0, added0, orphaned0 |
| Blofin `tests/coverage_scope.cjs` |0| `coverage.log`; source declarations match TOML, independent floors20/3/3/66, four files92 declarations |
| Existing Rust parity-check binary over Blofin only, empty exemptions |1| `rust-parity.log`; only3 inherited position_adapter public exports lack rows |

Each command/exit is recorded with the full SHA in JSON. Source and Cargo gates
ran independently; Cargo commands were sequential, jobs2. Node-oracle gates use
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
The Rust indexer binary was reused, not rebuilt this turn. No exemption was added,
no fake target was introduced and no parity gate was weakened.
The transport `tealstreet.rs` and venue `position_adapter.rs` sources have scoped
`git diff --exit-code`0 versus the original parent; see inherited-preservation.json.
This is a scoped compile/lint claim, not workspace compilation or a host+wasm
claim. Workspace suites, wasm lint and live venue/network gates were not run.

## Native evidence and controls

The actual-source oracle executes maintained AST initializer text, genuine enums,
constants/lexical constant pins, numeric helpers and real TypeScript DefaultStore.
Its independent floor is52 cases over11 method families, each exercised through
encoded/native/transformed shared REST bodies:156 native comparisons. Enumeration
stops at those52 inputs/three body variants; it does not claim exhaustive methods
or all malformed values. Four signed-zero witnesses compare source Object.is and
native cached buy/sell bits before request values. Additional witnesses cover
source-admitted/native-refused null updates, delayed side writes and immediate
transport rejection dispatch. Native callback options, provenance, IDs, store
positions/leverage and reconciled orders precede wire trace comparisons.

Original fixtures captured input after source Store mutation, masking the initial
position state. Inputs now freeze before source execution; a dedicated control
must catch regression to post-mutation capture. Fixed independent source anchors
cover ID ordering,20-item chunks, exact12-versus123 root reconciliation, leverage
clamp50 and unchanged leverage3 on rejected side updates.

All15 controls completed at the code SHA: each named assertion failed at101,
then exactly one restored test passed at0. Enumeration stopped after15/15,
with zero survivors and zero compiler failures counted as proof. One control
covers each newly mapped method, plus four controls for parallel rejection
dispatch, source input freeze, strict null cancellation and signed-zero erasure.
The independent method/control-name list and scoped git-diff exit0 prove final
source/test/manifest bytes were restored. See controls/results.json and30 raw
broken/restored logs; control-restoration.json records the independent floor.

## Review and earlier attempts

Read-only sub-agent `/root/ticker_review` reviewed exact final SHA
`dac09b909ff85bacd360997aef3856e408ee389b`: no remaining findings. The reviewer
ran no Cargo, source oracle or mutation gates. Earlier review found P2 signed-zero
clamp ties and null protective-cancel row access; shared extrema, strict field
access and new witnesses repair both. The scoped resolver also caught missing
exact rename notes and an incorrectly spelled UtilityInput.json target; corrected
target UtilityInput::json resolves. Source/native zero fixtures now capture
requests first, assert cached native values, then compare request values.

Earlier measured gates remain under pre-review/, bound to208530a0; their results
are not relabeled as covering final code. An a7c73e0 gate batch was stopped for the
fixture-order correction before measuring its check exit (runner exited143).
Its source gates/census completed; interrupted/ records exactly what ran and what
did not. No runtime proof is claimed for an interrupted or compilation-failing
attempt. Development fixture/schema/import/lifetime errors were corrected before
the final gate SHA. Final control findings, if any, must remain in the evidence.

Final merge preservation: integration `dc74077e6165f04841de6a6bbd9d73c55afc58db` merged via `5505b6bd642a2916b258c5dac555b3a7b6769132`. Scoped source/test/dependency/manifest/lock/extractor input diff versus the code/gate/review SHA exited0; see merge-preservation.json. Global extractor rechecked at the merge SHA exited0, output would update0/added0/orphaned0. No Cargo rerun was needed because its scoped inputs are unchanged; the recorded Cargo/controls/review SHA remains `dac09b909ff85bacd360997aef3856e408ee389b`. Later evidence commits alter only documentation.
