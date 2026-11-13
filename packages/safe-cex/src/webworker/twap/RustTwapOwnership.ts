import type { Exchange } from '../../exchanges/base';
import type { TwapOptions } from './TwapOptions';
import type { TwapOwner } from './TwapOwner';
import type { TwapState } from './TwapState';
import { diagnosticConsole } from '../../utils/diagnosticRedaction';

import { inFlightSlicePlaced } from './inFlightSlice';
import { acquireTwapLock, acquireTwapLockWithin, TWAP_RESTORE_LOCK_WAIT_MS, twapLockName } from './twapLock';
import { captureTwapOwner } from './TwapOwner';
import { twapRestoreRefusal } from './twapRestoreRefusal';
import { TwapStatus } from './TwapStatus';

type Row = { options: TwapOptions; state: TwapState; ownerId?: string; ownerGeneration?: number };
type PendingRow = Row & { ownerSession: TwapOwner | null | undefined };
type PersistedTwap = Row & { id: string };

/**
 * The worker's rules for a TWAP running in the Rust algos: one cross-tab Web
 * Lock per TWAP, a restore that runs only once it holds the lock and has
 * settled its in-flight slice, and a final persisted state when the engine dies. The algos persist their own rows
 * (`twap:stateUpdate`); this posts them with the restores still waiting.
 */
export class RustTwapOwnership {
  /** The algos' last persisted rows. */
  private rows: Record<string, Row> = {};
  /** Restored rows waiting for their lock and slice reconcile; the algos do not have them yet. */
  private readonly pending = new Map<string, PendingRow>();
  private readonly locks = new Map<string, () => void>();
  /** Started TWAPs whose lock was asked for (taken, being taken, or refused). */
  private readonly lockAsked = new Set<string>();
  private finished = false;

  constructor(
    private readonly exchange: Exchange,
    private readonly send: (op: string, data: unknown) => void,
    private readonly post: (rows: Record<string, Row>) => void,
    private readonly getOwner?: () => TwapOwner | null,
  ) {}

  /** One `twap:*` op for the algos. An op naming a restore still waiting applies to it here. */
  handle(op: string, data: unknown, ownerSession = captureTwapOwner(this.getOwner)) {
    if (this.finished) return;
    if (op === 'twap:state') return this.restore(data, ownerSession);
    const row = typeof data === 'string' ? this.pending.get(data) : undefined;
    if (!row) return this.send(op, data);
    // A restored row awaiting its lock: resume waits for the lock.
    if (op === 'twap:stop') this.pending.delete(data as string);
    else if (op === 'twap:pause' && row.state.status === TwapStatus.Running)
      row.state = { ...row.state, status: TwapStatus.Paused };
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

  private restore(data: unknown, ownerSession: TwapOwner | null | undefined) {
    if (!Array.isArray(data)) {
      diagnosticConsole.error('[TWAP] twap:state expects an array');
      return;
    }
    let refused = false;
    for (const value of data as Partial<PersistedTwap>[]) {
      const { id, options, state, ownerId, ownerGeneration } = value ?? {};
      if (!id || id in this.rows || this.pending.has(id) || this.locks.has(id)) continue;
      const symbol = options?.activeMarket?.symbol ?? id;
      const refusal = twapRestoreRefusal(options, state, Date.now());
      if (refusal) {
        this.refuse(`TWAP ${symbol} not restored: ${refusal}`);
        refused = true;
        continue;
      }
      if (ownerSession !== undefined && ownerId === undefined) {
        this.cancelForOwnerChange(id, symbol, 'persisted ownerId is missing');
        continue;
      }
      const row: PendingRow = {
        options: options!,
        state: state!,
        ownerSession,
        ...(ownerId !== undefined ? { ownerId } : {}),
        ...(ownerGeneration !== undefined ? { ownerGeneration } : {}),
      };
      if (!this.ownerCurrent(row)) {
        this.cancelForOwnerChange(id, symbol);
        continue;
      }
      this.pending.set(id, row);
      void this.claim(id, symbol, row);
    }
    // Rewrites this tab's rows, so a refused row is not restored and refused on every load.
    if (refused) this.publish();
  }

  private async claim(id: string, symbol: string, row: PendingRow) {
    const inFlight = row.state.inFlight;
    const release = await acquireTwapLockWithin(twapLockName(row.options.accountName, id), TWAP_RESTORE_LOCK_WAIT_MS);
    if (this.finished || this.pending.get(id) !== row) {
      release?.();
      return;
    }
    if (!this.ownerCurrent(row)) {
      release?.();
      this.cancelForOwnerChange(id, symbol);
      return;
    }
    if (!release) {
      this.pending.delete(id);
      this.refuse(`TWAP ${symbol} not restored: it is running in another tab`);
      this.publish();
      return;
    }
    this.locks.set(id, release);
    const placed = inFlight
      ? await inFlightSlicePlaced(this.exchange, symbol, inFlight, () => this.ownerCurrent(row))
      : true;
    if (this.finished || this.pending.get(id) !== row) {
      this.release(id);
      this.publish();
      return;
    }
    if (!this.ownerCurrent(row)) {
      this.cancelForOwnerChange(id, symbol);
      return;
    }
    this.pending.delete(id);
    const { inFlight: _settled, ...rest } = row.state;
    const state: TwapState = !inFlight
      ? row.state
      : placed
        ? { ...rest, ordersExecuted: rest.ordersExecuted + 1 }
        : { ...rest, ordersSizes: [inFlight.size, ...rest.ordersSizes] };
    // Its last slice was the one in flight: the TWAP is complete.
    const { ownerSession: _ownerSession, ...restored } = row;
    if (state.ordersSizes.length > 0) this.send('twap:state', [{ id, ...restored, state }]);
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
    const pending = [...this.pending].map(([id, { ownerSession: _ownerSession, ...row }]) => [id, row]);
    if (!this.finished) this.post({ ...this.rows, ...Object.fromEntries(pending) });
  }

  private ownerCurrent(row: PendingRow): boolean {
    const current = captureTwapOwner(this.getOwner);
    const captured = row.ownerSession;
    if (captured === undefined) return current === undefined && row.ownerId === undefined;
    return (
      !!captured &&
      !!current &&
      row.ownerId === captured.ownerId &&
      current.ownerId === captured.ownerId &&
      current.generation === captured.generation
    );
  }

  private cancelForOwnerChange(id: string, symbol: string, reason?: string) {
    this.pending.delete(id);
    this.release(id);
    this.exchange.emitter.emit(
      'warning',
      `TWAP ${symbol} restore cancelled-for-owner-change${reason ? `: ${reason}` : ''}`,
    );
    this.publish();
  }

  private refuse(message: string) {
    this.exchange.emitter.emit('error', message);
  }
}
