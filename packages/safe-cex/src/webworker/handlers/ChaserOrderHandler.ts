import { ChaserOrderInstance } from '../chaserOrder/ChaserOrderInstance';
import { chaserOrderManagerInstance } from '../chaserOrder/ChaserOrderManager';
import { ChaserOrderOptions } from '../chaserOrder/ChaserOrderOptions';
import { StoreWorkerPayload } from '../StoreWorkerPayload';
import { BaseHandler } from './BaseHandler';

export class ChaserOrderHandler extends BaseHandler {
  handle(payload: StoreWorkerPayload): void {
    switch (payload.op) {
      case 'chaser:start':
        const options = payload.data as ChaserOrderOptions;
        // Ensure accountName is set for filtering in dispose logic
        const optionsWithAccount = {
          ...options,
          accountName: options.accountName || this._accountName,
        };
        chaserOrderManagerInstance.add(new ChaserOrderInstance(this._exchange, optionsWithAccount));
        break;
      case 'chaser:terminate':
        const id = payload.data;
        chaserOrderManagerInstance.remove(id);
        break;
      // A hosted session parked for a resume: the client has already abandoned
      // these chases' pending TP/SL, so they must not fill unguarded later.
      case 'chaser:terminateAll':
        chaserOrderManagerInstance.cleanupExchange(this._exchange, 'Client disconnected while chase was active');
        break;
    }
  }
}
