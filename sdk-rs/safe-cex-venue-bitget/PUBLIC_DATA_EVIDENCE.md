# Bitget public data and stream bootstrap

Fresh branch `feat/rust-bitget-public-stream`; actual direct integration parent
`91bdde133d2f407dffa8f2140f94e87eac7c1d5f`. No separate unreviewed lane branch
was used as an ancestor. Final test/gate/review SHA:
`eee0411f7b2dd7bc7bb0cfe1401ee71e4d0fbbc3`. Production body SHA:
`d5e7b927f63ad47cf9f756f44af2c43141eaeea6`; production, capture script,
manifests and exemptions are byte-unchanged in the test-only follow-up.

## Census and actual body targets

Python3.11 **tomllib** compared `(manifest, kind, ts)` identities against that
parent: **8 pending -> ported, 0 -> divergent**. Eleven manifests, 227 identical
identities; **87 ported / 80 divergent / 60 pending** before, **95 / 80 / 52**
after. Enumeration stopped at sorted manifest eleven,
`bitgetusdc.exchange.ts.toml`. Census is separate from behavioral proof.

Eight actual targets in `src/public_data.rs`:

| TypeScript field | Rust body |
| --- | --- |
| BitgetExchange.fetchMarkets | fetch_markets |
| BitgetExchange.fetchTickers | fetch_tickers |
| BitgetExchange.fetchOHLCV | fetch_ohlcv |
| BitgetInverseExchange.fetchMarkets | fetch_inverse_markets |
| BitgetPublicWebsocket.connectAndSubscribe | connect_and_subscribe |
| BitgetPublicWebsocket.onOpen | on_open |
| BitgetPublicWebsocket#onMarketsChanged | on_markets_changed |
| BitgetPublicWebsocket.subscribe | subscribe |

REST bodies preserve captured xhr/get before parameter getters; native filter,
map and reduce order; Classic per-row margin lookup versus fixed inverse USD;
price precision and repeated symbol/status getters; ordered Classic maximum
quantity fallbacks versus inverse maxTradeNum; defaults, inverse marker and raw
info aliases; ticker lookup/skip and last-as-mark; limit fallback/cap, interval
lookup/digit split, repeated date-option getters, subtract capture before its
operands, 89-day/request-span bound, conditional startTime, and candle column-six
volume.

Socket bodies preserve lazy disposed/activity guards, attempt increment and
threshold, only using fallback when transport routing supplies a host, swallowed
fallback telemetry errors, counter updates, every-third ticker replacement,
ws assignment before ordered listener capture, subscribe/ping/off/on ordering,
optional reconnection event, existing ticker-key-count offset for added markets,
no-selected-argument early return, native keys/entries/flatMap/map ordering,
ordinary topic assignment, and send capture before JSON.stringify.

Opaque values retain native references and numeric/string representation. The
32 exemptions are actual trait/associated/operation-binding helpers, not
fabricated source rows. Native receiver member chains and captured callable
owners, array/property/object/iterator operations, imported operations (including
actual interval/timeframe/dayjs semantics), HTTP and separately mapped callees
are supplied contracts. The onMarketsChanged argument prologue supplies `added`.
Async bodies execute at the supplied execution/poll boundary. Concrete class,
Exchange/socket/HTTP assembly, eager promise start/lifetime/cancellation,
callback callability and arbitrary overridden/proxy/prototype operations remain
separate binding obligations; these fixtures do not establish them. No shared
crate type or trait was changed.

## Native assertions and source floors

Three tests in `tests/public_data.rs` assert native variants, numeric bits
(NaN class rather than payload bits), signed zero, UTF-16 units, independently
expected array/own-field/request/send/listener counts, full market/ticker/candle
fields, request parameters, topic state, flags and operation order before source
comparisons. Raw market info, captured listener values and off/on market-handler
aliases are asserted directly through Rc identity; original errors retain Rc
identity. Handler fixtures are opaque reference sentinels: alias forwarding is
proved, native function callability is not. Counts come from prescribed fixture
recipes and constants, not actual result lengths.

`public-data-capture.mjs` executes the eight actual field initializers, the real
endpoint/interval/base-url tables, timeframeToSeconds and 89-day constant. It
uses actual dayjs with a fixed now witness. HTTP, routing, emitter, store,
receiver methods and callback values are fixture collaborators. Sparse market
input has a hole at index 3 and exactly three filter callback reads. Source
callback-sentinel aliases are also checked before their output projection.
Original native objects passed to JSON.stringify are captured before
stringification. Independent expectations assert their op/args/instType/channel/
instId fields and array order; native send types are asserted separately. No
expected-string encoding or decode/re-encode comparison supplies that gate.
Wire output comparison follows all native assertions and is supplementary.

The **52** actual-source cases are bounded floors:

