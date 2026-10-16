# MEXC order workflow gate exits

Built on integration `2ba1109d294708d532774387792ec3a216df8c09`.
Branch: `feat/rust-mexc-order-workflow`.
Implementation/gate SHA: `6a23e87c94c46242df29e116d153dd0b466958cb`.
No unreviewed feature branch was used as an ancestor. The retired, unpushed
private-session attempt is not the parent of this slice.

Independent tomllib census against the actual base: **10 pending -> ported,
0 -> divergent**. MEXC goes from 126 ported / 40 divergent / 19 pending to
136 / 40 / 9, with the same 185 symbol rows across six source manifests.
The symbol-by-symbol difference is the ten method names below, not a global
progress estimate.

## Scope and bounds

`order_workflow.rs` ports source bodies over crate-local `OrderWorkflowHost`,
which extends the existing task/value owner. Imported helpers, native array
builtins, promise reactions, clocks and timer primitives are bound operations;
no host method evaluates a whole selected TypeScript method body. Concrete
MexcExchange construction/start and full adapter composition remain pending.
No shared type or shared trait changed.

The oracle extracts the actual ten class members from mexc.exchange.ts via
the TypeScript AST, unchanged. It loads actual imported utilities and source
enums/constants. It constructs selected method owners, not the full exchange.
Rust asserts native returned values, optimistic order fields, same receiver and
input identities, original thrown object identity, error annotations, timers,
capability state and ordered effects. No Rust wire assertion or roundtrip is
used. Test JSON carries source expectations; native results are never encoded
for comparison. Counts are independent literal expectations in both runtimes.

The **76 cases** are an enumeration floor; each group stopped after the listed
case in oracle order. This is not exhaustive value, proxy, getter or thenable
coverage.

| Source method | Rust function | Cases | Stopped on |
|---|---|---:|---|
| formatRegularOrderPayload | format_regular_order_payload | 10 | format-error |
| waitForRegularOrderSubmitSlot | wait_for_regular_order_submit_slot | 5 | string-last |
| enqueueRegularOrderSubmit | enqueue_regular_order_submit | 7 | double |
| addOptimisticRegularOrder | add_optimistic_regular_order | 8 | add-error |
| addOptimisticStopOrder | add_optimistic_stop_order | 6 | note-error |
| scheduleStopOrderSnapshotRefresh | schedule_stop_order_snapshot_refresh | 7 | logger-getter |
| placeOrder | place_order | 10 | emit-error |
| placeOrders | place_orders | 8 | sequential-error |
| placeOrdersSequentially | place_orders_sequentially | 7 | string-error |
| placeRegularOrdersBatch | place_regular_orders_batch | 8 | empty-callback |

Simultaneous queue submissions, live sequential array append, primitive and
partial failures, actual stop/trailing enum values, replaced logger during an
error getter, and a custom forEach invoking the callback without arguments are
explicit cases. Missing callback arguments become native undefined.

## Measured gates

All following runtime/check exits are attached to the immutable gate SHA above.
Source was restored byte-for-byte before the passing restored runs; a scoped
git diff of src/tests against that SHA independently exited 0.

| Gate | Exit and observed result |
|---|---|
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-mexc --all-targets --jobs 2 | 0; Finished dev profile, zero errors |
| Node 24.20.0 scoped order_workflow test, restored | 0; 10 passed, 76 source cases |
| Node 26.7.0 scoped order_workflow test, restored | 0; 10 passed, 76 source cases |
| parity:extract writer before gate commit | 0; Updated 1 manifests, 0 added, 0 orphaned |
| parity:extract:check | 0; Would update 0 manifests, 0 added, 0 orphaned |

Cargo executable: `/opt/homebrew/opt/rustup/bin/cargo`. Scoped test command:

```sh
/opt/homebrew/opt/rustup/bin/cargo test --offline --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-mexc --test order_workflow --jobs 2
```

Node 26 run prefixes PATH with `/opt/homebrew/opt/node@26/bin`. Each Rust oracle
launch sets SAFE_CEX_TYPESCRIPT_PATH to
`/Users/sam/cs/tealstreet-next/node_modules/typescript`.

Extractor commands:

```sh
NODE_PATH="$PWD/sdk-rs/target/grvt-extract-deps/node_modules" node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract
NODE_PATH="$PWD/sdk-rs/target/grvt-extract-deps/node_modules" node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check
```

Extractor output text was inspected; no shim exit was used as sole evidence.
One cargo command ran at a time. No CI run was requested or repeated.

