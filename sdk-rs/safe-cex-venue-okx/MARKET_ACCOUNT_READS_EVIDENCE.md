# OKX market and account reads evidence

Eight pending rows become ported; zero become divergent. The fresh branch is feat/okx-position-market-reads, based on actual integration parent c2e36c3d08a9b98d0431bd2e306fbe60c3aab82d. It does not inherit the unmerged Kucoin feature branch. Native implementation/manifest commit is e8855756cdf267723cc6771d89f2fb78dd5f81f3; the reviewed native ticker-collection assertion is committed at 3c93477ea009468da15ebac6ba36dd43355e71ca. The separate PR 7672 repair was already pushed at 8fb9e70d4feea0e38cb328b090cacba7d05b503e with fresh Kucoin measurements at 576414a1fd413e8aa3fe440b1a2ca68e51d5cfb6; its eight retained rows are not eight additional rows in this slice.

All targets are real bodies in safe-cex-venue-okx/src/market_account_reads.rs. Ordered mapping arrays and every target's provenance were produced through parseManifest/serializeManifest, including the transitive native helper bodies. All 24 functions have real manifest targets. Only four supporting declarations are C3 exemptions: MarketAccountOps, ReadAwait, ReadMapper and ReadAsyncMapper; none is an additional moved row. The parity writer ran after edits before the source commit: exit 0, Updated 0 manifests / 0 added / 0 orphaned. This precommit writer exit is not attributed to a later SHA. The existing venue-oracle:okx script already selects all targets; it includes this new test without a package-script change. That full script was not run.

| Maintained TypeScript function | Native root / owned callback helpers |
| --- | --- |
| OKXExchange.fetchMarkets | fetch_markets / map_market |
| OKXExchange.fetchTickersRaw | fetch_tickers_raw |
| OKXExchange.parseTickers | parse_tickers |
| OKXExchange.fetchPositionMetadata | fetch_position_metadata / metadata_pair |
| OKXExchange.fetchBalanceAndPositions | fetch_balance_and_positions |
| OKXExchange.fetchBalanceAndOpenPositions | fetch_balance_and_open_positions |
| OKXExchange.formatBalanceUsedAvail | format_balance_used_avail / margin_sum / currency_totals / order_leverage / order_used |
| OKXExchange.fetchLeverages | fetch_leverages / fetch_leverage_batch |

## Native proof and finite floors

The focused target is market_account_reads, test market_account_native_before_source. Every fixture asserts native values before packing and all 42 native fixtures run before child source comparison. Native assertions cover string IDs/symbols, ordered output and query keys, distinct IEEE number values and NaN, original row/market/balance/cache/error references, absent fresh-error metadata, original transport error name/message/code READ/ids [9,4], emitter arguments/counts, missing-symbol selection, spread-overwritten IDs, bound callee capture prefixes, endpoint/instrument order and literal batch widths/delays. Error before/after iterator-close fixtures both assert exactly one native balData read and one close. A throwing binding getter wins over a throwing iterator cleanup; the original native error object survives.

Independent literal floor: markets 7; raw tickers 4; parsed tickers 5; position metadata 4; combined positions 4; open positions 6; balance valuation 7; leverage 5, total 42. Enumeration stops respectively at markets/callee-reenter, raw/non-array, tickers/zero-open, metadata/hedged, combined/no-symbols, open/binding-error-close, balance/duplicate-currency, and leverage/batch-boundary. Global stop is leverage/batch-boundary. This is a finite floor, not exhaustive venue or provider proof.

Market fixtures discriminate M0 versus M1, linear/inverse base and quote, optional prettySymbol, amount precision 3 versus price 0.25, min amount 2 times contract 0.5, six distinct max sizes 101 through 106, NaN propagation, and leverage 1/9. A maxIcebergSz getter replaces Math.min after callee capture; the retained original callee still returns 101. Parsed ticker bid/ask 11/13, last/mark/index 12, percentage 20, volume 14 versus quote volume 15, and zero funding fields are independently asserted; zero-open witnesses Infinity. Unknown instruments are skipped. Metadata has ordered long/short pairs for SYM and MISSING, leverage 9/0, and OneWay/Hedged modes. Combined positions preserve original balance and no-symbol position references; metadata id OVERRIDE overwrites created id; fake position numeric fields are zero.

