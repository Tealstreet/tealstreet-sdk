/**
 * Timeframe-aware candle math. Re-exports of safe-cex's vetted helpers
 * (the candle-cache, chart pipeline, and TradingView datafeed all sit on
 * top of these — single source of truth). See SPEC §2.1 api.utils.
 */
export { getCandleStartTime } from '@tealstreet/safe-cex/src/utils/getCandleStartTime';
export { getCandleEndTime } from '@tealstreet/safe-cex/src/utils/getCandleEndTime';
export { getNumCandlesInRange } from '@tealstreet/safe-cex/src/utils/getNumCandlesInRange';
