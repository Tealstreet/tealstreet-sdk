# Kraken public helper gates

Measured implementation: `e387b170a108c2e807550c56f75ddcf428cb12b4`.
Parent/integration baseline: `355683b84a7c62f3f5c9e91910c7e15438c57076`.
This branch starts directly from integration, without the account-validation
branch or recovered orphan work as implementation ancestors.

Python 3.12 `tomllib` over the Kraken manifest directory measured
**2 pending → ported, 0 pending → divergent**:

| Source helper | Rust function |
| --- | --- |
| `KrakenPublicWebsocket#getTickerProductIds` | `ws_helpers.rs::get_ticker_product_ids` |
| `KrakenPublicWebsocket#mapTimeframe` | `ws_helpers.rs::map_timeframe` |

Counts: 24 ported / 23 divergent / 86 pending at integration; 26 ported /
23 divergent / 84 pending at the measured implementation. These counts do not
include the account-validation rows in the separate PR 7426.

Commands ran from the repository root. Native tests and controls used
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Command | Measured exit / result |
| --- | --- |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kraken --test ws_helpers --jobs 2` | 0; both named tests passed |
| `python3.12 sdk-rs/safe-cex-venue-kraken/scripts/ws-helper-controls.py` | 0; two named production failures and two restored passes; runner reported implementation SHA above |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kraken --all-targets --jobs 2` | 0 |
| `cargo fmt --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kraken -- --check` | 0 |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0; 7341 symbols, 512 files; zero changed manifests, additions or orphans |

The maintained extractor writer reconciled the edited manifest before commit;
it added zero symbols and orphaned zero symbols.

`product_ids_live_source` executes nine actual-source cases and compares
27 native product IDs. Independent case/output bounds cover market ordering,
duplicates, inactive markets, empty and no-overlap active sets, numeric-looking
and prototype-looking symbols, callback mutation of Store markets, and the
callback's original error allocation and metadata. The market snapshot is taken
before invoking the callback, and the native Store borrow is released first.

`timeframe_live_source` executes 32 actual-source cases: twelve own timeframe
keys, twelve inherited keys and eight unknown UTF-16 keys. Native assertions
observe 20 f64 outputs by bits and 12 inherited identities through shared
TableHit, including callable arities and Object.prototype identity. Unknown keys
include unmatched surrogate units. The Futures 12h entry remains 720 minutes;
the Spot table's different 240-minute entry is not reused. Both unchanged source
members are selected by AST and executed in one Node process. Capture records
Node, V8, architecture and the actual source checksum. These counts are witness
floors, not exhaustive input enumeration.

The product control inverts the native active-membership predicate. Its compiled
test exits 101 at `PUBLIC-HELPER independent native product count`; original
production bytes are restored and the test exits 0. The timeframe control changes
the 12h entry from 720 to 240. Its compiled test exits 101 at
`PUBLIC-HELPER native timeframe bits`; restoration exits 0. No controls survived.

Sub-agent review explicitly inspected the committed implementation using
`git show` and cleared that SHA. Both `ported` classifications were accepted;
no concrete parity or witness findings remain. The reviewer ran no Cargo or
mutation gates and did not inspect temporary mutated production bytes.

Not run: workspace compilation, host/wasm Clippy, full test suite, credentialed
integration or historical control matrices. These are two helper-body mappings;
websocket lifecycle methods remain pending. This evidence update changes only
the tracked sheet; measured implementation inputs remain the SHA above.

---

# Kraken account validation gates

Measured implementation: `a6a3695e9d3f3ba5cdff06b485dab4fa8b146041`.
Parent/integration baseline: `b6334d224847aa18aacb3f1da51b2d7bbdcb619d`.
This branch starts directly at that integration head, without the preceding
Futures-candle/Spot-balance slice or other unreviewed implementation ancestors.

Python 3.12 `tomllib` over the Kraken manifest directory measured
**1 pending → ported, 1 pending → divergent**:

| Source row | Rust method body | Status |
| --- | --- | --- |
| `KrakenSpotExchange.validateAccount` | `validation.rs::validate_spot_account` | ported |
| `KrakenExchange.validateAccount` | `validation.rs::validate_futures_account` | FORCED divergent |

At this branch's baseline: 24 ported / 21 divergent / 88 pending.
At the measured implementation: 25 ported / 22 divergent / 86 pending.
These counts exclude the two rows in the separate open PR 7405.

Commands ran from the repository root. Native tests used
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Command | Measured exit / result |
| --- | --- |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kraken --test validation --jobs 2` | 0; three named tests passed |
| `python3.12 sdk-rs/safe-cex-venue-kraken/scripts/validation-controls.py` | 0; two compiled named failures and two restored passes; runner reported the implementation SHA above |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kraken --all-targets --jobs 2` | 0 |
| `cargo fmt --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kraken -- --check` | 0 |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0; 7341 symbols across 512 files; zero changed manifests, additions or orphans |

The maintained extractor writer was also run after each manifest edit; its final
run updated zero manifests and found zero added/orphaned symbols.

## Native witness floors

`futures_validation_live_source` and `spot_validation_live_source` each execute
76 cases through the unchanged actual TypeScript arrow, in one shared Node
process. The oracle records Node, V8, architecture and both source checksums.
The committed bounds are independent of the captured case-array lengths.

Native request assertions cover 152 requests: method, endpoint, absent GET body
and empty POST body. Prefix/continuation traces observe deferred read completion,
synchronous and asynchronous failures, and continuation-time balance callback
replacement. Direct native error and balance assertions precede any serialization.
Futures returns four native balances across six balance calls and three catch-path
rejections; Spot returns 29 balances across 31 balance calls and zero rejections.
The 33 balances yield 264 direct native numeric field comparisons, plus currency
key/count, retained info and absent optional-field assertions.

`futures_utf16_representation_boundary` adds two actual-source/native pairs.
Source string indexing creates an observable lone surrogate in both an HTTP
envelope and a catch response. Source UTF-16 units are asserted directly. Native
validation visibly reports or propagates the representation cause; it cannot
retain that unit in shared `AccountValidationError`. The canonical FORCED note
is `parity/MANIFEST-SPEC.md#kraken-account-validation-utf16`.

The four known HTTP status diagnostics remain source-compatible even when an
unused intermediate error unit is a surrogate; those cases are in both 76-case
corpora. These corpus sizes are measured floors, not exhaustive input coverage.

## Discrimination and review

The Futures production control replaces `Account validation failed` with
`Mutated account validation failure`. The compiled witness exits 101 at
`VALIDATION futures native error`; restored production exits 0.
The Spot control replaces its invalid-nonce diagnostic with `Mutated nonce
message`. Its compiled witness exits 101 at `VALIDATION spot native error`;
restored production exits 0. Both controls restore original production bytes.
No controls survived.

Sub-agent review explicitly inspected the committed implementation with
`git show` and cleared that SHA. Its status-precedence finding was fixed and
witnessed; its observable-surrogate finding has paired witnesses and the FORCED
classification. No remaining concrete findings. The reviewer ran no Cargo or
mutation gates and did not inspect temporary mutated production bytes.

Not run: workspace compilation, host Clippy, wasm Clippy, full test suite,
credentialed integration or historical control matrices. API factories and
exchange lifecycle assembly remain separate pending rows. This evidence commit
only adds this tracked sheet; it does not extend the measured implementation.

---

# Kraken REST read gates

Measured implementation: `9da5cff257682fb017c817bb05a4dd6664e763dc`.
Parent/integration baseline: `2bc83d78d4f821c4b116d082d02c7b5a2595ffc5`.
No unreviewed implementation ancestors beyond that integration head.

Python 3.12 `tomllib` over `parity/manifest/exchanges/kraken` measured
**0 pending → ported, 2 pending → divergent**: Futures `fetchOHLCV` and Spot
`fetchBalance`. Counts changed from 24 ported / 21 divergent / 88 pending to
24 ported / 23 divergent / 86 pending.

