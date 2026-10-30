import type { StoreWorkerPayload } from '../StoreWorkerPayload';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { handlerArgs, NO_ALGOS, plainExchange, rustAlgoExchange } from '../../__tests__/fixtures/rustAlgoExchange';
import { setTwapLockManager } from '../twap/twapLock';
import { TwapStatus } from '../twap/TwapStatus';
import { TwapHandler } from './TwapHandler';

const settle = async () => {
  for (let i = 0; i < 10; i++) await Promise.resolve();
};

const options = {
  accountName: 'test-account',
  activeMarket: { symbol: 'BTCUSDT' },
  size: 1,
  duration: 10,
  ordersCount: 2,
  side: 'buy',
  randomnessPercentage: 0,
};
const restoredRow = {
  id: 'restored',
  options,
  state: {
    accountName: 'test-account',
    activeMarket: { symbol: 'BTCUSDT' },
    size: 1,
    ordersSizes: [0.5],
    side: 'buy',
    status: TwapStatus.Running,
    ordersCount: 2,
    ordersExecuted: 1,
    nextOrderAt: Date.now() + 5_000,
  },
};

describe('TwapHandler', () => {
  beforeEach(() => setTwapLockManager(null));
  afterEach(() => setTwapLockManager(undefined));

  it.each(['twap:start', 'twap:resume', 'twap:cleanup', 'twap:pause', 'twap:stop'] as const)(
    'sends %s to the account engine Rust algos',
    (op) => {
      const exchange = rustAlgoExchange();
      const data = op === 'twap:start' ? options : 'twap-1';
      new TwapHandler(...handlerArgs(exchange)).handle({ op, data } as StoreWorkerPayload);
      expect(exchange.routeAlgoOp).toHaveBeenCalledWith('test-account', op, expect.any(Function), expect.any(Function));
      expect(exchange.algoOp).toHaveBeenCalledWith(op, data);
    },
  );

  it('restores persisted rows into the Rust algos once it holds their lock', async () => {
    const exchange = rustAlgoExchange();
    new TwapHandler(...handlerArgs(exchange)).handle({ op: 'twap:state', data: [restoredRow] } as StoreWorkerPayload);
    expect(exchange.algoOp).not.toHaveBeenCalled();
    await settle();
    expect(exchange.algoOp).toHaveBeenCalledWith('twap:state', [restoredRow]);
  });

  it('posts the algos persisted rows to main, and stops listening on dispose', () => {
    const exchange = rustAlgoExchange();
    const ctx = { postMessage: vi.fn() } as unknown as Worker;
    const handler = new TwapHandler(...handlerArgs(exchange, ctx));
    const rows = { 'twap-1': { options, state: restoredRow.state } };
    exchange.listeners.get('twap:stateUpdate')!(rows);
    expect(ctx.postMessage).toHaveBeenCalledWith({ op: 'twap:stateUpdate', data: rows });

    handler.finishRustTwaps();
    expect(ctx.postMessage).toHaveBeenLastCalledWith({ op: 'twap:stateUpdate', data: {} });
    handler.dispose();
    expect(exchange.off).toHaveBeenCalledWith('twap:stateUpdate', expect.any(Function));
  });

  it('refuses a TWAP visibly on an engine build without algos', () => {
    const exchange = rustAlgoExchange('refuse');
    new TwapHandler(...handlerArgs(exchange)).handle({ op: 'twap:start', data: options } as StoreWorkerPayload);
    expect(exchange.algoOp).not.toHaveBeenCalled();
    expect(exchange.emitter.emit).toHaveBeenCalledWith('error', `twap:start ${NO_ALGOS}`);
  });

  it('refuses a TWAP visibly on an account that is not a Rust engine', () => {
    const exchange = plainExchange();
    new TwapHandler(...handlerArgs(exchange)).handle({ op: 'twap:start', data: options } as StoreWorkerPayload);
    expect(exchange.emitter.emit).toHaveBeenCalledWith(
      'error',
      'twap:start was not run: algos run only on a Rust engine account',
    );
  });

  it('ignores other ops', () => {
    const exchange = rustAlgoExchange();
    new TwapHandler(...handlerArgs(exchange)).handle({ op: 'chaser:start', data: {} } as StoreWorkerPayload);
    expect(exchange.routeAlgoOp).not.toHaveBeenCalled();
  });
});
