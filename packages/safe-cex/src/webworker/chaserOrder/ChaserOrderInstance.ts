import { nanoid } from 'nanoid';
import { Exchange } from '../../exchanges/base';
import { Order, OrderSide, PositionSide } from '../../types';
import { clone } from '../../utils/clone';
import { ChaserOrderOptions } from './ChaserOrderOptions';
import { ChaserOrderState } from './ChaserOrderState';

export const defaultChaserState: ChaserOrderState = Object.freeze({
  attempts: 0,
  isStopped: false,
  stopReason: '',
  currentOrdersIds: [],
});

// Most adapters expose `Order.filled` in the same unit accepted by
// `PlaceOrderOpts.amount`. BloFin-style contract adapters keep the public
// value in base units, so they supply the equivalent placement-unit fill.
export const placementFilled = (order: Order): number => {
  const placementFill = order.extra?.filledInPlacementUnits;
  return typeof placementFill === 'number' && Number.isFinite(placementFill) && placementFill >= 0
    ? placementFill
    : order.filled;
};

export class ChaserOrderInstance {
  private readonly _options: ChaserOrderOptions;
  private readonly _id: string;
  private readonly _state: ChaserOrderState;
  private _remainingSize: number;
  private _filledByOrderId: { [key: string]: number } = {};
  private _minTickDistance = 0;
  private _pendingOrdersUpdate = false;
  private _freshOrder = false;
  private _disposers: Array<() => void> = [];
  private _targetPositionContracts: number | undefined;
  private _completeEmitted = false;
  private _everTrackedOrderIds = new Set<string>();
  // Wall-clock ms of the most recent partial fill we observed.
  // Used by the manager to skip cancel+replace immediately after a
  // fill — see `ChaserOrderManager.updateOrders` for the rationale.
  private _lastPartialFillAt = 0;
  exchange: Exchange;

  constructor(exchange: Exchange, options: ChaserOrderOptions) {
    exchange.syncAccountState?.();
    this.exchange = exchange;
    this._options = options;
    this._id = nanoid();
    this._state = clone(defaultChaserState);
    this._remainingSize = options.size;
    if (!options.reduceOnly) {
      const targetSide = options.side === OrderSide.Buy ? PositionSide.Long : PositionSide.Short;
      const existingContracts =
        (exchange.store?.positions || []).find(
          (position) => position.symbol === options.symbol && position.side === targetSide,
        )?.contracts || 0;
      this._targetPositionContracts = Math.abs(existingContracts) + options.size;
    }
  }

  /**
   * Register a teardown callback (typically the `cleanup` closure built
   * inside `ChaserOrderManager.processInstance`). The supervisor closure
   * holds the event-listener `off` calls and any pending timeouts, so
   * external cancel paths (`ChaserOrderManager.remove` ->
   * `cleanupInstance`) need a way to detach them without waiting for
   * the next exchange `'update'` event to arrive. Disposers run once
   * and are dropped; calling `dispose` a second time is a no-op.
   */
  registerDisposer(fn: () => void) {
    this._disposers.push(fn);
  }

  dispose() {
    const disposers = this._disposers;
    this._disposers = [];
    for (const fn of disposers) {
      try {
        fn();
      } catch {
        // disposers must never throw — swallow so the rest still run
      }
    }
  }

  incrementAttempts() {
    if (this._state.attempts > this._options.maxAttempts) {
      this._state.isStopped = true;
      this._state.stopReason = 'Reached maximum number of attempts';
    } else {
      this._state.attempts++;
    }
  }

  markComplete(reason: string = 'Completed') {
    this._state.isStopped = true;
    this._state.stopReason = reason;
  }

  recalculateRemainingSize() {
    const remaining = this.options.size - Object.values(this._filledByOrderId).reduce((a, b) => a + b, 0);
    // Summed fills land a float hair off the size (0.1 + 0.058 vs 0.158); that
    // is filled, not a remainder to chase.
    this._remainingSize = Math.abs(remaining) <= Math.abs(this.options.size) * 1e-9 ? 0 : remaining;
  }

  subtractFilledSize(order: Order) {
    const filled = placementFilled(order);

    const prevFilled = this._filledByOrderId[order.id] || 0;
    if (filled > prevFilled) {
      this._lastPartialFillAt = Date.now();
    }
    this._filledByOrderId[order.id] = filled;
    this.recalculateRemainingSize();
    if (this._remainingSize <= 0) {
      this._state.isStopped = true;
      this._state.stopReason = 'Fully filled already';
    }
  }

