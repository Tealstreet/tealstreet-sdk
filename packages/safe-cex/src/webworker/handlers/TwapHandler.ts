import type { TwapOwner } from '../twap/TwapOwner';

import { StoreWorkerPayload } from '../StoreWorkerPayload';
import { RustTwapOwnership } from '../twap/RustTwapOwnership';
import { captureTwapOwner } from '../twap/TwapOwner';
import { BaseHandler, rustAlgoExchange } from './BaseHandler';

const TWAP_OPS = new Set<StoreWorkerPayload['op']>([
  'twap:start',
  'twap:resume',
  'twap:cleanup',
  'twap:pause',
  'twap:stop',
  'twap:state',
]);

/** TWAPs run in the account engine's Rust algos, under `RustTwapOwnership`'s lock and restore rules. */
export class TwapHandler extends BaseHandler {
  private readonly rustTwaps?: RustTwapOwnership;
  private readonly getOwner?: () => TwapOwner | null;

  constructor(...args: ConstructorParameters<typeof BaseHandler>) {
    super(...args);
    const exchange = rustAlgoExchange(this._exchange);
    if (!exchange) return;
    this.getOwner = this._ctx.getTwapOwner ? () => this._ctx.getTwapOwner?.() ?? null : undefined;
    this.rustTwaps = new RustTwapOwnership(
      this._exchange,
      (op, data) => exchange.algoOp(op, data),
      (rows) => this._ctx.postMessage({ op: 'twap:stateUpdate', data: rows }),
      this.getOwner,
    );
    // Exec's `twap` command starts them too, so listen before any op arrives.
    this._exchange.on('twap:stateUpdate', this.rustTwaps.receive);
  }

  handle(payload: StoreWorkerPayload): void {
    if (!TWAP_OPS.has(payload.op)) return;
    const rustTwaps = this.rustTwaps;
    const owner = captureTwapOwner(this.getOwner);
    this.runInRustAlgos(payload, rustTwaps && (() => rustTwaps.handle(payload.op, payload.data, owner)));
  }

  /** The engine died: post its TWAPs' final state while this handler's context is current. */
  finishRustTwaps() {
    this.rustTwaps?.finish();
  }

  override dispose() {
    if (this.rustTwaps) {
      this._exchange.off('twap:stateUpdate', this.rustTwaps.receive);
      this.rustTwaps.dispose();
    }
    super.dispose();
  }
}