## Discrimination

Ten independent production mutations were applied together to the gate SHA,
each in a different root body. Root fixtures stub other root methods, so the
corresponding named failures identify the affected family. The batch compiled
and the scoped test target exited **101: 0 passed, 10 failed**. Restoring exact
source yielded **0: 10 passed** on both Node majors. No compiled control survived.

| Mutation | Named failing test/assertion |
|---|---|
| Payload volume + 1 | payload_native_source; formatRegularOrderPayload/live native number 7 vs 6 |
| Replace post-wait clock read with 0 | slot_native_source; waitForRegularOrderSubmitSlot/live ordered effects, 2 vs 3 |
| Invoke note hook instead of wait-slot hook | queue_native_source; enqueueRegularOrderSubmit/live native string note vs slot |
| Regular optimistic filled 0 -> 1 | optimistic_regular_native_source; addOptimisticRegularOrder/live native number 1 vs 0 |
| Stop reduceOnly true -> false | optimistic_stop_native_source; addOptimisticStopOrder/live native bool false vs true |
| Refresh delay 1500 -> 450 | refresh_native_source; scheduleStopOrderSnapshotRefresh/live native number 450 vs 1500 |
| Add an extra plan result ID | single_native_source; placeOrder/plan native array shape 2 vs 1 |
| Successful batch capability true -> false | routing_native_source; placeOrders/live native bool false vs true |
| Partial failedOrderIndex + 1 | sequential_native_source; placeOrdersSequentially/second-error native number 2 vs 1 |
| Batch stpMode 0 -> 1 | batch_native_source; placeRegularOrdersBatch/live native number 1 vs 0 |

The first attempted batch exited 101 before tests because two substitutions
left `vol` and `id` unused under deny(warnings). That attempt is **not proof**.
The corrected controls retain both bindings and reached the assertions above.

Development runs: 977bd1b9cb had unused fixture binding/variant compile failures
(exit 101); 62edd567e6 compiled, then four fixture failures (exit 101: call trace
argument shapes and a retained polling RefCell borrow). Corrected fixtures
passed at 500760cce7 (73 cases; superseded) and 8104ba46dd (76 cases, exit 0).
Only the full gate SHA and restored runs are the release evidence here.

## Review and non-claims

Sub-agent reviews `review_order_helpers` and `review_snapshots` reviewed source
bodies and source/native fixtures. Enum literals, logger capture ordering,
missing callback arguments and sequential input identity were corrected. Both
reported no remaining blocker in their assigned scope; neither ran gates.

Not run: workspace compilation, full crate/monorepo suites, host clippy,
wasm check/clippy/tests, CI, live exchange requests, or full concrete exchange
assembly tests. No claim about those exits or venue-wide behavioral completeness.

## Integration merge and refreshed gates

Merged integration `a58e1e79223f01bd7abd93053e26f3294e120d01` down cleanly.
Merged gate SHA: `26eb419368efbc109d62e6aa1cea4834f045c9e2`.
The required scoped src/tests diff from the implementation SHA exited **0**.

A broader input comparison covered packages/, package.json/yarn.lock, workspace
Cargo.toml/Cargo.lock, and src/Cargo.toml/build.rs for the eight local dependency
crates (MEXC, types, num, store, exchange, transport, crypto, candle-cache).
It found exactly three changed source files: crypto/src/lib.rs,
crypto/src/poseidon_example.rs, crypto/src/poseidon_loaders.rs. It exhausted
that declared local dependency closure; it is not a workspace build claim.
Workspace/venue manifests, Cargo.lock, TypeScript sources and all other listed
local input files were unchanged.

Because those compiled dependency inputs changed, these gates were rerun at
**26eb419368efbc109d62e6aa1cea4834f045c9e2**, using the commands above:

- Scoped MEXC all-targets cargo check: **0**, Finished dev profile, zero errors.
- Node 24.20.0 order_workflow target: **0**, 10 passed / 76 source cases.
- Node 26.7.0 order_workflow target: **0**, 10 passed / 76 source cases.
- Extractor check after integration parity-spec/exemption changes: **0**,
  Would update 0 manifests, 0 added, 0 orphaned.

The compiled mutation exits remain measurements at the original implementation
SHA; no mutation run at the merged SHA is claimed. The two sub-agent reviews
cover the unchanged MEXC source/tests. The subsequent documentation commit
changes this gate sheet only, preserving these measured inputs.