All commands ran from the repository root. Native tests used
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Command | Measured exit / result |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kraken --all-targets --jobs 2` | 0 |
| `cargo fmt --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kraken -- --check` | 0 |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0; 7341 symbols, 512 source files; zero changed manifests, additions or orphans |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kraken --test reads futures_candle_reads:: --jobs 2` | 0; two named tests passed |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kraken --test spot_balance spot_balance_live_source --jobs 2` | 0; one named test passed |
| `python3.12 sdk-rs/safe-cex-venue-kraken/scripts/rest-read-controls.py` | 0; two named production failures and two restored passes |

The Futures mutation replaced `volume: parse("volume")?` with
`volume: parse("open")?`. The compiled test exited 101 at
`FCANDLE native volume case 0 row 0`; restoration exited 0, with both candle tests
passing. The Spot mutation replaced ticker multiplication with addition. Its
compiled test exited 101 at `SBALANCE direct native balance`; restoration exited 0.
Both controls restored original production bytes. No surviving controls.

The Futures oracle pins 47 cases, 74 native records and 444 numeric field
comparisons, plus one mixed-NaN boundary. The Spot oracle pins 29 cases and
49 native currency comparisons, plus its UTF-16 boundary. These are the measured
corpus floors, not an exhaustive runtime enumeration.

Sub-agent review explicitly inspected the committed implementation SHA using
`git show` and cleared the slice. Its sole finding was mixed NaN/non-NaN sorting:
the source sorts `[3,2,NaN,1]` as `[1,2,3,NaN]`, while Rust differed. The historical
CHOSEN whole-batch refusal was superseded by decision 3's per-row rule: each NaN
timestamp is diagnosed and refused, while good rows remain sorted and returned.
The original source corpus is unchanged; native admission now has 73 rows and
438 field comparisons, with the refused row observed separately. Other declared host, UTF-8 and owned-fallback
boundaries remain in `parity/MANIFEST-SPEC.md`.

Not run: workspace compilation, host Clippy, wasm Clippy, full test suite,
credentialed integration or full historical control matrices. No claim of API
factory/timer/runtime assembly parity is made.


## Integration merge-down verification

Measured merge: `237d76e43c1effa852162b4f4c2d05997188ce41`.
Parents: `20a144b34db1af4b7c7f49e4e16b3e88410571c9` and integration
`355683b84a7c62f3f5c9e91910c7e15438c57076`.
This repair moves zero new rows; it retains both previously measured slices.
Both parent additions in CLAUDE.md and MANIFEST-SPEC.md are preserved verbatim,
as are both gate sheets and all manifest row annotations.

`git diff --exit-code a6a3695e9d HEAD -- sdk-rs/safe-cex-venue-kraken/src/validation.rs`
exited 0. A second scoped diff over every source file present at that gate SHA,
except module registration in `src/lib.rs`, also exited 0. The whole source tree
is not byte unchanged: integration adds Futures candles and Spot balance and
registers those modules. A scoped diff of those two incoming bodies against
`9da5cff257` exited 0. The validation tests, oracle corpus and controls also
match `a6a3695e9d` with scoped diff exit 0.

The changed crate/dependency inputs were checked again at the merge SHA:

| Gate | Measured exit / result |
| --- | --- |
| scoped Kraken `cargo check --all-targets --jobs 2` | 0 |
| scoped `cargo test --test validation --test reads --test spot_balance --jobs 2` | 0; 20 tests passed across the three named files |
| scoped `cargo fmt -- --check` | 0 |
| maintained `parity:extract:check` through the explicit Yarn JS path | 0; 7341 symbols; zero changes, additions or orphans |
| `python3.12 sdk-rs/safe-cex-venue-kraken/scripts/validation-controls.py` | 0; both named production mutations exited 101, both restored runs exited 0; reported merge SHA above |

Original production bytes were restored. No workspace build, Clippy, full suite
or credentialed checks were run. This follow-up evidence commit only appends
this verification; all measured source/test inputs remain the merge SHA's bytes.


## Public helper merge-down

Measured merge: `4f1540a12755d816f8e91f5d47482c32c0192a9e`, integrating
`5b5d9004c528addfc33e1ff47ad6961ac5dc94d8` into the public-helper branch.
Both helper and account-validation gate sections are retained, along with their
shared REST-read evidence. The automatically merged CLAUDE additions are retained.
This conflict repair moves zero new rows.

Scoped `git diff --exit-code e387b170a1 HEAD` over `src/ws_helpers.rs`, its
native test, source oracle and control script exited 0. Shared numeric, model,
Store and exchange source dependencies and Cargo manifests/lockfile did not
change from the helper gate inputs; the original two mutation proofs remain
applicable without another control matrix.

At this merge SHA, scoped Kraken all-targets check and fmt each exited 0.
Scoped tests `--test ws_helpers --test validation --jobs 2` exited 0 with five
named tests passing. The maintained extractor check exited 0: 7341 symbols,
zero changed manifests/additions/orphans. No workspace build, Clippy, full-suite
or credentialed checks were run. This evidence-only commit adds this section;
measured production/test inputs remain the merge SHA above.

# Kraken private WebSocket authentication gates

Reviewed gate SHA: `b02db52b5a831ccbacf358721336277e52b4b4eb`.
Implementation: `a8a015f7f05336e248189e9d1737ed9f3afa7ee5`, built directly
on requested integration `5b5d9004c528addfc33e1ff47ad6961ac5dc94d8`.
Merge-down: `2fc00cf069814237ef39fa349dff22ae64453a40`, with actual
integration parent `c384a7e16bb7d91acc6a3041fd3337437c6e63b3`.
The shared remote-tracking ref advanced after fetching `54851c81b7`; that
fetched SHA is not the actual merge parent. Integration ancestry check exited 0.

Measured with Python 3.12 tomllib over the committed Kraken manifest directory:
**4 rows pending -> ported; 0 -> divergent**, against actual integration parent.
That parent's census is 27 ported / 24 divergent / 82 pending; this slice is
31 / 24 / 78. The two public-helper rows arriving through integration are
already ported and are excluded from this slice's movement.

The new mappings are Spot `setToken` and `onOpen`, and Futures `authenticate`
and `onOpen`, in `src/private_auth.rs`. These are method-body ports using
host-polled futures, host transport/logging, declared string credentials,
shared prepared `JsJsonValue` tokens and shared `ExchangeError` callbacks.
Complete WebSocket classes, factories, remote signing and feed handling remain
pending. The token value preserves undefined, signed zero and nonfinite numbers
until JS wire projection; the second Spot frame reads current token state after
the first send callback. No RefCell borrow spans a host callback or an await.

Actual TS arrows are selected by AST in `tests/private-auth-oracle.cjs` and its
corpus supplement, using the shared actual-source loader. Fixed native coverage
stops after 13 setter cases, 24 Spot-open cases, 11 Futures-auth cases and
6 Futures-open cases. Independent trace totals are 55 / 14 / 9 and propagated
rejection totals 4 / 2 / 1 for those three async families. Native token bits,
authentication flags, challenge state, callback order, log severity and original
error metadata identity are asserted before wire comparisons. These finite
corpora are coverage floors, not exhaustive exchange/runtime coverage.

| Gate | Actual measured SHA | Exit / result |
| --- | --- | --- |
| Kraken cargo check, all targets, jobs 2 | b02db52b5a | 0 |
| Private-auth and Spot-balance native test files, jobs 2 | c4263d1b37 | 0; 5 tests passed |
| Reads test filtered to futures_tickers_live_oracle, jobs 2 | c4263d1b37 | 0; 1 passed, 15 filtered out |
| Kraken cargo fmt --check | c4263d1b37 | 0 |
| parity:extract:check, explicit Yarn JS runner | c4263d1b37 | 0; 7341 symbols, zero changes/additions/orphans |
| private-auth-controls.py | b02db52b5a | 0; 4 compiled named failures and 4 restored passes |
| Read-only sub-agent review | b02db52b5a | clear for the four mappings and ticker compatibility fix |
| Scoped git diff c4263d1b37..HEAD: crate src, tests, Kraken manifests | b02db52b5a | 0; all earlier gate inputs byte unchanged |
| Scoped git diff HEAD: same production/test/manifest paths | b02db52b5a | 0; mutations restored |

One production mutation per newly ported row was measured at b02db52b5a:

| Row | Deliberate defect | Named native assertion | Mutated / restored exit |
| --- | --- | --- | --- |
| Spot setToken | replace assigned token with undefined | PRIVATE-AUTH native token | 101 / 0 |
| Spot onOpen | leave authenticated false before subscribing | PRIVATE-AUTH native authenticated | 101 / 0 |
| Futures authenticate | leave waitingChallenge false before sending | PRIVATE-AUTH native waiting challenge | 101 / 0 |
| Futures onOpen | log authentication failure as warning | PRIVATE-AUTH native open severity | 101 / 0 |

Earlier measured failures are retained explicitly: the scoped check at merge
2fc00cf exited 101 because integration added shared Ticker.timestamp. Compatibility
commit c4263d1b37 supplies None in the Kraken initializer and two test fixtures,
plus a native absence assertion, matching the actual TS method's omitted field.
Existing Store fallback snapshots remain copied without timestamp projection.
The first control-script attempt at c4263d1b37 exited 1 before mutation because
its anchor matched two assignments. The next attempt at da22a83928 exited 1
because the mutant's unused token parameter was a denied compiler warning.
Neither attempt counts as a named native failure or a surviving mutation.
The control at b02db52b5a consumes the token before its incorrect assignment;
all four mutants compiled, failed named assertions, and restored with exit 0.

No workspace compilation, Clippy, full suite, wasm build, credentialed exchange
checks or real WebSocket sessions were run. This evidence-only follow-up changes
no measured source, test, manifest or control-script inputs.

# Kraken Futures orders and stream adapters

Reviewed gate SHA: `f95e154dca0c3a922383dd32c214ce81f042025b`.
Implementation SHA: `ca99c29246920e28bbd245b787bced6d32311023`, direct parent
`d3f2bb3940811e315aa9e4ab9d970f6b3c08783a`. There is no unmerged feature
work in this slice's starting ancestry.

Python 3.12 tomllib over the Kraken manifest directory measures **5 rows
pending -> ported, 1 -> divergent**. Parent census: 31 ported / 24 divergent /
78 pending. Slice census: 36 / 25 / 72, of the same 133 rows.

The five ported mappings are Futures and Spot exchange listenOrderBook, Spot
exchange listenOHLCV, and the Futures public socket's listenOHLCV/handleOHLC.
The delegators call the host-bound concrete factory with the venue and original
parent reference, assign the slot before connecting, then read the current slot
again for delegation. They preserve callback, options, disposer and original
error identity. Concrete socket classes/factories and their protocols remain
pending. The Futures public socket's two OHLC methods intentionally perform no
stream work in the source: the subscription returns a fresh no-op disposer,
and the incoming OHLC handler ignores its opaque borrowed message. These ports
do not implement the separate pending REST candle poller.

The order read is CHOSEN divergent at the existing prepared-data/shared Order
boundary. It preserves field selection/coercion, trigger mapping, clock/date
behavior, endpoint, read/trace sequencing and callback error identity. On error,
Rust returns an owned current Store snapshot while source returns the live
array. Nine paired witnesses mutate the source result and observe the Store
change; native mutation remains isolated. Two source stop orders own an explicit
undefined trigger slot while shared Order projects it to None. Native presence
projection and actual-source ownership are asserted. Owned retained info and
existing UTF-8 ingress limits are declared in MANIFEST-SPEC.md's Kraken Futures
order read section. No order write or Store write is implemented by this read.

Actual source arrows and the source trigger helper are selected by AST. The
order corpus stops at 67 cases, 60 native records, 14 clock calls, 10 diagnostics,
1 propagated rejection, 9 alias pairs and 2 undefined-trigger pairs. Each stream
delegator stops at 13 cases / 26 attempts, 99 events and 9 failures. No-op methods
stop at 12 input cases, including ignored malformed inputs and UTF-16 strings.
All native fields, numeric bits, identities, state and outcomes are asserted
directly. No result is re-encoded for comparison. These are finite coverage
floors rather than complete exchange/runtime coverage.

| Gate at f95e154dca | Measured exit / result |
| --- | --- |
| Kraken cargo check --all-targets --jobs 2 | 0 |
| Kraken cargo fmt -- --check | 0 |
| parity:extract:check through explicit Yarn JS runner | 0; 7341 symbols, zero changes/additions/orphans |
| Reads test filtered to historical_positions_live_oracle, jobs 2 | 0; 1 passed, 15 filtered out |
| order-stream-controls.py, serial Cargo jobs 2 | 0; all six named tests passed baseline and restoration; six compiled named assertion failures |
| read-only sub-agent review | clear for six rows, canonical notes, witnesses, controls and fixed-clock fixture amendment |
| git diff --exit-code HEAD over crate src/tests/scripts after controls | 0; original production bytes restored |

One compiled production mutation per new row:

| Rust body | Deliberate defect | Named assertion | Mutated / restored exit |
| --- | --- | --- | --- |
| fetch_orders | filled += 1 while amount stays unchanged | ORDERS native numeric filled | 101 / 0 |
| futures_listen_order_book | construct Spot instead of Futures | STREAM native constructor venue | 101 / 0 |
| spot_listen_order_book | construct Futures instead of Spot | STREAM native constructor venue | 101 / 0 |
| spot_listen_ohlcv | construct Futures instead of Spot | STREAM native constructor venue | 101 / 0 |
| futures_listen_ohlcv | return a disposer that rejects | STREAM native no-op disposer completion | 101 / 0 |
| futures_handle_ohlc | throw a caught native panic instead of completing | STREAM native OHLC handler completion | 101 / 0 |

The initial all-target check at ca99c29246 exited 101 because the older history
fixture lacked the shared runtime's newly required monotonic_now_ms method.
Compatibility commit f95e154dca adds a fixed 0.0 monotonic clock to that frozen
fixture; the new source bodies, manifests and witnesses remain byte unchanged.
The specific history test was rerun and passed. Six-test pre-commit validation
also passed but is not substituted for the SHA-bound baseline/restoration runs.

No full suite, workspace compilation, host/wasm Clippy, wasm build, credentialed
exchange checks or real streaming sessions were run. This evidence commit only
appends this sheet; it changes no measured production, test, manifest or control
inputs. Final delivery will record the fresh integration merge-down and the
required scoped source/test comparison before pushing.

## Order/stream slice merge-down verification

Integration `2702cf272626f1a64a60f07ba9a750f722185c2b` was freshly fetched and
merged down as `31107c604517e9fb653576791875a6feb8ebd130`, with evidence parent
`1f3c4ffebf273d0f9d46e2e79ecc61d77cff5f7c`. The merge was clean; no CLAUDE
conflict occurred. Recounting committed integration versus this head with
Python 3.12 tomllib still measures **5 pending -> ported, 1 -> divergent**:
31 / 24 / 78 becomes 36 / 25 / 72.

The mandatory whole-crate src/tests diff against gate f95e154dca exited 1.
Its sole incoming change is tests/ws_helpers.rs adding is_perpetual: None to a
Market fixture. Production-only diff exited 0. A second scoped diff of all six
new bodies, native tests, oracles/corpora, control script and Kraken manifests
exited 0. Own work survived byte unchanged, but the whole source/test tree is
not claimed unchanged from f95e154dca.

Integration also adds optional Market.isPerpetual to shared TS/native models,
its number-output/store-patch handling, and lockfile inputs. The affected scoped
gates were rerun at merge SHA 31107c6045:

| Gate | Measured exit / result |
| --- | --- |
| Kraken cargo check --all-targets --jobs 2 | 0 |
| cargo test --test orders --test stream_adapters --test ws_helpers --jobs 2 | 0; 8 tests passed across the three named files |
| Kraken cargo fmt -- --check | 0 |
| explicit Yarn JS parity:extract:check | 0; 7341 symbols, zero changes/additions/orphans |
| read-only sub-agent merge amendment | clear; own six rows unchanged, incoming fixture preserves absent-field behavior |

The six named production mutation/restoration outcomes remain measured at
f95e154dca. Their bodies, controls and native witnesses are byte unchanged; the
new merged native runs above confirm them on the changed shared model inputs.
No historical control matrix, full suite, workspace build, Clippy, wasm build,
credentialed API calls or streaming session was run. This follow-up only appends
merge verification; measured source/test/control bytes remain those of 31107c6045.

## Refreshed constructor witness and second merge

A second fresh fetch merged integration `1181108d2dfeee7dd6cb1d21a3a39922e1e13b9e`
as `88a48f275cf119b0574e980e11a6c289be624e7e`. Whole-crate src/tests diff against
31107c6045 exited 0, as did the Kraken manifests/scripts/source-TS comparison.
Shared inputs add Store.balance_handle, retained balance storage, unused numeric
and crypto helpers, a type alias and lockfile entries. No Kraken source body
changed. A refreshed scoped all-target check at 55527644b4 exited 0.

The stream oracle was then tightened at 55527644b4: separate Futures/Spot
constructor bindings report which source identifier actually gets invoked,
rather than infer the label from the selected family. Current source outputs
and fixed counts stay the same. Control runner 191fa0e45a adds validated optional
function selectors; unrelated controls are not replayed merely for ceremony.

| Gate | Actual measured SHA | Exit / result |
| --- | --- | --- |
| Kraken cargo check --all-targets --jobs 2 | 55527644b4 | 0 |
| cargo test --test orders --test stream_adapters --jobs 2 | 191fa0e45a | 0; 6 tests passed |
| Kraken cargo fmt -- --check | 191fa0e45a | 0 |
| explicit Yarn JS parity:extract:check | 191fa0e45a | 0; 7341 symbols, zero changes/additions/orphans |
| selected constructor controls: futures_listen_order_book, spot_listen_order_book, spot_listen_ohlcv | 191fa0e45a | 0; 3 compiled named failures at exit 101, 3 restored passes at exit 0 |
| read-only sub-agent amendment | 191fa0e45a | clear; distinct source constructor bindings and validated control selection |
| scoped git diff HEAD over crate src/tests/scripts after controls | 191fa0e45a | 0; restored bytes |

The only difference between 55527644b4 and 191fa0e45a is control-script selection;
Cargo/source/test inputs of the measured check are unchanged. The three other
row controls (orders and both no-op bodies) retain their original f95e154dca
proofs; their bodies and assertions remain unchanged. No repeated historical
matrix or additional venue implementation was introduced. The six-row census
remains 5 pending -> ported and 1 -> divergent, with 72 pending remaining.
This evidence-only follow-up changes no gate inputs; final preservation is
compared to 191fa0e45a before push.

## Startup, snapshot and idle-feed slice

Implementation and gate SHA: `b5e97772aa82537a9b3d11e001fdc815c970a1fb`.
Direct integration parent: `d414d22d6ec665edd5344bbb41aa12f8fcad9396`.
Python 3.12 tomllib measured **5 pending -> ported, 0 -> divergent**:
36 / 25 / 72 becomes **41 ported / 25 divergent / 67 pending**, across 133 rows.

The rows are both exchange start bodies, Spot fetchAndSetOrders, and both public
websocket clearFeedStatsIfIdle bodies. Opaque Rc batches preserve original
identity and current loaded flags. Bound hosts provide Store operations, socket
factories and independently reported unawaited tick work; their concrete classes
and scheduler remain separate rows. Idle helpers consume a lazy source for-in
enumeration including inherited entries, preserve early stopping and original
errors, and forward an opaque feed reference including UTF-16 values.
Canonical boundaries are in MANIFEST-SPEC.md, Kraken startup, snapshot and
idle-feed hosts. No complete-class or new Store implementation is claimed.

Witnesses execute actual TypeScript AST bodies and assert native fields,
constructor kinds, original Rc/error identity, numeric bits, operation order and
settlement before wire comparisons. Independent fixed coverage floors are:

| Body family | Cases | Native events / reads | Failures |
| --- | --- | --- | --- |
| Futures start | 41 | 270 events | 14 |
| Spot start | 41 | 270 events | 14 |
| Spot fetchAndSetOrders | 9 | 21 events | 5 |
| Futures idle feed | 12 | 16 reads, 6 cleanups | 2 |
| Spot idle feed | 12 | 16 reads, 6 cleanups | 2 |

All gates below were measured at the full implementation SHA above:

| Gate | Measured exit / result |
| --- | --- |
| Kraken cargo check --all-targets --jobs 2 | 0 |
| Kraken cargo fmt -- --check | 0 |
| explicit Yarn JS parity:extract:check | 0; 7341 symbols, zero changes/additions/orphans |
| cargo test --test startup --test feed_stats --jobs 2 | 0; 5 passed |
| scripts/startup-controls.py, serial Cargo jobs 2 | 0; 5 named compiled failures, 5 restored passes |
| read-only sub-agent review | clear at b5e97772aa; five rows, host boundaries, native witnesses and controls |
| git diff --exit-code HEAD over crate src/tests/scripts after controls | 0; restored source bytes |

| Rust body | Deliberate defect | Named native assertion | Mutated / restored exit |
| --- | --- | --- | --- |
| start_futures | use Spot public constructor | START native constructor kind | 101 / 0 |
| start_spot | use Futures private constructor | START native constructor kind | 101 / 0 |
| fetch_and_set_orders | leave orders loaded false | SNAPSHOT native orders-loaded flag | 101 / 0 |
| futures_clear_feed_stats_if_idle | ignore one active handler | FEED native cleanup count | 101 / 0 |
| spot_clear_feed_stats_if_idle | ignore one active handler | FEED native cleanup count | 101 / 0 |

No historical control matrix, full suite, workspace compilation, Clippy, wasm
build, credentialed calls or streaming session was run. This evidence-only
commit changes no measured gate inputs. Delivery requires a fresh integration
merge-down and scoped source/test preservation proof before push.

## Startup slice merge-down preservation

Fresh integration `9c151409d125381f67d08d2b644afa89c77a79c8` merged cleanly as
`417762c42b49bc9b445f0d488c8137e6f2f586c2`; no CLAUDE conflict occurred.
The required git diff --exit-code from b5e97772aa over the whole Kraken crate
src/tests exited **0**. The separate Kraken scripts/manifests/source-TS diff
also exited **0**. This is the source/test byte-unchanged case.

Shared inputs add access-token and exchange-metadata modules, Store diagnostics
and regex, a websocket presence helper, a leg-resolver type-alias rename and
lockfile entries. At merge SHA 417762c42b, the scoped Kraken all-targets check
exited 0, parity extraction check exited 0 with zero updates/additions/orphans,
and the five startup/feed native tests passed. Read-only merge review was clear
at that same full SHA; committed TOML still measures 5 pending -> ported,
0 -> divergent and 67 pending. The five original mutation/restoration proofs
remain bound to b5e97772aa, with their bodies, assertions and controls unchanged.
This follow-up changes only the tracked evidence sheet.

## Disposal and websocket connection slice

Implementation and gate SHA: `25899dbe0208c7757cbe7499b276e523af45b465`.
Direct parent: `ed6234e728291cd1abe8bffd4af446d43ceda243`.
Python 3.12 tomllib measures **5 pending -> ported, 0 -> divergent**:
41 / 25 / 67 becomes **46 ported / 25 divergent / 62 pending**, across 133 rows.

Rows: KrakenExchange.dispose and all four Futures/Spot public/private websocket
connectAndSubscribe bodies. Host-bound factories and original callbacks preserve
URL choice, initial disposal guard, create/assign order, listener identity and
registration order, current-slot reads after reentrant replacement, and source
readyState/close catch behavior followed by onClose. Disposal retains original
timer handles, lazy live Map enumeration, inactive writes, clear order and the
superclass disposal operation. Concrete classes, event-loop/factory bindings,
Map storage and timer engines remain separate dependencies. Canonical scope:
MANIFEST-SPEC.md, Kraken lifecycle hosts.

Actual TypeScript AST bodies supply the oracle; dispose executes the unchanged
member inside a subclass with a bound superclass fixture. Assertions inspect
native event fields, Rc/borrowed identity, original error metadata, slot state,
UTF-16 keys and retained cleanup state without native re-encoding. Independent
finite floors are 25 cases / 187 events / 9 propagated failures for each of the
four connection families, and 11 cases / 66 events / 6 failures for disposal.
Covered reentrancy includes slot replacement during listener registration and
readyState reads, and Map deletion/appending during timeout/inactive iteration.

All following gates were measured at the full implementation SHA above:

| Gate | Exit / result |
| --- | --- |
| Kraken cargo check --all-targets --jobs 2 | 0 |
| Kraken cargo fmt -- --check | 0 |
| explicit Yarn JS parity:extract:check | 0; 7341 symbols, zero updates/additions/orphans |
| cargo test --test lifecycle --jobs 2 | 0; 5 passed |
| scripts/lifecycle-controls.py, serial Cargo jobs 2 | 0; 5 named compiled failures, 5 restored passes |
| read-only sub-agent review | clear at 25899dbe02; bodies, boundaries, witnesses and controls |
| git diff --exit-code HEAD over crate src/tests/scripts after controls | 0; restored source bytes |

| Body | Deliberate defect | Native assertion | Mutated / restored exit |
| --- | --- | --- | --- |
| futures_public_connect | reverse testnet choice | CONNECT native URL | 101 / 0 |
| futures_private_connect | reverse demo prefix choice | CONNECT native URL | 101 / 0 |
| spot_public_connect | use private URL | CONNECT native URL | 101 / 0 |
| spot_private_connect | use public URL | CONNECT native URL | 101 / 0 |
| futures_dispose | clear active flags instead of candles | DISPOSE native clear order | 101 / 0 |

No full suite, historical mutation matrix, workspace compilation, Clippy, wasm
build, credentialed API calls or streaming session was run. This evidence-only
commit changes no measured inputs. Final delivery requires a fresh integration
merge-down and whole-crate source/test preservation proof before pushing.

## Public subscription and dispatch merge preservation

Fresh integration `fed054df915a131fcb87163ccdea48416ac7faa0` merged cleanly as
`93a772220375d43e59047558dddf5b149e5fbc67`; no CLAUDE conflict occurred.
The required whole-crate src/tests git diff --exit-code from gate
`7efbf7273a715dcb9d54c1b057afa89f679a42a1` exited **0**. Separate comparisons
of controls/Kraken manifests/Kraken TypeScript and shared dependency crates,
workspace Cargo/lockfile, shared TypeScript types and log severity also exited
**0**. This is the byte-unchanged case; the scoped compile/native/format and four
mutation/restoration proofs retain their original SHA binding and stand.

The global extractor was rerun for incoming other-venue manifests at full merge
SHA 93a772220375d43e59047558dddf5b149e5fbc67: exit 0, 7341 symbols, zero
updates/additions/orphans. Read-only preservation review was clear at that full
SHA. Python 3.12 tomllib against the fetched integration still measures 4 pending
-> ported, 0 -> divergent, with 50 / 25 / 58 totals. This append changes only
evidence and does not change measured gate inputs.

## Lifecycle slice merge-down verification

Fresh integration `d8c841d4c02209793adfbd58522f3db1316488ce` merged cleanly as
`e23e27ab918ffdd86cedce4c9b539a288263af98`; no CLAUDE conflict occurred.
The required whole-crate src/tests git diff --exit-code against gate
25899dbe0208c7757cbe7499b276e523af45b465 exited **0**. A separate scripts,
Kraken manifest and Kraken TypeScript comparison also exited **0**.
This is the source/test byte-unchanged case; all five mutation proofs stand.

Incoming shared inputs change HTTP/transport query-signing support and add
numeric order-policy/metadata modules. At merge SHA e23e27ab918ffdd86cedce4c9b539a288263af98,
the scoped Kraken all-targets check exited 0, extraction check exited 0 with
zero updates/additions/orphans, and the five lifecycle native tests passed.
Read-only preservation review was clear at that full merge SHA. Python 3.12
tomllib against the fetched integration still measured 5 pending -> ported,
0 -> divergent, with totals 46 / 25 / 62. This append changes only evidence;
measured production/test/control inputs remain byte unchanged.

## Public resubscription and Spot dispatch slice

Implementation and gate SHA: `7efbf7273a715dcb9d54c1b057afa89f679a42a1`.
Direct parent: `57a115c476b3a9ecc391145fec5e2492aeb1f006`.
Python 3.12 tomllib measures **4 pending -> ported, 0 -> divergent**:
46 / 25 / 62 becomes **50 ported / 25 divergent / 58 pending**, across 133 rows.
Rows are both public onOpen bodies, Spot public handleBook and Spot private
handleBalances. Canonical scope: MANIFEST-SPEC.md, Kraken public subscription
and Spot dispatch hosts.

Prepared native subscription envelopes retain all literal fields, 50-symbol
batches, UTF-16 topics, inherited interval identities and the existing Spot
INTERVAL table. Keys are snapshotted at each loop start and handler lengths read
from the current map. Futures book send failures log the original error and
continue; Spot failures propagate. Book dispatch forwards original Rc symbols
and items through live array iteration/current type reads. Balance refresh is
initiated and unawaited, with asynchronous reporting owned by its bound host.

Actual-source witnesses capture payload fields before JSON.stringify and assert
native enums, numeric bits, UTF-16 units, original identities and operation order.
JSON.stringify/send, object getters/maps, array iteration, child parsers and
parent refresh remain explicitly bound dependencies. Source wires are recorded
only as supplementary information: no native serializer/wire parity claim is
made by these four body-row ports.

| Independent finite coverage floor | Cases | Events | Prepared frames | Failures |
| --- | --- | --- | --- | --- |
| Futures public onOpen | 43 | 446 | 85 | 8 |
| Spot public onOpen | 43 | 477 | 122 | 9 |
| Spot book dispatch | 14 | 141 | n/a | 6 |
| Spot balance dispatch | 6 | 10 | n/a | 2 |

| Gate measured at the full implementation SHA | Exit / result |
| --- | --- |
| Kraken cargo check --all-targets --jobs 2 | 0 |
| Kraken cargo fmt -- --check | 0 |
| explicit Yarn JS parity:extract:check | 0; 7341 symbols, zero updates/additions/orphans |
| cargo test --test public_subscriptions --test spot_dispatch --jobs 2 | 0; 4 passed |
| scripts/public-subscription-controls.py, serial Cargo jobs 2 | 0; 4 named compiled failures, 4 restored passes |
| read-only sub-agent review | clear at 7efbf7273a; body contracts, witnesses and controls |
| git diff --exit-code HEAD over crate src/tests/scripts after controls | 0; restored bytes |

| Body | Deliberate defect | Named native assertion | Mutated / restored exit |
| --- | --- | --- | --- |
| futures_on_open | ticker feed becomes book | OPEN native feed | 101 / 0 |
| spot_on_open | book depth 100 becomes 101 | OPEN native book depth | 101 / 0 |
| handle_book | reverse snapshot branch | DISPATCH native method | 101 / 0 |
| handle_balances | reverse Array.isArray guard | DISPATCH native event count | 101 / 0 |

Before committing, the Spot interval assertion caught use of the generic
websocket helper instead of Spot INTERVAL. The port now uses the existing Spot
table, including unsupported timeframe substitutions. An earlier oracle-envelope
parse failure was fixed by carrying configuration keys as UTF-16 units; it is
not counted as a native production mutation proof.

No full suite, historical mutation matrix, workspace compilation, Clippy, wasm
build, credentialed API calls or streaming session was run. This evidence-only
commit changes no measured inputs. Delivery requires a fresh integration
merge-down and whole-crate source/test preservation proof before pushing.

## Refreshed subscription integration head

The last fetch found integration `d4d9ae0b019e4c5466a67294aa474f897ee38061`,
merged cleanly as `a8a83f1700b7ffd7467643ffc54166872cb7f1c8`. Required whole-crate
src/tests comparison against 7efbf7273a again exited 0. A separate comparison
against 93a7722203 over Kraken controls/manifests, all source TypeScript, workspace
Cargo/lockfile and shared dependency crates exited 0. The only new extractor
input is a Backpack manifest. Extraction was rerun at full SHA
a8a83f1700b7ffd7467643ffc54166872cb7f1c8 and exited 0 with zero updates, added
rows or orphans. The four-row census and all original native/mutation proofs
remain unchanged; no Cargo was repeated for unchanged inputs. This append changes
only the evidence sheet.

## Delivery merge verification

Fresh integration `18465853456056541848be3bb9e60ff3c68f565e` merged cleanly as
`b7f0be5c01440e2607dc12414b33e6e48246b55e`. The required whole-crate src/tests
diff against gate 7efbf7273a exited 0. An additional full dependency/TS/Kraken
controls-and-manifests comparison against a8a83f1700 exited 0. Incoming changes
are confined to other-venue and webworker work; no Kraken gate input changed.
Global extraction was rerun at full merge SHA b7f0be5c01440e2607dc12414b33e6e48246b55e:
exit 0, zero updates/additions/orphans. The native gates and four row-specific
mutation proofs remain SHA-bound to 7efbf7273a and stand. This append changes only
evidence; the delivery source/test preservation check is repeated before push.

## Periodic synchronization and Spot cancellation slice

Implementation and gate SHA: `6bb2efb8f6f3f79f8b37ac03dd6133cd5ac27ddf`.
Direct parent: `9f59912c6f39a0df543e6228c38ad5a57258d2a2`.
Python 3.12 tomllib measures **3 pending -> ported, 0 -> divergent**:
50 / 25 / 58 becomes **53 ported / 25 divergent / 55 pending**, across 133 rows.
Rows are both exchange tick bodies and Spot cancelOrders. Canonical boundaries:
MANIFEST-SPEC.md, Kraken periodic synchronization and Spot cancellation hosts.

Tick bodies preserve the default zero/f64 counter, Futures leverage prefetch,
ordered registration of three owned then callbacks, original opaque Rc snapshots,
full updates before current loaded flags, falsy-balance guard, disposal checks,
visible error emission and 500 ms loop scheduling. The registered callback calls
the current tick method, retaining receiver identity and count + 1 even after
method replacement. Promise task ownership/settlement, Store operations, loop
scheduling and error-message selection remain explicit host dependencies.
Sibling tasks continue after early aggregate rejection. Cancellation retains
empty/single/multiple branches, live array iteration and current original IDs,
sequential REST POSTs, CANCEL_ORDER/txid, ignored responses, error emission and
fresh shared exception construction. No new exception model or complete class
is claimed.

Actual-source bodies run under independently controlled fulfillment/rejection.
Native flags, enum operations, original batch/ID/error identity, UTF-16 ID units,
numeric bits and completion checkpoints are asserted without wire re-encoding.
The finite loop floor invokes one captured next-tick callback where specified.

| Independent coverage floor | Cases | Events | Checkpoints | Propagated failures |
| --- | --- | --- | --- | --- |
| Futures tick | 38 | 291 | 200 | 2 |
| Spot tick | 38 | 268 | 200 | 2 |
| Spot cancellation | 14 | 49 | 70 | 7 |

| Gate measured at the full implementation SHA | Exit / result |
| --- | --- |
| Kraken cargo check --all-targets --jobs 2 | 0 |
| Kraken cargo fmt -- --check | 0 |
| explicit Yarn JS parity:extract:check | 0; 7341 symbols, zero updates/additions/orphans |
| cargo test --test periodic_sync --test spot_cancel --jobs 2 | 0; 3 passed |
| scripts/periodic-sync-controls.py, serial Cargo jobs 2 | 0; 3 compiled named failures, 3 restored passes |
| read-only sub-agent review | clear at 6bb2efb8f6; bodies, host contracts, witnesses and controls |
| git diff --exit-code HEAD over crate src/tests/scripts after controls | 0; restored bytes |

| Body | Deliberate defect | Named native assertion | Mutated / restored exit |
| --- | --- | --- | --- |
| futures_tick | schedule at 501 ms instead of 500 | TICK native loop delay | 101 / 0 |
| spot_tick | schedule at 501 ms instead of 500 | TICK native loop delay | 101 / 0 |
| cancel_orders | single-order ID field becomes order_id | CANCEL native ID field | 101 / 0 |

No full suite, historical control matrix, workspace compilation, Clippy, wasm
build, credentialed calls or real timers/streaming session were run. This
evidence-only commit changes no measured inputs. Final delivery requires fresh
integration merge-down and whole-crate source/test preservation proof.

## Periodic synchronization merge-down verification

Fresh integration `852b13d6c0385f0f1ae607af880953c335f61726` merged cleanly as
`a3ce19e0878773e7ea70ae155f66a6d60cc91803`; no CLAUDE conflict occurred.
The required whole-crate src/tests diff from gate
6bb2efb8f6f3f79f8b37ac03dd6133cd5ac27ddf exited **0**. A separate controls,
Kraken manifests and Kraken TypeScript comparison also exited **0**.
This is the source/test byte-unchanged case; all three compiled mutation and
restoration outcomes retain their original SHA binding.

Incoming shared inputs add utility modules, including loop_util, Store read_blob
and flate2, and expose existing clock/polling constants without changing values.
At full merge SHA a3ce19e0878773e7ea70ae155f66a6d60cc91803, scoped Kraken
all-targets check exited 0, extraction check exited 0 with zero updates/additions/
orphans, and the three periodic_sync/spot_cancel native tests passed. Read-only
preservation review was clear at that same full SHA. Python 3.12 tomllib against
fetched integration still measures 3 pending -> ported, 0 -> divergent, and
53 / 25 / 55 totals. This append changes only the tracked evidence sheet.

## Periodic slice delivery preservation

The final fresh fetch found integration `6e5c3103e731ff6cb5c7198861d53ff0b70514c2`,
merged cleanly as `facbc47681e73d5e77676723e713efbe8af83aa9`. Required whole-crate
src/tests comparison against 6bb2efb8f6 exited 0. A separate comparison from
a3ce19e087 over Kraken controls/manifests, all source TypeScript, shared dependency
crates and workspace Cargo/lock inputs also exited 0. Incoming extractor inputs
are only Cryptocom and Weex manifests. Extraction was rerun at full merge SHA
facbc47681e73d5e77676723e713efbe8af83aa9: exit 0, zero updates/additions/orphans.
Scoped native/compile proofs remain those of a3ce19e087 and mutation proofs those
of 6bb2efb8f6, with all relevant inputs unchanged. This append changes only
evidence; source/test preservation is repeated before push.

## Ticker and Spot candle stream updates

Implementation and gate SHA: **8b4b5e67c50433a29704ce505c3d1f263e09be13**.
Integration parent: **23311da7a600fca7f130b840095483feac518959**.
Measured movement: **3 pending -> ported, 0 pending -> divergent**. Kraken totals
are 56 ported / 25 divergent / 52 pending across 133 rows.

Rows: KrakenPublicWebsocket.handleTicker, KrakenSpotPublicWebsocket#handleTicker,
and KrakenSpotPublicWebsocket#handleOHLC. Production bodies are in
src/stream_updates.rs; native witnesses are tests/stream_updates.rs, with actual
TypeScript bodies and actual get-key source plus installed lodash exercised by
tests/stream-updates-oracle.cjs and tests/stream-updates-corpus.cjs.

The declared hosts supply current property access, intrinsics, Store lookup/update,
array iteration and callbacks. Native witnesses assert ordered numeric patch
presence/values, original ticker/error/handler identities, UTF-16 keys and shared
candle mutation/deletion before wire encoding. OHLC uses the existing Spot interval
table's first strict numeric match. The oracle date context is UTC; production
Date behavior belongs to its bound host. Complete classes, intrinsic engines,
Store integration, JSON serialization and live websocket sessions are not claimed.
Canonical boundaries: MANIFEST-SPEC.md, Kraken numeric stream update hosts.

| Independent coverage floor | Cases | Events | Ticker updates | Candle callbacks | Failures |
| --- | --- | --- | --- | --- | --- |
| Futures ticker | 31 | 480 | 23 | 0 | 6 |
| Spot ticker | 31 | 328 | 22 | 0 | 7 |
| Spot OHLC | 45 | 479 | 0 | 40 | 12 |

| Gate at the full implementation SHA | Exit / result |
| --- | --- |
| Kraken cargo fmt -- --check | 0 |
| Kraken cargo check --all-targets --jobs 2 | 0 |
| explicit Yarn JS parity:extract:check | 0; 7341 symbols, zero updates/additions/orphans |
| cargo test --test stream_updates --jobs 2 | 0; 3 passed |
| scripts/stream-update-controls.py, serial Cargo jobs 2 | 0; 3 compiled named failures, 3 restored passes |
| read-only sub-agent review | clear at 8b4b5e67c5; bodies, boundaries, witnesses and controls |
| git diff --exit-code HEAD over crate src/tests/scripts after controls | 0; restored bytes |

| Body | Deliberate production defect | Named native assertion | Mutated / restored exit |
| --- | --- | --- | --- |
| futures_handle_ticker | bid patch key becomes ask | STREAM native patch key | 101 / 0 |
| spot_handle_ticker | mark receives last + 1 | STREAM native patch mark | 101 / 0 |
| spot_handle_ohlc | timestamp divisor becomes 100 | STREAM native candle timestamp | 101 / 0 |

No full suite, historical mutation matrix, workspace compilation, credentialed
calls or real websocket session were run. This evidence-only commit changes no
measured inputs. Delivery requires integration merge-down and the scoped
source/test preservation comparison against the implementation gate SHA.

## Stream update merge-down verification

Fresh integration **972c5ec9bd8c4bb2023530199c45961dfca86229** merged cleanly
as **0f8ae368c3bb8063c7a19fef80cb753d30e6a7ce**. No conflict occurred.
Required git diff --exit-code from 8b4b5e67c50433a29704ce505c3d1f263e09be13
through merged HEAD over the whole Kraken src/tests trees exited **0**.
Separate controls, Kraken manifests, all source TypeScript, shared dependency
crates and workspace/venue Cargo.toml comparison also exited **0**. This is the
source/test byte-unchanged case. The only lockfile change adds base64 to Apexomni;
Kraken dependencies are unchanged.

At full merge SHA 0f8ae368c3bb8063c7a19fef80cb753d30e6a7ce, scoped Kraken
all-targets check exited 0, extraction check exited 0 (7341 symbols, zero updates/
additions/orphans), and all three stream_updates native tests passed. The original
three compiled mutation/restoration proofs remain bound to 8b4b5e67c5. Read-only
preservation review was clear at the full merge SHA. Independent Python 3.12
tomllib comparison against fetched integration measures **3 pending -> ported,
0 -> divergent**, totals **56 ported / 25 divergent / 52 pending**. This append
changes only tracked evidence; the scoped source/test comparison is repeated
before push.

## Websocket message dispatch and private controls

Implementation and gate SHA: **b8cb2da2c54bfcf4c38ef25d56b04a98542c1d94**.
Integration parent: **93ddd6b77b9ce509b8fa8ee77247a9fbf4dfc472**.
Independent Python 3.12 tomllib movement: **9 pending -> ported, 0 -> divergent**.
Totals: **65 ported / 25 divergent / 43 pending**, across 133 Kraken rows.

The nine rows are the four onMessage methods; Futures performAuth,
subscribePrivateFeeds, throttledFetchOrders and fetchThrottledOrders; and Spot
throttledFetchOrders. Bodies are src/message_dispatch.rs and src/private_control.rs.
Canonical boundaries are MANIFEST-SPEC.md, Kraken websocket message and private
control hosts. The witness is tests/message_control.rs with actual-source
AST-selected methods in tests/message-control-oracle.cjs and its corpus file.
The tolerant parser invokes the actual packages/json-parse source body.

Native values, UTF-16 text, numeric bits, field presence, original parsed/raw
message, request challenge and thrown-error identities are asserted before
wire-derived subscription checks. Hash construction precedes its lazy argument
read; remote signing and order fetch stop after one independently released
continuation. Signing arguments and current challenge reads, captured subscription
fields, explicit Error log severity, original timer callback selection, caught
failures and current loaded flags retain source ordering. Spot invokes at most
one captured refresh callback. Promise-internal then probes are excluded from
the method-property trace; assimilation remains a declared intrinsic boundary.

| Independent finite floor | Cases | Events | Propagated failures |
| --- | --- | --- | --- |
| Futures public dispatch | 42 | 262 | 1 |
| Futures private dispatch | 42 | 222 | 3 |
| Spot public dispatch | 42 | 269 | 1 |
| Spot private dispatch | 42 | 271 | 1 |
| Futures performAuth | 20 | 224 | 0 |
| Futures subscriptions | 8 | 59 | 0 |
| Futures throttle | 8 | 27 | 3 |
| Spot throttle/callback | 8 | 29 | 3 |
| Futures refresh | 8 | 25 | 2 |

| Gate at the full implementation SHA | Exit / result |
| --- | --- |
| explicit Yarn JS parity:extract writer | 0; zero updates/additions/orphans |
| Kraken cargo fmt -- --check | 0 |
| Kraken cargo check --all-targets --jobs 2 | 0 |
| explicit Yarn JS parity:extract:check | 0; 7341 symbols, zero updates/additions/orphans |
| cargo test --test message_control --jobs 2 | 0; 6 passed |
| scripts/message-control-controls.py, serial Cargo jobs 2 | 0; 8 defects, 9 selected named failures, 9 restored passes |
| read-only sub-agent review | clear at full b8cb2da2c54bfcf4c38ef25d56b04a98542c1d94 |
| git diff --exit-code HEAD over whole crate src/tests/scripts after controls | 0; restored bytes |

| Body / selected witness | Deliberate defect | Named native assertion | Mutated / restored exit |
| --- | --- | --- | --- |
| futures_public / dispatch | ticker invokes handleOHLC | DISPATCH native UTF16 | 101 / 0 |
| spot_message public / dispatch | ticker invokes handleOHLC | DISPATCH native UTF16 | 101 / 0 |
| spot_message private / dispatch | executions invokes handleBalances | DISPATCH native UTF16 | 101 / 0 |
| futures_private / dispatch | fills invokes handleBalances | DISPATCH native UTF16 | 101 / 0 |
| perform_auth / auth | request type becomes rest-auth | AUTH native UTF16 | 101 / 0 |
| subscribe_private_feeds / subscriptions | final feed becomes balances | SUBSCRIBE native UTF16 | 101 / 0 |
| throttle / futures_throttle_live_source | delay becomes 1001 | REFRESH native number | 101 / 0 |
| same held throttle defect / spot_throttle_live_source | delay remains 1001 | REFRESH native number | 101 / 0 |
| futures_fetch_throttled_orders / futures_refresh_live_source | loaded.orders becomes false | REFRESH native bool | 101 / 0 |

The shared delay defect is tested against separately selected Futures and Spot
witnesses, so the Futures assertion cannot prevent observing the Spot failure.
Earlier 7d87b1552c all-targets check exited 101 for a missed lazy-getter trait
signature; b8cb2da2c5 corrects it. The current measured compile/native gates above
supersede that failed compile. No mutation survived.

Not run: full crate/monorepo test suites, historical mutation matrices, workspace
compilation, host or wasm Clippy, wasm builds, CI reruns, credentialed calls,
real cryptography/signing sessions or live websocket/timer/Store integration.
JavaScript values/coercion/parsing, cryptographic/Buffer engines, remote signer,
actual timer callbacks and concrete Store operations remain declared host
contracts. Complete websocket classes and downstream handler/parser rows are
not claimed. This evidence-only append changes no measured gate inputs.

## Websocket control merge-down verification

Fresh integration **c5842fe5e72e2d3eb2a734a5c2ad1c82b92bab6e** merged cleanly
as **6a02c59ec2b9543be2abd1d1b6a3e4ddbd18ae47**. No conflict occurred.
Required git diff --exit-code from b8cb2da2c54bfcf4c38ef25d56b04a98542c1d94
through merged HEAD over the whole Kraken src/tests trees exited **0**.
Separate controls, Kraken manifests, all safe-cex TypeScript, json-parse/logging,
types/num/exchange/Store/util crates and workspace/venue Cargo.toml comparison
also exited **0**. This is the source/test byte-unchanged case; the original
mutation outcomes remain bound to the implementation gate SHA.

The transitive transport dependency did change: new modules and DirectTransport
constant/accessor additions. The lockfile adds an exchange dependency to Blofin.
The entire dependency graph is therefore not claimed byte-unchanged. Preservation
review found no effect on the declared control-body host contracts. Scoped Kraken
all-targets check, extraction check (7341 symbols, zero updates/additions/orphans),
and all six message_control native tests were rerun at full merge SHA
6a02c59ec2b9543be2abd1d1b6a3e4ddbd18ae47 and exited **0**. Body/witness/control
preservation review was clear at that SHA. No transport engine integration claim
is added by these reruns.

Python 3.12 tomllib comparison against fetched integration still measures
**9 pending -> ported, 0 -> divergent**, with **65 ported / 25 divergent /
43 pending**. This evidence-only append changes no measured gate inputs.

## Websocket control delivery preservation

Final fresh integration **65e086a3c9039a79e4c7e9f354aba31d8f83009e** merged
cleanly as **2b05c4aebb4be575067ac83d7e30a30b059977ef**. Required whole Kraken
src/tests comparison against gate b8cb2da2c54bfcf4c38ef25d56b04a98542c1d94
exited **0**. The comparison from 6a02c59ec2b9543be2abd1d1b6a3e4ddbd18ae47
over controls, Kraken manifests, all safe-cex/json-parse/logging source,
types/num/exchange/Store/util/transport crates and workspace/venue Cargo/lock
inputs also exited **0**. Incoming changes are Woo code, witness/evidence and
manifest only. No further compiled gate inputs changed.

Extraction was rerun at full delivery merge SHA
2b05c4aebb4be575067ac83d7e30a30b059977ef: **exit 0**, 7341 symbols, zero
updates/additions/orphans. Scoped compile/native exits remain the measured zeros
at 6a02c59ec2b9543be2abd1d1b6a3e4ddbd18ae47; mutation outcomes remain bound to
b8cb2da2c54bfcf4c38ef25d56b04a98542c1d94 with preserved bodies and witnesses.
Python 3.12 tomllib against final fetched integration again measures
**9 pending -> ported, 0 -> divergent**, totals **65 / 25 / 43**. This append
changes evidence only; source/test preservation is repeated immediately before
push. The non-claims listed above remain unchanged.

## Public order-book and listener bodies

Implementation and gate SHA: **c8bfb07683c1dda9211933e587b060be5268a12c**.
Integration parent: **f0cf136efbc17b93fe52369e71342e620dd925ff**.
Python 3.12 tomllib movement: **9 pending -> ported, 0 -> divergent**.
Kraken totals: **74 ported / 25 divergent / 34 pending**, across 133 rows.

Rows: Futures and Spot handleBookSnapshot, handleBookDelta and emitOrderBook;
both listenOrderBook methods; and Spot listenOHLCV. Targets are orderbooks.rs
and book_listeners.rs. Canonical boundaries are MANIFEST-SPEC.md, Kraken
order-book and listener hosts. Actual-source witnesses are tests/orderbooks.rs
and orderbook-oracle.cjs/orderbook-corpus.cjs, including the original get-key
initializer with installed lodash. Prepared native frame values, cache operations,
UTF-16 keys, numeric bits/NaN and original object/callback/error identities are
asserted before wire-derived checks.

Each callback gets a fresh outer book, shared bids/asks arrays and separately
allocated shared mutable level objects. Paired emit witnesses retain/remove a
level, mutate it in the second callback and inspect it after return. This fixes
the concrete per-level identity gap identified by review at bbd4cb087a43a5dc5d7da357d1965fd378c3a749.
That SHA's partial control run was interrupted (130), and restored src/tests
comparison exited 0. None of its partial outcomes count toward this gate sheet;
all seven defects were rerun to completion at the implementation SHA above.

| Independent finite floor | Cases | Events | Propagated failures |
| --- | --- | --- | --- |
| Futures snapshot | 17 | 332 | 5 |
| Spot snapshot | 17 | 328 | 5 |
| Futures delta | 25 | 252 | 5 |
| Spot delta | 25 | 530 | 6 |
| Futures emission | 21 | 129 | 3 |
| Spot emission | 21 | 114 | 2 |
| Futures book listener | 18 | 153 | 1 |
| Spot book listener | 18 | 199 | 3 |
| Spot OHLC listener | 18 | 260 | 3 |

The 180-case floor includes 41 independently fixed checkpoints per listener
family. Timer enumeration stops after at most two captured callbacks; disposer
invocation after at most two calls; forEach after its captured initial length.
The depth boundary uses 103 bid and 102 ask entries, asserting the native first
100 after sorting. General Map/SameValueZero/sort/array engines, coercion,
JSON.stringify, timer scheduling and current handler/cache storage are bound
host contracts; their concrete runtime implementations are not newly claimed.

| Gate at the full implementation SHA | Exit / result |
| --- | --- |
| explicit Yarn JS parity:extract writer | 0; zero updates/additions/orphans |
| Kraken cargo fmt -- --check | 0 |
| Kraken cargo check --all-targets --jobs 2 | 0 |
| explicit Yarn JS parity:extract:check | 0; 7341 symbols, zero updates/additions/orphans |
| cargo test --test orderbooks --jobs 2 | 0; 9 passed |
| scripts/orderbook-controls.py, serial Cargo jobs 2 | 0; 7 defects, 9 selected named failures, 9 restored passes |
| read-only sub-agent review | clear at c8bfb07683c1dda9211933e587b060be5268a12c |
| git diff --exit-code HEAD over crate src/tests/scripts after controls | 0; restored bytes |

| Defect / independently selected witness | Named native assertion | Mutated / restored exit |
| --- | --- | --- |
| snapshot qty filter becomes <= 20 / Futures snapshot | BOOK native event | 101 / 0 |
| same held filter defect / Spot snapshot | BOOK native event | 101 / 0 |
| Futures delete predicate becomes qty == 1 / Futures delta | BOOK native event | 101 / 0 |
| shared level delete predicate becomes qty == 1 / Spot delta | BOOK native event | 101 / 0 |
| depth limit becomes 99 / Futures emission | BOOK native array length | 101 / 0 |
| same held depth defect / Spot emission | BOOK native array length | 101 / 0 |
| initial poll delay becomes 101 / Futures listener | LISTEN native number | 101 / 0 |
| initial poll delay becomes 101 / Spot listener | LISTEN native number | 101 / 0 |
| OHLC frame channel becomes book / Spot OHLC listener | LISTEN native UTF16 | 101 / 0 |

Shared mutants require separately selected venue runs before restoration; a
failure in the first venue cannot hide the second. No mutation survived.
Not run: full crate/monorepo suites, historical mutation matrices, workspace
compilation, host/wasm Clippy, wasm builds, CI reruns, credentialed calls or live
websocket/timer/Store integration. Complete websocket classes, general intrinsic
engines and a native JSON serializer remain separate contracts/rows. This
tracked evidence-only append changes no measured gate inputs.

## Order-book merge-down verification

Fresh integration **c1b0ce61f2247f5604b26ed5a00b142be75d90ef** merged cleanly
as **2f82e3727a3eac6bd516071a9d48c53338f4aa55**. No conflict occurred.
Required whole Kraken src/tests comparison from gate
c8bfb07683c1dda9211933e587b060be5268a12c through merged HEAD exited **0**.
Separate controls, Kraken mappings, all safe-cex/json-parse/logging TypeScript,
types/num/exchange/Store/util/transport and workspace/venue Cargo.toml comparison
also exited **0**. Clock/polling/candle-cache/crypto source has no changed paths.
This is the source/test byte-unchanged case; native and mutation exits retain
their original full implementation SHA binding.

The only lockfile change adds candle-cache to Apexomni, leaving Kraken's graph
unchanged. Incoming other-venue manifests/exempt inputs require extraction to be
rerun. At full merge SHA 2f82e3727a3eac6bd516071a9d48c53338f4aa55, scoped Kraken
all-targets check exited **0** and extraction check exited **0** (7341 symbols,
zero updates/additions/orphans). Read-only preservation review was clear at
that SHA, including the corrected per-level graph. Python 3.12 tomllib against
fetched integration measures **9 pending -> ported, 0 -> divergent**, totals
**74 ported / 25 divergent / 34 pending**. This evidence-only append changes no
measured gate inputs.

## Order-book delivery preservation

Final fresh integration **dc74077e6165f04841de6a6bbd9d73c55afc58db** merged
cleanly as **81f9887de029df9b361b78f265ed9ccb65d51b78**. Required whole Kraken
src/tests comparison against c8bfb07683c1dda9211933e587b060be5268a12c exited **0**.
Comparison from 2f82e3727a3eac6bd516071a9d48c53338f4aa55 over controls, Kraken
mappings, all safe-cex/json-parse/logging source, shared runtime/num/types/Store/
transport/util/candle-cache/clock/polling/crypto crates and workspace/venue Cargo/
lock inputs also exited **0**. Incoming changes affect Paradex only; no relevant
compiled input changed. Native/mutation exits remain bound to the implementation
SHA and compile exit to the reviewed merge SHA.

At full delivery merge SHA 81f9887de029df9b361b78f265ed9ccb65d51b78, extraction
was rerun: **exit 0**, 7341 symbols, zero updates/additions/orphans. Python 3.12
tomllib against final fetched integration measures **9 pending -> ported,
0 -> divergent**, totals **74 / 25 / 34**. This append changes evidence only;
the required source/test preservation diff is repeated before push. Non-claims
listed above remain unchanged.

## REST account reads — eight pending rows

Implementation and gate SHA: **7e5f53d1a9b2bc6f4cdae6b38792776f071b7bdb**.
Parent: **85f3b9e596582fce3738807c4d9b7032c5b5a802**, the merged order-book
slice. No unreviewed feature ancestor. Python 3.12 tomllib over the Kraken
manifests against that parent measures **8 pending -> ported, 0 -> divergent**:
**74/25/34 -> 82/25/26** (ported/divergent/pending, 133 rows).

The new rows are Futures/Spot fetchMarkets, Spot fetchTickers/fetchOrders,
Futures fetchPositions/fetchBalance/fetchHistoricalFills, and Spot refreshBalance.
ReadHost boundaries and finite scope are documented in
`sdk-rs/parity/MANIFEST-SPEC.md#kraken-rest-account-read-hosts`.
Read-only review is clear at the full implementation SHA. Two findings were
corrected before gating: construct the refresh loaded patch in production, and
retain the source insertion order of flex currency value/collateral/info fields.

