# KuCoin futures order controls

## Source and census

Fresh branch: `feat/kucoin-order-control`. Actual integration parent:
`e8e691e4890213c383687a99a933e4332dddb758`.
Source and measured gate SHA: `ef5754481663a5fc316481f723557c40729ca38b`.
No feature branch or unreviewed local work was inherited.

Independent Python 3.11 `tomllib` census compared each tracked KuCoin manifest
with `git show` at that parent: **9 pending -> ported, 0 -> divergent**.
Before: 113 ported / 39 divergent / 39 pending.
After: 122 ported / 39 divergent / 30 pending.
The manifest enumeration reached 12 files, stopping at
`exchanges/kucoin/kucoinspot.ws-public.ts.toml`; it is a venue census, not proof
of every remaining function. An independent Rust function-name enumeration
found exactly 23 functions in `order_control.rs`, all present in the nine
transitive mapping arrays; it stopped at `change_position_mode`.

| Source body | Primary Rust function |
| --- | --- |
| cancelAllOrders | cancel_all_orders |
| cancelOrders | cancel_orders |
| cancelNormalOrders | cancel_normal_orders |
| cancelSymbolOrders | cancel_symbol_orders |
| updateOrder | update_order |
| updateAlgoOrder | update_algo_order |
| setLeverage | set_leverage |
| changeMarginType | change_margin_type |
| changePositionMode | change_position_mode |

Mapping arrays/provenance were written with the manifest parser/serializer.
The extractor writer then exited 0, reporting zero updated manifests, zero
added and zero orphaned symbols. One supporting `OrderControlOps` declaration
has a C3 record grouped beside existing KuCoin declarations. No algorithm is
exempted. No shared type/trait or core query ABI was changed.

## Native algorithm and host boundary

The Rust bodies own all venue branches, try/catch boundaries, callback bodies,
payload fields, property-presence choices, response checks, emission signatures,
sequential awaits and original error propagation. Normal cancellation spreads
before the parent-ID overwrite, deduplicates first parents and retries strict
numeric 404 failures on the stop book. A failed retry retains the original
failure. Body errors perform throw-preserving IteratorClose; iterator-next
failures propagate directly.

The local `OrderControlOps` extends existing local `StreamOps` without changing
that carrier. Opaque values retain object/error/callable identities. `capture`
resolves the complete live expression and a bound callee retains its receiver;
`invoke` calls that captured reference. `get`, optional access, truthiness,
strict equality, `in`, literals and error construction have their JavaScript
semantics, including observable accessor/conversion errors.

`map`/`filter` own complete live member lookup, invocation and imported/custom
callback behavior. `pair` owns array-binding iterator creation, element steps
and closing after the second binding. `iterator`/`next` own for-of protocol;
`close_throw` owns IteratorClose with an existing throw completion. Imported
partition and uniqBy use captured callables. The intrinsic contracts are
language expressions: spread/object-literal field definition, indexed access,
ID split, numeric comparison, template-prefix coercion before ID evaluation,
concatenation, `.replace` capture before its arguments, and live `.toString()`.
They do not execute a venue method or decide a venue branch.

Await handles complete already initiated host work without cancellation; callers
poll new futures immediately for pre-await initiation. Concrete exchange class,
provider, language-capability assembly and arbitrary thenable/host scheduling
behavior remain outside this body mapping. They are not proven by the fixture
adapter.

## Measured gates

At `ef5754481663a5fc316481f723557c40729ca38b`, sequential commands measured:

| Command | Exit | Observed output |
| --- | --- | --- |
| `/opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --all-targets --jobs 2` | 0 | Finished dev profile; no errors |
| `/opt/homebrew/opt/rustup/bin/cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --test order_control --jobs 2` | 0 | 2 passed, 0 failed |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 | 7,341 symbols / 512 source files; would update 0 manifests; 0 added/orphaned |

The test command used
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Extractor output text was inspected alongside its measured exit.
Development runs included one compiler exit 101 for an unused test enum variant,
then exit-0 focused targets after removing it. That compiler failure is not a
mutation proof and is not relabeled as a named-assertion gate.

## Native proof floor

`order_control_native_before_source_oracle` checks 48 independently listed
fixtures: all 5, dispatch 2, normal 7, symbol 3, update 8, algo 4, leverage 7,
margin 6, mode 6. The enumeration stopped at `mode/refresh-reject`.
Every one of those native assertions runs before any packing or source wire
comparison. Assertions cover native result/value types, payload field order,
amount/price presence and numeric types, original order references, sequential
request/emission/store traces, literal independent counts, original Error
identity/name/message/code/IDs, fresh Error metadata and indexed identities of
the separately emitted and thrown open-position Errors.

The source oracle extracts the maintained nine initializers, endpoint table
and MarginType enum directly from TypeScript ASTs. It installs arrow initializers
through a receiver-bound factory, preserving lexical `this`, and uses installed
lodash partition/uniqBy. Other receiver methods/XHR/error emitters are fixture
bindings; their full runtime implementations are not an integration claim.
Each source result is compared only after all 48 native fixture checks.

`cancel_normal_orders_waits_and_closes_only_abrupt_body` supplies three separate
native protocol witnesses: Pending until first settlement before second request,
once-only closing on body rejection, and zero closing/requests on next failure.
This enumeration stops at the next-failure witness. It proves these body call
sites under the stated capability contracts, not an arbitrary external provider.

