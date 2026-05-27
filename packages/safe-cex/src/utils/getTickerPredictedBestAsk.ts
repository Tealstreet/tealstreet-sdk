import { Market, Ticker } from '../types';

export const getTickerPredictedBestAsk = (ticker: Ticker, market?: Market | undefined | null) => {
  if (ticker.ask) {
    return ticker.ask;
  }

  if (ticker.last && market?.precision?.price) {
    return ticker.last + market.precision.price;
  }

  return ticker.last;
};
