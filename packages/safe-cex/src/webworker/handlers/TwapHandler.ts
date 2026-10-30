import { StoreWorkerPayload } from '../StoreWorkerPayload';
import { TwapInstance } from '../twap/TwapInstance';
import { acquireTwapLock, acquireTwapLockWithin, TWAP_RESTORE_LOCK_WAIT_MS, twapLockName } from '../twap/twapLock';
import { twapManager } from '../twap/TwapManager';
import { TwapOptions } from '../twap/TwapOptions';
import { twapRestoreRefusal } from '../twap/twapRestoreRefusal';
import { TwapState } from '../twap/TwapState';
import { RustTwapOwnership } from '../twap/RustTwapOwnership';
import { BaseHandler, rustAlgoExchange } from './BaseHandler';

const TWAP_OPS = new Set<StoreWorkerPayload['op']>([
  'twap:start',
  'twap:resume',
  'twap:cleanup',
  'twap:pause',
  'twap:stop',
  'twap:state',
]);

type PersistedTwap = { id: string; options: TwapOptions; state: TwapState };

export class TwapHandler extends BaseHandler {
  /** The lock, restore and crash rules for this account's TWAPs in the Rust algos. */
  private readonly rustTwaps?: RustTwapOwnership;

  constructor(...args: ConstructorParameters<typeof BaseHandler>) {
    super(...args);
    const exchange = rustAlgoExchange(this._exchange);
    if (!exchange) return;
    this.rustTwaps = new RustTwapOwnership(
      this._exchange,
      (op, data) => exchange.algoOp(op, data),
      (rows) => this._ctx.postMessage({ op: 'twap:stateUpdate', data: rows }),
    );
    // Exec's `twap` command starts them too, so listen before any op arrives.
    this._exchange.on('twap:stateUpdate', this.rustTwaps.receive);
  }

  handle(payload: StoreWorkerPayload): void {
    const rustTwaps = this.rustTwaps;
    if (
      TWAP_OPS.has(payload.op) &&
      rustTwaps &&
      this.runInRustAlgos(payload, () => rustTwaps.handle(payload.op, payload.data))
    )
      return;
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
        // A fresh id, so the lock is free; it stops a duplicated tab restoring this TWAP.
        void acquireTwapLock(twapLockName(options.accountName, instance.id)).then((release) => {
          if (release) instance.holdOwnerLock(release);
          else
            this.refuse(
              `TWAP ${options.activeMarket?.symbol ?? instance.id} started without its cross-tab lock; a duplicated tab could also run it`,
            );
        });
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
        this.restore(payload.data);
        break;
    }
  }

  /**
   * Builds every restorable row synchronously (the worker tracks the TWAPs a
   * dispatch creates), then runs each only once it holds the row's cross-tab
   * lock. A row another tab runs is stopped before it places anything.
   */
  private restore(data: unknown) {
    if (!Array.isArray(data)) {
      console.error('[TWAP] twap:state expects an array');
      return;
    }
    let refused = false;
    for (const value of data as Partial<PersistedTwap>[]) {
      const { id, options, state } = value ?? {};
      if (!id || twapManager.runningTwaps.has(id)) continue;
      const symbol = options?.activeMarket?.symbol ?? id;
      const refusal = twapRestoreRefusal(options, state, Date.now());
      if (refusal) {
        this.refuse(`TWAP ${symbol} not restored: ${refusal}`);
        refused = true;
        continue;
      }
      const instance = new TwapInstance(
        this._exchange,
        this._ctx,
        options!,
        () => {
          console.log('[TWAP] twap state updated');
        },
        state,
        id,
      );
      instance.awaitOwnerLock();
      twapManager.add(instance);
      void acquireTwapLockWithin(twapLockName(options!.accountName, id), TWAP_RESTORE_LOCK_WAIT_MS).then((release) => {
        if (!release) {
          this.refuse(`TWAP ${symbol} not restored: it is running in another tab`);
          void instance.terminate();
          return;
        }
        instance.holdOwnerLock(release);
        void instance.reconcileInFlight().then(() => {
          if (instance.state.status === 'running') {
            instance.pause();
            instance.resume();
          }
        });
      });
    }
    // Rewrites this tab's rows, so a refused row is not restored and refused on every load.
    if (refused) this._ctx.postMessage({ op: 'twap:stateUpdate', data: twapManager.serializeState() });
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

  private refuse(message: string) {
    this._exchange.emitter.emit('error', message);
  }
}
