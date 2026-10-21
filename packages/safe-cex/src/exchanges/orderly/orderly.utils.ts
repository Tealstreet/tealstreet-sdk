import memoize from 'lodash/memoize';

export const normalizeSymbol = memoize((str: string) => {
  const [, baseAsset, quoteAsset] = str.split('_');
  return `${baseAsset}${quoteAsset}`;
});

export const reverseSymbol = memoize((str: string) => {
  const asset = str.replace('USDC', '');
  return `PERP_${asset}_USDC`;
});
