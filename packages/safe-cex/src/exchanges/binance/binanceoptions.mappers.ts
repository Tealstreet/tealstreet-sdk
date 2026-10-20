import type {
  AccountGreeks,
  Balance,
  Candle,
  Execution,
  HistoricalOrder,
  Market,
  OptionGreeks,
  OptionQuote,
  Order,
  PlaceOrderOpts,
  Position,
  Ticker,
} from '../../types';

import { OrderSide, OrderStatus, OrderTimeInForce, OrderType, PositionSide } from '../../types';
import { inverseObj } from '../../utils/inverseObj';
import { omitUndefined } from '../../utils/omitUndefined';
import { adjust } from '../../utils/safe-math';
import { TIME_IN_FORCE } from './binanceoptions.types';

/**
 * Measured on 2026-10-20 against Black-Scholes on mainnet: BTC-261225-85000-C gave vega 137.17 and
 * delta 0.448 where the formula gives 137.2 and 0.448, and DOGE-261023-0.08-C (1000 DOGE a contract)
 * gave vega 2.84e-5, which is per DOGE, not per contract. Binance's docs do not state them.
 */
export const BINANCE_OPTION_GREEK_UNITS =
  'Black-Scholes per 1 unit of the underlying, in the premium asset: vega per 1 vol point, theta per calendar day';

/** `/eapi/v1/position` carries no greeks, so these are the mark greeks scaled by the position. */
export const BINANCE_POSITION_GREEK_UNITS = `${BINANCE_OPTION_GREEK_UNITS}; multiplied by the signed position size in units of the underlying (quantity × contract unit) from /eapi/v1/mark`;

/** Not measured: it needs a funded options account. */
export const BINANCE_ACCOUNT_GREEK_UNITS = 'As /eapi/v1/marginAccount reports them; scale not measured';

const num = (value: unknown): number => {
  const parsed = parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) ? parsed : 0;
};

/** Binance sends -1 for an implied volatility it cannot quote. */
const iv = (value: unknown): number => Math.max(num(value), 0);

const filterOf = (raw: Record<string, any>, type: string): Record<string, any> | undefined =>
  (raw.filters as Array<Record<string, any>> | undefined)?.find((f) => f.filterType === type);

export type BinanceOptionContract = { baseAsset: string; settleAsset: string };

/** `optionContracts` keyed by underlying (`BTCUSDT`), which is how symbols and account greeks name it. */
export const optionContractsByUnderlying = (raw: Record<string, any>): Map<string, BinanceOptionContract> =>
  new Map(
    ((raw.optionContracts ?? []) as Array<Record<string, any>>).map((c) => [
      c.underlying,
      { baseAsset: c.baseAsset, settleAsset: c.settleAsset },
    ]),
  );

/** An `exchangeInfo.optionSymbols` row as a market, or undefined when it is not trading. */
export const mapBinanceOptionMarket = (
  raw: Record<string, any>,
  contracts: Map<string, BinanceOptionContract>,
): Market | undefined => {
  if (raw.status !== 'TRADING') return undefined;
  const right = raw.side === 'CALL' ? 'call' : raw.side === 'PUT' ? 'put' : undefined;
  const strike = num(raw.strikePrice);
  const contract = contracts.get(raw.underlying);
  if (!right || !strike || !contract) return undefined;
  const lot = filterOf(raw, 'LOT_SIZE');
  const price = filterOf(raw, 'PRICE_FILTER');
  return {
    id: raw.symbol,
    symbol: raw.symbol,
    base: contract.baseAsset,
    quote: raw.quoteAsset,
    active: true,
    precision: { amount: num(lot?.stepSize), price: num(price?.tickSize) },
    limits: {
      amount: { min: num(lot?.minQty ?? raw.minQty), max: num(lot?.maxQty ?? raw.maxQty) },
      leverage: { min: 1, max: 1 },
      price: { min: num(price?.minPrice) },
    },
    option: {
      underlying: contract.baseAsset,
      expiry: num(raw.expiryDate),
      strike,
      right,
      style: 'european',
      settlement: 'cash',
      premiumAsset: raw.quoteAsset,
      settleAsset: contract.settleAsset,
    },
    // Prices and quantities are per contract, and a contract is `info.unit` of the underlying.
    contractSize: 1,
    isExpiring: true,
    info: raw,
  };
};

