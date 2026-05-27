import { Candle, Timeframe } from '../../types';
import { getCandleStartTime } from '../../utils/getCandleStartTime';

const BITMEX_DERIVED_INTERVAL_SOURCES: Partial<Record<Timeframe, { sourceInterval: Timeframe; sourcePerTarget: number }>> =
  {
    '3m': { sourceInterval: '1m', sourcePerTarget: 3 },
    '15m': { sourceInterval: '5m', sourcePerTarget: 3 },
    '30m': { sourceInterval: '5m', sourcePerTarget: 6 },
    '2h': { sourceInterval: '1h', sourcePerTarget: 2 },
    '4h': { sourceInterval: '1h', sourcePerTarget: 4 },
    '6h': { sourceInterval: '1h', sourcePerTarget: 6 },
    '12h': { sourceInterval: '1h', sourcePerTarget: 12 },
    '1w': { sourceInterval: '1d', sourcePerTarget: 7 },
  };

const BITMEX_NATIVE_INTERVALS = new Set<Timeframe>(['1m', '5m', '1h', '1d']);

export type BitmexOHLCVPlan = {
  sourceInterval: Timeframe;
  sourcePerTarget: number;
  isNative: boolean;
};

export const getBitmexOHLCVPlan = (interval: Timeframe): BitmexOHLCVPlan | null => {
  if (BITMEX_NATIVE_INTERVALS.has(interval)) {
    return {
      sourceInterval: interval,
      sourcePerTarget: 1,
      isNative: true,
    };
  }

  const derived = BITMEX_DERIVED_INTERVAL_SOURCES[interval];
  if (!derived) {
    return null;
  }

  return {
    ...derived,
    isNative: false,
  };
};

export const aggregateBitmexCandles = (candles: Candle[], targetInterval: Timeframe): Candle[] => {
  if (!candles.length) return [];

  const sorted = [...candles].sort((a, b) => a.timestamp - b.timestamp);
  const aggregated: Candle[] = [];

  for (const candle of sorted) {
    const bucketTimestamp = getCandleStartTime(targetInterval, candle.timestamp);
    const last = aggregated[aggregated.length - 1];

    if (!last || last.timestamp !== bucketTimestamp) {
      aggregated.push({
        timestamp: bucketTimestamp,
        open: candle.open,
        high: candle.high,
        low: candle.low,
        close: candle.close,
        volume: candle.volume,
      });
      continue;
    }

    aggregated[aggregated.length - 1] = {
      ...last,
      high: Math.max(last.high, candle.high),
      low: Math.min(last.low, candle.low),
      close: candle.close,
      volume: last.volume + candle.volume,
    };
  }

  return aggregated;
};

