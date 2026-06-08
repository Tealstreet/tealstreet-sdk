import type { Exchange } from '../../exchanges/base';
import type { PlaceOrderOpts } from '../../types';
import type { CrossTradeCopier } from '../types';
import type { CrossTradePlaceOrderFn } from './CrossTradeManager';

import { DEFAULT_PRICE_PRECISION } from '@tealstreet/utils/src/currency/currencyUtils';
import { OrderSide, OrderType } from '../../types';
import { adjust } from '../../utils/safe-math';

const debug = (...args: any[]) => console.log('[CrossTradeManager]', ...args);

const SLIPPAGE_LIMIT = 0.009;

export interface CrossTradePlaceOrderDeps {
  getExchange: (account: string) => Exchange | undefined;
}

export const createCrossTradePlaceOrder = (deps: CrossTradePlaceOrderDeps): CrossTradePlaceOrderFn => {
  return async (copier, market, referencePrice, account, side, amount, reduceOnly) => {
    const symbol = market.symbol;
    const exchange = deps.getExchange(account);
    if (!exchange) return;

    if (copier.execution === 'Chase') {
      // Scan only the copier's own exchange — a duplicate-chaser check
      // across master/other copier accounts would let an unrelated chaser
      // on the same symbol/side suppress this copier's cross-trade chaser.
      // Matches the effective behavior of the old webworker path under
      // dedicated-worker mode (the dominant configuration), where
      // exchangeWorkers held only the local account.
      const activeChasers = (exchange.store?.activeChasers ?? []).filter(
        (chaser) => !chaser.isStopped && chaser.symbol === symbol && chaser.side === side,
      );
      if (activeChasers.length > 1) {
        debug('Chaser already active for', symbol, side);
        return;
      }
      if (activeChasers.length === 1) {
        const instance = activeChasers[0]!;
        const chaseSize = instance.size;
        if (chaseSize === amount) {
          debug('Chaser already active for', symbol, side);
          return;
        }
        exchange.emitter.emit('chaser:terminate', instance.id);
      }

      if (!amount) {
        debug('Amount is 0 for', market.symbol);
        return;
      }

      exchange.emitter.emit('chaser:start', {
        symbol,
        size: amount,
        side,
        maxAttempts: 1000,
        reduceOnly,
        delay: 1000,
      });
      return;
    }

    if (copier.execution === 'LimitSlippage') {
      if (!referencePrice) {
        debug('No reference price for', symbol);
        return;
      }
      if (!amount) {
        debug('Amount is 0 for', market.symbol);
        return;
      }
      const maxSlipPrice =
        side === OrderSide.Buy ? referencePrice * (1 + SLIPPAGE_LIMIT) : referencePrice * (1 - SLIPPAGE_LIMIT);
      const adjustedPrice = adjust(maxSlipPrice, market.precision.price || DEFAULT_PRICE_PRECISION);
      const order: PlaceOrderOpts = {
        symbol,
        side,
        type: OrderType.Limit,
        price: adjustedPrice,
        reduceOnly,
        amount,
      };

      try {
        return await exchange.placeOrder(order);
      } catch (e) {
        const message = e instanceof Error ? e.message : String(e);
        console.error('CrossTradeManager', 'Failed to place order', e);
        exchange.emitter.emit('warning', `Cross Trade: failed to place limit order on ${symbol}: ${message}`);
        throw e;
      }
    }

    if (copier.execution === 'Market') {
      if (!amount) {
        debug('Amount is 0 for', market.symbol);
        return;
      }
      const order: PlaceOrderOpts = {
        symbol,
        side,
        type: OrderType.Market,
        reduceOnly,
        amount,
      };

      try {
        return await exchange.placeOrder(order);
      } catch (e) {
        const message = e instanceof Error ? e.message : String(e);
        console.error('CrossTradeManager', 'Failed to place order', e);
        exchange.emitter.emit('warning', `Cross Trade: failed to place market order on ${symbol}: ${message}`);
        throw e;
      }
    }

    debug('Unknown execution type', (copier as CrossTradeCopier).execution);
    return;
  };
};
