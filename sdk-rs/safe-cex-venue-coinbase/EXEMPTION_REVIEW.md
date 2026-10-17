# Coinbase exemption per-entry ledger

Owner: codex-4ri6pb. Reviewed baseline mirror master `83e92cff633666e35ea071e805ee85d5a45e6c83`; production fixes #671 (UUID), #677 (credential segments), and #691 (fallible health callbacks, merged `3b1e5560f3`). The additional actual-public-socket book-copyback RED is committed as `4ddb05c090`; the repair is #699.

Exact census: **419 unique exemption declarations across 25 files**. SHA-256 of newline-joined sorted `rs` identifiers: `401f56c345e1df752af4e949a9fbd09752d5ceb82e534c58f7fbbbc1709a6b00`. Every declaration appears once below. No exemption was removed.

`KEEP` means the declaration names a receiver, imported capability, inline value, retained root, or Rust plumbing, rather than an independent TypeScript declaration. It does **not** approve every mapped body using it. Existing manifest FINDINGs and shared UTF-8, typed-output, finite JSON and alias boundaries remain recorded; no carrier redesign is authorized here. `INERT` means the source Starbase feature remains closed, even though its exported contracts and tests remain real. Reused Promise/Frame primitives are identified separately from activation.

The review followed the concrete constructor/Exchange trait into each host implementation, rather than treating an exemption reason as proof. Checked dispatch, clock source, credential/UUID generation, error propagation and property/value projection against current TypeScript and the named native/source witnesses. Existing test references are evidence scope, not a claim of fresh execution of every historical test. The new regression executes source listenOrderBook and its real shared helpers and compares three socket frames for both Coinbase variants.

Outstanding mapped notification/raw-ID findings remain in NOTIFICATION_REACHABILITY_REVIEW.md and the canonical manifests. A valid receiver exemption neither hides nor closes those separately indexed findings. Native constructor/Store origin witnesses and #691 improve specific paths only. Shared transport-body phase 2 remains with codex-dwfl8p; no shared body/transport API changed.

## `private_health.rs`

Source owner: `coinbase.exchange.ts health methods`. Concrete route: `binding/lifecycle.rs and binding/sockets.rs`. Witnesses under this crate: `private_binding_frames.rs; private_health source/Store tests`.

| Entry | Rust declaration | Disposition and reviewed role |
|---:|---|---|
| 001 | `CoinbasePrivateUserStreamHealth` | KEEP: State carrier for CoinbaseExchange private-stream health handlers, whose logic is mapped individually from coinbase.exchange.ts; the helper type adds no independent venue behavior. |

## `exchange_lifecycle.rs`

Source owner: `coinbase.exchange.ts lifecycle fields`. Concrete route: `binding/lifecycle.rs::LifecycleHost`. Witnesses under this crate: `exchange_lifecycle.rs; exchange_binding.rs`.

| Entry | Rust declaration | Disposition and reviewed role |
|---:|---|---|
| 002 | `LifecycleAccount` | KEEP: Native receiver or named inline return/callback/update/Promise shape for the nine individually mapped CoinbaseExchange lifecycle bodies; retains roots and own-undefined presence, with no additional TypeScript symbol. |
| 003 | `LifecycleAsync` | KEEP: Native receiver or named inline return/callback/update/Promise shape for the nine individually mapped CoinbaseExchange lifecycle bodies; retains roots and own-undefined presence, with no additional TypeScript symbol. |
| 004 | `LifecycleCallback` | KEEP: Native receiver or named inline return/callback/update/Promise shape for the nine individually mapped CoinbaseExchange lifecycle bodies; retains roots and own-undefined presence, with no additional TypeScript symbol. |
| 005 | `LifecycleHost` | KEEP: Native receiver or named inline return/callback/update/Promise shape for the nine individually mapped CoinbaseExchange lifecycle bodies; retains roots and own-undefined presence, with no additional TypeScript symbol. |
| 006 | `LifecycleHost::clear_timeout` | KEEP: Concrete `binding/lifecycle.rs::LifecycleHost::clear_timeout` supplies this imported call site; source policy remains in mapped caller. |
| 007 | `LifecycleHost::connect_private` | KEEP: Concrete `binding/lifecycle.rs::LifecycleHost::connect_private` supplies this imported call site; source policy remains in mapped caller. |
| 008 | `LifecycleHost::connect_public` | KEEP: Concrete `binding/lifecycle.rs::LifecycleHost::connect_public` supplies this imported call site; source policy remains in mapped caller. |
| 009 | `LifecycleHost::dispose_base` | KEEP: Concrete `binding/lifecycle.rs::LifecycleHost::dispose_base` supplies this imported call site; source policy remains in mapped caller. |
| 010 | `LifecycleHost::dispose_private` | KEEP: Concrete `binding/lifecycle.rs::LifecycleHost::dispose_private` supplies this imported call site; source policy remains in mapped caller. |
| 011 | `LifecycleHost::dispose_public` | KEEP: Concrete `binding/lifecycle.rs::LifecycleHost::dispose_public` supplies this imported call site; source policy remains in mapped caller. |
| 012 | `LifecycleHost::emit_and_log_error` | KEEP: Concrete `binding/lifecycle.rs::LifecycleHost::emit_and_log_error` supplies this imported call site; source policy remains in mapped caller. |
| 013 | `LifecycleHost::emit_orders_fetch_error` | KEEP: Concrete `binding/lifecycle.rs::LifecycleHost::emit_orders_fetch_error` supplies this imported call site; source policy remains in mapped caller. |
| 014 | `LifecycleHost::fetch_balance` | KEEP: Concrete `binding/lifecycle.rs::LifecycleHost::fetch_balance` supplies this imported call site; source policy remains in mapped caller. |
| 015 | `LifecycleHost::fetch_markets` | KEEP: Concrete `binding/lifecycle.rs::LifecycleHost::fetch_markets` supplies this imported call site; source policy remains in mapped caller. |
| 016 | `LifecycleHost::fetch_open_orders_snapshot` | KEEP: Concrete `binding/lifecycle.rs::LifecycleHost::fetch_open_orders_snapshot` supplies this imported call site; source policy remains in mapped caller. |
| 017 | `LifecycleHost::fetch_tickers_intx` | KEEP: Concrete `binding/lifecycle.rs::LifecycleHost::fetch_tickers_intx` supplies this imported call site; source policy remains in mapped caller. |
| 018 | `LifecycleHost::find_selected_portfolio` | KEEP: Concrete `binding/lifecycle.rs::LifecycleHost::find_selected_portfolio` supplies this imported call site; source policy remains in mapped caller. |
| 019 | `LifecycleHost::get_markets` | KEEP: Concrete `binding/lifecycle.rs::LifecycleHost::get_markets` supplies this imported call site; source policy remains in mapped caller. |
| 020 | `LifecycleHost::init_external_public_data` | KEEP: Concrete `binding/lifecycle.rs::LifecycleHost::init_external_public_data` supplies this imported call site; source policy remains in mapped caller. |
| 021 | `LifecycleHost::is_disposed` | KEEP: Concrete `binding/lifecycle.rs::LifecycleHost::is_disposed` supplies this imported call site; source policy remains in mapped caller. |
| 022 | `LifecycleHost::last_private_user_stream_message_at` | KEEP: Concrete `binding/lifecycle.rs::LifecycleHost::last_private_user_stream_message_at` supplies this imported call site; source policy remains in mapped caller. |
| 023 | `LifecycleHost::loaded` | KEEP: Concrete `binding/lifecycle.rs::LifecycleHost::loaded` supplies this imported call site; source policy remains in mapped caller. |
| 024 | `LifecycleHost::log` | KEEP: Concrete `binding/lifecycle.rs::LifecycleHost::log` supplies this imported call site; source policy remains in mapped caller. |
| 025 | `LifecycleHost::mark_private_user_stream_degraded` | KEEP: FIXED #691 (merged `3b1e5560f3`): log/warning throws propagate through existing native carrier; real timer/auth frame/latch witnesses. |
| 026 | `LifecycleHost::merge_rest_orders_with_fresh_ws_state` | KEEP: Concrete `binding/lifecycle.rs::LifecycleHost::merge_rest_orders_with_fresh_ws_state` supplies this imported call site; source policy remains in mapped caller. |
| 027 | `LifecycleHost::public_only` | KEEP: Concrete `binding/lifecycle.rs::LifecycleHost::public_only` supplies this imported call site; source policy remains in mapped caller. |
| 028 | `LifecycleHost::set_timeout` | KEEP: Concrete `binding/lifecycle.rs::LifecycleHost::set_timeout` supplies this imported call site; source policy remains in mapped caller. |
| 029 | `LifecycleHost::tick` | KEEP: Concrete `binding/lifecycle.rs::LifecycleHost::tick` supplies this imported call site; source policy remains in mapped caller. |
| 030 | `LifecycleHost::update` | KEEP: Concrete `binding/lifecycle.rs::LifecycleHost::update` supplies this imported call site; source policy remains in mapped caller. |
| 031 | `LifecycleReceiver` | KEEP: Native receiver or named inline return/callback/update/Promise shape for the nine individually mapped CoinbaseExchange lifecycle bodies; retains roots and own-undefined presence, with no additional TypeScript symbol. |
| 032 | `LifecycleTimer` | KEEP: Native receiver or named inline return/callback/update/Promise shape for the nine individually mapped CoinbaseExchange lifecycle bodies; retains roots and own-undefined presence, with no additional TypeScript symbol. |
| 033 | `LifecycleUpdate` | KEEP: Native receiver or named inline return/callback/update/Promise shape for the nine individually mapped CoinbaseExchange lifecycle bodies; retains roots and own-undefined presence, with no additional TypeScript symbol. |
| 034 | `LifecycleValidation` | KEEP: Native receiver or named inline return/callback/update/Promise shape for the nine individually mapped CoinbaseExchange lifecycle bodies; retains roots and own-undefined presence, with no additional TypeScript symbol. |
| 379 | `LifecycleAccountRoot` | KEEP: Retained mutable carrier for the inline account object returned by the separately mapped getAccount body; names its Rc/RefCell representation and adds no independent behavior. |

## `public_market_listeners.rs`

Source owner: `coinbase.ws-public.ts listenOHLCV/listenOrderBook`. Concrete route: `binding/sockets.rs::PublicMarketListenerHost`. Witnesses under this crate: `public_market_data.rs; exchange_binding.rs`.

