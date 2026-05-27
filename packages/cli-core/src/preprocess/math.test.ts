import { describe, it, expect } from 'vitest';
import { PositionSide, MarginType, PositionMode } from '@tealstreet/safe-cex/src/types';
import type { Position } from '@tealstreet/safe-cex/src/types';
import { preprocessSegment } from './substitute';
import { tryEvalMathToken } from './math';
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

describe('tryEvalMathToken — pure parser/evaluator', () => {
  // Trivial resolver — only handles digits and a fixed map. Real
  // wiring is exercised by the integration tests below via
  // preprocessSegment.
  const fixed: Record<string, number> = {
    '$x': 10,
    '$y': 3,
    '@a': 5,
    '@b': 2,
  };
  const resolve = (op: string): number => {
    if (/^-?(?:\d+(?:\.\d+)?|\.\d+)$/.test(op)) return parseFloat(op);
    if (op in fixed) return fixed[op];
    throw new Error(`unknown operand ${op}`);
  };

  it('returns null for tokens without a math operator', () => {
    expect(tryEvalMathToken('$entry', resolve)).toBeNull();
    expect(tryEvalMathToken('@qtymul', resolve)).toBeNull();
    expect(tryEvalMathToken('100', resolve)).toBeNull();
    expect(tryEvalMathToken('hello', resolve)).toBeNull();
  });

  it('evaluates simple +/- chains left-to-right', () => {
    expect(tryEvalMathToken('1+2', resolve)).toBe('3');
    expect(tryEvalMathToken('10-3', resolve)).toBe('7');
    expect(tryEvalMathToken('1+2+3', resolve)).toBe('6');
    expect(tryEvalMathToken('10-3-2', resolve)).toBe('5');
    expect(tryEvalMathToken('1+2-3', resolve)).toBe('0');
  });

  it('evaluates simple */ chains left-to-right', () => {
    expect(tryEvalMathToken('2*3', resolve)).toBe('6');
    expect(tryEvalMathToken('10/2', resolve)).toBe('5');
    expect(tryEvalMathToken('2*3*4', resolve)).toBe('24');
    expect(tryEvalMathToken('100/4/5', resolve)).toBe('5');
  });

  it('respects standard precedence (* and / before + and -)', () => {
    expect(tryEvalMathToken('2+3*4', resolve)).toBe('14');
    expect(tryEvalMathToken('2*3+4', resolve)).toBe('10');
    expect(tryEvalMathToken('10-6/2', resolve)).toBe('7');
    expect(tryEvalMathToken('1+2*3-4', resolve)).toBe('3');
  });

  it('supports a unary minus on the first operand', () => {
    expect(tryEvalMathToken('-5*2', resolve)).toBe('-10');
    expect(tryEvalMathToken('-5+10', resolve)).toBe('5');
  });

  it('resolves $constants and @vars through the resolver', () => {
    expect(tryEvalMathToken('$x*2', resolve)).toBe('20');
    expect(tryEvalMathToken('1*@a', resolve)).toBe('5');
    expect(tryEvalMathToken('$x/$y', resolve)).toBe('3.33333333333');
    expect(tryEvalMathToken('@a-@b', resolve)).toBe('3');
    expect(tryEvalMathToken('$x*@a-@b', resolve)).toBe('48');
  });

  it('throws on division by zero', () => {
    const r2: typeof resolve = (op) => (op === '@zero' ? 0 : resolve(op));
    expect(() => tryEvalMathToken('1/@zero', r2)).toThrow(/division by zero/);
  });

  it('throws on unknown operands (propagates resolver error)', () => {
    expect(() => tryEvalMathToken('1+@nope', resolve)).toThrow(/unknown operand/);
  });

  it('does not match shapes with trailing operators or double operators', () => {
    expect(tryEvalMathToken('1+', resolve)).toBeNull();
    expect(tryEvalMathToken('1+*2', resolve)).toBeNull();
    // `+1` is a unary that should NOT match either — needs a chain.
    expect(tryEvalMathToken('+1', resolve)).toBeNull();
  });

  it('does not match decimal-only or operator-only tokens', () => {
    expect(tryEvalMathToken('.', resolve)).toBeNull();
    expect(tryEvalMathToken('+', resolve)).toBeNull();
  });
});

