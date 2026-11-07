import { describe, expect, it, vi } from 'vitest';

import { inverseUnitsFrame } from '@tealstreet/safe-cex/src/__tests__/fixtures/scxfFrame';
import { FrameApplier } from '@tealstreet/safe-cex/src/store/frameApplier';
import { DefaultStore } from '@tealstreet/safe-cex/src/store/store.base';
import { OrderSide, OrderType, type Ticker } from '@tealstreet/safe-cex/src/types';
import type { ChaserOrderInstance } from '@tealstreet/safe-cex/src/webworker/chaserOrder/ChaserOrderInstance';
import type { CLIPlatformAdapter, ExchangeLike } from '../../types';
import { ChartCommand, formatOrderLine } from './ChartCommand';
import { formatChaserStatus } from './ChaseCommand';
import { formatMultiAccountOrdersTable, formatOrdersTable } from './MonitorCommand';
import { OrdersCommand } from './OrdersCommand';

const strip = (text: string) => text.replace(/\x1b\[[0-9;]*m/g, '');
const sizeCells = (table: string) =>
  strip(table)
    .split('\n')
    .filter((line) => line.includes('BTCUSD'))
    .map((line) =>
      line
        .split('│')
        .map((cell) => cell.trim())
        .filter(Boolean),
    );

// Same $500 remaining exposure: COIN-M publishes contracts, OKX/Bybit publish base.
describe.each([
  { name: 'Binance COIN-M', units: 4, contractSize: 100, remaining: 5, native: 5 },
  { name: 'OKX inverse', units: 3, contractSize: 100, remaining: 0.01, native: 5 },
  { name: 'Bybit inverse', units: 2, contractSize: 1, remaining: 0.01, native: 500 },
])('$name order display', (profile) => {
  function fixture() {
    const frame = inverseUnitsFrame({
      units: profile.units,
      contractSize: profile.contractSize,
      native: true,
      orders: [{ id: 'partial', amount: 7, filled: 2, remaining: profile.remaining, native: profile.native }],
    });
    frame.orders()!.price[0] = 50_000;
    const store = new DefaultStore();
    new FrameApplier(store).apply(frame);
    expect(store.orders[0].remaining).toBe(profile.remaining);
    expect(store.orders[0].extra?.remainingNotional).toBe(500);
    return { name: profile.name, store } as ExchangeLike & { store: DefaultStore };
  }

  it('converts the decoded remainder in single and multi-account tables', () => {
    const exchange = fixture();
    expect(sizeCells(formatOrdersTable(exchange))[0][3]).toBe('0.01');
    expect(sizeCells(formatMultiAccountOrdersTable([{ accountName: 'coin', exchange }]))[0][4]).toBe('0.01');
  });

  it('converts the decoded remainder in the orders command', async () => {
    const exchange = fixture();
    const print = vi.fn();
    await new OrdersCommand().execute({
      raw: 'o',
      args: { _: ['o'] },
      symbol: 'BTCUSD',
      accountName: undefined,
      exchange,
      adapter: { print } as unknown as CLIPlatformAdapter,
    });
    expect(sizeCells(print.mock.calls.flat(2).join('\n'))[0][3]).toBe('0.01');
  });

  it.each([
    { type: OrderType.Limit, label: 'Limit' },
    { type: OrderType.StopLoss, label: 'SL' },
    { type: OrderType.TakeProfit, label: 'TP' },
    { type: OrderType.TrailingStopLoss, label: 'TSL' },
  ])('converts the decoded remainder in $label chart and watch annotations', ({ type, label }) => {
    const exchange = fixture();
    expect(formatOrderLine({ ...exchange.store.orders[0], type }, exchange.store.markets[0])?.label).toBe(
      `${label} 0.01`,
    );
  });

  it('converts the decoded remainder in rendered charts', async () => {
    const exchange = fixture();
    exchange.store.update({ tickers: [{ symbol: 'BTCUSD', last: 50_000 } as Ticker] });
    exchange.fetchOHLCV = vi
      .fn()
      .mockResolvedValue([
        { timestamp: 1600000000, open: 49_000, close: 50_000, high: 51_000, low: 48_000, volume: 1 },
      ]);
    const print = vi.fn();
    const result = await new ChartCommand().execute({
      raw: 'chart',
      args: { _: ['chart'] },
      symbol: 'BTCUSD',
      accountName: undefined,
      exchange,
      adapter: { print } as unknown as CLIPlatformAdapter,
    });
    expect(result.success).toBe(true);
    expect(print.mock.calls.flat(2).join('\n')).toContain('Limit 0.01');
  });

  it('converts the decoded remainder in the current chase order row', () => {
    const exchange = fixture();
    const instance = {
      options: { symbol: 'BTCUSD', side: OrderSide.Buy, size: 7 },
      state: { attempts: 1, stopReason: '' },
      remainingSize: 5,
      currentOrderIds: ['partial'],
      isStopped: false,
    } as unknown as ChaserOrderInstance;
    const output = strip(formatChaserStatus(instance, exchange));
    expect(
      output
        .split('\n')
        .find((line) => line.includes('Order:'))
        ?.split(' @ ')[1],
    ).toBe('0.01');
  });
});