| Entry | Rust declaration | Disposition and reviewed role |
|---:|---|---|
| 035 | `ListenerFailure` | KEEP: Native listener field-state adapter for mapped Coinbase listenOHLCV/listenOrderBook: shared book arrays/levels/inner state and callback/cleanup aliases retain source roots. |
| 036 | `PublicBook` | KEEP: Native listener field-state adapter for mapped Coinbase listenOHLCV/listenOrderBook: shared book arrays/levels/inner state and callback/cleanup aliases retain source roots. |
| 037 | `PublicBookCallback` | KEEP: Native listener field-state adapter for mapped Coinbase listenOHLCV/listenOrderBook: shared book arrays/levels/inner state and callback/cleanup aliases retain source roots. |
| 038 | `PublicBookInnerState` | KEEP: Native listener field-state adapter for mapped Coinbase listenOHLCV/listenOrderBook: shared book arrays/levels/inner state and callback/cleanup aliases retain source roots. |
| 039 | `PublicBookLevel` | KEEP: Native listener field-state adapter for mapped Coinbase listenOHLCV/listenOrderBook: shared book arrays/levels/inner state and callback/cleanup aliases retain source roots. |
| 040 | `PublicBookLevels` | KEEP: Native listener field-state adapter for mapped Coinbase listenOHLCV/listenOrderBook: shared book arrays/levels/inner state and callback/cleanup aliases retain source roots. |
| 041 | `PublicCandleCallback` | KEEP: Native listener field-state adapter for mapped Coinbase listenOHLCV/listenOrderBook: shared book arrays/levels/inner state and callback/cleanup aliases retain source roots. |
| 042 | `PublicCleanup` | KEEP: Native listener field-state adapter for mapped Coinbase listenOHLCV/listenOrderBook: shared book arrays/levels/inner state and callback/cleanup aliases retain source roots. |
| 043 | `PublicListenerBook` | KEEP: Native listener field-state adapter for mapped Coinbase listenOHLCV/listenOrderBook: shared book arrays/levels/inner state and callback/cleanup aliases retain source roots. |
| 044 | `PublicListenerReceiver` | KEEP: Native listener field-state adapter for mapped Coinbase listenOHLCV/listenOrderBook: shared book arrays/levels/inner state and callback/cleanup aliases retain source roots. |
| 045 | `PublicMarketDelegateHost` | KEEP: Imported BaseWebSocket/dayjs/orderbook/cache callback capability declarations for mapped public candle/book listeners and exchange delegates; source policy remains in the separately mapped field bodies. |
| 046 | `PublicMarketDelegateHost::public_listen_ohlcv` | KEEP: Concrete `binding/sockets.rs::PublicMarketListenerHost::public_listen_ohlcv` supplies this imported call site; source policy remains in mapped caller. |
| 047 | `PublicMarketDelegateHost::public_listen_order_book` | KEEP: Concrete `binding/sockets.rs::PublicMarketListenerHost::public_listen_order_book` supplies this imported call site; source policy remains in mapped caller. |
| 048 | `PublicMarketListenerHost` | KEEP: Imported BaseWebSocket/dayjs/orderbook/cache callback capability declarations for mapped public candle/book listeners and exchange delegates; source policy remains in the separately mapped field bodies. |
| 049 | `PublicMarketListenerHost::add` | KEEP: Concrete `binding/sockets.rs::PublicMarketListenerHost::add` supplies this imported call site; source policy remains in mapped caller. |
| 050 | `PublicMarketListenerHost::calc_order_book_total` | KEEP: FIXED #699: shared helper remains canonical; copyback consumes distinct same-price levels once. Actual socket/source RED 4ddb05c090. |
| 051 | `PublicMarketListenerHost::cleanup_message_handler_stats` | KEEP: Concrete `binding/sockets.rs::PublicMarketListenerHost::cleanup_message_handler_stats` supplies this imported call site; source policy remains in mapped caller. |
| 052 | `PublicMarketListenerHost::daily_candle` | KEEP: Concrete `binding/sockets.rs::PublicMarketListenerHost::daily_candle` supplies this imported call site; source policy remains in mapped caller. |
| 053 | `PublicMarketListenerHost::dayjs_unix` | KEEP: Concrete `binding/sockets.rs::PublicMarketListenerHost::dayjs_unix` supplies this imported call site; source policy remains in mapped caller. |
| 054 | `PublicMarketListenerHost::fetch_ohlcv_detached` | KEEP: Concrete `binding/sockets.rs::PublicMarketListenerHost::fetch_ohlcv_detached` supplies this imported call site; source policy remains in mapped caller. |
| 055 | `PublicMarketListenerHost::is_connected` | KEEP: Concrete `binding/sockets.rs::PublicMarketListenerHost::is_connected` supplies this imported call site; source policy remains in mapped caller. |
| 056 | `PublicMarketListenerHost::nanoid` | KEEP: Concrete `binding/sockets.rs::PublicMarketListenerHost::nanoid` supplies this imported call site; source policy remains in mapped caller. |
| 057 | `PublicMarketListenerHost::public_data_only_option` | KEEP: Concrete `binding/sockets.rs::PublicMarketListenerHost::public_data_only_option` supplies this imported call site; source policy remains in mapped caller. |
| 058 | `PublicMarketListenerHost::set_daily_candle` | KEEP: Concrete `binding/sockets.rs::PublicMarketListenerHost::set_daily_candle` supplies this imported call site; source policy remains in mapped caller. |
| 059 | `PublicMarketListenerHost::set_timeout` | KEEP: Concrete `binding/sockets.rs::PublicMarketListenerHost::set_timeout` supplies this imported call site; source policy remains in mapped caller. |
| 060 | `PublicMarketListenerHost::sort_order_book` | KEEP: FIXED #699: shared helper remains canonical; copyback consumes distinct same-price levels once. Actual socket/source RED 4ddb05c090. |
| 061 | `PublicMarketListenerHost::timeframe_seconds` | KEEP: Concrete `binding/sockets.rs::PublicMarketListenerHost::timeframe_seconds` supplies this imported call site; source policy remains in mapped caller. |
| 062 | `PublicMarketListenerHost::update_order_book` | KEEP: FIXED #699: shared helper remains canonical; copyback consumes distinct same-price levels once. Actual socket/source RED 4ddb05c090. |
| 063 | `PublicMarketListenerHost::utc_end_of_day_ms` | KEEP: Concrete `binding/sockets.rs::PublicMarketListenerHost::utc_end_of_day_ms` supplies this imported call site; source policy remains in mapped caller. |
| 064 | `PublicMarketListenerHost::warn` | KEEP: Concrete `binding/sockets.rs::PublicMarketListenerHost::warn` supplies this imported call site; source policy remains in mapped caller. |
| 417 | `PublicListenerConnectivity` | KEEP: Native receiver-binding selector for the separately mapped listener bodies: choose the supplied imported getter or the actual owning class socket; no independent listener algorithm. |

## `public_market_reads.rs`

Source owner: `coinbase.exchange.ts fetchMarkets/fetchOHLCV`. Concrete route: `binding/reads.rs::PublicMarketReadHost`. Witnesses under this crate: `public_market_data.rs; exchange_binding.rs`.

| Entry | Rust declaration | Disposition and reviewed role |
|---:|---|---|
| 065 | `CatalogMarket` | KEEP: Native field-body adapter for mapped Coinbase fetchMarkets/INTX/fetchOHLCV: retained catalog info and arrays, shared live options, hot Promise result, and typed HTTP parameters preserve identity and nonfinite numbers before the wire boundary. |
| 066 | `CatalogMarkets` | KEEP: Native field-body adapter for mapped Coinbase fetchMarkets/INTX/fetchOHLCV: retained catalog info and arrays, shared live options, hot Promise result, and typed HTTP parameters preserve identity and nonfinite numbers before the wire boundary. |
| 067 | `CatalogSupportedTypes` | KEEP: Native field-body adapter for mapped Coinbase fetchMarkets/INTX/fetchOHLCV: retained catalog info and arrays, shared live options, hot Promise result, and typed HTTP parameters preserve identity and nonfinite numbers before the wire boundary. |
| 068 | `PublicCandleQuery` | KEEP: Native field-body adapter for mapped Coinbase fetchMarkets/INTX/fetchOHLCV: retained catalog info and arrays, shared live options, hot Promise result, and typed HTTP parameters preserve identity and nonfinite numbers before the wire boundary. |
| 069 | `PublicCandles` | KEEP: Native field-body adapter for mapped Coinbase fetchMarkets/INTX/fetchOHLCV: retained catalog info and arrays, shared live options, hot Promise result, and typed HTTP parameters preserve identity and nonfinite numbers before the wire boundary. |
| 070 | `PublicMarketQuery` | KEEP: Native field-body adapter for mapped Coinbase fetchMarkets/INTX/fetchOHLCV: retained catalog info and arrays, shared live options, hot Promise result, and typed HTTP parameters preserve identity and nonfinite numbers before the wire boundary. |
| 071 | `PublicMarketReadHost` | KEEP: Imported HTTP/dayjs/cache/emitter host contract for mapped Coinbase market catalog and historical candle bodies; methods supply capabilities and add no exchange dispatch or venue policy. |
| 072 | `PublicMarketReadHost::cached_markets` | KEEP: Concrete `binding/reads.rs::PublicMarketReadHost::cached_markets` supplies this imported call site; source policy remains in mapped caller. |
| 073 | `PublicMarketReadHost::daily_candle` | KEEP: Concrete `binding/reads.rs::PublicMarketReadHost::daily_candle` supplies this imported call site; source policy remains in mapped caller. |
| 074 | `PublicMarketReadHost::dayjs_subtract` | KEEP: Concrete `binding/reads.rs::PublicMarketReadHost::dayjs_subtract` supplies this imported call site; source policy remains in mapped caller. |
| 075 | `PublicMarketReadHost::dayjs_value_of` | KEEP: Concrete `binding/reads.rs::PublicMarketReadHost::dayjs_value_of` supplies this imported call site; source policy remains in mapped caller. |
| 076 | `PublicMarketReadHost::emit_and_log_error` | KEEP: Concrete `binding/reads.rs::PublicMarketReadHost::emit_and_log_error` supplies this imported call site; source policy remains in mapped caller. |
| 077 | `PublicMarketReadHost::error_response_items` | KEEP: Concrete `binding/reads.rs::PublicMarketReadHost::error_response_items` supplies this imported call site; source policy remains in mapped caller. |
| 078 | `PublicMarketReadHost::get` | KEEP: Concrete `binding/reads.rs::PublicMarketReadHost::get` supplies this imported call site; source policy remains in mapped caller. |
| 079 | `PublicMarketReadHost::get_supported_types` | KEEP: Concrete `binding/reads.rs::PublicMarketReadHost::get_supported_types` supplies this imported call site; source policy remains in mapped caller. |
| 080 | `PublicMarketReadHost::public_only` | KEEP: Concrete `binding/reads.rs::PublicMarketReadHost::public_only` supplies this imported call site; source policy remains in mapped caller. |
| 081 | `PublicMarketReadHost::set_daily_candle` | KEEP: Concrete `binding/reads.rs::PublicMarketReadHost::set_daily_candle` supplies this imported call site; source policy remains in mapped caller. |
| 082 | `PublicMarketReadReceiver` | KEEP: Native field-body adapter for mapped Coinbase fetchMarkets/INTX/fetchOHLCV: retained catalog info and arrays, shared live options, hot Promise result, and typed HTTP parameters preserve identity and nonfinite numbers before the wire boundary. |
| 083 | `PublicOHLCVOptions` | KEEP: Native field-body adapter for mapped Coinbase fetchMarkets/INTX/fetchOHLCV: retained catalog info and arrays, shared live options, hot Promise result, and typed HTTP parameters preserve identity and nonfinite numbers before the wire boundary. |
| 084 | `PublicOHLCVOptionsRoot` | KEEP: Native field-body adapter for mapped Coinbase fetchMarkets/INTX/fetchOHLCV: retained catalog info and arrays, shared live options, hot Promise result, and typed HTTP parameters preserve identity and nonfinite numbers before the wire boundary. |
| 085 | `PublicReadAsync` | KEEP: Native field-body adapter for mapped Coinbase fetchMarkets/INTX/fetchOHLCV: retained catalog info and arrays, shared live options, hot Promise result, and typed HTTP parameters preserve identity and nonfinite numbers before the wire boundary. |

