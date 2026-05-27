import { describe, expect, it } from 'vitest';

import { formatStatusContent } from './status-line';

// Strip ANSI so assertions don't depend on chalk's color level in the test env.
const strip = (s: string): string => s.replace(/\x1b\[[0-9;]*m/g, '');

describe('formatStatusContent', () => {
  it('prefixes a category icon (warn ⚠, error ✗, critical ⛔)', () => {
    expect(strip(formatStatusContent('reconnecting', 'warn', 80, false)).trimEnd()).toBe('⚠ reconnecting');
    expect(strip(formatStatusContent('order rejected', 'error', 80, false)).trimEnd()).toBe('✗ order rejected');
    expect(strip(formatStatusContent('liquidated', 'critical', 80, false)).trimEnd()).toBe('⛔ liquidated');
  });

  it('pads to the full terminal width so the reversed bar spans the row', () => {
    expect(strip(formatStatusContent('hi', 'warn', 40, false)).length).toBe(40);
    expect(strip(formatStatusContent('hi', 'error', 100, true)).length).toBe(100);
  });

  it('collapses internal whitespace/newlines to single spaces', () => {
    expect(strip(formatStatusContent('a\n  b\t c', 'warn', 80, false)).trimEnd()).toBe('⚠ a b c');
  });

  it('truncates to the terminal width with an ellipsis', () => {
    const long = 'x'.repeat(200);
    const out = strip(formatStatusContent(long, 'warn', 20, false));
    expect(out.length).toBe(20);
    expect(out.startsWith('⚠ ')).toBe(true);
    expect(out.endsWith('…')).toBe(true);
  });

  it('does not truncate when it already fits', () => {
    expect(strip(formatStatusContent('short', 'error', 80, false)).trimEnd()).toBe('✗ short');
  });

  it('hard-clamps to width even when width is smaller than the prefix (never wraps)', () => {
    // A bar longer than one row would wrap into the gap/scroll-region rows and
    // corrupt the layout — the output must never exceed `width`.
    expect(() => formatStatusContent('anything', 'warn', 1, false)).not.toThrow();
    expect(strip(formatStatusContent('anything', 'warn', 1, false)).length).toBe(1);
    expect(strip(formatStatusContent('anything', 'warn', 0, false)).length).toBe(0);
    expect(strip(formatStatusContent('anything', 'critical', 3, false)).length).toBe(3);
  });

  it('is stable on empty/whitespace-only text', () => {
    expect(strip(formatStatusContent('   ', 'warn', 80, false)).trimEnd()).toBe('⚠');
  });
});
