import type { AssetClass, InstrumentClassResult } from './types';

/**
 * The canonical human-readable copy for a classified instrument.
 *
 * Lives beside `classifyMarket` on purpose: three surfaces (the CLI preflight,
 * the CLI `check` command, and the web order-entry preflight) must say the
 * SAME thing. Copy drift across those three is how a user ends up acting on
 * one phrasing in the terminal and seeing another in the browser — and the
 * phrasing is load-bearing. `bybit/CLAUDE.md` → "contract tradeability probes"
 * is explicit: never claim "you can trade this", because eligibility is a
 * function of the venue's calendar and the account's signed agreements, of
 * which only the first is public. The honest sentence is "may require an
 * agreement you have not signed".
 */

/** The reader-facing noun for an asset class. */
export const assetClassLabel = (ac: AssetClass): string => {
  switch (ac) {
    case 'crypto':
      return 'crypto';
    case 'equity':
      return 'tokenized equity';
    case 'etf':
      return 'tokenized ETF';
    case 'commodity':
      return 'tokenized commodity';
    case 'fx':
      return 'tokenized FX pair';
    case 'index':
      return 'tokenized index';
  }
};

/**
 * One-line summary used by the CLI `check` command and the web preflight
 * header. Shape:
 *
 *     BTCUSDT on bybit: crypto perpetual
 *     SHOPUSDT on bybit: tokenized equity (Shopify) — may require signing a trading agreement
 *     XAUUSDT on bybit: tokenized commodity — may require signing a trading agreement
 *     SOMEUSDT on binance: classifier not implemented for binance
 */
export const describeInstrument = (symbol: string, exchangeName: string, result: InstrumentClassResult): string => {
  if (result.kind === 'unknown') {
    return `${symbol} on ${exchangeName}: ${result.reason}`;
  }
  const base = `${symbol} on ${exchangeName}: ${assetClassLabel(result.assetClass)}`;
  if (result.assetClass === 'crypto') {
    return `${base} perpetual`;
  }
  const underlying = result.underlyingTicker ? ` (${result.underlyingTicker})` : '';
  return `${base}${underlying} — may require signing a trading agreement`;
};

/**
 * The refusal message a CLI order write throws when the symbol is not crypto.
 * The web preflight uses the same body, prefixed as a question for the user's
 * confirmation prompt.
 *
 * UNKNOWN never produces one of these — a venue with no classifier is a
 * normal state of this system, not a reason to block an order. Only a
 * positively classified non-crypto instrument refuses.
 */
export const tradfiPreflightMessage = (
  symbol: string,
  exchangeName: string,
  result: InstrumentClassResult,
): string | undefined => {
  if (result.kind !== 'classified' || result.assetClass === 'crypto') return undefined;
  const underlying = result.underlyingTicker ? ` (${result.underlyingTicker})` : '';
  const label = assetClassLabel(result.assetClass);
  return `${symbol} on ${exchangeName} is a ${label}${underlying}. Opening a position may require an agreement you have not signed with the venue — place the order through the venue's own UI first to accept it.`;
};