export const mapBinanceOptionGreeks = (mark: Record<string, any>): OptionGreeks | undefined =>
  mark.delta === undefined || mark.delta === ''
    ? undefined
    : {
        delta: num(mark.delta),
        gamma: num(mark.gamma),
        vega: num(mark.vega),
        theta: num(mark.theta),
        units: BINANCE_OPTION_GREEK_UNITS,
      };

/** One `/eapi/v1/mark` row and its `/eapi/v1/ticker` row as a quote. */
export const mapBinanceOptionQuote = (
  mark: Record<string, any>,
  ticker: Record<string, any> | undefined,
): OptionQuote => {
  const greeks = mapBinanceOptionGreeks(mark);
  return {
    symbol: mark.symbol,
    bid: num(ticker?.bidPrice),
    ask: num(ticker?.askPrice),
    mark: num(mark.markPrice),
    markIv: iv(mark.markIV),
    bidIv: iv(mark.bidIV),
    askIv: iv(mark.askIV),
    // Binance: the index price, or the estimated settlement price in the hour before expiry.
    underlyingPrice: num(ticker?.exercisePrice),
    ...(greeks ? { greeks } : {}),
  };
};

export const mapBinanceOptionTicker = (ticker: Record<string, any>, mark: Record<string, any> | undefined): Ticker => {
  const last = num(ticker.lastPrice);
  return {
    id: ticker.symbol,
    symbol: ticker.symbol,
    bid: num(ticker.bidPrice),
    ask: num(ticker.askPrice),
    last,
    mark: mark ? num(mark.markPrice) : last,
    index: num(ticker.exercisePrice),
    // A fraction on eapi (705 on an open of 3815 reads 0.185), where Ticker wants a percentage.
    percentage: num(ticker.priceChangePercent) * 100,
    openInterest: 0,
    fundingRate: 0,
    fundingTime: 0,
    volume: num(ticker.volume),
    quoteVolume: num(ticker.amount),
  };
};

/** `/eapi/v1/marginAccount`. Options margin in USDT only today, so totals sum the assets at par. */
export const mapBinanceOptionBalance = (account: Record<string, any>): Balance => {
  const currencies: Record<string, Balance> = {};
  let total = 0;
  let free = 0;
  let used = 0;
  let upnl = 0;
  for (const asset of (account.asset ?? []) as Array<Record<string, any>>) {
    const row = {
      total: num(asset.equity),
      free: num(asset.available),
      used: num(asset.initialMargin),
      upnl: num(asset.unrealizedPNL),
      info: asset,
    };
    currencies[asset.asset] = row;
    total += row.total;
    free += row.free;
    used += row.used;
    upnl += row.upnl;
  }
  return { total, free, used, upnl, currencies, info: account };
};

export const mapBinanceAccountGreeks = (
  account: Record<string, any>,
  contracts: Map<string, BinanceOptionContract>,
): AccountGreeks[] =>
  ((account.greek ?? []) as Array<Record<string, any>>).map((g) => ({
    underlying: contracts.get(g.underlying)?.baseAsset ?? g.underlying,
    delta: num(g.delta),
    gamma: num(g.gamma),
    vega: num(g.vega),
    theta: num(g.theta),
    units: BINANCE_ACCOUNT_GREEK_UNITS,
  }));

/** A `/eapi/v1/position` row, with greeks from that option's `/eapi/v1/mark` row when there is one. */
export const mapBinanceOptionPosition = (
  raw: Record<string, any>,
  market: Market | undefined,
  mark: Record<string, any> | undefined,
  now: number,
): Position => {
  const side = raw.side === 'SHORT' || num(raw.quantity) < 0 ? PositionSide.Short : PositionSide.Long;
  const contracts = Math.abs(num(raw.quantity));
  const perUnit = mark ? mapBinanceOptionGreeks(mark) : undefined;
  const exposure = (side === PositionSide.Short ? -1 : 1) * contracts * (num(market?.info?.unit) || 1);
  const greeks = perUnit && {
    delta: perUnit.delta * exposure,
    gamma: perUnit.gamma * exposure,
    vega: perUnit.vega * exposure,
    theta: perUnit.theta * exposure,
    units: BINANCE_POSITION_GREEK_UNITS,
  };
  return {
    id: raw.symbol,
    symbol: raw.symbol,
    side,
    entryPrice: num(raw.entryPrice),
    notional: Math.abs(num(raw.markValue)),
    // Options carry no leverage; 1 keeps notional / leverage finite for margin math.
    leverage: 1,
    unrealizedPnl: num(raw.unrealizedPNL),
    contracts,
    liquidationPrice: 0,
    extra: { contractsNative: contracts },
    info: raw,
    lastUpdate: now,
    option: greeks ? { greeks } : {},
  };
};

