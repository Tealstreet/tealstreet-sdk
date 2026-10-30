import random from 'lodash/random';
import { nanoid } from 'nanoid';

import { Exchange } from '../../exchanges/base';
import { LogSeverity, OrderSide, OrderType } from '../../types';
import { add, multiply, subtract } from '../../utils/safe-math';
import { ChaserOrderInstance } from '../chaserOrder/ChaserOrderInstance';
import { chaserOrderManagerInstance } from '../chaserOrder/ChaserOrderManager';
import { twapManager } from './TwapManager';
import { TwapFailAction, TwapOptions, TwapTimeoutAction } from './TwapOptions';
import { TwapState } from './TwapState';
import { TwapStatus } from './TwapStatus';

export class TwapInstance {
  public id: string;
  public state: TwapState;
  public options: TwapOptions;
  private timerId: NodeJS.Timeout | undefined;
  private readonly updateStateCallback: (state: TwapState) => void;
  readonly exchange: Exchange;
  private ctx: Worker;
  private trackedChaserOrderIds: string[] = [];
  private trackedLimitOrderIds: string[] = [];
  /** Per-slice timeout timers used by `twap chase` policy enforcement.
   *  Stored so terminate() can cancel them. */
  private sliceTimers: ReturnType<typeof setTimeout>[] = [];
  /** Releases the cross-tab owner lock (`twapLock.ts`) once the TWAP ends. */
  private releaseOwnerLock: (() => void) | undefined;
  /** A restored TWAP places nothing, even on a user resume, until it holds its lock. */
  private awaitingOwnerLock = false;

  constructor(
    exchange: Exchange,
    ctx: Worker,
    options: TwapOptions,
    updateStateCallback: (state: TwapState) => void,
    state?: TwapState,
    id?: string,
  ) {
    this.id = id ? id : nanoid();
    this.options = options;
    this.exchange = exchange;
    this.ctx = ctx;
    this.updateStateCallback = updateStateCallback;

    if (state) {
      this.state = state;
    } else {
      this.state = this.createNewState();
    }
  }

  get currentOrderIds(): string[] {
    return this.trackedLimitOrderIds;
  }

  /** Hold the cross-tab owner lock until the TWAP completes or stops. */
  awaitOwnerLock() {
    this.awaitingOwnerLock = true;
  }

  holdOwnerLock(release: () => void) {
    this.awaitingOwnerLock = false;
    if (this.state.status === TwapStatus.Completed) release();
    else this.releaseOwnerLock = release;
  }

  private updateState(state: Partial<TwapState>) {
    this.state = { ...this.state, ...state };
    if (this.state.status === TwapStatus.Completed && this.releaseOwnerLock) {
      this.releaseOwnerLock();
      this.releaseOwnerLock = undefined;
    }
    this.updateStateCallback(this.state);
    this.ctx.postMessage({ op: 'twap:stateUpdate', data: twapManager.serializeState() });
  }

  createNewState(): TwapState {
    const { accountName, activeMarket, side, size, ordersCount, randomnessPercentage, origin } = this.options;

    const arrayLength = Math.max(0, Math.floor(ordersCount / 2));
    const ordersSizes = Array(arrayLength)
      .fill(size / ordersCount)
      .flatMap((orderSize) => {
        const sizeMod = multiply(random(0, randomnessPercentage), 0.01);
        const diff = Math.abs(multiply(orderSize, sizeMod));
        const orderSizeA = add(orderSize, diff);
        const orderSizeB = subtract(orderSize, diff);
        return [orderSizeA, orderSizeB];
      });

    if (ordersCount % 2 !== 0) {
      ordersSizes.push(this.options.size / this.options.ordersCount);
    }

    const remainingSize = (size || 0) - ordersSizes.reduce((s, order) => s + order, 0);

    ordersSizes[ordersSizes.length - 1] += remainingSize;

    return {
      accountName,
      activeMarket,
      side,
      size,
      status: TwapStatus.Idle,
      ordersSizes,
      ordersCount,
      ordersExecuted: 0,
      nextOrderAt: 0,
      origin: origin || 'web', // Default to 'web' for backwards compatibility
      createdAt: Date.now(),
    };
  }

  placeOrder = async (orderSize: number) => {
    if (this.options.useChaserOrders) {
      const chaserInstance = new ChaserOrderInstance(this.exchange, {
        symbol: this.options.activeMarket.symbol,
        size: orderSize,
        side: this.options.side,
        maxAttempts: 1000,
        reduceOnly: Boolean(this.options.reduceOnly),
        delay: this.options.chaserDelay,
      });
      this.trackedChaserOrderIds.push(chaserInstance.id);
      chaserOrderManagerInstance.add(chaserInstance);

      // `twap chase` policy: enforce a per-slice deadline. At
      // `timeoutPct%` of the slice interval, settle the chaser based
      // on its state at that moment:
      //   - active        → timeoutAction
      //   - stopped + remaining size > 0 (failed) → failAction
      //   - stopped + remaining size = 0 (filled) → no-op
      // 'market' fires a market for the unfilled portion; 'next'
      // forfeits the unfilled size and lets the next slice continue.
      if (this.options.timeoutPct !== undefined) {
        this.scheduleSliceTimeout(chaserInstance.id, orderSize);
      }
    } else if (this.options.useLimitOrders) {
      const ticker = this.exchange.store.tickers.find((ticker) => ticker.symbol === this.options.activeMarket.symbol);
      const price =
        this.options.side === OrderSide.Buy ? ticker?.bid || ticker?.last || 0 : ticker?.ask || ticker?.last || 0;
      const orderIds = await this.exchange.placeOrder({
        symbol: this.options.activeMarket.symbol,
        side: this.options.side,
        type: price ? OrderType.Limit : OrderType.Market,
        ...(price ? { price } : {}),
        amount: orderSize,
        reduceOnly: Boolean(this.options.reduceOnly),
      });
      if (price && orderIds.length > 0) {
        this.trackedLimitOrderIds.push(...orderIds);
      }
    } else {
      await this.exchange.placeOrder({
        symbol: this.options.activeMarket.symbol,
        side: this.options.side,
        type: OrderType.Market,
        amount: orderSize,
        reduceOnly: Boolean(this.options.reduceOnly),
      });
    }
  };

