import type { Market, Order } from '../../../types';
import type { LIST_ORDERS_RESPONSE } from './coinbase.types';

import { OrderSide, OrderStatus, OrderType } from '../../../types';
import { add, divide, multiply, subtract } from '../../../utils/safe-math';
import { normalizeCfmContractSize } from './coinbase.utils';

type CoinbaseRestOrder = LIST_ORDERS_RESPONSE['orders'][number];

type ResolvedOrderConfig = {
  type: OrderType;
  baseSize: string | undefined;
  quoteSize: string | undefined;
  price: string | undefined;
};

const ORDER_STATUS: Partial<Record<CoinbaseRestOrder['status'], OrderStatus>> = {
  PENDING: OrderStatus.Open,
  OPEN: OrderStatus.Open,
  QUEUED: OrderStatus.Open,
  CANCEL_QUEUED: OrderStatus.Open,
  EDIT_QUEUED: OrderStatus.Open,
  FILLED: OrderStatus.Closed,
  CANCELLED: OrderStatus.Canceled,
  EXPIRED: OrderStatus.Canceled,
  FAILED: OrderStatus.Canceled,
};

const parseUnsignedDecimal = (value: string | undefined): number | undefined => {
  if (typeof value !== 'string' || value === '') return undefined;
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed >= 0 ? parsed : undefined;
};

const resolveOrderConfiguration = (order: CoinbaseRestOrder): ResolvedOrderConfig | undefined => {
  const config = order.order_configuration;
  if (!config) return undefined;

  const market = config.market_market_ioc || config.market_market_fok;
  if (market) {
    return {
      type: OrderType.Market,
      baseSize: market.base_size,
      quoteSize: market.quote_size,
      price: order.average_filled_price,
    };
  }

  const limit = config.limit_limit_gtc || config.limit_limit_gtd || config.limit_limit_fok || config.sor_limit_ioc;
  if (limit) {
    return {
      type: OrderType.Limit,
      baseSize: limit.base_size,
      quoteSize: limit.quote_size,
      price: limit.limit_price,
    };
  }

  const stopLimit = config.stop_limit_stop_limit_gtc || config.stop_limit_stop_limit_gtd;
  if (stopLimit) {
    return {
      type: OrderType.StopLoss,
      baseSize: stopLimit.base_size,
      quoteSize: undefined,
      price: stopLimit.stop_price || stopLimit.limit_price,
    };
  }

  return undefined;
};

const resolveAmountNative = ({ baseSize, quoteSize, price }: ResolvedOrderConfig): number | undefined => {
  const amountNative = parseUnsignedDecimal(baseSize);
  if (amountNative !== undefined) return amountNative;

  const quoteAmount = parseUnsignedDecimal(quoteSize);
  const orderPrice = parseUnsignedDecimal(price);
  if (quoteAmount === undefined || orderPrice === undefined || orderPrice === 0) return undefined;

  return divide(quoteAmount, orderPrice);
};

export const mapCoinbaseRestOrder = (
  rawOrder: CoinbaseRestOrder,
  market: Market,
  lastUpdate: number,
): Order | undefined => {
  const resolved = resolveOrderConfiguration(rawOrder);
  if (!resolved) return undefined;

  const amountNative = resolveAmountNative(resolved);
  const filledNative = parseUnsignedDecimal(rawOrder.filled_size) ?? 0;
  if (amountNative === undefined || amountNative <= 0 || filledNative > amountNative) return undefined;

  const contractSize = market.isExpiring ? normalizeCfmContractSize(market.contractSize) : 1;
  if (contractSize === undefined) return undefined;

  const status = ORDER_STATUS[rawOrder.status];
  if (!status) return undefined;

  const remainingNative = subtract(amountNative, filledNative);
  const filled = multiply(filledNative, contractSize);
  const remaining = multiply(remainingNative, contractSize);

  return {
    id: rawOrder.order_id,
    parentId: rawOrder.client_order_id,
    status,
    symbol: market.symbol,
    type: resolved.type,
    side: rawOrder.side === 'BUY' ? OrderSide.Buy : OrderSide.Sell,
    price: parseUnsignedDecimal(resolved.price) ?? 0,
    amount: add(filled, remaining),
    filled,
    remaining,
    extra: {
      remainingNative,
    },
    reduceOnly: false,
    info: rawOrder,
    lastUpdate,
  };
};