  /**
   * Add a fill amount from a fill event (incremental fill tracking).
   * Used for exchanges like Hyperliquid where fill events arrive faster than order status updates.
   * @param orderId The order ID that was filled
   * @param amount The incremental fill amount
   */
  addFillAmount(orderId: string, amount: number) {
    // Fill streams are external input. Exchange adapters validate their own
    // payloads, but keep the shared chaser fail-closed so a malformed event
    // can never increase remaining size or turn it into NaN/Infinity.
    if (!Number.isFinite(amount) || amount <= 0) {
      return;
    }

    // Accumulate fill amounts for this order
    this._lastPartialFillAt = Date.now();
    this._filledByOrderId[orderId] = (this._filledByOrderId[orderId] || 0) + amount;
    this.recalculateRemainingSize();
    if (this._remainingSize <= 0) {
      this._state.isStopped = true;
      this._state.stopReason = 'Fully filled via fill events';
    }
  }

  /**
   * Set the total filled amount for an order (absolute value from REST API).
   * Only updates if the new amount is greater than what we've tracked.
   * @param orderId The order ID
   * @param totalFilledAmount The total filled amount from REST API
   */
  setFilledAmount(orderId: string, totalFilledAmount: number) {
    const currentFilled = this._filledByOrderId[orderId] || 0;
    // Only update if REST shows more filled than we knew about
    if (totalFilledAmount > currentFilled) {
      this._lastPartialFillAt = Date.now();
      this._filledByOrderId[orderId] = totalFilledAmount;
      this.recalculateRemainingSize();
      if (this._remainingSize <= 0) {
        this._state.isStopped = true;
        this._state.stopReason = 'Fully filled via REST check';
      }
    }
  }

  /** True exactly once, so `chaser:complete` cannot fire twice for one chase. */
  claimCompleteEmission() {
    if (this._completeEmitted) return false;
    this._completeEmitted = true;
    return true;
  }

  get filledSize() {
    return Math.min(this._options.size, Math.max(0, this._options.size - this._remainingSize));
  }

  get lastPartialFillAt() {
    return this._lastPartialFillAt;
  }

  set currentOrderIds(orderIds: string[]) {
    this._state.currentOrdersIds = orderIds;
    for (const id of orderIds) this._everTrackedOrderIds.add(id);
  }

  /** Every order id this chase has placed, kept after `currentOrderIds` is cleared. */
  get everTrackedOrderIds(): ReadonlySet<string> {
    return this._everTrackedOrderIds;
  }

  get isStopped() {
    return this._state.isStopped;
  }

  get id() {
    return this._id;
  }

  get options() {
    return this._options;
  }

  get state() {
    return this._state;
  }

  get currentOrderIds() {
    return this._state.currentOrdersIds;
  }

  get remainingSize() {
    return this._remainingSize;
  }

  get targetPositionContracts() {
    return this._targetPositionContracts;
  }

  get minTickDistance() {
    return this._minTickDistance;
  }

  set minTickDistance(value: number) {
    this._minTickDistance = value;
  }

  get pendingOrdersUpdate() {
    return this._pendingOrdersUpdate;
  }

  set pendingOrdersUpdate(value: boolean) {
    this._pendingOrdersUpdate = value;
  }

  get freshOrder() {
    return this._freshOrder;
  }

  set freshOrder(value: boolean) {
    this._freshOrder = value;
  }

  /**
   * `chase ... to <length>` termination check. Returns `true` if the
   * given live price has moved past the user's threshold in the
   * unfavorable direction (up for buys, down for sells). The
   * threshold is resolved per the configured `terminate.type`:
   *
   *   - `price`   → absolute price threshold (anchor ignored)
   *   - `percent` → `anchor * (1 ± value/100)`
   *   - `usd`     → `anchor ± value`
   *
   * Returns `false` when no termination is configured. Caller is
   * responsible for calling `markComplete('Terminated by length
   * condition')` and tearing down on a `true`.
   */
  shouldTerminate(currentPrice: number): boolean {
    const t = this._options.terminate;
    if (!t) return false;
    if (!Number.isFinite(currentPrice) || currentPrice <= 0) return false;
    const isBuy = this._options.side === OrderSide.Buy;

    let threshold: number;
    switch (t.type) {
      case 'price':
        threshold = t.value;
        break;
      case 'percent':
        threshold = isBuy
          ? t.anchorPrice * (1 + t.value / 100)
          : t.anchorPrice * (1 - t.value / 100);
        break;
      case 'usd':
        threshold = isBuy ? t.anchorPrice + t.value : t.anchorPrice - t.value;
        break;
      default:
        return false;
    }
    return isBuy ? currentPrice >= threshold : currentPrice <= threshold;
  }
}
