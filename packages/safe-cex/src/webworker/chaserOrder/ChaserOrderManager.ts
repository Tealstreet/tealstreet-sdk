import type { Exchange } from '../../exchanges/base';

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
const CHASER_MISSING_ORDER_CONFIRMATIONS = 2;
const CHASER_PLACED_ORDER_VISIBILITY_ATTEMPTS = 6;
const CHASER_PLACED_ORDER_VISIBILITY_INTERVAL_MS = 500;

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
    void this.processInstance(instance).catch((error) => this.handleSupervisorError(instance, error, 'startup'));
  }

  private async handleSupervisorError(instance: ChaserOrderInstance, error: unknown, phase: string) {
    const detail = error instanceof Error ? error.message : String(error);
    const message = `Chaser supervisor failed during ${phase} for ${instance.options.symbol}: ${detail}`;
    this.log(message, instance);
    if (!instance.isStopped) {
      instance.markComplete(`Chaser ${phase} failed`);
    }
    instance.exchange.emitter.emit('error', message);
    await this.cleanupInstance(instance);
  }

  private getCurrentOrders(instance: ChaserOrderInstance) {
    return instance.exchange.store.orders.filter((order) => instance.currentOrderIds.includes(order.id)) || [];
  }

  private getErrorDetail(error: unknown) {
    return error instanceof Error ? error.message : String(error);
  }

  private async refreshPrivateSnapshotForTrackedOrders(instance: ChaserOrderInstance, attempt: number) {
    try {
      this.log(
        `tracked orders not visible after placement; refreshing private snapshot (${attempt}/${CHASER_PLACED_ORDER_VISIBILITY_ATTEMPTS})`,
        instance,
      );
      await instance.exchange.fetchAndSetPrivateSnapshot();
    } catch (error) {
      this.log(`tracked-order snapshot refresh failed: ${this.getErrorDetail(error)}`, instance);
    }
  }

  private async waitForPlacedOrdersToAppear(instance: ChaserOrderInstance) {
    let orders = this.getCurrentOrders(instance);
    if (orders.length > 0 || instance.currentOrderIds.length === 0) {
      return orders;
    }

    for (let attempt = 1; attempt <= CHASER_PLACED_ORDER_VISIBILITY_ATTEMPTS; attempt++) {
      if (instance.isStopped || instance.currentOrderIds.length === 0) {
        return [];
      }

      await this.refreshPrivateSnapshotForTrackedOrders(instance, attempt);
      orders = this.getCurrentOrders(instance);
      if (orders.length > 0) {
        return orders;
      }

      if (attempt < CHASER_PLACED_ORDER_VISIBILITY_ATTEMPTS) {
        await sleep(CHASER_PLACED_ORDER_VISIBILITY_INTERVAL_MS);
      }
    }

    return [];
  }

  private async confirmPlacedOrdersOrStop(instance: ChaserOrderInstance, phase: string) {
    await sleep(1000);

    if (instance.isStopped) {
      await this.cleanupInstance(instance);
      return [];
    }

    const orders = await this.waitForPlacedOrdersToAppear(instance);
    if (orders.length > 0 || instance.isStopped) {
      return orders;
    }

    this.log(`placed orders never appeared during ${phase}; stopping safely`, instance);
    instance.exchange.emitter.emit(
      'warning',
      `Chaser for ${instance.options.symbol} stopped: placed order IDs never appeared in the local order book; any resting remainder was cancelled`,
    );
    instance.markComplete('Placed orders did not appear in store');
    await this.cleanupInstance(instance);
    return [];
  }

  private hasReachedTargetPosition(instance: ChaserOrderInstance) {
    if (
      instance.exchange.name !== 'bybitdemo' ||
      instance.options.reduceOnly ||
      instance.targetPositionContracts === undefined
    ) {
      return false;
    }

    const targetSide = instance.options.side === OrderSide.Buy ? PositionSide.Long : PositionSide.Short;
    const position = (instance.exchange.store.positions || []).find(
      (p) => p.symbol === instance.options.symbol && p.side === targetSide,
    );
    const contracts = Math.abs(position?.contracts || 0);
    return contracts >= instance.targetPositionContracts - 1e-12;
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

    if (instance.currentOrderIds.length > 0) {
      const initialOrders = await this.confirmPlacedOrdersOrStop(instance, 'startup');
      if (instance.isStopped) {
        return;
      }

      // Use fill-based tracking for exchanges where order status updates may be delayed
      // or missed during cancel/replace cycles (pendingOrdersUpdate race condition)
      const useFillBasedTracking =
        exchange.name === 'hyperliquid' || exchange.name === 'grvt' || exchange.name === 'bybitdemo';
      this.log(`monitoring started, useFillBasedTracking=${useFillBasedTracking}`, instance);
      let priceCheckTimeout: ReturnType<typeof setTimeout> | undefined;
      let queuedOrdersTimeout: ReturnType<typeof setTimeout> | undefined;
      let isCleanedUp = false;
      let onUpdate: ((data: StoreData) => void | Promise<void>) | undefined;
      let onFill: ((fill: OrderFillEvent) => void) | undefined;
      // Track whether the current order IDs have appeared in the store. This
      // is intentionally reset after each replace, because new order IDs can
      // arrive in the private store after placeOrder returns.
      let hasSeenOrders = initialOrders.length > 0;
      let consecutiveMissingOrderUpdates = 0;
      let queuedOrdersCheck = false;
      let runPriceCheck: ((orders: Order[], confirmMissingOrders?: boolean) => Promise<void>) | undefined;
      let scheduleQueuedOrdersCheck: (() => void) | undefined;

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
        if (queuedOrdersTimeout) {
          clearTimeout(queuedOrdersTimeout);
          queuedOrdersTimeout = undefined;
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

      const applyOrderFillState = async (orders: Order[]) => {
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
          await this.cleanupInstance(instance);
          return true;
        }

        if (instance.isStopped) {
          cleanup();
          await this.cleanupInstance(instance);
          return true;
        }

        return false;
      };

      const updateOrders = async (orders: Order[]) => {
        if (instance.pendingOrdersUpdate) {
          queuedOrdersCheck = true;
          await applyOrderFillState(orders);
          return;
        }

        instance.pendingOrdersUpdate = true;

        try {
          const orderPrice = orders.find((o) => o.status === OrderStatus.Open)?.price;
          const lastPrice = this.findBestPrice(instance);

          // check if reduce only and position is already gone
          if (instance.options.reduceOnly) {
            if (
              !(exchange?.store.positions || []).find(
                (position) =>
                  position.symbol === instance.options.symbol &&
                  position.side ===
                    (instance.options.side === OrderSide.Buy ? PositionSide.Short : PositionSide.Long) &&
                  position.notional !== 0,
              )
            ) {
              instance.markComplete();
              cleanup();
              await this.cleanupInstance(instance);
              return;
            }
          }

          const isFilledOrStopped = await applyOrderFillState(orders);

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
            this.log(`hold steady: ${Date.now() - instance.lastPartialFillAt}ms since last partial fill`, instance);
            return;
          } else if (orderPrice !== lastPrice) {
            instance.incrementAttempts();
            if (instance.isStopped) {
              cleanup();
              await this.cleanupInstance(instance);
              return;
            }

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
            const replacementOrders = await this.confirmPlacedOrdersOrStop(instance, 'replacement');
            if (instance.isStopped || isCleanedUp) {
              return;
            }
            hasSeenOrders = replacementOrders.length > 0;
            this.log('created new orders', instance);
          }
        } finally {
          instance.pendingOrdersUpdate = false;
          if (queuedOrdersCheck && !isCleanedUp && !instance.isStopped) {
            queuedOrdersCheck = false;
            scheduleQueuedOrdersCheck?.();
          }
        }
      };

      let lastOrdersUpdateId = -1;
      let lastPositionsUpdateId = -1;
      let lastPriceCheckAt = 0;

      const handleTrackedOrders = async (orders: Order[], confirmMissingOrders: boolean) => {
        if (orders.length > 0) {
          hasSeenOrders = true;
          consecutiveMissingOrderUpdates = 0;
          await updateOrders(orders);
        }

        // A wholesale REST snapshot can have been fetched before the chaser's
        // WS order frame and applied after it, making a live resting order
        // disappear for one updateId. Require two distinct order mutations to
        // agree that the order is gone; ticker-only notifies do not count.
        if (!orders.length && !instance.pendingOrdersUpdate && hasSeenOrders && confirmMissingOrders) {
          consecutiveMissingOrderUpdates += 1;
          if (consecutiveMissingOrderUpdates < CHASER_MISSING_ORDER_CONFIRMATIONS) {
            this.log('tracked orders missing once; waiting for confirmation', instance);
            return;
          }

          this.log('tracked orders absent across confirmed order updates; stopping safely', instance);
          instance.markComplete('Tracked orders disappeared from store');
          // This stop used to be silent, which hid a real bug for weeks (Bybit
          // demo REST snapshots wiping just-placed orders — the chaser died and
          // cancelled its own order with no user-visible trace). Any resting
          // remainder is cancelled by cleanupInstance below, so tell the user.
          exchange?.emitter.emit(
            'warning',
            `Chaser for ${instance.options.symbol} stopped: its order left the order book (filled or cancelled externally); any resting remainder was cancelled`,
          );
          cleanup();
          await this.cleanupInstance(instance);
        }
      };

      const filterCurrentOrders = (orders: Order[]) =>
        orders.filter((order) => instance.currentOrderIds.includes(order.id));

      runPriceCheck = async (orders: Order[], confirmMissingOrders = false) => {
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

        if (this.hasReachedTargetPosition(instance)) {
          this.log('target position reached - marking ourselves completed', instance);
          instance.markComplete();
          cleanup();
          await this.cleanupInstance(instance);
          return;
        }

        await handleTrackedOrders(orders, confirmMissingOrders);
      };

      // Updates that arrive while a cancel/reprice pass is awaiting network I/O
      // still apply fill state immediately, but their follow-up reprice is one
      // bounded trailing check. Calling runPriceCheck recursively from finally
      // allowed an order-update storm to chain cancel/replace passes forever.
      scheduleQueuedOrdersCheck = () => {
        if (isCleanedUp || queuedOrdersTimeout) return;
        queuedOrdersTimeout = setTimeout(async () => {
          queuedOrdersTimeout = undefined;
          if (isCleanedUp || instance.isStopped) return;
          try {
            await runPriceCheck!(this.getCurrentOrders(instance));
          } catch (error) {
            await this.handleSupervisorError(instance, error, 'queued order update');
          }
        }, CHASER_PRICE_CHECK_MIN_INTERVAL_MS);
      };

      const scheduleTickerPriceCheck = () => {
        if (isCleanedUp || priceCheckTimeout) {
          return;
        }

        const wait = Math.max(CHASER_PRICE_CHECK_MIN_INTERVAL_MS - (Date.now() - lastPriceCheckAt), 0);
        priceCheckTimeout = setTimeout(async () => {
          priceCheckTimeout = undefined;
          try {
            if (instance.isStopped) {
              this.log('instance is stopped from outside, stopping queued price check', instance);
              instance.markComplete();
              cleanup();
              await this.cleanupInstance(instance);
              return;
            }

            await runPriceCheck(this.getCurrentOrders(instance));
          } catch (error) {
            await this.handleSupervisorError(instance, error, 'queued price update');
          }
        }, wait);
      };

      onUpdate = async (data: StoreData) => {
        try {
          if (instance.isStopped) {
            this.log('instance is stopped from outside, unsubbing', instance);
            instance.markComplete();
            cleanup();
            await this.cleanupInstance(instance);
            return;
          }

          const ordersUpdateId = data.updateIds?.orders;
          const hasOrderUpdate = ordersUpdateId === undefined || ordersUpdateId !== lastOrdersUpdateId;
          if (ordersUpdateId !== undefined) {
            lastOrdersUpdateId = ordersUpdateId;
          }
          const positionsUpdateId = data.updateIds?.positions;
          const hasPositionUpdate = positionsUpdateId === undefined || positionsUpdateId !== lastPositionsUpdateId;
          if (positionsUpdateId !== undefined) {
            lastPositionsUpdateId = positionsUpdateId;
          }

          if (hasOrderUpdate || hasPositionUpdate) {
            if (priceCheckTimeout) {
              clearTimeout(priceCheckTimeout);
              priceCheckTimeout = undefined;
            }
            if (queuedOrdersTimeout) {
              clearTimeout(queuedOrdersTimeout);
              queuedOrdersTimeout = undefined;
            }
            await runPriceCheck(filterCurrentOrders(data.orders), hasOrderUpdate);
            return;
          }

          if (Date.now() - lastPriceCheckAt >= CHASER_PRICE_CHECK_MIN_INTERVAL_MS) {
            await runPriceCheck(filterCurrentOrders(data.orders));
            return;
          }

          scheduleTickerPriceCheck();
        } catch (error) {
          await this.handleSupervisorError(instance, error, 'store update');
        }
      };

      // Fill event handler - provides faster fill detection than order status updates
      onFill = (fill: OrderFillEvent) => {
        try {
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
            void this.cleanupInstance(instance);
          }
        } catch (error) {
          void this.handleSupervisorError(instance, error, 'fill update');
        }
      };

      exchange.on('update', onUpdate);
      // Only subscribe to fill events for exchanges that use fill-based tracking
      if (useFillBasedTracking) {
        exchange.on('fill', onFill);
      }
      return;
    }

    // We didn't manage to even start a chaser. `tryPlaceOrders` returns []
    // for user-stop and after its own error-emitting failure paths (which
    // already set a stop reason) — but if we get here with the instance NOT
    // stopped, the chaser is dying without anything having told the user.
    // That silent vanish is exactly how the Bybit demo chaser bug went
    // unreported; keep this path loud.
    if (!instance.isStopped) {
      exchange.emitter.emit('warning', `Chaser for ${instance.options.symbol} could not start — no order was placed`);
      instance.markComplete('No order placed');
    }
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

    // Stop exposing and retaining the instance before any remote cancellation
    // await. A slow or hung exchange request must not keep a disposed account
    // reachable through the module singleton.
    const index = this._chaserOrderInstances.indexOf(instance);
    if (index > -1) {
      this._chaserOrderInstances.splice(index, 1);
    }

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
  }

  cleanupExchange(exchange: Exchange, reason = 'Exchange disposed while chase was active') {
    const instances = this._chaserOrderInstances.filter((instance) => instance.exchange === exchange);
    for (const instance of instances) {
      if (!instance.isStopped) {
        instance.markComplete(reason);
        exchange.emitter.emit(
          'warning',
          `Chaser for ${instance.options.symbol} was stopped because the exchange connection was disposed`,
        );
      }
      // cleanupInstance detaches and splices synchronously before its first
      // cancellation await, so callers can dispose the exchange immediately.
      void this.cleanupInstance(instance);
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