Measured at the implementation SHA:

| Gate | Exit / result |
| --- | --- |
| `/opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kraken --all-targets --jobs 2` | 0 |
| Cargo fmt for this crate with `-- --check` | 0 |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract` (WRITER) | 0; 7341 symbols, zero updates/additions/orphans |
| Same explicit Yarn binary, `parity:extract:check` | 0; 7341 symbols, zero updates/additions/orphans |
| `python3.12 sdk-rs/safe-cex-venue-kraken/scripts/rest-account-controls.py` | 0; compiled mutant Cargo exit 101, restored Cargo exit 0; original bytes restored |
| `git diff --exit-code 7e5f53d1a9b2bc6f4cdae6b38792776f071b7bdb -- sdk-rs/safe-cex-venue-kraken/src sdk-rs/safe-cex-venue-kraken/tests` after controls | 0 |

Both controlled Cargo runs select only `--test rest_account --jobs 2`, with
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Seven defects in disjoint production functions compile together. Each of the
eight named family tests sees its corresponding defect; the shared limits defect
is checked by separate Futures and Spot tests. All eight named native assertions
fail in the single compiled mutant run (101), and all eight tests pass in the
single restored run (0). No independent-per-defect process claim is made.

| Production defect | Named native test / assertion |
| --- | --- |
| shared market amount maximum reduced by one | futures_markets_native / native bits |
| same shared market limit | spot_markets_native / native bits |
| Spot mark becomes last + 1 | spot_tickers_native / native bits |
| remaining adds filled instead of subtracting | spot_orders_native / native bits |
| cached WS notional becomes zero | futures_positions_native / native bits |
| flex free margin increased by one | futures_balance_native / native bits |
| refresh patch sets balance:false | spot_refresh_native / boolean |
| fills cursor becomes oldest + 2 ms | futures_fills_native / UTF16 |

The actual-source witness covers **97 cases**: 13/12/11/12/13/14/5/17 in the
row order above, with **194 request/trace/emit/update/warning observations**.
Native recursive assertions check numeric bits/NaN, UTF-16 values, own-key order,
current Store fallback identity, original market info and original errors,
pre-await request prefix, patches and final preference state. The fills witness
includes the source's **50-page** stopping floor, original unwindowed first
request, advancing cursor, duplicates, window filtering, limit, disposal, rejected
reads and the explicit incomplete-window warning. These are finite floors.

Not run: full crate or monorepo suites, old mutation matrices, workspace build,
host or wasm Clippy, wasm builds, CI reruns, credentialed REST calls, live Store
or transport integration, arbitrary sparse/reentrant arrays or exhaustive JS
intrinsic tests. Complete exchange/API factories remain separate rows. No shared
type or trait was changed. This handwritten tracked append changes evidence only.

## REST account read delivery preservation

Fresh integration **9d7e0523cd16502a66901c3bf264e222bc5081ba** merged cleanly
as **9acf9dd0695f5e807b62480b6a07069a85e4ea70**. Required standalone
`git diff --exit-code 7e5f53d1a9b2bc6f4cdae6b38792776f071b7bdb HEAD -- sdk-rs/safe-cex-venue-kraken/src sdk-rs/safe-cex-venue-kraken/tests`
exited **0**. The same standalone diff over controls, Kraken manifests,
workspace/venue Cargo and lock inputs, shared types/num/exchange/Store/transport/
crypto/candle-cache/util/clock/polling paths, all safe-cex/json-parse/logging source,
and extractor source also exited **0**. These gate inputs are byte-unchanged;
original native, mutation, format and compile exits retain the implementation
SHA binding. No affected native/compile gate requires rerunning.

Incoming other-venue manifests required extraction to be repeated. At full merge
SHA 9acf9dd0695f5e807b62480b6a07069a85e4ea70, explicit Yarn
`parity:extract:check` exited **0**, 7341 symbols, zero updates/additions/orphans.
Python 3.12 tomllib against the fetched integration measures **8 pending ->
ported, 0 -> divergent**, totals **82 ported / 25 divergent / 26 pending**.
This tracked evidence-only append changes no measured input. The required whole
Kraken source/tests preservation diff is repeated on the delivery commit before
push. The explicit non-claims above continue to apply.

## Trading REST and API factories — nine pending rows

Implementation: **5ecc911aa65926bc8ab53f60d0cd39ffd4f7667a**.
Reviewed gate/control SHA: **70ef7c89af45975e6ecfbd1b6d830edb773a33bc**.
Parent: **b5d642aea03ced39a3cc27bb95767b8fd242c3b7**. No unreviewed feature
ancestor. Python 3.12 tomllib against that parent measures **9 pending -> ported,
0 -> divergent**, **82/25/26 -> 91/25/17** (ported/divergent/pending, 133 rows).

Rows: Futures/Spot placeOrder and updateOrder; Futures cancelOrders, setLeverage
and changeMarginType; Futures/Spot createAPI. Canonical host/intrinsic boundaries:
`sdk-rs/parity/MANIFEST-SPEC.md#kraken-trading-rest-and-api-hosts`.
Read-only review is clear at the gate SHA. Corrections before gating preserve
direct symbol.toUpperCase refusal, and execute the actual trigger-parser
initializer, including inherited-name provenance refusal before cancellation.

