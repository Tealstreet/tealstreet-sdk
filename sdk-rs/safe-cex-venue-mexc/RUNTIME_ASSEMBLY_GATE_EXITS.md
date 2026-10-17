# MEXC remaining-row gate exits (historical snapshot)

Built on integration `1ef197eb733830953e0ea1cffb40de13ee24269d`.
Branch: `feat/rust-mexc-runtime-assembly`.
Implementation/gate SHA: `967855bc4051cb8ee6c35c320f84d639b9edfd1f`.
Fresh branch from merged integration; no unreviewed feature branch was used as
an ancestor. No PR creation, integration merge-up, or history rewrite.

Independent tomllib comparison against that actual parent: **9 pending ->
ported, 0 -> divergent**. MEXC moves from 136 ported / 40 divergent / 9 pending
to **145 ported / 40 divergent / 0 pending**, still 185 rows across six files.
This is a per-symbol census, not a claim of complete venue behavioral proof.

## Source and test scope

The nine source rows target real functions in runtime_assembly.rs. AssemblyHost
is crate-local and extends the existing bound value/task owner; no shared type
or shared trait changed. Base/child construction, imported helpers, ordinary
property operations, array builtins and promise reactions are native bindings.
Member factories must bind existing mapped Rust bodies; production provides
no default member body and never evaluates a whole TypeScript source method.
Concrete adapter implementations of those owner bindings are not proven here.

Constructor initializes the source scalar/queue fields and five independent
empty maps, then 71 named member fields, then API/public/private children.
Native installed member names compare to actual source instance own callable
names, in source order; cardinality is an independent literal 71. The member
enumeration stops at orderListEndpoint. Initial map lengths expect literal zero,
and separate maps cannot alias. Original options/store/child-parent identities
are asserted independently of serialized expectations.

The oracle extracts the actual selected constructor/arrows via the TS AST,
unchanged. Existing source enums/constants/utilities are loaded. Other members
and native primitives are dependency fixtures. It constructs selected owners,
not a full concrete exchange adapter.

Rust asserts decoded native results, payload fields, state, ordered calls,
returned and input aliases, parsed-frame identity, and original thrown-object
identity before any wire assertion. No native result is re-encoded for a
comparison. Source expectation JSON is decoded into the comparison fixture;
JSON stringify/parse calls inside the source body remain bound primitives.

The **96 cases** are a floor, with independent literal counts in both runtimes.
Enumeration stopped after each case below; arbitrary getter/proxy/thenable
values, all native owners and venue-wide behavior are not claimed exhaustive.

| Source row | Rust target | Cases | Stopped on |
|---|---|---:|---|
| MexcExchange.constructor | new_exchange | 6 | private-error |
| MexcExchange.start | start_exchange | 12 | markets-error |
| MexcExchange.listenOHLCV | listen_ohlcv | 3 | leaf-error |
| MexcExchange.listenOrderBook | listen_order_book | 3 | leaf-error |
| MexcExchange#placePlanOrder | place_plan_order | 14 | refresh-error |
| MexcExchange#placePositionStopOrder | place_position_stop_order | 15 | nan-contracts |
| MexcExchange.updateOrder | update_order | 8 | mutate-order |
| MexcPublicWebsocket.listenOrderBook | public_listen_order_book | 10 | coercion-order |
| MexcPrivateWebsocket.onMessage | private_on_message | 25 | handler-error |

Fixtures include external/public startup, disposal barriers, original errors,
full-position and explicit-size validation, scalar/array/object response IDs,
inherited and present-undefined updates, pre-cancel option capture, prototype
snapshot setters, unsubscribe-before-snapshot source behavior, symbol coercion
changing the error message, all eight known private push channel spellings,
pong/login/error guards, control stats exclusion and a native large-integer ID.
The fetch case replaces findOpenPosition during fetchPositions; the captured
old callee still executes. Update returns the exact retained delegated ID array.

## Measured exits at the implementation/gate SHA

Cargo executable: `/opt/homebrew/opt/rustup/bin/cargo`. One cargo at a time.

```sh
/opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-mexc --all-targets --jobs 2
/opt/homebrew/opt/rustup/bin/cargo test --offline --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-mexc --test runtime_assembly --jobs 2
NODE_PATH="$PWD/sdk-rs/target/grvt-extract-deps/node_modules" node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check
```

- Scoped all-targets MEXC check: **0**, Finished dev profile, zero errors.
- Restored Node 24.20.0 target: **0**, 9 passed / 96 source cases.
- Restored Node 26.7.0 target: **0**, 9 passed / 96 source cases.
- Extractor check: **0**, Would update 0 manifests, 0 added, 0 orphaned.
- Exact restored src/tests diff against the gate SHA: **0**.

