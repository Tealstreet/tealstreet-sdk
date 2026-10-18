# Binance inverse

Behavior follows [the maintained Binance sources](../../packages/safe-cex/src/exchanges/binance/CLAUDE.md),
specifically binanceinverse.api/types/exchange/ws-public/ws-private.ts.
This crate uses the canonical native Js graph and shared Scope, BaseExchange,
HTTP factory, Store and BaseWebSocket services. Imported Binance depth/dispatch
and signing algorithms retain their original owners.

Construction and REST belong to exchange.rs/api.rs; the socket bodies belong to
ws_public.rs/ws_private.rs. Both sockets receive Rt with the actual exchange as
this, and retain Weak<Shared> when capturing callbacks. The parent provides the
actual options, Store, xhr/publicXhr and REST lifecycle methods.

The initial checkpoint establishes the shared constructor/runtime ABI. Registry
registration and parity claims follow the real installed bodies and binding
witnesses; the pending source census remains the implementation checklist.
