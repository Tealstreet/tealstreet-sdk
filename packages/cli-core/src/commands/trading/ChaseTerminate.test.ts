import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { ChaseCommand } from './ChaseCommand';
import type { CommandContext } from '../Command';
import { OrderSide } from '@tealstreet/safe-cex/src/types';
import type { ExchangeLike } from '../../types';
import { chaserOrderManagerInstance } from '@tealstreet/safe-cex/src/webworker/chaserOrder/ChaserOrderManager';

function makeExchange(): ExchangeLike {
  const markets = [
    { symbol: 'BTCUSDT', base: 'BTC', quote: 'USDT', precision: { amount: 0.001, price: 0.01 } },
    { symbol: 'ETHUSDT', base: 'ETH', quote: 'USDT', precision: { amount: 0.01, price: 0.01 } },
  ];
  const tickers = [
    { symbol: 'BTCUSDT', last: 50000, bid: 49995, ask: 50005 },
    { symbol: 'ETHUSDT', last: 3000, bid: 2999, ask: 3001 },
  ];
  return {
    // A REAL venue id, not 'test': every write now goes through the shared
    // order policy, which asks `exchangeMetadata` about round-down, infinity
    // stops and the reduce-only clamp. A made-up name answers "unknown venue"
    // and no longer exercises the branches these tests are about.
    name: 'binance',
    store: {
      markets,
      marketsBySymbol: Object.fromEntries(markets.map((m) => [m.symbol, m])),
      tickers,
      tickersBySymbol: Object.fromEntries(tickers.map((t) => [t.symbol, t])),
      orders: [],
      positions: [],
      balance: { used: 0, free: 1_000_000, total: 1_000_000, upnl: 0 },
      updateIds: { orders: 0, positions: 0, tickers: 0, balance: 0 },
    },
    emitter: { on: vi.fn(), off: vi.fn(), emit: vi.fn() },
    on: vi.fn(),
    off: vi.fn(),
    placeOrder: vi.fn().mockResolvedValue(['order-1']),
    cancelOrders: vi.fn().mockResolvedValue(undefined),
  } as unknown as ExchangeLike;
}

function ctx(
  args: string[],
  exchange: ExchangeLike,
  symbol = 'BTCUSDT',
  preprocessStore?: import('../../preprocess/memoryStore').MemoryPreprocessStore,
): CommandContext {
  return {
    raw: args.join(' '),
    args: { _: args },
    symbol,
    accountName: 'a',
    exchange,
    preprocessStore,
    adapter: {
      print: vi.fn(),
      printInfo: vi.fn(),
      printError: vi.fn(),
      printSuccess: vi.fn(),
      printWarning: vi.fn(),
      listAccounts: vi.fn().mockReturnValue([]),
      getAccount: vi.fn(),
      addAccount: vi.fn(),
      updateAccount: vi.fn(),
      removeAccount: vi.fn(),
      setDefaultAccount: vi.fn(),
      getDefaultSymbol: vi.fn(),
      setDefaultSymbol: vi.fn(),
      getActiveAccounts: vi.fn().mockReturnValue([]),
      createExchange: vi.fn(),
      startExchange: vi.fn(),
      stopExchange: vi.fn(),
    },
  } as unknown as CommandContext;
}

function clearChasers() {
  while (chaserOrderManagerInstance.instances.length > 0) {
    const inst = chaserOrderManagerInstance.instances[0];
    chaserOrderManagerInstance.removeKeepOrder(inst.id);
    if (chaserOrderManagerInstance.instances.includes(inst)) {
      chaserOrderManagerInstance.instances.splice(
        chaserOrderManagerInstance.instances.indexOf(inst),
        1,
      );
    }
  }
}

describe('ChaseCommand — `to <length>` parser', () => {
  let cmd: ChaseCommand;
  beforeEach(() => {
    cmd = new ChaseCommand();
  });

  it.each([
    ['49500', { type: 'price', value: 49500 }],
    ['1.5', { type: 'price', value: 1.5 }],
    ['1%', { type: 'percent', value: 1 }],
    ['0.5%', { type: 'percent', value: 0.5 }],
    ['$50', { type: 'usd', value: 50 }],
    ['100$', { type: 'usd', value: 100 }],
    ['$0.5', { type: 'usd', value: 0.5 }],
  ])('parses "%s" → %o', (input, expected) => {
    const r = (cmd as unknown as { parseTerminateLength: (s: string) => unknown }).parseTerminateLength(input);
    expect(r).toEqual(expected);
  });

  it.each(['', '  ', 'abc', '1%bal', '-5', '$', '%', '0', '0%', '$0'])(
    'rejects "%s" as malformed',
    (input) => {
      const r = (cmd as unknown as { parseTerminateLength: (s: string) => unknown }).parseTerminateLength(input);
      expect(r).toBeNull();
    },
  );
});

