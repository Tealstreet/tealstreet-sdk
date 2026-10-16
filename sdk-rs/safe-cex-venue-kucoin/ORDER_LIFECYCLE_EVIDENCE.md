# KuCoin order lifecycle evidence

Source and test gate commit: `dc32e6d0c1f3de6405aeb58cb0c2ad1110ff2607`.
Actual fresh integration parent: `372972fbfaa126366d6309f4ba15fa84d07bc7a5`.
Branch: `feat/kucoin-private-order-lifecycle`. No unreviewed feature work is an ancestor of the fresh slice.

Six pending → ported, zero pending → divergent: KucoinExchange.placeOrder,
placeOrderBatch, placeAlgoOrders, updateVirtualTpSlOrder, and the futures/Spot
private websocket handleOrderTopic methods. Independent tomllib census of 12
KuCoin manifests stopped at kucoinspot.ws-public.ts.toml: 138 ported / 39
divergent / 14 pending → 144 / 39 / 8, 191 rows. The remaining eight rows are
six class declarations and two constructors; no full venue completion claim.

The native module contains six roots and nine shared helpers. All 15 functions
are mapped, including each root's actual transitive native helpers. One C3
OrderLifecycleOps carrier is grouped beside existing KuCoin supporting carriers.
The parity writer measured exit 0 and reported zero rewritten/added/orphaned rows.
Only the KuCoin venue command gains the focused order_lifecycle test target.

The test independently declares 50 finite cases, in family order place(5),
batch(10), algo(6), replace(9), futures(12), Spot(8). Enumeration stops at
Spot/emit-reject. Every case asserts decoded native types, values, identities,
independent counts, effects and complete named error metadata before any value
is packed or any source comparison runs. Original payload/order/error/response
Rc identities are pinned. Literal event sequences assert diagnostics, fill
before status/add and no later refresh after throwing callbacks; returned ID
counts and rebuilt fields are native assertions. The source oracle extracts
the actual six maintained TypeScript AST bodies, endpoints, OrderStatus and
normalizeMillis; it does not implement those venue branches independently.

Adapted functions require complete live callee/member/property/language,
constructor, iterator and await host contracts. The finite ordinary fixtures do
not prove all exotic getters/proxies/prototypes, suspended or pending async
scheduling, IteratorClose failure precedence, next() failure, inherited update
properties or concrete provider/class assembly. These are explicit proof gaps;
those behaviors are reflected in native boundaries and reviewed against source,
not claimed as exhaustively tested. Real network requests and credentials are
not used. No OKX files are part of this slice.

Read-only sub-agent production review found no blocker; fixture review requested
stronger diagnostics, ID counts, rebuilt inputs and original options identities,
which were implemented before the gate commit. No human review was requested.

Measured scoped exits and compiling mutation controls are appended after they
finish; no gate still running is described as passed. Workspace compilation,
full venue suite, repository-wide Rust target validation, host/wasm clippy, CI,
live venue behavior and concrete provider assembly were not run/proved.

At source SHA `dc32e6d0c1f3de6405aeb58cb0c2ad1110ff2607`, measured:

| Gate | Exit | Scope/result |
| --- | ---: | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --all-targets --jobs 2 | 0 | Scoped crate/all targets; zero errors, not workspace compilation |
| SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --test order_lifecycle --jobs 2 | 0 | One test, 50 native cases before actual-source oracle; stop Spot/emit-reject |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0 | Output inspected: would update 0 manifests; 0 added/orphaned symbols |

Gate logs/exit records are local, uncommitted `/tmp/kucoin-order-lifecycle-`
files. The evidence is this manually authored tracked receipt; raw generated
reports are not committed. Review of source, native assertions, AST oracle,
transitive mappings, carrier grouping and package append was clear at this SHA.

Six compiling production mutations at base SHA
`dc32e6d0c1f3de6405aeb58cb0c2ad1110ff2607` each failed the named native assertion
at exit 101, then the restored focused 50-case native/source test exited 0:

