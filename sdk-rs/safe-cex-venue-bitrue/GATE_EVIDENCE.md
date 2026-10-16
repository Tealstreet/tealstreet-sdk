# Bitrue gate evidence

## Account, order reconciliation and trade-history reads

Actual parent: `8cdc8f6eb4a04700bd2c7f3c706ecd9fdf1d3ebf`.
Frozen gate SHA: `3a2b6d9ab917ee14c675a6c30467e78f1ff21ad8`.
Recursive tomllib comparison against parent moves **9 pending -> ported,
0 -> divergent**: **41 / 25 / 41 -> 50 / 25 / 32**, 107 Bitrue rows.
The nine rows are fetchOpenPositions, fetchPositions, fetchBalanceAndPositions,
fetchOrders, fetchAndEmitOrders, fetchOrdersForSymbol, fetchBitrueTradeRows,
fetchHistoricalTradesInternal and fetchHistoricalFills. parsePositions remains
pending and is an underlying method dependency of this adapted read surface.
No Cryptocom or Orderly files changed.

Measured gates on the frozen implementation:

- Scoped `cargo test -p safe-cex-venue-bitrue --test collection_reads --jobs 2
  --locked`: **0** on restored frozen source; nine named tests / **53** finite
  cases. Independent floors: open 3, positions 6, balance 3, orders 5, refresh 7,
  per-symbol 7, raw trades 8, internal history 8, fills 6. Actual unchanged source
  arrows/helpers/enums/tables in **6** SDK files are hashed. Native ordered
  values/arrays/keys/UTF-16/number bits/NaN, original errors, delegated result/
  row/info handles, receiver and option identities, effects and post-await
  state are asserted before encoding. Retained results are checked recursively
  after modeled Store position, balance and ticker replacements.
- `python3.12 .../scripts/collection-reads-controls.py`: **0** at frozen SHA.
  Nine disjoint compiled defects produce aggregate **101**, each at its required
  named native assertion; exact bytes restored and all nine named tests return
  **0**. Defects: open delegate, metadata fixed-zero fields, balance result key,
  serial flatten, refresh sleep, reduceOnly mapping, trade request limit cap,
  history seconds conversion and fill timestamp divisor. **No survivors.**
  Separate body witnesses use controlled underlying methods; the shared request
  cap defect does not affect the history control's first default-limit case.
- Scoped `cargo check -p safe-cex-venue-bitrue --all-targets --jobs 2 --locked`:
  **0**. Scoped `cargo fmt -p safe-cex-venue-bitrue --check`: **0**.
- Scoped `cargo test -p safe-cex-venue-bitrue --test balance_retention --jobs 2
  --locked`: **0**, one existing regression,
  fallback_retains_balance_across_ticker_and_balance_writes. Old fallback results
  remain alive across unrelated ticker writes, balance replacement and reset;
  this guards the documented prior Store-wide borrow defect. No new mutation
  matrix for this already-ported family was run.
- Explicit Yarn WRITER `parity:extract`: **0**, updated 0 manifests, added 0,
  orphaned 0. rust_files was first derived with tomllib from row target unions.
  Explicit Yarn extractor check: **0**, 7341 symbols / 512 files, would update 0,
  added 0, orphaned 0.
- Read-only sub-agent review at frozen SHA: **clear**; independently counted
  nine moved rows, 50/25/32 and 53 cases. No agent Cargo or edits. Two vacuous
  retention assertions were removed before freezing; recursive native values
  after Store replacement and original returned-handle checks remain.

