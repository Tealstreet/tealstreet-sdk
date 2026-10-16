# OKX and KuCoin exchange websocket boundaries

Nine pending rows become ported, zero become divergent: three OKX and six KuCoin. Fresh branch feat/okx-kucoin-socket-boundaries starts at actual integration parent `61102ead04c728512ff39ee6ae5cf952e82bf2ad`; no unreviewed feature branch is used as parent. Initial implementation/census SHA is `31b82c548961c17d479eca3595720cf1f5ba7413`; reviewed correction and mutation-control SHA is `fa9837cf89dd7ac64e046ead2824e8b44d537b21`.

## Exact mapping

| TypeScript symbol | Rust target |
| --- | --- |
| OKXExchange.listenOHLCV | safe-cex-venue-okx/src/socket_boundaries.rs::listen_ohlcv |
| OKXExchange.listenOrderBook | safe-cex-venue-okx/src/socket_boundaries.rs::listen_order_book |
| OKXExchange.dispose | safe-cex-venue-okx/src/socket_boundaries.rs::dispose |
| KucoinExchange.listenOHLCV | safe-cex-venue-kucoin/src/socket_boundaries.rs::listen_ohlcv |
| KucoinExchange.listenOrderBook | safe-cex-venue-kucoin/src/socket_boundaries.rs::listen_order_book |
| KucoinExchange.dispose | safe-cex-venue-kucoin/src/socket_boundaries.rs::dispose_futures |
| KucoinSpotExchange.listenOHLCV | safe-cex-venue-kucoin/src/socket_boundaries.rs::listen_ohlcv_spot |
| KucoinSpotExchange.listenOrderBook | safe-cex-venue-kucoin/src/socket_boundaries.rs::listen_order_book_spot |
| KucoinSpotExchange.dispose | safe-cex-venue-kucoin/src/socket_boundaries.rs::dispose_spot |

Forwarders select the native service/member, capture its bound callee, pass original argument/callback handles, and return its original opaque result. Spot performs its first live provider read and truthiness branch, followed by a fresh second read during external member capture; the second provider can differ. Native disposal statements retain order and stop at original synchronous throws without a guard or catch. Futures constructs a shared callback, performs snapshot forEach, assigns an empty snapshot array, then disposes the private socket. The callback preserves the original snapshot dispose result, including for custom forEach observers; callbacks can be retained and reentered.

Adapted expression capabilities own complete live member evaluation, bound receivers, intrinsic truthiness, super calls, array assignment, and forEach length/HasProperty/Get/custom semantics. Original values, including promise handles, pass through synchronously without awaiting. Concrete class, language and provider assembly remain outside these function-body mappings. No shared trait, shared type or dependency changes. Programmatically serialized manifests preserve provenance; parity:extract writer ran after edits, exit 0 with Updated 0 manifests / 0 added / 0 orphaned.

## Gates and their actual SHAs

Cargo is /opt/homebrew/opt/rustup/bin/cargo. All Cargo invocations ran sequentially with --jobs 2. Focused tests used SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript.

| Actual SHA | Command | Exit |
| --- | --- | --- |
| 31b82c548961c17d479eca3595720cf1f5ba7413 | cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --all-targets --jobs 2 | 0 |
| 31b82c548961c17d479eca3595720cf1f5ba7413 | cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --jobs 2 --test socket_boundaries | 0 |
| fa9837cf89dd7ac64e046ead2824e8b44d537b21 | cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --all-targets --jobs 2 | 0 |
| fa9837cf89dd7ac64e046ead2824e8b44d537b21 | cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --jobs 2 --test socket_boundaries | 0 |
| fa9837cf89dd7ac64e046ead2824e8b44d537b21 | node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0 |

Focused native test name: socket_boundaries_native_before_source. Extractor text inspected: 7341 symbols across 512 files; Would update 0 manifests / 0 added / 0 orphaned. At fa9837cf89, measured git diff --exit-code against 31b82c5489 for OKX src/tests/Cargo and workspace Cargo/lock, store/types/num/exchange/transport and OKX TypeScript inputs: exit 0. OKX check/test remain measured at 31b82c5489, rather than relabeled as new checks. KuCoin's earlier 34-fixture gates at 31b82c5489 do not cover its corrected 35-fixture corpus.

## Native expectations and witnessed floors

Independent literal expectations enumerate 46 fixtures: OKX six forwarding fixtures plus five disposal fixtures, KuCoin twenty forwarding fixtures plus fifteen disposal fixtures. Native argument/callback/captured-callee/return/snapshot Rc identity, UTF-16 values, original error identity and full error fields, empty reset, routing, and event order are checked before invoking any source oracle. JSON comparison happens after all native fixtures; no decode/reencode roundtrip or concrete transport serialization is used as proof.

