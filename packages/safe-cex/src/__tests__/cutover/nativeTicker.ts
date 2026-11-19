import type { Ticker } from '../../generated/Ticker';

/** Explicit synthetic native ticker facts for focused protocol tests. */
export function nativeTicker(row: Pick<Ticker, 'id' | 'symbol'> & Partial<Ticker>): Ticker {
  return {
    bid: 0,
    ask: 0,
    last: 0,
    mark: 0,
    index: 0,
    percentage: 0,
    openInterest: 0,
    fundingRate: 0,
    fundingTime: 0,
    volume: 0,
    quoteVolume: 0,
    ...row,
  };
}
