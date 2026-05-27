import { CrossTradeCopier } from '../../crossTrade/types';
import { Market, OrderSide, OrderType, type PlaceOrderOpts } from '../../types';
import { DEFAULT_PRICE_PRECISION } from '@tealstreet/utils/src/currency/currencyUtils';
import { adjust } from '../../utils/safe-math';
import { exchangeWorkers } from '../store.webworker';

const debug = (...args: any[]) => console.log('[CrossTradeManager]', ...args);

// TODO: Make this configurable
const SLIPPAGE_LIMIT = 0.009;

export const crossTradePlaceOrder = async (
  copier: CrossTradeCopier,
  market: Market,
  referencePrice: number | undefined,
  account: string,
  side: OrderSide,
  amount: number,
  reduceOnly: boolean,
) => {
  const symbol = market.symbol;
  const exchangeWorker = exchangeWorkers[account];
  if (!exchangeWorker) return;
  const exchange = exchangeWorker.exchange;
  if (!exchange) return;
  if (copier.execution === 'Chase') {
    const activeChasers =
      Object.values(exchangeWorkers).flatMap((exchangeWorker) =>
        exchangeWorker?.exchange?.store?.activeChasers?.filter(
          (chaser) => !chaser.isStopped && chaser.symbol === symbol && chaser.side === side,
        ),
      ) || [];
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
      } else {
        exchangeWorker.handlePayload({ op: 'chaser:terminate', data: instance.id });
      }
    }

    if (!amount) {
      debug('Amount is 0 for', market.symbol);
      return;
    }

    exchangeWorker.handlePayload({
      op: 'chaser:start',
      data: { symbol, size: amount, side, maxAttempts: 1000, reduceOnly, delay: 1000 },
    });
  } else if (copier.execution === 'LimitSlippage') {
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
    let order: PlaceOrderOpts = {
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
      console.error('CrossTradeManager', 'Failed to place order', e);
    }
  } else if (copier.execution === 'Market') {
    if (!amount) {
      debug('Amount is 0 for', market.symbol);
      return;
    }
    let order: PlaceOrderOpts = {
      symbol,
      side,
      type: OrderType.Market,
      reduceOnly,
      amount,
    };

    try {
      return await exchange.placeOrder(order);
    } catch (e) {
      console.error('CrossTradeManager', 'Failed to place order', e);
    }
  } else {
    debug('Unknown execution type', copier.execution);
  }
};