| Group | Independent floor | Stopped at |
| --- | --- | --- |
| Four REST bodies x six scenarios | 24 | index 23, candles/guarded |
| Four socket bodies x six scenarios | 24 | index 23, subscribe/emit-failure |
| Activity guards, no selected market, integer keys | 4 | index 3, subscription keys 2/10/z before candle channel |

REST witnesses include empty/absent data, original request rejection, sparse
filter input, NaN/signed-zero/default/infinity values, unmapped tickers, second
margin/index/to getter changes or error, daily UTC interval, invalid interval,
89-day cap, and omission of startTime when it is not earlier than endTime.
Socket witnesses include disposal, fallback threshold, missing fallback host,
telemetry failure swallowed versus send/subscription failure forwarded, a
missing ticker dictionary, activity preventing attempt increments, empty added
markets versus nonempty additions that select no argument, and integer own-key
ordering. These are not all error sites, array shapes, interval keys, calendar
units/date inputs, custom operations or native binding executions.

## Sub-agent review citation

`/root/account_read_review` reviewed exact committed blobs at
**d5e7b927f63ad47cf9f756f44af2c43141eaeea6**, verified immediate parent
**91bdde133d2f407dffa8f2140f94e87eac7c1d5f**, and found no blockers within the
declared supplied-operation body scope. It compared all eight actual TS
initializers, getter/capture order, fallbacks, date/window and volume selection,
sampling, listener aliases, open ordering and subscription enumeration; inspected
the fixed 52-case floor; independently parsed TOML and asserted exactly eight
transitions plus 32 distinct exemptions (exit 0). Its committed-source special
floor four asserted activity/reset/sampling, no-send guard and independently
decoded integer-key subscription fields/order (node/Python exit 0). The source
loader root was rebased for node -e and five TS dependency files verified
byte-identical to the reviewed SHA; actual dayjs was retained. It made no writes
and did not run Cargo/native Rust targets, extractor, compiling controls,
clippy/wasm, workspace or CI. Review covers this body commit, not all intervening
integration commits.

The same sub-agent reviewed exact follow-up
**eee0411f7b2dd7bc7bb0cfe1401ee71e4d0fbbc3**. It confirmed the native-proof
gap is resolved by checking original native payload objects directly, unchanged
independent counts (including zero), integer-key array order and send types
before source comparisons. It verified production/mappings/exemptions/script
bytes unchanged versus d5 (diff/read exits 0), found no blocker, and ran no
runtime gates at eee. Its earlier source-only executions apply to d5, not eee.

## Claims not run or not established

No workspace compilation, repo-wide builds/suites, host clippy, wasm clippy,
CI rerun, live credential/network tests, concrete runtime assembly or end-to-end
venue execution. No comprehensive calendar/date/coercion domain, overridden
operation/proxy/prototype/iterator behavior, callback callability or temporal
promise/cancellation proof. Separately pending class, transport assembly and
callee rows remain pending; these eight statuses cover the actual bodies over
the declared supplied native/imported-operation contracts.

## Positive gates at eee0411f7b2dd7bc7bb0cfe1401ee71e4d0fbbc3

