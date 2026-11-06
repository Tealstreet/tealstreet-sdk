# Hosted EXEC

`safe-cex-exec` ports the CLI-text worker route, not JavaScript evaluation.
Its `ExecFactory` is registered per account and creates scope-owned executions
for the starting socket. The shared algo runtime supplies `Host`, feeds exact
engine completions through `complete`, and calls `dispose` when that scope
closes. It owns all timers and scheduling.

`Host::snapshot` is a current engine-frame projection with the existing TS
`markets`, `tickers`, `orders` and `positions` fields. `Host::call` maps method
and arguments through the server's existing engine-backed `callMethod`
translation. It must return an admission failure or later deliver exactly one
completion; it never retries a write. Tokens belong to this Exec instance.
`Host::emit` receives the existing event's argument, before the server wraps it
in its existing worker event array/envelope.

This crate supports echo, last/mark/index prices, symbol/filtered/ID
cancellation, leverage, buy/sell, close, stop, break-even, trailing stop and
move, bump, scale, swarm and typed chaser/TWAP delegation. It preserves semicolon ordering, concurrent `@`
slices, optional reqId, last-sequential-result replies and structured native
errors. It creates no scheduler, process, venue transport or signer.

All 58 registered CLI command names have a native route or an explicit hosted
refusal. Server registration uses the shared account runtime and typed algo
books. Browser-side script preprocessing remains client-owned; there is no
JavaScript interpreter. Source oracles include cli-core command tests,
`CommandHandler.test.ts` and `worker-host/router*.test.ts`.

The native `ExecFactory` implements the shared runtime's `AlgoFactory`; add it
to the server registry (the native server already does this). Integration tests use the real shared LocalSet
scope and typed engine commands. Network and venue execution are not exercised.
Payload bounds and bare-chain failure behavior are specified in the caller
fragment, including the intentional cross-symbol ID-suffix cancellation rule.

Buy/sell sizes accept USD prefix/suffix or balance/position percentages and
convert into native contracts. Price, lot rounding, protective stop, reduce-only
clamp and instrument preflight follow cli-core's order policy. Comma symbols
issue one engine command at a time, preparing each successor from the current
store; failures stop that symbol fan-out without retry. An explicit semicolon
chain continues after failures as in TS, except a failed sleep/wait stops its
remaining steps so a false condition cannot release a trade.
Successful capture data contains only the native returned IDs, including [].
The account budget now counts every dispatched call, not just @ slices.

Inject `ExecFactory::with_algos(account_name, chasers, twap)` with the same
ChaserBook and cloned TwapFactory registered for direct ops. The account name
is trusted server configuration. CLI TWAP starts a real shared schedule, fixing
the TS hosted router's discarded taskAction; CLI list/stop inspect that schedule.

Sleep and wait run on the shared account scope. Conditions poll only current
engine frames (100 ms), support sustain/timeout, and dispose with their scope.
There is no venue-read loop. UTC clock waits use the next HHMM occurrence;
variables still require the client-side PreprocessStore.

Store displays include orders, positions/focused card, margin, spot, markets,
status/connection diagnostics, instrument check and maximum-size projection.
Missing coin/loaded telemetry stays unknown. Help names/aliases/descriptions/
examples come from the TS registered catalog via scripts/generate-cli-catalog.py;
its --check mode detects drift. Browser account/session/preprocess state remains
outside this crate. Display commands do not fetch venue data.

Chart requests bounded candles through the engine, renders the current position
and order overlays, and uses explicit UTC time-axis labels. It creates no candle
transport or independent subscription. Empty or malformed results refuse.

Account and prompt mutations explicitly refuse in hosted mode; payloads cannot
create account state or supply credentials. Account reads use the trusted
account, and the plain account list retains the TS worker's empty adapter
result. Prompt preview reads the current snapshot and keeps unknown socket
telemetry unknown; terminal clear is unavailable. Exchange schemas and prompt
presets are generated with the help catalog, with a scoped drift check. Legacy
sequential callers without reqId receive the TS worker's refusal-log fallback.