Cargo is `/opt/homebrew/opt/rustup/bin/cargo`, manifest `sdk-rs/Cargo.toml`.
Source oracle uses `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Yarn is `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js`.
Preflight fixture compilation exposed a temporary dependency-value borrow and
an incomplete find-to-some call replacement; both were corrected before the
frozen gates. These preflight exits receive no gate credit.

Scope: complete adapted read bodies over source-contract runtime/import
bindings, including current priorities, fetch generation, disposal checkpoints,
symbol-scoped error fallback, nullish timestamp precedence and per-fill fields.
Underlying methods/parsers, array/Set/numeric/string/date/table intrinsics,
HTTP, mapSeries, sleep and rejection observation remain dependencies. Existing
native read helpers supply bounded import implementations and retain their
older divergence classifications. Ordinary dense arrays and own properties,
primitive values, ASCII table/symbol fixtures, one ISO date plus invalid dates,
and finite controlled async schedules are the witness floor. Native Store
assembly is not claimed by this adapter; the separate production retention
regression above covers its prior guard defect.

Not run: full Bitrue/older test targets (except balance_retention), workspace
check/tests, host/wasm Clippy, wasm builds, TypeScript/monorepo suites, CI reruns,
browser/live exchange tests. No shared SDK type or trait changed. Fixed complete
exchange/Store/HTTP/scheduler assembly, arbitrary dates, sparse/custom arrays,
accessors/proxies/prototype table hits, arbitrary iterators/coercions and general
event-loop schedules are outside the finite floor. Canonical scope is
`sdk-rs/parity/MANIFEST-SPEC.md#bitrue-account-order-and-history-read-bodies`.

## Merge preservation and changed-input gates

Fetched integration `7639adb54fb3b475008a021cdea492d16241d7eb` merged down
without conflicts as `fef90c31e5e56b6f9b6e2118a73eeb277e544c5e`. Standalone

```
git diff --exit-code 3a2b6d9ab917ee14c675a6c30467e78f1ff21ad8 HEAD -- sdk-rs/safe-cex-venue-bitrue/src sdk-rs/safe-cex-venue-bitrue/tests
```

returned **0**. Source/test bytes are unchanged; additional scoped diffs of
Bitrue controls/Cargo.toml/manifests, selected source TypeScript/helpers, Yarn
lock/extractor and compiler/formatter configs are **0**. The Bitrue canonical
section is byte-identical despite other additions to MANIFEST-SPEC.md. All
**481** selected lock records compare identical through tomllib graph traversal;
source/test/script/Cargo.toml inputs of all **8** selected local crates have
scoped diff **0**. Recursive tomllib against fetched integration still measures
**9 pending -> ported, 0 -> divergent**, **41 / 25 / 41 -> 50 / 25 / 32**.

Changed inputs: workspace Cargo.toml adds safe-cex-lighter-sdk; package.json adds
two Kucoin test targets; other venues' manifests and canonical sections changed.
Therefore gates were revalidated at the merge SHA:

- Collection controls **0**: nine named compiled native assertion failures at
  aggregate **101**, nine restored passes at **0**, exact source bytes restored,
  all 53 cases passed. No survivors.
- Scoped all-targets check **0**; scoped balance_retention target **0** (one
  production regression); scoped fmt check **0**.
- Explicit Yarn extractor check **0**, 7341 symbols / 512 files, would update 0,
  added 0, orphaned 0. Selected extractor scripts and Yarn/package dependencies
  are unchanged; the package script change is specific to Kucoin.

Read-only review stands over byte-identical source/tests and canonical scope.
This record changes tracked evidence only; the standalone scoped source/test
diff repeats after its commit and before push. Non-claims above remain in force.

## REST trading and market bodies

Actual parent: `7d73334d51aab93a006d732997ec203cec61ffbc`.
Frozen gate SHA: `869657d1c9be7cbe4a28b7d84826316bc22d24a5`.
Production/test implementation SHA: `a678a3999e215e795cc82e996d2aaa2a76ce78b0`;
the following commit changes only the placement mutation to keep it compilable.
Recursive tomllib moves **9 pending -> ported, 0 -> divergent**:
**50 / 25 / 32 -> 59 / 25 / 23**, 107 Bitrue rows. Rows are fetchMarkets,
fetchOHLCV, formatCreateOrder, placeOrder, placeOrders, placeOrderBatch,
updateOrder, setLeverage and cancelOrders.

Measured gates on frozen/restored inputs:

