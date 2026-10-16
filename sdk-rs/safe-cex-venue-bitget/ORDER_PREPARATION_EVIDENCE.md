# Bitget order preparation and replacement

Branch `feat/rust-bitget-order-preparation`, direct integration parent
**2cdb439ce42f9889abad6252fd6a400f87db8632**. Exact body/review/gate SHA:
**30a6887738adc71befb621c52b384c7195349f0a**. No separate unreviewed lane
branch was an ancestor. Incoming integration work is outside own review scope.

Python 3.11 tomllib census at that SHA: **8 pending -> ported, 0 -> divergent**;
120 ported / 80 divergent / 27 pending becomes **128 / 80 / 19**. Eleven
manifests and 227 unchanged `(file, kind, ts)` identities. Enumeration stopped at
sorted file eleven, `bitgetusdc.exchange.ts.toml`. Census is distinct from proof.

| TypeScript symbol | Executable body in src/order_preparation.rs |
| --- | --- |
| BitgetExchange.formatCreateOrder | format_create_order |
| BitgetExchange#formatUtaCreateOrder | format_uta_create_order |
| BitgetExchange#formatUtaStrategyOrder | format_uta_strategy_order |
| BitgetExchange.formatAlgoOrder | format_algo_order |
| BitgetInverseExchange.formatAlgoOrder | inverse_format_algo_order |
| BitgetExchange.updateVirtualTpSlOrder | update_virtual_tp_sl_order |
| BitgetExchange#updateUtaStrategyOrder | update_uta_strategy_order |
| BitgetExchange.resolvePositionsForPlanType | resolve_positions_for_plan_type |

Bodies preserve source field/capture/read/guard order, cached precisions, TIF
lookup, native side results, undefined omission, split guards/remainder arithmetic,
client-ID assertion before UUID assignment, UTA full/partial live-position fallback,
Classic/inverse distinct algo schemas and trailing calculations, virtual parent/
child lookup and spread, presence-sensitive updates and cancel-before-place.
Strategy replacement snapshots the original before strict cancellation, retries
that exact original payload on place failure, retains the source nested emit/
rollback catches and rethrows the original placement error. Stop-sizing lookup
returns original store/cache/inflight results, starts owned work before assigning
inflight, caches only nonempty snapshots, degrades visible fetch failures to []
and clears inflight in finally, including when the fallback emitter throws.

Source lot arithmetic is preserved, not corrected: with amount 13, max 5 and
precision 1, the actual TS emits four size/qty-3 lots. Native expectations pin
that shape. Zero and negative max sizes retain one whole-order lot.

Four exemptions were generated from the actual OrderPreparationHost trait and
three methods, times/launch/await_value. Existing native REST/placement contracts
supply receiver/callee owners, native fields/arrays/iteration/operators and
imported operations. Host clones retain the same receiver/runtime. launch owns
the async body and starts it to first suspension; await_value adopts its native
promise. Concrete Exchange/class/HTTP/socket assembly remains pending. No shared
type, shared trait, dependency or lockfile was changed by this slice.

## Native and actual-source floors

Seven tests assert decoded native payload fields and field order, scalar variants,
f64 bits (NaN as native NaN), UTF-16 strings, independent counts/lot boundaries,
original opts/order/error/formatter/cache/position/promise aliases, fresh payload
objects, strict cancellation flags and effect ordering before source/wire checks.
Expected payloads come from literal fixture recipes; lot sizes/counts are explicit
independent expectations. Every native assertion for a group's cases completes
before that group's first wire/source assertion. There is no decode/re-encode gate.

The source oracle executes eight actual initializers, actual endpoint-independent
protocol order/TIF constants, seven enums, inverseObj/omitUndefined, the real
safe-math adjust and its actual cache loop/helpers, and Lodash times/omitBy.
Receiver/side/position/margin/formatter/write collaborators are fixtures. UUID is
fixed, not RNG proof. Source collaborator calls assert their receiver owner;
native array methods assert decoded selectors and owners. Native receiver-callee
ownership remains a supplied contract, not a separate decoded receiver-owner gate.

| Group | Independent case floor | Last iteration |
| --- | --- | --- |
| Classic normal | 13 | 12, UTA delegation |
| UTA normal | 13 | 12, unsupported type |
| UTA strategy | 12 | 11, live-position fetch failure |
| Classic/inverse algo | 19 | 18, Classic trailing margin failure |
| Virtual TP/SL replacement | 8 | 7, cancellation failure |
| UTA strategy replacement | 10 | 9, replacement-format failure before cancel |
| Stop-sizing position lookup | 9 | 8, rejected promise |

