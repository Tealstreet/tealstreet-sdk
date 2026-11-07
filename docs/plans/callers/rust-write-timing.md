# write.timing: per-write latency breakdown

**Change.** Every user write (placeOrder(s), placeOrderWithClientOrderId,
updateOrder, cancelOrders, cancelSymbolOrders) produces one info engine event
with code `write.timing`. It takes the existing route for an info engine event:
`engine-event` on the caller-side exchange emitter, plus `exchange.log(message,
Info, event)`. No new channel, flag or global is added.

Stages: `called` → `received` (worker hop only) → `accepted` (engine command
call returned; in-engine signing is included) → `signed` (provider signer
only) → `sent` → `response` → `reply` (engine `done`) → `resolved`. Fields:
`op`, `outcome` (`ok`/`error`), `totalMs`, and `<from>To<To>Ms` for each
consecutive pair of present stages. The clock is
`performance.timeOrigin + performance.now()`, so stamps compare across threads.

`packages/safe-cex/src/writeTiming.ts` stamps the stages in every placement:
- a write method claims its timing synchronously;
- a caller in another thread hands its timing down with `runWithWriteTiming`
  and carries the marks in the RPC both ways (`callMethod` / `callMethod:done`
  `timing`);
- whoever owns the caller's resolve emits.

The JS host (`EngineHost.command` option `onStage`) attributes the signatures,
requests and managed preparations a command starts back to that command. Bound
HTTP groups are not attributed: they report no `sent`/`response`.

**First-call variance.** The breakdown separates the candidate causes:
- capability resolution (`placeOrders` awaits `capabilitiesFor`; a mirror's
  first call asks the worker) and worker-bridge wiring land in
  `calledToReceivedMs` / `receivedToAcceptedMs`;
- a worker busy applying frames shows in `calledToReceivedMs` and
  `replyToResolvedMs`;
- connection setup (TLS, proxy or tunnel connect) shows as `sentToResponseMs`
  well above the RTT;
- wasm warm-up of the first write path shows in `receivedToAcceptedMs`.

**Callers.**
- SDK: tests that record `host.command` arguments now record only the command.
- NX: no code change. The NX worker listener already forwards `engine-event`
  (#8334) and `log`.
- Release: `sdk-rs/hosts/wasm/js` is a beta-wasm fingerprint input. Once this
  is mirrored, NX needs a wasm release and repin at 8ec5498279a or later.

**Obligation.** A new writer placement (19pt8s) uses the same helper:
`runWithWriteTiming` on the writer side, marks in its RPC, and
`emitWriteTiming` at the caller.

**Deletion prerequisites.** None added.
