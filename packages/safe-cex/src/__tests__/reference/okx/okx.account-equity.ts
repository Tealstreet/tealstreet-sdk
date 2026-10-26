import { roundUSD } from '../../../utils/roundUSD';

export type AccountEquity = {
  total: number;
  details: Array<{ ccy: string; eq: string }>;
};

const decimal = (value: unknown): number | null => {
  if (typeof value !== 'string' || !/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/.test(value)) return null;
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
};

/** Account details are deltas; validate the whole push before merging them. */
export const parseAccountEquity = (data: unknown): AccountEquity | null => {
  if (!Array.isArray(data) || !data.length) return null;
  let total = 0;
  const currencies = new Set<string>();
  const details: AccountEquity['details'] = [];
  for (const row of data) {
    if (!row || typeof row !== 'object') return null;
    const equity = decimal(row.totalEq);
    if (equity === null) return null;
    total += equity;
    if (row.details === undefined) continue;
    if (!Array.isArray(row.details)) return null;
    for (const detail of row.details) {
      if (
        !detail ||
        typeof detail.ccy !== 'string' ||
        !detail.ccy ||
        decimal(detail.eq) === null ||
        currencies.has(detail.ccy)
      )
        return null;
      currencies.add(detail.ccy);
      details.push({ ccy: detail.ccy, eq: detail.eq });
    }
  }
  return Number.isFinite(total) ? { total: roundUSD(total), details } : null;
};
