# Lighter managed sockets

Lighter's public and private feeds move onto the engine's managed sockets: the
engine owns registration, epochs, connect and silence bounds, the 10 s ping and
the 1 s to 16 s reconnect backoff. Both feeds declare `close_floors:
&[(1013, 30_000.0)]`, so a 1013 (try again later) close waits at least 30 s
before the retry, as the TS `RATE_LIMIT_RECONNECT_DELAY_MS` does on both feeds
(`packages/safe-cex/src/__tests__/reference/lighter/lighter.ws-public.ts:61`
and `lighter.ws-private.ts:103` beside it).

Callers: none in SDK, NX or server. The feed policy is internal to the venue
crate, and the account, order and market events the engine publishes keep their
shapes.

Known divergence: TS also treats a close whose reason text contains
`too many websocket messages` or `rate limit` as rate-limited, whatever its code
(`classifyLighterPublicClose` and `classifyLighterPrivateClose` in
`lighter.ws-policy.ts`). The engine's close floors match on the close code only,
so such a close under any other code takes the ordinary backoff. Closing this
divergence needs a reason-aware floor in the engine; nothing is planned.

Deletion prerequisites: the Lighter entries in `socket-legacy-families.json` and
`socket-legacy-sites.json` are cleared here. None are added.
