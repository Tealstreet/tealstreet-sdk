import type { ExchangeLike } from '@tealstreet/cli-core/src/types';
import type { Exchange, ExchangeCapabilities } from '@tealstreet/safe-cex/src/exchanges/base';
import type { Order, PlaceOrderOpts } from '@tealstreet/safe-cex/src/types';

import {
  OrderSide,
  OrderStopTriggerType,
  OrderTimeInForce,
  OrderType,
  PositionSide,
} from '@tealstreet/safe-cex/src/types';
import { z } from 'zod';

import { prepareCliAmendPrice, prepareCliOrder } from '@tealstreet/cli-core/src/order/prepareCliOrder';
import { copyHistoryCompleteness } from '@tealstreet/safe-cex/src/historyCompleteness';
import { CAPABILITY_METHODS } from '@tealstreet/safe-cex/src/exchanges/base';
import { roundOrderSizeToNearest } from '@tealstreet/script-api-core/src/order/orderPricing';

type Shape = z.ZodRawShape;

export type Tool<S extends Shape = Shape> = {
  name: string;
  description: string;
  /** The adapter method the tool calls. Adapters that leave it unimplemented get no tool. */
  method: keyof Exchange | 'fetchBalance';
  /** Listed only when the server runs with options on. */
  requiresOptions?: true;
  input: S;
  run: (exchange: Exchange, args: z.infer<z.ZodObject<S>>) => Promise<unknown>;
};

const tool = <S extends Shape>(t: Tool<S>): Tool => t as unknown as Tool;

const symbol = z.string().describe('Market symbol as get_markets or get_option_chain lists it');
const symbols = z.array(z.string()).optional().describe('Market symbols; omit for all');
const from = z.number().int().optional().describe('Start time, ms since epoch');
const to = z.number().int().optional().describe('End time, ms since epoch');
const limit = z.number().int().positive().optional();

const optionRight = z.enum(['call', 'put']);
const isoDate = (ms: number) => new Date(ms).toISOString();

/**
 * Adds `<field>Utc` as an ISO date next to each time field, so a reader need not know
 * which fields safe-cex keeps in seconds and which in milliseconds.
 */
const withUtc = <T extends object>(rows: T[], fields: Record<string, 's' | 'ms'>): T[] =>
  copyHistoryCompleteness(
    rows,
    rows.map((row) => {
      const out = { ...row } as Record<string, unknown>;
      for (const [field, unit] of Object.entries(fields)) {
        const value = (row as Record<string, unknown>)[field];
        if (typeof value === 'number' && value > 0) {
          out[`${field}Utc`] = new Date(unit === 's' ? value * 1000 : value).toISOString();
        }
      }
      return out as T;
    }),
  );

const inMarkets = <T extends { symbol: string }>(rows: T[], wanted?: string[]): T[] =>
  wanted?.length ? rows.filter((r) => wanted.includes(r.symbol)) : rows;

const openOrders = async (exchange: Exchange, ids: string[]): Promise<Order[]> => {
  const orders = (await exchange.fetchOrders()).filter((o) => ids.includes(o.id));
  const missing = ids.filter((id) => !orders.some((o) => o.id === id));
  if (missing.length) throw new Error(`No open order with id ${missing.join(', ')}`);
  return orders;
};

/** The amend size rule, refusing a size that rounds to nothing rather than amending to zero. */
const prepareAmendAmount = (exchange: Exchange, symbol: string, amount: number): number => {
  const market = exchange.store.markets.find((m) => m.symbol === symbol);
  if (!market) throw new Error(`Market not found for ${symbol}`);
  const rounded = roundOrderSizeToNearest(amount, market);
  if (!Number.isFinite(rounded) || rounded <= 0) {
    throw new Error(`amount ${amount} rounds to ${rounded} for '${symbol}' — below the venue lot size`);
  }
  return rounded;
};

