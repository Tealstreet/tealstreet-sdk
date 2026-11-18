/** Fixture oracle only. Production registries never import this module. */
import type {
  Balance,
  Candle,
  Execution,
  ExecutionsOptions,
  Market,
  OHLCVOptions,
  Order,
  PlaceOrderOpts,
  Position,
  Ticker,
} from '../../../types';

import createHmac from 'create-hmac';

import { bigIntAsStringJsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { withHistoryCompleteness } from '../../../historyCompleteness';
import { OrderSide, OrderStatus, OrderTimeInForce, OrderType } from '../../../types';
import { adjust } from '../../../utils/safe-math';

export const SPOT_PATHS = Object.freeze({
  markets: '/openApi/spot/v1/common/symbols',
  balances: '/openApi/spot/v1/account/balance',
  orders: '/openApi/spot/v1/trade/openOrders',
  order: '/openApi/spot/v1/trade/order',
  cancel: '/openApi/spot/v1/trade/cancel',
  fills: '/openApi/spot/v1/trade/myTrades',
  candles: '/openApi/spot/v2/quote/klines',
  ticker: '/openApi/spot/v2/quote/ticker',
  depth: '/openApi/spot/v1/market/depth',
});
export const SPOT_WS = 'wss://open-api-ws.bingx.com/market';
type Params = Record<string, string | number>;
export type SpotFixtureTransport = (method: 'GET' | 'POST', path: string, params: Params) => Promise<string>;
type Row = Record<string, unknown>;
const isRow = (value: unknown): value is Row => value !== null && typeof value === 'object' && !Array.isArray(value);
const object = (value: unknown): Row => {
  if (!isRow(value)) throw new Error('Invalid Spot object');
  return value;
};
const array = (value: unknown): unknown[] => {
  if (!Array.isArray(value)) throw new Error('Missing Spot list');
  return value;
};
const number = (value: unknown): number => {
  if ((typeof value !== 'number' && typeof value !== 'string') || value === '' || !Number.isFinite(Number(value)))
    throw new Error('Invalid Spot number');
  return Number(value);
};
const text = (value: unknown): string => {
  if (typeof value !== 'string' || !value) throw new Error('Invalid Spot text');
  return value;
};
const id = (value: unknown): string => {
  if (typeof value === 'number' && !Number.isSafeInteger(value)) throw new Error('Rounded Spot identifier');
  const result = String(value);
  if (!/^[1-9][0-9]*$/.test(result)) throw new Error('Invalid Spot identifier');
  return result;
};
const symbol = (value: unknown): string => text(value).replaceAll('-', '');
const side = (value: unknown): OrderSide => {
  if (value === 'BUY') return OrderSide.Buy;
  if (value === 'SELL') return OrderSide.Sell;
  throw new Error('Unknown Spot side');
};
const kind = (value: unknown): OrderType => {
  if (value === 'LIMIT') return OrderType.Limit;
  if (value === 'MARKET') return OrderType.Market;
  if (['TAKE_STOP_LIMIT', 'TAKE_STOP_MARKET', 'TRIGGER_LIMIT', 'TRIGGER_MARKET'].includes(String(value)))
    return OrderType.Conditional;
  throw new Error('Unknown Spot order type');
};
const status = (value: unknown): OrderStatus => {
  if (['NEW', 'PENDING', 'PARTIALLY_FILLED'].includes(String(value))) return OrderStatus.Open;
  if (value === 'FILLED') return OrderStatus.Closed;
  if (['CANCELED', 'CANCELLED', 'EXPIRED', 'REJECTED'].includes(String(value))) return OrderStatus.Canceled;
  throw new Error('Unknown Spot status');
};
export const spotSigningGolden = (params: Params, timestamp: number, fixtureSecret: string) => {
  const fields = { ...params, timestamp };
  const sorted = Object.fromEntries(Object.entries(fields).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)));
  const canonical = Object.entries(sorted)
    .map(([key, value]) => `${encodeURIComponent(key)}=${encodeURIComponent(value)}`)
    .join('&');
  const signature = createHmac('sha256', fixtureSecret).update(canonical).digest('hex');
  return { canonical, signature, body: JSON.stringify({ ...sorted, signature }) };
};
export class BingxSpotReference {
  readonly isSpot = true;
  markets: Market[] = [];
  tickers: Ticker[] = [];
  constructor(readonly transport: SpotFixtureTransport) {}
  private async read(method: 'GET' | 'POST', path: string, params: Params = {}): Promise<unknown> {
    const response = object(bigIntAsStringJsonParse(await this.transport(method, path, params)));
    if (response.code !== 0 || !('data' in response)) throw new Error('Spot request failed');
    return response.data;
  }
  async fetchMarkets(): Promise<Market[]> {
    this.markets = array(object(await this.read('GET', SPOT_PATHS.markets)).symbols).map((value) => {
      const r = object(value);
      const native = text(r.symbol);
      const [base, quote] = native.split('-');
      if (!base || !quote || number(r.tickSize) <= 0 || number(r.stepSize) <= 0) throw new Error('Invalid Spot market');
      return {
        id: native,
        symbol: symbol(native),
        base,
        quote,
        active: number(r.status) === 1,
        isSpot: true,
        isPerpetual: false,
        isInverse: false,
        isExpiring: false,
        precision: { amount: number(r.stepSize), price: number(r.tickSize) },
        limits: {
          amount: { min: number(r.minQty ?? 0), max: number(r.maxQty ?? Number.MAX_VALUE) },
          leverage: { min: 1, max: 1 },
        },
      };
    });
    return this.markets;
  }
  async fetchTickers(): Promise<Ticker[]> {
    this.tickers = array(await this.read('GET', SPOT_PATHS.ticker)).map((value) => {
      const r = object(value);
      return {
        id: text(r.symbol),
        symbol: symbol(r.symbol),
        last: number(r.lastPrice),
        bid: number(r.bidPrice),
        ask: number(r.askPrice),
        percentage: number(text(r.priceChangePercent).replace('%', '')),
        volume: number(r.volume),
        quoteVolume: number(r.quoteVolume),
        mark: 0,
        index: 0,
        fundingRate: 0,
        fundingTime: 0,
        openInterest: 0,
      };
    });
    return this.tickers;
  }
  fetchPositions(): Promise<Position[]> {
    return Promise.resolve([]);
  }
  async fetchBalance(): Promise<Balance> {
    const currencies: Record<string, Balance> = {};
    let free = 0;
    let used = 0;
    for (const value of array(object(await this.read('GET', SPOT_PATHS.balances)).balances)) {
      const r = object(value);
      const currency = text(r.asset);
      const available = number(r.free);
      const locked = number(r.locked);
      if (available < 0 || locked < 0) throw new Error('Invalid Spot wallet');
      currencies[currency] = { free: available, used: locked, total: available + locked, upnl: 0 };
      const price = ['USD', 'USDT', 'USDC'].includes(currency)
        ? 1
        : this.tickers.find((t) => t.symbol === `${currency}USDT`)?.last;
      if (price === undefined && available + locked > 0) throw new Error('Spot wallet valuation requires a quote');
      free += available * (price ?? 0);
      used += locked * (price ?? 0);
    }
    return { free, used, total: free + used, upnl: 0, currencies };
  }
  mapOrder(value: unknown): Order {
    const r = object(value);
    const amount = number(r.origQty);
    const filled = number(r.executedQty);
    if (amount <= 0 || filled < 0 || filled > amount) throw new Error('Invalid Spot order quantities');
    return {
      id: id(r.orderId),
      symbol: symbol(r.symbol),
      status: status(r.status),
      type: kind(r.type),
      side: side(r.side),
      price: number(r.stopPrice ?? r.StopPrice ?? 0) || number(r.price),
      amount,
      filled,
      remaining: amount - filled,
      reduceOnly: false,
      lastUpdate: number(r.updateTime ?? r.time ?? 0),
      ...(r.clientOrderID ? { clientOrderId: text(r.clientOrderID) } : {}),
    };
  }
  async fetchOrders(): Promise<Order[]> {
    const rows = array(object(await this.read('GET', SPOT_PATHS.orders)).orders);
    if (rows.length >= 2000) throw new Error('Spot open-order coverage is capped');
    return rows.map((row) => this.mapOrder(row));
  }
  async placeOrder(o: PlaceOrderOpts): Promise<string[]> {
    const market = this.markets.find((m) => m.symbol === o.symbol);
    if (
      !market ||
      ![OrderType.Limit, OrderType.Market].includes(o.type) ||
      o.reduceOnly ||
      o.close ||
      o.stopLoss ||
      o.takeProfit ||
      o.triggerPrice
    )
      throw new Error('Only ordinary Spot limit/market orders are supported');
    const clientOrderId = o.clientOrderId;
    if (!clientOrderId || !/^[A-Za-z0-9_]{1,40}$/.test(clientOrderId))
      throw new Error('Fixture placement requires an exact clientOrderId');
    const amount = adjust(o.amount, market.precision.amount);
    if (!Number.isFinite(amount) || amount <= 0) throw new Error('Invalid Spot amount');
    const params: Params = {
      symbol: market.id,
      side: o.side === OrderSide.Buy ? 'BUY' : 'SELL',
      type: o.type === OrderType.Limit ? 'LIMIT' : 'MARKET',
      newClientOrderId: clientOrderId,
    };
    if (o.type === OrderType.Market && o.side === OrderSide.Buy) {
      if (!o.price || !Number.isFinite(o.price))
        throw new Error('Spot market buys require a price for quote conversion');
      params.quoteOrderQty = amount * adjust(o.price, market.precision.price);
    } else params.quantity = amount;
    if (o.type === OrderType.Limit) {
      if (!o.price || !Number.isFinite(o.price)) throw new Error('Spot limit order requires price');
      params.price = adjust(o.price, market.precision.price);
      params.timeInForce = {
        [OrderTimeInForce.GoodTillCancel]: 'GTC',
        [OrderTimeInForce.ImmediateOrCancel]: 'IOC',
        [OrderTimeInForce.FillOrKill]: 'FOK',
        [OrderTimeInForce.PostOnly]: 'PostOnly',
      }[o.timeInForce ?? OrderTimeInForce.GoodTillCancel];
    }
    const ack = object(await this.read('POST', SPOT_PATHS.order, params));
    if (ack.symbol !== market.id || !['NEW', 'PENDING', 'PARTIALLY_FILLED', 'FILLED'].includes(String(ack.status)))
      throw new Error('Unproven Spot placement');
    return [id(ack.orderId)];
  }
  async cancelOrder(orderId: string, sdkSymbol: string): Promise<void> {
    const market = this.markets.find((m) => m.symbol === sdkSymbol);
    if (!market) throw new Error('Unknown Spot market');
    const ack = object(await this.read('POST', SPOT_PATHS.cancel, { symbol: market.id, orderId: id(orderId) }));
    if (id(ack.orderId) !== orderId || ack.symbol !== market.id || ack.status !== 'CANCELED')
      throw new Error('Unproven Spot cancel');
  }
  async fetchHistoricalFills(opts: ExecutionsOptions): Promise<Execution[]> {
    const params: Params = { limit: Math.min(opts.limit ?? 500, 500) };
    if (opts.symbol) {
      const m = this.markets.find((m) => m.symbol === opts.symbol);
      if (!m) throw new Error('Unknown Spot market');
      params.symbol = m.id;
    }
    if (opts.orderId) params.orderId = opts.orderId;
    if (opts.from !== undefined) params.startTime = opts.from;
    if (opts.to !== undefined) params.endTime = opts.to;
    const source = array(object(await this.read('GET', SPOT_PATHS.fills, params)).fills);
    const filtered = source.filter((value) => {
      const time = number(object(value).time);
      return (opts.from === undefined || time >= opts.from) && (opts.to === undefined || time <= opts.to);
    });
    const rows = filtered.map((value): Execution => {
      const r = object(value);
      if (typeof r.isBuyer !== 'boolean' || typeof r.isMaker !== 'boolean') throw new Error('Invalid Spot fill flags');
      return {
        id: id(r.id),
        orderId: id(r.orderId),
        symbol: symbol(r.symbol),
        side: r.isBuyer ? OrderSide.Buy : OrderSide.Sell,
        price: number(r.price),
        qty: number(r.qty),
        commission: Math.abs(number(r.commission)),
        commissionAsset: text(r.commissionAsset),
        timestamp: Math.floor(number(r.time) / 1000),
        maker: r.isMaker,
      };
    });
    return filtered.length !== source.length ||
      (source.length >= Number(params.limit) &&
        (opts.from !== undefined || opts.limit === undefined || opts.limit > 500))
      ? withHistoryCompleteness(rows, { incomplete: true, reason: 'coverageGap' })
      : rows;
  }
  async fetchOHLCV(opts: OHLCVOptions): Promise<Candle[]> {
    const params: Params = {
      symbol: this.markets.find((m) => m.symbol === opts.symbol)?.id ?? '',
      interval: opts.interval,
      timeZone: 0,
      limit: Math.min(opts.limit ?? 500, 1440),
    };
    if (opts.from !== undefined) params.startTime = opts.from;
    if (opts.to !== undefined) params.endTime = opts.to;
    return array(await this.read('GET', SPOT_PATHS.candles, params))
      .map((row) => {
        const [timestamp, open, high, low, close, volume] = array(row).map(number);
        return { timestamp: timestamp / 1000, open, high, low, close, volume };
      })
      .sort((a, b) => a.timestamp - b.timestamp);
  }
  async fetchOrderBook(sdkSymbol: string) {
    const market = this.markets.find((m) => m.symbol === sdkSymbol);
    if (!market) throw new Error('Unknown Spot market');
    const r = object(await this.read('GET', SPOT_PATHS.depth, { symbol: market.id, limit: 50 }));
    const levels = (value: unknown) =>
      array(value).map((v) => {
        const [price, amount] = array(v).map(number);
        return [price, amount];
      });
    return { bids: levels(r.bids).sort((a, b) => b[0] - a[0]), asks: levels(r.asks).sort((a, b) => a[0] - b[0]) };
  }
  async start() {
    const markets = await this.fetchMarkets();
    const tickers = await this.fetchTickers();
    const [balance, orders, positions] = await Promise.all([
      this.fetchBalance(),
      this.fetchOrders(),
      this.fetchPositions(),
    ]);
    return { markets, tickers, balance, orders, positions };
  }
}
