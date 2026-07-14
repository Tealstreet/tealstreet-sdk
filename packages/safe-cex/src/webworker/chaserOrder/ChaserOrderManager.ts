import { sleep } from '@tealstreet/sleep/src/sleep';
import {
  Order,
  OrderFillEvent,
  OrderRejectReason,
  OrderSide,
  OrderStatus,
  OrderTimeInForce,
  OrderType,
  PositionSide,
  StoreData,
} from '../../types';
import { getTickerPredictedBestAsk } from '../../utils/getTickerPredictedBestAsk';
import { getTickerPredictedBestBid } from '../../utils/getTickerPredictedBestBid';
import { adjust } from '../../utils/safe-math';
import { ChaserOrderInstance } from './ChaserOrderInstance';

const CHASER_PRICE_CHECK_MIN_INTERVAL_MS = 50;

// After observing a partial fill on the tracked order, freeze the
// cancel+replace path for this long. The order is at the touch and
// actively trading; canceling now races the in-flight fills (the cancel
// REST round-trip is ~tens-of-ms while another fill can complete on
// the exchange in <1ms). If we cancel mid-fill, the cancel arrives
// after the order has fully filled, the exchange replies "Unknown
// order" (silently swallowed), and we still go on to place a fresh
// order for the "remaining" size — overfilling the user's target.
// 1000ms is enough to absorb a normal multi-tranche fill burst; if no
// more fills arrive within the window, normal chasing resumes.
const CHASER_HOLD_STEADY_AFTER_PARTIAL_FILL_MS = 1000;

export class ChaserOrderManager {
  private _chaserOrderInstances: ChaserOrderInstance[] = [];

  log(message: string, instance: ChaserOrderInstance) {
    instance.exchange.emitter.emit('log', `[CHASER${instance.id}] ${message}`);
    // console.log(`[CHASER${instance.id}] ${message}`);
  }

  add(instance: ChaserOrderInstance) {
    this._chaserOrderInstances.push(instance);
    this.processInstance(instance);
  }

  private getCurrentOrders(instance: ChaserOrderInstance) {
    return instance.exchange.store.orders.filter((order) => instance.currentOrderIds.includes(order.id)) || [];
  }

  private findBestPrice(instance: ChaserOrderInstance) {
    const exchange = instance.exchange;

    const market =
      exchange.store.marketsBySymbol[instance.options.symbol] ||
      exchange.store.markets.find((market) => market.symbol === instance.options.symbol);
    const ticker =
      exchange.store.tickersBySymbol[instance.options.symbol] ||
      exchange.store.tickers.find((market) => market.symbol === instance.options.symbol);

    if (!ticker || !market) {
      this.log('no market or ticker found', instance);
      instance.markComplete('No market or ticker found');
      exchange.emitter.emit('error', `Chaser no market or ticker found for ${instance.options.symbol}`);
      throw new Error(`No market or ticker found for ${instance.options.symbol}`);
    }

    const bestBid = ticker && market ? getTickerPredictedBestBid(ticker, market) : 0;
    const bestAsk = ticker && market ? getTickerPredictedBestAsk(ticker, market) : 0;
    const precision = market?.precision.price;

    // Safety guard for stale/locked quotes:
    // always keep buy prices below ask and sell prices above bid, then apply tick distance.
    let rawPrice =
      instance.options.side === OrderSide.Buy
        ? bestBid - precision * instance.minTickDistance
        : bestAsk + precision * instance.minTickDistance;

    if (instance.options.side === OrderSide.Buy && bestAsk > 0) {
      rawPrice = Math.min(rawPrice, bestAsk - precision);
    }
    if (instance.options.side === OrderSide.Sell && bestBid > 0) {
      rawPrice = Math.max(rawPrice, bestBid + precision);
    }

    const price = adjust(Math.max(rawPrice, precision), precision);
    return price;
  }