Total **84 cases**, a bounded floor. Cache cases independently assert distinct
native effect order for synchronous fetch throws versus rejected promises, TTL
boundary expiry, store/cached/inflight aliases, nonempty cache and empty/failure
noncache, and finally clearing after success or emitter rejection. Native fixture
futures deliberately yield once and settle under a bounded polling executor;
this does not certify general concurrent dedup, multi-await, cancellation or
runtime lifetime/ownership scheduling. Arbitrary getters/proxies/prototypes,
overridden imports/array methods, broad coercion/numeric/calendar/crypto domains
and binding implementations are outside this fixture proof.

First precommit test run exited **101**, six groups passed, cache source-order
comparison failed: the harness incorrectly converted a synchronous collaborator
throw into a delayed promise rejection. The host start fixture was corrected to
return synchronous invocation errors directly; the second precommit run exited
**0**, seven groups passed. This was a fixture correction, not a production
body defect or compiling mutation control. A separate rejected-promise case and
independent native effect-order assertions were then added at the committed SHA
above. Committed restoration gates below cover that updated floor.

## Sub-agent review citation

`/root/account_read_review` reviewed exact committed
**30a6887738adc71befb621c52b384c7195349f0a**, verifying its direct parent above.
No blocking body-parity or mapping findings across the eight rows within declared
scope. Review compared capture/read/literal/lot/client-ID guards, strategy size
fallbacks, distinct algo branches, virtual update presence/cancel order, strict
replacement/rollback/error precedence, cache launch/assignment/finally behavior
and the native-first independent 84-case fixture floor. Host Clone and fixture
Rc-shared state/counters were checked.

Reviewer measured committed git reads **0**; TOML exact eight pending -> ported,
four distinct actual helper exemptions and direct parent assertion **0**;
independent committed-source cache floor nine node/Python **0**, checking result,
launch/assignment/fallback/finally event ordering. Stopped at case 8, rejected
promise. Script loader/root rebased for node -e; seven TS dependencies checked
byte-identical to the review SHA. No edits. Reviewer did not run Cargo/native
Rust tests, extractor, compiling mutations, host/wasm clippy, workspace builds or
CI. Broad native binding, concurrency, multi-await and cancellation remain unproven.

## Actual completed gates

All Cargo commands use `/opt/homebrew/opt/rustup/bin` on PATH, two jobs, one
Cargo process at a time. Native/source tests use
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
At exact committed **30a6887738adc71befb621c52b384c7195349f0a**:

| Gate | Measured exit |
| --- | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --all-targets --jobs 2 | 0, Finished dev |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract | 0; output read zero updated/added/orphaned |
| same node/yarn parity:extract:check | 0; output read zero would-update/added/orphaned |
| Python 3.11 tomllib census | 0, eight transitions above |

Compiling mutation/restoration and integration-preservation measurements are
appended only after completion. Earlier lib-only preflight exited 0 on the
uncommitted initial body; it is not claimed as an all-target or committed gate.

Not run for this slice: full Bitget crate suite, workspace/monorepo builds,
host clippy, wasm clippy, CI or live-venue tests. Earlier full-suite exits in
REST_ACTIONS_EVIDENCE.md belong to the previous slice. Concrete assembly remains
pending; this slice does not certify the remaining 19 rows or the existing 80
divergent rows.

## Compiling mutations and restoration

At exact committed **30a6887738adc71befb621c52b384c7195349f0a**, eight separate
mutations compiled and each exited **101** at its named native assertion.
Every control was restored before the next. No mutation survived.

| Control | Exact selected test | Named native assertion |
| --- | --- | --- |
| classic-force | `native_classic_then_source` | assertion `left == right` failed: PREP native own field order normal payload fields and independent lots.0 |
| uta-margin-mode | `native_uta_then_source` | assertion `left == right` failed: PREP native own field order normal payload fields and independent lots.0 |
| strategy-mode | `native_strategy_then_source` | assertion `left == right` failed: PREP native own field order strategy partial/full native payload |
| classic-algo-coin | `native_algo_then_source` | assertion `left == right` failed: PREP native UTF16 Classic/inverse algo payload.marginCoin |
| inverse-algo-rate | `native_algo_then_source` | assertion `left == right` failed: PREP native own field order Classic/inverse algo payload |
| virtual-stop-field | `native_virtual_then_source` | assertion `left == right` failed: PREP native UTF16 virtual TP/SL independently expected payload.presetStopSurplusPrice |
| replacement-strict-cancel | `native_replace_then_source` | PREP native scalar strict cancellation flag.treatNotFoundAsSuccess: true != false |
| cache-ttl | `native_cache_then_source` | assertion `left == right` failed: PREP native independent cache event count |

