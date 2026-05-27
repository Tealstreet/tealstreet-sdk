import { Market, Ticker } from '../types';

export const getTickerPredictedBestBid = (ticker: Ticker, market?: Market | undefined | null) => {
  if (ticker.bid) {
    return ticker.bid;
  }

  if (ticker.last && market?.precision?.price) {
    return Math.max(ticker.last - market.precision.price, 0);
  }

  return ticker.last;
};
