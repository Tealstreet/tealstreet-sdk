import { describe, it, expect, vi, beforeEach } from 'vitest';
import { TwapCommand } from './TwapCommand';
import type { CommandContext } from '../Command';
import { OrderSide } from '@tealstreet/safe-cex/src/types';
import type { ExchangeLike } from '../../types';
import { MemoryPreprocessStore } from '../../preprocess/memoryStore';

function makeExchange(): ExchangeLike {
  return {
    name: 'test',
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
      positions: [],
      balance: { used: 0, free: 1_000_000, total: 1_000_000, upnl: 0 },
    },
    emitter: { on: vi.fn(), off: vi.fn(), emit: vi.fn() },
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

function chaseInner(result: { taskAction?: { params?: { params?: Record<string, unknown> } } }) {
  return result.taskAction?.params?.params ?? {};
}

describe('TwapCommand — twap chase parsing', () => {
  let cmd: TwapCommand;
  beforeEach(() => {
    cmd = new TwapCommand();
  });

  describe('argument validation', () => {
    it('rejects missing side', async () => {
      const ex = makeExchange();
      const r = await cmd.execute(ctx(['twap', 'chase'], ex));
      expect(r.success).toBe(false);
      expect(r.error).toMatch(/buy.*sell/);
    });

    it('rejects an invalid side', async () => {
      const ex = makeExchange();
      const r = await cmd.execute(ctx(['twap', 'chase', 'long'], ex));
      expect(r.success).toBe(false);
    });
  });

  describe('default policy', () => {
    it('applies timeout 50% market and fail market when no overrides', async () => {
      const ex = makeExchange();
      const r = await cmd.execute(ctx(['twap', 'chase', 'buy', '$1000', 'into', '10', 'over', '60'], ex));
      expect(r.success).toBe(true);
      expect(r.taskAction?.type).toBe('twap');
      const inner = chaseInner(r as never);
      expect(inner.useChaserOrders).toBe(true);
      expect(inner.timeoutPct).toBe(50);
      expect(inner.timeoutAction).toBe('market');
      expect(inner.failAction).toBe('market');
    });

    it('handles a leading instrument', async () => {
      const ex = makeExchange();
      const r = await cmd.execute(ctx(['twap', 'chase', 'buy', '$1000', 'ETHUSDT', 'into', '10', 'over', '60'], ex));
      expect(r.success).toBe(true);
      expect(r.taskAction?.params).toBeDefined();
      const params = r.taskAction?.params as { symbol?: string };
      expect(params.symbol).toBe('ETHUSDT');
    });

    it('threads slice count and duration into params', async () => {
      const ex = makeExchange();
      const r = await cmd.execute(ctx(['twap', 'chase', 'buy', '$1000', 'into', '12', 'over', '90'], ex));
      const inner = chaseInner(r as never);
      expect(inner.slices).toBe(12);
      expect(inner.duration).toBe(90 * 60_000);
    });
  });

  describe('timeout override', () => {
    it.each([
      ['1%', 1],
      ['25%', 25],
      ['50%', 50],
      ['99.5%', 99.5],
      ['100%', 100],
    ])('accepts %s as a valid pct', async (pctStr, expected) => {
      const ex = makeExchange();
      const r = await cmd.execute(
        ctx(['twap', 'chase', 'buy', '$1000', 'into', '10', 'over', '60', 'timeout', pctStr, 'next'], ex),
      );
      expect(r.success).toBe(true);
      const inner = chaseInner(r as never);
      expect(inner.timeoutPct).toBe(expected);
      expect(inner.timeoutAction).toBe('next');
    });

    it.each([
      ['0%', /must be in/],
      ['101%', /must be in/],
      ['50', /expected '<pct%>'/],
      ['half', /expected '<pct%>'/],
    ])("rejects malformed pct '%s'", async (pctStr, pattern) => {
      const ex = makeExchange();
      const r = await cmd.execute(
        ctx(['twap', 'chase', 'buy', '$1000', 'into', '10', 'over', '60', 'timeout', pctStr, 'next'], ex),
      );
      expect(r.success).toBe(false);
      expect(r.error).toMatch(pattern);
    });

    it('rejects a non-market/next action', async () => {
      const ex = makeExchange();
      const r = await cmd.execute(
        ctx(['twap', 'chase', 'buy', '$1000', 'into', '10', 'over', '60', 'timeout', '30%', 'cancel'], ex),
      );
      expect(r.success).toBe(false);
      expect(r.error).toMatch(/market.*next/);
    });
  });

  describe('fail override', () => {
    it.each([['market'], ['next']])("accepts '%s'", async (action) => {
      const ex = makeExchange();
      const r = await cmd.execute(
        ctx(['twap', 'chase', 'buy', '$1000', 'into', '10', 'over', '60', 'fail', action], ex),
      );
      expect(r.success).toBe(true);
      expect(chaseInner(r as never).failAction).toBe(action);
    });

    it('rejects an invalid action', async () => {
      const ex = makeExchange();
      const r = await cmd.execute(
        ctx(['twap', 'chase', 'buy', '$1000', 'into', '10', 'over', '60', 'fail', 'maybe'], ex),
      );
      expect(r.success).toBe(false);
    });
  });

  describe('combined modifiers', () => {
    it('parses timeout AND fail in any order', async () => {
      const ex = makeExchange();
      const r = await cmd.execute(
        ctx(
          ['twap', 'chase', 'buy', '$1000', 'into', '10', 'over', '60', 'fail', 'next', 'timeout', '20%', 'market'],
          ex,
        ),
      );
      expect(r.success).toBe(true);
      const inner = chaseInner(r as never);
      expect(inner.timeoutPct).toBe(20);
      expect(inner.timeoutAction).toBe('market');
      expect(inner.failAction).toBe('next');
    });

    it('parses reduce + timeout + fail', async () => {
      const ex = makeExchange();
      const r = await cmd.execute(
        ctx(
          [
            'twap',
            'chase',
            'sell',
            '50%',
            'into',
            '5',
            'over',
            '30',
            'reduce',
            'timeout',
            '40%',
            'next',
            'fail',
            'next',
          ],
          ex,
        ),
      );
      // 50% with reduce should resolve to position, but the test
      // exchange has no position — it will error. That's OK; the
      // important thing is that arg parsing succeeds before delegating
      // to handleBuySell.
      // We're instead testing the canonical happy path with USD:
      const r2 = await cmd.execute(
        ctx(
          [
            'twap',
            'chase',
            'sell',
            '$500',
            'into',
            '5',
            'over',
            '30',
            'reduce',
            'timeout',
            '40%',
            'next',
            'fail',
            'next',
          ],
          ex,
        ),
      );
      expect(r2.success).toBe(true);
      const inner = chaseInner(r2 as never);
      expect(inner.timeoutPct).toBe(40);
      expect(inner.timeoutAction).toBe('next');
      expect(inner.failAction).toBe('next');
    });
  });

  describe('side threading', () => {
    it('forwards buy through to params', async () => {
      const ex = makeExchange();
      const r = await cmd.execute(ctx(['twap', 'chase', 'buy', '$1000', 'into', '5', 'over', '10'], ex));
      expect(r.success).toBe(true);
      const taskParams = r.taskAction?.params as { side?: OrderSide };
      expect(taskParams.side).toBe(OrderSide.Buy);
    });

    it('forwards sell through to params', async () => {
      const ex = makeExchange();
      const r = await cmd.execute(ctx(['twap', 'chase', 'sell', '$1000', 'into', '5', 'over', '10'], ex));
      expect(r.success).toBe(true);
      const taskParams = r.taskAction?.params as { side?: OrderSide };
      expect(taskParams.side).toBe(OrderSide.Sell);
    });
  });

  describe('plain twap subcommands still work', () => {
    it('twap buy positional', async () => {
      const ex = makeExchange();
      const r = await cmd.execute(ctx(['twap', 'buy', '$1000', 'into', '5', 'over', '10'], ex));
      expect(r.success).toBe(true);
      const inner = chaseInner(r as never);
      expect(inner.timeoutPct).toBeUndefined(); // not set on regular twap
    });

    it('twap list still routes to handleList', async () => {
      const ex = makeExchange();
      const r = await cmd.execute(ctx(['twap', 'list'], ex));
      expect(r.success).toBe(true);
      expect(r.taskAction).toBeUndefined();
    });
  });

  describe('suffix-USD size regression', () => {
    it('`twap buy 1000$ into 5 over 10` (plain twap)', async () => {
      const ex = makeExchange();
      const r = await cmd.execute(ctx(['twap', 'buy', '1000$', 'into', '5', 'over', '10'], ex));
      expect(r.success).toBe(true);
      const params = r.taskAction?.params as { symbol?: string };
      expect(params.symbol).toBe('BTCUSDT');
      const inner = (r.taskAction?.params as { params?: { totalSize?: number } }).params;
      // $1000 / 50000 = 0.02 BTC total
      expect(inner?.totalSize).toBeCloseTo(0.02);
    });

    it('`twap chase buy 1000$ into 5 over 10`', async () => {
      const ex = makeExchange();
      const r = await cmd.execute(ctx(['twap', 'chase', 'buy', '1000$', 'into', '5', 'over', '10'], ex));
      expect(r.success).toBe(true);
      const params = r.taskAction?.params as { symbol?: string };
      expect(params.symbol).toBe('BTCUSDT');
      const inner = (r.taskAction?.params as { params?: { totalSize?: number; useChaserOrders?: boolean } }).params;
      expect(inner?.totalSize).toBeCloseTo(0.02);
      expect(inner?.useChaserOrders).toBe(true);
    });

    it('e2e: `twap buy 1000$ into 5 over 10` with a base="1000" market on the focused symbol', async () => {
      const ex = makeExchange();
      ex.store!.markets = [
        ...(ex.store!.markets ?? []),
        { symbol: '1000USDT', base: '1000', quote: 'USDT', precision: { amount: 1, price: 0.0001 } } as never,
      ];
      ex.store!.tickers = [...(ex.store!.tickers ?? []), { symbol: '1000USDT', last: 0.5 } as never];
      const r = await cmd.execute(ctx(['twap', 'buy', '1000$', 'into', '5', 'over', '10'], ex));
      expect(r.success).toBe(true);
      const params = r.taskAction?.params as { symbol?: string };
      // Should NOT be 1000USDT — '1000$' is a size, not a symbol.
      expect(params.symbol).toBe('BTCUSDT');
      const inner = (r.taskAction?.params as { params?: { totalSize?: number } }).params;
      expect(inner?.totalSize).toBeCloseTo(0.02);
    });

    it('e2e: `twap chase buy 1000$ into 5 over 10` with a base="1000" market', async () => {
      const ex = makeExchange();
      ex.store!.markets = [
        ...(ex.store!.markets ?? []),
        { symbol: '1000USDT', base: '1000', quote: 'USDT', precision: { amount: 1, price: 0.0001 } } as never,
      ];
      ex.store!.tickers = [...(ex.store!.tickers ?? []), { symbol: '1000USDT', last: 0.5 } as never];
      const r = await cmd.execute(ctx(['twap', 'chase', 'buy', '1000$', 'into', '5', 'over', '10'], ex));
      expect(r.success).toBe(true);
      const params = r.taskAction?.params as { symbol?: string };
      expect(params.symbol).toBe('BTCUSDT');
    });
  });

  describe('fatfinger', () => {
    it('rejects when per-slice size exceeds the limit', async () => {
      const ex = makeExchange();
      const store = new MemoryPreprocessStore();
      // total = $5000 / 50000 = 0.1, slices = 5 → per-slice 0.02. Limit 0.005 → blocked.
      store.setFatfinger('BTCUSDT', 0.005);
      const cmd = new TwapCommand();
      const r = await cmd.execute(ctx(['twap', 'buy', '$5000', 'into', '5', 'over', '10'], ex, 'BTCUSDT', store));
      expect(r.success).toBe(false);
      expect(r.error).toMatch(/fatfinger/);
    });

    it('allows when per-slice size is within the limit', async () => {
      const ex = makeExchange();
      const store = new MemoryPreprocessStore();
      store.setFatfinger('BTCUSDT', 0.025);
      const cmd = new TwapCommand();
      const r = await cmd.execute(ctx(['twap', 'buy', '$5000', 'into', '5', 'over', '10'], ex, 'BTCUSDT', store));
      expect(r.success).toBe(true);
    });
  });

  // TSBUG-104a: the `reduce` keyword must be threaded into the emitted
  // task params — TwapInstance reads options.reduceOnly per slice. A
  // dropped flag turned close-only TWAPs into position-opening ones.
  describe('reduceOnly emission', () => {
    it('emits reduceOnly:true when `reduce` is present (plain twap)', async () => {
      const ex = makeExchange();
      const r = await cmd.execute(ctx(['twap', 'sell', '$500', 'into', '5', 'over', '30', 'reduce'], ex));
      expect(r.success).toBe(true);
      expect(chaseInner(r as never).reduceOnly).toBe(true);
    });

    it('emits reduceOnly:false when `reduce` is absent', async () => {
      const ex = makeExchange();
      const r = await cmd.execute(ctx(['twap', 'buy', '$500', 'into', '5', 'over', '30'], ex));
      expect(r.success).toBe(true);
      expect(chaseInner(r as never).reduceOnly).toBe(false);
    });

    it('emits reduceOnly:true through `twap chase ... reduce`', async () => {
      const ex = makeExchange();
      const r = await cmd.execute(ctx(['twap', 'chase', 'sell', '$500', 'into', '5', 'over', '30', 'reduce'], ex));
      expect(r.success).toBe(true);
      expect(chaseInner(r as never).reduceOnly).toBe(true);
    });
  });
});
