import { describe, it, expect } from 'vitest';
import {
  PositionSide,
  MarginType,
  PositionMode,
} from '@tealstreet/safe-cex/src/types';
import type { Position } from '@tealstreet/safe-cex/src/types';
import { evaluateLiveVarSource, validateLiveVarSource } from './reactiveVars';
import { MemoryPreprocessStore } from './memoryStore';
import type { PreprocessContext } from './types';
import type { ExchangeLike } from '../types';

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

function makeCtx(opts: {
  positions?: Position[];
  tickers?: Array<{
    symbol: string;
    last?: number;
    mark?: number;
    index?: number;
    bid?: number;
    ask?: number;
    fundingRate?: number;
  }>;
  balance?: { free?: number; total?: number; used?: number; upnl?: number };
  symbol?: string;
} = {}): PreprocessContext {
  const store = new MemoryPreprocessStore();
  const balance = opts.balance ?? {};
  const exchange = {
    store: {
      positions: opts.positions ?? [],
      orders: [],
      markets: [],
      tickers: opts.tickers ?? [],
      balance: {
        free: balance.free ?? 1_000,
        total: balance.total ?? 10_000,
        used: balance.used ?? 0,
        upnl: balance.upnl ?? 0,
      },
    },
  } as unknown as ExchangeLike;
  return { exchange, symbol: opts.symbol ?? 'BTCUSDT', store };
}

