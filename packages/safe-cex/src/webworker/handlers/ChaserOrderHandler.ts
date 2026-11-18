import { StoreWorkerPayload } from '../StoreWorkerPayload';
import { BaseHandler } from './BaseHandler';

const CHASER_OPS = new Set<StoreWorkerPayload['op']>(['chaser:start', 'chaser:terminate', 'chaser:terminateAll']);

/** Chases run in the account engine's Rust algos; `chaser:terminateAll` is a hosted
 * session parked for a resume, whose abandoned chases must not fill unguarded later. */
export class ChaserOrderHandler extends BaseHandler {
  handle(payload: StoreWorkerPayload): void {
    if (CHASER_OPS.has(payload.op)) this.runInRustAlgos(payload);
  }
}