const ORDER_STATUS: Record<string, OrderStatus> = {
  NEW: OrderStatus.Open,
  ACCEPTED: OrderStatus.Open,
  PARTIALLY_FILLED: OrderStatus.Open,
  FILLED: OrderStatus.Closed,
  CANCELLED: OrderStatus.Canceled,
  CANCELED: OrderStatus.Canceled,
  REJECTED: OrderStatus.Canceled,
  EXPIRED: OrderStatus.Canceled,
};

const sideOf = (raw: Record<string, any>) => (raw.side === 'SELL' ? OrderSide.Sell : OrderSide.Buy);

export const mapBinanceOptionOrder = (raw: Record<string, any>, now: number): Order => {
  const amount = num(raw.quantity);
  const filled = num(raw.executedQty);
  return {
    id: String(raw.orderId),
    status: ORDER_STATUS[raw.status] ?? OrderStatus.Open,
    symbol: raw.symbol,
    type: OrderType.Limit,
    side: sideOf(raw),
    price: num(raw.price),
    amount,
    filled,
    remaining: amount - filled,
    reduceOnly: raw.reduceOnly === true,
    ...(raw.clientOrderId ? { clientOrderId: raw.clientOrderId } : {}),
    timeInForce: raw.postOnly === true ? OrderTimeInForce.PostOnly : TIME_IN_FORCE[raw.timeInForce],
    lastUpdate: now,
    info: raw,
  };
};

export const mapBinanceOptionHistoricalOrder = (raw: Record<string, any>): HistoricalOrder => {
  const averagePrice = num(raw.avgPrice);
  return {
    id: String(raw.orderId),
    symbol: raw.symbol,
    side: sideOf(raw),
    type: OrderType.Limit,
    status: ORDER_STATUS[raw.status] ?? OrderStatus.Open,
    price: num(raw.price),
    ...(averagePrice ? { averagePrice } : {}),
    amount: num(raw.quantity),
    filled: num(raw.executedQty),
    reduceOnly: raw.reduceOnly === true,
    createdTime: num(raw.createTime ?? raw.createDate),
    updatedTime: num(raw.updateTime),
  };
};

/**
 * A `/eapi/v1/userTrades` row. The documented TAKER example carries a negative `fee`, so negative is a
 * charge. `realizedProfit` is left off: nothing says whether it is gross or net of the fee.
 */
export const mapBinanceOptionFill = (raw: Record<string, any>): Execution => ({
  id: String(raw.tradeId ?? raw.id),
  timestamp: num(raw.time) / 1000,
  symbol: raw.symbol,
  orderId: String(raw.orderId),
  price: num(raw.price),
  qty: num(raw.quantity),
  side: sideOf(raw),
  commission: -num(raw.fee),
  commissionAsset: raw.quoteAsset,
  maker: raw.liquidity === 'MAKER',
});

export const mapBinanceOptionCandle = ([openTime, open, high, low, close, volume]: unknown[]): Candle => {
  return {
    timestamp: num(openTime) / 1000,
    open: num(open),
    high: num(high),
    low: num(low),
    close: num(close),
    volume: num(volume),
  };
};

/** The `POST /eapi/v1/order` body. Callers have already refused every shape Binance options cannot take. */
export const formatBinanceOptionOrder = (opts: PlaceOrderOpts, market: Market, clientOrderId: string) => {
  const postOnly = opts.timeInForce === OrderTimeInForce.PostOnly;
  return omitUndefined({
    symbol: opts.symbol,
    side: opts.side === OrderSide.Sell ? 'SELL' : 'BUY',
    type: 'LIMIT',
    quantity: `${adjust(opts.amount, market.precision.amount)}`,
    price: `${adjust(opts.price!, market.precision.price)}`,
    timeInForce: postOnly ? 'GTC' : inverseObj(TIME_IN_FORCE)[opts.timeInForce ?? OrderTimeInForce.GoodTillCancel],
    postOnly: postOnly ? true : undefined,
    reduceOnly: opts.reduceOnly ? true : undefined,
    clientOrderId,
  });
};
