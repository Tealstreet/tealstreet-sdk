import { describe, expect, it, vi } from 'vitest';

import { OrderStatus, OrderType, PositionSide, type Market, type Order, type Position, type Ticker } from '@tealstreet/safe-cex/src/types';
import { DefaultStore } from '@tealstreet/safe-cex/src/store/store.base';
import type { CLIPlatformAdapter, ExchangeLike } from '../../types';
import { ChartCommand, formatOrderLine, formatPositionLine, formatPositionSummary } from './ChartCommand';

it('chart annotations preserve positions smaller than four decimal places', () => {
  const market = { base: 'BTC', precision: { amount: 0.000001, price: 0.1 }, contractSize: 1 } as Market;
  const position = { side: PositionSide.Long, contracts: 0.000006, entryPrice: 60_000, unrealizedPnl: 0 } as Position;

  expect(formatPositionLine(position, market)?.label).toContain('Long 0.000006');
  expect(formatPositionSummary(position, market)?.replace(/\x1b\[[0-9;]*m/g, '')).toContain('Long 0.000006');
});

const sizeCases = [
  { name: 'OKX DOGE', base: 'DOGE', step: 0.01, contractSize: 1000, amount: 0.4, remaining: 400, expected: '400' },
  { name: 'partially filled OKX DOGE', base: 'DOGE', step: 0.01, contractSize: 1000, amount: 1, remaining: 250, expected: '250' },
  { name: 'tiny spot BTC', base: 'BTC', step: 0.000001, contractSize: 1, amount: 0.000006, remaining: 0.000002, expected: '0.000002' },
  { name: 'inverse BTC', base: 'BTC', step: 1, contractSize: 1, amount: 1, remaining: 0.00001, expected: '0.00001', isInverse: true },
];

describe.each(sizeCases)('$name order annotations', (size) => {
  const market = {
    symbol: `${size.base}USDT`, base: size.base, precision: { amount: size.step, price: 0.1 },
    contractSize: size.contractSize, isInverse: size.isInverse,
  } as Market;
  const order = {
    id: 'order-1', symbol: market.symbol, status: OrderStatus.Open, side: 'buy',
    amount: size.amount, remaining: size.remaining, price: 100, stopLoss: 95, takeProfit: 105,
  } as Order;

  it.each([
    { type: OrderType.Limit, label: 'Limit', price: 100 },
    { type: OrderType.StopLoss, label: 'SL', price: 95 },
    { type: OrderType.TakeProfit, label: 'TP', price: 105 },
    { type: OrderType.TrailingStopLoss, label: 'TSL', price: 100 },
  ])('shows the remaining base size for $label', ({ type, label, price }) => {
    expect(formatOrderLine({ ...order, type }, market)).toMatchObject({ label: `${label} ${size.expected}`, price });
  });

  it('passes market precision through the chart command', async () => {
    const store = new DefaultStore();
    const ticker: Ticker = {
      id: market.id, symbol: market.symbol, last: 100, bid: 0, ask: 0, mark: 0, index: 0,
      percentage: 0, openInterest: 0, fundingRate: 0, fundingTime: 0, volume: 0, quoteVolume: 0,
    };
    store.update({
      markets: [market], orders: [{ ...order, type: OrderType.Limit }],
      tickers: [ticker],
    });
    const exchange = {
      store,
      fetchOHLCV: vi.fn().mockResolvedValue([
        { timestamp: 1600000000, open: 99, close: 101, high: 110, low: 90, volume: 1 },
      ]),
    } as unknown as ExchangeLike;
    const print = vi.fn();
    const adapter = { print } as unknown as CLIPlatformAdapter;
    const result = await new ChartCommand().execute({ raw: 'chart', args: { _: ['chart'] }, symbol: market.symbol, accountName: undefined, exchange, adapter });

    expect(result.success).toBe(true);
    expect(print.mock.calls.flat(2).join('\n')).toContain(`Limit ${size.expected}`);
  });
});

it('continues to omit market orders and orders without a valid chart price', () => {
  const order = { amount: 1, remaining: 1, price: 100 } as Order;
  expect(formatOrderLine({ ...order, type: OrderType.Market })).toBeNull();
  expect(formatOrderLine({ ...order, type: OrderType.Limit, price: 0 })).toBeNull();
});
