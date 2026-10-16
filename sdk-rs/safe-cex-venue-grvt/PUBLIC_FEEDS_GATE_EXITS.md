# GRVT public feed gates

Runtime/gate SHA: `e801c60afd38b6585a19a145818cfceba2867d44`.
Actual starting integration parent: `972c5ec9bd8c4bb2023530199c45961dfca86229`.
Fetched integration: `65e086a3c9039a79e4c7e9f354aba31d8f83009e`.
Actual merge-down SHA: `17363287ef05555d9035f1b2c4d624fff5e24863`.
Branch: `feat/rust-grvt-public-feeds`. This branch starts directly from the supplied integration parent, with no unmerged feature branch used as an ancestor.

## Rows and implementation

Moved **5 pending→ported, 1 pending→FORCED divergent**. Ported: onMessage, handleKlineUpdate, handleOrderbookUpdate, handleTickerUpdate, listenOHLCV. Divergent: listenOrderBook.

Independent `/opt/homebrew/bin/python3` tomllib census reads git objects across nine GRVT manifests. Fixed expectations: 164 rows, exactly five pending→ported transitions and one pending→divergent transition. Starting parent versus runtime/gate SHA, and fetched integration versus merge SHA, both measure 65 ported / 47 divergent / 52 pending becoming **70 / 48 / 46**. Census stops at those nine manifests; it is separate from behavioral proof.

PublicFeedHost binds the original JS value/property/coercion operations, live Store lookups/updates, tracking/statistics, subscription operations and timers. Local Rust bodies implement guards, routing, parsing, arithmetic, dispatch, registration/retry and cleanup. Opaque value handles preserve feed identity, retained ticker references, inherited market lookups and own undefined symbol/selector slots. Normal listener markets and position entries retain shared Market/Position references. HandlerSlot preserves holes versus explicit undefined across spread/filter/forEach. Counts, handler identities and read order survive callback-driven replacement and mutation.

FORCED divergence at `public_feeds.rs::listen_order_book`: shared OrderBook has bids/asks Vec<OrderBookOrders>, which cannot retain sparse JavaScript map output. Native code completes both sides' source parsing before returning visible ExchangeError `Sparse GRVT orderbook levels cannot be represented by shared OrderBook`, without fabricating a callback or invoking subsequent cleanup. Source trailing-hole witness independently asserts length 3, one callback, and Object.hasOwn(bids,2) false before the native representation-failure assertions. Source completes cleanup; native error leaves its registered handler. Dense output retains source behavior. No duplicate venue-local OrderBook or shared-type change was introduced.

Full public class/constructor remain pending. These configured host ports do not claim complete websocket/network integration.

## Gates measured at runtime/gate SHA

All cargo ran sequentially with jobs 2. Node versions: 24.20.0 and 26.7.0. Every venue oracle used `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Command | Measured exit / outcome |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-grvt --all-targets --jobs 2` | 0; compilation finished |
| `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-grvt --test public_feeds --jobs 2` | 0; six tests passed, Node 24 |
| Same test command with `/opt/homebrew/opt/node@26/bin` prepended to PATH | 0; six tests passed, Node 26 |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0; output `Would update 0 manifests; 0 added, 0 orphaned symbols.` |

Writer ran after each manifest edit through the pinned Yarn JS entry, exit 0; last output `Updated 1 manifests; 0 added, 0 orphaned symbols.` Arrays were emitted by TOML serialization/extractor, never hand-spelled.

The oracle extracts the six actual TypeScript members and actual jsonParse declaration. Native route enum, lookup keys, Candle/OrderBook fields, optional ticker fields and own slots, info/feed identity, errors, counters, handlers and ordered effects are asserted; no decode/re-encode comparison is used. No wire assertions are needed for this slice. Non-NaN f64 bits and NaN classification are asserted without claiming engine/CPU-dependent NaN payload equivalence.

