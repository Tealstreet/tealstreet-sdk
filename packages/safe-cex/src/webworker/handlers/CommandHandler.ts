import { StoreWorkerPayload } from '../StoreWorkerPayload';
import { BaseHandler } from './BaseHandler';

/** CLI `exec` runs in the account engine's Rust algos. */
export class CommandHandler extends BaseHandler {
  handle(payload: StoreWorkerPayload): void {
    if (payload.op !== 'exec') return;
    // Hosted frames arrive across reconnects and version skew: a malformed one
    // is answered, never thrown (see the webworker CLAUDE.md).
    const data = payload.data && typeof payload.data === 'object' ? payload.data : undefined;
    const reqId = data?.reqId;
    const refuse = (message: string) => {
      this._exchange.emitter.emit('exec-log', reqId === undefined ? { message } : { message, reqId });
      if (reqId !== undefined) {
        this._exchange.emitter.emit('exec-result', { reqId, handled: false, success: false, error: message });
      }
    };
    if (!data) {
      refuse('Invalid command payload.');
      return;
    }
    this.runInRustAlgos(payload, undefined, refuse);
  }
}