// ─── End-to-end through preprocessSegment ────────────────────────────

describe('math expressions in preprocessSegment', () => {
  it('substitutes a position-anchored math expression', () => {
    const ctx = makeCtx([pos({ entryPrice: 50_000 })]);
    expect(preprocessSegment('buy 0.1 at $entry-100', ctx)).toBe('buy 0.1 at 49900');
    expect(preprocessSegment('buy 0.1 at $entry+250', ctx)).toBe('buy 0.1 at 50250');
  });

  it('substitutes math involving @vars', () => {
    const ctx = makeCtx([pos({ entryPrice: 50_000 })]);
    ctx.store.setVar('mul', '1.8');
    // 100 * 1.8 = 180
    expect(preprocessSegment('buy 100*@mul', ctx)).toBe('buy 180');
    // $entry - 50 * 1.8 = 50000 - 90 = 49910 (precedence)
    expect(preprocessSegment('sell 0.1 at $entry-50*@mul', ctx)).toBe(
      'sell 0.1 at 49910',
    );
  });

  it('mixes ticker and position constants', () => {
    const ctx = makeCtx([pos({ entryPrice: 50_000 })]);
    (ctx.exchange!.store as { tickers: unknown[] }).tickers = [
      {
        symbol: 'BTCUSDT',
        bid: 49_995,
        ask: 50_005,
        last: 50_000,
        mark: 50_000,
        index: 50_000,
      },
    ];
    // Spread ($ask-$bid) = 10
    expect(preprocessSegment('echo $ask-$bid', ctx)).toBe('echo 10');
    // Mid = ($ask+$bid)/2 = 50000 (left-to-right with no parens →
    // 50005 + (49995/2) = 50005 + 24997.5 = 75002.5). Document the
    // left-to-right semantics so users don't expect arithmetic
    // grouping.
    expect(preprocessSegment('echo $ask+$bid/2', ctx)).toBe('echo 75002.5');
  });

  it('errors on @var that is not numeric', () => {
    const ctx = makeCtx();
    ctx.store.setVar('label', 'hello');
    expect(() => preprocessSegment('echo 1+@label', ctx)).toThrow(/not numeric/);
  });

  it('errors on division by zero in a substituted expression', () => {
    const ctx = makeCtx();
    ctx.store.setVar('z', '0');
    expect(() => preprocessSegment('echo 10/@z', ctx)).toThrow(/division by zero/);
  });

  it('does not interfere with `$entry +1%` (two-token shape still works)', () => {
    const ctx = makeCtx([pos({ entryPrice: 50_000 })]);
    // Two tokens — the ENTRY_OFFSET branch handles it.
    expect(preprocessSegment('buy 1 at $entry +1%', ctx)).toBe('buy 1 at 50500');
  });

  it('does not interfere with size tokens like `100$` / `5%`', () => {
    const ctx = makeCtx();
    // `100$` is not math — it's a size suffix. MATH_TOKEN_RE doesn't
    // match because there's no operator between operand-shaped chunks.
    expect(preprocessSegment('buy 100$', ctx)).toBe('buy 100$');
    // `5%` — same. MATH_TOKEN_RE skips.
    expect(preprocessSegment('close 5%', ctx)).toBe('close 5%');
  });

  it('does not interfere with user variables that have no math operator', () => {
    const ctx = makeCtx();
    ctx.store.setVar('size', '100$');
    expect(preprocessSegment('buy @size', ctx)).toBe('buy 100$');
  });

  it('errors clearly when a $constant in a math expression is unknown', () => {
    const ctx = makeCtx();
    expect(() => preprocessSegment('echo $whatever-1', ctx)).toThrow(
      /math: unknown constant \$whatever/,
    );
  });

  it('keeps unrelated tokens like BTCUSDT/USDT as a literal pass-through', () => {
    const ctx = makeCtx();
    // BTCUSDT is not an operand shape (no leading $/@/digit). The
    // `/USDT` part is also not an operand. The token has a `/`, but
    // the regex doesn't match because `BTCUSDT` doesn't fit the
    // operand alternation. So it passes through unchanged.
    expect(preprocessSegment('cancel BTCUSDT/USDT', ctx)).toBe('cancel BTCUSDT/USDT');
  });
});
