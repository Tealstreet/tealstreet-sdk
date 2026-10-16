import type { Store } from '@tealstreet/safe-cex/src/store/store.interface';
import type { Market } from '@tealstreet/safe-cex/src/types';

export type HydratableMarketStore = Pick<Store, 'loaded' | 'update'>;

export const hydrateMarketStore = (store: HydratableMarketStore, markets: Market[]): void => {
  store.update({
    markets,
    loaded: {
      ...store.loaded,
      markets: true,
    },
  });
};