| Row/control | Deliberate production change | Native assertion | Broken/restored exits |
| --- | --- | --- | --- |
| placeOrder / place-attached | Select ordinary endpoint for attached orders | PLACE native attached route | 101 / 0 |
| placeOrderBatch / batch-default | Default to attached endpoint | BATCH native route | 101 / 0 |
| placeAlgoOrders / algo-route | Force ordinary endpoint for both payloads | BATCH native route | 101 / 0 |
| updateVirtualTpSlOrder / replace-stop | Swap rebuilt stopLoss and takeProfit values | REPLACE native rebuilt numeric field | 101 / 0 |
| futures handleOrderTopic / futures-increment | Use cumulative filledSize instead of incremental matchSize | HANDLER native base fill | 101 / 0 |
| Spot handleOrderTopic / spot-event | Branch on status instead of event type | LIFECYCLE native independent effect sequence | 101 / 0 |

Control enumeration stops at spot-event, six literal expected names. No survivor
among these six compiling controls; no general mutation-completeness claim.
The initial replace-stop candidate substituted tp.clone() only for stopLoss;
at update_virtual_tp_sl_order's formatter call it left sl assignments unused.
Rust warnings-as-errors rejected compilation at exit 101, so this attempt is
not counted as native discrimination. Exact restoration passed at exit 0, then
the compiling two-field swap above was measured separately. Both attempt logs
and exits are retained in `/tmp/kucoin-order-lifecycle-controls.json` and the
resumed records/logs under `/tmp/kucoin-order-lifecycle-controls-resumed.json`.
After all controls, git diff --exit-code against the source SHA for the entire
KuCoin src/tests and manifest directory measured 0: no mutation left behind.

Integration `f364cee096ff92584c157cad8b96f12b92cf3a09` was merged without conflicts
at `c5b3942f517730957e0fdea626f5d0eabf4639d0`. Scoped source-SHA→merge-SHA
`git diff --exit-code` measured 0 separately for all KuCoin src/tests/Cargo.toml
and manifests, workspace Cargo.toml/Cargo.lock plus the six direct SDK crates,
and KuCoin TypeScript/types/utils/yarn.lock. The package command equals incoming
integration plus exactly the focused target append; incoming integration is the
sole merge base. Sub-agent independently confirmed exemption-union preservation.
Cargo check/test exits remain source-SHA measurements carried by these byte
checks; they were not rerun or relabeled as fresh merge-SHA measurements.

Fresh parity:extract:check at merge SHA
`c5b3942f517730957e0fdea626f5d0eabf4639d0` measured exit 0; output inspected:
would update 0 manifests, 0 added/orphaned symbols. Fresh independent tomllib
census against the incoming integration still measured six pending→ported,
zero divergent, 144/39/8, over the same 12-manifest floor. Diff whitespace check
against incoming integration measured 0. Read-only evidence review was clear.

Pre-push integration snapshot `f17498f6e1e8b8e8b93c75bc11f2535b2b4bf8eb` was
merged without conflicts at `f919e42d91078a55cf66094c38071c9aef1d3768`.
All three scoped owned/build/TypeScript diff checks against the source gate SHA
again measured 0; the expected package append and sole incoming merge base
were verified. The complete incoming exemption contents/order plus precisely
the reviewed local C3 were independently preserved (tomllib equality).
Source cargo/test exits and six compiling control exits therefore retain their
original measured-SHA attribution. No cargo rerun was needed or claimed.

Fresh parity:extract:check at pre-push merge SHA
`f919e42d91078a55cf66094c38071c9aef1d3768` measured exit 0; output text:
would update 0 manifests, 0 added/orphaned symbols. Independent census against
this incoming snapshot again measured 6 pending→ported, 0 divergent,
138/39/14 → 144/39/8 over 12 files, stop kucoinspot.ws-public.ts.toml.
Whitespace check exited 0. The branch diff against incoming integration has
exactly 10 files: one focused package command, one C3 exemption, three KuCoin
manifests, this receipt, two KuCoin source files and two focused test/oracle
files. No OKX changes or old feature work are carried in that diff. The final
receipt commit changes only this manually authored evidence file.
