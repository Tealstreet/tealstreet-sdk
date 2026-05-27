/**
 * Test fixtures — minimal but realistic seed data for the parity test and
 * any future adapter/proxy tests. Keep deterministic (no timestamps from
 * Date.now(), no randomness).
 */
import {
  type Balance,
  type Candle,
  type Market,
  type Order,
  type OrderBook,
  OrderSide,
  OrderStatus,
  OrderType,
  type Position,
  PositionSide,
  type Ticker,
} from '@tealstreet/safe-cex/src/types';

import type { AccountInfo, HistoricalPosition } from '../api/CoreApi.types';
import type { MockSeed } from './mockAdapter';

const btc: Market = {
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 1000 }, leverage: { min: 1, max: 100 } },
};

const eth: Market = {
  id: 'ETHUSDT',
  symbol: 'ETHUSDT',
  base: 'ETH',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.01, price: 0.01 },
  limits: { amount: { min: 0.01, max: 10000 }, leverage: { min: 1, max: 50 } },
};

const tickerBtc: Ticker = {
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  bid: 50000.0,
  ask: 50010.0,
  last: 50005.0,
  mark: 50007.0,
  index: 50006.0,
  percentage: 1.5,
  openInterest: 1000,
  fundingRate: 0.0001,
  fundingTime: 1700000000000,
  volume: 200,
  quoteVolume: 10_000_000,
};

const tickerEth: Ticker = {
  id: 'ETHUSDT',
  symbol: 'ETHUSDT',
  bid: 3000.0,
  ask: 3000.5,
  last: 3000.2,
  mark: 3000.3,
  index: 3000.1,
  percentage: -0.2,
  openInterest: 5000,
  fundingRate: -0.00005,
  fundingTime: 1700000000000,
  volume: 1000,
  quoteVolume: 3_000_000,
};

const positionLong: Position = {
  id: 'pos-1',
  symbol: 'BTCUSDT',
  side: PositionSide.Long,
  entryPrice: 49000,
  notional: 49_000,
  leverage: 10,
  unrealizedPnl: 100,
  contracts: 1,
  liquidationPrice: 45_000,
};

const positionShort: Position = {
  id: 'pos-2',
  symbol: 'ETHUSDT',
  side: PositionSide.Short,
  entryPrice: 3010,
  notional: 30_100,
  leverage: 5,
  unrealizedPnl: 50,
  contracts: 10,
  liquidationPrice: 3500,
};

const order1: Order = {
  id: 'o-1',
  status: OrderStatus.Open,
  symbol: 'BTCUSDT',
  type: OrderType.Limit,
  side: OrderSide.Buy,
  price: 49500,
  amount: 0.5,
  filled: 0,
  remaining: 0.5,
  reduceOnly: false,
  lastUpdate: 1700000000000,
};

const balance: Balance = {
  used: 5_000,
  free: 95_000,
  total: 100_000,
  upnl: 150,
};

const accountInfo: AccountInfo = {
  name: 'mybybit',
  exchange: 'bybit',
  isStreaming: true,
};

const accountInfo2: AccountInfo = {
  name: 'hl-prod',
  exchange: 'hyperliquid',
  isStreaming: true,
};

const orderbookBtc: OrderBook = {
  bids: [
    { price: 50000, amount: 1, total: 1 },
    { price: 49990, amount: 2, total: 3 },
  ],
  asks: [
    { price: 50010, amount: 1, total: 1 },
    { price: 50020, amount: 2, total: 3 },
  ],
};

const candleBtc: Candle = {
  timestamp: 1700000000,
  open: 50000,
  high: 50100,
  low: 49900,
  close: 50050,
  volume: 5,
};

const historical: HistoricalPosition = {
  account: 'mybybit',
  symbol: 'BTCUSDT',
  side: 'long',
  contracts: 0.5,
  entryPrice: 48000,
  unrealizedPnl: 0,
  fees: 12.5,
  timestamp: 1699000000000,
};

export const SEED: MockSeed = {
  activeAccount: 'mybybit',
  activeSymbol: 'BTCUSDT',
  activeExchange: 'bybit',
  streamingAccounts: ['mybybit', 'hl-prod'],
  accounts: [accountInfo, accountInfo2],
  markets: {
    mybybit: [btc, eth],
    'hl-prod': [btc],
  },
  tickers: {
    mybybit: [tickerBtc, tickerEth],
    'hl-prod': [tickerBtc],
  },
  positions: {
    mybybit: [positionLong, positionShort],
    'hl-prod': [],
  },
  orders: {
    mybybit: [order1],
    'hl-prod': [],
  },
  balance: {
    mybybit: balance,
    'hl-prod': { used: 0, free: 10_000, total: 10_000, upnl: 0 },
  },
  historical: [historical],
  orderbook: {
    'mybybit::BTCUSDT': orderbookBtc,
  },
  candles: {
    'mybybit::BTCUSDT::1m': [candleBtc],
  },
};
