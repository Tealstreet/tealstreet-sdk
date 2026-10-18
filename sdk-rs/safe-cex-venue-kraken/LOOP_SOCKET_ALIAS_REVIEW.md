# Socket and periodic tick callback aliases

`lifecycle::SocketCallback` is an `Rc<dyn Fn(&dyn Any) -> Result<(), ExchangeError>>`.
The actual producer captures the assembled socket receiver; `WsHandle` admits
one native event argument and invokes that capability. It is a live callback
with a captured environment, with no wire/record decoder entrypoint.

`periodic_sync::TickCallback` is an `Rc<dyn Fn() -> Result<ExchangeFuture<'static,
()>, ExchangeError>>`. It returns a native completion capability or a synchronous
error. `TickHost::schedule_loop` starts it at the shared timer deadline, owns the
returned completion, and reports future failure. No payload/generic profile is
invented for either concrete alias.

`scripts/loop-socket-alias-contracts.mjs` uses the maintained authenticated context
and stock native identity/Serde/conversion probes. Before these two contracts are
present, `--expect-unclassified` requires the two actual missing-contract failures.
The positive mode requires exactly those two owners and zero failed receipts.
Other aliases and the underlying payload decoder contracts remain independent.

`tests/support/accepted_loop_socket_aliases.rs` starts from a constructed
`KrakenExchange` and its canonical Store. The actual public socket class connects,
registers its production callbacks, and consumes a physical ticker message with
native signed-zero/funding bits. Its independently retained callback then reads
an original event getter exactly once, publishes a second Store update, and
propagates a throwing getter with its name/message. No message body is replaced.

The timer witness schedules a real `current_tick(20)` capability. No HTTP starts
at 499 ms; one signed leverage request begins at 500 ms. Holding and releasing the
terminal response proves the real orders/positions/balance continuations and
loaded flags, distinct balance amounts and all four signed requests. The following
non-sync tick issues no account request. Separate synchronous and delayed-future
refusals reach the native runtime reporter at their actual deadlines.

The TypeScript references are the actual `connect` listener registration and
`onMessage` arrow in `kraken.ws-public.ts`, plus `KrakenExchange.tick`'s
`loop(() => this.tick(count + 1), 500)` in `kraken.exchange.ts`. This cohort does
not change any production callback, parser, scheduler or Store body.

The controls runner compiles three independent real consumer faults: dropping
the socket event, omitting the timer capability invocation, and swallowing its
future rejection. Each must fail its named native assertion (a compile failure
never counts), restore exact source bytes, and pass the same named test again.

Commands run serially from `sdk-rs`, with `CARGO_INCREMENTAL=0` throughout:

```sh
RUSTC_BOOTSTRAP=1 node safe-cex-venue-kraken/scripts/loop-socket-alias-contracts.mjs --expect-unclassified
cargo test --locked --jobs 1 -p safe-cex-venue-kraken --lib accepted_loop_socket_aliases
python3 safe-cex-venue-kraken/scripts/loop-socket-alias-controls.py
RUSTC_BOOTSTRAP=1 node safe-cex-venue-kraken/scripts/loop-socket-alias-contracts.mjs
```

Readiness also requires the full parity index, cancellation/override inventories,
strict all-target Kraken/parity Clippy and fmt checks. Seven of the remaining nine
aliases are outside this cohort; no complete Kraken runtime inventory is claimed.
