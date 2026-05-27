import { StoreWorkerPayload } from '../StoreWorkerPayload';
import { TwapInstance } from '../twap/TwapInstance';
import { twapManager } from '../twap/TwapManager';
import { TwapOptions } from '../twap/TwapOptions';
import { TwapState } from '../twap/TwapState';
import { BaseHandler } from './BaseHandler';

export class TwapHandler extends BaseHandler {
  handle(payload: StoreWorkerPayload): void {
    switch (payload.op) {
      case 'twap:start':
        const options = payload.data;
        const instance = new TwapInstance(
          this._exchange,
          this._ctx,
          options,
          () => {
            console.log('[TWAP] twap state updated');
          },
          /* state= */ undefined,
        );
        twapManager.add(instance);
        instance.start();
        break;
      case 'twap:resume':
        twapManager.getById(payload.data)?.resume();
        break;
      case 'twap:cleanup':
        twapManager.getById(payload.data)?.cleanup();
        break;
      case 'twap:pause':
        twapManager.getById(payload.data)?.pause();
        break;
      case 'twap:stop':
        twapManager.getById(payload.data)?.terminate();
        break;
      case 'twap:state':
        try {
          const persistedTwapStates = payload.data as {
            id: string;
            options: TwapOptions;
            state: TwapState;
          }[];
          for (const value of persistedTwapStates) {
            const { id, options, state } = value;
            if (twapManager.runningTwaps.has(id)) continue;
            const instance = new TwapInstance(
              this._exchange,
              this._ctx,
              options,
              () => {
                console.log('[TWAP] twap state updated');
              },
              state,
              id,
            );
            twapManager.add(instance);
            if (state.status === 'running') {
              instance.pause();
              instance.resume();
            }
          }
        } catch (e) {
          console.error(e);
        }
        break;
    }
  }
}