Balance fixtures use USDT 100 and BTC 2 with BTC mark 10, position imr 5/1 and upl 2/0.25, plus a selected limit order with remaining 2, price 20, leverage 4 and buffered used value 11. Independent native dollar results are used 15, free 94, total 115.5, upnl 4.5. Per-currency results are USDT used/free/total/upnl 16/84/98/2 and BTC 1/1/1.75/0.25. Duplicate USDT equity 50 overwrites the initial collateral bucket; native dollar free/total become 44/65.5 and USDT free/total 34/48. String account level "2" does not satisfy strict numeric level 2; numeric zero falls back to 2. Leverage batching uses literal 41 symbols: three requests of widths 20/20/1, sleeps 0/250/500, and native decoded leverage 7. Disposed and empty inputs create zero requests.

The oracle extracts the actual eight maintained TS AST bodies, actual endpoint table, actual enums, actual roundUSD and safe-math declarations/initialization, and real BigNumber. Actual JS operators, array callbacks, Promise/await and nested iterator binding run in those bodies. The selected source methods are never replaced by delegation stubs. Receiver helpers/XHR/imported mapper results and sequential p-iteration/sum/chunk/flatten fixture providers are recorded dependencies. Native callback bodies run inside whole map/reduce/filter/flatMap/some/for-of/binding and async-map capabilities; host implementations are explicit expression contracts, not generated row algorithms.

Read-only sub-agent review found an initial nested-binding ordering defect: reading balData after the outer iterator closed. The native binding now receives a real field-reading callback, which runs before close; paired source witnesses exercise actual iterator.return mutation and throwing cleanup. Review also identified an ignored native ticker-list helper argument that would make a wrong collection invisible to this fixture. Commit 3c93477ea009468da15ebac6ba36dd43355e71ca asserts the original ticker collection Rc identity before computing its mark. That was a static blindness finding, not a measured mutation survivor. Draft type/unused-field compile failures preceded the measured SHA and are not mutation controls. Static review verified eight pending transitions, all 24 function targets/provenance and exactly four supporting exemptions at exit 0; it ran no Cargo or oracle gates.

## Independent status census

At 3c93477ea009468da15ebac6ba36dd43355e71ca, Python 3.11 tomllib comparison against actual fresh parent c2e36c3d08a9b98d0431bd2e306fbe60c3aab82d exited 0: OKX 96 ported / 19 divergent / 48 pending becomes 104 / 19 / 40, exactly eight pending-to-ported changes. Twelve OKX manifests were enumerated; stop file okxeudemo.exchange.ts.toml. This is a status census, not full venue proof. Fresh merge-head measurements are recorded below separately.

## Unmeasured scope

No workspace compilation, full OKX/Kucoin venue suite or script, Kucoin check/test for this new OKX slice, host clippy, wasm clippy, full Rust-index audit, CI rerun, or live network gate was run. Concrete exchange/XHR/class/module assembly, imported mapper/helper implementations, arbitrary BigNumber inputs and JS getters/prototypes/coercions/iterators, and production provider implementations were not proved. Native await fixtures use ready values; source fixtures run real JS await with supplied bindings. External p-iteration concurrency, eager/non-cancelling initiation across arbitrary providers, arbitrary scheduling/cancellation, and arbitrary language intrinsics are explicit unproved provider obligations. The native parallel-map callback owns the real leverage batch body; no opaque fake-start callback substitutes for it. The mapped claim is function-body parity under these explicit operation contracts; constructor, class and unmapped helper rows remain pending.

## SHA-bound gates

Every Cargo invocation was sequential with jobs 2 and executable /opt/homebrew/opt/rustup/bin/cargo. The focused test used SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript.

| Actual measured SHA | Command | Exit |
| --- | --- | --- |
| 3c93477ea009468da15ebac6ba36dd43355e71ca | cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --all-targets --jobs 2 | 0 |
| 3c93477ea009468da15ebac6ba36dd43355e71ca | cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --jobs 2 --test market_account_reads | 0 |
| 3c93477ea009468da15ebac6ba36dd43355e71ca | node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0 |

The focused target passed one test containing the independent 42-fixture floor. Extractor output was inspected: 7341 symbols / 512 source files, Would update 0 manifests / 0 added / 0 orphaned. A prior draft focused invocation also passed before the ticker argument review correction; it is not claimed as the committed-SHA gate above.