## `cdp_signing.rs`

Source owner: `coinbase.utils.ts sign and cached signing keys`. Concrete route: `binding/crypto.rs::CdpSigningHost`. Witnesses under this crate: `binding/crypto.rs native JWT tests; native_clock.rs; exchange_binding.rs`.

| Entry | Rust declaration | Disposition and reviewed role |
|---:|---|---|
| 086 | `CdpAsync` | KEEP: Native inline/receiver representation: Named hot Promise/result shape for imported PEM/WebCrypto signing operations. |
| 087 | `CdpCryptoKey` | KEEP: Native inline/receiver representation: Opaque imported WebCrypto CryptoKey identity handle retained by the source module cache. |
| 088 | `CdpImportKey` | KEEP: Native inline/receiver representation: Native inline importKey argument record with DER bytes, algorithm, extractability and usages. |
| 089 | `CdpJwtHeader` | KEEP: Native inline/receiver representation: Native anonymous JWT header literal retaining algorithm, type, key id and nonce. |
| 090 | `CdpJwtPayload` | KEEP: Native inline/receiver representation: Native anonymous JWT payload literal retaining issuer, audience, raw numeric claims and optional URI. |
| 091 | `CdpSigningCache` | KEEP: Native inline/receiver representation: Receiver for the two source module-global cached key/secret cells. |
| 092 | `CdpSigningHost` | KEEP: Native inline/receiver representation: Imported source PEM/WebCrypto/Ed25519/virtualClock/uuid capability boundary; cache, JWT policy and await order remain in individually mapped functions. |
| 093 | `CdpSigningHost::current_time_ms` | KEEP: Concrete `binding/crypto.rs::CdpSigningHost::current_time_ms` supplies this imported call site; source policy remains in mapped caller. |
| 094 | `CdpSigningHost::import_key` | KEEP: Concrete `binding/crypto.rs::CdpSigningHost::import_key` supplies this imported call site; source policy remains in mapped caller. |
| 095 | `CdpSigningHost::prepare_pkcs8` | KEEP: Concrete `binding/crypto.rs::CdpSigningHost::prepare_pkcs8` supplies this imported call site; source policy remains in mapped caller. |
| 096 | `CdpSigningHost::sign_ecdsa` | KEEP: Concrete `binding/crypto.rs::CdpSigningHost::sign_ecdsa` supplies this imported call site; source policy remains in mapped caller. |
| 097 | `CdpSigningHost::sign_ed25519` | KEEP: Concrete `binding/crypto.rs::CdpSigningHost::sign_ed25519` supplies this imported call site; source policy remains in mapped caller. |
| 098 | `CdpSigningHost::uuid` | KEEP: FIXED #671: concrete callback now uses shared nanoid21/separator-removal UUID helper; actual signed JWT, POST and mint witnesses. |
| 099 | `CdpSigningInput` | KEEP: Native inline/receiver representation: Primitive-call carrier retaining the native source header/payload literals before JSON.stringify and their actual signing bytes; adds no source invocation. |
| 100 | `CdpSigningReceiver` | KEEP: Native inline/receiver representation: Receiver grouping the source module cache, imported crypto/clock/uuid operations and Promise scheduler. |

## `exchange_tick.rs`

Source owner: `coinbase.exchange.ts tick`. Concrete route: `binding/lifecycle.rs::TickHost`. Witnesses under this crate: `order_sync_auth.rs; exchange_binding.rs`.

| Entry | Rust declaration | Disposition and reviewed role |
|---:|---|---|
| 101 | `TickAsync` | KEEP: Native inline/receiver representation: Named hot Promise/result shape for imported polling reads and detached scheduling. |
| 102 | `TickCallback` | KEEP: Native inline/receiver representation: Native public message handler callback accepting the original ticker root. |
| 103 | `TickHandlers` | KEEP: Native inline/receiver representation: Retained source handler array used with captured forEach length. |
| 104 | `TickHost` | KEEP: Native inline/receiver representation: Imported source receiver/store/read/emitter/timer capability boundary; sequential polling, callback iteration, disposal and catch/scheduling decisions remain in mapped tick. |
| 105 | `TickHost::emit_and_log_error` | KEEP: Concrete `binding/lifecycle.rs::TickHost::emit_and_log_error` supplies this imported call site; source policy remains in mapped caller. |
| 106 | `TickHost::emit_orders_fetch_error` | KEEP: Concrete `binding/lifecycle.rs::TickHost::emit_orders_fetch_error` supplies this imported call site; source policy remains in mapped caller. |
| 107 | `TickHost::fetch_balance` | KEEP: Concrete `binding/lifecycle.rs::TickHost::fetch_balance` supplies this imported call site; source policy remains in mapped caller. |
| 108 | `TickHost::fetch_open_orders_snapshot` | KEEP: Concrete `binding/lifecycle.rs::TickHost::fetch_open_orders_snapshot` supplies this imported call site; source policy remains in mapped caller. |
| 109 | `TickHost::fetch_positions` | KEEP: Concrete `binding/lifecycle.rs::TickHost::fetch_positions` supplies this imported call site; source policy remains in mapped caller. |
| 110 | `TickHost::fetch_tickers` | KEEP: Concrete `binding/lifecycle.rs::TickHost::fetch_tickers` supplies this imported call site; source policy remains in mapped caller. |
| 111 | `TickHost::get_tick_interval` | KEEP: Concrete `binding/lifecycle.rs::TickHost::get_tick_interval` supplies this imported call site; source policy remains in mapped caller. |
| 112 | `TickHost::is_disposed` | KEEP: Concrete `binding/lifecycle.rs::TickHost::is_disposed` supplies this imported call site; source policy remains in mapped caller. |
| 113 | `TickHost::loaded` | KEEP: Concrete `binding/lifecycle.rs::TickHost::loaded` supplies this imported call site; source policy remains in mapped caller. |
| 114 | `TickHost::loop_again` | KEEP: Concrete `binding/lifecycle.rs::TickHost::loop_again` supplies this imported call site; source policy remains in mapped caller. |
| 115 | `TickHost::merge_rest_orders_with_fresh_ws_state` | KEEP: Concrete `binding/lifecycle.rs::TickHost::merge_rest_orders_with_fresh_ws_state` supplies this imported call site; source policy remains in mapped caller. |
| 116 | `TickHost::public_message_handlers` | KEEP: Concrete `binding/lifecycle.rs::TickHost::public_message_handlers` supplies this imported call site; source policy remains in mapped caller. |
| 117 | `TickHost::should_poll_orders` | KEEP: Concrete `binding/lifecycle.rs::TickHost::should_poll_orders` supplies this imported call site; source policy remains in mapped caller. |
| 118 | `TickHost::update` | KEEP: Concrete `binding/lifecycle.rs::TickHost::update` supplies this imported call site; source policy remains in mapped caller. |
| 119 | `TickLoopCallback` | KEEP: Native inline/receiver representation: Captured source tick callback passed to the imported timer loop. |
| 120 | `TickReceiver` | KEEP: Native inline/receiver representation: Receiver for source polling store/read/emitter/timer operations and Promise scheduler. |
| 121 | `TickRegistry` | KEEP: Native inline/receiver representation: Retained source messageHandlers record with UTF16 topic keys. |
| 122 | `TickUpdate` | KEEP: Native inline/receiver representation: Named anonymous source store.update record retaining actual fetched value roots. |

## `exchange_ws_auth.rs`

Source owner: `coinbase.exchange.ts signWsAuth`. Concrete route: `binding/exchange.rs::ExchangeWsAuthHost`. Witnesses under this crate: `order_sync_auth.rs; exchange_binding.rs`.

| Entry | Rust declaration | Disposition and reviewed role |
|---:|---|---|
| 123 | `ExchangeWsAuthAsync` | KEEP: Native inline/receiver representation: Named hot signing Promise/result shape including own undefined signature. |
| 124 | `ExchangeWsAuthHost` | KEEP: Native inline/receiver representation: Imported source live options/local signing capability boundary; id read timing, truthiness and anonymous return slots remain in mapped sign_ws_auth. |
| 125 | `ExchangeWsAuthHost::options` | KEEP: Concrete `binding/exchange.rs::ExchangeWsAuthHost::options` supplies this imported call site; source policy remains in mapped caller. |
| 126 | `ExchangeWsAuthHost::sign_local` | KEEP: Concrete `binding/exchange.rs::ExchangeWsAuthHost::sign_local` supplies this imported call site; source policy remains in mapped caller. |
| 127 | `ExchangeWsAuthReceiver` | KEEP: Native inline/receiver representation: Receiver grouping live source options, imported local signing and Promise scheduler. |
| 128 | `ExchangeWsAuthResponse` | KEEP: Native inline/receiver representation: Named anonymous source signWsAuth return record retaining id/error/signature values and own undefined presence. |

## `order_sync.rs`

Source owner: `coinbase.exchange.ts snapshots and coinbase.ws-private.ts user-order events`. Concrete route: `binding/orders.rs::OrderSnapshotHost; classes.rs retained Frame views; binding/sockets.rs::CoinbasePrivateClassHost`. Witnesses under this crate: `order_sync_auth.rs; class_private.rs; private_binding_frames.rs`.

