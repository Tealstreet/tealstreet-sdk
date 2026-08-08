import type { PlaceOrderOpts } from '../../types';

export const getPlaceOrderClientOrderId = (opts: PlaceOrderOpts): string | undefined => {
  return opts.clientOrderId;
};

export const assertBinanceNewClientOrderId = (clientOrderId: string, maxLength: number) => {
  const pattern = new RegExp(`^[.A-Z:/a-z0-9_-]{1,${maxLength}}$`);
  if (!pattern.test(clientOrderId)) {
    throw new Error(`Binance clientOrderId must match ^[.A-Z:/a-z0-9_-]{1,${maxLength}}$`);
  }
};
