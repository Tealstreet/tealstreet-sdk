import { describe, it, expect, vi, beforeEach } from 'vitest';
import { SwarmCommand } from './SwarmCommand';
import type { CommandContext } from '../Command';
import { MemoryPreprocessStore } from '../../preprocess/memoryStore';
import {
  OrderSide,
  OrderType,
  PositionSide,
  MarginType,
  PositionMode,
  type Position,
  type Balance,
} from '@tealstreet/safe-cex/src/types';
import type { ExchangeLike } from '../../types';

function pos(p: Partial<Position> = {}): Position {
  return {
    id: p.id ?? 'pos1',
    symbol: p.symbol ?? 'BTCUSDT',
    side: p.side ?? PositionSide.Long,
    entryPrice: p.entryPrice ?? 50_000,
    notional: p.notional ?? 50_000,
    leverage: p.leverage ?? 1,
    marginType: p.marginType ?? MarginType.Isolated,
    positionMode: p.positionMode ?? PositionMode.OneWay,
    unrealizedPnl: p.unrealizedPnl ?? 0,
    contracts: p.contracts ?? 1,
    liquidationPrice: p.liquidationPrice ?? 0,
  } as Position;
}

function bal(p: Partial<Balance> = {}): Balance {
  return {
    used: p.used ?? 0,
    free: p.free ?? 1000,
    total: p.total ?? 1000,
    upnl: p.upnl ?? 0,
  } as Balance;
}

function makeExchange(opts: { positions?: Position[]; balance?: Balance } = {}): ExchangeLike {
  return {
    // A REAL venue id, not 'test': every write now goes through the shared
    // order policy, which asks `exchangeMetadata` about round-down, infinity
    // stops and the reduce-only clamp. A made-up name answers "unknown venue"
    // and no longer exercises the branches these tests are about.
    name: 'binance',
    store: {
      markets: [
        { symbol: 'BTCUSDT', base: 'BTC', quote: 'USDT', precision: { amount: 0.001, price: 0.01 } },
        { symbol: 'ETHUSDT', base: 'ETH', quote: 'USDT', precision: { amount: 0.01, price: 0.01 } },
      ],
      tickers: [
        { symbol: 'BTCUSDT', last: 50000 },
        { symbol: 'ETHUSDT', last: 3000 },
      ],
      orders: [],
      positions: opts.positions ?? [],
      balance: opts.balance ?? bal(),
    },
    emitter: { on: vi.fn(), off: vi.fn(), emit: vi.fn() },
    placeOrder: vi.fn().mockResolvedValue(['order-1']),
  } as unknown as ExchangeLike;
}