- Scoped `cargo test -p safe-cex-venue-bitrue --test trading_rest --jobs 2
  --locked`: **0**, nine named tests / **60** cases. Independent floors are
  markets3, candles7, formatter14, place6, places5, batch7, update5, leverage6,
  cancel7. Unchanged source arrows, enums/tables and omission/inverse/math
  helpers from **6** hashed SDK files run in the oracle. Native ordered values,
  UTF-16, number bits/NaN, original errors/results/market info/caller handles,
  effects and post-await state are asserted before encoding.
- `python3.12 .../scripts/trading-rest-controls.py`: **0** at full frozen SHA.
  Nine independent compiled defects each reach its required named native
  assertion at aggregate **101**; exact bytes restored, all nine tests/60 cases
  pass at **0**. Defects: startup market leverage max, candle cache threshold,
  formatter futureModel, nested placement result, multiple-placement worker
  method, placement lot size, update price presence, cache-delete symbol and
  cancellation lot size. **No survivors.** Standalone source/test working diff
  against gate returns **0**. Wrappers have controlled underlying methods, so
  their controls cannot mask the formatter/batch body defects.
- Scoped all-targets `cargo check -p safe-cex-venue-bitrue --jobs 2 --locked`:
  **0**. Scoped `cargo fmt -p safe-cex-venue-bitrue --check`: **0**.
- Scoped `cargo test -p safe-cex-venue-bitrue --test balance_retention --jobs 2
  --locked`: **0**, one existing production Store regression. Retained fallback
  balances survive ticker writes, replacement and reset. No old-family mutation
  matrix rerun.
- Explicit Yarn WRITER `parity:extract`: **0**, updated 0 manifests, added 0,
  orphaned 0; rust_files union was first derived with tomllib from row targets.
  Explicit Yarn extractor check: **0**, 7341 symbols / 512 files, would update 0,
  added 0, orphaned 0.
- Frozen read-only sub-agent review: **clear** at gate SHA; independently counted
  nine rows, 59/25/23 and 60 cases. No agent Cargo or edits. The witness Error
  constructor was corrected before freezing and empty-metadata rejection
  witnesses were added to both batch and cancellation (undefined -> empty
  Error message). No production change was needed for that binding correction.

Preflight compilation found an unused copied test helper, which was removed.
A sequencing-error preflight overlapped formatting and used the old six-case
batch/cancel binary; it failed the independent floors and receives no gate
credit. Cargo was then run sequentially on stable 60-case input and passed.
The first aggregate placement mutant discarded res and failed denied-warning
compilation, not a native assertion; it receives no discrimination credit.
The corrected nested-result mutant uses res and reaches its named assertion
in the nine-family frozen run recorded above. Source was restored in finally.

Cargo: `/opt/homebrew/opt/rustup/bin/cargo`, manifest `sdk-rs/Cargo.toml`.
Oracle: `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Yarn: `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js`.

Scope preserves source lot-sizing quirks, attached-zero versus truthy refresh,
client-ID/UUID ordering, immediate flatten under formatter reentry, completion
order IDs, authenticated cancellation versus unlimited placement, current
post-await options/methods, strict cache/ready-state guards and timeout floor.
Submitted siblings continue and settle after Promise-all rejection. Source
constructor/helper/intrinsic/HTTP/worker/scheduler implementations are explicit
bindings; ordinary dense arrays/own properties, ordinary primitive values,
ASCII symbols, bounded times and controlled request schedules are the floor.
Wrapper underlying bodies are dependencies rather than full adapter assembly.
The production balance-retention regression is separate from modeled Store
operations in this witness. Canonical scope:
`sdk-rs/parity/MANIFEST-SPEC.md#bitrue-rest-trading-and-market-bodies`.

Not run: full Bitrue/older targets (except balance_retention), workspace check/
tests, host/wasm Clippy, wasm builds, TypeScript/monorepo suites, CI reruns,
browser/live exchange tests. No shared SDK type/trait changed. Prototype/symbol/
accessor behavior, arbitrary coercions/iterators, unbounded times, GC and general
event-loop scheduling, concrete complete exchange/Store/HTTP/worker/signer
assembly remain outside this finite floor.

