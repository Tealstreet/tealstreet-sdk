import type { AccountGreeks, Market, OptionChainQuery, OptionChainRow, OptionGreeks, OptionQuote } from '../../../types';

/**
 * Measured on 2026-10-14 against Black-Scholes on mainnet BTC options, where Bybit's
 * numbers matched to four figures (e.g. vega 84.168 vs 84.163, theta -63.719 vs -63.720).
 * Bybit's docs do not state them.
 */
export const BYBIT_GREEK_UNITS =
  'Black-Scholes per 1 unit of the underlying, in the premium asset: vega per 1 vol point, theta per calendar day';

const num = (value: unknown): number => {
  const parsed = parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) ? parsed : 0;
};

/** `BTC-25SEP26-130000-C-USDT`: the strike is the third segment, and `d` marks a decimal point. */
const strikeOf = (symbol: string): number => num(symbol.split('-')[2]?.replace('d', '.'));

/** An `instruments-info?category=option` row as a market, or undefined when it is not tradable. */
export const mapBybitOptionInstrument = (raw: Record<string, any>): Market | undefined => {
  if (raw.status !== 'Trading') return undefined;
  const strike = strikeOf(raw.symbol);
  const right = raw.optionsType === 'Call' ? 'call' : raw.optionsType === 'Put' ? 'put' : undefined;
  if (!strike || !right) return undefined;
  return {
    id: raw.symbol,
    symbol: raw.symbol,
    base: raw.baseCoin,
    quote: raw.quoteCoin,
    active: true,
    precision: {
      amount: num(raw.lotSizeFilter?.qtyStep),
      price: num(raw.priceFilter?.tickSize),
    },
    limits: {
      amount: { min: num(raw.lotSizeFilter?.minOrderQty), max: num(raw.lotSizeFilter?.maxOrderQty) },
      leverage: { min: 1, max: 1 },
      price: { min: num(raw.priceFilter?.minPrice) },
    },
    option: {
      underlying: raw.baseCoin,
      expiry: num(raw.deliveryTime),
      strike,
      right,
      style: 'european',
      settlement: 'cash',
      premiumAsset: raw.quoteCoin,
      settleAsset: raw.settleCoin,
    },
    contractSize: 1,
    isExpiring: true,
    info: raw,
  };
};

export const mapBybitOptionGreeks = (raw: Record<string, any>): OptionGreeks | undefined =>
  raw.delta === undefined || raw.delta === ''
    ? undefined
    : {
        delta: num(raw.delta),
        gamma: num(raw.gamma),
        vega: num(raw.vega),
        theta: num(raw.theta),
        units: BYBIT_GREEK_UNITS,
      };

/** A `tickers?category=option` row as a quote. */
export const mapBybitOptionTicker = (raw: Record<string, any>): OptionQuote => {
  const greeks = mapBybitOptionGreeks(raw);
  return {
    symbol: raw.symbol,
    bid: num(raw.bid1Price),
    ask: num(raw.ask1Price),
    mark: num(raw.markPrice),
    markIv: num(raw.markIv),
    bidIv: num(raw.bid1Iv),
    askIv: num(raw.ask1Iv),
    underlyingPrice: num(raw.underlyingPrice),
    ...(greeks ? { greeks } : {}),
  };
};

/** A `/v5/asset/coin-greeks` row. */
export const mapBybitCoinGreeks = (raw: Record<string, any>): AccountGreeks => ({
  underlying: raw.baseCoin,
  delta: num(raw.totalDelta),
  gamma: num(raw.totalGamma),
  vega: num(raw.totalVega),
  theta: num(raw.totalTheta),
  units: BYBIT_GREEK_UNITS,
});

export const matchesOptionQuery = (market: Market, query: OptionChainQuery): boolean => {
  const terms = market.option;
  if (!terms || terms.underlying !== query.underlying) return false;
  if (query.expiry !== undefined && terms.expiry !== query.expiry) return false;
  if (query.right !== undefined && terms.right !== query.right) return false;
  if (query.strikeMin !== undefined && terms.strike < query.strikeMin) return false;
  if (query.strikeMax !== undefined && terms.strike > query.strikeMax) return false;
  return true;
};

/** Joins markets and quotes for one query, sorted by expiry, then strike, calls before puts. */
export const buildOptionChain = (
  markets: readonly Market[],
  quotes: readonly OptionQuote[],
  query: OptionChainQuery,
): OptionChainRow[] => {
  const quoteBySymbol = new Map(quotes.map((q) => [q.symbol, q]));
  return markets
    .filter((m) => matchesOptionQuery(m, query))
    .flatMap((market) => {
      const quote = quoteBySymbol.get(market.symbol);
      return quote ? [{ market, quote }] : [];
    })
    .sort(
      (a, b) =>
        a.market.option!.expiry - b.market.option!.expiry ||
        a.market.option!.strike - b.market.option!.strike ||
        a.market.option!.right.localeCompare(b.market.option!.right),
    );
};

/** Distinct expiries of the underlying's tradable options, ascending. */
export const optionExpiries = (markets: readonly Market[], underlying: string): number[] =>
  [...new Set(markets.filter((m) => m.option?.underlying === underlying).map((m) => m.option!.expiry))].sort(
    (a, b) => a - b,
  );

export const isOptionMarket = (market: Market | undefined): boolean => market?.option !== undefined;

/**
 * `BTC-25SEP26-130000-C-USDT`, `NVDA-30OCT26-247.5-P-USDT`, `SOL-6NOV26-112d5-P`. Read from the symbol so a contract
 * that is delivering, or already gone from the market list, still routes as an option.
 */
export const isBybitOptionSymbol = (symbol: string): boolean =>
  /^[A-Z0-9]+-\d{1,2}[A-Z]{3}\d{2}-\d+([d.]\d+)?-[CP](-[A-Z]+)?$/.test(symbol);
