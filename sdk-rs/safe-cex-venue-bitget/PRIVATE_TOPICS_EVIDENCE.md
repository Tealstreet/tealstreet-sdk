# Bitget private-stream state handlers

Fresh branch `feat/rust-bitget-private-topics`; actual direct integration parent
`9d7e0523cd16502a66901c3bf264e222bc5081ba`. No separate unreviewed lane branch
was used as an ancestor. Code/gate/review SHA:
`16b72aeea08c8959f07e91ee212cdc70d8d815ec`.

## Census and mapped bodies

Python3.11 **tomllib** compared `(manifest, kind, ts)` identities against that
actual parent: **8 pending -> ported, 0 -> divergent**. Eleven manifests, 227
identical identities; **71 ported / 80 divergent / 76 pending** before;
**79 / 80 / 68** after. Manifest enumeration stopped at the eleventh sorted file,
`bitgetusdc.exchange.ts.toml`. This census does not prove exhaustive behavior.

Eight actual targets in `src/private_topics.rs`:

| TypeScript field | Rust body |
| --- | --- |
| BitgetPrivateWebsocket#markWsPositions | mark_ws_positions |
| BitgetPrivateWebsocket.handleOrderTopic | handle_order_topic |
| BitgetPrivateWebsocket.handleUtaOrderTopic | handle_uta_order_topic |
| BitgetPrivateWebsocket.handleAlgoOrdersTopic | handle_algo_orders_topic |
| BitgetPrivateWebsocket.handlePositionsTopic | handle_positions_topic |
| BitgetPrivateWebsocket.handleUtaPositionsTopic | handle_uta_positions_topic |
| BitgetPrivateWebsocket.handleAccountTopic | handle_account_topic |
| BitgetPrivateWebsocket.handleUtaAccountTopic | handle_uta_account_topic |

These synchronous bodies retain normal/algo side differences, explicit versus
ambiguous fill policies, parent-only versus all-order updates, raw fill-price
type and parsed quantities, empty/full versus incremental position snapshots,
finite account fallback order, balance patches, original aliases, pending flags
and reconciliation order. Guarded action/data reads are repeated at their source
sites. Array forEach captures length and reads live elements; for-of uses supplied
live iterators. Snapshot Set assignment precedes timestamp Map updates.

Opaque Value clones preserve native reference identity, UTF-16 and nonfinite
numbers. The 34 exemptions are actual trait/helper/associated items, not invented
source rows. Calls and JS property/object/array/method/iterator/prototype operations
are supplied host contracts. Concrete private WebSocket assembly, arbitrary
proxy/overridden-operation behavior, iterator cleanup bindings and scheduling
are not established by these fixtures. Separately mapped callees retain their
own manifest contracts; this is not end-to-end native socket binding proof.

## Native assertions and actual-source floors

Five tests in `tests/private_topics.rs` run native assertions before their source
comparisons. Expectations independently prescribe event order, counts, types,
fields, number bits, UTF-16 units, flags, snapshot Set/Map values, balance totals,
fill values and original error Rc identity. Info, mapped-order and snapshot-array
aliases are asserted directly on native values. No decode/re-encode comparison
is used as a native gate; counts are expected constants/fixture recipes.

`private-topics-capture.mjs` extracts the eight actual source field initializers,
plus the original status/finite helpers and terminal-status Set. It transpiles
the real enum, ORDER_STATUS table and safe-math source. HTTP, store, emitter,
mappers, clocks and reconciliation callees are fixture collaborators.

The **73** actual-source cases are floors:

| Group | Independent floor | Stopped at |
| --- | --- | --- |
| Primary, eight bodies x six scenarios | 48 | index 47, uta-account/cancel |
| Original errors, first operation per body | 8 | index 7, uta-account first-operation failure |
| Guarded data getters | 4 | index 3, uta-account changed non-array second data read |
| Explicit/ambiguous statuses | 10 | index 9, algo/not_trigger |
| Live arrays / UTF-16 | 3 | index 2, mark lone-surrogate/NUL Set and Map keys |

The first-operation error floor is not all-operation failure coverage. The
getter group adds a throwing second data read. Live witnesses distinguish
forEach's initial length with replacement of its next element, for-of's appended
raw order, and native UTF-16 keys. Getter/alias/shape/status enumerations are
bounded; no completeness claim beyond the stated floors.

## Positive gates at 16b72aeea08c8959f07e91ee212cdc70d8d815ec

