/**
 * Deterministic fixtures derived from Coinbase's published Starbase
 * adv-starbase-openapi.json schema. Values are synthetic and never fetched
 * from a live or credentialled account.
 */

export const STARBASE_ACCOUNT_SUMMARY_FIXTURE = {
  available_funds: 38.32,
  available_withdrawal_funds: 36.11,
  balance: 50,
  cross_collateral_enabled: false,
  currency: 'USDC',
  equity: 51.25,
  futures_pl: 1.25,
  futures_session_rpl: 0.4,
  futures_session_upl: 0.85,
  initial_margin: 12.68,
  maintenance_margin: 8.1,
  margin_balance: 51.25,
  margin_model: 'segregated_sm',
  portfolio_margining_enabled: false,
  session_rpl: 0.4,
  session_upl: 0.85,
  total_pl: 1.25,
} as const;

export const STARBASE_POSITION_FIXTURE = {
  average_price: 50_000,
  direction: 'buy',
  estimated_liquidation_price: 41_000,
  floating_profit_loss: 40,
  index_price: 50_390,
  initial_margin: 500,
  instrument_name: 'BTC_USDC-PERPETUAL',
  kind: 'future',
  leverage: 10,
  maintenance_margin: 250,
  mark_price: 50_400,
  open_orders_margin: 25,
  realized_profit_loss: 10,
  settlement_price: 50_100,
  size: 5_000,
  size_currency: 0.1,
  total_profit_loss: 50,
} as const;

export const STARBASE_OPEN_ORDER_FIXTURE = {
  amount: 0.1,
  api: true,
  average_price: 0,
  contracts: 100,
  creation_timestamp: 1_800_000_000_000,
  direction: 'buy',
  filled_amount: 0,
  instrument_name: 'BTC_USDC-PERPETUAL',
  label: 'saved-client-label',
  last_update_timestamp: 1_800_000_000_100,
  order_id: 'USDC-ORDER-1',
  order_state: 'open',
  order_type: 'limit',
  post_only: false,
  price: 50_000,
  reduce_only: false,
  time_in_force: 'good_til_cancelled',
} as const;

export const STARBASE_PARTIAL_ORDER_FIXTURE = {
  ...STARBASE_OPEN_ORDER_FIXTURE,
  filled_amount: 50,
  order_id: 'USDC-ORDER-2',
} as const;

export const STARBASE_HISTORY_ORDER_FIXTURE = {
  ...STARBASE_PARTIAL_ORDER_FIXTURE,
  average_price: 50_400,
  last_update_timestamp: 1_800_000_000_200,
  order_state: 'filled',
} as const;

export const STARBASE_ETH_HISTORY_ORDER_FIXTURE = {
  ...STARBASE_OPEN_ORDER_FIXTURE,
  amount: 1,
  contracts: 10,
  instrument_name: 'ETH_USDC-PERPETUAL',
  order_id: 'USDC-ORDER-ETH-1',
  price: 3_000,
} as const;

export const STARBASE_USER_TRADE_FIXTURE = {
  amount: 0.05,
  contracts: 50,
  direction: 'buy',
  fee: 1.25,
  fee_currency: 'USDC',
  index_price: 50_390,
  instrument_name: 'BTC_USDC-PERPETUAL',
  label: 'saved-client-label',
  liquidity: 'T',
  mark_price: 50_400,
  matching_id: null,
  order_id: 'USDC-ORDER-2',
  order_type: 'limit',
  post_only: false,
  price: 50_400,
  profit_loss: 20,
  reduce_only: false,
  state: 'filled',
  tick_direction: 0,
  timestamp: 1_800_000_000_200,
  trade_id: 'USDC-TRADE-1',
  trade_seq: 42,
} as const;
