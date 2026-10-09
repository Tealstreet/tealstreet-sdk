import { Timeframe, timeframeToSeconds } from '../types';
import { getCandleStartTime } from './getCandleStartTime';
import { normalizeEpochToSeconds } from './normalizeEpochToSeconds';

export const getNumCandlesInRange = (timeframe: Timeframe, from: number, to: number) => {
  if (from > to) {
    console.warn(`[getNumCandlesInRange] Invalid range: from (${from}) must be less than to (${to})`);
    return 0;
  }
  // easy out
  if (from === to) {
    return 1;
  }
  const intervalInSeconds = timeframeToSeconds[timeframe];
  if (!intervalInSeconds) {
    throw new Error(`Unsupported timeframe: ${timeframe}`);
  }
  const fromCandleStart = getCandleStartTime(timeframe, normalizeEpochToSeconds(from));
  const toCandleStart = getCandleStartTime(timeframe, normalizeEpochToSeconds(to));
  // toMs is inclusive
  return Math.floor((toCandleStart - fromCandleStart) / intervalInSeconds) + 1;
};
