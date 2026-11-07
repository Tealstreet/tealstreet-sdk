# Shared ticker observation provenance

Public-only native sources and Shared consumers emit optional SCXF format-1
section 24: repeated f64 `[tickerRow, lastSeen, markSeen, indexSeen]` rows;
negative infinity means unobserved. Full ticker snapshots include all times;
ordinary frames include observed rows even when their values do not change.
Direct account feeds keep their existing bytes. Source clocks are epoch host
receipt times, not exchange payload times or destination receipt times.

The shared-feed hello negotiates `observations: true` only when the account's
native snapshot has section 24. Providers strip `observedAt` for older consumers,
whose native JSON parser rejects unknown fields. New consumers of older sources
retain the old changed-value dating; missing metadata never refreshes unchanged
values. New sources carry explicit null for unobserved fields. Cached replay,
book/candle traffic, snapshots and local restoration retain source times.

This protocol is generic across every registered family. Source parser semantics
remain authoritative: Binance, Bybit, AsterDEX, ApeX, Paradex and Hyperliquid
explicitly record supplied unchanged references; other producers retain their
existing changed-field/snapshot observations. No destination infers a supplied
field from an unrelated source update, timer or cached value. The 5 s protective
reference bound and predicate remain byte-identical to master. Protective vetoes
read only local host receipt times (`ticker_seen_at`/`reference_seen_at`); source
`observedAt` lives in a separate provenance sidecar and section 24. Imported source
clocks, including old or future clocks, never overwrite local receipt freshness.
Unchanged shared values retain master's existing changed-value receipt behavior;
source-time-only updates and cached snapshots preserve provenance independently.
