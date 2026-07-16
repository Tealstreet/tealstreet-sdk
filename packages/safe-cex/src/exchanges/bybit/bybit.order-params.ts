import type { PlaceOrderOpts } from '../../types';

import { OrderTimeInForce, OrderType } from '../../types';

const hasAtMostTwoDecimals = (value: number) => Math.abs(value * 100 - Math.round(value * 100)) < 1e-9;

export type BybitBboSideType = 'Queue' | 'Counterparty';
export type BybitBboLevel = 1 | 2 | 3 | 4 | 5;

export type BybitBboOrderOpts = Omit<
  PlaceOrderOpts,
  'maxSlippagePct' | 'price' | 'stopLoss' | 'takeProfit' | 'timeInForce' | 'triggerPrice' | 'type'
> & {
  readonly type: OrderType.Limit;
  readonly price?: never;
  readonly triggerPrice?: never;
  readonly stopLoss?: never;
  readonly takeProfit?: never;
  readonly maxSlippagePct?: never;
  readonly timeInForce?: OrderTimeInForce.GoodTillCancel;
  readonly bybitBbo: {
    readonly sideType: BybitBboSideType;
    readonly level: BybitBboLevel;
  };
  readonly bybitSlippageToleranceTicks?: never;
};

export type BybitTickSizeMarketOrderOpts = Omit<
  PlaceOrderOpts,
  'maxSlippagePct' | 'price' | 'stopLoss' | 'takeProfit' | 'triggerPrice' | 'type'
> & {
  readonly type: OrderType.Market;
  readonly price?: never;
  readonly triggerPrice?: never;
  readonly stopLoss?: never;
  readonly takeProfit?: never;
  readonly maxSlippagePct?: never;
  /** Bybit V5 TickSize tolerance, expressed as an integer number of ticks. */
  readonly bybitSlippageToleranceTicks: number;
  readonly bybitBbo?: never;
};

/**
 * Adapter-specific order input for programmatic Bybit callers. The generic
 * PlaceOrderOpts path remains unchanged and never enables these fields by
 * default.
 */
export type BybitPlaceOrderOpts =
  | (PlaceOrderOpts & {
      readonly bybitBbo?: never;
      readonly bybitSlippageToleranceTicks?: never;
    })
  | BybitBboOrderOpts
  | BybitTickSizeMarketOrderOpts;

export type BybitOrderExecutionParams = {
  readonly bboSideType?: BybitBboSideType;
  readonly bboLevel?: `${BybitBboLevel}`;
  readonly slippageToleranceType?: 'Percent' | 'TickSize';
  readonly slippageTolerance?: string;
};

const hasAttachedProtection = (opts: PlaceOrderOpts) =>
  opts.triggerPrice !== undefined || opts.stopLoss !== undefined || opts.takeProfit !== undefined;

/**
 * Serialize the shared percentage-slippage contract to Bybit V5's native
 * market-order fields. TickSize mode has no equivalent in PlaceOrderOpts.
 */
export const getBybitMarketSlippageParams = (opts: PlaceOrderOpts) => {
  const maxSlippagePct = opts.maxSlippagePct;
  if (maxSlippagePct === undefined) return {};

  if (
    opts.type !== OrderType.Market ||
    opts.triggerPrice !== undefined ||
    opts.stopLoss !== undefined ||
    opts.takeProfit !== undefined
  ) {
    throw new Error('Bybit maxSlippagePct is only supported for plain market orders');
  }
  if (
    !Number.isFinite(maxSlippagePct) ||
    maxSlippagePct < 0.01 ||
    maxSlippagePct > 10 ||
    !hasAtMostTwoDecimals(maxSlippagePct)
  ) {
    throw new Error('Bybit maxSlippagePct must be from 0.01 to 10 with at most two decimals');
  }

  return {
    slippageToleranceType: 'Percent' as const,
    slippageTolerance: `${maxSlippagePct}`,
  };
};

export const hasBybitSingleOrderParams = (opts: PlaceOrderOpts) => {
  const bybitOpts = opts as BybitPlaceOrderOpts;
  return (
    opts.maxSlippagePct !== undefined ||
    bybitOpts.bybitBbo !== undefined ||
    bybitOpts.bybitSlippageToleranceTicks !== undefined
  );
};

/** Validate and serialize adapter-only V5 execution controls. */
export const getBybitOrderExecutionParams = (
  opts: PlaceOrderOpts,
  category: string,
): BybitOrderExecutionParams => {
  const bybitOpts = opts as BybitPlaceOrderOpts;
  const bbo = bybitOpts.bybitBbo;
  const ticks = bybitOpts.bybitSlippageToleranceTicks;

  if (bbo !== undefined && ticks !== undefined) {
    throw new Error('Bybit BBO and TickSize slippage options are mutually exclusive');
  }

  if (bbo !== undefined) {
    if (category !== 'linear' && category !== 'inverse') {
      throw new Error('Bybit BBO orders are only supported for linear and inverse products');
    }
    if (opts.type !== OrderType.Limit) {
      throw new Error('Bybit BBO requires a plain limit order');
    }
    if (opts.timeInForce !== undefined && opts.timeInForce !== OrderTimeInForce.GoodTillCancel) {
      throw new Error('Bybit BBO currently supports only GoodTillCancel time in force');
    }
    if (
      opts.price !== undefined ||
      hasAttachedProtection(opts) ||
      opts.maxSlippagePct !== undefined
    ) {
      throw new Error('Bybit BBO cannot include price, conditional, attached TP/SL, or slippage fields');
    }
    if (bbo === null || typeof bbo !== 'object' || Array.isArray(bbo)) {
      throw new Error('Bybit BBO requires sideType Queue|Counterparty and level 1..5');
    }
    if (bbo.sideType !== 'Queue' && bbo.sideType !== 'Counterparty') {
      throw new Error('Bybit BBO sideType must be Queue or Counterparty');
    }
    if (!Number.isInteger(bbo.level) || bbo.level < 1 || bbo.level > 5) {
      throw new Error('Bybit BBO level must be an integer from 1 to 5');
    }

    return {
      bboSideType: bbo.sideType,
      bboLevel: `${bbo.level}`,
    };
  }

  if (ticks !== undefined) {
    if (
      opts.type !== OrderType.Market ||
      opts.price !== undefined ||
      hasAttachedProtection(opts) ||
      opts.maxSlippagePct !== undefined
    ) {
      throw new Error('Bybit TickSize slippage is only supported for plain market orders');
    }
    if (!Number.isInteger(ticks) || ticks < 1 || ticks > 10_000) {
      throw new Error('Bybit TickSize slippage must be an integer from 1 to 10000');
    }

    return {
      slippageToleranceType: 'TickSize' as const,
      slippageTolerance: `${ticks}`,
    };
  }

  return getBybitMarketSlippageParams(opts);
};
