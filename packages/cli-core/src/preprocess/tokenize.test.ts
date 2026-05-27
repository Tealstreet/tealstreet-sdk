import { describe, it, expect } from 'vitest';
import { splitChain } from './tokenize';

describe('splitChain', () => {
  it('returns a single segment for a plain command', () => {
    expect(splitChain('buy 100$')).toEqual([{ command: 'buy 100$', separator: null }]);
  });

  it('splits on `;`', () => {
    expect(splitChain('buy 1; sell 2')).toEqual([
      { command: 'buy 1', separator: null },
      { command: 'sell 2', separator: ';' },
    ]);
  });

  it('splits on `&`', () => {
    expect(splitChain('buy 1 & sell 2')).toEqual([
      { command: 'buy 1', separator: null },
      { command: 'sell 2', separator: '&' },
    ]);
  });

  it('mixes `;` and `&` in one chain', () => {
    expect(splitChain('a; b & c')).toEqual([
      { command: 'a', separator: null },
      { command: 'b', separator: ';' },
      { command: 'c', separator: '&' },
    ]);
  });

  it('drops empty segments from leading/trailing/double separators', () => {
    expect(splitChain(';;buy 1;;')).toEqual([{ command: 'buy 1', separator: ';' }]);
  });

  it('returns [] for whitespace-only input', () => {
    expect(splitChain('   ')).toEqual([]);
  });

  it('does not split inside parens (repeat blocks)', () => {
    expect(splitChain('repeat 5 (buy 1; sell 1); buy 2')).toEqual([
      { command: 'repeat 5 (buy 1; sell 1)', separator: null },
      { command: 'buy 2', separator: ';' },
    ]);
  });

  it('handles nested parens', () => {
    expect(splitChain('repeat 2 (repeat 3 (buy 1; sell 1)); cancel')).toEqual([
      { command: 'repeat 2 (repeat 3 (buy 1; sell 1))', separator: null },
      { command: 'cancel', separator: ';' },
    ]);
  });

  it('does not split inside double-quoted strings', () => {
    expect(splitChain('alias x "buy 1; sell 1"; b')).toEqual([
      { command: 'alias x "buy 1; sell 1"', separator: null },
      { command: 'b', separator: ';' },
    ]);
  });

  it('does not split inside single-quoted strings', () => {
    expect(splitChain("alias x 'buy 1 & sell 1'")).toEqual([
      { command: "alias x 'buy 1 & sell 1'", separator: null },
    ]);
  });

  it('throws on unmatched (', () => {
    expect(() => splitChain('repeat 5 (buy 1; sell 1')).toThrow(/Unmatched/);
  });

  it('throws on unmatched )', () => {
    expect(() => splitChain('buy 1)')).toThrow(/Unmatched/);
  });

  it('throws on unterminated quote', () => {
    expect(() => splitChain('alias x "buy 1; sell 1')).toThrow(/Unterminated/);
  });
});
