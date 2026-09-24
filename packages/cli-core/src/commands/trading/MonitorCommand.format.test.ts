import { describe, expect, it } from 'vitest';

import { formatMarginTable } from './MonitorCommand';
import type { ExchangeLike } from '../../types';

// `formatPrice` and `formatPriceCompact` both PREFIX the currency symbol. Every
// call site that also wrote a literal `$` rendered `$$0.00`, which a user hit on
// the shipped 0.10.5 binary. This pins the symbol count, not the numbers.
const exchangeWith = (balance: { total: number; free: number; used: number; upnl: number }): ExchangeLike =>
  ({
    name: 'bybitdemo',
    store: { balance, positions: [] },
    emitter: { on: () => {}, off: () => {}, emit: () => {} },
  }) as unknown as ExchangeLike;

describe('margin table currency rendering', () => {
  it('never doubles the currency symbol', () => {
    const table = formatMarginTable(exchangeWith({ total: 0, free: 0, used: 0, upnl: 0 }));
    expect(table).not.toMatch(/\$\$/);
  });

  it('still renders one symbol per money value', () => {
    const table = formatMarginTable(exchangeWith({ total: 1234.5, free: 1000, used: 234.5, upnl: 12.25 }));
    expect(table).toMatch(/\$1,234\.50/);
    expect(table.match(/\$/g)?.length).toBe(5);
  });

  it('leaves the non-money rows without a symbol', () => {
    const table = formatMarginTable(exchangeWith({ total: 1000, free: 1000, used: 0, upnl: 0 }));
    const levRow = table.split('\n').find((line) => line.includes('Account Lev')) ?? '';
    const ratioRow = table.split('\n').find((line) => line.includes('Margin Ratio')) ?? '';
    expect(levRow).not.toContain('$');
    expect(ratioRow).not.toContain('$');
  });
});
