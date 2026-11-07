import { EventEmitter } from 'node:events';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { OrderSide } from '@tealstreet/safe-cex/src/types';
import { ChaserOrderInstance } from '@tealstreet/safe-cex/src/webworker/chaserOrder/ChaserOrderInstance';
import { chaserOrderManagerInstance } from '@tealstreet/safe-cex/src/webworker/chaserOrder/ChaserOrderManager';

import { chaseAbandonedByExit, watchOneShotChases } from './chaseAbandoned';

describe('chaseAbandonedByExit', () => {
  const instances = chaserOrderManagerInstance.instances as unknown as { isStopped: boolean }[];
  afterEach(() => instances.splice(0));

  it('names a chase the CLI manager is running, which the exchange store never lists', () => {
    instances.push({ isStopped: false });
    expect(chaseAbandonedByExit()).toMatch(/nothing is chasing now/);
  });

  it('says nothing once no chase is running', () => {
    expect(chaseAbandonedByExit()).toBeUndefined();
    instances.push({ isStopped: true });
    expect(chaseAbandonedByExit()).toBeUndefined();
  });
});

describe('watchOneShotChases', () => {
  const startChase = (placeOrder: () => Promise<string[]>) => {
    const emitter = new EventEmitter();
    emitter.on('error', () => undefined);
    const exchange = {
      name: 'bitget',
      emitter,
      on: emitter.on.bind(emitter),
      off: emitter.off.bind(emitter),
      placeOrder: vi.fn(placeOrder),
      cancelOrders: vi.fn(async () => []),
      store: {
        orders: [],
        positions: [],
        markets: [{ symbol: 'FILUSDT', precision: { price: 0.001, amount: 1 } }],
        marketsBySymbol: { FILUSDT: { symbol: 'FILUSDT', precision: { price: 0.001, amount: 1 } } },
        tickers: [{ symbol: 'FILUSDT', bid: 5, ask: 5.001, last: 5 }],
        tickersBySymbol: { FILUSDT: { symbol: 'FILUSDT', bid: 5, ask: 5.001, last: 5 } },
      },
    };
    const settle = watchOneShotChases(exchange as never);
    const instance = new ChaserOrderInstance(exchange as never, {
      symbol: 'FILUSDT',
      size: 1,
      side: OrderSide.Buy,
      maxAttempts: 10,
      delay: 1_000,
      origin: 'cli',
    });
    chaserOrderManagerInstance.add(instance);
    return { settle, instance, exchange };
  };

  afterEach(() => {
    for (const chase of [...chaserOrderManagerInstance.instances]) chaserOrderManagerInstance.remove(chase.id);
  });

  it('fails a chase whose first placements all fail, naming why', async () => {
    const { settle, exchange } = startChase(async () => {
      throw new Error('placeOrders refused');
    });
    await expect(settle()).resolves.toEqual({
      code: 'COMMAND_FAILED',
      error: expect.stringMatching(/Chase on FILUSDT placed no order: Failed to place orders/),
    });
    expect(exchange.placeOrder).toHaveBeenCalled();
    expect(getActive()).toEqual([]);
  });

  it('still reports a chase with a resting order as abandoned by the exit', async () => {
    const { settle } = startChase(async () => ['order-1']);
    await expect(settle()).resolves.toMatchObject({ code: 'CHASE_ABANDONED' });
  });

  it('has nothing to say for a command that started no chase', async () => {
    const settle = watchOneShotChases({ emitter: new EventEmitter() } as never);
    await expect(settle()).resolves.toBeUndefined();
  });

  const getActive = () => chaserOrderManagerInstance.instances.filter((chase) => !chase.isStopped);
});
