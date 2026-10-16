# KuCoin public market stream evidence

Eight pending rows become ported, zero become divergent. Actual fresh parent: 407c879ef469c4ad7f702980834d11e80aacedaa. Native source/manifest/fixture commit: 3de034637d8b1b0ec19ebd512858eccf2997d848; direct callee assertions: 0685ce8c60f11d836d4436249b8d2b70c6b218c7. Package script wiring was added in c9eae342e55ea17c26b52edcbf19745a3e4661d1. No unreviewed feature branch was used as an ancestor; OKX is untouched by this slice.

All eight bodies are in safe-cex-venue-kucoin/src/public_streams.rs. Each row's ordered rs array includes its real transitive native helpers, computed from the native call graph and serialized with serializeManifest; notes contain every target and file.rust_files contains the module. Read-only review independently checked eight status changes and all 28 native functions covered. Four C3 supporting declarations are StreamOps, StreamCallback, StreamFuture, and BookState; no algorithm receives an exemption.

| Maintained TypeScript row | Native root |
| --- | --- |
| KucoinPublicWebsocket.handleTickerV2 | handle_ticker_v2 |
| KucoinPublicWebsocket.handleSnapshot | handle_snapshot |
| KucoinPublicWebsocket.handleInstrument | handle_instrument |
| KucoinPublicWebsocket.listenOHLCV | listen_ohlcv |
| KucoinPublicWebsocket.listenOrderBook | listen_order_book |
| KucoinSpotPublicWebsocket.handleTickerSnapshot | handle_spot_ticker_snapshot |
| KucoinSpotPublicWebsocket.listenOHLCV | listen_spot_ohlcv |
| KucoinSpotPublicWebsocket.listenOrderBook | listen_spot_order_book |

The native source owns disjoint ticker updates; nullish alias fallback; native candle numeric fields; parser registration/removal identity; Spot final-subscriber deletion versus Futures retained-empty topics; timer and unsubscribe ordering; persistent Futures book identity; snapshot request/catch/finally; buffering, stale rejection, gap recovery, replay, and requested refetch. Named local host capabilities bind complete live member/receiver expressions and whole standard intrinsics, array map/filter/for-of and iterator destructuring including abrupt/close precedence. Constructors are pure and preserve values/identities. Spawn initiates a native future immediately through its first await, routes ignored child rejection to a visible host reporter, and does not return it as a synchronous callback error. Await completion uses the host microtask scheduler. Those contracts are explicit obligations, not proof of arbitrary host implementations.

## Measured gates

Cargo executable /opt/homebrew/opt/rustup/bin/cargo; each invocation sequential with jobs 2. Oracle environment SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript.

| Actual SHA | Command | Exit |
| --- | --- | --- |
| 3de034637d8b1b0ec19ebd512858eccf2997d848 | cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --all-targets --jobs 2 | 0 |
| 3de034637d8b1b0ec19ebd512858eccf2997d848 | cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --jobs 2 --test public_streams | 0 |
| 0685ce8c60f11d836d4436249b8d2b70c6b218c7 | cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --all-targets --jobs 2 | 0 |
| 0685ce8c60f11d836d4436249b8d2b70c6b218c7 | cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --jobs 2 --test public_streams | 0 |
| c9eae342e55ea17c26b52edcbf19745a3e4661d1 | node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0 |

The focused target contains public_streams_native_before_source. parity:extract writer ran after programmatic manifest edits, before the source commit: exit 0; inspected output 7341 symbols / 512 files, Updated 0 / 0 added / 0 orphaned. It is not attributed to a later committed SHA. venue-oracle:kucoin explicitly includes --test public_streams now; that full venue script was not run. Extractor check output at c9 was inspected: 7341 symbols / 512 files, Would update 0 / 0 added / 0 orphaned. The direct Cargo check/test inputs stayed byte-identical between 0685 and script-wiring c9eae342e55ea17c26b52edcbf19745a3e4661d1; git diff --exit-code over source, tests, manifests and exemptions exited 0. Measured exits remain assigned to their original commits.

## Native assertions and finite profile

The independent fixture floor is 61: 29 ticker (seven per four bodies plus snapshot/getter-error), nine Futures candle, seven Spot candle, ten Futures book, six Spot book. Stops: ticker/alias, snapshot/getter-error, instrument/alias, spot-ticker/alias; Futures candle/lazy-topic, Spot candle/disposed, Futures book/invalid-code, Spot book/iterable. Global stop is Spot book/iterable. The native async driver stops once its pending queue is empty; an eight-wave cap fails if tasks remain. Scheduler waves were not counted.