  /** Compute slice interval in ms (duration is minutes per safe-cex's
   *  TwapOptions; ordersPace is per the existing convention). */
  private sliceIntervalMs(): number {
    const ordersPace = this.options.duration / this.options.ordersCount;
    return ordersPace * 60 * 1000;
  }

  /** Place a market order for the slice's unfilled remainder. */
  private async fireMarketRemainder(amount: number): Promise<void> {
    if (amount <= 0) return;
    try {
      await this.exchange.placeOrder({
        symbol: this.options.activeMarket.symbol,
        side: this.options.side,
        type: OrderType.Market,
        amount,
        reduceOnly: Boolean(this.options.reduceOnly),
      });
    } catch {
      // Best-effort fallback; TWAP itself stays alive for the next slice.
    }
  }

  private scheduleSliceTimeout(chaserId: string, _orderSize: number): void {
    const sliceMs = this.sliceIntervalMs();
    const pct = this.options.timeoutPct ?? 50;
    const timeoutAction: TwapTimeoutAction = this.options.timeoutAction ?? 'market';
    const failAction: TwapFailAction = this.options.failAction ?? 'market';
    const timeoutMs = Math.max(0, sliceMs * (pct / 100));

    const timer = setTimeout(() => {
      const idx = this.sliceTimers.indexOf(timer);
      if (idx >= 0) this.sliceTimers.splice(idx, 1);

      const inst = chaserOrderManagerInstance.instances.find((i) => i.id === chaserId);
      if (!inst) return; // chaser already cleaned up
      const remaining = inst.remainingSize;
      if (remaining <= 0) return; // already filled

      const action: TwapTimeoutAction | TwapFailAction = !inst.isStopped ? timeoutAction : failAction;

      // Cancel chaser + its open order, then take action on the remainder.
      chaserOrderManagerInstance.remove(inst.id);
      if (action === 'market') {
        void this.fireMarketRemainder(remaining);
      }
      // 'next' → do nothing; the next slice fires on its own timer.
    }, timeoutMs);

    this.sliceTimers.push(timer);
  }

  start = async () => {
    if (this.awaitingOwnerLock) return;
    if (this.state.status === TwapStatus.Completed || this.state.status === TwapStatus.Paused) {
      return;
    }

    const orderSize = this.state.ordersSizes.shift();
    const ordersPace = this.options.duration / this.options.ordersCount;
    const rand = multiply(random(-this.options.randomnessPercentage, this.options.randomnessPercentage), 0.01);
    const wait = ordersPace * 60 * 1000;
    const waitWithRandom = wait + wait * rand;
    const nextOrderAt = Date.now() + waitWithRandom;

    if (this.state.status === TwapStatus.Idle) {
      this.updateState({
        status: TwapStatus.Running,
        nextOrderAt: Date.now(),
        startedAt: Date.now(),
      });
    }

    let ordersExecuted = this.state.ordersExecuted;

    if (orderSize) {
      try {
        await this.placeOrder(orderSize);
      } catch (error) {
        // Surface the failed slice instead of swallowing it — bridged
        // to the web toast / CLI log via the exchange `log` event. The
        // TWAP stays alive and advances to the next slice on purpose.
        this.exchange.emitter.emit(
          'log',
          `TWAP ${this.options.activeMarket.symbol} slice failed: ${(error as Error)?.message ?? error}`,
          LogSeverity.Error,
        );
      }
      // terminate() can run while the placement above is awaiting a reply.
      // Its continuation must not schedule another slice on the old exchange.
      if ([TwapStatus.Completed].includes(this.state.status)) return false;
      ordersExecuted++;
    }

    if (this.state.ordersSizes.length === 0) {
      this.updateState({ status: TwapStatus.Completed, ordersExecuted });
      twapManager.remove(this.id);
      return false;
    }

    this.updateState({ ordersExecuted, nextOrderAt });
    this.timerId = setTimeout(() => this.start(), waitWithRandom);

    return true;
  };

  pause = async () => {
    if (this.state.status === TwapStatus.Running) {
      this.updateState({ status: TwapStatus.Paused });
      if (this.timerId) {
        clearTimeout(this.timerId);
      }
    }
  };

  resume = async () => {
    if (this.awaitingOwnerLock) return;
    if (this.state.status === TwapStatus.Paused) {
      let nextOrderAt: number;

      if (this.state.nextOrderAt > Date.now()) {
        nextOrderAt = this.state.nextOrderAt;
      } else {
        nextOrderAt = Date.now();
      }

      this.timerId = setTimeout(() => {
        this.start();
      }, nextOrderAt - Date.now());

      this.updateState({ status: TwapStatus.Running });
    }
  };

  terminate = async () => {
    if (this.state.status !== TwapStatus.Completed) {
      this.updateState({ status: TwapStatus.Completed });
      if (this.timerId) {
        clearTimeout(this.timerId);
      }
      // Cancel any pending per-slice timeout fires
      for (const t of this.sliceTimers) clearTimeout(t);
      this.sliceTimers = [];
    }
    twapManager.remove(this.id);
  };

  cleanup = async () => {
    this.updateState({ markForCleanup: true });
  };
}
