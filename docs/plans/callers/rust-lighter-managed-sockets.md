# Lighter managed sockets

Lighter's public and private feeds move onto the engine's managed sockets: the
engine owns registration, epochs, connect and silence bounds, the 10 s ping and
the 1 s to 16 s reconnect backoff. Both feeds declare `close_floors:
&[(1013, 30_000.0)]`, so a 1013 (try again later) close waits at least 30 s
before the retry, as the TS `RATE_LIMIT_RECONNECT_DELAY_MS` does on both feeds
(`packages/safe-cex/src/__tests__/reference/lighter/lighter.ws-public.ts:61`
and `lighter.ws-private.ts:103` beside it).

Master's legacy-socket behaviours carry over onto the managed feeds:

- Read-only fallback: two failed direct-route public connects, or three early
  1006/1008 private closes, reconnect that feed on `?readonly=true`; a routed
  account (`ws_routed`) never switches. The read-only private socket sends
  every other engine ping, legacy's 20 s cadence, and skips stream recovery.
  A connect or idle timeout counts as 1006, as the legacy supervisors closed
  it; a silent reconnect (the engine's repair) never counts.
- Ticker pacing: ticker channels join the public feed's interests two a second,
  displayed symbols first; a retired feed paces them again on reopen rather
  than replaying them at once.
- Subscription repair: `subscription_interest` names the book or candle
  channel and the engine re-sends it. A repair while the public feed is down
  sends nothing (the reconnect resubscribes every interest once), and an
  unsubscribed topic's warning names its channel key.

Callers: none in SDK, NX or server. The feed policy is internal to the venue
crate, and the account, order and market events the engine publishes keep their
shapes.

Known divergence: TS also treats a close whose reason text contains
`too many websocket messages` or `rate limit` as rate-limited, whatever its code
(`classifyLighterPublicClose` and `classifyLighterPrivateClose` in
`lighter.ws-policy.ts`). The engine's close floors match on the close code only,
so such a close under any other code takes the ordinary backoff. Closing this
divergence needs a reason-aware floor in the engine; nothing is planned.

Deletion prerequisites: the Lighter entries in `socket-legacy-families.json`,
`socket-legacy-sites.json` and `socket-legacy-replacements.json` are cleared
here. None are added.
