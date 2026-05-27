/**
 * Fully populated stub store for runtime shape validation.
 *
 * This is typed as StoreData — if the type changes, this file will fail
 * to compile until updated. At runtime, we fingerprint this stub's shape
 * and compare against cached store data to detect schema drift.
 *
 * IMPORTANT: When modifying StoreData, Position, Order, Market, Ticker,
 * Balance, or DisplayedPosition types, update the corresponding stub
 * values here. TypeScript will enforce this via type errors.
 */
import type {
  ActiveChaser,
  ActiveTWAP,
  Balance,
  DisplayedPosition,
  Market,
  Order,
  OrderRiskProfile,
  Position,
  StoreData,
  Ticker,
} from '../types';

import { OrderSide, OrderStatus, OrderType, PositionSide } from '../types';

const stubBalance: Balance = {
  used: 0,
  free: 0,
  total: 0,
  upnl: 0,
};

const stubMarket: Market = {
  id: '',
  symbol: '',
  base: '',
  quote: '',
  active: false,
  precision: { amount: 0, price: 0 },
  limits: {
    amount: { min: 0, max: 0 },
    leverage: { min: 0, max: 0 },
  },
};

const stubTicker: Ticker = {
  id: '',
  symbol: '',
  bid: 0,
  ask: 0,
  last: 0,
  mark: 0,
  index: 0,
  percentage: 0,
  openInterest: 0,
  fundingRate: 0,
  fundingTime: 0,
  volume: 0,
  quoteVolume: 0,
};

const stubOrder: Order = {
  id: '',
  status: OrderStatus.Open,
  symbol: '',
  type: OrderType.Limit,
  side: OrderSide.Buy,
  price: 0,
  amount: 0,
  filled: 0,
  remaining: 0,
  reduceOnly: false,
  lastUpdate: 0,
};

const stubPosition: Position = {
  id: '',
  symbol: '',
  side: PositionSide.Long,
  entryPrice: 0,
  notional: 0,
  leverage: 0,
  unrealizedPnl: 0,
  contracts: 0,
  liquidationPrice: 0,
};

const stubDisplayedPosition: DisplayedPosition = {
  ...stubPosition,
  account: '',
};

const stubActiveChaser: ActiveChaser = {
  id: '',
  symbol: '',
  size: 0,
  side: OrderSide.Buy,
  maxAttempts: 0,
  attempts: 0,
  isStopped: false,
  stopReason: '',
};

const stubActiveTWAP: ActiveTWAP = {
  id: '',
  symbol: '',
  accountName: '',
  activeMarket: stubMarket,
  size: 0,
  duration: 0,
  ordersCount: 0,
  side: OrderSide.Buy,
  chaserDelay: 0,
  reduceOnly: false,
  randomnessPercentage: 0,
  useLimitOrders: false,
  useChaserOrders: false,
  ordersSizes: [0],
  status: 'idle' as any,
  ordersExecuted: 0,
  nextOrderAt: 0,
};

const stubOrderRiskProfile: OrderRiskProfile = {
  orderPnl: 0,
  totalRealizedPnl: 0,
  rollingUnrealizedPnl: 0,
  orderLinePosSide: PositionSide.Long,
  positionSize: 0,
};

/**
 * A fully populated StoreData stub with at least one element in every array
 * and every required field filled. TypeScript enforces that this matches
 * the StoreData type at compile time.
 */
export const stubStore: StoreData = {
  latency: 0,
  balance: stubBalance,
  markets: [stubMarket],
  tickers: [stubTicker],
  positions: [stubPosition],
  displayedPositions: [stubDisplayedPosition],
  ordersRisk: { stub: [stubOrderRiskProfile] },
  activeChasers: [stubActiveChaser],
  activeTWAPs: [stubActiveTWAP],
  orders: [stubOrder],
  fetchedLeverage: { stub: { buyLeverage: 0, sellLeverage: 0 } },
  loaded: {
    balance: false,
    orders: false,
    markets: false,
    tickers: false,
    positions: false,
  },
  options: {
    isHedged: false,
  },
  updateIds: {
    positions: 0,
    orders: 0,
  },
  socketHealth: {
    publicHealthy: false,
    privateHealthy: false,
    publicInitialized: false,
    privateInitialized: false,
  },
};