  private async tryPlaceOrders(instance: ChaserOrderInstance, tries = 1): Promise<string[]> {
    const exchange = instance.exchange;
    const maxTries = exchange.name === 'binance' ? 20 : 10;

    // Bail out on user-cancel BEFORE placing anything. Without these
    // guards, a user clicking STOP while a placeOrder is in flight
    // (post-only retries, slow HTTP) would still see a fresh order
    // land in the book after `cleanupInstance` already read the
    // stale `currentOrderIds` and spliced the instance.
    if (instance.isStopped) {
      this.log(`tryPlaceOrders early-exit: instance is stopped`, instance);
      return [];
    }

    if (!instance.remainingSize) {
      this.log('size is 0', instance);
      instance.markComplete('No remaining size. User likely cancelled order');
      exchange.emitter.emit('error', 'Chaser has no remaining size. User likely cancelled order');
      throw new Error('size is 0');
    }

    if (tries >= maxTries) {
      this.log(`failed to place orders after ${maxTries} tries`, instance);
      exchange.emitter.emit('error', `Chaser failed to place orders after ${maxTries} tries`);
      instance.markComplete(`Failed to place orders after ${maxTries} tries`);
      this.checkAndMarkIfComplete(instance);
      this.cleanupInstance(instance);
      return [];
    }

    // hack for bybit for 1st order rejection
    instance.freshOrder = tries === 1;

    const price = this.findBestPrice(instance);
    this.log(
      `tryPlaceOrders try=${tries} price=${price} size=${instance.remainingSize} tickDist=${instance.minTickDistance}`,
      instance,
    );

    let orderIds: string[] = [];
    try {
      orderIds = await exchange.placeOrder({
        symbol: instance.options.symbol,
        side: instance.options.side,
        type: OrderType.Limit,
        price,
        //amount: instance.options.size,
        amount: instance.remainingSize,
        reduceOnly: instance.options.reduceOnly,
        timeInForce: OrderTimeInForce.PostOnly,
      });
      this.log(`placeOrder returned: ${JSON.stringify(orderIds)}`, instance);
    } catch (e) {
      console.error(e);
      this.log(`placeOrder exception: ${e}`, instance);
    }

    // If the user cancelled while `placeOrder` was in flight, cancel
    // the just-placed order(s) ourselves rather than handing them back
    // to the (now-detached) supervisor. `cleanupInstance` may have
    // already read `instance.currentOrderIds` (still empty/stale at
    // this point) and finished — these IDs would otherwise be orphan
    // resting orders the user can't see in the chaser list.
    if (instance.isStopped && orderIds.length > 0) {
      this.log(`isStopped after placeOrder; cancelling just-placed ${JSON.stringify(orderIds)}`, instance);
      for (const orderId of orderIds) {
        try {
          await exchange.cancelOrders([{ id: orderId, symbol: instance.options.symbol } as Order]);
        } catch {
          // already cancelled / filled / never made it — fine
        }
      }
      return [];
    }

    if (instance.isStopped) {
      return [];
    }

    if (orderIds.length > 0) {
      return orderIds;
    }

    instance.minTickDistance++;
    // Longer delay for Hyperliquid to allow rejection to be processed
    const retryDelay = exchange.name === 'hyperliquid' ? 500 : 100;
    await sleep(retryDelay);

    // Re-check after the retry sleep — user may have cancelled during it.
    if (instance.isStopped) {
      return [];
    }

    return this.tryPlaceOrders(instance, tries + 1);
  }

