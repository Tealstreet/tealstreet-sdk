# Precision-swap audit

Audited production SHA: a55c6c4295bcc9694327a2724d2db9cafd7d3c26.
PR 7266 and its remote branch remain pinned there; this audit is follow-on local work.

The five Rust production files contain zero price-precision reads, zero
amount-precision reads, and zero calls to adjust or another step-rounding helper.
calculate_price_diff delegates to shared subtract; it does not select a Market
precision. format_symbol_out reads Market.symbol and info, never its precision.
Account reads, constants, and response types do not construct order payloads.
Thus there are zero compiled two-axis sites to swap and zero existing order
formatter targets. No production mutation was run for this audit. This is not
evidence of swap resistance for future formatters.

The TypeScript source contains pending exposure:

- phemex.exchange.ts:1321 and :1323 select price and amount precision independently
  in formatCreateOrder. Price, stopLoss, and takeProfit use pPrice (three adjust
  calls); amount, split remainder, and lot size use pAmount (three adjust calls).
  The price payload key switches between priceRp and stopPxRp.
- phemex.exchange.ts:1450 adjusts trailing-stop price distance using
  market.precision.price (one additional price adjust call).
- formatCreateOrder and formatCreateTrailingStopLossOrder, along with placeOrder
  and updateOrder, remain pending in the manifest. No coverage promotion follows
  from this audit; the triple remains 14 ported / 5 divergent / 60 pending.

Before promoting these formatter rows, execute the actual TypeScript with
fractional values where price and amount steps produce different rounded results,
across several distinct step pairs, both sides, limit and stop-market/bracket
paths. Assert native payload fields directly, including each stop/trigger field
and split quantity. For every applicable price/amount precision swap, first run
the original target, record any survivor, then prove the supplemented target
compiles and fails with exit 101 at its named assertion and passes on restoration.
Keep one Node process per corpus and all Cargo invocations sequential with jobs 1.
