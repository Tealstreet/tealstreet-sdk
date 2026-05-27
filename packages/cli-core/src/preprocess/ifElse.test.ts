import { describe, it, expect } from 'vitest';
import { PositionSide, MarginType, PositionMode } from '@tealstreet/safe-cex/src/types';
import type { Position } from '@tealstreet/safe-cex/src/types';
import { preprocessCommand } from './preprocess';
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

function makeCtx(
  opts: {
    positions?: Position[];
    tickers?: Array<{ symbol: string; last?: number; mark?: number; index?: number; bid?: number; ask?: number }>;
    orders?: Array<{ symbol: string; status: string }>;
  } = {},
): PreprocessContext {
  const store = new MemoryPreprocessStore();
  const exchange = {
    store: {
      positions: opts.positions ?? [],
      orders: opts.orders ?? [],
      markets: [],
      tickers: opts.tickers ?? [],
    },
  } as unknown as ExchangeLike;
  return { exchange, symbol: 'BTCUSDT', store };
}

const cmds = (input: string, ctx: PreprocessContext) => preprocessCommand(input, ctx).steps.map((s) => s.command);

describe('if (<cond>) (<then>) [else (<else>)]', () => {
  describe('condition evaluation', () => {
    it('runs the then-branch when condition is true', () => {
      const ctx = makeCtx({ positions: [pos({ contracts: 1 })] });
      expect(cmds('if (position) (close)', ctx)).toEqual(['close']);
    });

    it('runs the else-branch when condition is false', () => {
      const ctx = makeCtx();
      expect(cmds('if (position) (close) else (sleep 1m)', ctx)).toEqual(['sleep 1m']);
    });

    it('emits no steps when false and no else', () => {
      const ctx = makeCtx();
      expect(cmds('if (position) (close)', ctx)).toEqual([]);
    });

    it('long / short conditions filter by side', () => {
      const ctx = makeCtx({
        positions: [pos({ side: PositionSide.Short, contracts: 1 })],
      });
      expect(cmds('if (long) (close) else (close shorts)', ctx)).toEqual(['close shorts']);
      expect(cmds('if (short) (close shorts) else (sleep)', ctx)).toEqual(['close shorts']);
    });

    it('price comparators read the focused-symbol ticker', () => {
      const ctx = makeCtx({
        tickers: [{ symbol: 'BTCUSDT', last: 50_000 }],
      });
      expect(cmds('if (price > 49000) (close)', ctx)).toEqual(['close']);
      expect(cmds('if (price > 60000) (close) else (sleep 1m)', ctx)).toEqual(['sleep 1m']);
      expect(cmds('if (price <= 50000) (close) else (sleep)', ctx)).toEqual(['close']);
    });

    it('`==` and `!=` comparators (exact match)', () => {
      const ctx = makeCtx({
        tickers: [{ symbol: 'BTCUSDT', last: 50_000 }],
      });
      expect(cmds('if (price == 50000) (close) else (sleep)', ctx)).toEqual(['close']);
      expect(cmds('if (price != 50000) (close) else (sleep)', ctx)).toEqual(['sleep']);
      expect(cmds('if (price != 49000) (close) else (sleep)', ctx)).toEqual(['close']);
    });

    it('`==` on pnl handles exact-zero (the common "flat" check)', () => {
      const ctx = makeCtx({
        positions: [pos({ contracts: 1, unrealizedPnl: 0 })],
      });
      expect(cmds('if (pnl == 0) (close) else (sleep)', ctx)).toEqual(['close']);
    });

    it('pnl comparator reads focused-symbol unrealizedPnl (sum)', () => {
      const ctx = makeCtx({
        positions: [
          pos({ id: 'a', side: PositionSide.Long, contracts: 1, unrealizedPnl: 75 }),
          pos({ id: 'b', side: PositionSide.Short, contracts: 0.5, unrealizedPnl: -25 }),
        ],
      });
      // Sum is 50 — > 0 is true.
      expect(cmds('if (pnl > 0) (close) else (sleep)', ctx)).toEqual(['close']);
      expect(cmds('if (pnl > 100) (close) else (sleep 1m)', ctx)).toEqual(['sleep 1m']);
    });

    it('fill condition is true when no live orders on the symbol', () => {
      const ctxNoOrders = makeCtx({ orders: [] });
      expect(cmds('if (fill) (close) else (sleep)', ctxNoOrders)).toEqual(['close']);
      const ctxOpen = makeCtx({
        orders: [{ symbol: 'BTCUSDT', status: 'open' }],
      });
      expect(cmds('if (fill) (close) else (sleep 1m)', ctxOpen)).toEqual(['sleep 1m']);
    });
  });

  describe('multi-statement branches', () => {
    it('then-branch with `;` produces multiple steps', () => {
      const ctx = makeCtx({ positions: [pos({ contracts: 1 })] });
      expect(cmds('if (position) (cancel; close)', ctx)).toEqual(['cancel', 'close']);
    });

    it('else-branch with `&` produces fan-out steps', () => {
      const ctx = makeCtx();
      expect(cmds('if (position) (close) else (cancel & sleep 1s)', ctx)).toEqual(['cancel', 'sleep 1s']);
    });

    it('preserves block flags inside branches', () => {
      const ctx = makeCtx();
      const r = preprocessCommand('if (position) (close) else (sleep 1m)', ctx);
      expect(r.steps).toEqual([{ command: 'sleep 1m', block: true }]);
    });
  });

  describe('chain composition', () => {
    it('top-level chain after the if is preserved', () => {
      const ctx = makeCtx({ positions: [pos({ contracts: 1 })] });
      expect(cmds('if (position) (close); buy 1$', ctx)).toEqual(['close', 'buy 1$']);
    });

    it('top-level chain before the if is preserved', () => {
      const ctx = makeCtx();
      expect(cmds('cancel; if (position) (close)', ctx)).toEqual(['cancel']);
    });

    it('runs as expected inside repeat', () => {
      const ctx = makeCtx({
        tickers: [{ symbol: 'BTCUSDT', last: 50_000 }],
      });
      expect(cmds('repeat 3 (if (price > 40000) (cancel))', ctx)).toEqual(['cancel', 'cancel', 'cancel']);
    });

    it('nested if compose — inner condition evaluates against the same ctx', () => {
      const ctx = makeCtx({
        positions: [pos({ contracts: 1, unrealizedPnl: 75 })],
        tickers: [{ symbol: 'BTCUSDT', last: 50_000 }],
      });
      expect(cmds('if (position) (if (pnl > 50) (close) else (sleep 1s))', ctx)).toEqual(['close']);
      // PnL too low → inner else → sleep 1s.
      const ctxLow = makeCtx({
        positions: [pos({ contracts: 1, unrealizedPnl: 10 })],
      });
      expect(cmds('if (position) (if (pnl > 50) (close) else (sleep 1s))', ctxLow)).toEqual(['sleep 1s']);
    });
  });

  describe('chain-modifier inheritance', () => {
    it('outer set ec propagates to branch steps', () => {
      const ctx = makeCtx({ positions: [pos({ contracts: 1 })] });
      const r = preprocessCommand('set ec; if (position) (cancel; close)', ctx);
      expect(r.steps.every((s) => s.errorMode === 'continue')).toBe(true);
    });

    it('inner set ec inside a branch flips for the rest of that branch only', () => {
      const ctx = makeCtx();
      const r = preprocessCommand('if (long) (close) else (set ec; cancel; close)', ctx);
      // Only the else branch fires (no long position).
      expect(r.steps).toEqual([
        { command: 'cancel', block: false, errorMode: 'continue' },
        { command: 'close', block: false, errorMode: 'continue', awaitPrior: true },
      ]);
    });

    it("outer retry propagates to inner steps that didn't override it", () => {
      const ctx = makeCtx({ positions: [pos({ contracts: 1 })] });
      const r = preprocessCommand('retry 3 if (position) (close)', ctx);
      // The whole if-segment got retry: 3 tagged outside; inner step
      // should inherit it.
      expect(r.steps).toEqual([{ command: 'close', block: false, retry: 3 }]);
    });
  });

  describe('errors / unparseable', () => {
    it('"if" without `(` after it falls through as a literal command — preprocess does NOT claim it', () => {
      // `if foo bar` doesn't match the IF_HEAD_RE so tryExpandIfElse
      // returns null. The segment becomes a normal step; the
      // dispatcher will reject `if` as an unknown command at
      // execution time. This keeps the door open for a future
      // builtin command literally named `if` without breaking
      // existing scripts.
      const ctx = makeCtx();
      const r = preprocessCommand('if foo bar', ctx);
      expect(r.steps).toEqual([{ command: 'if foo bar', block: false }]);
    });

    it('throws on missing then-branch', () => {
      const ctx = makeCtx();
      expect(() => preprocessCommand('if (position)', ctx)).toThrow(/expected '\(<then>\)'/);
    });

    it('throws on garbage between then and else', () => {
      const ctx = makeCtx();
      expect(() => preprocessCommand('if (position) (close) garbage (sleep)', ctx)).toThrow(
        /unexpected trailing tokens/,
      );
    });

    it('throws on unparseable condition', () => {
      const ctx = makeCtx();
      expect(() => preprocessCommand('if (banana > 100) (close)', ctx)).toThrow(/unrecognized condition/);
    });

    it('throws when not connected', () => {
      const store = new MemoryPreprocessStore();
      const ctx: PreprocessContext = {
        exchange: null as unknown as ExchangeLike,
        symbol: 'BTCUSDT',
        store,
      };
      expect(() => preprocessCommand('if (position) (close)', ctx)).toThrow(/not connected/);
    });

    it('rejects an `else` block followed by trailing tokens', () => {
      const ctx = makeCtx();
      expect(() => preprocessCommand('if (position) (close) else (sleep) extra', ctx)).toThrow(
        /unexpected trailing tokens after else-branch/,
      );
    });
  });

  describe('end inside branches', () => {
    it('`if (cond) (end); buy 1$` — true branch cuts the rest of the chain', () => {
      const ctx = makeCtx({ positions: [pos({ contracts: 1 })] });
      expect(cmds('if (position) (end); buy 1$', ctx)).toEqual([]);
    });

    it('`if (cond) (end); buy 1$` — false branch leaves the chain intact', () => {
      const ctx = makeCtx(); // no positions
      expect(cmds('if (position) (end); buy 1$', ctx)).toEqual(['buy 1$']);
    });

    it('else-branch `end` cuts the rest (and emits earlier branch steps)', () => {
      // No position → else branch → `sleep 1s` emits, then `end` cuts,
      // so the trailing `buy 1$` is dropped.
      const ctx = makeCtx();
      expect(cmds('if (position) (buy 2$) else (sleep 1s; end); buy 1$', ctx)).toEqual(['sleep 1s']);
    });

    it('nested — `if (a) (if (b) (end)); buy 1$` cuts when both true', () => {
      const ctx = makeCtx({
        positions: [pos({ contracts: 1 })],
        tickers: [{ symbol: 'BTCUSDT', last: 50_000 }],
      });
      expect(cmds('if (position) (if (price > 49000) (end)); buy 1$', ctx)).toEqual([]);
    });

    it('nested — inner condition false → outer chain continues', () => {
      const ctx = makeCtx({
        positions: [pos({ contracts: 1 })],
        tickers: [{ symbol: 'BTCUSDT', last: 50_000 }],
      });
      expect(cmds('if (position) (if (price > 60000) (end)); buy 1$', ctx)).toEqual(['buy 1$']);
    });

    it('`end` propagates `endEarly` upward through the if-recursion', () => {
      const ctx = makeCtx({ positions: [pos({ contracts: 1 })] });
      const r = preprocessCommand('if (position) (end); buy 1$', ctx);
      expect(r.endEarly).toBe(true);
    });
  });

  describe('@<var> LHS conditions inside if', () => {
    it('`if (@x > N)` resolves the live var at evaluation time', () => {
      const ctx = makeCtx({
        tickers: [{ symbol: 'BTCUSDT', last: 50_000 }],
      });
      ctx.store.setLiveVar('x', '$last');
      expect(cmds('if (@x > 40000) (close) else (sleep)', ctx)).toEqual(['close']);
      expect(cmds('if (@x > 60000) (close) else (sleep)', ctx)).toEqual(['sleep']);
    });

    it('@x with math source evaluates correctly inside if', () => {
      const ctx = makeCtx({
        tickers: [{ symbol: 'BTCUSDT', bid: 49_990, ask: 50_010 }],
      });
      ctx.store.setLiveVar('spread', '$ask-$bid');
      // spread = 20
      expect(cmds('if (@spread > 15) (close) else (sleep)', ctx)).toEqual(['close']);
      expect(cmds('if (@spread < 15) (close) else (sleep)', ctx)).toEqual(['sleep']);
    });

    it('@<staticVar> also works (frozen numeric value)', () => {
      const ctx = makeCtx();
      ctx.store.setVar('x', '75');
      expect(cmds('if (@x > 50) (close) else (sleep)', ctx)).toEqual(['close']);
      expect(cmds('if (@x > 100) (close) else (sleep)', ctx)).toEqual(['sleep']);
    });

    it('@<missing> throws — fail loudly rather than silently picking the else branch', () => {
      const ctx = makeCtx();
      expect(() => preprocessCommand('if (@missing > 50) (close)', ctx)).toThrow(/@missing is not set/);
    });

    it('@<staticVar> with non-numeric value throws', () => {
      const ctx = makeCtx();
      ctx.store.setVar('x', 'banana');
      expect(() => preprocessCommand('if (@x > 50) (close)', ctx)).toThrow(/@x='banana' is not numeric/);
    });

    it('all comparators work', () => {
      const ctx = makeCtx();
      ctx.store.setVar('x', '50');
      expect(cmds('if (@x == 50) (a) else (b)', ctx)).toEqual(['a']);
      expect(cmds('if (@x != 50) (a) else (b)', ctx)).toEqual(['b']);
      expect(cmds('if (@x >= 50) (a) else (b)', ctx)).toEqual(['a']);
      expect(cmds('if (@x <= 50) (a) else (b)', ctx)).toEqual(['a']);
    });

    it('combinator: `if (either (@x > 100) or (@y > 100))` plumbs ctx into subs', () => {
      const ctx = makeCtx({
        tickers: [{ symbol: 'BTCUSDT', last: 50, mark: 200 }],
      });
      ctx.store.setLiveVar('x', '$last');
      ctx.store.setLiveVar('y', '$mark');
      // x = 50 (not > 100), y = 200 (> 100) → either is true → close.
      expect(cmds('if (either (@x > 100) or (@y > 100)) (close) else (sleep)', ctx)).toEqual(['close']);
    });

    it('`@<trackedVar>` as LHS — seeds + compares against target', () => {
      const ctx = makeCtx({
        tickers: [{ symbol: 'BTCUSDT', last: 100 }],
      });
      ctx.store.setTrackedVar('hi', 'max', '$last');
      // First check seeds @hi to 100. 100 > 50 → close.
      expect(cmds('if (@hi > 50) (close) else (sleep)', ctx)).toEqual(['close']);
      // Tracker value persisted across calls — 100 < 200 → sleep.
      expect(cmds('if (@hi > 200) (close) else (sleep)', ctx)).toEqual(['sleep']);
    });

    it('tracker LHS — ratchets across multiple if evaluations', () => {
      const tickers = [{ symbol: 'BTCUSDT', last: 100 }];
      const ctx = makeCtx({ tickers });
      ctx.store.setTrackedVar('hi', 'max', '$last');
      cmds('if (@hi > 0) (close)', ctx); // seeds to 100
      tickers[0].last = 200;
      cmds('if (@hi > 0) (close)', ctx); // ratchets to 200
      tickers[0].last = 50;
      cmds('if (@hi > 0) (close)', ctx); // sticks at 200
      expect(ctx.store.getTrackedVar('hi')!.value).toBe(200);
    });
  });

  describe('@<var> RHS conditions (e.g. `if (pnl < @threshold)`)', () => {
    it('price < @<staticVar> resolves the RHS at check time', () => {
      const ctx = makeCtx({
        tickers: [{ symbol: 'BTCUSDT', last: 50_000 }],
      });
      ctx.store.setVar('floor', '40000');
      expect(cmds('if (price < @floor) (close) else (sleep)', ctx)).toEqual(['sleep']);
      expect(cmds('if (price > @floor) (close) else (sleep)', ctx)).toEqual(['close']);
    });

    it('pnl < @<liveVar> — both sides re-evaluate at check time', () => {
      const ctx = makeCtx({
        positions: [pos({ unrealizedPnl: 50 })],
        tickers: [{ symbol: 'BTCUSDT', last: 60 }],
      });
      ctx.store.setLiveVar('floor', '$last'); // = 60
      expect(cmds('if (pnl < @floor) (close) else (sleep)', ctx)).toEqual(['close']);
      ctx.exchange!.store!.positions![0].unrealizedPnl = 100;
      expect(cmds('if (pnl < @floor) (close) else (sleep)', ctx)).toEqual(['sleep']);
    });

    it('trailing-stop pattern — pnl < @stop where @stop is `track max @hi - 1` analog', () => {
      // `track max @hi $pnl; sets @stop @hi-1; wait pnl < @stop`.
      // Here we use `if` to step through deterministically.
      const ctx = makeCtx({
        positions: [pos({ unrealizedPnl: 100 })],
      });
      ctx.store.setTrackedVar('hi', 'max', '$pnl');
      ctx.store.setLiveVar('stop', '@hi-1');
      // First check: pnl=100. @hi seeds to 100, @stop = 99. 100 < 99 → false.
      expect(cmds('if (pnl < @stop) (close) else (sleep)', ctx)).toEqual(['sleep']);
      // PnL climbs to 200 → @hi ratchets, @stop = 199.
      ctx.exchange!.store!.positions![0].unrealizedPnl = 200;
      expect(cmds('if (pnl < @stop) (close) else (sleep)', ctx)).toEqual(['sleep']);
      // PnL falls to 150 — @hi sticks at 200, @stop = 199. 150 < 199 → close.
      ctx.exchange!.store!.positions![0].unrealizedPnl = 150;
      expect(cmds('if (pnl < @stop) (close) else (sleep)', ctx)).toEqual(['close']);
    });

    it('@x < @y — both sides are @-refs', () => {
      const ctx = makeCtx();
      ctx.store.setVar('a', '10');
      ctx.store.setVar('b', '20');
      expect(cmds('if (@a < @b) (close) else (sleep)', ctx)).toEqual(['close']);
      expect(cmds('if (@a > @b) (close) else (sleep)', ctx)).toEqual(['sleep']);
    });

    it('RHS @-var is name-scoped — unbound RHS throws "is not set"', () => {
      const ctx = makeCtx({ tickers: [{ symbol: 'BTCUSDT', last: 50_000 }] });
      // Note: `if`'s condition body skips substitution (FULL_PASSTHROUGH-
      // like via tryExpandIfElse), so the unknown @<typo> falls through
      // to parseCondition which extracts it as a var RHS. resolveRhsTarget
      // calls resolveAtNameForCondition which throws at check time.
      expect(() => preprocessCommand('if (price > @typo) (close)', ctx)).toThrow(/@typo is not set/);
    });

    it('garbage RHS still falls through to "unrecognized condition"', () => {
      const ctx = makeCtx({ tickers: [{ symbol: 'BTCUSDT', last: 50_000 }] });
      // `banana` matches neither the numeric literal regex nor the
      // `@<name>` shape, so parseCondition returns null and the
      // top-level error fires.
      expect(() => preprocessCommand('if (price > banana) (close)', ctx)).toThrow(/unrecognized condition/);
    });
  });

  describe('combinator conditions inside if', () => {
    it('`if (either (...) or (...))` picks the then branch when any sub-cond is true', () => {
      const ctx = makeCtx({
        positions: [pos({ contracts: 1 })],
      });
      // position is true, pnl > 100 is false. either → true → then.
      expect(cmds('if (either (position) or (pnl > 100)) (close) else (sleep)', ctx)).toEqual(['close']);
    });

    it('`if (both (...) and (...))` requires all sub-conds to be true', () => {
      const ctx = makeCtx({
        positions: [pos({ contracts: 1, unrealizedPnl: 50 })],
        tickers: [{ symbol: 'BTCUSDT', last: 50_000 }],
      });
      // price > 49000 is true; pnl > 100 is false. both → false → else.
      expect(cmds('if (both (price > 49000) and (pnl > 100)) (close) else (sleep)', ctx)).toEqual(['sleep']);
    });

    it('nested combinators work in if', () => {
      const ctx = makeCtx({
        positions: [pos({ contracts: 1 })],
        tickers: [{ symbol: 'BTCUSDT', last: 50_000 }],
      });
      // outer both: inner either(position OR price>60000) AND price<60000
      // inner either: position true. outer both: true AND true. → then.
      expect(
        cmds('if (both (either (position) or (price > 60000)) and (price < 60000)) (close) else (sleep)', ctx),
      ).toEqual(['close']);
    });
  });
});
