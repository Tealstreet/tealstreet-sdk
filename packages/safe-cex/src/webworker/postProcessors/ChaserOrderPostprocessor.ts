import { ActiveChaser, StoreData, Writable } from '../../types';
import { chaserOrderManagerInstance } from '../chaserOrder/ChaserOrderManager';
import { ExchangeDependentPostprocessor } from './ExchangeDependentPostprocessor';

export class ChaserOrderPostprocessor extends ExchangeDependentPostprocessor {
  process(store: Writable<StoreData>): void {
    const instances = chaserOrderManagerInstance.instances;
    const activeChasers: Writable<ActiveChaser[]> = [];

    for (let i = 0; i < instances.length; i++) {
      const instance = instances[i];
      if (instance.exchange !== this._exchange) continue;
      if (instance.state.isStopped) continue;

      activeChasers.push({
        id: instance.id,
        symbol: instance.options.symbol,
        size: instance.options.size,
        side: instance.options.side,
        maxAttempts: instance.options.maxAttempts,
        reduceOnly: instance.options.reduceOnly,
        attempts: instance.state.attempts,
        isStopped: instance.state.isStopped,
        stopReason: instance.state.stopReason,
        currentOrderIds: instance.currentOrderIds,
        origin: instance.options.origin,
      });
    }

    store.activeChasers = activeChasers;
  }
}