## Trading REST merge preservation

Fetched integration `98a847bb171d9a895a95f1e4e287c9126fc2c66e` merged down
without conflicts as `db56d386d0fb65704771303f9e3b50815c70870d`. Standalone

```
git diff --exit-code 869657d1c9be7cbe4a28b7d84826316bc22d24a5 HEAD -- sdk-rs/safe-cex-venue-bitrue/src sdk-rs/safe-cex-venue-bitrue/tests
```

returned **0**. Source/test bytes are unchanged. Additional scoped diffs of
Bitrue controls/Cargo.toml/manifests, canonical notes, workspace/compiler/
formatter inputs, extractor/Yarn lock, source Bitrue/types and selected math/
inverse/omission/position-ID helpers return **0**. Tomllib graph traversal
compares all **481** selected lock records: identical. Source/test/script/
Cargo.toml inputs of all **8** selected local crates have scoped diff **0**.
Therefore Cargo/test/mutation/retention/fmt/review exits stand and were not
repeated after merge.

package.json changes only the unrelated Kucoin test command; JSON comparison
proves selected dependency declarations and both extractor commands identical.
Other venues' manifests changed, so extractor check reran at the merge SHA:
**0**, 7341 symbols / 512 files, would update 0, added 0, orphaned 0. Recursive
tomllib against fetched integration still measures **9 pending -> ported,
0 -> divergent**, **50 / 25 / 32 -> 59 / 25 / 23**, 107 total.

This record changes tracked evidence only; the standalone source/test diff
repeats after committing it and before push. Non-claims above remain in force.

## Private authentication and stream slice

Parent: `372972fbfaa126366d6309f4ba15fa84d07bc7a5`.
Frozen implementation/full gate SHA: `b0ca03b896911af37d690f20ca85b791bf6f5a45`.
Branch: `feat/rust-venue-bitrue-private-auth-streams`.
Recursive tomllib measurement: **8 pending -> ported, 0 -> divergent**;
**59 / 25 / 23 -> 67 / 25 / 15**, 107 total. Selected rows are createAPI,
BitruePrivateWebsocket, incrementalFills initializer, connectAndSubscribe,
onMessage, handleOrderTradeUpdate, fetchListenKey and updateListenKey.

Controls, compilation, scoped tests, extractor check, formatting and review
were measured on the frozen SHA. The writer ran before freezing on the same
manifest inputs. One Cargo, including fmt, ran at a time:

| Gate | Actual result |
| --- | --- |
| Explicit Yarn parity:extract writer | 0; 7341 symbols / 512 files; updated0, added0, orphaned0 |
| Explicit Yarn parity:extract:check | 0; would update0, added0, orphaned0 |
| private-auth-controls.py | 0; eight compiled defects each101 at its named native assertion; exact restoration; all eight restored0 |
| cargo test -p safe-cex-venue-bitrue --test private_auth --jobs2 --locked, restored run inside control | 0; eight tests, 62 independently fixed finite cases |
| cargo check -p safe-cex-venue-bitrue --all-targets --jobs2 --locked | 0 |
| cargo test -p safe-cex-venue-bitrue --test balance_retention fallback_retains_balance_across_ticker_and_balance_writes --jobs2 --locked -- --exact | 0; one production Store regression |
| cargo fmt -p safe-cex-venue-bitrue --check | 0 |
| Read-only sub-agent review | Clear at frozen SHA; independently8 pending->ported,0 divergent,67/25/15,62cases; no Cargo/edits |
| Scoped source/test diff against frozen SHA after restoration | 0 |

Discriminating assertions: api_native api/signed; class_native
class/pair.timeout; incremental_native incremental/factory-result;
connect_native connect/subscribe; message_native message/order; order_native
order/incremental-raw-scaling; fetch_native fetch/retry-success;
refresh_native refresh/invalid-response. All eight reached compiled native
assertion failure101; none survived. The class timeout assertion precedes
initializer-value observation, so its control is independent of the simultaneous
initializer defect. Each exact original source byte was restored in finally.

