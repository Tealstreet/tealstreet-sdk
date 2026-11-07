# Hyperliquid candle repair readiness

A candle repair uses the same native `candle` topic and `Interval::as_str()` as
initial subscription. All 14 advertised intervals, including 4h and 1d, are
supported. The native feed now starts with the first catalogue, before
perpDexs/HIP-3 discovery finishes. Known candle demand can still precede feed
registration, for example with a cached native catalogue during bootstrap. A
reset in that interval retains its existing candle demand; it does not need
another timer or subscription count. Feed startup replays that demand.

An unnamed managed topic may use the existing venue repair hook to defer or
explain the refusal. Hyperliquid defers only known, demanded candles with no
registered feed, with a Debug `subscription.deferred` event. Unknown markets stay
visible as `subscription.rejected`, including while discovery is pending. A
refused feed registration is latched: later demanded candle repairs return a
visible `feed-refused` reason, rather than deferring indefinitely. Other
managed venues retain the default refusal; the normal named-stream repair,
silence escalation and reconnect paths are unchanged. The readiness hook falls
through to the existing refused-subscription retry once the feed is registered;
the public data-silence watch and native control classification remain enabled.

Attribution carries `feed-not-started`, `feed-refused` or `unknown-market`, `engineBoot`,
`symbolKnown`, `catalogueSize`, `discovery` and `feedRegistered`. The boot sequence
identifies a native venue instance within its runtime, remains stable across
transport reconnects, and is diagnostic only. Catalogue fields come from that
instance's Store, not the page's market cache. No user identity is added.

The mid-session HYPE 4h report is not explained by a proven feed-clear path: no
such path exists after registration. Book resubscription, candle
unsubscribe/resubscribe and transport reconnect controls retain the feed. The
new attribution distinguishes another engine's missing catalogue from pending
feed startup on the next occurrence. The earlier DOGE bar-starvation report is
separate; this change does not claim to diagnose it or the venue error channel.
