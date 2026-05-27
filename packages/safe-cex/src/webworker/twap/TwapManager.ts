import type { TwapInstance } from './TwapInstance';
import type { TwapOptions } from './TwapOptions';
import type { TwapState } from './TwapState';

import { TwapStatus } from './TwapStatus';

type SerializedState = {
  [id: string]: {
    options: TwapOptions;
    state: TwapState;
  };
};

class TwapManager {
  runningTwaps: Map<string, TwapInstance> = new Map();

  hasId(id: string) {
    return this.runningTwaps.has(id);
  }

  add(twapInstance: TwapInstance) {
    this.runningTwaps.set(twapInstance.id, twapInstance);
    return twapInstance.id;
  }

  remove(id: string) {
    this.runningTwaps.delete(id);
  }

  getById(id: string) {
    return this.runningTwaps.get(id);
  }

  // for saving into SessionStorage
  serializeState(): SerializedState {
    const result: SerializedState = {};
    for (const twap of this.runningTwaps.values()) {
      if (twap.state.status !== TwapStatus.Completed) {
        result[twap.id] = {
          options: twap.options,
          state: twap.state,
        };
      }
    }
    return result;
  }
}

export const twapManager = new TwapManager();