Measured at gate SHA 70ef7c89af45975e6ecfbd1b6d830edb773a33bc:

| Gate | Exit / result |
| --- | --- |
| `/opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kraken --all-targets --jobs 2` | 0 |
| Cargo fmt for this crate with `-- --check` | 0 |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract` (WRITER) | 0; 7341 symbols, zero updates/additions/orphans |
| Same explicit Yarn binary, `parity:extract:check` after restoration | 0; 7341 symbols, zero updates/additions/orphans |
| `python3.12 sdk-rs/safe-cex-venue-kraken/scripts/trading-controls.py` | 0; compiled mutant Cargo 101, restored Cargo 0 |
| Standalone `git diff --exit-code 70ef7c89af45975e6ecfbd1b6d830edb773a33bc -- sdk-rs/safe-cex-venue-kraken/src sdk-rs/safe-cex-venue-kraken/tests` after controls | 0 |

Controlled Cargo uses only `--test trading --jobs 2` and
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Eight production defects compile together, in disjoint trading families plus the
shared API request body. Every one of nine named native assertions fails in the
single compiled mutant run (101). Every test passes in the single restored run
(0), and both production files restore byte-for-byte. This is not a claim of
nine separate Cargo processes. No mutation survived.

| Production defect | Named native assertion |
| --- | --- |
| Futures post-only orderType becomes ioc | futures_place_native / native UTF16 |
| Spot volume increased by one | spot_place_native / native UTF16 |
| Futures size amendment uses price precision | futures_update_native / native bits |
| Spot amendment returns amend_id instead of canonical txid | spot_update_native / native UTF16 |
| single cancellation sends orderId instead of order_id | futures_cancel_native / native own-key order |
| clamped leverage increased by one | leverage_native / native bits |
| isolated margin preference becomes cross | margin_native / native UTF16 |
| API nonce timestamp multiplier becomes 1001 | futures_api_native / native UTF16 |
| same shared API defect, separate Spot selection | spot_api_native / native UTF16 |

Initial control attempt at 5ecc911aa65926bc8ab53f60d0cd39ffd4f7667a stopped at
compilation: replacing min's only caller made that helper unused (Cargo 101,
script 1). It reached no native assertions and is not counted as discrimination.
The script restored both original files; standalone source/tests diff exited 0.
The gate commit changes only that mutation to an additive leverage defect.

Native/source witness: **228 finite cases**, with **1031 recorded constructor,
request, helper, emitter, log and parent-call observations**. Family floors are
37 placement Futures / 29 placement Spot / 21 amendment Futures / 24 amendment
Spot / 12 cancellation / 18 leverage / 20 margin / 33 API Futures / 34 API Spot.
Assertions check native numeric bits/NaN, UTF-16 values, own-key order, original
options/error/response/cause identity, pre-await order, current cache state,
protective/trailing guards, acknowledgement checks and shared versus per-client
nonce materials. Actual source methods and trigger/get-key/adjust helpers execute;
encoding/Buffer/crypto bindings record original arguments with sentinels.

Not run: full crate/monorepo suites, old mutation matrices, workspace compilation,
host/wasm Clippy, wasm builds, CI reruns, credentialed trading or live signing/
transport/Store integration, third-party encoding/crypto or exhaustive intrinsic/
sparse/reentrant-array tests. No valid-live-signature or complete exchange-class
claim. No shared type or trait changed. This tracked handwritten append changes
only evidence.

## Trading REST delivery preservation

Fresh integration **61102ead04c728512ff39ee6ae5cf952e82bf2ad** merged cleanly
as **28e98dac593a1ebbd55de62cd76f6f826f1d352f**. Required standalone
`git diff --exit-code 70ef7c89af45975e6ecfbd1b6d830edb773a33bc HEAD -- sdk-rs/safe-cex-venue-kraken/src sdk-rs/safe-cex-venue-kraken/tests`
exited **0**. Separate standalone comparison over controls, Kraken mappings,
workspace/venue Cargo manifests, all shared types/num/exchange/Store/transport/
crypto/candle-cache/util/clock/polling paths, safe-cex/json-parse/logging/math
TypeScript, extractor source and root package/Yarn inputs also exited **0**.
Native/mutation and formatting inputs are byte-unchanged; their measured exits
retain the original gate-SHA binding.

The lockfile differs only by adding indexmap and safe-cex-crypto to OKX's
existing package dependencies. No Kraken dependency source or version changed.
At full merge SHA 28e98dac593a1ebbd55de62cd76f6f826f1d352f, scoped Kraken
all-targets Cargo check was nevertheless rerun: **exit 0**. Incoming other-venue
manifests required extraction to be rerun: **exit 0**, 7341 symbols, zero
updates/additions/orphans. Native controls were not repeated for unrelated OKX
lock metadata.

Python 3.12 tomllib against the fetched integration measures **9 pending ->
ported, 0 -> divergent**, totals **91 ported / 25 divergent / 17 pending**.
This evidence-only append changes no gate input. Required whole-crate source/
tests preservation is repeated on the delivery commit before push. Explicit
non-claims above remain applicable.

# Kraken private account feeds and income wrappers

Gate/review snapshot: `48515747546d9faef729b8af2934fca526b19370`.
Implementation commit: `a59a802e4d39dbb512a0c40be66ed37396a4a578`.
Fresh branch: `feat/rust-venue-kraken-private-accounts`, directly off integration
`2a24bb3c48b25e781167071443dd603b0d0d2b8e`. The follow-up changes only canonical
pager boundaries, row notes and unused oracle imports; Rust source and witnesses
remain unchanged. No unmerged feature branch is an implementation ancestor.

Python 3.12 `tomllib` measured **8 pending -> ported, 0 -> divergent** against
that integration parent: **91 ported / 25 divergent / 17 pending** becomes
**99 ported / 25 divergent / 9 pending**, 133 rows total.

| Source row | Rust body | Independent finite case floor |
| --- | --- | ---: |
| Futures private `handleFills` | `private_accounts.rs::futures_fills` | 18 |
| Futures private `handleOpenOrders` | `private_accounts.rs::futures_open_orders` | 23 |
| Futures private `handleOpenPositions` | `private_accounts.rs::futures_open_positions` | 14 |
| Futures private `handleBalances` | `private_accounts.rs::futures_balances` | 15 |
| Spot private `parseOrder` | `private_accounts.rs::spot_parse_order` | 15 |
| Spot private `handleExecutions` | `private_accounts.rs::spot_executions` | 15 |
| Futures `fetchIncomeHistoryRange` | `private_accounts.rs::income_range` | 14 |
| Futures `fetchIncomeHistory` | `private_accounts.rs::income_history` | 15 |

The 129-case enumeration stops at these named fixture lists, with at most 20
native continuation polls per case. Actual TypeScript arrow bodies are selected
by AST; tests check native fields, numeric bits/NaN, UTF-16, own-key order,
original info/error allocations, cache mutations and ordered effects before any
wire claim. Store and option changes exercise current reads between calls and
after await. Signed-zero filled quantities, numeric/string Date construction,
malformed rows, callback overrides and synchronous/asynchronous errors are in
the floor. Custom getters/prototypes, sparse arrays, networking and a concrete
websocket/exchange runtime are outside this witness.

The income wrappers bind a controlled imported pager dependency. The existing
shared native `paginate_income_range` has documented CHOSEN divergence at its
typed owned record/options, UTF-8 and snapshot boundaries. This slice adds no
pager and proves no end-to-end native pager assembly or live ledger walk. The
nine remaining rows are six concrete class definitions, two exchange
constructors and Futures exchange OHLCV polling.

All commands ran from this worktree root. Cargo is
`/opt/homebrew/opt/rustup/bin/cargo`, one Cargo process at a time, `--jobs 2`.
Native tests/controls used
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Gate at the gate/review snapshot | Exit and measured result |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kraken --all-targets --jobs 2` | 0 |
| `cargo fmt --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kraken -- --check` | 0 |
| `python3.12 sdk-rs/safe-cex-venue-kraken/scripts/private-account-controls.py` | 0; all eight defects compiled together, eight named native assertions failed with Cargo exit 101; exact source restoration, eight named passes with Cargo exit 0 |
| Scoped `cargo test ... --test private_accounts --jobs 2` | 0 in the control runner's restored invocation; all eight tests, 129 cases |
| Explicit Yarn `parity:extract` writer | 0; 7341 symbols / 512 files, updated 0 manifests, added 0, orphaned 0; rust_files derived programmatically before writer |
| Explicit Yarn `parity:extract:check` | 0; would update 0 manifests, added 0, orphaned 0 |

