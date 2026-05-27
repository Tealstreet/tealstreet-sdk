import { OrderTimeInForce, OrderType } from '../../types';

type HyperliquidOrderLike = {
  orderType?: unknown;
  triggerPx?: unknown;
  tpsl?: unknown;
  limitPx?: unknown;
  tif?: unknown;
};

export type HyperliquidOrderInferenceSource =
  | 'orderType=Limit'
  | 'orderType=TP'
  | 'orderType=SL'
  | 'triggerPx+tpsl=tp'
  | 'triggerPx+tpsl=sl'
  | 'triggerPx-only'
  | 'no-orderType-fallback'
  | 'fallback-market';

export type HyperliquidOrderInferenceResult = {
  type: OrderType;
  price: number;
  source: HyperliquidOrderInferenceSource;
};

const toNumberOrZero = (value: unknown): number => {
  const n = Number.parseFloat(String(value ?? ''));
  return Number.isFinite(n) ? n : 0;
};

export const inferHyperliquidOrderTypeAndPrice = (
  order: HyperliquidOrderLike,
): HyperliquidOrderInferenceResult => {
  const orderType = String(order.orderType || '');
  const triggerPx = order.triggerPx;
  const tpsl = String(order.tpsl || '');
  const limitPx = order.limitPx;

  if (orderType === 'Limit') {
    return {
      type: OrderType.Limit,
      price: toNumberOrZero(limitPx),
      source: 'orderType=Limit',
    };
  }

  if (orderType === 'Take Profit Market' || (triggerPx && tpsl === 'tp')) {
    return {
      type: OrderType.TakeProfit,
      price: toNumberOrZero(triggerPx),
      source: orderType ? 'orderType=TP' : 'triggerPx+tpsl=tp',
    };
  }

  if (orderType === 'Stop Market' || (triggerPx && tpsl === 'sl')) {
    return {
      type: OrderType.StopLoss,
      price: toNumberOrZero(triggerPx),
      source: orderType ? 'orderType=SL' : 'triggerPx+tpsl=sl',
    };
  }

  if (triggerPx) {
    return {
      type: OrderType.StopLoss,
      price: toNumberOrZero(triggerPx),
      source: 'triggerPx-only',
    };
  }

  if (limitPx) {
    return {
      type: OrderType.Limit,
      price: toNumberOrZero(limitPx),
      source: 'no-orderType-fallback',
    };
  }

  return {
    type: OrderType.Market,
    price: 0,
    source: 'fallback-market',
  };
};

export const parseHyperliquidOrderTimeInForce = (tif: unknown): OrderTimeInForce | undefined => {
  if (tif === 'Alo') return OrderTimeInForce.PostOnly;
  if (tif === 'Gtc') return OrderTimeInForce.GoodTillCancel;
  return undefined;
};