  private async processInstance(instance: ChaserOrderInstance) {
    const exchange = instance.exchange;

    this.log(`processInstance starting for ${instance.options.symbol}`, instance);
    instance.currentOrderIds = await this.tryPlaceOrders(instance);
    this.log(`initial orderIds: ${JSON.stringify(instance.currentOrderIds)}`, instance);
    await sleep(1000);

    if (instance.currentOrderIds.length > 0) {
      // Use fill-based tracking for exchanges where order status updates may be delayed
      // or missed during cancel/replace cycles (pendingOrdersUpdate race condition)
      const useFillBasedTracking = exchange.name === 'hyperliquid' || exchange.name === 'grvt';
      this.log(`monitoring started, useFillBasedTracking=${useFillBasedTracking}`, instance);
      let priceCheckTimeout: ReturnType<typeof setTimeout> | undefined;
      let isCleanedUp = false;
      let onUpdate: ((data: StoreData) => void | Promise<void>) | undefined;
      let onFill: ((fill: OrderFillEvent) => void) | undefined;
      // Track whether the current order IDs have appeared in the store. This
      // is intentionally reset after each replace, because new order IDs can
      // arrive in the private store after placeOrder returns.
      let hasSeenOrders = false;
      let queuedOrdersCheck = false;
      let runPriceCheck: ((orders: Order[]) => Promise<void>) | undefined;

      // Helper to cleanup all event subscriptions
      const cleanup = () => {
        if (isCleanedUp) {
          return;
        }

        isCleanedUp = true;
        if (priceCheckTimeout) {
          clearTimeout(priceCheckTimeout);
          priceCheckTimeout = undefined;
        }

        if (onUpdate) {
          exchange?.off('update', onUpdate);
        }
        if (useFillBasedTracking && onFill) {
          exchange?.off('fill', onFill);
        }
      };

      // Make `cleanup` reachable from `cleanupInstance` so a user STOP
      // detaches the WS listeners synchronously instead of waiting for
      // the next `'update'` event. The closure's `isCleanedUp` guard
      // keeps it idempotent if a queued event also calls it.
      instance.registerDisposer(cleanup);

      const applyOrderFillState = (orders: Order[]) => {
        for (const order of orders) {
          if (order.status === OrderStatus.Canceled && order.extra?.rejectReason === OrderRejectReason.PostOnly) {
            continue;
          } else {
            instance.subtractFilledSize(order);
          }
        }

        // If all orders are closed = all of them are filled, also finishing chaser
        if (orders.every((order) => order.status === OrderStatus.Closed)) {
          this.log('all orders are closed (executed) - marking ourselves completed', instance);
          instance.markComplete();
          cleanup();
          return true;
        }

        if (instance.isStopped) {
          cleanup();
          return true;
        }

        return false;
      };

      const updateOrders = async (orders: Order[]) => {
        if (instance.pendingOrdersUpdate) {
          queuedOrdersCheck = true;
          applyOrderFillState(orders);
          return;
        }

        instance.pendingOrdersUpdate = true;

        try {
          const orderPrice = orders.find((o) => o.status === OrderStatus.Open)?.price;
          const lastPrice = this.findBestPrice(instance);

          // check if reduce only and position is already gone
          if (instance.options.reduceOnly) {
            if (
              !exchange?.store.positions.find(
                (position) =>
                  position.symbol === instance.options.symbol &&
                  position.side ===
                    (instance.options.side === OrderSide.Buy ? PositionSide.Short : PositionSide.Long) &&
                  position.notional !== 0,
              )
            ) {
              instance.markComplete();
              cleanup();
              return;
            }
          }

          const isFilledOrStopped = applyOrderFillState(orders);

          if (instance.freshOrder) {
            instance.freshOrder = false;
            if (orders.every((order) => order.status === OrderStatus.Canceled)) {
              instance.minTickDistance++;
            }
          }

          if (isFilledOrStopped) {
            return;
          } else if (
            orderPrice &&
            ((instance.options.side === OrderSide.Buy && lastPrice < orderPrice) ||
              (instance.options.side === OrderSide.Sell && lastPrice > orderPrice))
          ) {
            // this.log('inner condition', instance);
            return;
          } else if (
            instance.lastPartialFillAt > 0 &&
            Date.now() - instance.lastPartialFillAt < CHASER_HOLD_STEADY_AFTER_PARTIAL_FILL_MS
          ) {
            // Order is currently filling — don't yank it out from under
            // an in-flight fill. See the constant's comment above for
            // the race this guards against.
            this.log(
              `hold steady: ${Date.now() - instance.lastPartialFillAt}ms since last partial fill`,
              instance,
            );
            return;
          } else if (orderPrice !== lastPrice) {
            const ordersToCancel = orders.filter(
              (order) => order.status !== OrderStatus.Closed && order.status !== OrderStatus.Canceled,
            );
            if (ordersToCancel.length > 0) {
              this.log('cancelling orders', instance);
              try {
                await exchange?.cancelOrders(ordersToCancel);
              } catch (cancelError: any) {
                // For Hyperliquid: "already canceled, or filled" is expected when order was filled
                // before we could cancel it - check fills via REST to confirm
                const errorMsg = String(cancelError?.message || cancelError || '');
                const isAlreadyFilledOrCanceled =
                  errorMsg.includes('already canceled') ||
                  errorMsg.includes('already filled') ||
                  errorMsg.includes('never placed');

                if (useFillBasedTracking && isAlreadyFilledOrCanceled) {
                  this.log(`cancel failed (order may be filled): ${errorMsg}`, instance);
                  // Don't stop - we'll check fills via REST below
                } else {
                  cleanup();
                  exchange?.emitter.emit('error', 'Chaser failed to cancel orders, stopping');
                  instance.markComplete('Failed to cancel orders');
                  this.log('failed to cancel orders', instance);
                  throw new Error('Failed to cancel orders');
                }
              }
            }

            // Fill-based exchanges must reconcile authoritatively before
            // replacement. Cancellation can have won even when its response is
            // ambiguous, so an unavailable or failed REST read is unsafe.
            if (useFillBasedTracking) {
              try {
                if (typeof (exchange as any).fetchFillsForOrderIds !== 'function') {
                  throw new Error('fill reconciliation is unavailable');
                }
                const fillsMap = await (exchange as any).fetchFillsForOrderIds(instance.currentOrderIds);
                for (const [orderId, filledAmount] of fillsMap) {
                  // Only update if we got more filled than we knew about
                  this.log(`REST fills check: orderId=${orderId}, filledAmount=${filledAmount}`, instance);
                  instance.setFilledAmount(orderId, filledAmount);
                }
                // Check if fully filled after REST update
                if (instance.isStopped) {
                  this.log('fully filled after REST check - marking ourselves completed', instance);
                  cleanup();
                  return;
                }
              } catch (e) {
                const message = `Chaser reconciliation failed after cancellation; stopped before replacement: ${e}`;
                this.log(message, instance);
                instance.markComplete('Failed to reconcile fills after cancellation');
                exchange?.emitter.emit('error', message);
                await this.cleanupInstance(instance);
                return;
              }
            }

            if (instance.isStopped || isCleanedUp) {
              return;
            }

            instance.currentOrderIds = await this.tryPlaceOrders(instance);
            hasSeenOrders = false;
            await sleep(1000);
            this.log('created new orders', instance);
          }
        } finally {
          instance.pendingOrdersUpdate = false;
          if (queuedOrdersCheck && !isCleanedUp && !instance.isStopped && runPriceCheck) {
            queuedOrdersCheck = false;
            await runPriceCheck(this.getCurrentOrders(instance));
          }
        }
      };

      let lastOrdersUpdateId = -1;
      let lastPriceCheckAt = 0;

      const handleTrackedOrders = async (orders: Order[]) => {
        if (orders.length > 0) {
          hasSeenOrders = true;
          await updateOrders(orders);
        }

        // Only unsub if we've previously seen our orders and now they're gone
        // This prevents race condition where orders haven't appeared in store yet
        if (!orders.length && !instance.pendingOrdersUpdate && hasSeenOrders) {
          this.log('unsubbing, orders no longer in store', instance);
          instance.markComplete();
          cleanup();
        }
      };

      const filterCurrentOrders = (orders: Order[]) =>
        orders.filter((order) => instance.currentOrderIds.includes(order.id));

      runPriceCheck = async (orders: Order[]) => {
        lastPriceCheckAt = Date.now();

        // `chase ... to <length>` termination — evaluate before any
        // cancel/replace work. If the live price has moved past the
        // user's threshold in the unfavorable direction, stop chasing
        // and let cleanupInstance cancel the resting order.
        if (instance.options.terminate) {
          const ticker =
            exchange?.store.tickersBySymbol[instance.options.symbol] ||
            exchange?.store.tickers.find((t) => t.symbol === instance.options.symbol);
          const livePrice = ticker?.last;
          if (livePrice && instance.shouldTerminate(livePrice)) {
            this.log(
              `terminating: live=${livePrice} breached ${instance.options.terminate.type}=${instance.options.terminate.value}`,
              instance,
            );
            instance.markComplete('Terminated by length condition');
            cleanup();
            await this.cleanupInstance(instance);
            return;
          }
        }

        await handleTrackedOrders(orders);
      };

      const scheduleTickerPriceCheck = () => {
        if (isCleanedUp || priceCheckTimeout) {
          return;
        }

        const wait = Math.max(CHASER_PRICE_CHECK_MIN_INTERVAL_MS - (Date.now() - lastPriceCheckAt), 0);
        priceCheckTimeout = setTimeout(async () => {
          priceCheckTimeout = undefined;

          if (instance.isStopped) {
            this.log('instance is stopped from outside, stopping queued price check', instance);
            instance.markComplete();
            cleanup();
            return;
          }

          await runPriceCheck(this.getCurrentOrders(instance));
        }, wait);
      };

      onUpdate = async (data: StoreData) => {
        if (instance.isStopped) {
          this.log('instance is stopped from outside, unsubbing', instance);
          instance.markComplete();
          cleanup();
          return;
        }

        const ordersUpdateId = data.updateIds?.orders;
        const hasOrderUpdate = ordersUpdateId === undefined || ordersUpdateId !== lastOrdersUpdateId;
        if (ordersUpdateId !== undefined) {
          lastOrdersUpdateId = ordersUpdateId;
        }

        if (hasOrderUpdate) {
          if (priceCheckTimeout) {
            clearTimeout(priceCheckTimeout);
            priceCheckTimeout = undefined;
          }
          await runPriceCheck(filterCurrentOrders(data.orders));
          return;
        }

        if (Date.now() - lastPriceCheckAt >= CHASER_PRICE_CHECK_MIN_INTERVAL_MS) {
          await runPriceCheck(filterCurrentOrders(data.orders));
          return;
        }

        scheduleTickerPriceCheck();
      };

      // Fill event handler - provides faster fill detection than order status updates
      onFill = (fill: OrderFillEvent) => {
        // Check if this fill belongs to one of our chaser orders
        if (!fill.orderId || !instance.currentOrderIds.includes(fill.orderId)) {
          return;
        }

        // Also verify it's for the correct symbol
        if (fill.symbol !== instance.options.symbol) {
          return;
        }

        this.log(`fill event received: orderId=${fill.orderId}, amount=${fill.amount}`, instance);

        // Track the fill amount
        instance.addFillAmount(fill.orderId, fill.amount);

        // Check if fully filled via fill events
        if (instance.isStopped) {
          this.log('fully filled via fill events - marking ourselves completed', instance);
          cleanup();
        }
      };

      exchange.on('update', onUpdate);
      // Only subscribe to fill events for exchanges that use fill-based tracking
      if (useFillBasedTracking) {
        exchange.on('fill', onFill);
      }
      return;
    }

    // We didn't manage to even start a chaser, marking complete
    instance.markComplete();
    // cleanup routine
    await this.cleanupInstance(instance);
  }

