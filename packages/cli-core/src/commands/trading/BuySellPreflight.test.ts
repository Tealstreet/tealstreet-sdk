import { describe, expect, it, vi } from 'vitest';
import { executeCommand } from '../CommandExecutor';
import { MemoryPreprocessStore } from '../../preprocess/memoryStore';
import type { ExchangeLike } from '../../types';
import { WorkerPlatformAdapter } from '../../worker-host/WorkerPlatformAdapter';
import { commandResultError, runCommandWithRetries } from '../../../../../apps/cli/src/command-outcome';

function fixture() {
  const placeOrder = vi.fn().mockResolvedValue(['accepted-id']);
  const exchange = {
    name: 'binance',
    store: {
      markets: [
        { symbol: 'BTCUSDT', base: 'BTC', quote: 'USDT', precision: { amount: 0.001, price: 0.01 } },
        { symbol: 'ETHUSDT', base: 'ETH', quote: 'USDT', precision: { amount: 0.01, price: 0.01 } },
      ],
      tickers: [
        { symbol: 'BTCUSDT', last: 50000, bid: 49000, ask: 49000 },
        { symbol: 'ETHUSDT', last: 3000, bid: 49000, ask: 49000 },
      ],
      positions: [],
      orders: [],
      balance: { free: 1000, used: 0, total: 1000 },
    },
    placeOrder,
    emitter: { on: vi.fn(), off: vi.fn(), emit: vi.fn() },
  } as unknown as ExchangeLike;
  const store = new MemoryPreprocessStore();
  const run = async (command: string) => {
    const result = await executeCommand(
      command,
      'BTCUSDT',
      undefined,
      exchange,
      new WorkerPlatformAdapter(exchange),
      undefined,
      store,
    );
    if (!result.success) throw commandResultError(result);
    return result;
  };
  return { exchange, placeOrder, store, run };
}

function deferredHealthFixture() {
  const f = fixture();
  const health = { publicHealthy: true, publicInitialized: true, privateHealthy: true, privateInitialized: true };
  f.exchange.store!.socketHealth = health;
  f.store.setBudget(5000);
  f.store.addBudgetSpent(25);
  let release: ((ids: string[]) => void) | undefined;
  f.placeOrder
    .mockImplementationOnce(() => new Promise<string[]>((resolve) => { release = resolve; }))
    .mockResolvedValueOnce(['second-id']);
  return {
    ...f,
    health,
    finishFirst(ids: string[]) {
      if (!release) throw new Error('First placement has not started');
      release(ids);
    },
  };
}

