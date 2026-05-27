/**
 * Tests for `TasksCommand` (the unified-task aggregator) and
 * `KillCommand` (most-recent-active / by-id / kill-all). Both touch
 * the singletons `chaserOrderManagerInstance` and `twapManager`, so
 * the harness drains both before each test.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { TasksCommand, listTasks } from './TasksCommand';
import { KillCommand } from './KillCommand';
import { ChaserOrderInstance } from '@tealstreet/safe-cex/src/webworker/chaserOrder/ChaserOrderInstance';
import { chaserOrderManagerInstance } from '@tealstreet/safe-cex/src/webworker/chaserOrder/ChaserOrderManager';
import { twapManager } from '@tealstreet/safe-cex/src/webworker/twap/TwapManager';
import { TwapInstance } from '@tealstreet/safe-cex/src/webworker/twap/TwapInstance';
import { TwapStatus } from '@tealstreet/safe-cex/src/webworker/twap/TwapStatus';
import { OrderSide, type Market } from '@tealstreet/safe-cex/src/types';
import type { Exchange } from '@tealstreet/safe-cex/src/exchanges/base';
import type { CommandContext } from '../Command';
import type { CLIPlatformAdapter } from '../../types';

const MARKET: Market = {
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.01 },
  limits: { amount: { min: 0.001, max: 1000 }, leverage: { min: 1, max: 100 } },
} as Market;

const ETH_MARKET: Market = {
  ...MARKET,
  id: 'ETHUSDT',
  symbol: 'ETHUSDT',
  base: 'ETH',
} as Market;

function makeExchange(): Exchange {
  const tickers = [
    { symbol: 'BTCUSDT', last: 50000, bid: 49995, ask: 50005 },
    { symbol: 'ETHUSDT', last: 3000, bid: 2999, ask: 3001 },
  ];
  return {
    name: 'mock',
    store: {
      tickers,
      tickersBySymbol: Object.fromEntries(tickers.map((t) => [t.symbol, t])),
      orders: [],
      positions: [],
      markets: [MARKET, ETH_MARKET],
      marketsBySymbol: { BTCUSDT: MARKET, ETHUSDT: ETH_MARKET },
      updateIds: { orders: 0, positions: 0, tickers: 0, balance: 0 },
    },
    emitter: { on: vi.fn(), off: vi.fn(), emit: vi.fn() },
    on: vi.fn(),
    off: vi.fn(),
    placeOrder: vi.fn().mockResolvedValue(['order-1']),
    cancelOrders: vi.fn().mockResolvedValue(undefined),
  } as unknown as Exchange;
}

function makeAdapter(): CLIPlatformAdapter {
  return {
    print: vi.fn(),
    printInfo: vi.fn(),
    printError: vi.fn(),
    printSuccess: vi.fn(),
    printWarning: vi.fn(),
    printRaw: vi.fn(),
    listAccounts: vi.fn().mockReturnValue([]),
    getAccount: vi.fn(),
    addAccount: vi.fn(),
    updateAccount: vi.fn(),
    removeAccount: vi.fn(),
    setDefaultAccount: vi.fn(),
    getDefaultSymbol: vi.fn(),
    setDefaultSymbol: vi.fn(),
    getActiveAccounts: vi.fn().mockReturnValue([]),
    listExchanges: vi.fn().mockReturnValue([]),
    createExchange: vi.fn(),
    startExchange: vi.fn(),
    stopExchange: vi.fn(),
    startCapture: vi.fn(),
  } as unknown as CLIPlatformAdapter;
}

function ctx(args: string[], adapter: CLIPlatformAdapter): CommandContext {
  return {
    raw: args.join(' '),
    args: { _: args },
    symbol: 'BTCUSDT',
    accountName: 'a',
    exchange: makeExchange() as never,
    adapter,
  } as unknown as CommandContext;
}

/** Add a chaser via the manager — picks up the manager's processInstance
 *  side effects, but for these tests we only care about the registry
 *  bookkeeping (instances array) and the instance state. We immediately
 *  cancel its order placement promise so it doesn't actually try to
 *  trade. */
function spawnChaser(symbol = 'BTCUSDT', side: OrderSide = OrderSide.Buy): ChaserOrderInstance {
  const exchange = makeExchange();
  const inst = new ChaserOrderInstance(exchange, {
    symbol,
    size: 0.1,
    side,
    maxAttempts: 1000,
    delay: 1000,
  });
  chaserOrderManagerInstance.add(inst);
  return inst;
}

function spawnTwap(symbol = 'BTCUSDT', side: OrderSide = OrderSide.Buy): TwapInstance {
  const exchange = makeExchange();
  const worker = { postMessage: vi.fn() } as unknown as Worker;
  const inst = new TwapInstance(exchange, worker, {
    accountName: 'a',
    activeMarket: { ...MARKET, symbol } as Market,
    size: 1.0,
    duration: 10,
    ordersCount: 5,
    side,
    chaserDelay: 1000,
    reduceOnly: false,
    randomnessPercentage: 0,
    useLimitOrders: false,
    useChaserOrders: false,
  }, vi.fn());
  twapManager.runningTwaps.set(inst.id, inst);
  return inst;
}