  checkAndMarkIfComplete(instance: ChaserOrderInstance) {
    const exchange = instance.exchange;
    if (instance.options.reduceOnly) {
      if (
        !exchange.store.positions.find(
          (position) =>
            position.symbol === instance.options.symbol &&
            position.side === (instance.options.side === OrderSide.Buy ? PositionSide.Short : PositionSide.Long) &&
            position.notional !== 0,
        )
      ) {
        instance.markComplete();
        return;
      }
    }
    if (Boolean(instance.currentOrderIds?.length)) {
      const myOrders = this.getCurrentOrders(instance);
      this.log(`my order ids ${myOrders.map((order) => `${order.id}:${order.status}`).join(',')}`, instance);
      if (myOrders?.length && myOrders.every((order) => order.status === OrderStatus.Closed)) {
        instance.markComplete();
        instance.currentOrderIds = [];
        return;
      }
    }
  }

  async cleanupInstance(instance: ChaserOrderInstance) {
    const exchange = instance.exchange;

    // Detach the supervisor's event listeners and pending timeouts up
    // front, before we start awaiting cancels. Without this, an
    // in-flight `onUpdate` could pass its `isStopped` check on a
    // stale microtask and still call `tryPlaceOrders` between our
    // awaits below — the order would land in the book after this
    // function returned. `tryPlaceOrders` also now guards against
    // this, but detaching here is the cheaper belt-and-braces fix.
    instance.dispose();

    if (instance.currentOrderIds?.length) {
      // Snapshot the IDs and clear synchronously so an in-flight
      // `updateOrders` that reassigns `currentOrderIds` after our
      // await doesn't leak a fresh order past us. We cancel by ID
      // rather than filtering `exchange.store.orders` because a
      // freshly-placed order may not be in the store yet (WS lag) —
      // the old filter-then-cancel form would skip it and the user
      // would see an "orphan" resting order after STOP.
      const idsToCancel = [...instance.currentOrderIds];
      instance.currentOrderIds = [];
      try {
        for (const orderId of idsToCancel) {
          const order =
            exchange.store.orders.find((o) => o.id === orderId) ||
            ({ id: orderId, symbol: instance.options.symbol } as Order);
          try {
            await exchange.cancelOrders([order]);
          } catch {
            // already cancelled / filled / never placed — fine
          }
        }
      } catch {
        // never trip the splice below on a cancel-loop error
      }
    }
    // Remove instance from array to prevent memory leak
    const index = this._chaserOrderInstances.indexOf(instance);
    if (index > -1) {
      this._chaserOrderInstances.splice(index, 1);
    }
  }