Floors: API10, class4, initializer2, connect6, message13, order10, acquire10,
refresh7. They end respectively at retry-error, incremental-error,
factory-error, retry-error, diagnostic-error, store-error, mutable-delays and
logger-throws. Actual SDK source is parsed/transpiled without body rewriting;
eight SDK inputs are hashed. Native ordered values/UTF-16/numeric bits/NaN,
genuine Error versus plain object, original caller/error/info/client/factory
handles, effects and final state precede encoding. Node/native HMAC compare
actual signatures. The retry array is live across awaits, raw cumulative fill
conversion/replay is exercised, refresh retains its source disposal behavior,
and class-member targets have an independent15-entry expectation.

Preflight lock omission, Rust shadow/unused test variant, witness event tags/
JSON numeric serialization and early-return flat trade fixtures were corrected
before freezing. Early-return frames received no discrimination credit; the
final order cases use actual nested futures payloads and include NaN quantities.
The offline preflight updated only Bitrue's direct dev-crypto lock edge, with
no package versions changed. No preflight failure is counted as a control.

Cargo: `/opt/homebrew/opt/rustup/bin/cargo`, manifest `sdk-rs/Cargo.toml`.
Oracle env: `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Yarn: `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js`.

Canonical scope:
`sdk-rs/parity/MANIFEST-SPEC.md#bitrue-private-authentication-and-stream-bodies`.
Original intrinsic/import/underlying method/Store/clock/timer/promise bindings
are explicit. Factory class environment owns persistent static/prototype
infrastructure; all15 existing body ABIs are provisioned, with detached ping
as the invoked class-member floor. The field initializer imports the original
incremental utility. Existing ts-bug DOUBLE PARSE f6de9ca8c4 is preserved through
mandatory original axios/transport bigInt bindings, not remeasured or fixed.
No shared SDK type/trait, complete JSON/query codec, general accessor/prototype/
symbol/coercion/iterator/GC/promise runtime or concrete complete SDK/Store/HTTP/
websocket/signer/live exchange assembly claim is made.

Not run: full Bitrue suite/older targets except balance_retention, workspace
check/tests, host/wasm Clippy, wasm builds, TypeScript/monorepo suites, CI reruns,
browser or live exchange tests. Gate evidence is tracked and manually recorded.

## Private authentication merge preservation

Fetched integration `51a76599506b631dbe122bcc00a259e4a8aeb259` merged down
without conflicts as `f345a94cc0073e4e86caf7d98026ccbe8a8ebbad`. Standalone

```
git diff --exit-code b0ca03b896911af37d690f20ca85b791bf6f5a45 HEAD -- sdk-rs/safe-cex-venue-bitrue/src sdk-rs/safe-cex-venue-bitrue/tests
```

returned **0**. Additional scoped diffs of Bitrue controls/Cargo.toml/manifests,
workspace/compiler/formatter/selected build scripts and extractor/Yarn lock/
actual SDK oracle inputs return **0**. The exact private-auth canonical section
is unchanged. Tomllib graph traversal compares all **481** selected lock
records: identical. Source/test/script/Cargo.toml inputs for all **8** selected
local crates are byte-unchanged. Cargo, controls, retention, fmt and review
exits therefore stand and were not repeated after merging.

package.json changes only an unrelated Kucoin test command. JSON comparison
proves all dependencies/devDependencies/resolutions/package-manager/engines
fields and both extractor commands identical. Thirteen other-venue manifest
inputs changed, so extractor check reran at the merge SHA: **0**, 7341 symbols /
512 files, would update0, added0, orphaned0. Recursive tomllib against fetched
integration measures **8 pending -> ported, 0 -> divergent**;
**59 / 25 / 23 -> 67 / 25 / 15**, 107 total.

This commit changes tracked evidence only; the standalone source/test diff
repeats after committing it and before push. Non-claims above remain in force.

## Exchange runtime and position slice

