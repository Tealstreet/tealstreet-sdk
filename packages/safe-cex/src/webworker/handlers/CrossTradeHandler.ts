import { CrossTradeConfig } from '../../crossTrade/types';
import { crossTradeManagerInstance } from '../CrossTrade/CrossTradeManager';
import { StoreWorkerPayload } from '../StoreWorkerPayload';
import { BaseHandler } from './BaseHandler';

export class CrossTradeHandler extends BaseHandler {
  handle(payload: StoreWorkerPayload): void {
    switch (payload.op) {
      case 'crosstrade:start':
        crossTradeManagerInstance.start(payload.data as CrossTradeConfig[]);
        break;
      case 'crosstrade:stop':
        crossTradeManagerInstance.stop();
        break;
    }
  }
}
