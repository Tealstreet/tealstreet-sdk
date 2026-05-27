import { Market, StoreData, Ticker, Writable } from '../../types';
import { buildOrdersRiskMaps, calculateOrdersRisk, OrdersRiskMaps } from '../../utils/risk';
import { BasePostprocessor } from './BasePostprocessor';

/**
 * Adds ticker, market, balance, exchange information to positions to save some UI cycles.
 * Also calculates 5 min pnl.
 */
export class OrdersRiskProfilePostprocessor extends BasePostprocessor {
  // Cache the symbol-keyed lookup maps across postprocessor passes —
  // the store mutates ticker/market fields in place per WS frame but
  // only swaps the array reference on full snapshot replacement, so
  // when the source array reference is unchanged the maps are still
  // correct. Saves ~600+600 hashmap inserts per pass on Bybit linear.
  private _cachedMaps: OrdersRiskMaps | undefined;
  private _cachedTickerSource: Ticker[] | undefined;
  private _cachedMarketSource: Market[] | undefined;

  process(store: Writable<StoreData>): void {
    try {
      let maps = this._cachedMaps;
      if (!maps || this._cachedTickerSource !== store.tickers || this._cachedMarketSource !== store.markets) {
        maps = buildOrdersRiskMaps(store.markets, store.tickers);
        this._cachedMaps = maps;
        this._cachedTickerSource = store.tickers;
        this._cachedMarketSource = store.markets;
      }
      (store.ordersRisk as Writable<typeof store.ordersRisk>) = calculateOrdersRisk(
        store.markets,
        store.tickers,
        store.positions,
        store.orders,
        maps,
      );
    } catch (e) {
      console.trace();
      console.error(e);
      (store.ordersRisk as Writable<typeof store.ordersRisk>) = {};
    }
  }
}