Parent: `72e143e508b0bd2fbcfa55de48da95a95bb165c0`.
Frozen implementation: `1fab8fd1324115b97c3374c42a38db6d11317f9f`.
Frozen merged/full gate SHA: `210c41b828c8295f9cc52f8bfb6962537cd35b81`.
Branch: `feat/rust-venue-bitrue-runtime-lifecycle`.
Recursive tomllib against parent and fetched integration:
**9 pending -> ported, 0 -> divergent**, **67 / 25 / 15 -> 76 / 25 / 6**,
107 total. Rows: Exchange.dispose, queueFastPrivateRefresh,
runFastPrivateRefresh, validateAccount, start, tick, parsePositions,
listenOHLCV and listenOrderBook. Class/constructor and four public socket rows
are the six remaining pending rows.

The writer ran before freezing on final manifest inputs: **0**, 7341 symbols /
512 files, updated0, added0, orphaned0. Nine compiled controls/restoration,
scoped check and extractor check first passed at the implementation SHA.
Fetched integration `6db94316e85a5487104f2030082f47a1401d9df4` merged cleanly
as the full gate SHA. The selected shared dependency
safe-cex-exchange/src/websocket.rs changed11 lines (optional before-close hook),
so compiled controls/restoration and check were rerun on the merge SHA rather
than carrying their old exits forward. Production retention and fmt were run
on the merge SHA. One Cargo, including fmt, ran at a time.

| Gate at merged/full gate SHA | Actual result |
| --- | --- |
| runtime-lifecycle-controls.py | 0; nine compiled defects each101 at named native assertion; exact restoration; all nine restored0 |
| cargo test -p safe-cex-venue-bitrue --test runtime_lifecycle --jobs2 --locked, restored run inside control | 0; nine tests, 65 independently fixed finite cases |
| cargo check -p safe-cex-venue-bitrue --all-targets --jobs2 --locked | 0 |
| cargo test -p safe-cex-venue-bitrue --test balance_retention fallback_retains_balance_across_ticker_and_balance_writes --jobs2 --locked -- --exact | 0; one production Store regression |
| cargo fmt -p safe-cex-venue-bitrue --check | 0 |
| Explicit Yarn parity:extract:check | 0; 7341 symbols / 512 files; would update0, added0, orphaned0 |
| Read-only sub-agent review | Clear at both implementation/full gate SHAs; independently9 rows/0 divergent,76/25/6,65cases; no Cargo/edits |
| Standalone source/test diff against implementation after merge | 0 |

Controls/assertions: dispose_native dispose/active; queue_native
queue/debounce; refresh_native refresh/success; validate_native
validate/invalid-fallback; start_native start/authenticated; tick_native
tick/closed-positions; positions_native positions/mapped; candles_native
candles/original-return; book_native book/original-return. All nine compiled
and failed native assertions101, then passed after exact restoration0 at each
frozen SHA. No survivor. Linked refresh is later than the queue's independently
failing debounce case, preserving independent control attribution.

Floors: disposal7, queue8, refresh10, validation7, start10, tick9, positions9,
candles3 and book2 (65 total). They end at replace-private, linked-refresh,
empty-pending, balance-sync-throw, private-throws, current-loop-member,
after-await-mutation, delegate-throws and delegate-throws. Four actual SDK
source inputs are hashed. Unchanged source arrows/enums/constants/position-ID
helper execute in the oracle. Native ordered values/UTF-16/numeric bits/NaN,
original errors/caller/snapshot/info/cleanup and fallback handles, effects,
callbacks and final state precede encoding. Original refreshed positions and
original account input/snapshot arguments are asserted directly. The queue
callback invokes the new refresh body; finally uses a distinct thrown value
from the request failure; current tick callback returns its original result.

Preflight compilation found an unused copied callable variant, removed before
freeze. Review corrected the canonical/start-note count to13 fields/11 zeros
in `828283909e`; source/tests/controls stayed unchanged. The evidence commit
also narrows a canonical assertion sentence to the directly checked identities,
without changing code, cases or manifest targets/statuses.