Read-only correction/receipt review at 3c93477ea009468da15ebac6ba36dd43355e71ca confirmed the ticker argument fix, source preservation from e8855756cdf267723cc6771d89f2fb78dd5f81f3 at exit 0, and exact agreement of the recorded gate/census logs with this receipt. It found no additional concrete blocker within the stated expression/provider obligations. The reviewer ran no Cargo, oracle, index, or runtime gate.

## Local operation contracts for maintainers

MarketAccountOps is local to this module and does not adopt a new shared query ABI. capture returns the current source value or a bound callable handle, preserving its receiver before arguments run. The xhr.get:ENDPOINTS.* handles include the endpoint argument evaluation at that source call site. BigNumber capture/invoke represents the source constructor call, including constructor semantics; member then captures its bound multipliedBy/toNumber method before the next argument getter. Error construction is explicit through new_error. Imported helpers are captured before the native argument expressions and invoked through their supplied binding; this does not port their implementations.

get/set and computed get-index/set-index retain the whole property operation and its exceptions. to-property-key freezes a computed assignment key before later RHS calls. Object/array/literal constructors represent pure authored literals; spread/destructure/uppercase/join/Object.values intrinsic handles are complete source expressions, including their internal method lookup, coercion, iterator acquisition and cleanup. Arithmetic handles are the exact authored expression, including ticker percentage and (remaining * price) / leverage. The finite fixture implementations are not substitutes for production expression implementations.

map/filter/flat_map/reduce/some receive real native callback bodies. for_of and bind_first must acquire/step/close the iterator around those bodies and preserve original body-error precedence over cleanup failure. bind_first invokes the nested field-reading callback before closing the outer iterator. parallel_map must invoke the captured imported map with the native batch callback, start the native async body when the imported map invokes that callback, preserve live callee/receiver binding and source continuation scheduling, and retain already-started tasks through rejection. await_value must adopt the supplied value with source await semantics. These are provider requirements, not claims proved by the sequential ready-value fixture.

A local read of installed p-iteration/lib/static-methods.js map confirms it registers Promise.resolve(item).then callbacks before Promise.all; it does not synchronously enter every callback at the outer map call. The production provider must preserve that scheduling and first-poll the native body when the callback actually runs. The sequential fixture proves body values/order only; no timing gate was run.

## Compiling family controls

All nine controls were measured with HEAD 3c93477ea009468da15ebac6ba36dd43355e71ca plus the specified temporary production patch. Each compiled, failed a named native assertion at exit 101, restored the original source bytes, and then passed the same focused market_account_reads target at exit 0. No test/oracle file was weakened or mutated. Source/test byte preservation against that full SHA exited 0 after the last restoration. Eight new row families have one control each; the additional balance ticker-collection control closes the review's instrument blindness. Control enumeration stops at leverage-stagger, nine controls; zero measured survivors and zero mutation compiler failures. This does not imply exhaustive mutation coverage.

| Native call site | Production mutation | Assertion that actually failed | Broken / restored |
| --- | --- | --- | --- |
| map_market | contractSize reads lotSz instead of ctVal | MARKET native contract size | 101 / 0 |
| fetch_tickers_raw | successful response returns empty rows | RAW native independent row count | 101 / 0 |
| parse_tickers | volume reads vol24h instead of volCcy24h | TICKER native distinct numbers | 101 / 0 |
| metadata_pair | second side repeats Long instead of Short | METADATA native side ordering | 101 / 0 |
| fetch_balance_and_positions | fake position defaults become 1 instead of 0 | COMBINED native fake zeros | 101 / 0 |
| fetch_balance_and_open_positions | read balData after closing outer binding iterator | OPEN native balData before iterator close identity | 101 / 0 |
| format_balance_used_avail | output used receives upnl instead of used_p | BALANCE native independent dollar values | 101 / 0 |
| format_balance_used_avail | mark helper receives store.markets instead of store.tickers | BALANCE native original ticker collection identity | 101 / 0 |
| fetch_leverage_batch | stagger multiplier 250 becomes 25 | LEVERAGE native stagger delays | 101 / 0 |

The last control decoded native delays [0,25,50] against independent [0,250,500] before any source comparison. The ticker collection mutation was measured after the native Rc assertion correction and is caught; the static gap is closed, and no survivor was measured.

