import type { ChaserOrderOptions } from './webworker/chaserOrder/ChaserOrderOptions';
import type { TwapOptions } from './webworker/twap/TwapOptions';
import type { TwapState } from './webworker/twap/TwapState';

import {
  delegatedKeys as keys,
  delegatedNumber as number,
  delegatedRecord as record,
  delegatedTarget as target,
} from './delegatedWriteParams';

type Options = Omit<TwapOptions, 'activeMarket'> & { activeMarket: { symbol: string } };
type State = Omit<TwapState, 'activeMarket'> & { activeMarket: { symbol: string } };
export type DelegatedAlgoAction =
  | { kind: 'algo'; op: 'exec'; data: { command: string; symbol?: string } }
  | { kind: 'algo'; op: 'chaser:start'; data: ChaserOrderOptions }
  | { kind: 'algo'; op: 'chaser:terminate' | 'twap:resume' | 'twap:pause' | 'twap:stop' | 'twap:cleanup'; data: string }
  | { kind: 'algo'; op: 'chaser:terminateAll'; data: Record<string, never> }
  | { kind: 'algo'; op: 'twap:start'; data: Options }
  | {
      kind: 'algo';
      op: 'twap:state';
      data: { id: string; ownerId?: string; ownerGeneration?: number; options: Options; state: State }[];
    };
const optionsFields = [
  'accountName',
  'activeMarket',
  'size',
  'duration',
  'ordersCount',
  'side',
  'chaserDelay',
  'reduceOnly',
  'randomnessPercentage',
  'useLimitOrders',
  'useChaserOrders',
  'timeoutPct',
  'timeoutAction',
  'failAction',
  'origin',
];
const stateFields = [
  'accountName',
  'activeMarket',
  'size',
  'ordersSizes',
  'side',
  'status',
  'ordersCount',
  'ordersExecuted',
  'nextOrderAt',
  'markForCleanup',
  'origin',
  'createdAt',
  'startedAt',
  'inFlight',
];
const market = (v: unknown) => record(v) && keys(v, ['symbol']) && target(v.symbol);
const side = (v: unknown) => v === 'buy' || v === 'sell';
const uint = (v: unknown) => number(v) && Number.isInteger(v) && v <= 0xffff_ffff;
function options(v: unknown): boolean {
  if (
    !record(v) ||
    !keys(v, optionsFields) ||
    !target(v.accountName) ||
    !market(v.activeMarket) ||
    !number(v.size) ||
    !number(v.duration) ||
    !uint(v.ordersCount) ||
    !side(v.side)
  )
    return false;
  for (const key of ['chaserDelay', 'randomnessPercentage', 'timeoutPct'])
    if (key in v && !number(v[key])) return false;
  for (const key of ['reduceOnly', 'useLimitOrders', 'useChaserOrders'])
    if (key in v && typeof v[key] !== 'boolean') return false;
  for (const key of ['timeoutAction', 'failAction'])
    if (key in v && v[key] !== 'market' && v[key] !== 'next') return false;
  return !('origin' in v) || ['web', 'cli', 'webhook', 'automation'].some((s) => v.origin === s);
}
function state(v: unknown): boolean {
  if (
    !record(v) ||
    !keys(v, stateFields) ||
    !target(v.accountName) ||
    !market(v.activeMarket) ||
    !number(v.size) ||
    !side(v.side) ||
    !uint(v.ordersCount) ||
    !uint(v.ordersExecuted) ||
    !number(v.nextOrderAt) ||
    !Array.isArray(v.ordersSizes) ||
    !v.ordersSizes.every(number) ||
    !['idle', 'running', 'paused', 'completed'].some((s) => v.status === s)
  )
    return false;
  for (const key of ['createdAt', 'startedAt']) if (key in v && !number(v[key])) return false;
  if ('markForCleanup' in v && typeof v.markForCleanup !== 'boolean') return false;
  if (
    'inFlight' in v &&
    (!record(v.inFlight) ||
      !keys(v.inFlight, ['size', 'clientOrderId']) ||
      !number(v.inFlight.size) ||
      ('clientOrderId' in v.inFlight && !target(v.inFlight.clientOrderId)))
  )
    return false;
  return !('origin' in v) || ['web', 'cli', 'webhook', 'automation'].some((s) => v.origin === s);
}
export function isDelegatedAlgoAction(v: Record<string, unknown>): v is Record<string, unknown> & DelegatedAlgoAction {
  if (!keys(v, ['kind', 'op', 'data'])) return false;
  const d = v.data;
  switch (v.op) {
    case 'exec':
      return (
        record(d) &&
        keys(d, ['command', 'symbol']) &&
        typeof d.command === 'string' &&
        d.command.length > 0 &&
        new TextEncoder().encode(d.command).length <= 16384 &&
        (!('symbol' in d) || target(d.symbol))
      );
    case 'chaser:start':
      if (
        !record(d) ||
        !keys(d, [
          'symbol',
          'size',
          'side',
          'maxAttempts',
          'reduceOnly',
          'delay',
          'origin',
          'accountName',
          'terminate',
          'clientId',
        ]) ||
        !target(d.symbol) ||
        !number(d.size) ||
        !side(d.side) ||
        !number(d.maxAttempts)
      )
        return false;
      if ('reduceOnly' in d && typeof d.reduceOnly !== 'boolean') return false;
      if ('delay' in d && !number(d.delay)) return false;
      if ('accountName' in d && !target(d.accountName)) return false;
      if ('clientId' in d && !target(d.clientId)) return false;
      if ('origin' in d && !['web', 'cli', 'webhook', 'automation'].some((s) => d.origin === s)) return false;
      return (
        !('terminate' in d) ||
        (record(d.terminate) &&
          keys(d.terminate, ['type', 'value', 'anchorPrice']) &&
          ['price', 'percent', 'usd'].some((s) => (d.terminate as Record<string, unknown>).type === s) &&
          number(d.terminate.value) &&
          (!('anchorPrice' in d.terminate) || number(d.terminate.anchorPrice)))
      );
    case 'chaser:terminate':
    case 'twap:resume':
    case 'twap:pause':
    case 'twap:stop':
    case 'twap:cleanup':
      return target(d);
    case 'chaser:terminateAll':
      return record(d) && keys(d, []);
    case 'twap:start':
      return options(d);
    case 'twap:state':
      return (
        Array.isArray(d) &&
        d.length <= 32 &&
        d.every(
          (row) =>
            record(row) &&
            keys(row, ['id', 'ownerId', 'ownerGeneration', 'options', 'state']) &&
            target(row.id) &&
            (!('ownerId' in row) || target(row.ownerId)) &&
            (!('ownerGeneration' in row) ||
              (number(row.ownerGeneration) && Number.isSafeInteger(row.ownerGeneration))) &&
            options(row.options) &&
            state(row.state),
        )
      );
    default:
      return false;
  }
}