| Entry | Rust declaration | Disposition and reviewed role |
|---:|---|---|
| 129 | `OrderPage` | KEEP: Native inline/receiver representation: Native HTTP response.data projection retaining original orders array, has_next and optional cursor. |
| 130 | `OrderPageQuery` | KEEP: Native inline/receiver representation: Named anonymous source page query projection, including absent cursor and type/expiry filters. |
| 131 | `OrderSnapshotHost` | KEEP: Native inline/receiver representation: Imported source receiver/supported-types/HTTP/market/mapper/clock/logger capability boundary; requests, pagination, dedup and diagnostic policy remain in mapped snapshot/wrapper. |
| 132 | `OrderSnapshotHost::emit_and_log_error` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 133 | `OrderSnapshotHost::emit_orders_fetch_error` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 134 | `OrderSnapshotHost::get_orders` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 135 | `OrderSnapshotHost::map_rest_order` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 136 | `OrderSnapshotHost::now_ms` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 137 | `OrderSnapshotHost::public_only` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 138 | `OrderSnapshotHost::resolve_market_by_product` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 139 | `OrderSnapshotHost::supported_types` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 140 | `OrderSnapshotReceiver` | KEEP: Native inline/receiver representation: Receiver for imported supported-type/HTTP/resolver/mapper/clock/logger operations and Promise scheduler. |
| 141 | `OrderSyncAsync` | KEEP: Native inline/receiver representation: Named hot Promise/result shape for source snapshot reads and imported helpers. |
| 142 | `PrivateOrderEvent` | KEEP: Native inline/receiver representation: Native source user-order event DTO projection retaining original raw orders array and type. |
| 143 | `PrivateOrderEventHost` | KEEP: Native inline/receiver representation: Imported source store/alias resolver/snapshot/logger/clock/emitter capability boundary; field evaluation, scaling/quarantine, callback order and fill reads remain in mapped event/wrapper. |
| 144 | `PrivateOrderEventHost::add_or_update_orders` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 145 | `PrivateOrderEventHost::apply_order_snapshot_batch` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 146 | `PrivateOrderEventHost::console_error` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 147 | `PrivateOrderEventHost::current_time_ms` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 148 | `PrivateOrderEventHost::emit_and_log_error` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 149 | `PrivateOrderEventHost::emit_fill` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 150 | `PrivateOrderEventHost::markets_by_id` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 151 | `PrivateOrderEventHost::merge_snapshot_orders` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 152 | `PrivateOrderEventHost::resolve_coinbase_market` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 153 | `PrivateOrderEventHost::snapshot_active` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 154 | `PrivateOrderEventRoot` | KEEP: Native inline/receiver representation: Retained source user-order event root permitting callback mutations before later reads. |
| 155 | `PrivateOrderFill` | KEEP: Native inline/receiver representation: Named anonymous emitter fill literal with native scalar price and amount. |
| 156 | `PrivateOrderMarkets` | KEEP: Native inline/receiver representation: Retained source marketsById record carrying original typed Market roots. |
| 157 | `SyncedOrder` | KEEP: Native inline/receiver representation: Native typed order fields paired with the original mutable raw info item, avoiding cloned/re-encoded info. |
| 158 | `SyncedOrderRoot` | KEEP: Native inline/receiver representation: Retained source mapped order identity handle passed through snapshot/store callbacks. |
| 159 | `SyncedOrders` | KEEP: Native inline/receiver representation: Retained source mapped order array carrying native typed fields and original info roots. |
| 397 | `PrivateOrderScalar` | KEEP: Generic native value/root views for the separately mapped private order-event body. |
| 398 | `PrivateOrderScalar::float` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 399 | `PrivateOrderScalar::text` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 400 | `PrivateOrderScalar::truthy` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 401 | `PrivateOrderScalar::is_text` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 402 | `PrivateOrderScalar::from_text` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 403 | `PrivateOrderInfo` | KEEP: Generic native value/root views for the separately mapped private order-event body. |
| 404 | `PrivateOrderInfo::Scalar` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 405 | `PrivateOrderInfo::property` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 406 | `PrivateOrderArray` | KEEP: Generic native value/root views for the separately mapped private order-event body. |
| 407 | `PrivateOrderArray::item` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 408 | `PrivateOrderArray::len` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 409 | `PrivateOrderArray::is_empty` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 410 | `PrivateOrderEventInput` | KEEP: Generic native value/root views for the separately mapped private order-event body. |
| 411 | `PrivateOrderEventInput::Array` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 412 | `PrivateOrderEventInput::orders` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 413 | `PrivateOrderEventInput::kind` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |

## `starbase.rs`

Source owner: `coinbase.starbase.ts jsonrpc literal`. Concrete route: `source-inert exported protocol`. Witnesses under this crate: `starbase.rs; protocol.rs`.

| Entry | Rust declaration | Disposition and reviewed role |
|---:|---|---|
| 160 | `CoinbaseStarbaseJsonRpcVersion` | INERT / KEEP: Singleton Rust representation of the inline TypeScript literal field `jsonrpc: '2.0'`; adds no independent protocol value or behavior. |

## `private_order_snapshot.rs`

Source owner: `coinbase.ws-private.ts snapshot fields/applyOrderSnapshotBatch`. Concrete route: `classes.rs snapshot receiver; binding/sockets.rs::publish_private_order_snapshot`. Witnesses under this crate: `class_private.rs; private_binding_frames.rs`.

| Entry | Rust declaration | Disposition and reviewed role |
|---:|---|---|
| 161 | `COINBASE_USER_ORDER_SNAPSHOT_BATCH_SIZE` | KEEP: Private Coinbase websocket batching constant represented by an unexported TypeScript module constant; behavior is covered by CoinbasePrivateWebsocket#applyOrderSnapshotBatch. |
| 162 | `CoinbasePrivateOrderSnapshot` | KEEP: State carrier extracted from CoinbasePrivateWebsocket private fields; adds no independent venue behavior, and its operations map to the source methods in coinbase.ws-private.ts.toml. |
| 368 | `CoinbasePrivateOrderSnapshot::has_pending_snapshot` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 369 | `CoinbasePrivateOrderSnapshot::pending_raw_order_count` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 414 | `CoinbaseSnapshotOrder` | KEEP: Native order-view interface used by the separately mapped retained snapshot Map to read the insertion-time ID without copying the order object; no independent SDK operation. |
| 415 | `CoinbaseSnapshotOrder::snapshot_order_id` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 416 | `CoinbasePrivateOrderSnapshot::pending_snapshot_orders` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |

## `starbase_rpc.rs`

Source owner: `coinbase.starbase.ts request/response pipeline`. Concrete route: `source-inert injected transport`. Witnesses under this crate: `rpc.rs; rpc_error_response_type.rs`.

| Entry | Rust declaration | Disposition and reviewed role |
|---:|---|---|
| 163 | `CoinbaseStarbaseJsonRpcParams::new` | INERT / KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 164 | `CoinbaseStarbaseJsonRpcParams::borrow` | INERT / KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 165 | `CoinbaseStarbaseJsonRpcParams::borrow_object_mut` | INERT / KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 166 | `CoinbaseStarbaseJsonRpcParams::shares_object` | INERT / KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 167 | `CoinbaseStarbaseRequestHandle` | INERT / KEEP: Rust shared request handle needed to preserve transport mutations observed by the mapped response parser; no standalone TypeScript handle declaration. |
| 168 | `CoinbaseStarbaseRequestHandle::new` | INERT / KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 169 | `CoinbaseStarbaseRequestHandle::borrow` | INERT / KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 170 | `CoinbaseStarbaseRequestHandle::borrow_mut` | INERT / KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 171 | `CoinbaseStarbaseRequestHandle::shares_request` | INERT / KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 172 | `CoinbaseStarbaseRequestError` | INERT / KEEP: Rust error sum and boxed local future plumbing for Promise rejection and success. |
| 173 | `CoinbaseStarbaseRpcFuture` | INERT / KEEP: Rust error sum and boxed local future plumbing for Promise rejection and success. |

## `utils.rs`

Source owner: `coinbase.utils.ts credential normalization/PEM decode`. Concrete route: `binding/crypto.rs; binding/api.rs::sign`. Witnesses under this crate: `credentials.rs; exchange_binding.rs credential admission`.

| Entry | Rust declaration | Disposition and reviewed role |
|---:|---|---|
| 174 | `CoinbaseSignatureAlgorithm` | KEEP: Native discriminator for the anonymous normalization result algorithm string; no independent TypeScript enum declaration or behavior. Credential organization-segment admission FIXED #677; PEM platform-error representation remains its existing mapped choice. |
| 175 | `NormalizedCoinbaseApiCredentials` | KEEP: Named native shape for normalizeCoinbaseApiCredentials anonymous result: algorithm, key name/secret and optional Ed25519 seed; no standalone TypeScript declaration or new behavior. Credential organization-segment admission FIXED #677; PEM platform-error representation remains its existing mapped choice. |
| 176 | `CoinbasePemDecodeError` | KEEP: Native unit rejection carrier for pemToPkcs8Der, whose mapped row already documents CHOSEN loss of platform DOMException name/message metadata; no extra behavior is exempted. Credential organization-segment admission FIXED #677; PEM platform-error representation remains its existing mapped choice. |

## `starbase_auth.rs`

Source owner: `coinbase.starbase-session.ts session and injected Promise operations`. Concrete route: `source-inert session; Promise scheduler also reused by live binding`. Witnesses under this crate: `auth_session.rs; completion.rs; binding/crypto.rs::spawner`.