## Discriminating controls

Eight family controls were measured against source SHA
`ef5754481663a5fc316481f723557c40729ca38b`. Each altered production code,
compiled, ran the named native assertion, exited 101, restored the exact source
bytes, then passed both focused tests at exit 0. These are eight family controls,
not nine independent symbol controls; the two replacement rows share one family.

| Injected production change | Observed native failure | Broken / restored exits |
| --- | --- | --- |
| Wrong stop-cancel fallback diagnostic | EMIT native diagnostic value all/bad-empty | 101 / 0 |
| Virtual group spread before ordinary group | DISPATCH native original order identity and grouping | 101 / 0 |
| Retain suffixed ID in normal-order map | SEQUENCE native normal/ok; AWAIT native completion | 101 / 0 |
| Stop-book route for symbol cancellation | SYMBOL native route | 101 / 0 |
| Algo reduceOnly false | REPLACE native reduceOnly | 101 / 0 |
| Capture Math.min after Math.max reentry | LEVERAGE native clamped string | 101 / 0 |
| Incorrect isolated margin literal | MARGIN native wire mode | 101 / 0 |
| Swap position-mode string literals | MODE native string position mode | 101 / 0 |

The normal-parent runner initially exited 1 because its planned path assertion
was preempted by the earlier native independent sequence/count checks. The actual
native assertions above failed at exit 101, with a successful restoration at
exit 0. The runner's recorded label was corrected from those raw logs before
continuing the remaining five controls. No mutation survived. This control
enumeration stopped at the position-mode literal swap; it is not exhaustive.
No negative control was accepted on a compiler error.

Raw gate/control JSON and logs are under `/tmp/kucoin-order-control-*`; they are
not tracked generated artifacts. This manually written receipt records their
measured exits and exact SHA. A final scoped git diff against that SHA confirmed
source and test byte restoration at exit 0.

## Review and limits

Read-only sub-agent review found evaluation-order and assertion gaps; the
reviewed implementation now preserves pair/close protocol, endpoint coercion
and replace capture order, native Error identities/metadata, original Math.min
capture across Math.max reentry and numeric field types. A subsequent read-only
parser/callgraph check exited 0 with exact nine-row/transitive target provenance;
no concrete blocker remained under the stated contracts. The reviewer ran no
Cargo, tests or full Rust index.

Not run: workspace compilation, full KuCoin tests, other venues' tests, full
monorepo suites, host clippy, wasm clippy, full Rust-index coverage, CI or live
exchange/network tests. No claim of complete class/provider or host scheduling
parity, or complete remaining KuCoin/OKX coverage.

## Integration merge and fresh gates

Merged fetched integration `3cc94529835dde67ba3e6af513a233baa307c3f0`
cleanly into this branch at `71f0f4598ea265788afe8df22a63dde0e9c95f2e`.
Its sole merge base against that integration ref is the integration SHA above;
the branch diff contains seven files, stopping at `tests/order_control.rs`.

The scoped `git diff --exit-code ef5754481663a5fc316481f723557c40729ca38b
HEAD -- sdk-rs/safe-cex-venue-kucoin/src sdk-rs/safe-cex-venue-kucoin/tests
sdk-rs/safe-cex-venue-kucoin/scripts sdk-rs/safe-cex-venue-kucoin/Cargo.toml
sdk-rs/parity/manifest/exchanges/kucoin` exited **0**. Those inputs are
byte-unchanged. Listed shared Rust crates, SDK Cargo.toml, KuCoin TypeScript,
TypeScript types and root package/yarn inputs also diffed at exit **0**.
Cargo.lock gained an unrelated incoming dependency line; it was not included
in the shared-input zero claim.

Because the lockfile changed, the scoped check, focused target and extractor
were all measured freshly at **71f0f4598ea265788afe8df22a63dde0e9c95f2e**:

| Gate (same exact commands/environment above) | Fresh exit | Observed output |
| --- | --- | --- |
| KuCoin check, all targets, jobs 2 | 0 | Finished dev profile; no errors |
| KuCoin order_control test, jobs 2 | 0 | 2 passed, 0 failed |
| parity:extract:check | 0 | 7,341 symbols / 512 source files; 0 updates/additions/orphans |

These results are fresh measurements at the merge SHA, not relabeled source-SHA
numbers. Controls remain explicitly measured at the original source SHA, with
their source/test inputs preserved by the scoped diff.

Independent `tomllib` remeasurement against the actual incoming integration
confirmed **9 pending -> ported, 0 -> divergent**, again 113/39/39 -> 122/39/30.
Its 12-file census stopped at `kucoinspot.ws-public.ts.toml`.
Parsed exemptions are exactly the 1,091 incoming records plus our unchanged
single C3 record = 1,092; incoming values and relative order are preserved.
Read-only sub-agent review independently confirmed the controls, preservation,
census and exact exemption union, with no remaining concrete blocker.

The delivery receipt update changes documentation only. Gate SHA provenance is
retained; source/test/build/manifest preservation is checked again before push.
Raw fresh records remain in `/tmp/kucoin-order-control-merge-*` and are not
committed artifacts. The unrun scopes and host-contract limits above still apply.
