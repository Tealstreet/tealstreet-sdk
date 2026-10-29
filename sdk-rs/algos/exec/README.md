# Hosted EXEC

`safe-cex-exec` ports the CLI-text worker route, not JavaScript evaluation.
Its `Exec` is one per account/starting socket. The shared algo runtime supplies
`Host`, feeds exact engine completions back through `complete`, and calls
`dispose` when that scope closes. It owns all timers and scheduling.

`Host::snapshot` is a current engine-frame projection with the existing TS
`markets`, `tickers`, `orders` and `positions` fields. `Host::call` maps method
and arguments through the server's existing engine-backed `callMethod`
translation. It must return an admission failure or later deliver exactly one
completion; it never retries a write. Tokens belong to this Exec instance.
`Host::emit` receives the existing event's argument, before the server wraps it
in its existing worker event array/envelope.

This crate supports echo, last/mark/index prices, symbol/filtered/ID
cancellation, leverage, buy/sell, close, stop, break-even, trailing stop and
move, bump and scale placement/amendment. It preserves semicolon ordering, concurrent `@`
slices, optional reqId, last-sequential-result replies and structured native
errors. It creates no scheduler, process, venue transport or signer.

Full CLI catalog parity and server registration follow in separate slices.
This crate alone does not enable hosted users. Pending work includes the remaining trading catalog,
amendment policy, display formatting, timer-backed waits and
chaser/TWAP delegation. Browser-side script preprocessing stays client-owned.
The source oracles are `CommandHandler.test.ts`, `worker-host/router*.test.ts`,
`PriceCommand.ts`, `CancelCommand.ts` and `LeverageCommand.ts` in cli-core.

The native `ExecFactory` implements the shared runtime's `AlgoFactory`; add it
to the server registry. Its integration tests use the real shared LocalSet
scope and typed engine commands. Network and venue execution are not exercised.
Payload bounds and bare-chain failure behavior are specified in the caller
fragment, including the intentional cross-symbol ID-suffix cancellation rule.

Buy/sell sizes accept USD prefix/suffix or balance/position percentages and
convert into native contracts. Price, lot rounding, protective stop, reduce-only
clamp and instrument preflight follow cli-core's order policy. Comma symbols
issue one engine command at a time, preparing each successor from the current
store; failures stop that symbol fan-out without retry. An explicit semicolon
chain still continues after failures, exactly as the TS worker router does.
Successful capture data contains only the native returned IDs, including [].
The account budget now counts every dispatched call, not just @ slices.
