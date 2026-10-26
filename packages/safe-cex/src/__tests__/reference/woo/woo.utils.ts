import memoize from 'lodash/memoize';

export const normalizeSymbol = memoize((str: string) => {
  if (!str) return str;

  const normalized = str.toUpperCase().split(':').pop() || str.toUpperCase();

  if (normalized.includes('_')) {
    const parts = normalized.split('_');
    if (parts.length >= 3 && (parts[0] === 'PERP' || parts[0] === 'SPOT')) {
      return `${parts[1]}${parts[2]}`;
    }
    if (parts.length >= 3 && parts[parts.length - 1] === 'PERP') {
      return `${parts[0]}${parts[1]}`;
    }
  }

  if (normalized.includes('-')) {
    const parts = normalized.split('-');
    if (parts.length >= 3 && (parts[0] === 'PERP' || parts[0] === 'SPOT')) {
      return `${parts[1]}${parts[2]}`;
    }
    if (parts.length >= 3 && parts[parts.length - 1] === 'PERP') {
      return `${parts[0]}${parts[1]}`;
    }
  }

  if (normalized.endsWith('PERP') && !normalized.includes('_') && !normalized.includes('-')) {
    const base = normalized.slice(0, -4);
    return `${base}USDT`;
  }

  return normalized.replace(/[_\-./:]/g, '');
});

export const reverseSymbol = memoize((str: string) => {
  const normalized = (str || '').toUpperCase().split(':').pop() || (str || '').toUpperCase();
  const withoutPerp = normalized
    .replace(/\.P$/, '')
    .replace(/_PERP$/, '')
    .replace(/-PERP$/, '')
    .replace(/PERP$/, '');
  const asset = withoutPerp.replace(/USDT$/, '').replace(/[^A-Z0-9]/g, '');
  return `PERP_${asset}_USDT`;
});

/**
 * `/v3/trade/transactionHistory` reports `side` in two vocabularies: BUY/SELL
 * in one-way mode, and OPEN|CLOSE LONG|SHORT in hedge mode. The hedge strings
 * are the only place that endpoint carries the position side — it has no
 * `positionSide` field. Returns undefined for one-way/unknown input so the
 * caller can fall back to its BUY/SELL mapping.
 */
export const mapWooHedgeTransactionSide = (
  rawSide: string,
): { side: 'buy' | 'sell'; positionSide: 'long' | 'short' } | undefined => {
  const normalized = `${rawSide || ''}`
    .toUpperCase()
    .replace(/[_\s]+/g, ' ')
    .trim();

  switch (normalized) {
    case 'OPEN LONG':
      return { side: 'buy', positionSide: 'long' };
    case 'CLOSE LONG':
      return { side: 'sell', positionSide: 'long' };
    case 'OPEN SHORT':
      return { side: 'sell', positionSide: 'short' };
    case 'CLOSE SHORT':
      return { side: 'buy', positionSide: 'short' };
    default:
      return undefined;
  }
};
