import { describe, it, expect } from 'vitest';
import {
  PositionSide,
  MarginType,
  PositionMode,
} from '@tealstreet/safe-cex/src/types';
import type { Position } from '@tealstreet/safe-cex/src/types';
import { resolveTrackedVar } from './trackedVars';
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
  tickers?: Array<{ symbol: string; last?: number; bid?: number; ask?: number; mark?: number }>;
} = {}): PreprocessContext {
  const store = new MemoryPreprocessStore();
  const exchange = {
    store: {
      positions: opts.positions ?? [],
      orders: [],
      markets: [],
      tickers: opts.tickers ?? [],
    },
  } as unknown as ExchangeLike;
  return { exchange, symbol: 'BTCUSDT', store };
}

describe('resolveTrackedVar — high-water / low-water mark aggregator', () => {
  describe('max kind', () => {
    it('seeds the value on the FIRST resolution', () => {
      const ctx = makeCtx({ positions: [pos({ unrealizedPnl: 50 })] });
      ctx.store.setTrackedVar('hi', 'max', '$pnl');
      const entry = ctx.store.getTrackedVar('hi')!;
      expect(entry.value).toBeUndefined();

      const result = resolveTrackedVar('hi', entry, ctx, new Set());

      expect(result).toBe(50);
      expect(ctx.store.getTrackedVar('hi')!.value).toBe(50);
    });

    it('updates to a new max when source moves up', () => {
      const ctx = makeCtx({ positions: [pos({ unrealizedPnl: 50 })] });
      ctx.store.setTrackedVar('hi', 'max', '$pnl');
      resolveTrackedVar('hi', ctx.store.getTrackedVar('hi')!, ctx, new Set());

      // Source moves up.
      ctx.exchange!.store!.positions![0].unrealizedPnl = 75;
      const result = resolveTrackedVar('hi', ctx.store.getTrackedVar('hi')!, ctx, new Set());
      expect(result).toBe(75);
      expect(ctx.store.getTrackedVar('hi')!.value).toBe(75);
    });

    it('STICKS at the high when source falls back', () => {
      const ctx = makeCtx({ positions: [pos({ unrealizedPnl: 100 })] });
      ctx.store.setTrackedVar('hi', 'max', '$pnl');
      resolveTrackedVar('hi', ctx.store.getTrackedVar('hi')!, ctx, new Set());
      // Source falls.
      ctx.exchange!.store!.positions![0].unrealizedPnl = 50;
      const result = resolveTrackedVar('hi', ctx.store.getTrackedVar('hi')!, ctx, new Set());
      expect(result).toBe(100); // the high-water mark, not the current 50
    });

    it('multi-step ratchet — only updates on new highs', () => {
      const ctx = makeCtx({ positions: [pos({ unrealizedPnl: 10 })] });
      ctx.store.setTrackedVar('hi', 'max', '$pnl');
      const sequence = [10, 20, 15, 30, 25, 30, 50, 40];
      const observed: number[] = [];
      for (const v of sequence) {
        ctx.exchange!.store!.positions![0].unrealizedPnl = v;
        observed.push(
          resolveTrackedVar('hi', ctx.store.getTrackedVar('hi')!, ctx, new Set()),
        );
      }
      expect(observed).toEqual([10, 20, 20, 30, 30, 30, 50, 50]);
    });
  });

  describe('min kind', () => {
    it('seeds + ratchets DOWN', () => {
      const ctx = makeCtx({ positions: [pos({ unrealizedPnl: 100 })] });
      ctx.store.setTrackedVar('lo', 'min', '$pnl');
      // First read seeds with 100.
      expect(resolveTrackedVar('lo', ctx.store.getTrackedVar('lo')!, ctx, new Set())).toBe(100);
      // Source drops — new low.
      ctx.exchange!.store!.positions![0].unrealizedPnl = 50;
      expect(resolveTrackedVar('lo', ctx.store.getTrackedVar('lo')!, ctx, new Set())).toBe(50);
      // Source rises — stays at low.
      ctx.exchange!.store!.positions![0].unrealizedPnl = 200;
      expect(resolveTrackedVar('lo', ctx.store.getTrackedVar('lo')!, ctx, new Set())).toBe(50);
    });

    it('negative-pnl drawdown — low-water tracks the worst loss', () => {
      const ctx = makeCtx({ positions: [pos({ unrealizedPnl: -10 })] });
      ctx.store.setTrackedVar('lo', 'min', '$pnl');
      const sequence = [-10, -25, -50, -30, -75, -40];
      const observed: number[] = [];
      for (const v of sequence) {
        ctx.exchange!.store!.positions![0].unrealizedPnl = v;
        observed.push(
          resolveTrackedVar('lo', ctx.store.getTrackedVar('lo')!, ctx, new Set()),
        );
      }
      expect(observed).toEqual([-10, -25, -50, -50, -75, -75]);
    });
  });

  describe('source flexibility', () => {
    it('works with a math source', () => {
      const ctx = makeCtx({
        tickers: [{ symbol: 'BTCUSDT', bid: 100, ask: 110 }],
      });
      ctx.store.setTrackedVar('maxSpread', 'max', '$ask-$bid');
      expect(
        resolveTrackedVar('maxSpread', ctx.store.getTrackedVar('maxSpread')!, ctx, new Set()),
      ).toBe(10);
      // Spread widens.
      ctx.exchange!.store!.tickers![0].ask = 130;
      expect(
        resolveTrackedVar('maxSpread', ctx.store.getTrackedVar('maxSpread')!, ctx, new Set()),
      ).toBe(30);
      // Spread tightens — still at 30.
      ctx.exchange!.store!.tickers![0].ask = 115;
      expect(
        resolveTrackedVar('maxSpread', ctx.store.getTrackedVar('maxSpread')!, ctx, new Set()),
      ).toBe(30);
    });

    it('works with a static @-var source (degenerate but valid)', () => {
      const ctx = makeCtx();
      ctx.store.setVar('val', '42');
      ctx.store.setTrackedVar('hi', 'max', '@val');
      expect(
        resolveTrackedVar('hi', ctx.store.getTrackedVar('hi')!, ctx, new Set()),
      ).toBe(42);
    });
  });

  describe('error paths', () => {
    it('propagates source errors with the live-var wrapping', () => {
      // No position → $pnl path throws; the live-var wrapper attaches
      // `live var @hi source '$pnl': …` prefix. resolveTrackedVar
      // doesn't catch — it bubbles up.
      const ctx = makeCtx();
      ctx.store.setTrackedVar('hi', 'max', '$pnl');
      expect(() =>
        resolveTrackedVar('hi', ctx.store.getTrackedVar('hi')!, ctx, new Set()),
      ).toThrow(/live var @hi source.*no position/i);
    });

    it('detects cycle when the seen set already includes the name', () => {
      // Simulate the substitute layer already on the recursion stack
      // for 'hi'. resolveTrackedVar passes seen through; the inner
      // evaluateLiveVarSource detects the cycle.
      const ctx = makeCtx({ positions: [pos({ unrealizedPnl: 50 })] });
      ctx.store.setTrackedVar('hi', 'max', '$pnl');
      const seen = new Set<string>(['hi']);
      expect(() =>
        resolveTrackedVar('hi', ctx.store.getTrackedVar('hi')!, ctx, seen),
      ).toThrow(/cycle detected/i);
    });

    it('a value remains the previous extreme when a subsequent eval throws', () => {
      // First resolve seeds. Then the source fails. The tracker value
      // is unchanged (we never updateTrackedVar on the failed call).
      const ctx = makeCtx({ positions: [pos({ unrealizedPnl: 50 })] });
      ctx.store.setTrackedVar('hi', 'max', '$pnl');
      resolveTrackedVar('hi', ctx.store.getTrackedVar('hi')!, ctx, new Set());
      expect(ctx.store.getTrackedVar('hi')!.value).toBe(50);

      // Remove the position so $pnl throws.
      ctx.exchange!.store!.positions = [];
      expect(() =>
        resolveTrackedVar('hi', ctx.store.getTrackedVar('hi')!, ctx, new Set()),
      ).toThrow();
      // Value preserved.
      expect(ctx.store.getTrackedVar('hi')!.value).toBe(50);
    });
  });
});