Yarn commands used
`node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js <script>`.
The eight compiled defects are altered filled quantity, order TIF, position
notional, balance free margin, Spot parser TIF, Spot execution quantity priority,
pager page limit and default history window. Each requires its own named native
failure, not a compiler error or an aggregate failure alone; no survivors.
Read-only sub-agent review is clear at the gate/review snapshot after the shared
pager boundary clarification.

Not run: full Kraken test suite/older scoped test targets, workspace check or
tests, host/wasm Clippy, wasm builds, TypeScript/monorepo suites, CI reruns,
browser/live exchange tests, signing/network or live pager completeness checks.
No shared SDK type or trait was changed.

## Private-account merge preservation

Fetched integration `97b6f919d26396b52e4b3514f98fcfa34b4dcc6c` and merged it down
as `b9878cd6e5469d872d5fbc7422db4005fef9f043`, with no conflicts.
The mandatory standalone command

```
git diff --exit-code 48515747546d9faef729b8af2934fca526b19370 HEAD -- sdk-rs/safe-cex-venue-kraken/src sdk-rs/safe-cex-venue-kraken/tests
```

returned **0**, proving byte-unchanged production and native/oracle witnesses.
A separate scoped diff returned **0** for controls, Kraken manifests, canonical
notes, shared type/num/exchange/store/transport/crypto/candle-cache/util inputs,
workspace and venue Cargo files/lock, extractor code, Yarn lock and relevant
Kraken/utility/type/math/logging TypeScript inputs. The only root package-script
change adds a KuCoin test target; both parity extractor commands are unchanged.
Other venues' manifests and parity exemptions changed, so extractor check was
rerun at the merge SHA: **0**, 7341 symbols / 512 files, would update 0 manifests,
added 0, orphaned 0. Native, mutation, fmt and scoped check exits at the gate SHA
still cover the unchanged inputs; those Cargo gates were not rerun after merge.

