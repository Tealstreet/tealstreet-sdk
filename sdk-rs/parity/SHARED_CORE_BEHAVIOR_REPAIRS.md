# Shared-core behavior repairs

This slice repairs the effects behind the twelve historical behavior findings.
It does not equate twelve rows with twelve independent defects: four calendar
rows share a mechanism, as do the two gateway rows and two worker rows.
All twelve remain divergent because independent admitted boundaries remain;
residual unsupported rationale stays FINDING rather than being called supported;
zero status rows move. TypeScript is unchanged.

The branch starts at integration d4e20845a85cc4e2711c99648ef7547f8aa806df.
It does not inherit the unmerged CrossTrade/Store resolution PR7954. That PR
already contains the subsequent six CrossTrade and five Store code-repair rows;
it remains a separate deliverable for the integration owner to merge.

The frozen audit identities are exchanges16/66, utils3/4/15/41/51,
journal14, worker71/105 and HOW19/22. The original classifications are in the
shared-core, state/worker and HOW/transport audit documents on their audit
branches; this repair does not rewrite their historical measurements.

Applicable declarations are linked, not duplicated:
[14](MANIFEST-SPEC.md#shared-utf8-string-boundary),
[15](MANIFEST-SPEC.md#closed-typed-output-boundary),
[16](MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[17](MANIFEST-SPEC.md#numeric-ingress-defect),
[18](MANIFEST-SPEC.md#divergence-taxonomy),
[19](MANIFEST-SPEC.md#forced-surrogate-conflict).

| Historical rows | Repaired effect | Native assertion before source/wire comparison |
| --- | --- | --- |
| utils51 chartCandleStartTime | Date.UTC remaps years0–99 and invalid dates remain NaN | Eight literal monthly results |
| utils15 nextChartCandleStartTime | Original-year remapping precedes month rollover | Eight literal monthly results |
| utils4 chartCandleCloseTime | TimeClip, remapping and relative rollover retain source effects | Eight literal monthly results |
| utils41 getNumChartCandlesInRange | Counting uses remapped starts, preserving NaN and early reversed-range zero | Ten literal pairs |
| exchanges16 GatewayMessage | Literal private-number-key info remains an object | Own key, string value, sibling bid and independent count |
| exchanges66 ExternalPublicDataProvider.onMessage | Raw provider capture preserves that object before typed ticker decoding | Actual provider-to-parent info object |
| utils3 redactJsonValue | Fresh-object __proto__ setter omits an output own key | Four values at depths0,12,13, then public text |
| journal14 mapOkxPositionHistory | Computed Infinity, NaN and negative zero survive in canonical prepared values | Number variant, infinity/zero bits and NaN before any wire projection |
| worker71 CallMethodHandler.handle | Recovery command reaches the inner dispatcher | Native outer reply boolean |
| worker105 CallMethodHandler.handleCallMethod | Recovery invokes the Exchange hook and preserves its result/failure | Native inner boolean, false policy, all-socket visits and error |
| HOW19 parseWebsocketData | Form fallback respects the decoded __proto__ setter effect | Eight literal native parse results |
| HOW22 HOWWebSocket.onMessage | Disposed direct entry does not decode or dispatch the server clock | One active callback, zero after disposal |

Two coupled discoveries are recorded separately from those twelve identities.
The native BaseWebSocket diagnostic path had its own private redactor; repairing
only the utility would leave actual socket send/receive diagnostics wrong.
Its native key assertion failed before correction, and enabled public diagnostic
send/receive assertions now cover the real path. Three already-ported consumer
rows receive explanatory notes without status changes. This repairs own-key JSON
projection; it does not supply source local prototype identity or hooks.

Reusing the millisecond next-period helper exposed a negative-year rollover bug:
December year−1 must advance into January year0 before later start calculations
remap year0. A new assertion failed against the reused helper. Its correction
covers helper, typed-next and live-cache transition; that previously-ported helper
row gets a note, not a manufactured historical behavior identity.

## Actual consumers and inheritance limits

This is a bounded code inventory, not execution of every venue. No blanket
claim that all25 venues encounter every defect is supported.

* Calendar: native CandleCache calls these helpers during cached/fetched range
  preparation. Source CandleCache has the corresponding calls. No direct
  venue implementation call to the four typed helpers was found. Cache fetchers
  call venue fetchOHLCV outward; that does not establish reverse inheritance.
  The separately repaired millisecond helper also serves live-candle calculation.
* Gateway: registry declaration and a real external-provider fixture are executed.
  Source BaseExchange selects this provider conditionally through its public-data
  factory. Concrete native provider assembly was found only in registry tests;
  no particular venue's production provider construction is claimed.
* Redactor: textual source BaseWebSocket diagnostic consumers were found in
  apexomni, binance, bingx, bitget, bitrue, bitunix, blofin, bybit, coinbase,
  hyperliquid, kucoin, lighter, mexc, okx, orderly, phemex, weex and woo.
  Native emit/send-with-log consumers were found in binance, bingx, bitget,
  bitrue, bitunix, blofin, bybit, coinbase, kucoin, mexc, okx, weex and woo.
  This eighteen-source/thirteen-native inventory does not prove that hostile
  keys pass each venue formatter/filter or that every socket uses diagnostics.
* Journal: source OKX calls the mapper; OKXDemo, OKXEU and OKXEUDemo inherit it.
  Direct native consumers found here are tests. OrderReadOps.map_position is an
  injected production callback; its binding to this helper was not established.
  The repair supplies prepared native values, not a new production Vec<Value>
  bridge. Existing tests explicitly project to wire only where required.
* Worker: the command is a shared handler for the supplied Exchange object.
  Native shared socket registration was inspected in Bybit and Backpack.
  Source special false-policy subclasses occur in Lighter public/private,
  Polymarket public/private and HyroTrader private. The protocol hook supports
  false and is tested, but concrete native assembly of those special policies
  was not established. No ExchangeName-based policy guess is introduced.
* HOW: concrete native adapter uses were inspected in WOO's factory and Kucoin's
  API factory. HOW selection is conditional. Other factories inject adapters;
  their selection was not established. Form controls cover ordinary raw=false
  fallback, not every custom parser/raw/bigint profile. Disposal proves direct
  entry suppression, not a full late-network scheduling race.

All inventories stop at the sites/groups named above. Review is independent
code/source inspection; the reviewer did not run Cargo or claim venue execution.
Gate exits, deliberate controls and exact measured SHAs belong in
SHARED_CORE_BEHAVIOR_GATE_EXITS.md.
