import { OrderStopTriggerType, OrderTimeInForce } from '../../../types';

type GrvtOrderLike = Record<string, any>;

export type GrvtTriggerMetadata = {
  readonly type: 'STOP_LOSS' | 'TAKE_PROFIT';
  readonly triggerPrice?: number;
  readonly triggerType?: OrderStopTriggerType;
  readonly close?: boolean;
  readonly isSplitPosition?: boolean;
};

/**
 * GRVT represents post-only as a separate boolean, whereas safe-cex exposes it
 * as a TIF. Do not default an unknown upstream TIF: cancel-and-replace must
 * refuse an order whose execution semantics cannot be reconstructed.
 */
export const getGrvtOrderTimeInForce = (order: GrvtOrderLike): OrderTimeInForce | undefined => {
  const postOnly = order.post_only ?? order.po;
  if (postOnly !== undefined && typeof postOnly !== 'boolean') return undefined;

  switch (order.time_in_force ?? order.ti) {
    case 'GOOD_TILL_TIME':
      return postOnly ? OrderTimeInForce.PostOnly : OrderTimeInForce.GoodTillCancel;
    case 'IMMEDIATE_OR_CANCEL':
      return postOnly ? undefined : OrderTimeInForce.ImmediateOrCancel;
    case 'FILL_OR_KILL':
      return postOnly ? undefined : OrderTimeInForce.FillOrKill;
    default:
      // ALL_OR_NONE and RETAIL_PRICE_IMPROVEMENT have no safe-cex
      // representation. Do not collapse either into PostOnly before a
      // cancel-and-replace.
      return undefined;
  }
};

export const getGrvtTriggerMetadata = (order: GrvtOrderLike): GrvtTriggerMetadata | undefined => {
  const metadata = order.metadata ?? order.m;
  const trigger = metadata?.trigger ?? metadata?.t;
  if (!trigger) return undefined;

  const type = trigger.trigger_type ?? trigger.tt;
  if (type !== 'STOP_LOSS' && type !== 'TAKE_PROFIT') return undefined;

  const tpsl = trigger.tpsl ?? trigger.t;
  const rawTriggerPrice = tpsl?.trigger_price ?? tpsl?.tp;
  const parsedTriggerPrice = Number(rawTriggerPrice);
  const triggerPrice = Number.isFinite(parsedTriggerPrice) ? parsedTriggerPrice : undefined;

  let triggerType: OrderStopTriggerType | undefined;
  switch (tpsl?.trigger_by ?? tpsl?.tb) {
    case 'LAST':
      triggerType = OrderStopTriggerType.LastPrice;
      break;
    case 'MARK':
      triggerType = OrderStopTriggerType.MarkPrice;
      break;
    case 'INDEX':
      triggerType = OrderStopTriggerType.IndexPrice;
      break;
  }

  const rawClose = tpsl?.close_position ?? tpsl?.cp;
  const rawIsSplitPosition = tpsl?.is_split_position ?? tpsl?.is;
  return {
    type,
    triggerPrice,
    triggerType,
    ...(typeof rawClose === 'boolean' ? { close: rawClose } : {}),
    ...(typeof rawIsSplitPosition === 'boolean' ? { isSplitPosition: rawIsSplitPosition } : {}),
  };
};