`tomllib` against the fetched integration again measures **8 pending -> ported,
0 -> divergent**, with the same **99 / 25 / 9** Kraken census. This preservation
record is evidence-only; source/tests are rechecked standalone before push.

## Last Kraken rows: class factories, constructors and OHLCV polling

Parent: `5911167261853a8cbcc99d79cf45b785bb016648`. Gate/review snapshot:
`4b2c87b21ec04afba47328bb56a3564d0510d232`. `tomllib` against that parent
measures **3 pending -> ported, 6 -> CHOSEN divergent**; census
**102 ported / 31 divergent / 0 pending**, 133 total. The six factories require
complete external callable assembly rather than shipped fixed class-body links.
Each class row has its own construction difference and canonical note.

Measured gates at the snapshot:

- Scoped composition target: **0**, nine named tests, 59 finite cases
  (27 class, 16 constructor, 16 polling). Native identities, ordered fields,
  numeric bits, UTF-16 and original errors precede wire observations.
- `python3.12 scripts/composition-controls.py`: **0**. Three compiled family
  defects produce **101** with all nine required named native assertions, then
  exact source restoration produces **0** for all nine. No survivors.
- Scoped `cargo check -p safe-cex-venue-kraken --all-targets --jobs 2`: **0**.
- Scoped `cargo fmt -p safe-cex-venue-kraken --check`: **0**.
- Explicit Yarn `parity:extract` WRITER: **0**, updated 0; extractor check:
  **0**, 7341 symbols / 512 files, would update 0, added 0, orphaned 0.
