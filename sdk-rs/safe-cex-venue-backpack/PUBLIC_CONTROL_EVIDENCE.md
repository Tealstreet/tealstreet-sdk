# Backpack public WebSocket control methods

Code SHA: `d911b0589c4ad5339f8ad175c905a05e0429be93`.
Actual branch parent: `d4d9ae0b019e4c5466a67294aa474f897ee38061`, the integration merge of PR 7505. No separate unreviewed lane branch was used as an ancestor.

Python 3.14 `tomllib` census over the six Backpack manifests: parent 22 ported / 53 divergent / 72 pending; code SHA 32 / 53 / 62, 147 total. This slice moved **10 pending → ported, 0 → divergent**: onOpen, ping, resubscribe, subscribeToOrderBook, subscribeToTicker, subscribeToTrades, subscribeToBookTicker, subscribeToKline, unsubscribeFromKline, unsubscribe.

The Rust targets in `src/public_control.rs` implement these ten method bodies over compatible native UTF-16 strings, ordered string Sets, mutable sparse streams arrays, and configured callbacks. State borrows are dropped before callbacks. Subscription insertion precedes send; unsubscribe sends its snapshot before inspecting the live streams for deletion; original errors propagate unchanged. Ping stores the exact f64 timestamp before send, and onOpen checks disposed once before the four ordered callbacks. Constructor, transport wiring, message handlers, and order-book recovery remain pending.

## Measured gates at the code SHA

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --all-targets --jobs 2`: exit **0**, dev profile finished, no compile errors.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: exit **0**; output says 7341 symbols across 512 files, would update 0 manifests, 0 added / 0 orphaned symbols.
- `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --test public_control --jobs 2`: exit **0**, 10 passed / 0 failed after all controls were restored.
- Ten individually selected `public_control` tests after mutation restoration: each exit **0**, output says 1 passed / 0 failed. All Cargo processes were sequential, jobs 2.
- `git diff --exit-code d911b0589c -- sdk-rs/safe-cex-venue-backpack/src sdk-rs/safe-cex-venue-backpack/tests`: exit **0** after all ten controls.

Oracle tests use `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`, execute the actual TS public socket class, and decode sent JSON into method/UTF-16 params before comparing native Rust messages, memberships, timestamps, callback order/count, and original error identity. Every wire equality comparison is deferred until all native assertions in that test have run.

Independent Rust inputs and literal case counts pin 188 records: 120 subscribe, 12 resubscribe, 16 unsubscribe, 12 kline delegation, 18 ping, 10 open. Inputs cover empty strings, controls, quotes/backslashes, astral pairs, lone high/low surrogates, duplicates, deletion/reinsertion, send failure, live stream append/shrink/rewrite, sparse holes, signed zero, NaN, infinities, and failure at each callback stage. These are finite floors: enumeration stops at subscribe index 119 (method 4 / case 23), resubscribe 11, unsubscribe 15, kline 11, ping 17, and open 9. Arbitrary replaced prototypes/getters, JSON monkeypatches, and the full socket runtime are not proved.

## Discrimination controls at the code SHA

Each change below compiled and failed its selected test with exit **101**, including the named native assertion and `test result: FAILED`. Each was restored byte-identically and the same selected test then exited **0** with 1 passed / 0 failed. No survivors and no compile-only controls.

| Selected test | Deliberate regression | Named failing native assertion |
| --- | --- | --- |
| order_book_subscriptions | depth prefix changed to wrongDepth | subscribeToOrderBook case 0 native insertion before send |
| ticker_subscriptions | ticker prefix changed to wrongTicker | subscribeToTicker case 0 native insertion before send |
| trade_subscriptions | trade prefix changed to wrongTrade | subscribeToTrades case 0 native insertion before send |
| book_ticker_subscriptions | bookTicker prefix changed to wrongBookTicker | subscribeToBookTicker case 0 native insertion before send |
| kline_subscriptions | interval and symbol swapped | subscribeToKline case 0 native insertion before send |
| resubscribe_ordered_snapshot | SUBSCRIBE changed to UNSUBSCRIBE | RESUBSCRIBE case 2 native method |
| unsubscribe_live_streams_and_failure | UNSUBSCRIBE changed to SUBSCRIBE | UNSUBSCRIBE case 2 native method |
| unsubscribe_kline_delegate | forwarded interval and symbol swapped | KLINE-UNSUBSCRIBE case 0 native delegate params |
| ping_guards_time_and_failure | timestamp increased by 1 | PING case 0 native timestamp before send |
| open_callback_order_and_failure | reset callback moved before track | OPEN case 0 native callback order |

Restored production source SHA-256: `7912d56b175764e29eb4a3368978be27150bc03cb753b595b48bb0664b64f3fc`.

Read-only sub-agent review found no concrete parity defects in the ten bodies and supported ported classification over the stated views. The reviewer did not execute gates.

Development failures before the code SHA: the first TS loader exited 1 because an instance initializer calls lodash/throttle; loading the actual dependency fixed it, and direct Node oracle execution exited 0. The first manifest writer exited 1 with invalid layer `venue`; correcting it to schema-supported `adapted` allowed the writer to exit 0, with 0 manifest updates / 0 added / 0 orphaned symbols. Those failures are not semantic controls.

Not run: workspace compilation, unrelated venue tests, host clippy, wasm compilation/clippy, full monorepo suites, CI, or a live exchange socket.

## Integration merge validation

Integration `95e6665431234b770e820f5f06dab0e3ad386d76` was merged down at `27110df6007c40a085605cb4dbe1b36b1fa31a03`, without conflicts. At that merge SHA:

- Full Backpack `src`/`tests` comparison against code SHA `d911b0589c4ad5339f8ad175c905a05e0429be93`: exit **0**, byte-unchanged; all discrimination controls still apply to the same production/test inputs.
- Shared dependency sources and Cargo.lock changed upstream, so the scoped all-targets Cargo check was rerun: exit **0**, dev profile finished with no compile errors.
- The focused `public_control` oracle target was rerun with the stated TypeScript environment: exit **0**, 10 passed / 0 failed.
- Extractor check rerun: exit **0**, output says 7341 symbols / 512 files, would update 0 manifests, 0 added / 0 orphaned symbols.
- Fresh `tomllib` census: 32 ported / 53 divergent / 62 pending, 147 total; unchanged movement of 10 pending → ported, 0 → divergent.

These exits were measured at the merge SHA, not at a later evidence-only commit.

## Delivery preservation

Immediately before push, integration was fetched again and `1273115ab915ad59ed9b478c48ca6356d8ac03f0` was merged down at `79af344f2bec1033ff67ac2bcb9a5f768dbda26f` without conflicts. `git diff --exit-code 27110df6007c40a085605cb4dbe1b36b1fa31a03 HEAD -- sdk-rs/safe-cex-venue-backpack/src sdk-rs/safe-cex-venue-backpack/tests` exited **0**: the entire crate source/test inputs remained byte-identical, so the recorded scoped gate/control exits stand. Final independent tomllib census remains 32 / 53 / 62; this slice moves exactly 10 pending rows to ported. No gates were rerun at this final merge or the following evidence-only commit; the measured SHAs above delimit the claims.
