import memoize from 'lodash/memoize';

export const normalizeSymbol = memoize((str: string) => {
  const [, baseAsset, quoteAsset, ...broker] = str.split('_');
  return `${baseAsset}${quoteAsset}${broker.length ? `_${broker.join('_')}` : ''}`;
});

export const reverseSymbol = memoize((str: string) => {
  const index = str.lastIndexOf('USDC');
  return `PERP_${str.slice(0, index)}_USDC${str.slice(index + 4)}`;
});
