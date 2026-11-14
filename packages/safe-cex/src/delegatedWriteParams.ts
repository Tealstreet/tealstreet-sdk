import type { PlaceOrderOpts } from './types';

/** Native placement fields, without account identity or signing material. */
export type DelegatedOrderParams = PlaceOrderOpts & {
  stpMode?: 'EXPIRE_TAKER' | 'EXPIRE_MAKER' | 'EXPIRE_BOTH';
  bybitBbo?: { sideType: 'Queue' | 'Counterparty'; level: number };
  bybitSlippageToleranceTicks?: number;
};
export const delegatedTarget = (value: unknown): value is string =>
  typeof value === 'string' && value.length > 0 && new TextEncoder().encode(value).length <= 256;
export const delegatedRecord = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === 'object' && !Array.isArray(value);
export const delegatedKeys = (value: Record<string, unknown>, allowed: readonly string[]): boolean =>
  Object.keys(value).every((key) => allowed.includes(key));
export const delegatedNumber = (value: unknown): value is number =>
  typeof value === 'number' && Number.isFinite(value) && value >= 0;
const orderFields = [
  'symbol',
  'type',
  'side',
  'amount',
  'price',
  'triggerPrice',
  'stopLoss',
  'takeProfit',
  'maxSlippagePct',
  'reduceOnly',
  'timeInForce',
  'triggerType',
  'tpTriggerType',
  'slTriggerType',
  'clientOrderId',
  'close',
  'stpMode',
  'bybitBbo',
  'bybitSlippageToleranceTicks',
  'impliedVolatility',
] as const;

export function isDelegatedOrderParams(value: unknown, close = false): value is DelegatedOrderParams {
  if (
    !delegatedRecord(value) ||
    !delegatedKeys(value, orderFields) ||
    !delegatedTarget(value.symbol) ||
    typeof value.type !== 'string' ||
    !['market', 'limit', 'conditional', 'stop_market', 'take_profit_market', 'trailing_stop_market'].includes(
      value.type,
    ) ||
    (value.side !== 'buy' && value.side !== 'sell') ||
    !delegatedNumber(value.amount)
  )
    return false;
  if (close && (value.reduceOnly !== true || (value.type !== 'market' && value.type !== 'limit'))) return false;
  for (const key of [
    'price',
    'triggerPrice',
    'stopLoss',
    'takeProfit',
    'maxSlippagePct',
    'bybitSlippageToleranceTicks',
    'impliedVolatility',
  ])
    if (key in value && !delegatedNumber(value[key])) return false;
  for (const key of ['reduceOnly', 'close']) if (key in value && typeof value[key] !== 'boolean') return false;
  for (const [key, values] of [
    ['timeInForce', ['GoodTillCancel', 'ImmediateOrCancel', 'FillOrKill', 'PostOnly']],
    ['triggerType', ['mark_price', 'last_price', 'index_price']],
    ['tpTriggerType', ['mark_price', 'last_price', 'index_price']],
    ['slTriggerType', ['mark_price', 'last_price', 'index_price']],
    ['stpMode', ['EXPIRE_TAKER', 'EXPIRE_MAKER', 'EXPIRE_BOTH']],
  ] as const)
    if (key in value && (typeof value[key] !== 'string' || !values.includes(value[key] as never))) return false;
  if ('clientOrderId' in value && !delegatedTarget(value.clientOrderId)) return false;
  if ('bybitBbo' in value) {
    const bbo = value.bybitBbo;
    if (
      !delegatedRecord(bbo) ||
      !delegatedKeys(bbo, ['sideType', 'level']) ||
      (bbo.sideType !== 'Queue' && bbo.sideType !== 'Counterparty') ||
      typeof bbo.level !== 'number' ||
      !Number.isInteger(bbo.level) ||
      bbo.level < 1 ||
      bbo.level > 5
    )
      return false;
  }
  return true;
}