The fixed floor stops at **78 cases**: message 12, kline 11, book dispatch 11, ticker 19, candle listener 12, book listener 13. Cases include tracking/dispatch/statistics/callback/update/sync failures; changing error and ticker-last getters; inherited constructor and false market lookup; nullish ticker; market-ID/symbol and position-contract mutation; live handler deletion/addition/replacement, feed mutation and spread-materialized undefined; delayed registration after early cleanup; string versus numeric time units; partial ticker fields, volume/change ordering and mark fallback; book floating-point addition order; current/prior row replacement and prior-size fallback reread; trailing sparse output. Fixed independent expectations cover row/effect/callback/level/handler/update/identity counts. This floor is not exhaustive enumeration.

Development check at `ff8b617d9540e519718560aac5531b0b23a7890b` exited 101 due missing test Host initializers. Fixture-only correction `85be018aa4ab39dfb18eb55824d9d26ff55c5023` fixed that. The runtime/gate SHA above also includes the finer prior-entry fallback reread; the full named scoped gates and six controls were measured again there. Earlier successful measurements are not substituted for this SHA.

## Compiling deliberate controls

Each control temporarily changes only src/public_feeds.rs against the runtime/gate SHA, then runs the Node 24 test command above with `<target> -- --exact --nocapture`. Each broken run compiled, entered the named test and panicked at the named native assertion. Original bytes were restored before each passing target; final source/tests diff against runtime/gate SHA exited 0. No compiler-only failure counts as a control, and no control survived.

| Method / mutation | Exact target | Named failing assertion | Broken exit | Restored exit |
| --- | --- | --- | --- | --- |
| onMessage: book stream dispatches Ticker enum | message_native | MESSAGE native route | 101 | 0 |
| handleKlineUpdate: dispatch whole message instead of original feed | kline_native | HANDLER native feed identity | 101 | 0 |
| handleOrderbookUpdate: dispatch whole message instead of original feed | book_native | HANDLER native feed identity | 101 | 0 |
| handleTickerUpdate: funding divisor 100→10 | ticker_native | TICKER native fundingRate | 101 | 0 |
| listenOHLCV: string time divisor 1e9→1e6 | candle_listener_native | CANDLE native timestamp | 101 | 0 |
| listenOrderBook: current-level cumulative total starts at zero | book_listener_native | BOOK native total | 101 | 0 |

Sub-agent review_snapshots approved immutable runtime/gate SHA e801c60afd38b6585a19a145818cfceba2867d44, including the final prior-size fallback reread, 78-case floor and real sparse-output divergence. Reviewer ran no cargo/gates and did not certify parent gate measurements.

## Merge-down measurements

Fetched and merged integration down immediately before delivery, no conflicts. At the merge SHA named above, `git diff --exit-code e801c60afd38b6585a19a145818cfceba2867d44 HEAD -- sdk-rs/safe-cex-venue-grvt/src sdk-rs/safe-cex-venue-grvt/tests packages/safe-cex/src/exchanges/grvt/grvt.ws-public.ts packages/json-parse/src/jsonParse.ts` exited **0**. Venue source/tests and actual oracle inputs are byte-identical, so behavioral/control exits remain measurements at the runtime/gate SHA. They were not rerun at merge.

Cargo.lock and shared transport implementation changed during merge. The exact scoped cargo-check command reran at merge SHA 17363287ef05555d9035f1b2c4d624fff5e24863: **exit 0**, compilation finished. This slice's behavioral witnesses use configured host operations; they do not exercise the changed transport or live sockets. Pinned extractor check reran at that merge SHA: **exit 0**, output `Would update 0 manifests; 0 added, 0 orphaned symbols.` `git diff --check` exited 0. Independent merge census reconfirmed six moved rows and 46 remaining pending.

The following evidence-only commit changes this manually authored file; no runtime gate is claimed measured at that evidence-only SHA.

Not run: workspace compilation; full monorepo/crate suites; host clippy; wasm check/clippy/tests; Rust parity-validator execution; live transport/network/private authentication; previously ported test files; pending class/constructor; exhaustive domains; custom/mutated Object.prototype execution; engine-specific NaN payload proof. No workspace, wasm, full Rust-validator or live-network green claim.
