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

function makeCtx(positions: Position[] = []): PreprocessContext {
  const store = new MemoryPreprocessStore();
  const exchange = {
    store: { positions, orders: [], markets: [], tickers: [] },
  } as unknown as ExchangeLike;
  return { exchange, symbol: 'BTCUSDT', store };
}

const cmds = (input: string, ctx: PreprocessContext) =>
  preprocessCommand(input, ctx).steps.map((s) => s.command);

const blocks = (input: string, ctx: PreprocessContext) =>
  preprocessCommand(input, ctx).steps.map((s) => s.block);

describe('loop <variant>: <body> — preprocess fan-out', () => {
  describe('variant filtering', () => {
    it('"loop longs:" emits one step per long position', () => {
      const ctx = makeCtx([
        pos({ id: 'a', symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1 }),
        pos({ id: 'b', symbol: 'ETHUSDT', side: PositionSide.Long, contracts: 10 }),
        pos({ id: 'c', symbol: 'SOLUSDT', side: PositionSide.Short, contracts: 100 }),
      ]);
      expect(cmds('loop longs: close', ctx)).toEqual([
        'close BTCUSDT',
        'close ETHUSDT',
      ]);
    });

    it('"loop shorts:" emits one step per short position', () => {
      const ctx = makeCtx([
        pos({ id: 'a', symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1 }),
        pos({ id: 'b', symbol: 'SOLUSDT', side: PositionSide.Short, contracts: 100 }),
        pos({ id: 'c', symbol: 'WIFUSDT', side: PositionSide.Short, contracts: 200 }),
      ]);
      // Body with no $constants — pure passthrough so the only thing
      // we lock in here is that the right symbols got appended.
      expect(cmds('loop shorts: cancel buys', ctx)).toEqual([
        'cancel buys SOLUSDT',
        'cancel buys WIFUSDT',
      ]);
    });

    it.each(['openpos', 'open', 'allpos', 'all'])(
      '"loop %s:" emits one step per open position regardless of side',
      (variant) => {
        const ctx = makeCtx([
          pos({ id: 'a', symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1 }),
          pos({ id: 'b', symbol: 'ETHUSDT', side: PositionSide.Short, contracts: 10 }),
        ]);
        expect(cmds(`loop ${variant}: close`, ctx)).toEqual([
          'close BTCUSDT',
          'close ETHUSDT',
        ]);
      },
    );

    it('skips positions with contracts === 0', () => {
      const ctx = makeCtx([
        pos({ id: 'a', symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1 }),
        pos({ id: 'b', symbol: 'ETHUSDT', side: PositionSide.Long, contracts: 0 }),
      ]);
      expect(cmds('loop longs: close', ctx)).toEqual(['close BTCUSDT']);
    });

    it('emits zero steps when no positions match the variant', () => {
      const ctx = makeCtx([
        pos({ symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1 }),
      ]);
      expect(cmds('loop shorts: close', ctx)).toEqual([]);
    });

    it('deduplicates symbols (hedge mode: one long + one short → 1 step in openpos)', () => {
      const ctx = makeCtx([
        pos({ id: 'l', symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1 }),
        pos({ id: 's', symbol: 'BTCUSDT', side: PositionSide.Short, contracts: 0.5 }),
      ]);
      expect(cmds('loop openpos: close', ctx)).toEqual(['close BTCUSDT']);
    });
  });

  describe('error handling', () => {
    it('throws on an empty body', () => {
      const ctx = makeCtx([pos({ symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1 })]);
      expect(() => preprocessCommand('loop longs:', ctx)).toThrow(/empty body/);
    });

    it('throws on an unknown variant', () => {
      const ctx = makeCtx([pos()]);
      expect(() => preprocessCommand('loop wat: close', ctx)).toThrow(/unknown variant/);
    });
  });

  describe('composition', () => {
    it('substitutes $constants AFTER fan-out (per-leg snapshot)', () => {
      // $entry resolves per-symbol because the trailing instrument
      // changes which position the symbol/$entry pair refers to. With
      // current substitute logic, $entry is symbol-agnostic — it
      // reads from ctx.symbol, not from the trailing instrument arg.
      // That means in a `loop longs: ... at $entry` shape, $entry
      // would read the FOCUSED symbol's entry, not each leg's. Lock
      // that in here so a future per-leg-context refactor doesn't
      // silently change semantics.
      const ctx = makeCtx([
        pos({ id: 'a', symbol: 'BTCUSDT', side: PositionSide.Long, entryPrice: 50_000, contracts: 1 }),
        pos({ id: 'b', symbol: 'ETHUSDT', side: PositionSide.Long, entryPrice: 3_000, contracts: 10 }),
      ]);
      // Both expanded steps see ctx.symbol = BTCUSDT entry = 50_000.
      expect(cmds('loop longs: stop sell at $entry -2%', ctx)).toEqual([
        'stop sell at 49000 BTCUSDT',
        'stop sell at 49000 ETHUSDT',
      ]);
    });

    it('inside repeat — each iteration re-expands the loop', () => {
      const ctx = makeCtx([
        pos({ symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1 }),
        pos({ symbol: 'ETHUSDT', side: PositionSide.Long, contracts: 10 }),
      ]);
      // repeat expands to `loop longs: close; loop longs: close`,
      // then each `loop` expands to two steps → 4 total.
      expect(cmds('repeat 2 (loop longs: close)', ctx)).toEqual([
        'close BTCUSDT',
        'close ETHUSDT',
        'close BTCUSDT',
        'close ETHUSDT',
      ]);
    });

    it('aliasable — `alias risk-all "loop longs: stop -2%"`', () => {
      const ctx = makeCtx([
        pos({ symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1 }),
        pos({ symbol: 'ETHUSDT', side: PositionSide.Long, contracts: 10 }),
      ]);
      ctx.store.setAlias('riskAll', 'loop longs: stop sell at $entry -2%');
      expect(cmds('riskAll', ctx)).toEqual([
        'stop sell at 49000 BTCUSDT',
        'stop sell at 49000 ETHUSDT',
      ]);
    });

    it('chains around a loop survive normally', () => {
      const ctx = makeCtx([
        pos({ symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1 }),
        pos({ symbol: 'ETHUSDT', side: PositionSide.Long, contracts: 10 }),
      ]);
      // The pre-loop and post-loop steps still run. `sleep` keeps its
      // block flag.
      const result = preprocessCommand(
        'cancel; loop longs: close; sleep 5s',
        ctx,
      );
      expect(result.steps.map((s) => s.command)).toEqual([
        'cancel',
        'close BTCUSDT',
        'close ETHUSDT',
        'sleep 5s',
      ]);
      expect(result.steps.map((s) => s.block)).toEqual([
        false,
        false,
        false,
        true,
      ]);
    });

    it('sleep inside the loop body is preserved with block:true', () => {
      const ctx = makeCtx([
        pos({ symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1 }),
      ]);
      // `loop longs: sleep 1s` would expand to `sleep 1s BTCUSDT`,
      // which is a malformed sleep. Don't suggest this in docs — but
      // lock in that the sleep classifier still fires on the head.
      expect(blocks('loop longs: sleep 1s', ctx)).toEqual([true]);
    });
  });

  describe('non-loop segments pass through', () => {
    it('"loop" without colon is NOT a loop — passes through', () => {
      const ctx = makeCtx([pos()]);
      // Currently, `loop` as a head with no colon falls through to
      // preprocessSegment, which doesn't recognize it as a command.
      // The runner will reject it. Lock in that we don't accidentally
      // expand it as a loop.
      expect(cmds('loop something', ctx)).toEqual(['loop something']);
    });

    it('something that LOOKS like loop but uses semicolon separator stays a chain', () => {
      const ctx = makeCtx([pos()]);
      // `loop foo; bar` — first segment is `loop foo` (no colon → no
      // loop), second is `bar`. Each step is independent.
      expect(cmds('loop foo; bar', ctx)).toEqual(['loop foo', 'bar']);
    });
  });
});
