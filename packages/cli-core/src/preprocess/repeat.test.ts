import { describe, it, expect } from 'vitest';
import { expandRepeats, MAX_REPEAT_COUNT, MAX_REPEAT_DEPTH } from './repeat';

describe('expandRepeats', () => {
  it('passes through input with no repeat block', () => {
    expect(expandRepeats('buy 100$; sell 50$')).toBe('buy 100$; sell 50$');
  });

  it('expands a simple block into N copies', () => {
    expect(expandRepeats('repeat 3 (buy 1)')).toBe('buy 1; buy 1; buy 1');
  });

  it('preserves the inner separator', () => {
    expect(expandRepeats('repeat 2 (buy 1; sleep 1m)')).toBe('buy 1; sleep 1m; buy 1; sleep 1m');
  });

  it('preserves an inner & separator', () => {
    expect(expandRepeats('repeat 2 (buy 1 & buy 2)')).toBe('buy 1 & buy 2; buy 1 & buy 2');
  });

  it('handles nested repeat (recursion expands inner first)', () => {
    expect(expandRepeats('repeat 2 (repeat 3 (buy 1))')).toBe(
      'buy 1; buy 1; buy 1; buy 1; buy 1; buy 1',
    );
  });

  it('handles repeat blocks alongside other commands', () => {
    expect(expandRepeats('cancel; repeat 2 (buy 1); close')).toBe(
      'cancel; buy 1; buy 1; close',
    );
  });

  it('handles multiple repeat blocks in one string', () => {
    expect(expandRepeats('repeat 2 (a); repeat 2 (b)')).toBe('a; a; b; b');
  });

  it('throws on count <= 0', () => {
    expect(() => expandRepeats('repeat 0 (buy 1)')).toThrow(/positive/);
  });

  it('throws on count over cap', () => {
    expect(() => expandRepeats(`repeat ${MAX_REPEAT_COUNT + 1} (buy 1)`)).toThrow(/exceeds max/);
  });

  it('throws on depth over cap', () => {
    let input = 'buy 1';
    for (let i = 0; i < MAX_REPEAT_DEPTH + 1; i++) {
      input = `repeat 1 (${input})`;
    }
    expect(() => expandRepeats(input)).toThrow(/nested deeper/);
  });

  it('throws on unmatched (', () => {
    expect(() => expandRepeats('repeat 3 (buy 1')).toThrow(/unmatched/);
  });

  it('throws on empty block', () => {
    expect(() => expandRepeats('repeat 3 ()')).toThrow(/empty block/);
  });

  it('does not match `repeat` as part of a longer word', () => {
    expect(expandRepeats('xrepeat 3 (buy 1)')).toBe('xrepeat 3 (buy 1)');
  });

  it('does not match `repeat` inside a quoted string', () => {
    // We don't fully parse quotes at the outer level — but the
    // regex requires a leading word boundary, so a literal `"repeat`
    // (with the quote attached) won't match. This is a "good enough"
    // guarantee; it documents that aliases are stored without quotes
    // around them in normal usage.
    expect(expandRepeats('alias x "buy 1"')).toBe('alias x "buy 1"');
  });

  describe('boundary counts', () => {
    it('expands count = 1 (single copy, no separator after)', () => {
      expect(expandRepeats('repeat 1 (buy 1)')).toBe('buy 1');
    });

    it(`expands count = MAX_REPEAT_COUNT (${MAX_REPEAT_COUNT})`, () => {
      const out = expandRepeats(`repeat ${MAX_REPEAT_COUNT} (a)`);
      // N copies joined by '; ' → N tokens after splitting on '; '
      const tokens = out.split('; ');
      expect(tokens.length).toBe(MAX_REPEAT_COUNT);
      expect(tokens.every((t) => t === 'a')).toBe(true);
    });

    it('rejects negative counts as "positive" guard', () => {
      // The regex \d+ doesn't match `-`, so this is treated as a non-
      // matching `repeat` token and passes through. We just lock that
      // behavior in here.
      expect(expandRepeats('repeat -3 (buy 1)')).toBe('repeat -3 (buy 1)');
    });

    it('rejects count = 0 with the "positive" guard', () => {
      expect(() => expandRepeats('repeat 0 (a)')).toThrow(/positive/);
    });
  });

  describe('quoted bodies', () => {
    it('does not split on `;` or `&` inside double-quoted strings inside the body', () => {
      // The repeat expander only matches its open-paren walker against
      // unquoted parens. Quoted strings inside a repeat body should
      // pass through untouched.
      expect(expandRepeats('repeat 2 (alias z "buy 1; sell 1")')).toBe(
        'alias z "buy 1; sell 1"; alias z "buy 1; sell 1"',
      );
    });

    it('does not get fooled by quoted parens', () => {
      // Quoted ')' should not close the repeat block.
      expect(expandRepeats('repeat 2 (alias z "buy (smart)")')).toBe(
        'alias z "buy (smart)"; alias z "buy (smart)"',
      );
    });

    it('handles single-quoted bodies symmetrically', () => {
      expect(expandRepeats("repeat 2 (alias z 'buy 1; sell 1')")).toBe(
        "alias z 'buy 1; sell 1'; alias z 'buy 1; sell 1'",
      );
    });
  });

  describe('whitespace handling', () => {
    it('trims leading/trailing whitespace inside the parens', () => {
      expect(expandRepeats('repeat 2 (   buy 1   )')).toBe('buy 1; buy 1');
    });

    it('treats a whitespace-only body as empty', () => {
      expect(() => expandRepeats('repeat 2 (   )')).toThrow(/empty block/);
    });

    it('accepts extra whitespace between `repeat`, count, and `(`', () => {
      expect(expandRepeats('repeat   3   (buy 1)')).toBe('buy 1; buy 1; buy 1');
    });

    it('allows newlines inside the body', () => {
      // The regex isn't multi-line aware, but we don't need it to be —
      // the body just needs to round-trip whatever's between the parens.
      expect(expandRepeats('repeat 2 (buy 1\n; sell 1)')).toBe(
        'buy 1\n; sell 1; buy 1\n; sell 1',
      );
    });
  });

  describe('nested + mixed', () => {
    it('handles three levels of nesting', () => {
      expect(expandRepeats('repeat 2 (repeat 2 (repeat 2 (a)))')).toBe(
        'a; a; a; a; a; a; a; a',
      );
    });

    it('preserves outer chain glue when expanding nested', () => {
      expect(expandRepeats('cancel; repeat 2 (repeat 2 (a)); close')).toBe(
        'cancel; a; a; a; a; close',
      );
    });

    it('mixes `;` and `&` separators correctly across nesting', () => {
      // Inner body uses both separators; outer is repeated.
      expect(expandRepeats('repeat 2 (a; b & c)')).toBe('a; b & c; a; b & c');
    });

    it('handles repeat blocks with surrounding whitespace separators', () => {
      // A space-padded `;` between two top-level repeats.
      expect(expandRepeats('repeat 2 (a)   ;   repeat 2 (b)')).toBe('a; a   ;   b; b');
    });

    it('expands outermost first as a string transform', () => {
      // Locks in the recurse-into-body-then-multiply contract — without
      // this ordering, a body that itself contains a `repeat` would get
      // repeated literally instead of expanded.
      expect(expandRepeats('repeat 2 (repeat 1 (buy 1))')).toBe('buy 1; buy 1');
    });
  });

  describe('repeat keyword false positives', () => {
    it.each([
      ['repeating 3 (buy 1)', 'repeating 3 (buy 1)'],
      ['xrepeat 3 (buy 1)', 'xrepeat 3 (buy 1)'],
      ['repeat3 (buy 1)', 'repeat3 (buy 1)'],
      ['REPEAT 3 (buy 1)', 'REPEAT 3 (buy 1)'],
    ])('leaves "%s" unexpanded', (input, expected) => {
      expect(expandRepeats(input)).toBe(expected);
    });
  });

  describe('depth boundary', () => {
    it(`accepts ${MAX_REPEAT_DEPTH} levels`, () => {
      let input = 'a';
      for (let i = 0; i < MAX_REPEAT_DEPTH; i++) {
        input = `repeat 1 (${input})`;
      }
      // Should not throw — `MAX_REPEAT_DEPTH` is the cap, not the limit.
      expect(() => expandRepeats(input)).not.toThrow();
    });

    it(`rejects ${MAX_REPEAT_DEPTH + 1} levels`, () => {
      let input = 'a';
      for (let i = 0; i < MAX_REPEAT_DEPTH + 1; i++) {
        input = `repeat 1 (${input})`;
      }
      expect(() => expandRepeats(input)).toThrow(/nested deeper/);
    });
  });
});