| Entry | Rust declaration | Disposition and reviewed role |
|---:|---|---|
| 177 | `CoinbaseStarbaseLocalFuture` | KEEP: Native local Promise/host boundary, shared result handle or runtime-number/rejection carrier for the mapped Starbase authentication/session functions; no standalone TypeScript declaration. Starbase activation stays closed; live binding reuses this primitive representation. |
| 178 | `CoinbaseStarbasePromiseSpawner` | KEEP: Native local Promise/host boundary, shared result handle or runtime-number/rejection carrier for the mapped Starbase authentication/session functions; no standalone TypeScript declaration. Starbase activation stays closed; live binding reuses this primitive representation. |
| 179 | `CoinbaseStarbaseSessionError` | KEEP: Native local Promise/host boundary, shared result handle or runtime-number/rejection carrier for the mapped Starbase authentication/session functions; no standalone TypeScript declaration. Starbase activation stays closed; live binding reuses this primitive representation. |
| 180 | `CoinbaseStarbaseSessionFailure` | KEEP: Native local Promise/host boundary, shared result handle or runtime-number/rejection carrier for the mapped Starbase authentication/session functions; no standalone TypeScript declaration. Starbase activation stays closed; live binding reuses this primitive representation. |
| 181 | `CoinbaseStarbasePromise` | KEEP: Native local Promise/host boundary, shared result handle or runtime-number/rejection carrier for the mapped Starbase authentication/session functions; no standalone TypeScript declaration. Starbase activation stays closed; live binding reuses this primitive representation. |
| 182 | `CoinbaseStarbasePromiseResolver` | KEEP: Native local Promise/host boundary, shared result handle or runtime-number/rejection carrier for the mapped Starbase authentication/session functions; no standalone TypeScript declaration. Starbase activation stays closed; live binding reuses this primitive representation. |
| 183 | `CoinbaseStarbaseAuthResultHandle` | KEEP: Native local Promise/host boundary, shared result handle or runtime-number/rejection carrier for the mapped Starbase authentication/session functions; no standalone TypeScript declaration. Starbase activation stays closed; live binding reuses this primitive representation. |
| 184 | `CoinbaseStarbaseAuthPromise` | KEEP: Native local Promise/host boundary, shared result handle or runtime-number/rejection carrier for the mapped Starbase authentication/session functions; no standalone TypeScript declaration. Starbase activation stays closed; live binding reuses this primitive representation. |
| 185 | `CoinbaseStarbaseClock` | KEEP: Native local Promise/host boundary, shared result handle or runtime-number/rejection carrier for the mapped Starbase authentication/session functions; no standalone TypeScript declaration. Starbase activation stays closed; live binding reuses this primitive representation. |
| 186 | `CoinbaseStarbaseRefreshInterval` | KEEP: Native local Promise/host boundary, shared result handle or runtime-number/rejection carrier for the mapped Starbase authentication/session functions; no standalone TypeScript declaration. Starbase activation stays closed; live binding reuses this primitive representation. |
| 187 | `CoinbaseStarbasePromise::pending` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. Starbase activation stays closed; live binding reuses this primitive representation. |
| 188 | `CoinbaseStarbasePromise::resolved` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. Starbase activation stays closed; live binding reuses this primitive representation. |
| 189 | `CoinbaseStarbasePromise::same_promise` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. Starbase activation stays closed; live binding reuses this primitive representation. |
| 190 | `CoinbaseStarbasePromiseResolver::resolve` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. Starbase activation stays closed; live binding reuses this primitive representation. |
| 191 | `CoinbaseStarbaseAuthResultHandle::borrow` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. Starbase activation stays closed; live binding reuses this primitive representation. |
| 192 | `CoinbaseStarbaseAuthResultHandle::borrow_mut` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. Starbase activation stays closed; live binding reuses this primitive representation. |
| 193 | `CoinbaseStarbaseAuthResultHandle::shares_result` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. Starbase activation stays closed; live binding reuses this primitive representation. |

## `starbase_ws_controller.rs`

Source owner: `coinbase.starbase-ws-session.ts activation/replay/state`. Concrete route: `source-inert controller`. Witnesses under this crate: `ws_controller.rs`.

| Entry | Rust declaration | Disposition and reviewed role |
|---:|---|---|
| 194 | `CoinbaseStarbaseActivationPromise` | INERT / KEEP: Native completion/replay signature or accessor for the source public state property on the mapped WebSocket controller; no separate source callable or type declaration. |
| 195 | `CoinbaseStarbaseReplaySubscriptions` | INERT / KEEP: Native completion/replay signature or accessor for the source public state property on the mapped WebSocket controller; no separate source callable or type declaration. |
| 196 | `CoinbaseStarbaseWebsocketSessionController::state` | INERT / KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 197 | `CoinbaseStarbaseWebsocketSessionController::set_state` | INERT / KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |

## `json_key_scanner.rs`

Source owner: `coinbase.starbase-public-frames.ts scanner result`. Concrete route: `source-inert Starbase frame classifier`. Witnesses under this crate: `json_key_scanner.rs; frame_classification.rs`.

| Entry | Rust declaration | Disposition and reviewed role |
|---:|---|---|
| 198 | `JsonKeyScanResult` | INERT / KEEP: Native tag for the scanner internal JsonKeyScanResult union, which has no extracted source row. |

## `order_create.rs`

Source owner: `coinbase.exchange.ts create/cancel/reconciliation fields`. Concrete route: `binding/orders.rs::CoinbaseOrderHost`. Witnesses under this crate: `order_create.rs; exchange_binding.rs`.

| Entry | Rust declaration | Disposition and reviewed role |
|---:|---|---|
| 199 | `CoinbaseOrderFailure` | KEEP: Rust receiver capability or inline native carrier for mapped Coinbase create/cancel/REST fallback field bodies. |
| 200 | `CoinbaseOrderPolling` | KEEP: Rust receiver capability or inline native carrier for mapped Coinbase create/cancel/REST fallback field bodies. |
| 201 | `CoinbaseOrderResponse` | KEEP: Rust receiver capability or inline native carrier for mapped Coinbase create/cancel/REST fallback field bodies. |
| 202 | `CoinbaseOrderResponse::from_json` | KEEP: Concrete `binding/orders.rs::CoinbaseOrderHost::from_json` supplies this imported call site; source policy remains in mapped caller. |
| 203 | `CoinbaseOptimisticOrder` | KEEP: Rust receiver capability or inline native carrier for mapped Coinbase create/cancel/REST fallback field bodies. |
| 204 | `CoinbaseOrderHost` | KEEP: Rust receiver capability or inline native carrier for mapped Coinbase create/cancel/REST fallback field bodies. |
| 205 | `CoinbaseOrderHost::now_ms` | KEEP: Concrete `binding/orders.rs::CoinbaseOrderHost::now_ms` supplies this imported call site; source policy remains in mapped caller. |
| 206 | `CoinbaseOrderHost::orders_loaded` | KEEP: Concrete `binding/orders.rs::CoinbaseOrderHost::orders_loaded` supplies this imported call site; source policy remains in mapped caller. |
| 207 | `CoinbaseOrderHost::orders` | KEEP: Concrete `binding/orders.rs::CoinbaseOrderHost::orders` supplies this imported call site; source policy remains in mapped caller. |
| 208 | `CoinbaseOrderHost::market_by_symbol` | KEEP: Concrete `binding/orders.rs::CoinbaseOrderHost::market_by_symbol` supplies this imported call site; source policy remains in mapped caller. |
| 209 | `CoinbaseOrderHost::markets_by_id` | KEEP: Concrete `binding/orders.rs::CoinbaseOrderHost::markets_by_id` supplies this imported call site; source policy remains in mapped caller. |
| 210 | `CoinbaseOrderHost::resolve_caller_client_order_id` | KEEP: Concrete `binding/orders.rs::CoinbaseOrderHost::resolve_caller_client_order_id` supplies this imported call site; source policy remains in mapped caller. |
| 211 | `CoinbaseOrderHost::uuid` | KEEP: FIXED #671: concrete callback now uses shared nanoid21/separator-removal UUID helper; actual signed JWT, POST and mint witnesses. |
| 212 | `CoinbaseOrderHost::post_create_order` | KEEP: Concrete `binding/orders.rs::CoinbaseOrderHost::post_create_order` supplies this imported call site; source policy remains in mapped caller. |
| 213 | `CoinbaseOrderHost::post_cancel_orders` | KEEP: Concrete `binding/orders.rs::CoinbaseOrderHost::post_cancel_orders` supplies this imported call site; source policy remains in mapped caller. |
| 214 | `CoinbaseOrderHost::add_or_update_order` | KEEP: Concrete `binding/orders.rs::CoinbaseOrderHost::add_or_update_order` supplies this imported call site; source policy remains in mapped caller. |
| 215 | `CoinbaseOrderHost::update_orders_status` | KEEP: Concrete `binding/orders.rs::CoinbaseOrderHost::update_orders_status` supplies this imported call site; source policy remains in mapped caller. |
| 216 | `CoinbaseOrderHost::schedule_open_orders_rest_reconciliation` | KEEP: Concrete `binding/orders.rs::CoinbaseOrderHost::schedule_open_orders_rest_reconciliation` supplies this imported call site; source policy remains in mapped caller. |
| 217 | `CoinbaseOrderHost::schedule_private_user_stream_write_check` | KEEP: Concrete `binding/orders.rs::CoinbaseOrderHost::schedule_private_user_stream_write_check` supplies this imported call site; source policy remains in mapped caller. |
| 218 | `CoinbaseOrderHost::log_warning` | KEEP: Concrete `binding/orders.rs::CoinbaseOrderHost::log_warning` supplies this imported call site; source policy remains in mapped caller. |
| 219 | `CoinbaseOrderHost::emit_and_log_error` | KEEP: Concrete `binding/orders.rs::CoinbaseOrderHost::emit_and_log_error` supplies this imported call site; source policy remains in mapped caller. |

## `starbase_public_frames.rs`

Source owner: `coinbase.starbase-public-frames.ts JSON/frame classification`. Concrete route: `source-inert classifier; Frame scalar/container representation also used by live binding`. Witnesses under this crate: `frame_classification.rs; public_market_data.rs; class_private.rs`.

| Entry | Rust declaration | Disposition and reviewed role |
|---:|---|---|
| 220 | `CoinbaseStarbaseFrameValue` | KEEP: Native parsed-JSON representation or closed inline DTO branch for the mapped public frame classification. Starbase activation stays closed; live binding reuses this primitive representation. |
| 221 | `CoinbaseStarbaseFrameValue::from_json` | KEEP: Concrete `source-inert classifier; Frame scalar/container representation also used by live binding::from_json` supplies this imported call site; source policy remains in mapped caller. Starbase activation stays closed; live binding reuses this primitive representation. |
| 222 | `CoinbaseStarbaseFrameValue::get` | KEEP: Concrete `source-inert classifier; Frame scalar/container representation also used by live binding::get` supplies this imported call site; source policy remains in mapped caller. Starbase activation stays closed; live binding reuses this primitive representation. |
| 223 | `CoinbaseStarbaseFrameValue::own_keys` | KEEP: Concrete `source-inert classifier; Frame scalar/container representation also used by live binding::own_keys` supplies this imported call site; source policy remains in mapped caller. Starbase activation stays closed; live binding reuses this primitive representation. |
| 224 | `CoinbaseStarbaseUnknownFrameReason` | KEEP: Native parsed-JSON representation or closed inline DTO branch for the mapped public frame classification. Starbase activation stays closed; live binding reuses this primitive representation. |
| 225 | `CoinbaseStarbaseUnsupportedSubscriptionReason` | KEEP: Native parsed-JSON representation or closed inline DTO branch for the mapped public frame classification. Starbase activation stays closed; live binding reuses this primitive representation. |
| 226 | `CoinbaseStarbaseFrameClassificationScope` | KEEP: Native parsed-JSON representation or closed inline DTO branch for the mapped public frame classification. Starbase activation stays closed; live binding reuses this primitive representation. |
| 227 | `CoinbaseStarbasePublicSubscriptionPayload` | KEEP: Native parsed-JSON representation or closed inline DTO branch for the mapped public frame classification. Starbase activation stays closed; live binding reuses this primitive representation. |
| 228 | `CoinbaseStarbasePublicTestError` | KEEP: Native parsed-JSON representation or closed inline DTO branch for the mapped public frame classification. Starbase activation stays closed; live binding reuses this primitive representation. |
| 229 | `CoinbaseStarbasePublicTestOutcome` | KEEP: Native parsed-JSON representation or closed inline DTO branch for the mapped public frame classification. Starbase activation stays closed; live binding reuses this primitive representation. |

