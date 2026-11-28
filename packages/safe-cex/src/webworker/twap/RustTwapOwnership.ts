import type { Exchange } from '../../exchanges/base';
import type { Order } from '../../types';
import type { TwapOptions } from './TwapOptions';
import type { TwapOwner } from './TwapOwner';
import type { TwapState } from './TwapState';

import { OrderType } from '../../types';
import { diagnosticConsole } from '../../utils/diagnosticRedaction';
import { settleInFlightSlice } from './inFlightSlice';
import { acquireTwapLock, acquireTwapLockWithin, TWAP_RESTORE_LOCK_WAIT_MS, twapLockName } from './twapLock';
import { captureTwapOwner } from './TwapOwner';
import { inFlightLimitNote, restingSlicesNote, twapRestoreRefusal } from './twapRestoreRefusal';
import { TwapStatus } from './TwapStatus';

/**
 * `stopped`: the user stopped a restore that waited for its lock while it held (or
 * was placing) limit slices. Persisted, so a reload still sends it paused and then
 * stops it, which is what cancels the slices; the algos never see the flag.
 */
type Row = { options: TwapOptions; state: TwapState; ownerId?: string; ownerGeneration?: number; stopped?: boolean };
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
    // A restored row awaiting its lock: resume waits for the lock. A stopped row that
    // holds (or may be placing) limit slices is still restored, paused, so the algos can cancel them.
    if (op === 'twap:stop' && (row.state.limitSlices?.length || (row.options.useLimitOrders && row.state.inFlight))) {
      row.stopped = true;
      row.state = { ...row.state, status: TwapStatus.Paused };
    } else if (op === 'twap:stop') this.pending.delete(data as string);
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
    // A restore still waiting never reached the algos, so nothing will cancel its slices.
    for (const [id, { options, state }] of this.pending) {
      const note = `${restingSlicesNote(state)}${inFlightLimitNote(options, state)}`;
      if (note) this.refuse(`TWAP ${options.activeMarket?.symbol ?? id} stopped before it was restored${note}.`);
    }
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
      const { id, options, state, ownerId, ownerGeneration, stopped } = value ?? {};
      if (!id || id in this.rows || this.pending.has(id) || this.locks.has(id)) continue;
      const symbol = options?.activeMarket?.symbol ?? id;
      const refusal = twapRestoreRefusal(options, state, Date.now());
      if (refusal) {
        this.refuse(
          `TWAP ${symbol} not restored: ${refusal}${restingSlicesNote(state)}${inFlightLimitNote(options, state)}`,
        );
        refused = true;
        continue;
      }
      if (ownerSession !== undefined && ownerId === undefined) {
        // The persisted ownerId is missing.
        this.cancelForOwnerChange(id, symbol, options, state);
        continue;
      }
      const row: PendingRow = {
        options: options!,
        state: state!,
        ownerSession,
        ...(ownerId !== undefined ? { ownerId } : {}),
        ...(ownerGeneration !== undefined ? { ownerGeneration } : {}),
        ...(stopped ? { stopped } : {}),
      };
      if (!this.ownerCurrent(row)) {
        this.cancelForOwnerChange(id, symbol, row.options, row.state);
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
      this.cancelForOwnerChange(id, symbol, row.options, row.state);
      return;
    }
    if (!release) {
      this.pending.delete(id);
      this.refuse(`TWAP ${symbol} not restored: it is running in another tab`);
      this.publish();
      return;
    }
    this.locks.set(id, release);
    const { placed, order } = inFlight
      ? await settleInFlightSlice(this.exchange, symbol, inFlight, row.state.side, () => this.ownerCurrent(row))
      : { placed: true, order: undefined };
    if (this.finished || this.pending.get(id) !== row) {
      this.release(id);
      this.publish();
      return;
    }
    const { inFlight: _settled, ...rest } = row.state;
    // A found limit slice rests like any other: the TWAP owns it and cancels it at its end.
    const adopted = inFlight && isLimitSlice(row, order) ? adoptSlice(rest, order, inFlight) : rest;
    if (!this.ownerCurrent(row)) {
      // Not ours to cancel under the new owner: name the found order, or say it is unconfirmed.
      const unconfirmed =
        inFlight && placed && !order
          ? `. A ${inFlight.size} slice being placed couldn’t be confirmed; check your open orders`
          : '';
      this.cancelForOwnerChange(id, symbol, row.options, adopted, unconfirmed);
      return;
    }
    this.pending.delete(id);
    const state: TwapState = !inFlight
      ? row.state
      : placed
        ? { ...adopted, ordersExecuted: rest.ordersExecuted + 1, nextOrderAt: lastIntervalEnd(row, adopted) }
        : { ...rest, ordersSizes: [inFlight.size, ...rest.ordersSizes] };
    // Its last slice was the one in flight: the TWAP is complete, unless limit slices
    // it placed still rest, which it cancels at the end of their interval.
    const { ownerSession: _ownerSession, stopped, ...restored } = row;
    if (stopped ? state.limitSlices?.length : state.ordersSizes.length > 0 || state.limitSlices?.length) {
      this.send('twap:state', [{ id, ...restored, state }]);
      if (stopped) this.send('twap:stop', id);
    }
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

  private cancelForOwnerChange(
    id: string,
    symbol: string,
    options: TwapOptions | undefined,
    state: TwapState | undefined,
    unconfirmed = '',
  ) {
    this.pending.delete(id);
    this.release(id);
    this.exchange.emitter.emit(
      'warning',
      `TWAP ${symbol} wasn’t restored because the account connection changed${restingSlicesNote(state)}${inFlightLimitNote(options, state)}${unconfirmed}.`,
    );
    this.publish();
  }

  private refuse(message: string) {
    this.exchange.emitter.emit('error', message);
  }
}

const isLimitSlice = (row: Row, order: Order | undefined): order is Order =>
  !!order && !!row.options.useLimitOrders && order.type === OrderType.Limit;

const adoptSlice = (
  state: Omit<TwapState, 'inFlight'>,
  order: Order,
  inFlight: NonNullable<TwapState['inFlight']>,
): Omit<TwapState, 'inFlight'> => ({
  ...state,
  limitSlices: [
    ...(state.limitSlices ?? []),
    { orderIds: [order.id], price: order.price || inFlight.price || 0, size: inFlight.size },
  ],
});

/** An adopted last slice still gets its own interval to fill before the TWAP ends and cancels it. */
const lastIntervalEnd = (row: Row, state: Omit<TwapState, 'inFlight'>): number =>
  state.ordersSizes.length === 0 && state.limitSlices !== row.state.limitSlices
    ? state.nextOrderAt + (row.options.duration / row.options.ordersCount) * 60_000
    : state.nextOrderAt;
