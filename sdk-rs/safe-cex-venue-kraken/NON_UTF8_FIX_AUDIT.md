# Kraken non-UTF-8 FIX-audit review

This reviews exactly the twelve Kraken rows in gap-census evidence
`non-utf8-fix-by-crate.json`, against master `5368a1491c` and the current
constructed `KrakenExchange`. UTF-16 output choices are separate. ACCEPT means
that the alleged non-UTF-8 difference is absent at the assembled production
binding, with the reason and scope below; it does not declare every generic
Rust utility API equivalent to arbitrary executable JavaScript.

All native probes are in `tests/support/non_utf8_fix_audit.rs`, compiled as a
child of the production exchange module. They obtain the actual Scope from a
constructed Futures or Spot exchange, canonical DefaultStore and initialized
signed clients. The terminal runtime queues `spawn_local` without polling it.
A client-entry observer forwards to the original installed method and counts its
synchronous prefix; HTTP is then held/released without polling a returned SDK waiter. No replacement
ReadHost, member dispatcher, numeric mapper or class implementation is installed.
`non-utf8-audit-source.cjs` runs the maintained fill/income modules; the optional
`eager-wrapper-source.cjs --audit` selects unchanged maintained class arrows.

| Assigned row | Decision and real-binding observation |
| --- | --- |
| KrakenSpotExchange.fetchOHLCV | ACCEPT: `/0/public/OHLC` starts with an unpolled waiter, returns the source empty candle result, and is sent once. The lazy borrowed standalone helper is driven by the assembled eager Promise, repaired in #1067. |
| KrakenSpotExchange.fetchBalance | ACCEPT: signed POST `/0/private/Balance` starts without waiter polling and returns total 13.25 exactly once. The generic borrowed helper's polling boundary is not a public SDK startup delay. |
| KrakenExchange.fetchTickers | ACCEPT: the exact ticker entry starts `/v3/tickers` without waiter polling, keeps the source empty result and does not restart on await. This supplements #1067's earlier native fetchMarkets proof. |
| KrakenExchange.fetchOrders | ACCEPT: signed GET `/v3/openorders` starts without waiter polling, keeps the empty result and runs once. |
| KrakenExchange.fetchOHLCV | ACCEPT: real charts HTTP starts without waiter polling and retains source from=1/to=120 seconds plus the empty candle result. No clock/HTTP prefix is stranded inside a discarded Rust waiter. |
| KrakenExchange.validateAccount | ACCEPT: signed GET `/v3/accounts` starts without waiter polling; the source invalid-key diagnostic returns through the original Promise result. |
| KrakenExchange.fetchHistoricalPositions | ACCEPT: the actual venue reaches its configured shared JournalFolder before awaiting, with account `acct-A`, symbolScoped=false, default thirty-day window, USDT fallback and both ordered returned records including -0 bits. The imported folder is an explicit shared dependency, not an omitted venue constructor or local replacement fold. |
| income.num | FIX: a native NaN/Infinity in one monetary field previously failed JSON projection of the entire account-log response. Source Number + isFinite excludes that field while keeping its other monetary legs and good neighboring rows. The live binding now reads only source-selected native cells and applies canonical Number before its finite guard. |
| income.toMs | FIX: a native nonfinite date previously hit the same whole-response JSON refusal before Date.parse could return an invalid date and skip only that row. The live binding now uses canonical native String and shared Date.parse; the good neighbor survives. The paired array-valued date, fractional milliseconds and invalid-text cases also match source. |
| fills.toSeconds | ACCEPT: the constructed ReadHost uses the live native mapper, not `fills.rs`'s former DTO boundary. Date array coercion, fractional seconds with floor, invalid-date skip and an independent good neighbor match source. |
| fills.num | ACCEPT: live price/size conversion preserves signed zero, array/Boolean/null coercions, missing cells and finite fallback for NaN/Infinity. All output fields and cardinalities match actual source before serialization. |
| fills.optionalNum | ACCEPT: null/undefined/empty remain distinct from present zero; source-exact finite filtering preserves -0 and skips nonfinite PnL without dropping a fill. The real Scope mapper preserves the native graph before the final shared Execution projection. |

## Repair and limits

The original new native income probe compiled and failed on master with
`"nonfinite": numeric passthrough refuses NaN JSON`; the other three native
probe groups passed. The production fix changes only the account-log dispatch
arm in `binding/members.rs`. It preserves the signed history client, query,
error-envelope refusal, row order, four independent monetary categories and
fallback balance-delta classification. It retains the native graph until final
shared output admission. The borrowed `reads.rs`/`income.rs` standalone APIs
keep their documented closed wire profile; they are no longer the assembled
account-log implementation. Their existing source corpora remain tested.

The income profile has eight two-row cases, the fill profile seven. Both inspect
native f64 bits, own-undefined output slots, exact scalars and every output row;
NaN is canonicalized only in the oracle's input-bit transport. For these cases, monetary outputs
are finite. Existing retained boundary observers
now check income's original native text followed separately by final typed
IncomeRecord String projection.

`scripts/non-utf8-audit-controls.py` injects three independent production faults:
JSON-first response materialization, removal of the finite guard, and a date
unit change. Each must compile and fail its named actual-binding income test;
the JSON-first fault must fail both independently named numeric and date probes.
Exact production-byte restoration must pass. Compilation failure, zero selected
tests or a source-only failure cannot satisfy these controls.

These observations do not certify arbitrary host implementations of the generic
borrowed APIs, future runtime scheduling policies, or unrestricted shared typed
output shapes. The low-priority Array constructor identity item is separate.