Input audit from implementation to merge: selected lock graph **481 records
identical**. For all **8** selected local crates, only the shared websocket
source named above changed; this triggered the Cargo reruns. Workspace/compiler/
formatter/Node/extractor/Yarn lock and actual SDK source inputs are byte-unchanged.
The sole Bitrue manifest difference is reviewed startup-note wording. Eighteen
extractor manifest inputs changed, so extractor check reran after merging.
Source/test bytes still match the implementation. The standalone source/test
diff against full gate SHA repeats after the evidence commit and before push.

Cargo: `/opt/homebrew/opt/rustup/bin/cargo`, manifest `sdk-rs/Cargo.toml`.
Oracle env: `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Yarn: `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js`.
Canonical scope:
`sdk-rs/parity/MANIFEST-SPEC.md#bitrue-exchange-runtime-and-position-bodies`.
Original helper/intrinsic/Store/HTTP/socket/super/timer/loop/promise bindings
are explicit. Existing helper/metadata divergence classifications remain.
The floor covers ordinary own properties/dense arrays, primitive/ASCII fixtures
and bounded controlled schedules, rather than a complete SDK/Store/network
assembly, arbitrary proxy/accessor/prototype/symbol/coercion/iterator behavior
or general GC/promise/loop runtime. No shared SDK type/trait changed in this slice.

Not run: full Bitrue suite/older targets except balance_retention, workspace
check/tests, host/wasm Clippy, wasm builds, TypeScript/monorepo suites, CI reruns,
browser or live exchange tests. Evidence is tracked and manually recorded.

## Class composition and public subscriptions

Parent: `4d4b43c2bfc2ef7649b3a24cb658f9003525bec5`.
Frozen implementation/full Cargo gate SHA:
`aae4f2367ae094afd81b2bbcfd741d3c5aaeb768`.
Fetched integration: `a553ce92e78a0559194e953f569bf4fe42e6db8c`.
Clean merge SHA: `07ddf09ce803966745842bbe63e92894e5ecad02`.
Branch: `feat/rust-venue-bitrue-composition-public-streams`.

Recursive Python3.12 tomllib comparison against fetched integration:
**6 pending -> ported, 0 -> divergent**, **76 / 25 / 6 -> 82 / 25 / 0**,
107 total. The six rows are grouped into composition (BitrueExchange,
BitrueExchange.constructor, BitruePublicWebsocket), message routing
(BitruePublicWebsocket.onMessage), and retained subscriptions
(BitruePublicWebsocket.listenOHLCV and listenOrderBook). Sorted file.rust_files
unions independently match actual row.rs targets across every Bitrue manifest.

| Named gate | Actual exit / result |
| --- | --- |
| Explicit Yarn parity:extract, before freeze on final manifest inputs | 0; 7341 symbols / 512 files; updated0, added0, orphaned0 |
| cargo test -p safe-cex-venue-bitrue --test composition_streams --jobs2 --locked | 0; six tests / 50 fixed finite cases, baseline and restored run |
| composition-streams-controls.py at frozen SHA | 0; combined compiled mutation run101, all six tests fail their own named native assertions; exact restoration, six restored passes0 |
| cargo check -p safe-cex-venue-bitrue --all-targets --jobs2 --locked | 0 at frozen SHA |
| cargo test -p safe-cex-venue-bitrue --test balance_retention fallback_retains_balance_across_ticker_and_balance_writes --jobs2 --locked -- --exact | 0; one production Store regression at frozen SHA |
| cargo fmt -p safe-cex-venue-bitrue --check | 0 at frozen SHA |
| Explicit Yarn parity:extract:check | 0 at frozen SHA and rerun at merge SHA; 7341 symbols / 512 files; would update0, added0, orphaned0 |
| Read-only sub-agent review at frozen SHA | Clear; independently6 rows/0 divergent,82/25/0,50cases; no Cargo/edits |
| Standalone git diff --exit-code frozen SHA HEAD -- crate/src crate/tests after merge | 0 |
| Selected gate-input audit after merge | 0 for each scoped diff; 481 selected Cargo.lock records identical, all8 local crates unchanged; canonical section unchanged |