describe.each(['buy', 'sell'])('%s fan-out preflight', (side) => {
  describe.each([
    { name: 'market', args: '' },
    { name: 'limit', args: 'at 49000' },
    { name: 'best', args: 'best' },
  ])('$name health during a pending write', ({ args: mode }) => {
    it.each([false, true])('stops later legs without retrying accepted work (empty IDs=%s)', async (emptyIds) => {
      const { run, placeOrder, store, health, finishFirst } = deferredHealthFixture();
      const acceptedIds = emptyIds ? [] : ['first-id'];
      const attempt = vi.fn(async () => { await run(`${side} BTCUSDT,ETHUSDT $1000 ${mode}`); });
      const pending = runCommandWithRetries(attempt, 1).catch((error: unknown) => error);
      await vi.waitFor(() => expect(placeOrder).toHaveBeenCalledOnce());
      expect(store.getBudgetSpent()).toBe(25);
      health.privateHealthy = false;
      finishFirst(acceptedIds);
      const error = await pending;
      expect(placeOrder).toHaveBeenCalledOnce();
      expect(attempt).toHaveBeenCalledOnce();
      expect(error).toMatchObject({
        message: expect.stringMatching(/degraded/i),
        partialOrderIds: acceptedIds,
        partialOrderCount: 1,
      });
      expect(error).not.toHaveProperty('outcome');
      expect(store.getBudgetSpent()).toBe(25 + (mode ? 980 : 1000));
    });

    it('keeps both writes and their budget charges when health stays good', async () => {
      const { run, placeOrder, store, finishFirst } = deferredHealthFixture();
      const pending = run(`${side} BTCUSDT,ETHUSDT $1000 ${mode}`);
      await vi.waitFor(() => expect(placeOrder).toHaveBeenCalledOnce());
      expect(store.getBudgetSpent()).toBe(25);
      finishFirst(['first-id']);
      await expect(pending).resolves.toMatchObject({ data: { orderIds: ['first-id', 'second-id'] } });
      expect(placeOrder).toHaveBeenCalledTimes(2);
      expect(store.getBudgetSpent()).toBe(25 + (mode ? 1960 : 1990));
    });

    it('refuses already degraded health before the first write', async () => {
      const { run, placeOrder, store, health } = deferredHealthFixture();
      health.privateHealthy = false;
      await expect(run(`${side} BTCUSDT,ETHUSDT $1000 ${mode}`)).rejects.toThrow(/degraded/i);
      expect(placeOrder).not.toHaveBeenCalled();
      expect(store.getBudgetSpent()).toBe(25);
    });

    it('allows reduce-only writes after health degrades without charging budget', async () => {
      const { run, placeOrder, store, health, finishFirst } = deferredHealthFixture();
      const pending = run(`${side} BTCUSDT,ETHUSDT $1000 ${mode} reduce`);
      await vi.waitFor(() => expect(placeOrder).toHaveBeenCalledOnce());
      health.privateHealthy = false;
      finishFirst(['first-id']);
      await expect(pending).resolves.toMatchObject({ data: { orderIds: ['first-id', 'second-id'] } });
      expect(placeOrder).toHaveBeenCalledTimes(2);
      expect(store.getBudgetSpent()).toBe(25);
    });
  });

  it.each(['', 'at 100', 'best'])('refuses the later coarse-lot symbol before any write: %s', async (mode) => {
    const { run, exchange, placeOrder } = fixture();
    exchange.store!.markets!.forEach((market, index) => {
      market.symbol = index === 0 ? 'AAAUSDC' : 'BBBUSDC';
      market.base = index === 0 ? 'AAA' : 'BBB';
      market.precision.amount = index === 0 ? 0.1 : 1;
    });
    exchange.store!.tickers!.forEach((ticker, index) => {
      Object.assign(ticker, { symbol: index === 0 ? 'AAAUSDC' : 'BBBUSDC', last: 100, bid: 100, ask: 100 });
    });
    await expect(
      runCommandWithRetries(async () => {
        await run(`${side} AAAUSDC,BBBUSDC $30 ${mode}`);
      }, 1),
    ).rejects.toThrow(/rounds to 0 BBBUSDC/);
    expect(placeOrder).not.toHaveBeenCalled();
  });

  it.each([undefined, true])('refuses a lost or unusable placement capability before writing (%s)', async (capability) => {
    const { run, exchange, placeOrder, store } = fixture();
    let reads = 0;
    Object.defineProperty(exchange, 'placeOrder', {
      get: () => ++reads === 1 ? placeOrder : capability,
    });
    await expect(run(`${side} BTCUSDT,ETHUSDT $1000 best`)).rejects.toThrow(
      'Exchange does not support placing orders',
    );
    expect(placeOrder).not.toHaveBeenCalled();
    expect(store.getBudgetSpent()).toBe(0);
  });

  it.each(['', 'at 49000', 'best'])('preserves the placement receiver (%s)', async (mode) => {
    const { run, exchange, placeOrder } = fixture();
    exchange.placeOrder = function (order) {
      expect(this).toBe(exchange);
      return placeOrder(order);
    };
    await expect(run(`${side} $100 ${mode}`)).resolves.toMatchObject({ success: true });
    expect(placeOrder).toHaveBeenCalledOnce();
  });

  it('waits for each accepted leg before placing and charging the next', async () => {
    const { run, placeOrder, store } = fixture();
    store.setBudget(2000);
    let finishFirst: (() => void) | undefined;
    placeOrder.mockImplementationOnce(() => new Promise<string[]>((resolve) => {
      finishFirst = () => resolve(['first-id']);
    })).mockResolvedValueOnce(['second-id']);
    const pending = run(`${side} BTCUSDT,ETHUSDT $1000 best`);
    await vi.waitFor(() => expect(placeOrder).toHaveBeenCalledOnce());
    expect(store.getBudgetSpent()).toBe(0);
    if (!finishFirst) throw new Error('First placement did not start');
    finishFirst();
    await expect(pending).resolves.toMatchObject({ data: { orderIds: ['first-id', 'second-id'] } });
    expect(placeOrder).toHaveBeenCalledTimes(2);
    expect(store.getBudgetSpent()).toBe(1960);
  });

  it.each(['at 49000', 'best'])('refuses a later sub-lot leg before any write, including retry: %s', async (mode) => {
    const { run, placeOrder, store } = fixture();
    store.setBudget(10000);
    await expect(
      runCommandWithRetries(async () => {
        await run(`${side} BTCUSDT,ETHUSDT $100 ${mode}`);
      }, 1),
    ).rejects.toThrow(/rounds to 0 ETHUSDT/);
    expect(placeOrder).not.toHaveBeenCalled();
    expect(store.getBudgetSpent()).toBe(0);
  });

  it.each(['', 'at 49000', 'best'])('checks the whole command budget before any write: %s', async (mode) => {
    const { run, placeOrder, store } = fixture();
    store.setBudget(1500);
    store.addBudgetSpent(100);
    await expect(run(`${side} $1000 ${mode} BTCUSDT,ETHUSDT`)).rejects.toThrow(/budget/);
    expect(placeOrder).not.toHaveBeenCalled();
    expect(store.getBudgetSpent()).toBe(100);
  });

  it.each(['', 'at 49000', 'best'])('runs shared policy for every leg before any write: %s', async (mode) => {
    const { run, exchange, placeOrder } = fixture();
    exchange.name = 'bybit';
    exchange.store!.markets![1].info = { symbolType: 'stock' };
    await expect(run(`${side} BTCUSDT,ETHUSDT $1000 ${mode}`)).rejects.toThrow();
    expect(placeOrder).not.toHaveBeenCalled();
  });

  it.each(['$600', '60%bal'])('refuses above-cap limit and best notional for %s', async (size) => {
    for (const mode of ['at 60000', 'best']) {
      const { run, exchange, placeOrder, store } = fixture();
      Object.assign(exchange.store!.tickers![0], { bid: 60000, ask: 60000 });
      store.setBudget(500);
      await expect(run(`${side} ${size} ${mode}`)).rejects.toThrow(/budget/);
      expect(placeOrder).not.toHaveBeenCalled();
      expect(store.getBudgetSpent()).toBe(0);
    }
  });

  it('allows and charges the actual lower-price limit notional', async () => {
    const { run, placeOrder, store } = fixture();
    store.setBudget(400);
    await expect(run(`${side} $400 at 40000`)).resolves.toMatchObject({ success: true });
    expect(placeOrder).toHaveBeenCalledWith(expect.objectContaining({ amount: 0.01, price: 40000 }));
    expect(store.getBudgetSpent()).toBe(400);
  });

  it.each([false, true])('preserves accepted legs and native failure attribution (unknown=%s)', async (unknown) => {
    const { run, placeOrder, store } = fixture();
    store.setBudget(5000);
    const failure = Object.assign(new Error('Second write failed'), {
      code: 'ENGINE_DEAD',
      command: 'placeOrders',
      clientOrderIds: ['second-client-id'],
      ...(unknown ? { outcome: 'unknown' } : { partialOrderIds: ['native-partial-id'], partialOrderCount: 1 }),
    });
    placeOrder.mockResolvedValueOnce(['first-entry-id', 'first-stop-id']).mockRejectedValueOnce(failure);
    const error = await runCommandWithRetries(async () => {
      await run(`${side} BTCUSDT,ETHUSDT $1000 best`);
    }, 1).catch((e: unknown) => e);
    expect(placeOrder).toHaveBeenCalledTimes(2);
    expect(error).toMatchObject({
      message: 'Second write failed',
      code: failure.code,
      command: failure.command,
      clientOrderIds: failure.clientOrderIds,
      partialOrderIds: unknown
        ? ['first-entry-id', 'first-stop-id']
        : ['first-entry-id', 'first-stop-id', 'native-partial-id'],
      partialOrderCount: unknown ? 2 : 3,
      ...(unknown ? { outcome: 'unknown' } : {}),
    });
    if (!unknown) expect(error).not.toHaveProperty('outcome');
    expect(store.getBudgetSpent()).toBeCloseTo(980);
  });

  it('submits only preflighted payloads and keeps accepted IDs in order', async () => {
    const { run, placeOrder, exchange, store } = fixture();
    store.setBudget(2000);
    placeOrder
      .mockImplementationOnce(async () => {
        exchange.store!.markets![1].precision.amount = 100;
        exchange.store!.tickers![1].last = 1e9;
        return ['first-entry-id', 'first-stop-id'];
      })
      .mockResolvedValueOnce(['second-id']);
    const result = await run(`${side} BTCUSDT,ETHUSDT $1000 best`);
    expect(result.data).toMatchObject({ orderIds: ['first-entry-id', 'first-stop-id', 'second-id'] });
    expect(placeOrder).toHaveBeenNthCalledWith(
      2,
      expect.objectContaining({ symbol: 'ETHUSDT', amount: 0.02, price: 49000 }),
    );
    expect(store.getBudgetSpent()).toBe(1960);
  });

  it('blocks retry even when an accepted leg returns no IDs', async () => {
    const { run, placeOrder, store } = fixture();
    store.setBudget(5000);
    placeOrder.mockResolvedValueOnce([]).mockRejectedValueOnce(new Error('Second write rejected'));
    const error = await runCommandWithRetries(async () => {
      await run(`${side} BTCUSDT,ETHUSDT $1000 best`);
    }, 1).catch((e: unknown) => e);
    expect(placeOrder).toHaveBeenCalledTimes(2);
    expect(error).toMatchObject({ message: 'Second write rejected', partialOrderIds: [], partialOrderCount: 1 });
    expect(store.getBudgetSpent()).toBe(980);
  });

  it.each([false, true])('charges rounded submitted prices in native contract units (inverse=%s)', async (inverse) => {
    const { run, placeOrder, exchange, store } = fixture();
    const market = exchange.store!.markets![0];
    market.contractSize = inverse ? 100 : 0.01;
    market.isInverse = inverse;
    market.precision = { amount: 1, price: 1000 };
    store.setBudget(1200);
    await expect(run(`${side} $1200 at 59600`)).resolves.toMatchObject({ success: true });
    expect(placeOrder).toHaveBeenCalledWith(expect.objectContaining({ amount: inverse ? 12 : 2, price: 60000 }));
    expect(store.getBudgetSpent()).toBe(1200);
  });
});