describe('ChaseCommand — `to` flag dispatch', () => {
  let cmd: ChaseCommand;
  beforeEach(() => {
    cmd = new ChaseCommand();
    clearChasers();
  });
  afterEach(() => {
    clearChasers();
  });

  it('errors when `to` has no length argument', async () => {
    const ex = makeExchange();
    const r = await cmd.execute(ctx(['chase', 'buy', '$100', 'to'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/'to' must be followed/);
  });

  it('errors on a malformed length', async () => {
    const ex = makeExchange();
    const r = await cmd.execute(ctx(['chase', 'buy', '$100', 'to', 'farther'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/invalid 'to' length/);
  });

  it('attaches absolute-price terminate to the spawned chaser', async () => {
    const ex = makeExchange();
    await cmd.execute(ctx(['chase', 'buy', '$100', 'to', '49500'], ex));
    expect(chaserOrderManagerInstance.instances.length).toBe(1);
    const inst = chaserOrderManagerInstance.instances[0];
    expect(inst.options.terminate).toBeDefined();
    expect(inst.options.terminate?.type).toBe('price');
    expect(inst.options.terminate?.value).toBe(49500);
    expect(inst.options.terminate?.anchorPrice).toBe(50000); // ticker.last
  });

  it('attaches percent terminate', async () => {
    const ex = makeExchange();
    await cmd.execute(ctx(['chase', 'buy', '$100', 'to', '1%'], ex));
    const inst = chaserOrderManagerInstance.instances[0];
    expect(inst.options.terminate).toEqual({
      type: 'percent',
      value: 1,
      anchorPrice: 50000,
    });
  });

  it('attaches usd terminate (both prefix and suffix forms)', async () => {
    const ex = makeExchange();
    await cmd.execute(ctx(['chase', 'buy', '$100', 'to', '$50'], ex));
    expect(chaserOrderManagerInstance.instances[0].options.terminate).toEqual({
      type: 'usd',
      value: 50,
      anchorPrice: 50000,
    });

    clearChasers();
    await cmd.execute(ctx(['chase', 'buy', '$100', 'to', '50$'], ex));
    expect(chaserOrderManagerInstance.instances[0].options.terminate?.value).toBe(50);
  });

  it('captures anchor from the trailing instrument when provided', async () => {
    const ex = makeExchange();
    await cmd.execute(ctx(['chase', 'buy', '$100', 'to', '1%', 'ETHUSDT'], ex));
    const inst = chaserOrderManagerInstance.instances[0];
    expect(inst.options.symbol).toBe('ETHUSDT');
    expect(inst.options.terminate?.anchorPrice).toBe(3000); // ETH ticker.last
  });

  it('uses sell-side anchor as expected', async () => {
    const ex = makeExchange();
    await cmd.execute(ctx(['chase', 'sell', '$100', 'to', '1%'], ex));
    const inst = chaserOrderManagerInstance.instances[0];
    expect(inst.options.side).toBe(OrderSide.Sell);
    expect(inst.options.terminate?.anchorPrice).toBe(50000);
  });

  it('does not attach terminate when `to` is absent', async () => {
    const ex = makeExchange();
    await cmd.execute(ctx(['chase', 'buy', '$100'], ex));
    expect(chaserOrderManagerInstance.instances[0].options.terminate).toBeUndefined();
  });

  it('errors when no anchor price is available for the symbol', async () => {
    const ex = makeExchange();
    // Wipe BTC's ticker so anchor lookup fails
    ex.store!.tickers = ex.store!.tickers!.filter((t) => t.symbol !== 'BTCUSDT');
    const c = ctx(['chase', 'buy', '$100', 'to', '1%'], ex);
    const r = await cmd.execute(c);
    expect(chaserOrderManagerInstance.instances).toHaveLength(0);
    expect(c.adapter.printError).toHaveBeenCalledWith(
      "No anchor price available for BTCUSDT — can't compute 'to' threshold",
    );
    expect(c.adapter.print).not.toHaveBeenCalled();
    expect(r.success).toBe(false);
    expect(r.error).toBe('No chasers started');
  });

  it('reports only symbols whose chasers actually started', async () => {
    const ex = makeExchange();
    (ex.store!.tickers!.find((t) => t.symbol === 'ETHUSDT') as { last?: number }).last = undefined;
    const c = ctx(['chase', 'buy', 'BTCUSDT,ETHUSDT', '$100', 'to', '1%'], ex);

    const r = await cmd.execute(c);

    expect(r.success).toBe(true);
    expect(chaserOrderManagerInstance.instances).toHaveLength(1);
    expect(chaserOrderManagerInstance.instances[0].options.symbol).toBe('BTCUSDT');
    expect(c.adapter.printError).toHaveBeenCalledWith(
      "No anchor price available for ETHUSDT — can't compute 'to' threshold",
    );
    expect(c.adapter.print).toHaveBeenCalledWith([
      expect.stringContaining('Chaser started for BTCUSDT'),
    ]);
    expect(c.adapter.print).toHaveBeenCalledWith([
      expect.not.stringContaining('ETHUSDT'),
    ]);
  });

  it("doesn't try to validate 'to' or the length token as a market symbol", async () => {
    const ex = makeExchange();
    // Without `to`, args = [chase, buy, $100, ETHUSDT] → ETH chase.
    // With `to 1%` between size and ETHUSDT, ETHUSDT must still win.
    await cmd.execute(ctx(['chase', 'buy', '$100', 'to', '1%', 'ETHUSDT'], ex));
    expect(chaserOrderManagerInstance.instances[0].options.symbol).toBe('ETHUSDT');
  });

  it('e2e: `chase buy 1000$` (suffix size) chases on the focused symbol', async () => {
    const ex = makeExchange();
    await cmd.execute(ctx(['chase', 'buy', '1000$'], ex));
    expect(chaserOrderManagerInstance.instances).toHaveLength(1);
    expect(chaserOrderManagerInstance.instances[0].options.symbol).toBe('BTCUSDT');
    // $1000 / 50000 = 0.02 BTC contracts
    expect(chaserOrderManagerInstance.instances[0].options.size).toBeCloseTo(0.02);
  });

  it('e2e: `chase buy 1000$` with a base="1000" market alongside still chases BTCUSDT', async () => {
    const ex = makeExchange();
    ex.store!.markets = [
      ...ex.store!.markets!,
      { symbol: '1000USDT', base: '1000', quote: 'USDT', precision: { amount: 1, price: 0.0001 } } as never,
    ];
    ex.store!.tickers = [
      ...ex.store!.tickers!,
      { symbol: '1000USDT', last: 0.5 } as never,
    ];
    await cmd.execute(ctx(['chase', 'buy', '1000$'], ex));
    expect(chaserOrderManagerInstance.instances).toHaveLength(1);
    expect(chaserOrderManagerInstance.instances[0].options.symbol).toBe('BTCUSDT');
    expect(chaserOrderManagerInstance.instances[0].options.size).toBeCloseTo(0.02);
  });
});

describe('ChaseCommand — fatfinger', () => {
  let cmd: ChaseCommand;
  beforeEach(() => {
    cmd = new ChaseCommand();
    clearChasers();
  });
  afterEach(() => {
    clearChasers();
  });

  it('rejects when chase size exceeds the limit', async () => {
    const ex = makeExchange();
    const { MemoryPreprocessStore } = await import('../../preprocess/memoryStore');
    const store = new MemoryPreprocessStore();
    // $1000 / 50000 = 0.02. Limit 0.005 → blocked.
    store.setFatfinger('BTCUSDT', 0.005);
    const c = ctx(['chase', 'buy', '$1000'], ex, 'BTCUSDT', store);
    const r = await cmd.execute(c);
    expect(r.success).toBe(false);
    expect(chaserOrderManagerInstance.instances).toHaveLength(0);
    expect(c.adapter.printError).toHaveBeenCalledWith(
      expect.stringMatching(/fatfinger/),
    );
  });

  it('allows when chase size is within the limit', async () => {
    const ex = makeExchange();
    const { MemoryPreprocessStore } = await import('../../preprocess/memoryStore');
    const store = new MemoryPreprocessStore();
    store.setFatfinger('BTCUSDT', 0.05);
    const r = await cmd.execute(ctx(['chase', 'buy', '$1000'], ex, 'BTCUSDT', store));
    expect(r.success).toBe(true);
    expect(chaserOrderManagerInstance.instances).toHaveLength(1);
  });
});
