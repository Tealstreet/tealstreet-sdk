import { getPrecisionFromSigDigits } from '@tealstreet/math/src/getPrecisionFromSigDigits';

export const BTC_SIG_DIGITS = 6;
export const USD_SIG_DIGITS = 2;
export const DEFAULT_SIG_DIGITS = 4;

export const DEFAULT_AMOUNT_PRECISION = 0.00000001;
export const DEFAULT_PRICE_PRECISION = 0.01;

export const isUsdLikeCurrency = (currency: string | undefined) => {
  return (
    currency === 'USD' ||
    currency === 'USDT' ||
    currency === 'BUSD' ||
    currency === 'USDC' ||
    currency === 'FDUSD' ||
    currency === 'USDY' ||
    currency === 'USDD' ||
    currency === 'USDP'
  );
};

export const isBtcLikeCurrency = (currency: string | undefined) => {
  return currency === 'BTC' || currency === 'XBT';
};

export const isEquivalentCurrency = (currency1: string, currency2: string) => {
  return (
    currency1 === currency2 ||
    (isUsdLikeCurrency(currency1) && isUsdLikeCurrency(currency2)) ||
    (isBtcLikeCurrency(currency1) && isBtcLikeCurrency(currency2))
  );
};

export const isBtcLikeSymbol = (symbol: string) => {
  return symbol.startsWith('BTC') || symbol.startsWith('XBT');
};

export const getCurrencySigDigits = (currency: string): number => {
  currency = currency && currency.toUpperCase();
  if (isBtcLikeCurrency(currency)) {
    return BTC_SIG_DIGITS;
  } else if (isUsdLikeCurrency(currency)) {
    return USD_SIG_DIGITS;
  } else {
    return DEFAULT_SIG_DIGITS;
  }
};

export const getCurrencyPrecision = (currency: string): number => {
  return getPrecisionFromSigDigits(getCurrencySigDigits(currency));
};