function drainSingletons() {
  while (chaserOrderManagerInstance.instances.length > 0) {
    const inst = chaserOrderManagerInstance.instances[0];
    try {
      chaserOrderManagerInstance.removeKeepOrder(inst.id);
    } catch {
      /* fall through */
    }
    if (chaserOrderManagerInstance.instances.includes(inst)) {
      chaserOrderManagerInstance.instances.splice(
        chaserOrderManagerInstance.instances.indexOf(inst),
        1,
      );
    }
  }
  twapManager.runningTwaps.clear();
}

// ─── listTasks (the aggregator) ──────────────────────────────────────

describe('listTasks', () => {
  beforeEach(drainSingletons);

  it('returns [] when nothing is registered', () => {
    expect(listTasks()).toEqual([]);
  });

  it('returns chasers in registration order, with active=true while live', () => {
    const c1 = spawnChaser();
    const c2 = spawnChaser('ETHUSDT', OrderSide.Sell);
    const rows = listTasks();
    expect(rows.map((r) => r.id)).toEqual([c1.id, c2.id]);
    expect(rows.map((r) => r.type)).toEqual(['chase', 'chase']);
    expect(rows.map((r) => r.side)).toEqual(['BUY', 'SELL']);
    expect(rows.map((r) => r.symbol)).toEqual(['BTCUSDT', 'ETHUSDT']);
    expect(rows.every((r) => r.active)).toBe(true);
  });

  it('returns TWAPs after chasers (insertion-order convention)', () => {
    const c = spawnChaser();
    const t = spawnTwap();
    const rows = listTasks();
    expect(rows.map((r) => r.id)).toEqual([c.id, t.id]);
    expect(rows.map((r) => r.type)).toEqual(['chase', 'twap']);
  });

  it('marks completed tasks active=false', () => {
    const c = spawnChaser();
    c.markComplete('done');
    const rows = listTasks();
    expect(rows[0].active).toBe(false);
    expect(rows[0].status).toBe('done');
  });

  it('TWAP status maps onto the canonical strings', () => {
    const t = spawnTwap();
    // Default TwapInstance state.status is 'idle' — everything other
    // than Running / Paused / Completed falls through to String(...).
    expect(listTasks()[0].status).toBe(String(t.state.status));
  });

  it('TWAP progress reads ordersExecuted / ordersCount', () => {
    const t = spawnTwap();
    // Default ordersExecuted = 0, ordersCount = 5.
    expect(listTasks()[0].progress).toBe('0/5 slices (0.0%)');
    // Mutate state directly (test scaffolding, not the public API).
    t.state.ordersExecuted = 2;
    expect(listTasks()[0].progress).toBe('2/5 slices (40.0%)');
  });
});

// ─── TasksCommand (the print path) ───────────────────────────────────

describe('TasksCommand', () => {
  let cmd: TasksCommand;
  beforeEach(() => {
    drainSingletons();
    cmd = new TasksCommand();
  });

  it('prints "(no active tasks)" when the registry is empty', async () => {
    const adapter = makeAdapter();
    await cmd.execute(ctx(['tasks'], adapter));
    expect(adapter.printInfo).toHaveBeenCalledWith('(no active tasks)');
  });

  it('prints one row per task plus a summary footer', async () => {
    spawnChaser();
    spawnChaser('ETHUSDT', OrderSide.Sell);
    const adapter = makeAdapter();
    await cmd.execute(ctx(['tasks'], adapter));
    const lines = (adapter.print as ReturnType<typeof vi.fn>).mock.calls[0][0] as string[];
    // 2 task rows + 1 summary line
    expect(lines).toHaveLength(3);
    expect(lines[0]).toMatch(/chase/);
    expect(lines[1]).toMatch(/chase/);
    expect(lines[2]).toMatch(/active: 2.*completed: 0/);
  });

  it('formats short IDs to 8 chars', async () => {
    const c = spawnChaser();
    const adapter = makeAdapter();
    await cmd.execute(ctx(['tasks'], adapter));
    const lines = (adapter.print as ReturnType<typeof vi.fn>).mock.calls[0][0] as string[];
    expect(lines[0]).toMatch(new RegExp(`\\[${c.id.slice(0, 8)}\\]`));
  });

  it('formats stopped tasks with brackets around the status', async () => {
    const c = spawnChaser();
    c.markComplete('user');
    const adapter = makeAdapter();
    await cmd.execute(ctx(['tasks'], adapter));
    const lines = (adapter.print as ReturnType<typeof vi.fn>).mock.calls[0][0] as string[];
    expect(lines[0]).toMatch(/\[user\]$/);
  });

  it('summary counts active vs completed correctly', async () => {
    const c1 = spawnChaser();
    spawnChaser();
    c1.markComplete('done');
    const adapter = makeAdapter();
    await cmd.execute(ctx(['tasks'], adapter));
    const lines = (adapter.print as ReturnType<typeof vi.fn>).mock.calls[0][0] as string[];
    expect(lines[lines.length - 1]).toMatch(/active: 1.*completed: 1/);
  });
});

