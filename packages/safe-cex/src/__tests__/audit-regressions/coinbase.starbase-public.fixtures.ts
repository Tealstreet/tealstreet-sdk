/**
 * Deterministic fixtures derived from Coinbase's published Starbase
 * adv-starbase-openapi.json and adv-starbase-asyncapi.json schemas.
 */

export const STARBASE_PUBLIC_INSTRUMENT_FIXTURE = {
  base_currency: 'BTC',
  contract_size: 1,
  counter_currency: 'USDC',
  creation_timestamp: 1_536_569_522_277,
  expiration_timestamp: 4_102_444_800_000,
  instrument_id: 42,
  instrument_name: 'BTC_USDC-PERPETUAL',
  instrument_type: 'linear',
  is_active: true,
  kind: 'future',
  max_leverage: 50,
  min_trade_amount: 0.001,
  price_index: 'btc_usdc',
  quote_currency: 'USD',
  settlement_period: 'perpetual',
  state: 'open',
  tick_size: 0.5,
  tick_size_steps: [{ above_price: 100_000, tick_size: 1 }],
} as const;

export const STARBASE_PUBLIC_TICKER_FIXTURE = {
  best_ask_amount: 100_020,
  best_ask_price: 50_010,
  best_bid_amount: 150_000,
  best_bid_price: 50_000,
  current_funding: 0.000_001,
  estimated_delivery_price: 50_004,
  funding_8h: 0.000_008,
  index_price: 50_004,
  instrument_name: 'BTC_USDC-PERPETUAL',
  last_price: 50_005,
  mark_price: 50_003,
  max_price: 55_000,
  min_price: 45_000,
  open_interest: 25_000_000,
  state: 'open',
  stats: {
    high: 51_000,
    low: 49_000,
    price_change: 2.5,
    volume: 1_200,
    volume_usd: 60_000_000,
  },
  timestamp: 1_800_000_000_000,
} as const;

export const STARBASE_PUBLIC_ORDER_BOOK_FIXTURE = {
  asks: [
    [50_010, 100_020],
    [50_020, 50_020],
  ],
  bids: [
    [50_000, 100_000],
    [49_990, 49_990],
  ],
} as const;

export const STARBASE_BOOK_SNAPSHOT_PAYLOAD = {
  data: {
    asks: [['new', 50_010, 100_020]],
    bids: [['new', 50_000, 100_000]],
    change_id: 297_217,
    instrument_name: 'BTC_USDC-PERPETUAL',
    timestamp: 1_800_000_000_000,
    type: 'snapshot',
  },
} as const;

export const STARBASE_BOOK_CHANGE_PAYLOAD = {
  data: {
    asks: [['change', 50_010, 50_010]],
    bids: [['delete', 50_000, 0]],
    change_id: 297_218,
    instrument_name: 'BTC_USDC-PERPETUAL',
    prev_change_id: 297_217,
    timestamp: 1_800_000_000_100,
    type: 'change',
  },
} as const;

export const STARBASE_CHART_DATA_FIXTURE = {
  close: [50_100, 50_200],
  cost: [5_000_000, 5_100_000],
  high: [50_150, 50_250],
  low: [49_900, 50_050],
  open: [50_000, 50_100],
  status: 'ok',
  ticks: [1_800_000_000_000, 1_800_000_060_000],
  volume: [100, 101],
} as const;

export const STARBASE_CHART_PAYLOAD_FIXTURE = {
  data: {
    close: 50_100,
    cost: 5_000_000,
    high: 50_150,
    low: 49_900,
    open: 50_000,
    tick: 1_800_000_000_000,
    volume: 100,
  },
} as const;