describe('evaluateLiveVarSource', () => {
  describe('single $constant source', () => {
    it('resolves $last from focused-symbol ticker', () => {
      const ctx = makeCtx({
        tickers: [{ symbol: 'BTCUSDT', last: 50_000, bid: 49_999, ask: 50_001 }],
      });
      expect(evaluateLiveVarSource('x', '$last', ctx)).toBe(50_000);
    });

    it('resolves $bid', () => {
      const ctx = makeCtx({
        tickers: [{ symbol: 'BTCUSDT', bid: 49_999, ask: 50_001, last: 50_000 }],
      });
      expect(evaluateLiveVarSource('x', '$bid', ctx)).toBe(49_999);
    });

    it('resolves $ask', () => {
      const ctx = makeCtx({
        tickers: [{ symbol: 'BTCUSDT', bid: 49_999, ask: 50_001, last: 50_000 }],
      });
      expect(evaluateLiveVarSource('x', '$ask', ctx)).toBe(50_001);
    });

    it('resolves $mark', () => {
      const ctx = makeCtx({ tickers: [{ symbol: 'BTCUSDT', mark: 49_950 }] });
      expect(evaluateLiveVarSource('x', '$mark', ctx)).toBe(49_950);
    });

    it('resolves $index', () => {
      const ctx = makeCtx({ tickers: [{ symbol: 'BTCUSDT', index: 49_900 }] });
      expect(evaluateLiveVarSource('x', '$index', ctx)).toBe(49_900);
    });

    it('resolves $entry from focused position', () => {
      const ctx = makeCtx({ positions: [pos({ entryPrice: 55_000, contracts: 1 })] });
      expect(evaluateLiveVarSource('x', '$entry', ctx)).toBe(55_000);
    });

    it('resolves $pnl as sum across positions on focused symbol', () => {
      const ctx = makeCtx({
        positions: [
          pos({ id: 'a', side: PositionSide.Long, unrealizedPnl: 75 }),
          pos({ id: 'b', side: PositionSide.Short, unrealizedPnl: -25 }),
        ],
      });
      expect(evaluateLiveVarSource('x', '$pnl', ctx)).toBe(50);
    });

    it('resolves $total from balance', () => {
      const ctx = makeCtx({ balance: { total: 25_000 } });
      expect(evaluateLiveVarSource('x', '$total', ctx)).toBe(25_000);
    });

    it('resolves $free from balance', () => {
      const ctx = makeCtx({ balance: { free: 750 } });
      expect(evaluateLiveVarSource('x', '$free', ctx)).toBe(750);
    });
  });

  describe('math expression source', () => {
    it('evaluates $ask - $bid (the spread)', () => {
      const ctx = makeCtx({
        tickers: [{ symbol: 'BTCUSDT', bid: 49_990, ask: 50_010 }],
      });
      expect(evaluateLiveVarSource('spread', '$ask-$bid', ctx)).toBe(20);
    });

    it('evaluates $last * 0.99 (a 1% downside target)', () => {
      const ctx = makeCtx({ tickers: [{ symbol: 'BTCUSDT', last: 50_000 }] });
      expect(evaluateLiveVarSource('lo', '$last*0.99', ctx)).toBe(49_500);
    });

    it('evaluates @mul * 2 (var-based math)', () => {
      const ctx = makeCtx();
      ctx.store.setVar('mul', '7.5');
      expect(evaluateLiveVarSource('twice', '@mul*2', ctx)).toBe(15);
    });

    it('evaluates compound expressions with precedence', () => {
      const ctx = makeCtx({
        tickers: [{ symbol: 'BTCUSDT', last: 100, bid: 99, ask: 101 }],
      });
      // $last - $ask + $bid → 100 - 101 + 99 = 98
      expect(evaluateLiveVarSource('e', '$last-$ask+$bid', ctx)).toBe(98);
      // $last / 2 + 10 → 50 + 10 = 60
      expect(evaluateLiveVarSource('e', '$last/2+10', ctx)).toBe(60);
    });
  });

  describe('multi-token source with anchor + offset', () => {
    it('combines `$entry +1%` into a single number', () => {
      const ctx = makeCtx({ positions: [pos({ entryPrice: 50_000, contracts: 1 })] });
      expect(evaluateLiveVarSource('above', '$entry +1%', ctx)).toBe(50_500);
    });

    it('combines `$ask -50` (absolute offset)', () => {
      const ctx = makeCtx({ tickers: [{ symbol: 'BTCUSDT', ask: 50_000 }] });
      expect(evaluateLiveVarSource('below', '$ask -50', ctx)).toBe(49_950);
    });
  });

  describe('numeric literal source', () => {
    it('accepts a bare positive number', () => {
      const ctx = makeCtx();
      expect(evaluateLiveVarSource('k', '42', ctx)).toBe(42);
    });

    it('accepts a negative number', () => {
      const ctx = makeCtx();
      expect(evaluateLiveVarSource('k', '-7.5', ctx)).toBe(-7.5);
    });
  });

  describe('static @var source', () => {
    it('resolves @y referencing a static var', () => {
      const ctx = makeCtx();
      ctx.store.setVar('y', '123.45');
      expect(evaluateLiveVarSource('x', '@y', ctx)).toBe(123.45);
    });
  });

  describe('error handling', () => {
    it('wraps inner substitute errors with the live-var name', () => {
      // No position → $entry throws "no position for BTCUSDT".
      // The wrapper should prepend "live var @x …".
      const ctx = makeCtx();
      expect(() => evaluateLiveVarSource('x', '$entry', ctx)).toThrow(
        /live var @x source '\$entry':.*no position/i,
      );
    });

    it('rejects empty source', () => {
      const ctx = makeCtx();
      expect(() => evaluateLiveVarSource('x', '', ctx)).toThrow(/empty source/);
      expect(() => evaluateLiveVarSource('x', '   ', ctx)).toThrow(/empty source/);
    });

    it('wraps unknown-constant errors', () => {
      const ctx = makeCtx();
      // Unknown $constant — the substitute layer leaves it literal, so the
      // result is non-numeric and we trip the "not numeric" branch.
      expect(() => evaluateLiveVarSource('x', '$banana', ctx)).toThrow(
        /live var @x source.*not numeric|live var @x/i,
      );
    });

    it('wraps unknown @var errors', () => {
      const ctx = makeCtx();
      expect(() => evaluateLiveVarSource('x', '@missing', ctx)).toThrow(
        /live var @x source.*Unknown variable|Unknown variable @missing/i,
      );
    });

    it('rejects multi-value sources', () => {
      // Two separate ticker reads, not combined by an anchor+offset
      // shape — substituteTokens returns two tokens.
      const ctx = makeCtx({
        tickers: [{ symbol: 'BTCUSDT', bid: 49_999, ask: 50_001, last: 50_000 }],
      });
      expect(() => evaluateLiveVarSource('x', '$bid $ask', ctx)).toThrow(
        /expected single value/,
      );
    });

    it('throws on division by zero', () => {
      const ctx = makeCtx({ tickers: [{ symbol: 'BTCUSDT', last: 50_000 }] });
      expect(() => evaluateLiveVarSource('x', '$last/0', ctx)).toThrow(
        /live var @x.*division by zero/i,
      );
    });
  });

  describe('reactivity contract — same source, different state', () => {
    it('reads CURRENT ticker, not a snapshot', () => {
      // Same store, swap ticker, expect different result. This is what
      // makes a live var "live" — repeated calls re-read the source.
      const ctx = makeCtx({ tickers: [{ symbol: 'BTCUSDT', last: 100 }] });
      expect(evaluateLiveVarSource('x', '$last', ctx)).toBe(100);
      // Mutate the ticker in place — same shape, different value.
      const tickers = ctx.exchange!.store!.tickers!;
      (tickers[0] as { last: number }).last = 150;
      expect(evaluateLiveVarSource('x', '$last', ctx)).toBe(150);
    });

    it('reads CURRENT position pnl on each evaluation', () => {
      const ctx = makeCtx({
        positions: [pos({ contracts: 1, unrealizedPnl: 10 })],
      });
      expect(evaluateLiveVarSource('x', '$pnl', ctx)).toBe(10);
      (ctx.exchange!.store!.positions![0] as { unrealizedPnl: number }).unrealizedPnl = 25;
      expect(evaluateLiveVarSource('x', '$pnl', ctx)).toBe(25);
    });
  });
});

