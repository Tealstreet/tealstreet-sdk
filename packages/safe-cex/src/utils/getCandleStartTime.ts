import { SecondsTimeframe, Timeframe, timeframeToSeconds } from '../types';
import { normalizeEpochToSeconds } from './normalizeEpochToSeconds';

/**
 * Get correct candle for binning in seconds
 * @param timeframe
 * @param timeInSeconds
 */
export const getCandleStartTime = (timeframe: Timeframe | SecondsTimeframe, timeInSeconds: number) => {
  const epochInSeconds = normalizeEpochToSeconds(timeInSeconds);
  const seconds = timeframeToSeconds[timeframe];
  const res = Math.floor(epochInSeconds / seconds) * seconds;
  // passed in time in ms, give back time in ms
  if (timeInSeconds > 1_000_000_000_000) {
    return res * 1000;
  }
  return res;
};
