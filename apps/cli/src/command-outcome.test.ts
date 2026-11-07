import { expect, it, vi } from 'vitest';

import { executeCommand } from '../../../packages/cli-core/src/commands/CommandExecutor';
import type { ExchangeLike } from '../../../packages/cli-core/src/types';
import { WorkerPlatformAdapter } from '../../../packages/cli-core/src/worker-host/WorkerPlatformAdapter';
import { commandResultError, runCommandWithRetries } from './command-outcome';

it.each(['COMMAND_TIMEOUT', 'ENGINE_DISPOSED', 'ENGINE_DEAD', 'COMMAND_DEADLINE_FAILED', 'ENGINE_PROTOCOL_ERROR'])(
  'preserves real buy-command attribution and refuses retry after %s with unknown outcome', async (code) => {
    const interrupted = Object.assign(new Error('Reconcile before retrying'), {
      code, command: 'placeOrders', outcome: 'unknown', clientOrderIds: ['caller-parent', 'caller-child'],
    });
    const { exchange, placeOrder } = fixture(interrupted);
    const run = async () => {
      const result = await executeCommand('buy 100$', 'BTCUSDT', undefined, exchange, new WorkerPlatformAdapter(exchange));
      if (result.success === false) throw commandResultError(result);
    };
    const failure = await runCommandWithRetries(run, 3).catch((error: unknown) => error);
    expect(placeOrder).toHaveBeenCalledTimes(1);
    expect(failure).toMatchObject({
      message: 'Reconcile before retrying', code, command: 'placeOrders', outcome: 'unknown',
      clientOrderIds: ['caller-parent', 'caller-child'], alreadyReported: true,
    });
  },
);

it('keeps accepted partial order IDs through the actual CLI and refuses to repeat the whole placement', async () => {
  const rejected = Object.assign(new Error('Second order rejected'), {
    partialOrderIds: ['accepted-native-id'], partialOrderCount: 1, totalOrderCount: 2, failedOrderIndex: 1,
  });
  const { exchange, placeOrder } = fixture(rejected);
  const failure = await runCommandWithRetries(async () => {
    const result = await executeCommand('buy 100$', 'BTCUSDT', undefined, exchange, new WorkerPlatformAdapter(exchange));
    if (result.success === false) throw commandResultError(result);
  }, 3).catch((error: unknown) => error);
  expect(placeOrder).toHaveBeenCalledTimes(1);
  expect(failure).toMatchObject({
    message: 'Second order rejected', partialOrderIds: ['accepted-native-id'],
    partialOrderCount: 1, totalOrderCount: 2, failedOrderIndex: 1, alreadyReported: true,
  });
  expect(failure).not.toHaveProperty('outcome');
});

it('allows the requested retries for known rejections and preserves terminal metadata', async () => {
  const rejected = Object.assign(new Error('Venue rejected the order'), { code: 400, alreadyReported: true });
  const run = vi.fn().mockRejectedValue(rejected);
  const failure = await runCommandWithRetries(run, 2).catch((error: unknown) => error);
  expect(run).toHaveBeenCalledTimes(3);
  expect(failure).toMatchObject({ message: '(after 2 retries) Venue rejected the order', code: 400, alreadyReported: true });
  expect(failure).not.toHaveProperty('outcome');
});

it('stops retrying as soon as a known rejection is followed by success', async () => {
  const run = vi.fn().mockRejectedValueOnce(new Error('Temporary known rejection')).mockResolvedValue(undefined);
  await expect(runCommandWithRetries(run, 3)).resolves.toBeUndefined();
  expect(run).toHaveBeenCalledTimes(2);
});

it('does not require an ID array to respect a positive known accepted count', async () => {
  const error = Object.assign(new Error('One accepted order'), { partialOrderCount: 1 });
  const run = vi.fn().mockRejectedValue(error);
  await expect(runCommandWithRetries(run, 3)).rejects.toBe(error);
  expect(run).toHaveBeenCalledTimes(1);
});

it.each([-1, 11, NaN, Infinity, 1.5])('refuses an invalid retry count %s before dispatch', async (retries) => {
  const run = vi.fn();
  await expect(runCommandWithRetries(run, retries)).rejects.toThrow('Retry count must be from 0 to 10');
  expect(run).not.toHaveBeenCalled();
});

function fixture(error: Error) {
  const placeOrder = vi.fn().mockRejectedValue(error);
  const exchange = {
    name: 'binance', store: {
      markets: [{ symbol: 'BTCUSDT', base: 'BTC', quote: 'USDT', precision: { amount: 0.001, price: 0.01 } }],
      tickers: [{ symbol: 'BTCUSDT', last: 50000 }], orders: [], positions: [],
    },
    emitter: { emit: vi.fn(), on: vi.fn(), off: vi.fn() }, placeOrder,
  } as unknown as ExchangeLike;
  return { exchange, placeOrder };
}