## `portfolios_balances.rs`

Source owner: `coinbase.exchange.ts and coinbasespot.exchange.ts portfolio/balance fields`. Concrete route: `binding/reads.rs::PortfolioHost; classes.rs::PortfolioClassBinding`. Witnesses under this crate: `portfolios_balances.rs; exchange_binding.rs`.

| Entry | Rust declaration | Disposition and reviewed role |
|---:|---|---|
| 230 | `PortfolioState` | KEEP: Rust receiver storage behind the Portfolio handle: carries the uuid/type fields observed by mapped portfolio selection and retains unused response metadata. |
| 231 | `Portfolio` | KEEP: Native inline return/receiver and external call-site capability for the ten mapped portfolio/balance bodies. |
| 232 | `Portfolios` | KEEP: Native inline return/receiver and external call-site capability for the ten mapped portfolio/balance bodies. |
| 233 | `PortfolioFailure` | KEEP: Native inline return/receiver and external call-site capability for the ten mapped portfolio/balance bodies. |
| 234 | `PortfolioThrown` | KEEP: Native inline return/receiver and external call-site capability for the ten mapped portfolio/balance bodies. |
| 235 | `PortfolioReply` | KEEP: Native inline return/receiver and external call-site capability for the ten mapped portfolio/balance bodies. |
| 236 | `Balance` | KEEP: Native inline return/receiver and external call-site capability for the ten mapped portfolio/balance bodies. |
| 237 | `Currencies` | KEEP: Native inline return/receiver and external call-site capability for the ten mapped portfolio/balance bodies. |
| 238 | `PortfolioBalance` | KEEP: Native inline return/receiver and external call-site capability for the ten mapped portfolio/balance bodies. |
| 239 | `PortfolioCurrencies` | KEEP: Native inline return/receiver and external call-site capability for the ten mapped portfolio/balance bodies. |
| 240 | `PortfolioHost` | KEEP: Native inline return/receiver and external call-site capability for the ten mapped portfolio/balance bodies. |
| 241 | `PortfolioHost::get` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 242 | `PortfolioHost::emit_and_log_error` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 243 | `PortfolioHost::cached_balance` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 244 | `PortfolioHost::position_upnls` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 245 | `PortfolioHost::supported_types_override` | KEEP: Optional standalone injected override; assembled class uses PortfolioClassBinding/current captured field, not a second production supported-types policy. |
| 246 | `PortfolioReceiver` | KEEP: Native inline return/receiver and external call-site capability for the ten mapped portfolio/balance bodies. |
| 380 | `SupportedTypes` | KEEP: Retained mutable carrier for the inline array returned by the separately mapped base and Spot getSupportedTypes bodies; names its Rc/RefCell representation and adds no independent behavior. |
| 381 | `PortfolioClassBinding` | KEEP: Receiver-view interface joining an existing portfolio body to the actual class options/support/selection and current bound fields. |
| 382 | `PortfolioClassBinding::public_only` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 383 | `PortfolioClassBinding::support_spot` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 384 | `PortfolioClassBinding::find` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 385 | `PortfolioClassBinding::supported` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 418 | `PortfolioThrown::from_value` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 419 | `PortfolioThrown::store_mutation` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |

## `market_position_reads.rs`

Source owner: `coinbase.exchange.ts ticker/position reads`. Concrete route: `binding/reads.rs::MarketPositionHost`. Witnesses under this crate: `ticker_position_reads.rs; exchange_binding.rs`.

| Entry | Rust declaration | Disposition and reviewed role |
|---:|---|---|
| 247 | `ReadItem` | KEEP: Rust inline scalar-field/retained-info and receiver representation or source HTTP/Store/supported-type/timezone operation boundary for the nine mapped ticker/position reads. |
| 248 | `ReadMarket` | KEEP: Rust inline scalar-field/retained-info and receiver representation or source HTTP/Store/supported-type/timezone operation boundary for the nine mapped ticker/position reads. |
| 249 | `ReadMarkets` | KEEP: Rust inline scalar-field/retained-info and receiver representation or source HTTP/Store/supported-type/timezone operation boundary for the nine mapped ticker/position reads. |
| 250 | `ReadTicker` | KEEP: Rust inline scalar-field/retained-info and receiver representation or source HTTP/Store/supported-type/timezone operation boundary for the nine mapped ticker/position reads. |
| 251 | `ReadPosition` | KEEP: Rust inline scalar-field/retained-info and receiver representation or source HTTP/Store/supported-type/timezone operation boundary for the nine mapped ticker/position reads. |
| 252 | `ReadTickers` | KEEP: Rust inline scalar-field/retained-info and receiver representation or source HTTP/Store/supported-type/timezone operation boundary for the nine mapped ticker/position reads. |
| 253 | `ReadPositions` | KEEP: Rust inline scalar-field/retained-info and receiver representation or source HTTP/Store/supported-type/timezone operation boundary for the nine mapped ticker/position reads. |
| 254 | `ReadReply` | KEEP: Rust inline scalar-field/retained-info and receiver representation or source HTTP/Store/supported-type/timezone operation boundary for the nine mapped ticker/position reads. |
| 255 | `MarketPositionHost` | KEEP: Rust inline scalar-field/retained-info and receiver representation or source HTTP/Store/supported-type/timezone operation boundary for the nine mapped ticker/position reads. |
| 256 | `MarketPositionHost::get` | KEEP: Concrete `binding/reads.rs::MarketPositionHost::get` supplies this imported call site; source policy remains in mapped caller. |
| 257 | `MarketPositionHost::error_response_items` | KEEP: Concrete `binding/reads.rs::MarketPositionHost::error_response_items` supplies this imported call site; source policy remains in mapped caller. |
| 258 | `MarketPositionHost::get_supported_types` | KEEP: Concrete `binding/reads.rs::MarketPositionHost::get_supported_types` supplies this imported call site; source policy remains in mapped caller. |
| 259 | `MarketPositionHost::markets` | KEEP: Concrete `binding/reads.rs::MarketPositionHost::markets` supplies this imported call site; source policy remains in mapped caller. |
| 260 | `MarketPositionHost::market_by_id` | KEEP: Concrete `binding/reads.rs::MarketPositionHost::market_by_id` supplies this imported call site; source policy remains in mapped caller. |
| 261 | `MarketPositionHost::cached_tickers` | KEEP: Concrete `binding/reads.rs::MarketPositionHost::cached_tickers` supplies this imported call site; source policy remains in mapped caller. |
| 262 | `MarketPositionHost::cached_positions` | KEEP: Concrete `binding/reads.rs::MarketPositionHost::cached_positions` supplies this imported call site; source policy remains in mapped caller. |
| 263 | `MarketPositionHost::emit_and_log_error` | KEEP: Concrete `binding/reads.rs::MarketPositionHost::emit_and_log_error` supplies this imported call site; source policy remains in mapped caller. |
| 264 | `MarketPositionHost::local_wall_time_to_utc` | KEEP: Concrete `binding/reads.rs::MarketPositionHost::local_wall_time_to_utc` supplies this imported call site; source policy remains in mapped caller. |
| 265 | `MarketPositionReceiver` | KEEP: Rust inline scalar-field/retained-info and receiver representation or source HTTP/Store/supported-type/timezone operation boundary for the nine mapped ticker/position reads. |
| 266 | `ReadItems` | KEEP: Original declared response array retained by native projection; CFM for-of reads its live length and each item retains source info identity. |

## `public_stream_dispatch.rs`

Source owner: `coinbase.ws-public.ts socket/event routing`. Concrete route: `binding/sockets.rs::PublicStreamHost`. Witnesses under this crate: `public_stream_dispatch.rs; public_market_data.rs; exchange_binding.rs`.

