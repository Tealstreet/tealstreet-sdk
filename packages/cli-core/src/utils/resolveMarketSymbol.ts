import type { Market } from '@tealstreet/safe-cex/src/types';

/**
 * Resolve user input to a real market symbol.
 *
 * Traders type `btc`, not `BTCUSDT`, and the `symbol` command has always
 * advertised that ("sym BTC", "switch ETH" are its own examples). Resolution
 * lived only in `TradingBaseCommand.transformSymbol`, so `symbol btc` set the
 * session symbol to the literal `BTC` and reported success — then the next
 * order failed with "Price data is not available yet for a symbol", which
 * reads as a market-data outage rather than a symbol that was never real.
 * Reported from Discord 2026-09-28.
 *
 * Returns the input uppercased when nothing matches, so a caller can still
 * distinguish "resolved" from "left alone" by comparing.
 */
export function resolveMarketSymbol(markets: Market[] | undefined, symbol: string): string {
  const finalSymbol = symbol.trim().toUpperCase();
  if (!finalSymbol) return finalSymbol;

  const exact = markets?.find((market) => market.symbol.toUpperCase() === finalSymbol);
  if (exact) return exact.symbol;

  // The suffix decides the settlement currency, so `ETHUSD` resolves to the
  // inverse contract rather than being rewritten to the USDT one.
  let settle = 'USDT';
  if (finalSymbol.endsWith('USD')) settle = 'USD';
  if (finalSymbol.endsWith('USDC')) settle = 'USDC';

  // Longest base first: otherwise `ETHFI` resolves via the `ETH` market.
  const base = (markets ?? [])
    .filter((market) => finalSymbol.startsWith(market.base))
    .map((market) => market.base)
    .sort((a, b) => b.length - a.length)[0];

  if (!base) return finalSymbol;

  const probable = markets?.find((market) => market.base === base && market.quote === settle);
  return probable ? probable.symbol : finalSymbol;
}

/** Whether the exchange actually lists this symbol. */
export function marketSymbolExists(markets: Market[] | undefined, symbol: string): boolean {
  const target = symbol.trim().toUpperCase();
  return (markets ?? []).some((market) => market.symbol.toUpperCase() === target);
}
