import { SecondsTimeframe, Timeframe, timeframeToSeconds } from '../types';
import { normalizeEpochToSeconds } from './normalizeEpochToSeconds';

/**
 * Get correct candle for binning in seconds
 * @param timeframe
 * @param timeInSeconds
 */
export const getCandleEndTime = (timeframe: Timeframe | SecondsTimeframe, timeInSeconds: number) => {
  const epochInSeconds = normalizeEpochToSeconds(timeInSeconds);
  const seconds = timeframeToSeconds[timeframe];
  
  // Check if the time is exactly at a candle boundary
  const isAtBoundary = epochInSeconds % seconds === 0;
  
  // If at boundary, we want the end of THIS candle, not the start
  const res = isAtBoundary 
    ? epochInSeconds + seconds 
    : Math.ceil(epochInSeconds / seconds) * seconds;
    
  // passed in time in ms, give back time in ms
  if (timeInSeconds > 1_000_000_000_000) {
    return res * 1000;
  }
  return res;
};
