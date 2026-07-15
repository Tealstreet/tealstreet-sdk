/**
 * Deterministic fixtures derived from Coinbase's published Starbase
 * adv-starbase-openapi.json write-response schemas and technical migration
 * guide. Values are synthetic and never fetched from a live account.
 */

import { STARBASE_OPEN_ORDER_FIXTURE, STARBASE_USER_TRADE_FIXTURE } from './coinbase.starbase-private.fixtures';

export const STARBASE_WRITE_ORDER_FIXTURE = {
  ...STARBASE_OPEN_ORDER_FIXTURE,
  label: 'display-label-is-not-unique',
  order_id: 'USDC-PRIMARY-1',
  oto_order_ids: ['USDC-TP-1', 'USDC-SL-1'],
  is_primary_otoco: true,
} as const;

export const STARBASE_TRIGGERED_ORDER_FIXTURE = {
  ...STARBASE_OPEN_ORDER_FIXTURE,
  label: 'display-label-is-not-unique',
  order_id: 'USDC-POST-TRIGGER-1',
  order_type: 'stop_limit',
  trigger: 'mark_price',
  trigger_order_id: 'USDC-PRE-TRIGGER-1',
  trigger_price: 49_000,
} as const;

export const STARBASE_OTOCO_CHILD_FIXTURE = {
  ...STARBASE_OPEN_ORDER_FIXTURE,
  direction: 'sell',
  label: 'display-label-is-not-unique',
  order_id: 'USDC-TP-1',
  order_type: 'take_limit',
  primary_order_id: 'USDC-PRIMARY-1',
  reduce_only: true,
  trigger: 'last_price',
  trigger_price: 55_000,
} as const;

export const STARBASE_WRITE_RESULT_FIXTURE = {
  order: STARBASE_WRITE_ORDER_FIXTURE,
  trades: [],
} as const;

export const STARBASE_FILLED_WRITE_RESULT_FIXTURE = {
  order: {
    ...STARBASE_WRITE_ORDER_FIXTURE,
    average_price: 50_400,
    filled_amount: 0.1,
    order_state: 'filled',
  },
  trades: [STARBASE_USER_TRADE_FIXTURE],
} as const;

export const STARBASE_WRITE_ERROR_RESPONSE = {
  error: {
    code: 10004,
    data: { reason: 'order_not_found' },
    message: 'order_not_found',
  },
  id: 1,
  jsonrpc: '2.0',
} as const;