Node 26 prefixes PATH with `/opt/homebrew/opt/node@26/bin`. Oracle launches set
SAFE_CEX_TYPESCRIPT_PATH to
`/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Extractor output text was inspected, not inferred from a Volta shim exit.
Preparation writer used the same command with parity:extract before commit
61ade0715f: **0**, Updated 3 manifests, 0 added, 0 orphaned. The final immutable
extractor-check SHA above is the gate; the writer's working-tree preparation
is not represented as an immutable runtime gate measurement.

## Discrimination

Nine independent production mutations were applied together to the gate SHA,
one per new root family. Root fixtures stub other root methods. The batch
compiled and the scoped target exited **101: 0 passed, 9 failed**. Exact source
restore then exited **0: 9 passed** on both Node majors. No compiled control
survived; no compiler failure was counted as mutation proof.

| Mutation | Named failure |
|---|---|
| Registry start -> unknownMexcMethod, unchanged count 71 | constructor_native_source; constructor/live native string |
| Market loaded flag -> tickers | startup_native_source; start/live native key tickers |
| External candle callee -> subscribeOrderbook | candle_entry_native_source; listenOHLCV/external native string |
| External book callee -> subscribeCandle | book_entry_native_source; listenOrderBook/external native string |
| Plan executeCycle 2 -> 1 | plan_native_source; placePlanOrder/live native number |
| Move findOpenPosition callee lookup after fetch await | position_stop_native_source; placePositionStopOrder/fetch unexpected native error |
| Replacement payload amount key -> size | update_native_source; updateOrder/live native key size |
| Snapshot indexed assignment -> own definition | snapshot_listener_native_source; publicListenOrderBook/setter native string callback vs book-set |
| Pong threshold 15000 -> 15001 | private_message_native_source; privateOnMessage/pong native number |

Development: 89fe3457e9 compiled but exited 101 (5 passed / 4 failed). Source
wrapper imports lacked emitted CommonJS constant scope; copied native promise
fixtures failed to propagate rejection when then had no rejection callback;
a synchronous source pair-error hook was modeled as asynchronous rejection.
These were fixture findings at tests/runtime_assembly_oracle.cjs method wrapper
and tests/runtime_assembly.rs Runtime::field(P)/fetchBalanceAndPositions hooks,
not production divergences or mutation survivors. Corrected target at
94d547f9f5 passed all nine (96 cases), exit 0. After member/callee-name assertions,
61ade0715f check/test/extractor all exited 0. The final gate SHA adds the callee
replacement and delegated-result identity assertions listed above.

## Review and non-claims

Sub-agents review_order_helpers and review_snapshots reviewed source and native
fixtures at the gate SHA; both reported no remaining blocker in their assigned
scope. They ran no cargo/tests/mutations. Their identified <=/NaN guard,
ordinary snapshot assignment, symbol/error evaluation order, source-derived
member-name comparison, await callee capture and returned ID identity are
reflected in the source and assertions.

Not run: workspace compilation, full crate/monorepo suites, host clippy, wasm
check/clippy/tests, CI, live exchange requests, or full concrete adapter
construction/startup integration. The 40 existing divergent rows remain
unchanged; 0 pending is the census claim, not equivalence of every row.

## Integration merge preservation

Merged integration `eb5339fff8f489456695778fe2af9701f8c18b69` down cleanly,
yielding `edaf15c6635a980f5d5c675d0306613865ca8412`.
Required scoped src/tests diff from the implementation/gate SHA: **0**.

Broader source-input comparison covered MEXC Cargo.toml, workspace Cargo.toml,
packages/, package.json/yarn.lock, and src/Cargo.toml for the seven local
MEXC dependencies: types, num, store, exchange, crypto, transport, candle-cache.
All those files were unchanged. Only Cargo.lock differed: the Phemex package
added crypto and the Weex package added exchange. Those packages are outside
MEXC's conservative reachable lock closure. Comparing complete name/version/
source-keyed entries in the old and merged MEXC closures found **481 entries in
each, structurally identical**. This declared enumeration stopped at worklist
exhaustion and conservatively included all matching name/version candidates;
no feature pruning or workspace compilation is claimed.

Thus the implementation-SHA check, Node 24/26 native tests and mutation/restored
measurements are preserved by unchanged runtime inputs. They were not rerun at
the merged SHA and are not misrepresented as fresh merged-SHA measurements.

The global parity-spec/exemption inputs changed, so extractor check was measured
at **edaf15c6635a980f5d5c675d0306613865ca8412**: **0**, Would update 0 manifests,
0 added, 0 orphaned. An earlier post-merge extractor launch lost its monitor
session handle; its exit was not measured and it is not gate evidence. The
monitored repeat is the measured result above.

The following documentation-only commit preserves these inputs. Final tomllib
comparison to merged integration still moves exactly nine pending -> ported,
zero -> divergent, with MEXC 145 / 40 / 0.

## Dead socket-copy retirement

The publicListenOrderBook/privateOnMessage generic branches and their35 source
cases were removed after the unbound-name caller census. The old9-row/96-case
proof table above is historical; the maintained assembly target now has7 groups
and61 source cases. The two socket rows point to their live native owners and
are explicit coverage findings. Their fake-host controls above no longer run.

Retirement verification: retained production assembly prefix is byte-identical;
all seven retained live-source oracle groups (61 cases) match the original oracle
output exactly. Scoped runtime_assembly7, exchange_trait4 and socket_assembly7
pass. Actual registry HTTP/WebSocket private order/fill/position/balance4 pass.
Native/wasm strict scoped Clippy, extractor0updates, parity7459rows/254items,
all-features cancellation6pass/1explicitexportignore and fmt/diff checks pass.
