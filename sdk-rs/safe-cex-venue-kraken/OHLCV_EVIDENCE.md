# Spot OHLCV read evidence

Measured row triple: 24 ported / 20 divergent / 89 pending (Python tomllib).
The one new row is KrakenSpotExchange.fetchOHLCV. Source body is selected unchanged
by the existing AST oracle and executed in one Node process for the entire read
corpus, including this supplement. Source-header architecture and Node major remain
recorded; only NaN is canonicalized. The original read oracle is byte-identical.

55 cases pin 93 returned candles and 558 direct native field comparisons. Per-case
output lengths come from the committed independent census, not the oracle being
compared. Source totals/field inventories, native lengths, fixed field/index walks,
completed cases and native total records are asserted. Two separate chosen-boundary
cases demonstrate source string/null timestamps and the numeric profile refusal.
Price/volume nonfinite values, signed zero, positive/negative infinite timestamps,
interval aliases and inherited identity, query fields, key order, stable duplicate
timestamps, malformed responses and synchronous/asynchronous errors are covered.
No market precision-step adjustment or notifying Store write is introduced.

Review identified an unnecessary draft restriction to finite timestamps. It was
removed before these controls. Numeric infinities are preserved and sorting uses
source subtraction semantics; the remaining CHOSEN restriction is nonnumeric
timestamps. The infinity case was added to this new supplement; no existing shared
or original fixture was rewritten.

18 controls ran serially: 14 venue production mutations (including one for every
native Candle field), two shared consuming-decoder mutations, and two source-witness
omissions. Each compiled, exited 101 at its named assertion, restored original
source/witness bytes and passed. Consuming controls drain input before yielding
None, avoiding the non-consuming trailing-element failure. With both native length
guards removed, fixed native index coverage still rejects the empty collection.
No production decoder defect is claimed: these are injected mutation controls.

```text
field-timestamp: compiled; exit 101; OHLCV native timestamp
field-timestamp: restored pass
field-open: compiled; exit 101; OHLCV native open
field-open: restored pass
field-high: compiled; exit 101; OHLCV native high
field-high: restored pass
field-low: compiled; exit 101; OHLCV native low
field-low: restored pass
field-close: compiled; exit 101; OHLCV native close
field-close: restored pass
field-volume: compiled; exit 101; OHLCV native volume
field-volume: restored pass
empty: compiled; exit 101; OHLCV independent native length
empty: restored pass
subset: compiled; exit 101; OHLCV independent native length
subset: restored pass
extra: compiled; exit 101; OHLCV independent native length
extra: restored pass
empty-guards-bypassed: compiled; exit 101; OHLCV native index coverage
empty-guards-bypassed: restored pass
query-since: compiled; exit 101; OHLCV native query fields
query-since: restored pass
query-interval: compiled; exit 101; OHLCV native query fields
query-interval: restored pass
sort-direction: compiled; exit 101; OHLCV native timestamp
sort-direction: restored pass
console-flatten: compiled; exit 101; OHLCV console error allocation
console-flatten: restored pass
consuming-empty: compiled; exit 101; OHLCV independent native length
consuming-empty: restored pass
consuming-empty-guards-bypassed: compiled; exit 101; OHLCV native index coverage
consuming-empty-guards-bypassed: restored pass
source-empty: compiled; exit 101; KRAKEN-CARDINALITY source total and per-case counts reads
source-empty: restored pass
source-subset: compiled; exit 101; KRAKEN-CARDINALITY source total and per-case counts reads
source-subset: restored pass
14 venue production + 2 consuming shared production + 2 source witness controls; 18 named failures; 18 restored passes
```

Original reads-oracle.cjs SHA256:
70da2916f8efa6626a1f47f74e23d30432a648e6823b2593d8d16e946959c8fa

SHA-specific final gates and review are reported after integration merge-down;
this record does not relabel mutation runs as having run on a later merge SHA.