Control enumeration stopped at index 7, cache TTL guard. This is a bounded
control floor, not exhaustive mutation coverage. Exact committed source bytes
were restored. The complete new target command
`cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget
--test order_preparation --jobs 2` then completed with exit **0**, seven passed,
84-case floor. Controls used this command with the selected test and
`-- --exact`, with the same TS environment. Scoped own src/tests/scripts byte
restoration, edition-2024 rustfmt check on the two new Rust files, whitespace
and oracle node syntax each independently exited **0** after restoration.

## Integration merge and fresh merged gates

Merged latest observed integration **a07e971bb08f867925ff18ca8b13d2f288d0deba** down.
Exact merge SHA **a9d80a52ab6f849ad0a61db9f347f0ef1f256ecf**. Sole conflict was parity/exempt.toml;
complete stage-3 integration file plus the four generated actual host records
were retained. Tomllib asserted all 1,242 integration records retained in order,
four own records retained and unique Rust symbols; exit **0**. No CLAUDE conflict
or unreconciled file. No separate unreviewed lane branch was merged. Incoming
integration code is outside this own-slice review.

At the exact merge SHA, scoped git diff --exit-code versus code/review SHA
30a6887738 over Bitget src/tests/scripts exited **0**, byte-identical. Independent
tomllib census versus merged integration exited **0**, still exactly eight pending
-> ported, zero divergent, eleven files / 227 unchanged identities, 120/80/27
-> 128/80/19; stopped at sorted file eleven named above. Wider recursive inputs
changed: Exchange/transport sources, workspace Cargo.toml/Cargo.lock and
package.json. These are disclosed, not called byte-identical. Fresh scoped Cargo
gates were therefore run at the committed merge SHA; controls were not repeated,
and their actual exits remain attached to preserved body SHA 30a6887738.

Fresh scoped all-target Cargo check at **a9d80a52ab6f849ad0a61db9f347f0ef1f256ecf**
exited **0**, Finished dev. Fresh manifest writer exited **0**, output read zero
updated/added/orphaned. Fresh extractor checker exited **0**, output read zero
would-update/added/orphaned. Native/source target completion is recorded below
only after the test process finishes. No full Bitget/workspace/clippy/CI/live
sweep was run after merge.

Fresh `cargo test ... --test order_preparation --jobs 2` with the TS environment
at exact committed **a9d80a52ab6f849ad0a61db9f347f0ef1f256ecf** completed with
exit **0**, seven passed, 84-case floor. Completion was observed before this
evidence-only commit. Final own source/test/script byte preservation, formatting,
oracle syntax and whitespace checks are measured again before push; this
documentation commit is not a new Cargo measurement.

Final pre-push fetch advanced integration to **7d73334d51aab93a006d732997ec203cec61ffbc**.
Merged cleanly, exact merge SHA **e82da18efc0581a483c34d2031bc849e23b9fbd3**.
Own src/tests/scripts and all recursive Rust/TS/build/package/lock inputs versus
fresh Cargo gate SHA **a9d80a52ab6f849ad0a61db9f347f0ef1f256ecf** each diff
**0**, including Cargo.lock. Incoming changes were Bitrue/parity documentation
and manifests. Cargo gates were not repeated after this preserved-input merge;
their actual fresh exits remain attached to a9d80a52ab. Own review/controls remain
attached to preserved 30a6887738. No incoming integration review claim.

At the exact final merge SHA, fresh writer and checker each completed with exit
**0**, output read zero updated/would-update, added or orphaned symbols.
Independent tomllib census versus the merged head exited **0**, exactly eight
pending -> ported / zero divergent, 120/80/27 -> 128/80/19, eleven manifests /
227 unchanged identities; stopped at sorted file eleven as above. Own restoration
diff, Rust formatting, oracle syntax and whitespace again each exited **0**.
The following commit changes only this evidence; it is not a new Cargo gate SHA.