All native assertions run before the child source oracle: exact Native::S frame operation/channel and helper topic arguments; ordered frame/partial-update/candle keys; independent native numeric bits and distinct OHLCV values; original ticker/error identity and full TypeError/down/STREAM/ids [7,2]; mandatory synchronous getter failure; emitted invalid/down values and independent emission counts; subscriber preservation/deletion; original captured stringify/send callees; independent callback/request/frame counts; finite Spot book filtering and sorting; decoded price/amount/total; persistent Futures book Rc identity; custom iterable versus indexed values and iterator close count. Futures array timestamps remain unnormalized while Spot millisecond timestamps normalize; this matches the source behavior.

Getter-reentry profile forces eight message.data reads in a Futures object candle, while inline OHLCV unsubscribe keeps old send/stringify despite Date.now replacing them. Lazy opaque api-symbol coercion happens twice at frame construction, after Date.now, producing API1/API2. Custom iterable levels have misleading indexed values 999 but iterator values 100/2; both native decoded values and close count one are asserted. Snapshot profiles include stale update, ordinary gap, in-flight buffering, replay gap/refetch, failed code, rejected response, delayed connection, disposal, and cleanup before pending snapshot completion. The last profile intentionally permits the original in-flight snapshot to deliver after cleanup. Timer reread after clearTimeout capture and error/finally structure were statically reviewed; arbitrary timer-getter reentry and throwing cleanup providers were not dynamically enumerated.

Source oracle extracts the maintained eight TypeScript AST bodies, actual ENDPOINTS and both interval tables, and actual Spot parseNumber/normalizeTimestamp declarations. Actual JS Promise/iteration/destructuring/sort/JSON and scalar helpers execute; recorded orderbook sort/total/price-update bindings and receiver methods remain fixture providers. No imported book-helper body, class/module assembly, arbitrary array/proxy/coercion intrinsic, runtime scheduler/cancellation or ignored-rejection provider implementation is proved. Source wire events are logical observations after parsing serialized frames; native decoded assertions are primary, not a decode/reencode equality gate.

Read-only review caught and corrected repeated candle reads, iterator destructuring, lazy wire topics, inline callee order, timer rereads, Spot predicate destructuring, and fallible spawn semantics before the source commit. Review subsequently requested emitted native values/counts, frame/topic values, and mandatory synchronous error assertions; these were included before SHA-bound gates. It identified missing venue-script wiring, corrected by the additive script commit. A precommit draft test compile failed with exit 101 for ambiguous float type and temporary string borrows; it was corrected before the named gates and is not mutation evidence.

## Independent status census

Python 3.11 tomllib census at 3de034637d8b1b0ec19ebd512858eccf2997d848 against actual parent 407c879ef469c4ad7f702980834d11e80aacedaa: exit 0. KuCoin 105 ported / 39 divergent / 47 pending becomes 113 / 39 / 39, exactly eight pending-to-ported rows. Twelve manifests enumerated, stopped at kucoinspot.ws-public.ts.toml. This is a status census, not complete venue runtime proof. Fresh merge-head measurements follow below.

## Unmeasured scope

No workspace compilation, full venue suite/script, OKX gates, host clippy, wasm clippy, full Rust-index audit, CI rerun, or live-network test ran. No concrete websocket/XHR/module assembly, imported orderbook-helper body proof, arbitrary host provider/intrinsic/iterator/coercion implementation, exhaustive timer/getter/cleanup throwing, concurrent scheduling/cancellation, or ignored child rejection reporter proof was produced. Previously ported rows outside this coherent eight-body unit were not audited.

## Compiling family controls

Five compiling production mutations at c9eae342e55ea17c26b52edcbf19745a3e4661d1 cover five new families. Each failed a named native assertion at exit 101, restored source bytes, then passed the same public_streams target at exit 0. Enumeration stopped at Futures book stale-sequence handling. Zero survivors and zero mutation compiler failures. Five family controls, not eight per-symbol controls.

| Family / call site | Mutation | Named native assertion | Broken / restored |
| --- | --- | --- | --- |
| Futures disjoint ticker / handle_snapshot | percentage field becomes volume | TICKER native disjoint fields | 101 / 0 |
| Spot ticker aliases / handle_spot_ticker_snapshot | last-to-mark projection overwrites bid | TICKER native disjoint fields | 101 / 0 |
| Candle fields / candle_parser | raw close index2 becomes index3 | CANDLE native distinct OHLCV fields | 101 / 0 |
| Spot book filtering / spot_book_parser | admit nonfinite price | BOOK native valid level floor | 101 / 0 |
| Futures book sequence / apply_increment | stale sequence returns true, emits duplicate | STREAM native independent callback floor | 101 / 0 |

## Fresh integration preservation