## Integration merge and fresh measurements

The final fetch merged actual integration 71ab6dd78100ff547dbfb64e2ded8d84196adcc8 at 290cddc849c87ad14571ef807e5985c603707f5d. Only sdk-rs/parity/exempt.toml conflicted. Python 3.11 tomllib resolution preserved the exact incoming 1019 records plus exactly four unchanged local C3 records; parsed union equality exited 0. No conflict markers were committed and no history was rewritten. git merge-base --all returned exactly the fetched integration SHA, and the diff against it contains the seven slice files, stopping at tests/market_account_reads.rs; no old unreviewed integration merge-down is carried.

Measured at 290cddc849c87ad14571ef807e5985c603707f5d:

- git diff --exit-code 3c93477ea009468da15ebac6ba36dd43355e71ca HEAD over the whole OKX src/tests/scripts/Cargo.toml and whole OKX manifest directory exited 0.
- A separate scoped byte comparison over sdk-rs/Cargo.toml; safe-cex-types, num, store, exchange, transport, crypto, candle-cache; TS OKX sources, types.ts, roundUSD.ts, safe-math.ts; and yarn.lock exited 0. This proves those source inputs only. Cargo.lock and package.json were deliberately outside this comparison: incoming integration adds the GRVT crypto dependency and the already-reviewed Kucoin public_streams script target. All-build-input preservation is not claimed; the scoped OKX build/test gates were freshly run at this merge SHA.
- Fresh Python 3.11 tomllib comparison against the merged integration exited 0: exactly eight pending-to-ported changes, zero divergent transitions. Integration OKX 96/19/48 becomes branch 104/19/40 (ported/divergent/pending). Floor twelve manifests, stop okxeudemo.exchange.ts.toml. These are fresh measurements, not relabeled values from the old census sheet.
- The fetched integration contains Kucoin repair 8fb9e70d4feea0e38cb328b090cacba7d05b503e, measured by git merge-base --is-ancestor at exit 0 before this final merge. The original branch parent remains c2e36c3d08a9b98d0431bd2e306fbe60c3aab82d; the reviewed repair arrives through integration, not as an unreviewed feature ancestor.

| Actual measured SHA | Command | Exit |
| --- | --- | --- |
| 290cddc849c87ad14571ef807e5985c603707f5d | cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --all-targets --jobs 2 | 0 |
| 290cddc849c87ad14571ef807e5985c603707f5d | cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --jobs 2 --test market_account_reads | 0 |
| 290cddc849c87ad14571ef807e5985c603707f5d | node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0 |

Final read-only sub-agent review verified all nine raw broken/restored outputs, original-SHA binding, exact parsed 1019+4 exemption union and eight native row transitions. It flagged the unrelated Cargo.lock change and corrected the distinction between a static blindness finding and a measured survivor. Those receipt corrections are applied above. It ran no Cargo, test, oracle or index gate.

The fresh merge-head focused test passed its 42-fixture floor. Fresh extractor output was inspected: 7341 symbols / 512 source files; Would update 0 manifests / 0 added / 0 orphaned. All three actual merge-head gate exits are 0. The nine controls remain attributed to their original 3c93477ea009468da15ebac6ba36dd43355e71ca-plus-patch measurements.

For delivery, only the four local C3 records are relocated beside an existing OKX declaration, avoiding an additional local append at the shared file EOF. Independent parsed comparisons exited 0: every incoming record retains its exact values and relative order, and all four reviewed local records retain their exact values and order. This metadata placement does not change Rust, tests, manifests or the extractor source inputs; no gate is relabeled as running at the later documentation/placement commit.

Final placement/receipt review independently measured parsed preservation at exit 0: four local records follow existing OKX TickerPriceField, all 1019 incoming values/relative order and four local values/order remain unchanged, total 1023 unique records. Its source/test/script/Cargo/whole OKX manifest diff versus 290cddc849c87ad14571ef807e5985c603707f5d and tracked diff excluding this receipt/exempt.toml both exited 0. It read the completed fresh merge-head logs and confirmed all three exits are 0 at their actual SHA, with no delivery blocker. The reviewer ran no Cargo/test/index gate. The ensuing commit contains receipt and exemption placement only; the measured source and oracle inputs remain unchanged.
