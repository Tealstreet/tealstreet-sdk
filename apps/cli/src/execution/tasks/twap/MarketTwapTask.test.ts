import { describe, expect, it, vi } from 'vitest';

import { BybitExchange } from '@tealstreet/safe-cex/src/exchanges/bybit/bybit.exchange';
import { OrderSide } from '@tealstreet/safe-cex/src/types';
import type { Market } from '@tealstreet/safe-cex/src/types';

import { createConfirmedOrders } from '../../confirmed';
import { MarketTwapTask } from './MarketTwapTask';

const market = {
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
} as Market;

const store = {
  subscribe: vi.fn(),
  markets: [market],
  marketsBySymbol: { BTCUSDT: market },
  tickersBySymbol: {},
  orders: [],
  positions: [],
} as never;

describe('MarketTwapTask', () => {
  it('places each due slice once and does not make up reserved quantity', async () => {
    const adapter = new BybitExchange('bybit', { key: 'k', secret: 's' }, store);
    adapter.placeOrder = vi.fn().mockResolvedValue(['oid-1']);
    adapter.getOrderByClientOrderId = vi.fn().mockResolvedValue({
      retCode: 0,
      result: { category: 'linear', list: [] },
    });
    const orders = createConfirmedOrders({ exchange: adapter, signerFingerprint: 'signer' });
    const task = new MarketTwapTask({
      orders,
      spec: {
        taskId: 'twap-1',
        market,
        side: OrderSide.Buy,
        target: { decimal: '0.03', atoms: '30', atomScale: 3 },
        quantityStep: { decimal: '0.01', atoms: '10', atomScale: 3 },
        minQuantity: { decimal: '0.01', atoms: '10', atomScale: 3 },
        contractSize: { decimal: '1', atoms: '1', atomScale: 0 },
        sliceCount: 3,
        startAtMs: 1_000,
        durationMs: 2_000,
      },
    });
    task.start();
    await task.placeDueSlices(1_000);
    expect(adapter.placeOrder).toHaveBeenCalledTimes(1);
    await task.placeDueSlices(3_000);
    expect(adapter.placeOrder).toHaveBeenCalledTimes(3);
    const report = task.report();
    expect(report.reservedUnresolved.atoms).toBe('30');
    expect(report.confirmedFilled.atoms).toBe('0');
  });

  it('projects contract-denominated order truth into base units in every slice report', async () => {
    const adapter = new BybitExchange('bybit', { key: 'k', secret: 's' }, store);
    adapter.placeOrder = vi.fn().mockResolvedValue(['oid-contract']);
    adapter.getOrderByClientOrderId = vi.fn().mockImplementation((_symbol, wireClientOrderId) =>
      Promise.resolve({
        retCode: 0,
        result: {
          category: 'linear',
          list: [
            {
              orderLinkId: wireClientOrderId,
              orderId: 'oid-contract',
              symbol: 'BTCUSDT',
              orderStatus: 'Filled',
              qty: '0.01',
              cumExecQty: '0.01',
            },
          ],
        },
      }),
    );
    const orders = createConfirmedOrders({ exchange: adapter, signerFingerprint: 'signer' });
    const task = new MarketTwapTask({
      orders,
      spec: {
        taskId: 'twap-contract',
        market,
        side: OrderSide.Buy,
        target: { decimal: '0.001', atoms: '1', atomScale: 3 },
        quantityStep: { decimal: '0.001', atoms: '1', atomScale: 3 },
        minQuantity: { decimal: '0.001', atoms: '1', atomScale: 3 },
        contractSize: { decimal: '0.1', atoms: '1', atomScale: 1 },
        sliceCount: 1,
        startAtMs: 1_000,
        durationMs: 0,
      },
    });

    task.start();
    await task.placeDueSlices(1_000);
    expect(adapter.placeOrder).toHaveBeenCalledWith(expect.objectContaining({ amount: 0.01 }));
    const [handle] = task.unresolvedHandles();
    await task.refreshHandle(handle!);

    expect(task.report()).toMatchObject({
      confirmedFilled: { decimal: '0.001', atoms: '1', atomScale: 3 },
      slices: [
        {
          requested: { decimal: '0.001', atoms: '1', atomScale: 3 },
          truth: {
            kind: 'terminal',
            observation: {
              total: { decimal: '0.001', atoms: '1', atomScale: 3 },
              filled: { decimal: '0.001', atoms: '1', atomScale: 3 },
              residual: { decimal: '0', atoms: '0', atomScale: 0 },
            },
          },
        },
      ],
    });
  });

  it('retains a latched incident reservation after the operator fences the task', async () => {
    const adapter = new BybitExchange('bybit', { key: 'k', secret: 's' }, store);
    adapter.placeOrder = vi.fn().mockResolvedValue(['oid-incident']);
    adapter.getOrderByClientOrderId = vi.fn().mockImplementation((_symbol, wireClientOrderId) =>
      Promise.resolve({
        retCode: 0,
        result: {
          category: 'linear',
          list: [
            {
              orderLinkId: wireClientOrderId,
              orderId: 'oid-incident',
              symbol: 'BTCUSDT',
              orderStatus: 'Filled',
              qty: '0.02',
              cumExecQty: '0.02',
              avgPrice: '100000',
            },
          ],
        },
      }),
    );
    const orders = createConfirmedOrders({ exchange: adapter, signerFingerprint: 'signer' });
    const task = new MarketTwapTask({
      orders,
      spec: {
        taskId: 'twap-incident',
        market,
        side: OrderSide.Buy,
        target: { decimal: '0.01', atoms: '10', atomScale: 3 },
        quantityStep: { decimal: '0.01', atoms: '10', atomScale: 3 },
        minQuantity: { decimal: '0.01', atoms: '10', atomScale: 3 },
        contractSize: { decimal: '1', atoms: '1', atomScale: 0 },
        sliceCount: 1,
        startAtMs: 1_000,
        durationMs: 0,
      },
    });

    task.start();
    await task.placeDueSlices(1_000);
    const [handle] = task.unresolvedHandles();
    expect(handle).toBeDefined();
    await task.refreshHandle(handle!);

    expect(task.report()).toMatchObject({
      lifecycle: 'paused',
      slices: [{ truth: { kind: 'incident' } }],
    });
    expect(task.hasUnresolved()).toBe(true);
    expect(task.isSettled()).toBe(false);
    expect(task.unresolvedHandles()).toEqual([handle]);

    task.stop();
    expect(task.hasUnresolved()).toBe(true);
    expect(task.unresolvedHandles()).toEqual([handle]);
    expect(task.isSettled()).toBe(false);
    expect(task.report()).toMatchObject({
      lifecycle: 'stopping',
      reservedUnresolved: { atoms: '10' },
      slices: [{ truth: { kind: 'incident' } }],
    });
  });
});
