import { handleRoundingError } from '@tealstreet/math/src/handleRoundingError';
import { roundToDefaultPrecision } from '@tealstreet/math/src/roundToDefaultPrecision';
import { Market } from '../types';
import { adjust } from './safe-math';

export const convertFromQuoteSizeToNative = (quoteSize: number, market: Market, price: number): number => {
  const contractSize = market.contractSize || 1;
  // inverse are already sized in quote
  if (market.isInverse) {
    return adjust(quoteSize / contractSize, market.precision.amount);
  }
  const baseSize = quoteSize / price;
  return adjust(baseSize / contractSize, market.precision.amount);
};

export const convertFromBaseSizeToNative = (baseSize: number, market: Market, price: number): number => {
  const contractSize = market.contractSize || 1;
  // linear are already sized in base
  if (!market.isInverse) {
    return adjust(baseSize / contractSize, market.precision.amount);
  }
  const quoteSize = baseSize * price;
  return adjust(quoteSize / contractSize, market.precision.amount);
};

export const convertFromNativeSizeToQuote = (size: number, market: Market, price: number): number => {
  const contractSize = market.contractSize || 1;
  // inverse are already sized in quote
  if (market.isInverse) {
    return roundToDefaultPrecision(size * contractSize);
  }
  const baseSize = size * contractSize;
  return roundToDefaultPrecision(baseSize * price);
};

export const convertFromNativeSizeToBase = (size: number, market: Market, price: number): number => {
  const contractSize = market.contractSize || 1;
  // linear are already sized in base
  if (!market.isInverse) {
    return roundToDefaultPrecision(size * contractSize);
  }
  const quoteSize = size * contractSize;
  return roundToDefaultPrecision(quoteSize / price);
};

export const convertMarginToSizeCurrency = (
  margin: number,
  market: Market,
  price: number,
  btcPrice?: number,
): number => {
  // BITMEX support code
  if (market.info?.isQuanto) {
    if (!btcPrice) {
      console.error('btcPrice is required for quanto markets');
      return 0;
    }
    const marginInQuote = handleRoundingError(margin * btcPrice);
    return convertFromQuoteSizeToNative(marginInQuote, market, price);
  }
  // END BITMEX support code
  if (market.isInverse) {
    return convertFromBaseSizeToNative(margin, market, price);
  } else {
    return convertFromQuoteSizeToNative(margin, market, price);
  }
};

export const convertSizeToMarginCurrency = (size: number, market: Market, price: number, btcPrice?: number): number => {
  // BITMEX support code
  if (market.info?.isQuanto) {
    if (!btcPrice) {
      console.error('btcPrice is required for quanto markets');
      return 0;
    }
    const sizeInUsd = convertFromNativeSizeToQuote(size, market, price);
    return handleRoundingError(sizeInUsd / btcPrice);
  }
  // END BITMEX support code
  if (market.isInverse) {
    return convertFromNativeSizeToBase(size, market, price);
  } else {
    return convertFromNativeSizeToQuote(size, market, price);
  }
};

export const convertFromMarginToQuote = (margin: number, market: Market, price?: number, btcPrice?: number): number => {
  // BITMEX support code
  if (market.info?.isQuanto) {
    if (!btcPrice) {
      console.error('btcPrice is required for quanto markets');
      return 0;
    }
    const marginInQuote = handleRoundingError(margin * btcPrice);
    return marginInQuote;
  }
  // END BITMEX support code
  if (market.isInverse) {
    if (!price) {
      console.error('price is required for inverse markets');
      return 0;
    }
    return handleRoundingError(margin * price);
  } else {
    // linear already margined in quote
    return margin;
  }
};

export const getSizeCurrency = (market?: Market): string => {
  const contractSize = market?.contractSize || 1;
  const units = contractSize !== 1 ? 'Contracts' : market?.isInverse ? market?.quote : market?.base;
  return units || '';
};

export const getMarginCurrency = (market?: Market, fallback = ''): string => {
  // BITMEX support code
  if (market?.info?.isQuanto) {
    return 'BTC';
  }
  // END BITMEX support code
  if (market?.isSpot) {
    return '';
  }
  const units = market?.isInverse ? market?.base : market?.quote;
  return units || fallback;
};

export const getContractType = (market?: Market): 'Inverse' | 'Quanto' | 'Linear' => {
  return market?.isInverse ? 'Inverse' : market?.info?.isQuanto ? 'Quanto' : 'Linear';
};