function ctx(
  args: string[],
  exchange: ExchangeLike,
  symbol = 'BTCUSDT',
  preprocessStore?: MemoryPreprocessStore,
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

function placedOrders(ex: ExchangeLike) {
  return (ex.placeOrder as ReturnType<typeof vi.fn>).mock.calls.map((c) => c[0]);
}

class DeterministicSwarm extends SwarmCommand {
  // Cycle through fixed weights so size distribution is testable.
  private weights: number[];
  private idx = 0;
  constructor(weights: number[] = [1.5, 0.5, 1.0, 0.7, 1.3]) {
    super();
    this.weights = weights;
  }
  protected randomMultiplier(): number {
    const w = this.weights[this.idx % this.weights.length];
    this.idx++;
    return w;
  }
}

describe('SwarmCommand — argument parsing', () => {
  let cmd: SwarmCommand;
  beforeEach(() => {
    cmd = new SwarmCommand();
  });

  it('rejects missing side', async () => {
    const ex = makeExchange();
    const r = await cmd.execute(ctx(['swarm'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/buy.*sell/);
  });

  it("rejects a side that isn't buy/sell", async () => {
    const ex = makeExchange();
    const r = await cmd.execute(ctx(['swarm', 'long', '$100', 'into', '5'], ex));
    expect(r.success).toBe(false);
  });

  it('rejects missing size', async () => {
    const ex = makeExchange();
    const r = await cmd.execute(ctx(['swarm', 'buy'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/size/);
  });

  it("requires the literal 'into <count>' keyword pair", async () => {
    const ex = makeExchange();
    const r = await cmd.execute(ctx(['swarm', 'buy', '$100', 'across', '5'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/into/);
  });

  it('rejects non-positive count', async () => {
    const ex = makeExchange();
    const r = await cmd.execute(ctx(['swarm', 'buy', '$100', 'into', '0'], ex));
    expect(r.success).toBe(false);
  });

  it('rejects bare-number sizes per Phase 0 decision 1', async () => {
    const ex = makeExchange();
    const r = await cmd.execute(ctx(['swarm', 'buy', '100', 'into', '5'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/Invalid size/);
  });

  it('rejects unknown trailing tokens', async () => {
    const ex = makeExchange();
    const r = await cmd.execute(ctx(['swarm', 'buy', '$100', 'into', '5', 'frizzle'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/unexpected token/);
  });
});

describe('SwarmCommand — slice distribution', () => {
  let cmd: SwarmCommand;
  beforeEach(() => {
    cmd = new SwarmCommand();
  });

  it('default mode splits evenly across N orders', async () => {
    const ex = makeExchange();
    // $1000 / 50000 = 0.02 BTC total, /5 = 0.004 each
    await cmd.execute(ctx(['swarm', 'buy', '$1000', 'into', '5'], ex));
    const orders = placedOrders(ex);
    expect(orders).toHaveLength(5);
    expect(orders.every((o) => o.type === OrderType.Market)).toBe(true);
    expect(orders.every((o) => o.side === OrderSide.Buy)).toBe(true);
    expect(orders.every((o) => o.amount === 0.004)).toBe(true);
  });

  it('honors reduceOnly when `reduce` keyword is present', async () => {
    const ex = makeExchange({
      positions: [pos({ side: PositionSide.Long, contracts: 1 })],
    });
    await cmd.execute(ctx(['swarm', 'sell', '50%', 'into', '5', 'reduce'], ex));
    const orders = placedOrders(ex);
    expect(orders).toHaveLength(5);
    expect(orders.every((o) => o.reduceOnly === true)).toBe(true);
    // 50% of 1 contract = 0.5, /5 = 0.1 each
    expect(orders.every((o) => o.amount === 0.1)).toBe(true);
  });

  it('uses balance-scoped % for non-reduce entries', async () => {
    const ex = makeExchange({ balance: bal({ free: 100_000 }) });
    // 10% of 100000 USD = 10000 → /50000 = 0.2 BTC → /5 slices = 0.04 each
    await cmd.execute(ctx(['swarm', 'buy', '10%', 'into', '5'], ex));
    const orders = placedOrders(ex);
    expect(orders.every((o) => o.amount === 0.04)).toBe(true);
  });

  it('errors when total size is too small to split — no slice rounded to zero', async () => {
    // $100 / 50000 = 0.002 BTC, /5 slices = 0.0004 → rounds to 0 at 0.001 precision
    const ex = makeExchange();
    const r = await cmd.execute(ctx(['swarm', 'buy', '$100', 'into', '5'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/too small/);
  });

  it('irregular mode varies sizes around the mean', async () => {
    const cmd2 = new DeterministicSwarm([1.5, 0.5, 1.0, 0.7, 1.3]);
    const ex = makeExchange();
    // $1000 / 50000 = 0.02 BTC total. Weights normalize to mean 1.0
    // (sum 5.0). Each = (w / 5) * 0.02 → adjusted to 0.001 precision.
    await cmd2.execute(ctx(['swarm', 'buy', '$1000', 'into', '5', 'irregular'], ex));
    const orders = placedOrders(ex);
    expect(orders).toHaveLength(5);
    const amounts = orders.map((o) => o.amount);
    // Sizes are not all equal
    expect(new Set(amounts).size).toBeGreaterThan(1);
    // Sum lands on the exact total (0.02) — last slice absorbs rounding
    const sum = amounts.reduce((a, b) => a + b, 0);
    expect(sum).toBeCloseTo(0.02, 5);
  });

  it('handles trailing instrument override', async () => {
    const ex = makeExchange();
    await cmd.execute(ctx(['swarm', 'sell', '$300', 'into', '3', 'ETHUSDT'], ex));
    const orders = placedOrders(ex);
    expect(orders.every((o) => o.symbol === 'ETHUSDT')).toBe(true);
    // $300 / 3000 = 0.1 ETH total, /3 = 0.0333... → 0.03 at 0.01 precision
    // Last slice absorbs remainder.
    const sum = orders.map((o) => o.amount).reduce((a, b) => a + b, 0);
    expect(sum).toBeCloseTo(0.1, 5);
  });

  it('handles leading instrument', async () => {
    const ex = makeExchange();
    await cmd.execute(ctx(['swarm', 'buy', 'ETHUSDT', '$300', 'into', '3'], ex));
    const orders = placedOrders(ex);
    expect(orders.every((o) => o.symbol === 'ETHUSDT')).toBe(true);
    expect(orders).toHaveLength(3);
  });
});

describe('SwarmCommand — failure modes', () => {
  let cmd: SwarmCommand;
  beforeEach(() => {
    cmd = new SwarmCommand();
  });

  it('reports partial when some orders fail', async () => {
    const ex = makeExchange();
    // First call rejects, others succeed
    let n = 0;
    (ex.placeOrder as ReturnType<typeof vi.fn>).mockImplementation(async () => {
      n++;
      if (n === 1) throw new Error('boom');
      return ['ok'];
    });
    const r = await cmd.execute(ctx(['swarm', 'buy', '$1000', 'into', '5'], ex));
    expect(r.success).toBe(true); // partial is still success() per implementation
  });

  it('reports failure when ALL orders fail', async () => {
    const ex = makeExchange();
    (ex.placeOrder as ReturnType<typeof vi.fn>).mockRejectedValue(new Error('boom'));
    const r = await cmd.execute(ctx(['swarm', 'buy', '$1000', 'into', '5'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/all orders failed/i);
  });

  it('errors when not connected', async () => {
    const r = await cmd.execute(ctx(['swarm', 'buy', '$100', 'into', '5'], null as unknown as ExchangeLike));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/Not connected/);
  });

  it('errors when exchange does not support placeOrder', async () => {
    const ex = makeExchange();
    delete (ex as unknown as { placeOrder?: unknown }).placeOrder;
    const r = await cmd.execute(ctx(['swarm', 'buy', '$100', 'into', '5'], ex));
    expect(r.success).toBe(false);
  });
});

describe('SwarmCommand — suffix-USD size regression', () => {
  let cmd: SwarmCommand;
  beforeEach(() => {
    cmd = new SwarmCommand();
  });

  it('e2e: `swarm buy 1000$ into 5` fans out across 5 slices', async () => {
    const ex = makeExchange();
    await cmd.execute(ctx(['swarm', 'buy', '1000$', 'into', '5'], ex));
    expect(placedOrders(ex)).toHaveLength(5);
    expect(placedOrders(ex).every((o) => o.amount === 0.004)).toBe(true);
  });

  it('e2e: `swarm buy 1000$ into 5` with a base="1000" market still fans out on focused symbol', async () => {
    const ex = makeExchange();
    ex.store!.markets = [
      ...(ex.store!.markets ?? []),
      { symbol: '1000USDT', base: '1000', quote: 'USDT', precision: { amount: 1, price: 0.0001 } } as never,
    ];
    ex.store!.tickers = [
      ...(ex.store!.tickers ?? []),
      { symbol: '1000USDT', last: 0.5 } as never,
    ];
    await cmd.execute(ctx(['swarm', 'buy', '1000$', 'into', '5'], ex));
    const orders = placedOrders(ex);
    expect(orders).toHaveLength(5);
    expect(orders.every((o) => o.symbol === 'BTCUSDT')).toBe(true);
    expect(orders.every((o) => o.amount === 0.004)).toBe(true);
  });

  it('e2e: `swarm sell 1000$ into 5` works symmetrically', async () => {
    const ex = makeExchange();
    await cmd.execute(ctx(['swarm', 'sell', '1000$', 'into', '5'], ex));
    expect(placedOrders(ex).every((o) => o.amount === 0.004)).toBe(true);
  });

  describe('fatfinger', () => {
    it('rejects when per-slice size exceeds the limit', async () => {
      const ex = makeExchange();
      const store = new MemoryPreprocessStore();
      // Each slice = $1000 / 50000 / 5 = 0.004 BTC. Limit 0.001 → blocked.
      store.setFatfinger('BTCUSDT', 0.001);
      const r = await cmd.execute(
        ctx(['swarm', 'buy', '1000$', 'into', '5'], ex, 'BTCUSDT', store),
      );
      expect(r.success).toBe(false);
      expect(r.error).toMatch(/fatfinger/);
      expect(ex.placeOrder).not.toHaveBeenCalled();
    });

    it('allows when per-slice size is within the limit', async () => {
      const ex = makeExchange();
      const store = new MemoryPreprocessStore();
      store.setFatfinger('BTCUSDT', 0.005);
      const r = await cmd.execute(
        ctx(['swarm', 'buy', '1000$', 'into', '5'], ex, 'BTCUSDT', store),
      );
      expect(r.success).toBe(true);
    });
  });

  describe('result.data.orderIds — capture support', () => {
    it('returns IDs from every successful slice', async () => {
      const ex = makeExchange();
      const placeOrder = ex.placeOrder as ReturnType<typeof vi.fn>;
      placeOrder
        .mockResolvedValueOnce(['s1'])
        .mockResolvedValueOnce(['s2'])
        .mockResolvedValueOnce(['s3'])
        .mockResolvedValueOnce(['s4'])
        .mockResolvedValueOnce(['s5']);
      const r = await cmd.execute(ctx(['swarm', 'buy', '1000$', 'into', '5'], ex));
      expect(r.success).toBe(true);
      expect(r.data?.orderIds).toEqual(['s1', 's2', 's3', 's4', 's5']);
    });

    it('partial failures contribute only successful slice IDs', async () => {
      // 5 slices: 3 succeed, 2 reject. The capture only carries the
      // IDs that actually landed — honest about what's cancelable.
      const ex = makeExchange();
      const placeOrder = ex.placeOrder as ReturnType<typeof vi.fn>;
      placeOrder
        .mockResolvedValueOnce(['s1'])
        .mockRejectedValueOnce(new Error('slice 2 failed'))
        .mockResolvedValueOnce(['s3'])
        .mockRejectedValueOnce(new Error('slice 4 failed'))
        .mockResolvedValueOnce(['s5']);
      const r = await cmd.execute(ctx(['swarm', 'buy', '1000$', 'into', '5'], ex));
      // Partial warning fires but the command still reports success
      // — same shape as before this commit.
      expect(r.success).toBe(true);
      expect(r.data?.orderIds).toEqual(['s1', 's3', 's5']);
    });

    it('all-failure returns error (no data binding)', async () => {
      const ex = makeExchange();
      (ex.placeOrder as ReturnType<typeof vi.fn>).mockRejectedValue(new Error('boom'));
      const r = await cmd.execute(ctx(['swarm', 'buy', '1000$', 'into', '5'], ex));
      expect(r.success).toBe(false);
      // Error path doesn't construct data — runner sees data=undefined
      // and writes empty string + warning if capture was requested.
      expect(r.data).toBeUndefined();
    });
  });
});

// TSBUG-106: aggregate budget gates exposure-increasing swarms.
describe('SwarmCommand — budget', () => {
  it('blocks a swarm that exceeds the cap (and places nothing)', async () => {
    const ex = makeExchange();
    const store = new MemoryPreprocessStore();
    store.setBudget(500); // $1000 swarm > $500 cap
    const cmd = new SwarmCommand();
    const r = await cmd.execute(ctx(['swarm', 'buy', '$1000', 'into', '5'], ex, 'BTCUSDT', store));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/budget/i);
    expect(ex.placeOrder).not.toHaveBeenCalled();
  });

  it('charges the placed slices when within the cap', async () => {
    const ex = makeExchange();
    const store = new MemoryPreprocessStore();
    store.setBudget(2000);
    const cmd = new SwarmCommand();
    const r = await cmd.execute(ctx(['swarm', 'buy', '$1000', 'into', '5'], ex, 'BTCUSDT', store));
    expect(r.success).toBe(true);
    expect(store.getBudgetSpent()).toBeGreaterThan(0);
    expect(store.getBudgetSpent()).toBeLessThanOrEqual(1000);
  });

  it('does not count a reduce-only swarm against the budget', async () => {
    const ex = makeExchange({ positions: [{ symbol: 'BTCUSDT', contracts: 1 } as unknown as Position] });
    const store = new MemoryPreprocessStore();
    store.setBudget(500);
    const cmd = new SwarmCommand();
    const r = await cmd.execute(ctx(['swarm', 'sell', '$1000', 'into', '5', 'reduce'], ex, 'BTCUSDT', store));
    expect(r.success).toBe(true);
    expect(store.getBudgetSpent()).toBe(0);
  });
});
