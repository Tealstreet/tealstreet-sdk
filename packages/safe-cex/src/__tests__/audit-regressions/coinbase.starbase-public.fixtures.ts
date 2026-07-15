/**
 * Sanitized deterministic fixtures derived from read-only Coinbase Starbase
 * public gateway captures. Chart fixtures remain schema-derived because they
 * are unrelated to the live contract corrections covered here.
 */

export const STARBASE_PUBLIC_INSTRUMENT_FIXTURE = {
  base_currency: 'BTC',
  contract_size: 0.0001,
  counter_currency: 'USDC',
  creation_timestamp: 1_646_824_342_000,
  expiration_timestamp: 32_503_708_800_000,
  future_type: 'linear',
  instrument_name: 'BTC_USDC-PERPETUAL',
  instrument_type: 'linear',
  is_active: true,
  kind: 'future',
  max_leverage: 50,
  min_trade_amount: 0.0001,
  price_index: 'btc_usdc',
  quote_currency: 'USDC',
  settlement_currency: 'USDC',
  settlement_period: 'perpetual',
  state: 'open',
  tick_size: 0.5,
  tick_size_steps: [],
  underlying_type: 'crypto',
} as const;

export const STARBASE_PUBLIC_ETH_INSTRUMENT_FIXTURE = {
  ...STARBASE_PUBLIC_INSTRUMENT_FIXTURE,
  base_currency: 'ETH',
  contract_size: 0.001,
  creation_timestamp: 1_647_262_800_000,
  instrument_name: 'ETH_USDC-PERPETUAL',
  min_trade_amount: 0.001,
  price_index: 'eth_usdc',
  tick_size: 0.05,
} as const;

export const STARBASE_PUBLIC_INSTRUMENT_DISCOVERY_FIXTURE = [
  { instrument_name: 'SOL_USDC-PERPETUAL' },
  STARBASE_PUBLIC_INSTRUMENT_FIXTURE,
  STARBASE_PUBLIC_ETH_INSTRUMENT_FIXTURE,
] as const;

export const STARBASE_PUBLIC_TICKER_FIXTURE = {
  best_ask_amount: 0.0914,
  best_ask_price: 65_017,
  best_bid_amount: 0.0046,
  best_bid_price: 65_016.5,
  current_funding: 0.00001483,
  funding_8h: 0.00002502,
  index_price: 64_984.48,
  instrument_name: 'BTC_USDC-PERPETUAL',
  last_price: 64_985.5,
  mark_price: 65_001.69,
  open_interest: 430.2811,
  state: 'open',
  stats: {
    high: 65_529,
    low: 64_407.5,
    price_change: 0.748,
    volume: 216.8374,
    volume_notional: 14_082_490.77755,
    volume_usd: 14_079_674.28,
  },
  timestamp: 1_784_139_975_074,
} as const;

export const STARBASE_PUBLIC_ORDER_BOOK_FIXTURE = {
  asks: [
    [65_015, 0.0002],
    [65_018.5, 0.01],
  ],
  bids: [
    [65_013.5, 0.01],
    [65_005, 0.2222],
  ],
} as const;

export const STARBASE_BOOK_SNAPSHOT_PAYLOAD = {
  data: {
    asks: [
      ['new', 65_087, 0.0149],
      ['new', 65_087.5, 0.0498],
      ['new', 97_577.5, 0.0011],
    ],
    bids: [],
    change_id: 167_744_732_271,
    instrument_name: 'BTC_USDC-PERPETUAL',
    type: 'snapshot',
  },
} as const;

export const STARBASE_BOOK_CHANGE_PAYLOAD = {
  data: {
    asks: [
      ['new', 65_073, 0.0149],
      ['new', 65_073.5, 0.0498],
      ['delete', 65_087, 0],
      ['delete', 65_087.5, 0],
      ['change', 97_577.5, 0.0012],
    ],
    bids: [['new', 64_809.5, 2.8663]],
    change_id: 167_744_733_629,
    instrument_name: 'BTC_USDC-PERPETUAL',
    prev_change_id: 167_744_732_271,
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