describe('validateLiveVarSource', () => {
  it('accepts a single $constant', () => {
    expect(validateLiveVarSource('$last')).toBe(null);
    expect(validateLiveVarSource('$pnl')).toBe(null);
  });

  it('accepts an @var', () => {
    expect(validateLiveVarSource('@x')).toBe(null);
  });

  it('accepts numeric literals', () => {
    expect(validateLiveVarSource('42')).toBe(null);
    expect(validateLiveVarSource('-7.5')).toBe(null);
    expect(validateLiveVarSource('+1%')).toBe(null);
  });

  it('accepts math expressions', () => {
    expect(validateLiveVarSource('$ask-$bid')).toBe(null);
    expect(validateLiveVarSource('@mul*2')).toBe(null);
    expect(validateLiveVarSource('$last/2+10')).toBe(null);
  });

  it('accepts anchor + offset shapes', () => {
    expect(validateLiveVarSource('$entry +1%')).toBe(null);
    expect(validateLiveVarSource('$ask -50')).toBe(null);
  });

  it('accepts UI constants by SHAPE — semantic error surfaces at evaluation', () => {
    // UI constants are still `$<name>` shape. We let them through at
    // validation time and let the substitute layer throw "only
    // available in the web terminal" at evaluation if needed.
    expect(validateLiveVarSource('$click')).toBe(null);
  });

  it('rejects empty', () => {
    expect(validateLiveVarSource('')).toBe('empty source');
    expect(validateLiveVarSource('   ')).toBe('empty source');
  });

  it('rejects garbage tokens', () => {
    expect(validateLiveVarSource('banana')).toMatch(/unrecognised source token 'banana'/);
    expect(validateLiveVarSource('BTCUSDT')).toMatch(/unrecognised/);
  });

  it('rejects shapes the substitute layer leaves intact (size tokens)', () => {
    // `100$` and `$100` are sized-money tokens the substitute layer
    // does NOT collapse to a single number. Permitting them as source
    // would produce confusing errors at evaluation time. Reject.
    expect(validateLiveVarSource('100$')).toMatch(/unrecognised/);
    expect(validateLiveVarSource('$100')).toMatch(/unrecognised/);
  });

  it('rejects a malformed math token', () => {
    // Trailing operator — not a math shape.
    expect(validateLiveVarSource('$last+')).toMatch(/unrecognised/);
    expect(validateLiveVarSource('*$ask')).toMatch(/unrecognised/);
  });

  it('reports the FIRST bad token when multiple are present', () => {
    // `$ask + garbage` splits to `['$ask', '+', 'garbage']`. A bare `+`
    // is itself invalid as a source token (math operators only fly
    // INSIDE a single contiguous token), so it's the first one caught.
    const err = validateLiveVarSource('$ask + garbage');
    expect(err).toMatch(/unrecognised source token '\+'/);
  });
});