Merged fetched integration acf29148c3f26a07b72f93d1d964b49e81016ec6 into this fresh feature branch, producing 67c46fbf546fe7d1d93928bc381a7e9cea1f1142. Only parity/exempt.toml append additions conflicted; Python 3.11 parsed both sides and retained every incoming record plus exactly four unchanged local carrier records. No history rewrite or old unreviewed integration merge-down was used. The receipt's eight-wave wording was corrected to a cap; no claim all eight waves were reached is made.

Measurements at 67c46fbf546fe7d1d93928bc381a7e9cea1f1142:

- git diff --exit-code c9eae342e55ea17c26b52edcbf19745a3e4661d1 HEAD over KuCoin src, tests, scripts, Cargo.toml, and the whole KuCoin manifest directory: exit 0.
- Separate diff over sdk-rs/Cargo.toml and Cargo.lock; safe-cex-types, num, store, exchange, transport, crypto, candle-cache; maintained KuCoin TypeScript directory and yarn.lock: exit 0. Direct Cargo and child source-oracle inputs are byte-unchanged. Their actual measured 0685 exits and c9 restored-test exits stand; Cargo was not rerun at the merge SHA and exits are not relabeled.
- Exact parsed exemption union and byte-identical venue-oracle:kucoin script value: exit 0. Other incoming package script changes are preserved and are not direct Cargo/oracle inputs.
- Fresh node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check: exit 0. Output inspected: 7341 symbols / 512 files, Would update 0 / 0 added / 0 orphaned.
- Fresh Python 3.11 tomllib census versus merged integration acf29148c3f26a07b72f93d1d964b49e81016ec6: exit 0, exactly eight pending-to-ported rows and zero divergent transitions. Integration KuCoin 105 / 39 / 47 becomes branch 113 / 39 / 39 (ported / divergent / pending). Floor twelve manifests; stopped at kucoinspot.ws-public.ts.toml. OKX is unchanged by this slice.
- git merge-base --all origin/feat/safe-cex-rust HEAD returned only acf29148c3f26a07b72f93d1d964b49e81016ec6. Branch diff against that fetched integration has nine intended slice files.

Read-only sub-agent review checked code/proof corrections, all 28 native function target mappings, four supporting exemptions, actual five compiling-control logs, focused gate records and additive venue target. It found no code/wiring blocker; it caught the receipt's scheduler floor/cap wording, now corrected. It ran no Cargo, runtime test, or Rust-index audit. This subsequent receipt update changes documentation only.

Final read-only sub-agent review at 67c46fbf546fe7d1d93928bc381a7e9cea1f1142 independently measured own/shared preservation and working diff excluding this receipt at exit 0; parsed the exact incoming-plus-four exemption union; checked nine intended files, one integration merge base, and the actual five control/61-fixture/fresh-census/extractor records. It found no delivery blocker and ran no Cargo, runtime test or Rust-index gate.

## PR 7672 conflict repair

The slice was still unmerged when repair was requested. Merged origin/feat/safe-cex-rust at c2e36c3d08a9b98d0431bd2e306fbe60c3aab82d, producing 576414a1fd413e8aa3fe440b1a2ca68e51d5cfb6. Only exemption append additions conflicted; the parsed exact incoming-plus-four-local union was resolved without changing production/tests/manifests. Scoped git diff --exit-code against reviewed pushed b5122038cb46d252fbd4ca256fa6f99b7239b794 over KuCoin src/tests/scripts/Cargo and manifest directory: exit 0. Independent read-only review confirmed the unchanged venue script, exact union, nine intended files, and original eight status changes. It ran no Cargo/tests/index.

Fresh measurements, all actually run at merged SHA 576414a1fd413e8aa3fe440b1a2ca68e51d5cfb6:

| Command | Exit |
| --- | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --all-targets --jobs 2 | 0 |
| cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --jobs 2 --test public_streams | 0 |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0 |

Fresh focused test is public_streams_native_before_source, same 61-case floor ending Spot book/iterable. Extractor text inspected: 7341 symbols / 512 files, Would update 0 / 0 added / 0 orphaned. Fresh Python 3.11 tomllib census versus c2e36c3d08a9b98d0431bd2e306fbe60c3aab82d: exit 0, preserves the existing eight pending-to-ported rows, zero divergent transitions, 105/39/47 to 113/39/39. Twelve-manifest floor stopped at kucoinspot.ws-public.ts.toml. These are retained rows, not eight additional ports. Single merge base is c2e36c3d08a9b98d0431bd2e306fbe60c3aab82d. The original five source controls were not repeated because their source/tests are byte-unchanged; full venue scripts/suites, workspace checks, clippy, CI, live network, and provider assembly remain unrun/unproved as above.
