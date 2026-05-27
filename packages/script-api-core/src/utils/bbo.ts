/**
 * Ticker-derived BBO + near/far touch prices. Pure helpers — no orderbook
 * lookup. The web/cli adapters can layer live-OB freshness on top in later
 * phases if needed (see SPEC §2.1).
 *
 * Logic mirrors apps/web/src/utils/pricing/* but strips the
 * `getOrderBook(exchangeName, symbol)` short-circuit, since that pulls from
 * a web-only singleton (`apps/web/src/utils/market/listenOrderBook`) and
 * would make this util web-bound.
 */
import { OrderSide, type Market, type Ticker } from '@tealstreet/safe-cex/src/types';
import { getTickerPredictedBestAsk } from '@tealstreet/safe-cex/src/utils/getTickerPredictedBestAsk';
import { getTickerPredictedBestBid } from '@tealstreet/safe-cex/src/utils/getTickerPredictedBestBid';
import { handleRoundingError } from '@tealstreet/safe-cex/src/utils/safe-math';

export const getNearTouchPrice = (side: OrderSide, ticker: Ticker, market?: Market | null): number => {
  if (side === OrderSide.Buy) return getTickerPredictedBestBid(ticker, market);
  if (side === OrderSide.Sell) return getTickerPredictedBestAsk(ticker, market);
  return ticker.last || 0;
};

export const getFarTouchPrice = (side: OrderSide, ticker: Ticker, market?: Market | null): number => {
  if (side === OrderSide.Buy) return getTickerPredictedBestAsk(ticker, market);
  if (side === OrderSide.Sell) return getTickerPredictedBestBid(ticker, market);
  return ticker.last || 0;
};

export const getBBOMid = (ticker: Ticker, market?: Market | null): number => {
  const bid = getNearTouchPrice(OrderSide.Buy, ticker, market);
  const ask = getNearTouchPrice(OrderSide.Sell, ticker, market);
  if (bid && ask) return handleRoundingError((bid + ask) / 2);

  const singleSided = bid || ask;
  if (singleSided) return handleRoundingError(singleSided);

  const reference = ticker.last || ticker.mark || ticker.index || 0;
  return reference > 0 ? handleRoundingError(reference) : 0;
};
