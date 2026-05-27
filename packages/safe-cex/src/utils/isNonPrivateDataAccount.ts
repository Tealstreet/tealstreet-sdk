import { ExchangeName, ExchangeOptions } from '../types';

export const isNonPrivateDataAccount = (exchangeName?: ExchangeName, options?: ExchangeOptions): boolean => {
  // hyperliquid does not have key or secret in traditional sense, so we assume it's always private acc
  return (
    (options?.publicDataOnly || options?.key === '' || options?.secret === '') &&
    exchangeName !== 'hyperliquid' &&
    exchangeName !== 'hyperliquidspot'
  );
};
