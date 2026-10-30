import { StoreData, Writable } from '../../types';
import { twapManager } from '../twap/TwapManager';
import { TwapStatus } from '../twap/TwapStatus';
import { ExchangeDependentPostprocessor } from './ExchangeDependentPostprocessor';

export class TwapPostprocessor extends ExchangeDependentPostprocessor {
  process(store: Writable<StoreData>): void {
    // A Rust-algo account's engine publishes its own `activeTWAPs` rows into the store.
    if ((this._exchange as { runsRustAlgos?: boolean }).runsRustAlgos === true) return;
    const activeTWAPs: Writable<typeof store.activeTWAPs> = [];

    for (const twapInstance of twapManager.runningTwaps.values()) {
      if (
        !twapInstance.options.activeMarket ||
        twapInstance.options.accountName !== this._accountName ||
        twapInstance.state.status === TwapStatus.Completed
      ) {
        continue;
      }

      activeTWAPs.push({
        id: twapInstance.id,
        ...twapInstance.options,
        ...twapInstance.state,
        currentOrderIds: twapInstance.currentOrderIds,
        size: twapInstance.options.side === 'sell' ? twapInstance.options.size * -1 : twapInstance.options.size,
        symbol: twapInstance.options.activeMarket?.symbol || 'Unknown',
      } as (typeof activeTWAPs)[number]);
    }

    store.activeTWAPs = activeTWAPs;
  }
}