  remove(id: string) {
    const instance = this._chaserOrderInstances.find((instance) => instance.id === id);
    if (instance) {
      instance.markComplete('User cancelled');
      this.checkAndMarkIfComplete(instance);
      this.cleanupInstance(instance);
    }
  }

  /**
   * Like `remove`, but leaves any current open order alive in the book.
   * Used for `chase cancel keep` from the CLI — stops the supervisor
   * loop without yanking the resting order.
   */
  removeKeepOrder(id: string) {
    const instance = this._chaserOrderInstances.find((instance) => instance.id === id);
    if (!instance) return;
    instance.markComplete('User cancelled, kept order in book');
    // Clear currentOrderIds BEFORE the array splice so any concurrent
    // pass through cleanupInstance won't see them; we deliberately
    // don't call cleanupInstance here (it would cancel them).
    instance.currentOrderIds = [];
    // Detach the supervisor's WS listeners synchronously — same
    // reason as `cleanupInstance` does it. Without this, the
    // listeners stay subscribed until the next `'update'` event
    // arrives (which then no-ops via the `isStopped` guard). That
    // window leaks if the exchange disconnects before the next
    // tick fires, and it leaves room for an in-flight `updateOrders`
    // to keep running past STOP.
    instance.dispose();
    const index = this._chaserOrderInstances.indexOf(instance);
    if (index > -1) this._chaserOrderInstances.splice(index, 1);
  }

  get instances() {
    return this._chaserOrderInstances;
  }
}

export const chaserOrderManagerInstance = new ChaserOrderManager();