| Entry | Rust declaration | Disposition and reviewed role |
|---:|---|---|
| 267 | `PublicSocket` | KEEP: Native receiver/inline callback, socket, payload, failure and retained-frame binding for eight mapped Coinbase public websocket bodies; host exposes existing BaseWebSocket/store/imported JSON.parse operations at their call sites. |
| 268 | `PublicSocketListener` | KEEP: Native receiver/inline callback, socket, payload, failure and retained-frame binding for eight mapped Coinbase public websocket bodies; host exposes existing BaseWebSocket/store/imported JSON.parse operations at their call sites. |
| 269 | `PublicStreamCallback` | KEEP: Native receiver/inline callback, socket, payload, failure and retained-frame binding for eight mapped Coinbase public websocket bodies; host exposes existing BaseWebSocket/store/imported JSON.parse operations at their call sites. |
| 270 | `PublicStreamFailure` | KEEP: Native receiver/inline callback, socket, payload, failure and retained-frame binding for eight mapped Coinbase public websocket bodies; host exposes existing BaseWebSocket/store/imported JSON.parse operations at their call sites. |
| 271 | `PublicStreamHandler` | KEEP: Native receiver/inline callback, socket, payload, failure and retained-frame binding for eight mapped Coinbase public websocket bodies; host exposes existing BaseWebSocket/store/imported JSON.parse operations at their call sites. |
| 272 | `PublicStreamHandlers` | KEEP: Native receiver/inline callback, socket, payload, failure and retained-frame binding for eight mapped Coinbase public websocket bodies; host exposes existing BaseWebSocket/store/imported JSON.parse operations at their call sites. |
| 273 | `PublicStreamHost` | KEEP: Native receiver/inline callback, socket, payload, failure and retained-frame binding for eight mapped Coinbase public websocket bodies; host exposes existing BaseWebSocket/store/imported JSON.parse operations at their call sites. |
| 274 | `PublicStreamHost::add_listener` | KEEP: Concrete `binding/sockets.rs::PublicStreamHost::add_listener` supplies this imported call site; source policy remains in mapped caller. |
| 275 | `PublicStreamHost::console_error_frame` | KEEP: Concrete `binding/sockets.rs::PublicStreamHost::console_error_frame` supplies this imported call site; source policy remains in mapped caller. |
| 276 | `PublicStreamHost::console_error_text` | KEEP: Concrete `binding/sockets.rs::PublicStreamHost::console_error_text` supplies this imported call site; source policy remains in mapped caller. |
| 277 | `PublicStreamHost::create_websocket` | KEEP: Concrete `binding/sockets.rs::PublicStreamHost::create_websocket` supplies this imported call site; source policy remains in mapped caller. |
| 278 | `PublicStreamHost::emit_error` | KEEP: Concrete `binding/sockets.rs::PublicStreamHost::emit_error` supplies this imported call site; source policy remains in mapped caller. |
| 279 | `PublicStreamHost::emit_ws_reconnected` | KEEP: Concrete `binding/sockets.rs::PublicStreamHost::emit_ws_reconnected` supplies this imported call site; source policy remains in mapped caller. |
| 280 | `PublicStreamHost::handle_pong_event` | KEEP: Concrete `binding/sockets.rs::PublicStreamHost::handle_pong_event` supplies this imported call site; source policy remains in mapped caller. |
| 281 | `PublicStreamHost::is_disposed` | KEEP: Concrete `binding/sockets.rs::PublicStreamHost::is_disposed` supplies this imported call site; source policy remains in mapped caller. |
| 282 | `PublicStreamHost::json_parse` | KEEP: Concrete `binding/sockets.rs::PublicStreamHost::json_parse` supplies this imported call site; source policy remains in mapped caller. |
| 283 | `PublicStreamHost::markets_symbols` | KEEP: Concrete `binding/sockets.rs::PublicStreamHost::markets_symbols` supplies this imported call site; source policy remains in mapped caller. |
| 284 | `PublicStreamHost::reconnecting_enabled` | KEEP: Concrete `binding/sockets.rs::PublicStreamHost::reconnecting_enabled` supplies this imported call site; source policy remains in mapped caller. |
| 285 | `PublicStreamHost::record_message_handler_stats` | KEEP: Concrete `binding/sockets.rs::PublicStreamHost::record_message_handler_stats` supplies this imported call site; source policy remains in mapped caller. |
| 286 | `PublicStreamHost::reset_orderbook` | KEEP: Concrete `binding/sockets.rs::PublicStreamHost::reset_orderbook` supplies this imported call site; source policy remains in mapped caller. |
| 287 | `PublicStreamHost::send_subscription` | KEEP: Concrete `binding/sockets.rs::PublicStreamHost::send_subscription` supplies this imported call site; source policy remains in mapped caller. |
| 288 | `PublicStreamHost::testnet` | KEEP: Concrete `binding/sockets.rs::PublicStreamHost::testnet` supplies this imported call site; source policy remains in mapped caller. |
| 289 | `PublicStreamHost::track_last_received` | KEEP: Concrete `binding/sockets.rs::PublicStreamHost::track_last_received` supplies this imported call site; source policy remains in mapped caller. |
| 290 | `PublicStreamMessage` | KEEP: Native receiver/inline callback, socket, payload, failure and retained-frame binding for eight mapped Coinbase public websocket bodies; host exposes existing BaseWebSocket/store/imported JSON.parse operations at their call sites. |
| 291 | `PublicStreamReceiver` | KEEP: Native receiver/inline callback, socket, payload, failure and retained-frame binding for eight mapped Coinbase public websocket bodies; host exposes existing BaseWebSocket/store/imported JSON.parse operations at their call sites. |
| 292 | `PublicStreamReceiver::new` | KEEP: Concrete `binding/sockets.rs::PublicStreamHost::new` supplies this imported call site; source policy remains in mapped caller. |
| 293 | `PublicSubscription` | KEEP: Native receiver/inline callback, socket, payload, failure and retained-frame binding for eight mapped Coinbase public websocket bodies; host exposes existing BaseWebSocket/store/imported JSON.parse operations at their call sites. |

## `private_stream_dispatch.rs`

Source owner: `coinbase.ws-private.ts socket/auth/event routing`. Concrete route: `binding/sockets.rs::PrivateStreamHost`. Witnesses under this crate: `private_stream_dispatch.rs; class_private.rs; private_binding_frames.rs`.

| Entry | Rust declaration | Disposition and reviewed role |
|---:|---|---|
| 294 | `PrivateOptions` | KEEP: Native receiver/options/signer/socket/payload/host representation for nine mapped private websocket bodies. |
| 295 | `PrivateRemoteSigner` | KEEP: Native receiver/options/signer/socket/payload/host representation for nine mapped private websocket bodies. |
| 296 | `PrivateStreamHost` | KEEP: Native receiver/options/signer/socket/payload/host representation for nine mapped private websocket bodies. |
| 297 | `PrivateStreamHost::add_listener` | KEEP: Concrete `binding/sockets.rs::PrivateStreamHost::add_listener` supplies this imported call site; source policy remains in mapped caller. |
| 298 | `PrivateStreamHost::close_socket` | KEEP: Concrete `binding/sockets.rs::PrivateStreamHost::close_socket` supplies this imported call site; source policy remains in mapped caller. |
| 299 | `PrivateStreamHost::create_websocket` | KEEP: Concrete `binding/sockets.rs::PrivateStreamHost::create_websocket` supplies this imported call site; source policy remains in mapped caller. |
| 300 | `PrivateStreamHost::emit_and_log_error` | KEEP: Concrete `binding/sockets.rs::PrivateStreamHost::emit_and_log_error` supplies this imported call site; source policy remains in mapped caller. |
| 301 | `PrivateStreamHost::emit_ws_diagnostic_receive` | KEEP: Concrete `binding/sockets.rs::PrivateStreamHost::emit_ws_diagnostic_receive` supplies this imported call site; source policy remains in mapped caller. |
| 302 | `PrivateStreamHost::exchange_name` | KEEP: Concrete `binding/sockets.rs::PrivateStreamHost::exchange_name` supplies this imported call site; source policy remains in mapped caller. |
| 303 | `PrivateStreamHost::handle_pong_event` | KEEP: Concrete `binding/sockets.rs::PrivateStreamHost::handle_pong_event` supplies this imported call site; source policy remains in mapped caller. |
| 304 | `PrivateStreamHost::handle_user_event_orders` | KEEP: Concrete `binding/sockets.rs::PrivateStreamHost::handle_user_event_orders` supplies this imported call site; source policy remains in mapped caller. |
| 305 | `PrivateStreamHost::is_disposed` | KEEP: Concrete `binding/sockets.rs::PrivateStreamHost::is_disposed` supplies this imported call site; source policy remains in mapped caller. |
| 306 | `PrivateStreamHost::json_parse` | KEEP: Concrete `binding/sockets.rs::PrivateStreamHost::json_parse` supplies this imported call site; source policy remains in mapped caller. |
| 307 | `PrivateStreamHost::mark_private_user_stream_auth_failed` | KEEP: FIXED #691 (merged `3b1e5560f3`): log/warning throws propagate through existing native carrier; real timer/auth frame/latch witnesses. |
| 308 | `PrivateStreamHost::mark_private_user_stream_message_received` | KEEP: Concrete `binding/sockets.rs::PrivateStreamHost::mark_private_user_stream_message_received` supplies this imported call site; source policy remains in mapped caller. |
| 309 | `PrivateStreamHost::options` | KEEP: Concrete `binding/sockets.rs::PrivateStreamHost::options` supplies this imported call site; source policy remains in mapped caller. |
| 310 | `PrivateStreamHost::reset_order_snapshot_assembly` | KEEP: Concrete `binding/sockets.rs::PrivateStreamHost::reset_order_snapshot_assembly` supplies this imported call site; source policy remains in mapped caller. |
| 311 | `PrivateStreamHost::resolve_private_route` | KEEP: Concrete `binding/sockets.rs::PrivateStreamHost::resolve_private_route` supplies this imported call site; source policy remains in mapped caller. |
| 312 | `PrivateStreamHost::send_with_log` | KEEP: Concrete `binding/sockets.rs::PrivateStreamHost::send_with_log` supplies this imported call site; source policy remains in mapped caller. |
| 313 | `PrivateStreamHost::sign_local` | KEEP: Concrete `binding/sockets.rs::PrivateStreamHost::sign_local` supplies this imported call site; source policy remains in mapped caller. |
| 314 | `PrivateStreamHost::testnet` | KEEP: Concrete `binding/sockets.rs::PrivateStreamHost::testnet` supplies this imported call site; source policy remains in mapped caller. |
| 315 | `PrivateStreamHost::track_last_received` | KEEP: Concrete `binding/sockets.rs::PrivateStreamHost::track_last_received` supplies this imported call site; source policy remains in mapped caller. |
| 316 | `PrivateStreamOptions` | KEEP: Native receiver/options/signer/socket/payload/host representation for nine mapped private websocket bodies. |
| 317 | `PrivateStreamReceiver` | KEEP: Native receiver/options/signer/socket/payload/host representation for nine mapped private websocket bodies. |
| 318 | `PrivateStreamRoute` | KEEP: Native receiver/options/signer/socket/payload/host representation for nine mapped private websocket bodies. |
| 319 | `PrivateSubscription` | KEEP: Native receiver/options/signer/socket/payload/host representation for nine mapped private websocket bodies. |
| 320 | `PrivateWsAuthRequest` | KEEP: Native receiver/options/signer/socket/payload/host representation for nine mapped private websocket bodies. |

## `api_factory.rs`

Source owner: `coinbase.api.ts createAPI interceptor`. Concrete route: `binding/api.rs::CoinbaseApiHost`. Witnesses under this crate: `completion.rs; transport_binding.rs`.

