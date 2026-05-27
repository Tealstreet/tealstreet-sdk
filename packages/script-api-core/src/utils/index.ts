/**
 * Frozen api.utils.* bundle. See SPEC §2.1.
 *
 * Individual helpers are exported by name from this barrel so the
 * standalone CLI / web adapter / Phase 8 worker bootstrap can all
 * construct the same shape. The runtime object handed to scripts is
 * `Object.freeze(CORE_UTILS)`.
 */
import type { CoreUtils } from '../api/CoreApi.types';

import { adjust } from './adjust';
import { getBBOMid, getFarTouchPrice, getNearTouchPrice } from './bbo';
import { getCandleEndTime, getCandleStartTime, getNumCandlesInRange } from './candleHelpers';
import { calculateReturns } from './calculateReturns';
import { convertSymbol } from './convertSymbol';
import { formatPnl } from './formatPnl';
import { getMarginCurrency } from './getMarginCurrency';
import { normalizeEpochToSeconds } from './normalizeEpochToSeconds';
import { sleep } from './sleep';
import { getTickerPredictedBestAsk, getTickerPredictedBestBid } from './tickerPriceHelpers';
import { uuid } from './uuid';

export {
  adjust,
  calculateReturns,
  convertSymbol,
  formatPnl,
  getBBOMid,
  getCandleEndTime,
  getCandleStartTime,
  getFarTouchPrice,
  getMarginCurrency,
  getNearTouchPrice,
  getNumCandlesInRange,
  getTickerPredictedBestAsk,
  getTickerPredictedBestBid,
  normalizeEpochToSeconds,
  sleep,
  uuid,
};

// `calculateOrdersRisk` is intentionally NOT exposed on `CORE_UTILS` yet —
// the SPEC §2.1 shape (`RiskResult`) doesn't match the existing safe-cex
// helper (`OrderRiskProfile` per-order map), and shipping a stub that
// throws at call time on a 1.0.0 contract would lie to script authors.
// The implementation lands once the shape discrepancy is resolved.
export const CORE_UTILS: CoreUtils = Object.freeze({
  sleep,
  uuid,
  adjust,
  formatPnl,
  calculateReturns,
  getMarginCurrency,
  convertSymbol,
  getTickerPredictedBestBid,
  getTickerPredictedBestAsk,
  getBBOMid,
  getNearTouchPrice,
  getFarTouchPrice,
  getCandleStartTime,
  getCandleEndTime,
  getNumCandlesInRange,
  normalizeEpochToSeconds,
});
