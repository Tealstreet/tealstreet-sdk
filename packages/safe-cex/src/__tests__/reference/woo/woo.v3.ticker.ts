import type { Market, Ticker } from '../../../types';
import type { WooV3FuturesRow, WooV3FuturesStats } from './woo.v3.public';
import type { WooV3WsBbo, WooV3WsTrade } from './woo.v3.websocket';

import { mapWooV3FuturesStats } from './woo.v3.public';

/**
 * Pure, transport-free composition of the three authoritative WOO V3 ticker
 * sources. This module is intentionally disconnected from the active runtime.
 */

export type WooV3AuthoritativeTickerInfo = Readonly<{
  futuresStats: WooV3FuturesStats;
  bbo: WooV3WsBbo;
  trade: WooV3WsTrade;
}>;

export type WooV3AuthoritativeTicker = Omit<Ticker, 'info'> &
  Readonly<{ info: WooV3AuthoritativeTickerInfo }>;

const normalizePerpetualSymbol = (symbol: string, path: string): string => {
  const match = /^PERP_([A-Z0-9]+)_([A-Z0-9]+)$/.exec(symbol);
  if (!match) throw new Error(`${path} must be a documented WOO PERP_BASE_QUOTE symbol`);
  return `${match[1]}${match[2]}`;
};

const requirePositiveFinitePrice = (value: string, path: string): number => {
  const price = Number(value);
  if (!Number.isFinite(price) || price <= 0) {
    throw new Error(`${path} must convert to a positive finite price`);
  }
  return price;
};

export const composeWooV3AuthoritativeTicker = (
  futuresRow: WooV3FuturesRow,
  bbo: WooV3WsBbo,
  trade: WooV3WsTrade,
  market: Market,
): WooV3AuthoritativeTicker => {
  const futuresSymbol = normalizePerpetualSymbol(futuresRow.symbol, 'WOO V3 futures ticker symbol');
  const bboSymbol = normalizePerpetualSymbol(bbo.symbol, 'WOO V3 BBO ticker symbol');
  const tradeSymbol = normalizePerpetualSymbol(trade.symbol, 'WOO V3 Trade ticker symbol');

  if (futuresSymbol !== bboSymbol || futuresSymbol !== tradeSymbol || futuresSymbol !== market.symbol) {
    throw new Error('WOO V3 ticker sources must have matching normalized symbol identities');
  }

  const futuresStats = mapWooV3FuturesStats([futuresRow], { [market.symbol]: market })[0];
  if (!futuresStats) throw new Error('WOO V3 futures ticker row does not map to its market');

  return Object.freeze({
    id: futuresStats.id,
    symbol: futuresStats.symbol,
    bid: requirePositiveFinitePrice(bbo.bidPrice, 'WOO V3 ticker bid'),
    ask: requirePositiveFinitePrice(bbo.askPrice, 'WOO V3 ticker ask'),
    last: requirePositiveFinitePrice(trade.price, 'WOO V3 ticker last trade'),
    mark: futuresStats.mark,
    index: futuresStats.index,
    percentage: futuresStats.percentage24h,
    openInterest: futuresStats.openInterest,
    fundingRate: futuresStats.estimatedFundingRate,
    fundingTime: futuresStats.fundingTime,
    volume: futuresStats.volume24h,
    quoteVolume: futuresStats.quoteVolume24h,
    info: Object.freeze({ futuresStats, bbo, trade }),
  });
};
