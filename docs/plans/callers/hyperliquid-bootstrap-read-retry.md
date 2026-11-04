# Hyperliquid initial account reads

Initial core `clearinghouseState` and unified/portfolio perp collateral
`spotClearinghouseState` failures retry after managed completion, without a caller
read, private stream, socket refresh or later catalogue publication. Existing
engine startup backoff supplies 1/2/4/8/16/30-second nominal delays, 0.75–1 jitter
and a 30-second nominal cap; Retry-After floors take precedence. Existing
in-flight keys and revision/valuation guards remain authoritative. Valid core
state or collateral cancels its retry; validated unvalued collateral waits for
the existing price/valuation path instead of repeatedly fetching identical data.

The recurring perp poll is unchanged: adding core there would introduce steady
traffic and a two-minute bootstrap delay. Retries are restricted to missing
initial core/collateral proof and stop after success. Spot-only accounts retain
their existing polling. Timers belong to this engine through Cx; unregister/free
drops the engine and its retry state, with no process-global work or timer.

`request.failed` and native startup facts name only the closed last failure kind:
state, spot, orders, role, mode or discovery. A prior failure does not identify
the current missing stage or request egress. The coordinated cold-start PR owns
TS closed decoding/public exposure and timeout facts; the NX DQP companion reads
that optional property, includes tickers and per-flag readiness, and continues
to work with an older SDK. Lookup/admission and recurring socket policy are
outside this change.
