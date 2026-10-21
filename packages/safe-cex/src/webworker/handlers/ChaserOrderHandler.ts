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
        let instance: ChaserOrderInstance;
        try {
          // Creation reads native account state; an engine that is not ready must
          // refuse the chase visibly rather than escape the worker message handler.
          instance = new ChaserOrderInstance(this._exchange, optionsWithAccount);
        } catch (error) {
          const reason = error instanceof Error ? error.message : String(error);
          this._exchange.emitter.emit('error', `Chase for ${options.symbol} was not started: ${reason}`);
          break;
        }
        chaserOrderManagerInstance.add(instance);
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
