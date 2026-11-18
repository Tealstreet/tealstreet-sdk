import type { SettingTable, Status } from '@tealstreet/safe-cex-wasm/wire';
import type { FetchedLeverage } from '../generated/FetchedLeverage';
import type { StoreDataLoaded } from '../generated/StoreDataLoaded';
import type { StoreOptions } from '../generated/StoreOptions';

import { LOADED_FLAG } from '@tealstreet/safe-cex-wasm/wire';
import { MarginType, PositionMode } from '../types';

const margins = { cross: MarginType.Cross, isolated: MarginType.Isolated };

export function readStoreStatus(status: Status): {
  loaded: StoreDataLoaded;
  options: StoreOptions;
  latency: number | null;
} {
  const loaded = status.loaded;
  return {
    loaded: {
      balance: (loaded & LOADED_FLAG.BALANCE) !== 0,
      orders: (loaded & LOADED_FLAG.ORDERS) !== 0,
      markets: (loaded & LOADED_FLAG.MARKETS) !== 0,
      tickers: (loaded & LOADED_FLAG.TICKERS) !== 0,
      positions: (loaded & LOADED_FLAG.POSITIONS) !== 0,
    },
    options: { isHedged: status.hedged },
    latency: status.latency,
  };
}

export function readStoreSettings(
  settings: SettingTable,
  hedged: boolean,
  symbol: (market: number, fallback?: string) => string,
): Record<string, FetchedLeverage> {
  const buy = settings.buyLeverage;
  const sell = settings.sellLeverage;
  if (!buy || !sell) throw new Error('Native Store leverage rails are required');
  const result: Record<string, FetchedLeverage> = {};
  for (let i = 0; i < settings.count; i++) {
    if (!Number.isFinite(buy[i]) || !Number.isFinite(sell[i])) throw new Error('Invalid native Store leverage rails');
    const margin = settings.marginType(i);
    result[symbol(settings.market[i], settings.symbol?.(i))] = {
      buyLeverage: buy[i],
      sellLeverage: sell[i],
      ...(margin !== undefined && { marginType: margins[margin] }),
      positionMode: hedged ? PositionMode.Hedged : PositionMode.OneWay,
    };
  }
  return result;
}