Cargo uses `/opt/homebrew/opt/rustup/bin` on PATH, `--jobs 2`, one process at a
time. Oracle tests set
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Gate | Measured exit / result |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --all-targets --jobs 2` | 0, Finished dev |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --jobs 2 -- --include-ignored` | 0, crate unit/integration/doc tests passed |
| `cargo test ... -p safe-cex-venue-bitget --test private_topics --jobs 2` after restoration | 0, five passed |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0; 7341 symbols, would update zero manifests, zero added/orphaned, output inspected |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract` | 0; 7341 symbols, zero updates/additions/orphans, output inspected |
| Rustfmt check of the two new Rust files; `git diff --check` | 0 |
| Python3.11 tomllib identity/status census | 0, exactly eight pending -> ported transitions |

The initial writer ran after manifest edits before the initial commit. It is not
claimed as an execution at the committed SHA. A second writer run at the code SHA
completed with exit 0 and zero updates/additions/orphans, as recorded above. The added live/UTF-16 test
initially had an extra delimiter and failed compilation (101); that was corrected
before this commit. That compile failure is not a discriminating mutation gate.

## Compiling controls at the same code SHA

Each control edited only the production body and ran scoped Cargo
`--test private_topics native_topics_then_actual_source -- --exact`. All six
compiled (Finished test), failed at the named **native** assertion with exit
**101**, and restored the original bytes. No measured survivor. The restored
full five-test file then exited **0**.

| Family / mutation | Named native failure, exit 101 |
| --- | --- |
| Snapshot clock: write now + 1 | `TOPIC native numeric mark/normal/1:set:lastWsPositionSnapshotAt.0` |
| Normal orders: disable partial-fill recognition | `TOPIC native independent event count order/partial` |
| UTA wrapper: replace nullish base volume fallback with falsy fallback | `TOPIC native scalar uta-order/partial/0:handleOrderTopic.0.data.0.baseVolume` |
| Algo fill: parse actualSize as price | `TOPIC native numeric algo/normal/6:emitter.emit.1.price` |
| UTA positions: select Classic mapper | `TOPIC native operation order uta-positions/normal/1` |
| Account total: subtract available instead of upnl | `TOPIC native numeric account/normal/2:store.update.0.balance.total` |

## Sub-agent review citation

`/root/account_read_review` reviewed exact committed blobs at
**16b72aeea08c8959f07e91ee212cdc70d8d815ec** and verified direct parent
**9d7e0523cd16502a66901c3bf264e222bc5081ba**. No blocking body-parity or mapping
findings in the eight rows within the declared supplied-operation scope.

Reviewer verified guarded rereads, outer-spread-before-row-map order, normal/algo
side differences, explicit/ambiguous fills, eager fallback evaluation, first
account selection, snapshots and native aliases. Reviewer measured committed
blob reads, Python3.11 TOML/transition checks and three decoded source-only
live/UTF-16 assertions: exits 0. Its initial source eval invocation exited 1
because mode argv was absent at process.argv[2]; corrected invocation exited 0.
The reviewer verified four TS dependency files byte-identical to the code SHA.

Reviewer did not run Cargo/native Rust tests, extractor, compiling controls,
clippy/wasm, CI or workspace checks. Review plus census is not exhaustive proof
of error paths or language/socket binding.

## Delivery preservation

Integration `77746a5b498ce3d3532424345f072425181f33ca` was merged down as
`98277376e1f366cc6df95ead9a688f8f8de18968`. Only the exemption append conflicted:
the exact integration text plus all our 34 records was retained. Python3.11
verified all integration and lane records survive, TOML parses and rs keys are
unique; exit 0. No CLAUDE conflict occurred.

At merge SHA **98277376e1f366cc6df95ead9a688f8f8de18968**, scoped
`git diff --exit-code 16b72aeea08c8959f07e91ee212cdc70d8d815ec HEAD --
sdk-rs/safe-cex-venue-bitget/src sdk-rs/safe-cex-venue-bitget/tests
sdk-rs/safe-cex-venue-bitget/scripts` exited **0**. A separate comparison of
Cargo.lock, the direct SDK dependency crates, Bitget/types/math TS sources,
workspace/crate Cargo manifests, cargo configs and toolchain paths also exited
**0**. Tested source/tests/scripts and dependency/build inputs are byte-unchanged.
Cargo and compiling controls were not rerun after this merge; their actual
measured SHA remains **16b72aeea08c8959f07e91ee212cdc70d8d815ec**.

Extractor check at the merge SHA exited **0**: 7341 symbols, would update zero
manifests, zero additions/orphans; output inspected. Post-merge Python3.11 tomllib
census against integration 77746 exited **0**, eleven manifests, 227 identical
identities, exactly **8 pending -> ported, 0 -> divergent** and **79/80/68**.
The following delivery commit changes only this evidence file.

Final fetch/merge: integration **3a957718fe445a833bb10a464e16e7cc6c585276**
merged as **10251f4a8a91d82806b1083a4f8e0662e4ffb97a**. The exemption append
was again resolved as exact integration text plus our 34 helper records;
Python3.11 parse/preservation/unique-rs assertions exited **0**. Scoped own
source/tests/scripts and dependency/build/TS-input comparisons against gate SHA
16b72 exited **0**. No changed gate inputs; Cargo/controls were not rerun and
remain attached to their actual measured 16b72 SHA.

Final extractor check at **10251f4a8a91d82806b1083a4f8e0662e4ffb97a** exited
**0**, 7341 symbols, would update zero manifests, zero added/orphaned; output
read. Final Python3.11 tomllib census at the same SHA against integration 3a957
exited **0**: eleven manifests, 227 matching identities, exactly **8 pending ->
ported, 0 -> divergent**, **79 ported / 80 divergent / 68 pending**. The final
delivery commit changes this evidence file only; it does not claim new gate runs.

Not run: workspace/monorepo builds or suites, host clippy, wasm clippy/check,
CI reruns, live/network tests, concrete native JS/WebSocket binding verification,
exhaustive getter/proxy/prototype/iterator/error/scheduling enumeration. No PR
was created. Production mutation bytes were restored before delivery.