Cargo uses `/opt/homebrew/opt/rustup/bin` on PATH, `--jobs 2`, one process at a
time. Tests set
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Gate | Measured exit / output |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --all-targets --jobs 2` | 0, Finished dev |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --jobs 2 -- --include-ignored` | 0, crate unit/integration/doc tests passed, including three new tests |
| `cargo test ... -p safe-cex-venue-bitget --test public_data --jobs 2` before controls and after restoration | 0 each, three passed each |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract` | 0, 7341 symbols; zero updates/additions/orphans, output inspected |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0, 7341 symbols; would update zero manifests, zero added/orphaned, output inspected |
| Rustfmt check of both new Rust files; `git diff --check` | 0 |
| Scoped git diff of source/tests/scripts versus this SHA after controls | 0, original bytes restored |

Earlier scoped check/full suite/writer/checker also passed with exits 0 at
**d5e7b927f63ad47cf9f756f44af2c43141eaeea6**, before the native-payload test
strengthening. The first writer ran after manifest editing before d5 and is not
claimed as a committed-SHA execution. The initial test development compile
failed due to a duplicated copied fixture Error type (101); source comparisons
then caught fixture numeric-state representation and the incorrect fixture daily
interval constant (1D versus actual 1Dutc). Those were corrected before d5.
The native subscription proof gap identified after d5 was corrected in the
eee test-only follow-up; no control had been run at the weaker version. These
are development/proof corrections, not measured mutation survivors or gates.

## Compiling controls at the same final gate SHA

Each changed only production `src/public_data.rs` and ran scoped Cargo
`--test public_data --jobs 2 <named test> -- --exact`. All six compiled (Finished
test), failed at the named native assertion with **exit 101**, and restored
original bytes. No measured survivor. The restored full three-test file then
exited **0**. These are bounded discrimination proofs, not an exhaustive matrix.

| Family / deliberate mutation | Scoped test | Named native failure, exit 101 |
| --- | --- | --- |
| Market maximum: maxMarketOrderQty -> maxTradeNum | `native_rest_then_source` | `PUBLIC native numeric REST result markets/normal.0.limits.amount.max: 7 != 6` |
| Ticker mark: last -> last + 1 | `native_rest_then_source` | `PUBLIC native numeric REST result tickers/normal.0.mark: 11 != 10` |
| Candle volume: column 6 -> 5 | `native_rest_then_source` | `PUBLIC native numeric REST result candles/normal.0.volume: 999 != 7` |
| Connection fallback: compare against MAX_ATTEMPTS + 1 | `native_socket_then_source` | `PUBLIC native fallback flag` |
| Open: ping before subscribe | `native_socket_then_source` | `PUBLIC native socket operation order open/normal` |
| Subscriptions: op subscribe -> unsubscribe in both bodies | `native_socket_then_source` | `PUBLIC native UTF16 native subscription payload.op` |

Python3.11 tomllib census repeated at **eee0411f7b2dd7bc7bb0cfe1401ee71e4d0fbbc3**: **exit 0**, identical 227 identities over eleven files, exactly eight pending -> ported, 87/80/60 -> 95/80/52.

## Integration merges and preservation

Merged integration **c0567b4ed712109f72436698411e8b35ed782b33** down in
**04c19409796386f717e4b07566de01175a957be5**. The only conflict was
`sdk-rs/parity/exempt.toml`: retained exact integration records and appended all
32 own helpers. Python3.11 tomllib asserted complete preservation, valid TOML and
unique rs identities, **exit 0**. No CLAUDE.md conflict or local edit.

`git diff --exit-code eee0411f7b2dd7bc7bb0cfe1401ee71e4d0fbbc3 HEAD --
sdk-rs/safe-cex-venue-bitget/src sdk-rs/safe-cex-venue-bitget/tests
sdk-rs/safe-cex-venue-bitget/scripts` measured **0**. The byte check over the
recursively discovered local Cargo dependencies (types/num/exchange/store/
transport/candle-cache/crypto), workspace/crate Cargo.toml, .cargo, Cargo.lock,
packages/safe-cex/src, package.json and yarn.lock also measured **0**. Cargo.lock
was separately parsed: no changed package records. Cargo and controls were
**not rerun after integration merges**; their measured exits remain at eee.

Writer and extractor check at **04c19409796386f717e4b07566de01175a957be5** both
measured **exit 0**, 7341 symbols, zero updates/additions/orphans, output inspected.
Tomllib census against c0567b4ed7 measured **exit 0**, the same eight transitions
and 87/80/60 -> 95/80/52 over 227 identities in eleven manifests.

A subsequent fetch brought integration **6d8cb632677f074c1452e9ea047a73f0c7a732bb**;
merged down without conflicts in **d1e817d728278a43d46852ee9c6cce62ea4b4434**.
That merge changed only Weex files. Repeated own source/tests/scripts and the
entire dependency/build/TS/package/lock byte checks measured **0** against eee.
Latest extractor check at **d1e817d728278a43d46852ee9c6cce62ea4b4434** measured
**exit 0**, 7341 symbols, would update zero manifests, zero additions/orphans;
output inspected. Writer was not rerun at this second merge; own metadata is
unchanged and the latest check verifies extractor agreement. Final tomllib
census against 6d8cb63267 measured **exit 0**, identical 227 identities over eleven
files, **8 pending -> ported, 0 -> divergent**, **87/80/60 -> 95/80/52**.
Evidence-only follow-up commits do not change these gate inputs; no execution is
misattributed to their SHA. Intervening integration work is outside this own
sub-agent body/test review.

The final pre-push fetch advanced integration to
**407c879ef469c4ad7f702980834d11e80aacedaa**. Merged down in
**93a3c60b62ca238c91c76ae023fcc27ba39d67e2**, resolving only exemptions with the
same exact-integration-plus-32-own-records preservation and uniqueness proof
(**exit 0**). Repeated scoped own source/tests/scripts and full dependency/build/
TS/package/lock byte comparisons against eee measured **0**. Latest extractor
check at **93a3c60b62ca238c91c76ae023fcc27ba39d67e2** measured **exit 0**, 7341
symbols, would update zero manifests, zero additions/orphans, output inspected.
No Cargo/controls or writer rerun at this final merge. Tomllib census against
407c879ef4 measured **exit 0**, unchanged 227 identities over eleven manifests,
**8 pending -> ported, 0 -> divergent**, **87/80/60 -> 95/80/52**. Final evidence
commit changes only this document; the Cargo/controls SHA remains eee and the
latest metadata gate SHA remains 93a3c60b62.
