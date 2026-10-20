import { describe, expect, it, vi } from 'vitest';
import type { Exchange } from '@tealstreet/safe-cex/src/exchanges/base';
import { routeCommands } from './router';

// Do not mock executeCommand: actual BuyCommand -> executor catch -> worker
// router is where structured errors used to become only a string.
describe('normal CLI worker command failure attribution', () => {
  it.each(['COMMAND_TIMEOUT', 'ENGINE_DISPOSED', 'ENGINE_DEAD'])(
    'preserves %s and caller IDs from a real buy command',
    async (code) => {
      const error = Object.assign(new Error('placement interrupted'), {
        code,
        command: 'placeOrders',
        outcome: 'unknown',
        timeoutMs: 120000,
        clientOrderIds: ['caller-1', 'caller-2'],
      });
      const { exchange, emit, placeOrder } = fixture(error);
      await routeCommands(exchange, 'BTCUSDT', 'buy 100$', 'request-1');
      expect(placeOrder).toHaveBeenCalledTimes(1);
      const replies = emit.mock.calls.filter(([event]) => event === 'exec-result');
      expect(replies).toHaveLength(1);
      expect(replies[0][1]).toMatchObject({
        reqId: 'request-1',
        handled: true,
        success: false,
        error: 'placement interrupted',
        errorReported: true,
        errorDetails: {
          message: 'placement interrupted',
          code,
          command: 'placeOrders',
          outcome: 'unknown',
          timeoutMs: 120000,
          clientOrderIds: ['caller-1', 'caller-2'],
        },
      });
      expect(structuredClone(replies[0][1])).toEqual(replies[0][1]);
    },
  );

  it('reports an actual buy rejection once when no reqId was requested', async () => {
    const { exchange, emit, placeOrder } = fixture(
      Object.assign(new Error('write interrupted'), {
        code: 'COMMAND_TIMEOUT',
        command: 'placeOrders',
        outcome: 'unknown',
        clientOrderId: 'caller',
      }),
    );
    await routeCommands(exchange, 'BTCUSDT', 'buy 100$');
    expect(placeOrder).toHaveBeenCalledTimes(1);
    const logs = emit.mock.calls.filter(([event]) => event === 'exec-log');
    expect(logs).toEqual([['exec-log', { message: 'Error: Command failed: write interrupted' }]]);
    expect(emit.mock.calls.some(([event]) => event === 'exec-result')).toBe(false);
  });

  it('preserves acknowledged partial IDs as known rejection without claiming all writes are unknown', async () => {
    const error = Object.assign(new Error('second leg rejected'), {
      code: 'ORDER_REJECTED',
      partialOrderIds: ['venue-1'],
      partialOrderCount: 1,
      totalOrderCount: 2,
      failedOrderIndex: 1,
    });
    const { exchange, emit, placeOrder } = fixture(error);
    await routeCommands(exchange, 'BTCUSDT', 'buy 100$', 'partial');
    expect(placeOrder).toHaveBeenCalledTimes(1);
    const result = emit.mock.calls.find(([event]) => event === 'exec-result')?.[1];
    expect(result).toMatchObject({
      reqId: 'partial',
      success: false,
      error: 'second leg rejected',
      errorDetails: {
        code: 'ORDER_REJECTED',
        partialOrderIds: ['venue-1'],
        partialOrderCount: 1,
        totalOrderCount: 2,
        failedOrderIndex: 1,
      },
    });
    expect(result.errorDetails).not.toHaveProperty('outcome');
    expect(result.errorDetails).not.toHaveProperty('clientOrderIds');
  });
});

function fixture(error: Error) {
  const emit = vi.fn();
  const placeOrder = vi.fn().mockRejectedValue(error);
  const exchange = {
    name: 'binance',
    store: {
      markets: [{ symbol: 'BTCUSDT', base: 'BTC', quote: 'USDT', precision: { amount: 0.001, price: 0.01 } }],
      tickers: [{ symbol: 'BTCUSDT', last: 50000 }],
      orders: [],
      positions: [],
    },
    emitter: { emit, on: vi.fn(), off: vi.fn() },
    placeOrder,
  } as unknown as Exchange;
  return { exchange, emit, placeOrder };
}
