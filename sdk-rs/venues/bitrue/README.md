# Bitrue socket recovery

Public and private sockets use independent engine `Supervisor` instances with
10-second connect/token deadlines, 20-second receive watchdogs, and capped
exponential reconnect with deterministic jitter. Opening a socket does not
reset an unstable session's failure count.

Heartbeats remain server initiated. Public replies preserve the existing
literal and numeric pong forms; private replies preserve the server timestamp.
Each private recovery POSTs a fresh futures listen key. Session generations
fence late responses and cancel old keepalives. Public reconnects retain
subscription ownership, retire pending socket reads, and invalidate books.

## Legacy TS allocation parity

The splitter emits equal rounded lots plus a separately rounded modulo tail,
matching `packages/safe-cex/src/__tests__/reference/bitrue/bitrue.exchange.ts:1180-1196`.
At amount 26, maximum 5 and step 1 this sends `[4, 4, 4, 4, 4, 4, 1]`, sum 25.
KNOWN-ISSUE: legacy TS under-allocates (sum 25 of requested 26); kept for parity per Sam.
Rounding can also over-allocate: `0.025 / 0.01 / 0.001` sends
`[0.007, 0.007, 0.007, 0.005]`, sum `0.026`. No conservation correction is applied.
Generated below-minimum tails reach the venue rather than causing a local
split refusal, and there is no local thousand-lot gate. Existing limit/market
cap selection, quote conversion, command dispatch and guarded protective
put-back rules stay unchanged.
