# Rust worker mirror never runs an engine

`ExchangeOptions.workerMirror` marks the main-thread `RustEngineExchange` paired
with a worker that owns the account. Set it only where a host builds that
main-thread instance (apps/web `createExchangeFromAccount` in
`exchangeAccountController`, and the hosted equivalent); `ExchangeWorker`
strips it from the options its own engine runs with.

A mirror:

- routes worker-forwarded methods, native account reads, `getAccount` and
  capability loading to the worker from construction; a call made before the
  host wires `callMethod` waits for that bridge (30 s) instead of starting a
  local engine;
- answers `fetchMarkets`/`fetchTickers` and other native-data waits from the
  store the worker's frames fill, and `start()` is a no-op;
- never applies frames from a local engine. Hosted signing (`signHttpRequest`,
  `signWsAuth`) may still build an engine without starting it.

Why: the eager warm start (`ENABLE_EAGER_EXCHANGE`, hosted) called
`fetchMarkets`/`fetchOrders`/`fetchPositions` before `callMethod` was wired, so
the lazy `routesToWorker` check sent them to a keyless main-thread engine whose
frames replaced the worker's orders (`orders=[]`, `loaded.orders=true`).
