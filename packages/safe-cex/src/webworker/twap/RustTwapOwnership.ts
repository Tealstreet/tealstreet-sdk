import type { Exchange } from '../../exchanges/base';
import type { TwapOptions } from './TwapOptions';
import type { TwapState } from './TwapState';

import { inFlightSlicePlaced } from './inFlightSlice';
import { acquireTwapLock, acquireTwapLockWithin, TWAP_RESTORE_LOCK_WAIT_MS, twapLockName } from './twapLock';
import { twapRestoreRefusal } from './twapRestoreRefusal';
import { TwapStatus } from './TwapStatus';

type Row = { options: TwapOptions; state: TwapState };
type PersistedTwap = Row & { id: string };

/**
 * The TS TWAP rules a TWAP running in the Rust algos keeps (`TwapHandler`,
 * `TwapInstance`): one cross-tab Web Lock per TWAP, a restore that runs only
 * once it holds the lock and has settled its in-flight slice, and a final
 * persisted state when the engine dies. The algos persist their own rows
 * (`twap:stateUpdate`); this posts them with the restores still waiting.
 */
export class RustTwapOwnership {
  /** The algos' last persisted rows. */
  private rows: Record<string, Row> = {};
  /** Restored rows waiting for their lock and slice reconcile; the algos do not have them yet. */
  private readonly pending = new Map<string, Row>();
  private readonly locks = new Map<string, () => void>();
  /** Started TWAPs whose lock was asked for (taken, being taken, or refused). */
  private readonly lockAsked = new Set<string>();
  private finished = false;

  constructor(
    private readonly exchange: Exchange,
    private readonly send: (op: string, data: unknown) => void,
    private readonly post: (rows: Record<string, Row>) => void,
  ) {}

  /** One `twap:*` op for the algos. An op naming a restore still waiting applies to it here. */
  handle(op: string, data: unknown) {
    if (op === 'twap:state') return this.restore(data);
    const row = typeof data === 'string' ? this.pending.get(data) : undefined;
    if (!row) return this.send(op, data);
    // As a restored TwapInstance awaiting its lock: resume waits for the lock.
    if (op === 'twap:stop') this.pending.delete(data as string);
    else if (op === 'twap:pause' && row.state.status === TwapStatus.Running) row.state = { ...row.state, status: TwapStatus.Paused };
    else if (op === 'twap:cleanup') row.state = { ...row.state, markForCleanup: true };
    else return;
    this.publish();
  }

  /** The algos' `twap:stateUpdate`: take a new TWAP's lock, release a finished one's. */
  receive = (data: unknown) => {
    if (this.finished) return;
    this.rows = data && typeof data === 'object' ? { ...(data as Record<string, Row>) } : {};
    for (const [id, release] of this.locks) {
      if (!(id in this.rows) && !this.pending.has(id)) {
        release();
        this.locks.delete(id);
      }
    }
    for (const id of this.lockAsked) if (!(id in this.rows)) this.lockAsked.delete(id);
    for (const [id, row] of Object.entries(this.rows)) {
      if (!this.locks.has(id) && !this.lockAsked.has(id)) this.lockStarted(id, row);
    }
    this.publish();
  };

  /** The engine died: its TWAPs are stopped, so main must not restore them. */
  finish() {
    if (this.finished) return;
    const persisted = Object.keys(this.rows).length > 0 || this.pending.size > 0;
    this.dispose();
    if (persisted) this.post({});
  }

  dispose() {
    this.finished = true;
    this.rows = {};
    this.pending.clear();
    for (const release of this.locks.values()) release();
    this.locks.clear();
  }

  private restore(data: unknown) {
    if (!Array.isArray(data)) {
      console.error('[TWAP] twap:state expects an array');
      return;
    }
    let refused = false;
    for (const value of data as Partial<PersistedTwap>[]) {
      const { id, options, state } = value ?? {};
      if (!id || id in this.rows || this.pending.has(id) || this.locks.has(id)) continue;
      const symbol = options?.activeMarket?.symbol ?? id;
      const refusal = twapRestoreRefusal(options, state, Date.now());
      if (refusal) {
        this.refuse(`TWAP ${symbol} not restored: ${refusal}`);
        refused = true;
        continue;
      }
      this.pending.set(id, { options: options!, state: state! });
      void this.claim(id, symbol, options!.accountName, state!.inFlight);
    }
    // Rewrites this tab's rows, so a refused row is not restored and refused on every load.
    if (refused) this.publish();
  }

  private async claim(id: string, symbol: string, accountName: string, inFlight: TwapState['inFlight']) {
    const release = await acquireTwapLockWithin(twapLockName(accountName, id), TWAP_RESTORE_LOCK_WAIT_MS);
    if (this.finished || !this.pending.has(id)) {
      release?.();
      return;
    }
    if (!release) {
      this.pending.delete(id);
      this.refuse(`TWAP ${symbol} not restored: it is running in another tab`);
      this.publish();
      return;
    }
    this.locks.set(id, release);
    const placed = inFlight ? await inFlightSlicePlaced(this.exchange, symbol, inFlight) : true;
    const row = this.pending.get(id);
    this.pending.delete(id);
    if (this.finished || !row) {
      this.release(id);
      this.publish();
      return;
    }
    const { inFlight: _settled, ...rest } = row.state;
    const state: TwapState = !inFlight
      ? row.state
      : placed
        ? { ...rest, ordersExecuted: rest.ordersExecuted + 1 }
        : { ...rest, ordersSizes: [inFlight.size, ...rest.ordersSizes] };
    // Its last slice was the one in flight: the TWAP is complete.
    if (state.ordersSizes.length > 0) this.send('twap:state', [{ id, options: row.options, state }]);
    // The algos refused it, or it already completed.
    if (!(id in this.rows)) this.release(id);
    this.publish();
  }

  private lockStarted(id: string, row: Row) {
    this.lockAsked.add(id);
    void acquireTwapLock(twapLockName(row.options.accountName, id)).then((release) => {
      if (!release) {
        if (!this.finished && id in this.rows)
          this.refuse(
            `TWAP ${row.options.activeMarket?.symbol ?? id} started without its cross-tab lock; a duplicated tab could also run it`,
          );
        return;
      }
      if (this.finished || !(id in this.rows) || this.locks.has(id)) release();
      else this.locks.set(id, release);
    });
  }

  private release(id: string) {
    this.locks.get(id)?.();
    this.locks.delete(id);
  }

  private publish() {
    if (!this.finished) this.post({ ...this.rows, ...Object.fromEntries(this.pending) });
  }

  private refuse(message: string) {
    this.exchange.emitter.emit('error', message);
  }
}
