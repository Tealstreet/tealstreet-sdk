/**
 * Ticker-derived price helpers. Re-exports of safe-cex's vetted helpers
 * (used by the order-form, chase, and TWAP runtimes — same math as the live
 * UI). See SPEC §2.1 api.utils.
 */
export { getTickerPredictedBestAsk } from '@tealstreet/safe-cex/src/utils/getTickerPredictedBestAsk';
export { getTickerPredictedBestBid } from '@tealstreet/safe-cex/src/utils/getTickerPredictedBestBid';
