import { describe, expect, it } from 'vitest';

import { formatFooterLine, visibleLength } from './footer';

const ESC = '\x1b';
const red = (s: string): string => `${ESC}[31m${s}${ESC}[0m`;
const strip = (s: string): string => s.replace(/\x1b\[[0-9;]*m/g, '');

describe('visibleLength', () => {
  it('ignores ANSI SGR colour runs', () => {
    expect(visibleLength('abc')).toBe(3);
    expect(visibleLength(red('abc'))).toBe(3);
    expect(visibleLength(`${red('ab')} ${red('cd')}`)).toBe(5);
  });
});

describe('formatFooterLine', () => {
  it('left group only — returns it unchanged when it fits (row is pre-cleared)', () => {
    expect(formatFooterLine('● bybit', '', 40)).toBe('● bybit');
  });

  it('right-aligns the right group with a space-filled gap', () => {
    const out = formatFooterLine('left', 'right', 20);
    expect(out).toBe('left' + ' '.repeat(20 - 4 - 5) + 'right');
    expect(out.length).toBe(20);
    expect(out.startsWith('left')).toBe(true);
    expect(out.endsWith('right')).toBe(true);
  });

  it('measures visible width so colour codes do not break alignment', () => {
    const out = formatFooterLine(red('left'), red('right'), 20);
    expect(strip(out).length).toBe(20);
    expect(strip(out)).toBe('left' + ' '.repeat(11) + 'right');
  });

  it('clamps to width with an ellipsis on overflow (never wraps)', () => {
    const out = formatFooterLine('x'.repeat(50), 'tail', 20);
    expect(visibleLength(out)).toBe(20);
    expect(out.endsWith('…')).toBe(true);
  });

  it('drops colour on overflow rather than miscounting a coloured run', () => {
    const out = formatFooterLine(red('x'.repeat(50)), red('tail'), 20);
    expect(visibleLength(out)).toBe(20);
    expect(out).not.toContain(ESC);
  });

  it('is stable at degenerate widths', () => {
    expect(formatFooterLine('anything', 'x', 0)).toBe('');
    expect(visibleLength(formatFooterLine('anything', 'x', 1))).toBe(1);
  });
});