/** The shared order policy reads positions and orders from the store, which nothing polls here. */
const prepare = async (exchange: Exchange, opts: PlaceOrderOpts): Promise<PlaceOrderOpts> => {
  const [positions, orders, tickers] = await Promise.all([
    exchange.fetchPositions(),
    exchange.fetchOrders(),
    exchange.fetchTickers?.(),
  ]);
  const like = exchange as unknown as ExchangeLike;
  return prepareCliOrder(
    { ...like, name: like.name, store: { markets: exchange.store.markets, positions, orders, tickers } },
    opts,
  );
};

export const tools: Tool[] = [
  tool({
    name: 'get_markets',
    description:
      'Markets on this exchange with precision, size limits and leverage limits. With no filter, returns symbols only. Options are left out unless named in symbols; use get_option_chain for them. A market with isInverse is coin-margined and sized in USD contracts: its order amounts and fills count USD, its open orders give amount and extra.remainingNative in USD and remaining in the coin, and its positions give contracts in the coin and extra.contractsNative in USD.',
    method: 'start',
    input: {
      symbols,
      search: z.string().optional().describe('Case-insensitive substring of the symbol or base asset'),
    },
    run: async (exchange, { symbols: wanted, search }) => {
      const markets = exchange.store.markets.filter((m) => !m.option || wanted?.includes(m.symbol));
      if (!wanted?.length && !search) return markets.map((m) => m.symbol);
      const needle = search?.toLowerCase();
      return markets
        .filter(
          (m) =>
            wanted?.includes(m.symbol) ||
            (needle && (m.symbol.toLowerCase().includes(needle) || m.base.toLowerCase().includes(needle))),
        )
        .map(({ info: _venueResponse, ...market }) => market);
    },
  }),
  tool({
    name: 'get_tickers',
    description: 'Last, mark, index, bid and ask prices, 24h volume and funding rate. Options are in get_option_chain.',
    method: 'fetchTickers',
    input: { symbols },
    run: async (exchange, { symbols: wanted }) => {
      const tickers = (await exchange.fetchTickers?.()) ?? [];
      return inMarkets(tickers, wanted);
    },
  }),
  tool({
    name: 'get_candles',
    description: 'OHLCV candles.',
    method: 'fetchOHLCV',
    input: {
      symbol,
      interval: z.enum(['1m', '3m', '5m', '15m', '30m', '1h', '2h', '4h', '6h', '12h', '1d', '1w']),
      from,
      to,
      limit,
    },
    run: async (exchange, args) => withUtc(await exchange.fetchOHLCV(args), { timestamp: 's' }),
  }),
  tool({
    name: 'get_balance',
    description: 'Account balance: total, free, used and unrealised PnL.',
    method: 'fetchBalance',
    input: {},
    run: (exchange) => (exchange as Exchange & { fetchBalance: () => Promise<unknown> }).fetchBalance(),
  }),
  tool({
    name: 'get_positions',
    description: 'Open positions.',
    method: 'fetchPositions',
    input: { symbols },
    run: async (exchange, { symbols: wanted }) =>
      inMarkets(await exchange.fetchPositions(wanted?.length ? wanted : undefined), wanted),
  }),
  tool({
    name: 'get_open_orders',
    description: 'Open orders, including conditional orders.',
    method: 'fetchOrders',
    input: { symbols },
    run: async (exchange, { symbols: wanted }) =>
      inMarkets(await exchange.fetchOrders(wanted?.length ? wanted : undefined), wanted),
  }),
  tool({
    name: 'get_order_history',
    description: 'Past orders, including filled and cancelled ones. Known partial history includes rows and incompleteness metadata.',
    method: 'fetchHistoricalOrders',
    input: { symbol: symbol.optional(), orderId: z.string().optional(), from, to, limit },
    run: async (exchange, args) =>
      withUtc(await exchange.fetchHistoricalOrders(args), { createdTime: 'ms', updatedTime: 'ms' }),
  }),
  tool({
    name: 'get_fills',
    description: 'Trade executions (fills). Known partial history includes rows and incompleteness metadata.',
    method: 'fetchHistoricalFills',
    input: { symbol: symbol.optional(), orderId: z.string().optional(), from, to, limit },
    run: async (exchange, args) => withUtc(await exchange.fetchHistoricalFills(args), { timestamp: 's' }),
  }),
  tool({
    name: 'get_closed_pnl',
    description: 'Realised PnL of closed positions for one market. Known partial history includes rows and incompleteness metadata.',
    method: 'fetchClosedPnl',
    input: { symbol, from, to, limit },
    run: async (exchange, args) => withUtc(await exchange.fetchClosedPnl(args), { time: 'ms', closedTime: 'ms' }),
  }),
  tool({
    name: 'get_income',
    description: 'Account income records: funding, fees, realised PnL and transfers. Known partial history includes rows and incompleteness metadata.',
    method: 'fetchIncomeHistory',
    input: {
      symbol: symbol.optional(),
      incomeType: z.string().optional().describe('Filter by the type field of returned records'),
      startTime: from,
      endTime: to,
      limit,
    },
    run: async (exchange, args) => withUtc(await exchange.fetchIncomeHistory(args), { timestamp: 'ms' }),
  }),
  tool({
    name: 'get_option_expiries',
    description: 'Expiries of the tradable options on one underlying, oldest first.',
    method: 'fetchOptionExpiries',
    requiresOptions: true,
    input: { underlying: z.string().describe('Underlying asset, such as BTC') },
    run: async (exchange, { underlying }) =>
      (await exchange.fetchOptionExpiries(underlying)).map((expiry) => ({ expiry, date: isoDate(expiry) })),
  }),
  tool({
    name: 'get_option_chain',
    description:
      'Option chain for one underlying: prices, implied volatilities as fractions, and greeks where the venue publishes them. units says how the greeks are scaled. Filter by expiry or a strike range: one underlying can have hundreds of options.',
    method: 'fetchOptionChain',
    requiresOptions: true,
    input: {
      underlying: z.string().describe('Underlying asset, such as BTC'),
      expiry: z.number().int().optional().describe('Exact expiry in ms, as get_option_expiries returns it'),
      right: optionRight.optional(),
      strikeMin: z.number().optional(),
      strikeMax: z.number().optional(),
    },
    run: async (exchange, query) => {
      const rows = await exchange.fetchOptionChain(query);
      return {
        units: rows.find((r) => r.quote.greeks)?.quote.greeks?.units,
        rows: rows.map(({ market, quote }) => ({
          symbol: market.symbol,
          strike: market.option!.strike,
          right: market.option!.right,
          expiry: market.option!.expiry,
          date: isoDate(market.option!.expiry),
          bid: quote.bid,
          ask: quote.ask,
          mark: quote.mark,
          markIv: quote.markIv,
          bidIv: quote.bidIv,
          askIv: quote.askIv,
          underlyingPrice: quote.underlyingPrice,
          ...(quote.greeks
            ? {
                delta: quote.greeks.delta,
                gamma: quote.greeks.gamma,
                vega: quote.greeks.vega,
                theta: quote.greeks.theta,
              }
            : {}),
        })),
      };
    },
  }),
  tool({
    name: 'get_account_greeks',
    description: 'Option greeks summed across the account, per underlying. units says how they are scaled.',
    method: 'fetchAccountGreeks',
    requiresOptions: true,
    input: {},
    run: (exchange) => exchange.fetchAccountGreeks(),
  }),
  tool({
    name: 'place_order',
    description:
      "Place an order. Returns the new order ids. Size is in the units of the market's limits.amount from get_markets: on a market with isInverse, a count of USD contracts.",
    method: 'placeOrder',
    input: {
      symbol,
      side: z.enum(OrderSide),
      type: z.enum(OrderType),
      amount: z.number().positive(),
      price: z
        .number()
        .positive()
        .optional()
        .describe('Limit price; required for limit orders unless an option order gives impliedVolatility'),
      triggerPrice: z.number().positive().optional().describe('Trigger price for conditional and stop orders'),
      triggerType: z.enum(OrderStopTriggerType).optional(),
      stopLoss: z.number().positive().optional(),
      takeProfit: z.number().positive().optional(),
      reduceOnly: z.boolean().optional(),
      timeInForce: z.enum(OrderTimeInForce).optional().describe('PostOnly makes a limit order maker-only'),
      clientOrderId: z.string().optional(),
      impliedVolatility: z
        .number()
        .positive()
        .optional()
        .describe('Option orders only: price by implied volatility as a fraction, 0.5 for 50%. price is not sent then'),
    },
    run: async (exchange, args) => exchange.placeOrder(await prepare(exchange, args)),
  }),
  tool({
    name: 'amend_order',
    description: 'Change the price or the size of one open order. Returns the order ids after the change.',
    method: 'updateOrder',
    input: {
      orderId: z.string(),
      price: z.number().positive().optional(),
      amount: z.number().positive().optional(),
    },
    run: async (exchange, { orderId, price, amount }) => {
      if ((price === undefined) === (amount === undefined)) throw new Error('Set exactly one of price and amount');
      const [order] = await openOrders(exchange, [orderId]);
      const update =
        price !== undefined
          ? { price: prepareCliAmendPrice(exchange as unknown as ExchangeLike, order.symbol, price) }
          : { amount: prepareAmendAmount(exchange, order.symbol, amount!) };
      return exchange.updateOrder({ order, update });
    },
  }),
  tool({
    name: 'cancel_orders',
    description: 'Cancel open orders by id.',
    method: 'cancelOrders',
    input: { orderIds: z.array(z.string()).min(1) },
    run: async (exchange, { orderIds }) => {
      await exchange.cancelOrders(await openOrders(exchange, orderIds));
      return { cancelled: orderIds };
    },
  }),
  tool({
    name: 'cancel_all_orders',
    description: 'Cancel every open order, or every open order on one market.',
    method: 'cancelOrders',
    input: { symbol: symbol.optional() },
    run: async (exchange, { symbol: market }) => {
      const wanted = market ? [market] : undefined;
      const orders = inMarkets(await exchange.fetchOrders(wanted?.length ? wanted : undefined), wanted);
      if (orders.length) await exchange.cancelOrders(orders);
      return { cancelled: orders.map((o) => o.id) };
    },
  }),
  tool({
    name: 'set_leverage',
    description: 'Set leverage on one market.',
    method: 'setLeverage',
    input: { symbol, leverage: z.number().positive(), side: z.enum(PositionSide).optional() },
    run: async (exchange, { symbol: market, leverage, side }) => {
      await exchange.setLeverage(market, leverage, side);
      return { symbol: market, leverage };
    },
  }),
  tool({
    name: 'set_position_mode',
    description: 'Switch between hedged and one-way position mode.',
    method: 'changePositionMode',
    input: { hedged: z.boolean(), symbol: symbol.optional() },
    run: async (exchange, { hedged, symbol: market }) => {
      await exchange.changePositionMode(hedged, market);
      return { hedged };
    },
  }),
  tool({
    name: 'set_margin_type',
    description: 'Switch between isolated and cross margin.',
    method: 'changeMarginType',
    input: { isolated: z.boolean(), symbol: symbol.optional() },
    run: async (exchange, { isolated, symbol: market }) => {
      await exchange.changeMarginType(isolated, market);
      return { isolated };
    },
  }),
];

const capabilityOf = new Map<string, keyof ExchangeCapabilities>(
  Object.entries(CAPABILITY_METHODS).map(([capability, method]) => [method, capability as keyof ExchangeCapabilities]),
);

export const supportedTools = (exchange: Exchange, { options = false }: { options?: boolean } = {}): Tool[] => {
  const { capabilities } = exchange;
  return tools.filter((t) => {
    if (t.requiresOptions && !options) return false;
    if (typeof (exchange as unknown as Record<string, unknown>)[t.method] !== 'function') return false;
    const capability = capabilityOf.get(t.method);
    return capability === undefined || capabilities[capability];
  });
};