- Read-only sub-agent review: clear at the full gate snapshot after the paired
  immediate-task-rejection case. Cleanup remains available and rejection visible
  out of band even when the first poll fails before suspension.

Cargo commands used `/opt/homebrew/opt/rustup/bin/cargo`; scoped tests used
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Yarn used `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js`.

Not run: full venue suite/older test targets, workspace check/tests, host/wasm
Clippy, wasm build, TypeScript/monorepo suites, CI reruns, browser/live exchange
tests. No shared SDK type/trait changed. No fixed production class assembly,
JavaScript heap/GC, general scheduling, fresh API/nonce/signing integration or
live network claim. Constructor imports and unrelated class member bodies are
controlled dependencies within the finite floor.

## Last-row merge preservation

Integration `95b56a79bc646156e1812b17428e0fb6295413a5` merged down as
`a8e70926cab64d691c791ac5dc23cb8a92d370b4`, no conflicts. Standalone
`git diff --exit-code 4b2c87b21ec04afba47328bb56a3564d0510d232 HEAD -- sdk-rs/safe-cex-venue-kraken/src sdk-rs/safe-cex-venue-kraken/tests`
returned **0**. Additional input scope (controls, Kraken manifests/canonical,
SDK dependencies, Cargo/lock, extractor/Yarn/package scripts and relevant
TypeScript) has no changed paths. Scoped Cargo/mutation exits therefore stand.
Other venue manifests changed; extractor check at merge SHA returned **0**,
7341/512, would update 0, added 0, orphaned 0. This is evidence-only.

## Native hosted signer integration (2026-10-17)

`safe-cex-registry/tests/native_host_kraken_signer.rs` runs three local native
HTTP/WS witnesses. Both Kraken names construct with empty local secrets and
validate accounts through the shared hosted signer, preserving account/name,
returned signed wire headers and Spot form bytes, exact balance and read purity.
The Futures socket asks for a challenge before subscribing; after remote signing
all four private feeds retain the key, original challenge and returned signature.
The request pins hosted account, toSign and kraken-challenge output format.

With only the HTTP-host wiring fixed, the WS witness failed101: accountName was
null instead of hosted-kraken-account. `AuthHost::read(remote)` now reads the live
parent options injected by shared #151, under its existing capability guard.
The three native witnesses pass after that fix.

`python3 safe-cex-registry/scripts/kraken-hosted-controls.py` runs two permanent
compiling controls from sdk-rs. Omitting the native signer fails the HTTP tests;
reading a missing live signer property fails the WS witness. Each restored tree
passes all three native witnesses. The controls ran sequentially with one Cargo
job. Read-only sub-agent review reported no blocking findings.


## Accepted base-URL string union (2026-10-17)

Source `kraken.types.ts::KrakenBaseUrl` declares exactly `derivatives | history`
as strings. The original derived Rust decoder compiled and passed its valid
native-variant witness, but failed the new named tagged-shape test by admitting
`{"derivatives":null}`. The production fix uses existing `String::deserialize`
and a private remote Serde schema. Serialize spellings and numeric-output
registration remain unchanged.

The two new native tests pass alongside all four existing actual-source `types`
tests, with zero ignored. Both hardcoded native variants are checked before
Serialize through five ingress paths. Each of the two valid tagged-null maps,
five other non-string shapes and four invalid tags is refused through the same
five paths (55 refusal assertions). Existing `types` independently checks the
complete two-literal source union, as well as tables and normalization.

`scripts/base-url-string-control.py` ran successfully: baseline six tests pass;
remove only String admission and the compiled named
`tagged_base_url_shapes_are_rejected_on_every_ingress_path` fails on the native
owned tagged-map assertion; restore exact source bytes and all six pass again.
A compiler error cannot satisfy this control.

`scripts/base-url-contracts.mjs` ran fresh native/default private/hidden rustdoc,
then the maintained selected-owner production probe: **one actual runtime
result, zero classification-only receipts, zero failed owners**. Its hand-authored
observation matches actual native enum variants without serializing the enum,
and records all 11 refusals independently. Compiler identity confirms the private
remote schema has no independent Deserialize or public inherent API. The new
per-crate contract contains this owner only; no shared policy row is exempted.

The full inventory attempt still refused **25 unsupported Kraken receiver
methods**. This is open review work, not a passing gate. The selected owner does
not discharge other owners, aliases, local functions, construction or feature/
wasm accepted coverage. The old seven-file `types-parity-check.py` wrapper crashed on list-valued
mappings before indexing. It now preserves complete split targets and notes,
uses every committed Kraken manifest, and intersects only existing provenance
declarations. All Kraken declarations, including unused ones that must fail,
remain present. A mixed-crate split retains its already-declared external file;
a missing declaration is never manufactured.

The maintained checker builds the complete workspace index for canonical shared
reexport resolution. Its explicit `--invention-crate` scope narrows only public
invention obligations, with explicit manifest/exemption inputs and every target
resolution record intact. Global/default behavior is unchanged; report/progress
refuse this scope. **73 existing Rust gates and three new CLI regressions pass**,
with zero ignored. The new fixture proves list-valued cross-crate resolution,
shared reexports, default global rejection, scoped unmapped-item rejection,
missing-provenance rejection, exact restoration and invalid-scope refusal.

Two Python regressions pass. Reverting only scalar/list selection reproduces the
original AttributeError in the named split regression; exact restoration passes
both again. `scripts/manifest-controls.py` also ran against the actual Kraken API
manifest: baseline passes, removing its committed rust_files declaration fails
the named source-provenance gate, and exact restoration passes. No permanent
manifest mapping or exemption changed.

The maintained global gate previously passed **7459 rows / 17204 public Rust
items**; the seven-file source extractor independently passed **83 symbols**.
After integrating master f3691e05a6, the repaired Kraken wrapper passes
**134 rows / 539 public Rust items** with no exemptions, and the freshly built
global checker passes **7459 rows / 17204 public Rust items**. These gates
validate metadata claims, not behavioral parity.

Read-only independent review found no actionable issue in the production fix,
native fixture, selected probe, contract or mutation/restoration controller.


Full scoped Kraken Cargo tests passed **141 tests, zero failures and zero ignored**
across 28 target receipts including the empty doctest target. Separate native and
wasm32-unknown-unknown all-targets Clippy runs passed with warnings denied. The
changed parity tooling also passed native all-targets Clippy with warnings denied.
These compile checks do not extend the one-owner native/default accepted proof
to wasm behavior or other accepted owners.


Integration `354acaa8b28435d975b2051303581a21198dc9cc` preserves the exact
validated Kraken source/tests, parity CLI source/tests, Cargo manifests/lock,
shared native dependencies and Kraken TypeScript oracle inputs. The full native
and host/wasm Clippy receipts therefore remain applicable without repeating
unchanged tests. Frozen installation and all-workspace formatting pass after
integration; the selected production probe is rerun independently.


### Master conflict refresh

