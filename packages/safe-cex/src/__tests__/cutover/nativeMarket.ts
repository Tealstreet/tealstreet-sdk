import type { Market } from '../../generated/Market';

/** Synthetic native catalogue facts for tests focused on startup or one market field. */
export function nativeMarket(
  row: Pick<Market, 'symbol'> &
    Partial<Omit<Market, 'precision' | 'limits'>> & {
      precision?: Partial<Market['precision']>;
      limits?: Partial<Omit<Market['limits'], 'amount' | 'leverage'>> & {
        amount?: Partial<Market['limits']['amount']>;
        leverage?: Partial<Market['limits']['leverage']>;
      };
    },
): Market {
  return {
    id: row.symbol,
    base: 'BASE',
    quote: 'QUOTE',
    active: true,
    ...row,
    precision: { amount: 0.001, price: 0.1, ...row.precision },
    limits: {
      ...row.limits,
      amount: { min: 0.001, max: 1000, ...row.limits?.amount },
      leverage: { min: 1, max: 100, ...row.limits?.leverage },
    },
  };
}