The OKX wrapper enumeration stops at listenOrderBook/invoke-error; disposal stops at businessWebsocket-error. KuCoin wrappers stop at Spot listenOrderBook/provider-error. Futures disposal's eleven cases stop at custom-return, followed by four Spot cases stopping at privateWebsocket-error. These are floors, not exhaustive language/provider or lifecycle coverage. The oracle extracts the maintained method bodies from the three exchange source files and executes getter/callee/receiver, retained/reentrant callback and opaque-return witnesses. Original synchronous errors assert name TypeError, message socket boundary failure, code SOCKET, ids [9,2] and object identity.

Existing OKX all-target oracle wiring already includes the focused target. KuCoin's explicit venue-oracle:kucoin script now includes --test socket_boundaries; the full venue scripts were not run.

Read-only sub-agent review found that a unit-returning Futures snapshot callback lost a custom forEach observer's dispose result. The correction adds generic R and native/source original-marker return identity assertions, and fixes the omitted KuCoin script target. During correction, a stale independent fixture expectation of 14 failed at exit 101 against the new 15; the fixed expectation passed. That development failure is not counted as a deliberate mutation proof.

## Census

Independent Python 3.11 tomllib census at 31b82c548961c17d479eca3595720cf1f5ba7413 versus actual parent 61102ead04c728512ff39ee6ae5cf952e82bf2ad: exit 0. OKX 79 ported / 19 divergent / 65 pending becomes 82 / 19 / 62, exactly three pending-to-ported changes. KuCoin 97 / 39 / 55 becomes 103 / 39 / 49, exactly six. Each venue enumeration covered 12 manifests, stopping at OKX okxeudemo.exchange.ts.toml and KuCoin kucoinspot.ws-public.ts.toml. Census is a status claim, not complete venue behavior proof. Fresh merge-head measurements follow below.

## Unmeasured scope

No workspace check, full venue script/suite, host clippy, wasm clippy, full Rust-index audit, CI rerun, live network test, concrete exchange/websocket class/provider assembly, or exhaustive intrinsic/prototype/custom provider behavior was run or proved. Already-ported rows outside this slice were not audited.

## Compiling controls at fa9837cf89

Each broken mutation was applied to fa9837cf89dd7ac64e046ead2824e8b44d537b21, then compiled and failed the named assertion at exit 101. Source was restored byte-for-byte and the same scoped socket_boundaries test passed at exit 0. Four venue/family controls, not nine independent symbol controls; enumeration stopped at KuCoin disposal. Zero survivors and zero compiler failures.

| Venue / family / call site | Mutation | Named assertion | Broken / restored |
| --- | --- | --- | --- |
| OKX forwarding / listen_ohlcv | businessWebsocket to publicWebsocket | FORWARD native receiver selection | 101 / 0 |
| OKX disposal / dispose | business before private | DISPOSE native independent stop reset trace | 101 / 0 |
| KuCoin forwarding / listen_ohlcv_spot | Invert provider truthiness branch | FORWARD native receiver selection | 101 / 0 |
| KuCoin disposal / dispose_futures | Private disposal before snapshot reset | DISPOSE native independent stop reset trace | 101 / 0 |

## Integration merge receipt

Fetched integration and merged `2a24bb3c48b25e781167071443dd603b0d0d2b8e` cleanly. Actual merge head is `92993c1874537425da9432cc11393a4bbdeab19b`.

At that merge head measured `git diff --exit-code fa9837cf89dd7ac64e046ead2824e8b44d537b21 HEAD --` both venue crates' src/tests/scripts/Cargo.toml, both venue manifest directories, and package.json: exit 0, byte-unchanged. Separately measured the same diff over workspace Cargo.toml/Cargo.lock, safe-cex-store (including the source loader), types/num/exchange/transport, both maintained TypeScript venue directories, parity/extract-ts, and yarn.lock: exit 0. Scoped Cargo checks/tests and compiling controls remain attached to the actual SHAs above; no Cargo rerun at the merge SHA is claimed because all tested inputs are unchanged.

Fresh extractor at 92993c1874537425da9432cc11393a4bbdeab19b: `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`, exit 0. Output inspected: 7341 symbols / 512 source files; Would update 0 manifests / 0 added / 0 orphaned.

Fresh independent Python 3.11 tomllib census at that merge head against merged integration: exit 0, exactly three OKX and six KuCoin pending-to-ported changes, zero divergent changes or other status changes. OKX 79/19/65 becomes 82/19/62; KuCoin 97/39/55 becomes 103/39/49 (ported/divergent/pending). Each enumeration covered 12 files and stopped at the same file named in the initial census above. These counts do not assert completeness of runtime proof.

The delivery receipt edit changes only this manually authored evidence file. Original parent remains 61102ead04c728512ff39ee6ae5cf952e82bf2ad; the later integration merge is recorded separately.

Measured receipt-only preservation before its commit: `git diff --exit-code 92993c1874537425da9432cc11393a4bbdeab19b -- . ":!sdk-rs/safe-cex-venue-kucoin/SOCKET_BOUNDARIES_EVIDENCE.md"` exit 0; `git diff --check` exit 0. No source or gate-input changes in this edit.