Six discriminating controls: exchange_class_native at
exchange_class/pair.initial-id (initial fetch ID0->1); constructor_native at
constructor/wiring (maxRPS5->4); public_class_native at public_class/pair
(nonempty initial topics); message_native at message/ping-zero
(trackLastReceived replaced); candles_native at candles/waiting
(100ms->101ms); book_native at book/primary-update (3000ms->3001ms).
Every defect compiled and reached its independently named native assertion
in the combined failing run101. Exact source bytes were restored and all six
tests passed0. No survivor. One Cargo, including fmt, ran at a time.

Independent floors: exchange class4, constructor6, public class4, message15,
candles8, book13 (50 total); stops map-throws, listener-throws, set-throws,
handler-throws, callback-throws and string-amounts. Registry expectations are
fixed46/16 lists independent of production tables. Seven actual SDK files are
hashed. Unchanged classes/arrows/constants and actual client-ID/JSON/totals
helpers execute in the oracle; Blob decompression remains a controlled original
import. Native ordered values, UTF-16, numeric bits/NaN, original errors,
rule/Grafana/frame/book/function handles, callback returns and final state are
asserted before encoding. Waiting and cleanup-before-connect schedules are
separate. Original returned cleanup remains retained after later callback errors.

Preflight compilation removed an unused copied import. Fixture corrections
retained cleanup after later callback errors and separated waiting schedules.
Explicit constructor excess arguments were truncated to the source's three
slots before the final baseline/freeze. These preflight failures are not counted
as mutation controls.

Merge audit: source/tests/scripts, venue Cargo.toml and Bitrue manifests are
byte-unchanged from frozen SHA. All8 selected local crates' sources/tests/scripts/
Cargo.toml/build.rs and all481 selected transitive lock records are unchanged.
Workspace/compiler/formatter configuration, extractor implementation, root
package commands/Yarn lock, Bitrue TypeScript, clientOrderId, jsonParse,
orderbook and readBlob inputs are unchanged. The canonical section is identical.
Thirteen other manifest inputs changed, so only extractor check reran after
merge; unchanged Cargo gates retain their frozen exits. The standalone source/
test diff repeats after the evidence commit and before push.

Cargo: `/opt/homebrew/opt/rustup/bin/cargo`, manifest `sdk-rs/Cargo.toml`.
Oracle env: `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Yarn: `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js`.
Canonical scope:
`sdk-rs/parity/MANIFEST-SPEC.md#bitrue-class-composition-and-public-subscriptions`.
Original imports, superclass, member ABIs, property/callee/array/Set/Map/JSON/
Blob/timer/Store/HTTP/socket bindings are explicit dependencies. Existing leaf
divergence classifications remain. Ordinary own properties/dense arrays,
primitive/ASCII fixtures and bounded controlled schedules define this floor.
Arbitrary proxy/accessor/prototype/symbol/coercion/iterator/GC/promise behavior,
complete JSON/gzip codecs, general event-loop scheduling, all-member invocation
and concrete complete SDK assembly remain outside it. No shared SDK type/trait
changed.

Not run: full Bitrue suite/older targets except balance_retention, workspace
check/tests, host/wasm Clippy, wasm builds, TypeScript/monorepo suites, CI reruns,
browser or live exchange tests. This evidence is tracked and manually recorded.

Final pre-push fetch advanced integration to
`2a0e1813663e4b5bb627bfde9db765cd958c5d84`; it merged cleanly as
`4ce9fd5e5a15ce9db52c0ada3e83d656b47d051b`. Repeated standalone source/test
diff and all selected input scopes exit0; the same481 lock records and8 local
crates remain identical. One further extractor manifest changed
(lighter-sdk/signer/signer.ts.toml), so parity:extract:check reran at this merge
SHA: **0**, 7341 symbols / 512 files, would update0/added0/orphaned0.
Recursive tomllib against this final fetched head repeats **76 / 25 / 6 ->
82 / 25 / 0**, **6 pending -> ported, 0 -> divergent**. Cargo exits still
bind to the unchanged frozen implementation inputs.
