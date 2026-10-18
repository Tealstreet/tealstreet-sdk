# WEEX public websocket gates

This public-websocket unit moves **8 rows off pending: 7 ported, 1 divergent**. Constructor, `connectAndSubscribe`, `onMessage`, `listenOHLCV`, `listenOrderBook`, `dispose` and `handleTicker` are ported. `handleDepth` is CHOSEN divergent: gap snapshot rejections and routed-callback throws produce a warning; TypeScript silently discards them with `catch(() => {})`. The complete websocket class stays pending.

Branch: `feat/rust-venue-weex-public-websocket`. Authoring parent: **`97b6f919d26396b52e4b3514f98fcfa34b4dcc6c`**, the supplied merged integration head. No external unreviewed branch was used as an authoring ancestor. Implementation commit: `a8761f634aab08e5f20895d6bdf65880173fc245`; corrected test fixture and mutation measurement SHA: **`fd0b92f53a86e3d56dc7cc10bc3f60363115b735`**.

## Native proof scope

The oracle transpiles the actual TypeScript class, including its constructor, eight data initializers and twenty arrow fields. It executes the actual bigint-safe parser, actual `toNumber`/`toIntegerString`, venue constants, shared warning severity and book update/total helpers. Base construction, transport, store callbacks, candle normalization, JavaScript property/coercion/iteration operations and Promise observation are dependency boundaries. Native code implements the local algorithms; no complete class or host integration is claimed.

The fixture enumeration stops at **fixture 71, `depth_live_append`**, in one corpus pass; the constructor enumeration stops at **28 fields**. Both counts are checked against independent literals. Native comparisons decode the source values into Rust values and check field contents, binary64 bits, UTF-16 units, undefined/null, sparse arrays, book state, callback arguments, ticker patches, errors, listener options, subscription keys and reads. Constructor field order and fresh set/object identity have native assertions. Every native assertion across the floor runs before any event-trace assertion. Native values are never re-encoded or roundtripped.

The native parser dependency returns decoded values produced by the actual imported source parser, while the native invocation's UTF-16 argument is separately asserted. This proves dispatcher argument handling and behavior after parsing; it does not prove a new Rust parser implementation. The numeric, property-key, iterator, normalizer and book-helper test adapters are bounded models. Arbitrary proxies/prototypes, all coercion permutations, custom thenables, every iterator-close failure and all scheduler interleavings were not enumerated. Snapshot observers own their callback values and catch success-callback throws as well as request rejection. No store-wide borrow guard is held by the new local algorithms across an observer settlement.

Witnesses include invalid/false/null packets, ping ID mutation before send, receipt and disposal guards, exact subscription error defaults, bigint and unpaired-surrogate values, writable ticker/depth handlers, routed-method capture before symbol coercion, kline getter ordering, live/sparse candle iteration, callback-error iterator closing, unsupported intervals, unsubscribe before snapshot settlement, disposal after fetch, callback/subscription failures caught by the snapshot observer, ticker fallback types and percentage boundaries, bid/ask deltas, ID fallback, live depth iteration and gap refresh behavior.

## Review finding and its measured control

Read-only sub-agent review identified coercion-order and writable-method capture defects; these were fixed before the implementation commit. A later review found that the original dynamic ticker fixture omitted `d`. The deliberate direct-dispatch mutation at **`a8761f634aab08e5f20895d6bdf65880173fc245`** exited **101**, but only `public_feed::event_trace "message_dynamic_ticker"` failed; the decoded native assertion survived. Restoration exited **0**. This is an instrument finding at `on_message`'s `this.handleTicker` call site, not a native discrimination pass. Commit `fd0b92f53a86e3d56dc7cc10bc3f60363115b735` adds ticker data so bypassing the writable handler changes native ticker state. Remeasurement at the corrected fixture SHA exited **101** on `public_feed::message_native message_dynamic_ticker`, then **0** after restoration. The original native-assertion survivor remains documented here.

Review also identified an expectation copied from native reads for the intentional gap-warning difference. The rejection case now asserts literal `error:message`; routed-callback rejection retains the source's literal empty read list. The reviewer verified the eight real targets and the stated scopes; it ran no builds, tests, mutations, edits or commits.

## Initial measured gates