// ─── KillCommand ─────────────────────────────────────────────────────

describe('KillCommand', () => {
  let cmd: KillCommand;
  beforeEach(() => {
    drainSingletons();
    cmd = new KillCommand();
  });

  describe('no args — kill the most-recent active task', () => {
    it("errors when there's nothing to kill", async () => {
      const r = await cmd.execute(ctx(['kill'], makeAdapter()));
      expect(r.success).toBe(false);
      expect(r.error).toMatch(/No active tasks/);
    });

    it('targets the most-recent active row in listTasks() order', async () => {
      const c1 = spawnChaser();
      const c2 = spawnChaser();
      const adapter = makeAdapter();
      await cmd.execute(ctx(['kill'], adapter));
      // c2 is the most-recent → it's the one removed.
      expect(chaserOrderManagerInstance.instances.find((i) => i.id === c1.id)).toBeDefined();
      expect(chaserOrderManagerInstance.instances.find((i) => i.id === c2.id)).toBeUndefined();
      expect(adapter.printSuccess).toHaveBeenCalledWith(
        expect.stringMatching(new RegExp(`Killed chase \\[${c2.id.slice(0, 8)}\\]`)),
      );
    });

    it('skips already-stopped tasks when finding most-recent', async () => {
      const c1 = spawnChaser();
      const c2 = spawnChaser();
      c2.markComplete('done'); // c2 is already stopped → kill targets c1
      await cmd.execute(ctx(['kill'], makeAdapter()));
      expect(chaserOrderManagerInstance.instances.find((i) => i.id === c1.id)).toBeUndefined();
    });
  });

  describe('id-prefix targeting', () => {
    it('matches an active task by ID prefix', async () => {
      const c = spawnChaser();
      const prefix = c.id.slice(0, 6);
      const adapter = makeAdapter();
      await cmd.execute(ctx(['kill', prefix], adapter));
      expect(chaserOrderManagerInstance.instances.find((i) => i.id === c.id)).toBeUndefined();
      expect(adapter.printSuccess).toHaveBeenCalledWith(
        expect.stringMatching(/Killed chase/),
      );
    });

    it('matches the full ID', async () => {
      const c = spawnChaser();
      await cmd.execute(ctx(['kill', c.id], makeAdapter()));
      expect(chaserOrderManagerInstance.instances.find((i) => i.id === c.id)).toBeUndefined();
    });

    it('errors when no active task matches', async () => {
      spawnChaser();
      const r = await cmd.execute(ctx(['kill', 'nonexistent'], makeAdapter()));
      expect(r.success).toBe(false);
      expect(r.error).toMatch(/No active task matches/);
    });

    it('skips already-stopped tasks during prefix match', async () => {
      const c = spawnChaser();
      c.markComplete('done');
      const r = await cmd.execute(ctx(['kill', c.id.slice(0, 6)], makeAdapter()));
      expect(r.success).toBe(false);
      // Stopped tasks don't count as active matches.
    });
  });

  describe('"kill all"', () => {
    it("returns info when there's nothing active", async () => {
      const adapter = makeAdapter();
      const r = await cmd.execute(ctx(['kill', 'all'], adapter));
      expect(r.success).toBe(true);
      expect(adapter.printInfo).toHaveBeenCalledWith('(no active tasks to kill)');
    });

    it('kills every active chaser', async () => {
      spawnChaser();
      spawnChaser();
      spawnChaser();
      await cmd.execute(ctx(['kill', 'all'], makeAdapter()));
      expect(chaserOrderManagerInstance.instances).toHaveLength(0);
    });

    it('does not double-count stopped tasks', async () => {
      const c1 = spawnChaser();
      spawnChaser();
      c1.markComplete('done');
      const adapter = makeAdapter();
      await cmd.execute(ctx(['kill', 'all'], adapter));
      expect(adapter.printSuccess).toHaveBeenCalledWith('Killed 1 task(s)');
    });
  });

  describe('"kill last" / aliases — fuck / terminate', () => {
    it('"kill last" behaves like "kill" (most-recent active task)', async () => {
      const c1 = spawnChaser();
      const c2 = spawnChaser();
      await cmd.execute(ctx(['kill', 'last'], makeAdapter()));
      // c2 is the most-recent active row → it's the one removed.
      expect(chaserOrderManagerInstance.instances.find((i) => i.id === c1.id)).toBeDefined();
      expect(chaserOrderManagerInstance.instances.find((i) => i.id === c2.id)).toBeUndefined();
    });

    it('"kill LAST" is case-insensitive', async () => {
      spawnChaser();
      const c2 = spawnChaser();
      await cmd.execute(ctx(['kill', 'LAST'], makeAdapter()));
      expect(chaserOrderManagerInstance.instances.find((i) => i.id === c2.id)).toBeUndefined();
    });

    it('exposes "fuck" and "terminate" as aliases', () => {
      expect(cmd.aliases).toEqual(expect.arrayContaining(['fuck', 'terminate']));
    });
  });
});