Master `f9b0ba6ec662c5c2ae084305751ec91a2c702ae3` integrated in
`7e4ca5c5490fea969a872576653fdefac13f731d`. The sole conflict joined two appended
parity-tool documentation sections: the complete master GRVT borrowed-map recipe
and the scoped-checker guidance are both retained. The latter now sits beside
structural-gate documentation, preserving the master append point. The CLI Rust
source and its test fixture are byte-identical to the reviewed `2333c92883`.

Master also changed the candle-cache dependency. Fresh full scoped Kraken tests
pass **141/0 failures/0 ignored**, and fresh native plus wasm all-targets Clippy
pass with warnings denied. Fresh scoped/global structural checks pass
**134/539** and **7459/17204** respectively. Frozen installation and all-workspace
formatting pass. The base-URL decoder, witness and guard-removal control remain
unchanged; their source/dependency inputs for string admission are preserved.

The fresh integrated production enum probe also passes: one actual runtime
result, zero classification-only receipts, zero failed owners.

## Four accepted module-helper bodies (2026-10-17)

Each complete body was reviewed against the actual TS declarations and the
existing module-utility host profile in MANIFEST-SPEC. `parse_trigger_signal`
decodes JSON-domain values to native trigger variants/absence or a coercion error.
The other three functions have distinct non-decoder roles: typed literal string
comparison through the shared root collator; shared Error construction with
literal message and separate optional metadata snapshots; and a module-owned ID
generator factory driven by fallible host entropy. The factory's mutable byte
slice belongs to the host fill callback, rather than an inbound wire schema.
The binding factory and SpotIds alias remain separately reviewable owners.

Four new direct native witnesses pass with all five existing actual-source helper
tests: **nine passed, zero failures/ignored**. Observations include all three
trigger variants and absence/error before Serialize; actual Ordering; literal
message/name, separate nested response/cause fields, negative-zero bits, explicit
null versus absence and snapshot behavior; and an entropy failure followed by
native 32-character IDs and the independent source refill schedule. The installed
nanoid 6.0.1 source was reviewed directly: growth advances before entropy filling,
and constant 0xaf maps to hexadecimal f. The existing live source witness compares
2640 generated results and callback schedules with the pinned dependency.

The selected native/default production gate reports **one runtime result, three
explicit classifications, zero failed owners**. Its 21 trigger input/output cases
were authored independently of the Rust decoder. The call observes actual native
enum variants/absence/error rather than serializing the enum or observing is_ok.
A classification receipt does not execute the comparator, error constructor or
factory; their separate native witnesses provide that execution evidence.

`scripts/helper-contract-controls.py` completed four sequential compiled controls:
reverse comparator operands, swap response/cause metadata, change the hexadecimal
mask from 15 to 7, and map spot to the wrong trigger variant. The first three each
fail their named native witness after compilation, then pass that same witness
after exact source restoration. The trigger mutation fails only its own actual
production output probe with native output mismatches; restored production1 plus
classifications3 pass again. Exact final helpers.rs bytes match the original,
and all nine native/source tests pass again with zero ignored. Compiler failures
cannot count as control success. No production code or protocol boundary changed.

These four contracts leave other Kraken owners, aliases, local functions,
construction, the 25 receiver refusals and feature/wasm accepted coverage open.


Helper readiness at integration `44c962414960992337f5c745f762edf35281686a`:
full scoped Kraken tests passed **143**, with zero failures or ignored tests;
separate native and wasm32 all-targets Clippy passed with warnings denied.
Fresh production helper verification passed **one runtime result, three explicit
classifications, zero failed owners**. Global structural verification passed
**7459 rows / 17206 public Rust items**; the maintained TS extractor checked
**83 symbols across seven slice files**. Workspace format and frozen install
passed. Independent read-only review found no actionable issues; CodeRabbit's
manual review was rate-limited. No unresolved review conversations were present
at this checkpoint.

The final master integration changed only other packages' accepted contracts and
their review docs. Kraken source/tests/controls/contracts, all Cargo inputs and
the maintained probe/indexer sources are byte-identical to the tested checkpoint.
At this original checkpoint the base-URL/scoped-wrapper repair was separate in
PR #327. The later master integration below includes it without duplicating its
implementation. These are selected native/default accepted-contract
receipts; the successful wasm Clippy check does not establish wasm behavioral
coverage or clear any remaining accepted-set obligation.


Helper master refresh at `63f666e668` integrates master `cda5584612`, including
#327 and the shared cancellation review from #342. The three merge conflicts
were resolved by retaining every master contract/documentation section, then
appending only the four unchanged helper contracts and their guidance/evidence.
All five Kraken rows equal their original branch/master values exactly.
Fresh full scoped Kraken tests pass **145 / zero failures / zero ignored**;
separate native and wasm32 all-targets Clippy, frozen install and format pass.
The checker was rebuilt from this integrated source. The repaired scoped wrapper
passes **134 rows / 539 public items / zero exemptions**, and the global checker
passes **7459 rows / 17206 public items**. Fresh production helper verification
passes **one runtime result / three explicit classifications / zero failed owners**.
Helper production bodies, native fixtures, controller and runner are byte-identical
to the checkpoint that executed all four compiled controls and exact restorations.
Independent merge-preservation review is clean; the obsolete #327-pending wording
has been corrected. Other accepted-set obligations remain open as listed above.


## Six accepted scalar helper bodies (2026-10-17)

The complete fills/income helper bodies were reviewed individually against the
actual TS source and MANIFEST-SPEC#kraken-utility-wire-domain. All six are decoders:
`fills::num`, `fills::optional_num`, `fills::to_seconds`, `income::num`,
`income::to_ms` and `income::is_position_size_leg`. The last function reads and
coerces raw asset/contract fields before comparison, rather than comparing two
already typed literal strings. The two optional-number functions remain separate
owners with independently executed calls.

Six new native witnesses plus all six existing utility source/boundary tests pass
**12 / zero failures / zero ignored**. Native observations retain finite f64 bits
including negative zero, missing/null/empty versus present zero, coercion errors,
pre-epoch mathematical floor, milliseconds versus seconds and explicit local-host
callback value/count. UTC inputs must not consult that callback. Raw predicate
cases include numeric/Boolean/array/object fields, no trimming, null/missing keys
and actual coercion errors. The maintained actual-source corpus independently
checks 164 cases across utility scalars, dates, mappers and boundary supplements;
it does not replace the new independently authored expected values.

Fresh native/default production probes execute all six actual APIs over **103
independently authored cases**, reporting **six runtime results / zero explicit
classifications / zero failed owners**. Numeric output uses native `to_bits`,
optional output observes actual Some/None, and Boolean/error-name results are
read before serialization. Production date probes use an explicit UTC identity
host; local UTC+02 behavior and missing scalar inputs have separate native tests.

`scripts/scalar-helper-contract-controls.py` completed all six compiled controls:
finite numeric result plus one, each optional decoder's missing result changed
to Some(0), date floor changed to ceil, milliseconds divided by 1000, and the position-leg
nonempty test changed to empty. Every mutation fails its own named native test
and only its own actual production output probe. Each exact restoration passes
that native witness and all six production probes. The final source bytes equal
the original fills.rs/income.rs and all 12 native/source tests pass again with
zero ignored. Compiler failures and tooling refusals cannot count as controls.

Master `9be4149af3` integrates as `ca3629d8ca`, including the four helper rows from
#345. All five existing master Kraken contracts are preserved exactly; only the
six new scalar rows are added. No production helper or protocol boundary changed.
The remaining owners, aliases, construction, local functions, 25 receiver refusals
and feature/wasm accepted behavior remain independent obligations.


Scalar readiness at integration `ca3629d8ca`: fresh full scoped Kraken tests pass
**151 / zero failures / zero ignored** across 30 target receipts including docs.
Separate native and wasm32 all-targets Clippy pass with warnings denied; frozen
install and workspace format pass. The scoped structural gate passes **134 rows /
539 public items / zero exemptions**, and global verification passes **7459 rows /
17206 public items**. Those checker executions reuse the compiled maintained
indexer after comparison confirmed unchanged Rust/Cargo/lock inputs. The maintained
TS extractor checks **83 symbols across seven slice files**. Fresh post-integration
production verification passes **six actual runtime results / zero classifications
/ zero failed owners**. Independent draft review is clean; CodeRabbit's requested
manual review returned a rate limit. The paginated review audit contains zero
inline conversations and zero substantive review summaries at this checkpoint.
This is six native/default owners, not complete venue or wasm behavioral coverage.

## Four private request-record object boundaries (2026-10-17)

The Futures/Spot candle Options, AccountLogParams and HistoricalOptions were each
reviewed against their actual TS object declarations and existing host profiles.
All four original derived decoders compiled and accepted independently authored
complete valid positional arrays in declaration order. Their named native tests
failed only the array rejection assertions, after valid object fields/presence
had passed through all seven ordinary/qualified/shared numeric routes. This was
an actual runtime baseline, rather than a compile error or invalid-array sample.

The concrete owner now captures through the maintained shared numeric backend,
then calls a private Serde remote schema through `strict_decode::object`.
Serialize, camelCase AccountLog keys, original field order, optional/null rules,
ignored unknown fields and JsNumberOutput registration are retained. No public
mapper method body, transport, signer, Store or callback boundary changed.

All four native owner tests pass through seven decoder routes. Native assertions
read every actual field, Option presence and f64 bits before Serialize, including
negative zero, 9007199254740993 rounding to binary64 2^53, the smallest subnormal,
and positive/negative infinity from overflowing numeric tokens. Missing required
fields, wrong typed values and the non-admitted snake_case AccountLog key fail.
The complete arrays and scalar/null/empty-array shapes are rejected independently
of generic map-sample ordering. The initial numeric supplement command launched
from the repository root was stopped and is excluded from evidence; the successful
receipt used sdk-rs and its pinned toolchain.

The native controller completed four sequential guard-only removals, each
requiring a compiled named failure containing that owner's exact valid array.
Every exact restoration passed all four native tests. The controller was repeated
successfully after extending the numeric witnesses, with the same four named
array-admission failures, four restored passes and final exact guarded source bytes.
No compiler failure or tooling refusal counts as a negative control.

Fresh native/default private/hidden compiler inspection and actual production
probes pass **four runtime results / zero classifications / zero failed owners**.
Each remote companion is private and has no independent Deserialize/conversion
trait or extra inherent API. Its sole generated helper takes the actual generic
Serde deserializer and returns the exact reviewed owner identity and source span.
Independent review caught a runner mistake requesting non-decoder companions as
required decoder owners; only the four actual owners are now requested, and the
complete companion compiler implementations are inspected separately. The first
runner refusal happened before runtime probes and is excluded from passing proof.

These are four concrete native/default record contracts. Other owners, aliases,
local functions, construction, 25 receiver refusals and feature/wasm accepted
behavior remain independent obligations under the maintained host profiles.


Private-record readiness after integrating master `51015dc5f9` as `858950c865`:
all eleven existing master Kraken contracts remain exactly preserved, with only
the four reviewed record rows added. The test-only fixture module is loaded once
at the crate root and shared by the three native unit modules; this fixes
all-target Clippy's duplicate-module finding without changing the production
schemas, guards, observations or control anchors.

Fresh full scoped Kraken tests pass **155 / zero failures / zero ignored** across
30 target receipts including docs. Separate native and wasm32 all-targets Clippy
pass with warnings denied. Frozen install and workspace format pass. The scoped
structural gate passes **134 rows / 539 public items / zero exemptions**; the global
gate passes **7459 rows / 17206 public items**. Both reuse the maintained compiled
indexer after comparing its actual Rust/Cargo/lock/config inputs. The maintained
TS extractor checks **83 symbols across seven slice files**. Fresh production
verification after the test-wiring fix again passes **four actual runtime results /
zero classifications / zero failed owners**. Guard-removal/restoration controls
remain the independently executed four-owner receipts described above.

The additional shared cancellation inventory audit passes with CI's required
`--all-features` profile: **one passed / zero failed / zero ignored** for
`production_cancellable_future_classifications_are_current`. An earlier
no-features invocation could not see its feature-gated health witness and is
excluded from proof; no cancellation source or classification was changed.
Independent review of the test-wiring fix is clean. CodeRabbit's manual request
returned a rate limit, rather than an executed review.

## Per-row Futures candle timestamp admission

The constructed `futures_candles_refuse_only_nan_timestamp_rows` test compiled
against unchanged production and failed because three valid candles were lost
with the bad row (zero returned). It now passes mixed, multiple-refusal and
all-refused responses, checks each visible `timestamp=NaN` diagnostic, and keeps
NaN/infinities/signed zero in other candle fields. The two existing Futures
candle source tests pass with the explicit admission distinction; source bytes
and source cardinality remain unchanged. Scoped strict Clippy, formatting,
required all-features cancellation inventory, global parity and extractor check
pass. Receipts: `/tmp/kraken-candle-row-{before,after,read-corpus,clippy,cancellation,parity,extract}.log`.
