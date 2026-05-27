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