The following four commands exited **0** at **`a8761f634aab08e5f20895d6bdf65880173fc245`**. They are initial measurements and are not claimed as reruns at the later fixture SHA. Cargo commands ran serially, one cargo process at a time, using `/opt/homebrew/opt/rustup/bin/cargo`. Oracle tests used `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Command | Exit | Observation |
| --- | --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --all-targets --jobs 2` | 0 | Scoped crate and targets compile; no workspace compilation claim. |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --jobs 2 --test public_feed` | 0 | `public_feed_native_source_floor`: 1 passed, 0 failed; 71 fixtures. |
| `python3.12 sdk-rs/safe-cex-venue-weex/tests/parity-check.py` | 0 | Canonical invention/status gate: 156 rows, 171 public Rust items, 0 WEEX exemptions. |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 | 7341 symbols; `Would update 0 manifests; 0 added, 0 orphaned symbols.` |

The extractor WRITER ran after manifest edits before the implementation commit and printed `Updated 1 manifests; 0 added, 0 orphaned symbols.` Yarn was judged by output text, in addition to the measured exit. Preliminary uncommitted cargo-test preparation runs exited 101 on test adapter compilation and 101 on an incorrect interval mapping; the mapping was corrected to the existing Rust venue constants. Neither preparation failure is reported as a gate at a later SHA.

## Census and limitations

Tomllib census at `fd0b92f53a86e3d56dc7cc10bc3f60363115b735` against the authoring parent: **156 WEEX rows**, before 97 ported / 30 divergent / 29 pending, after **104 / 31 / 21**. Exactly eight rows move off pending; seven to ported and one to divergent. Every new row target was independently checked against a real public function, struct or enum declaration in the committed Rust source. The canonical gate checks all 171 public Rust items. This is a venue census, not complete runtime proof. Shared SDK types and runtime traits were not changed.

Not run: full WEEX suite, workspace compilation, full monorepo suites, host clippy, wasm clippy, live venue requests or CI. No CI rerun, PR creation, integration merge-up, income pager work or history rewrite was performed.

## Compiled native mutation floor

`python3.12 sdk-rs/safe-cex-venue-weex/tests/public-feed-mutations.py` exited **0** at **`fd0b92f53a86e3d56dc7cc10bc3f60363115b735`**. Each broken source compiled and failed its named decoded native assertion with **101**; each original source was restored byte-for-byte in `finally` and the same test passed with **0**, `1 passed; 0 failed`. Cargo ran serially. Enumeration stops at **control 11, gap-warning-symbol**, not exhaustive mutation coverage. No native assertion survivors remain among these selected corrected controls; the original instrument finding above is retained.

| Mutation / call site | Named native assertion | Broken / restored exits |
| --- | --- | --- |
| Constructor: pong flag true→false | `public_feed::constructor_native constructor_defaults` | 101 / 0 |
| Connection: alter User-Agent value | `public_feed::connect_native connect_basic` | 101 / 0 |
| Packet dispatch: alter pong interval | `public_feed::message_native message_ping` | 101 / 0 |
| Candle listener: use mark-price topic | `public_feed::candle_native candles_supported` | 101 / 0 |
| Book snapshot callback: subscribe before callback | `public_feed::book_native book_callback_rejection` | 101 / 0 |
| Disposal: call parent.on in place of off | `public_feed::dispose_native dispose_off_failure` | 101 / 0 |
| Ticker patch: swap native mark/index fields | `public_feed::message_native message_ticker` | 101 / 0 |
| Depth delta: swap native price/amount arguments | `public_feed::message_native message_depth` | 101 / 0 |
| Writable ticker dispatch: bypass current method field | `public_feed::message_native message_dynamic_ticker` | 101 / 0 |
| Kline routing: capture method after key coercion | `public_feed::message_native message_coercion_order` | 101 / 0 |
| Gap rejection: erase warning's symbol | `public_feed::depth_native_gap_warning depth_gap_reject` | 101 / 0 |

`git diff --exit-code fd0b92f53a86e3d56dc7cc10bc3f60363115b735 -- sdk-rs/safe-cex-venue-weex/src sdk-rs/safe-cex-venue-weex/tests` exited **0** after the controls. Native mark/index and price/amount swaps are caught before wire assertions, without re-encoding native values.

## Gates at the corrected fixture SHA

All four baseline commands above were rerun serially and measured at **`fd0b92f53a86e3d56dc7cc10bc3f60363115b735`**, after every control was restored:

| Command from the initial gate table | Measured exit | Observation |
| --- | --- | --- |
| Scoped cargo check, all targets, jobs 2 | 0 | Scoped crate and targets compile. |
| `public_feed` test target, jobs 2 and the same TypeScript environment | 0 | 1 passed, 0 failed; 71-fixture floor. |
| Scoped canonical parity wrapper | 0 | 156 rows, 171 public Rust items; 0 WEEX exemptions. |
| Extractor check via node/yarn | 0 | 7341 symbols; `Would update 0 manifests; 0 added, 0 orphaned symbols.` |

## Pre-push integration merge-down

Fetched and merged the exact integration commit **`a739087a0791b49fb0d6d34a5ac0d631f3ce6130`**. Merge SHA: **`63b6d1c2978a38c94e2450313fbee1917584c838`**. The merge was clean, without conflicts or history rewriting.

`git diff --exit-code fd0b92f53a86e3d56dc7cc10bc3f60363115b735 63b6d1c2978a38c94e2450313fbee1917584c838 -- sdk-rs/safe-cex-venue-weex/src sdk-rs/safe-cex-venue-weex/tests` exited **0**. A separate preservation diff also exited **0** for Cargo.toml/lock, shared Rust types/num/store, the actual `safe-cex-parity` checker crate, WEEX TypeScript, shared enums/logging, parser/book helper source, extractor implementation and yarn.lock. The root package change adds only Kucoin's `rest_reads` target; the global exemption file also changes. WEEX still has zero exemptions.

The changed global inputs warranted these two merge measurements:

| Command | SHA | Exit | Observation |
| --- | --- | --- | --- |
| `python3.12 sdk-rs/safe-cex-venue-weex/tests/parity-check.py` | `63b6d1c2978a38c94e2450313fbee1917584c838` | 0 | 156 rows, 171 public Rust items; 0 WEEX exemptions. |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | `63b6d1c2978a38c94e2450313fbee1917584c838` | 0 | 7341 symbols; `Would update 0 manifests; 0 added, 0 orphaned symbols.` |
| Integration-relative tomllib census plus real declaration audit | `63b6d1c2978a38c94e2450313fbee1917584c838` | 0 | **7 pending→ported, 1 pending→divergent**, zero missing new targets. |

The census compares the actual merged integration parent `a739087a0791b49fb0d6d34a5ac0d631f3ce6130`, rather than a stale merge-base. Integration remains 97 ported / 30 divergent / 29 pending; this branch is **104 / 31 / 21**, across the same 156 rows in six manifests. Enumeration stops at that venue census.

Scoped cargo check, the native test and mutation controls were **not rerun at the merge SHA**. Their measured exits remain bound to `fd0b92f53a86e3d56dc7cc10bc3f60363115b735`; source/tests and their primitive implementation/dependency inputs are byte-unchanged. The subsequent commit edits only this evidence sheet.

## Wave-1 gap recovery correction

This supersedes the historical gap-warning divergence above. Gap snapshot
rejections and routed-callback throws now use the source's silent catch callback;
initial listenOrderBook snapshot failures keep their warning callback. The
handleDepth manifest row is now ported. Observer ownership remains unchanged.

Test-only commit1ed3bccc40 compiles against master and fails through real native
HTTP and the public socket: after a sequence gap and HTTP400, the log emitter
receives an unexpected snapshot warning. The fixed regression observes no log,
no failed replacement delivery, then a valid contiguous update to the retained
book. All38 binding tests and the71-fixture actual-source public-feed corpus pass.
The corpus no longer substitutes native warning/read values into source results
or filters warning trace events. Its existing initial-snapshot warning cases
continue to compare directly with source.

The updated gap-silence mutation reinstates the old reporting callback, compiles
and fails public_feed::depth_native depth_gap_reject with101; exact restoration
passes the source corpus with0. Other historical mutation controls were not
rerun. Measured fixed worktree public_feed.rs SHA256 before implementation commit:
`28fcb99fd35864bd21ab2c5bb42832f0b5fa32c817513c5c2b161e9d97d1f565`.
