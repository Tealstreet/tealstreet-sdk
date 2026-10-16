# Polymarket exchange completion

Eight exchange rows move pending → ported: evaluated class, constructor,
fetchMarkets, fetchTickers, placeOrder, updateOrder, fetchOHLCV and listenOHLCV.
The combined slice closes sixteen pending venue rows: **123/18/0**
(ported/divergent/pending), with an independent census of 141 rows across seven manifests.

The implementation is in [exchange.rs](src/exchange.rs); source and native
witnesses are in [exchange.rs tests](tests/exchange.rs) and
[the actual-source oracle](tests/exchange_oracle.cjs). Exchange fixture floor:
128, stopped at listen/15. Rust production controls cover class maps,
constructor default, collateral, ticker bid, separate price/amount axes,
replacement presence, candle timestamp and duplicate filtering. Each compiled
fault fails its named native assertion and passes after restoration.

See [COMPLETION_EVIDENCE.md](COMPLETION_EVIDENCE.md) for the exact measured SHAs,
all gate exits, 17 Rust production controls plus one source-guard meta-control, dependency
boundaries, retained global-reader failures, sub-agent reviews and unrun checks.
No full SDK/VM or live-network coverage is inferred from the census.