| Entry | Rust declaration | Disposition and reviewed role |
|---:|---|---|
| 321 | `ApiCreateConfig` | KEEP: Native own-property request/config or bound callable receiver representation for mapped createAPI; retains shallow roots/undefined slots without a new SDK body. |
| 322 | `ApiInterceptor` | KEEP: Native own-property request/config or bound callable receiver representation for mapped createAPI; retains shallow roots/undefined slots without a new SDK body. |
| 323 | `ApiRequest` | KEEP: Native own-property request/config or bound callable receiver representation for mapped createAPI; retains shallow roots/undefined slots without a new SDK body. |
| 324 | `ApiSerializer` | KEEP: Native own-property request/config or bound callable receiver representation for mapped createAPI; retains shallow roots/undefined slots without a new SDK body. |
| 325 | `CoinbaseApiHost` | KEEP: Native imported HTTP factory/header/transport/retry/signer capability for mapped createAPI; associated client retains the actual leaf result. |
| 326 | `CoinbaseApiHost::Client` | KEEP: Concrete `binding/api.rs::CoinbaseApiHost::Client` supplies this imported call site; source policy remains in mapped caller. |
| 327 | `CoinbaseApiHost::headers` | KEEP: Concrete `binding/api.rs::CoinbaseApiHost::headers` supplies this imported call site; source policy remains in mapped caller. |
| 328 | `CoinbaseApiHost::install_interceptor` | KEEP: Concrete `binding/api.rs::CoinbaseApiHost::install_interceptor` supplies this imported call site; source policy remains in mapped caller. |
| 329 | `CoinbaseApiHost::install_transport_adapter` | KEEP: Concrete `binding/api.rs::CoinbaseApiHost::install_transport_adapter` supplies this imported call site; source policy remains in mapped caller. |
| 330 | `CoinbaseApiHost::mark_skip_remote_signing` | KEEP: Concrete `binding/api.rs::CoinbaseApiHost::mark_skip_remote_signing` supplies this imported call site; source policy remains in mapped caller. |
| 331 | `CoinbaseApiHost::network_error_on_read` | KEEP: Concrete `binding/api.rs::CoinbaseApiHost::network_error_on_read` supplies this imported call site; source policy remains in mapped caller. |
| 332 | `CoinbaseApiHost::proxy_prefix` | KEEP: Concrete `binding/api.rs::CoinbaseApiHost::proxy_prefix` supplies this imported call site; source policy remains in mapped caller. |
| 333 | `CoinbaseApiHost::replace_url_params` | KEEP: Concrete `binding/api.rs::CoinbaseApiHost::replace_url_params` supplies this imported call site; source policy remains in mapped caller. |
| 334 | `CoinbaseApiHost::retry` | KEEP: Concrete `binding/api.rs::CoinbaseApiHost::retry` supplies this imported call site; source policy remains in mapped caller. |
| 335 | `CoinbaseApiHost::safe_create` | KEEP: Concrete `binding/api.rs::CoinbaseApiHost::safe_create` supplies this imported call site; source policy remains in mapped caller. |
| 336 | `CoinbaseApiHost::serialize_repeat` | KEEP: Concrete `binding/api.rs::CoinbaseApiHost::serialize_repeat` supplies this imported call site; source policy remains in mapped caller. |
| 337 | `CoinbaseApiHost::sign` | KEEP: Concrete `binding/api.rs::CoinbaseApiHost::sign` supplies this imported call site; source policy remains in mapped caller. |
| 338 | `CoinbaseApiReceiver` | KEEP: Native own-property request/config or bound callable receiver representation for mapped createAPI; retains shallow roots/undefined slots without a new SDK body. |

## `classes.rs`

Source owner: `coinbase.exchange.ts / coinbasespot.exchange.ts / websocket constructors and arrow fields`. Concrete route: `binding/classes.rs::CoinbaseConstructionHost; binding/sockets.rs::CoinbasePrivateClassHost`. Witnesses under this crate: `public_market_data.rs; class_private.rs; portfolios_balances.rs; exchange_binding.rs`.

| Entry | Rust declaration | Disposition and reviewed role |
|---:|---|---|
| 339 | `ClassApiFactory` | KEEP: Native bound callable or opaque Base receiver identity carrier for the mapped constructor; remaining dynamic receiver coupling is recorded on the class rows. |
| 340 | `ClassIdentity` | KEEP: Native bound callable or opaque Base receiver identity carrier for the mapped constructor; remaining dynamic receiver coupling is recorded on the class rows. |
| 341 | `ClassMint` | KEEP: Native bound callable or opaque Base receiver identity carrier for the mapped constructor; remaining dynamic receiver coupling is recorded on the class rows. |
| 342 | `ClassRateLimit` | KEEP: Native bound callable or opaque Base receiver identity carrier for the mapped constructor; remaining dynamic receiver coupling is recorded on the class rows. |
| 343 | `CoinbaseConstructionHost` | KEEP: Native imported BaseExchange/BaseWebSocket, UUID, id-rule and secondary HTTP/rate-limit leaf capability for the mapped constructor body; class callback coupling remains explicitly divergent. |
| 344 | `CoinbaseConstructionHost::Private` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 345 | `CoinbaseConstructionHost::Public` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 346 | `CoinbaseConstructionHost::construct_base` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 347 | `CoinbaseConstructionHost::construct_private_base` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 348 | `CoinbaseConstructionHost::construct_public_base` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 349 | `CoinbaseConstructionHost::create_secondary` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 350 | `CoinbaseConstructionHost::pattern_client_order_id_rule` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 351 | `CoinbaseConstructionHost::rate_limit` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 352 | `CoinbaseConstructionHost::uuid` | KEEP: FIXED #671: concrete callback now uses shared nanoid21/separator-removal UUID helper; actual signed JWT, POST and mint witnesses. |
| 370 | `ClassFindResult` | KEEP: Native signature/slot/retained-closure representation for the separately mapped base and Spot portfolio arrow fields; these aliases add no independently callable SDK operation. |
| 371 | `ClassSupportedResult` | KEEP: Native signature/slot/retained-closure representation for the separately mapped base and Spot portfolio arrow fields; these aliases add no independently callable SDK operation. |
| 372 | `ClassFindMethod` | KEEP: Native signature/slot/retained-closure representation for the separately mapped base and Spot portfolio arrow fields; these aliases add no independently callable SDK operation. |
| 373 | `ClassSupportedMethod` | KEEP: Native signature/slot/retained-closure representation for the separately mapped base and Spot portfolio arrow fields; these aliases add no independently callable SDK operation. |
| 374 | `CoinbaseExchange::capture_finder` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 375 | `CoinbaseExchange::capture_supported` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 386 | `CoinbaseExchange::set_finder` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 387 | `CoinbaseExchange::set_supported` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 388 | `CoinbasePrivateClassHost` | KEEP: Typed imported market/clock/console/Store/event leaves used by the separately mapped class order-event body; this host interface has no local implementation of the venue algorithm. |
| 389 | `CoinbasePrivateClassHost::private_order_markets` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 390 | `CoinbasePrivateClassHost::resolve_private_order_market` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 391 | `CoinbasePrivateClassHost::private_order_console_error` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 392 | `CoinbasePrivateClassHost::private_order_now_ms` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 393 | `CoinbasePrivateClassHost::private_orders_loaded` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 394 | `CoinbasePrivateClassHost::publish_private_order_snapshot` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 395 | `CoinbasePrivateClassHost::add_private_orders` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 396 | `CoinbasePrivateClassHost::emit_private_order_fill` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |

## `order_edit.rs`

Source owner: `coinbase.exchange.ts updateOrder`. Concrete route: `binding/orders.rs::OrderEditHost`. Witnesses under this crate: `write_edit.rs; exchange_binding.rs`.

| Entry | Rust declaration | Disposition and reviewed role |
|---:|---|---|
| 353 | `EditAsync` | KEEP: Native promise/input/replacement/receiver representation for mapped updateOrder; own update presence and retained original order roots are explicit. |
| 354 | `EditReplacement` | KEEP: Native promise/input/replacement/receiver representation for mapped updateOrder; own update presence and retained original order roots are explicit. |
| 355 | `EditUpdate` | KEEP: Native promise/input/replacement/receiver representation for mapped updateOrder; own update presence and retained original order roots are explicit. |
| 356 | `EditUpdateRoot` | KEEP: Native promise/input/replacement/receiver representation for mapped updateOrder; own update presence and retained original order roots are explicit. |
| 357 | `OrderEditHost` | KEEP: Native store/HTTP/cancellation/timer/log callback capability for the mapped updateOrder body; callback state wiring belongs to the separately divergent CoinbaseExchange class. |
| 358 | `OrderEditHost::cancel_orders` | KEEP: Concrete `binding/orders.rs::OrderEditHost::cancel_orders` supplies this imported call site; source policy remains in mapped caller. |
| 359 | `OrderEditHost::emit_and_log_error` | KEEP: Concrete `binding/orders.rs::OrderEditHost::emit_and_log_error` supplies this imported call site; source policy remains in mapped caller. |
| 360 | `OrderEditHost::market_by_symbol` | KEEP: Concrete `binding/orders.rs::OrderEditHost::market_by_symbol` supplies this imported call site; source policy remains in mapped caller. |
| 361 | `OrderEditHost::orders` | KEEP: Concrete `binding/orders.rs::OrderEditHost::orders` supplies this imported call site; source policy remains in mapped caller. |
| 362 | `OrderEditHost::place_order` | KEEP: Concrete `binding/orders.rs::OrderEditHost::place_order` supplies this imported call site; source policy remains in mapped caller. |
| 363 | `OrderEditHost::post_edit` | KEEP: Concrete `binding/orders.rs::OrderEditHost::post_edit` supplies this imported call site; source policy remains in mapped caller. |
| 364 | `OrderEditHost::schedule_reconciliation` | KEEP: Concrete `binding/orders.rs::OrderEditHost::schedule_reconciliation` supplies this imported call site; source policy remains in mapped caller. |
| 365 | `OrderEditHost::schedule_write_check` | KEEP: Concrete `binding/orders.rs::OrderEditHost::schedule_write_check` supplies this imported call site; source policy remains in mapped caller. |
| 366 | `OrderEditHost::selected_portfolio_type` | KEEP: Concrete `binding/orders.rs::OrderEditHost::selected_portfolio_type` supplies this imported call site; source policy remains in mapped caller. |
| 367 | `OrderEditReceiver` | KEEP: Native promise/input/replacement/receiver representation for mapped updateOrder; own update presence and retained original order roots are explicit. |

## `class_methods.rs`

Source owner: `coinbase.exchange.ts / coinbasespot.exchange.ts bound portfolio arrow fields`. Concrete route: `classes.rs private slots and captures`. Witnesses under this crate: `portfolios_balances.rs; class_private.rs`.

| Entry | Rust declaration | Disposition and reviewed role |
|---:|---|---|
| 376 | `BoundClassMethod` | KEEP: Generic Rust closure storage, retained bound receiver invocation and function/receiver identity plumbing for mapped source arrow fields; no independent venue algorithm, extra Promise or scheduling operation. |
| 377 | `BoundClassMethod::call` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |
| 378 | `BoundClassMethod::same_function` | KEEP: Retained-root access, bound receiver, native scalar view or imported call-site operation; no independent source algorithm is exempted. |

## Review outcome

Four concrete gap families emerged from this exemption review: UUID capabilities (#671), credential organization-segment admission (#677), health log/warning propagation (#691), and distinct same-price public-book copyback (#699). Legitimate receivers/capabilities remain. Historical mapped findings and declared carrier boundaries are not silently promoted to parity by this ledger.
