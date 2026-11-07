export const BASE_URLS = {
  PRODUCTION: 'https://api.hyperliquid.xyz',
  TESTNET: 'https://api.hyperliquid-testnet.xyz',
};

export const WSS_URLS = {
  PRODUCTION: 'wss://api.hyperliquid.xyz/ws',
  TESTNET: 'wss://api.hyperliquid-testnet.xyz/ws',
};

export const ENDPOINTS = {
  INFO: '/info',
  EXCHANGE: '/exchange',
};

// HyperliquidSpot uses same action-based API as Hyperliquid
export const GRAFANA_ENDPOINTS_EVERY = [
  ENDPOINTS.EXCHANGE, // Covers all trading actions
];

export const GRAFANA_ENDPOINTS_PERIODIC = [
  ENDPOINTS.INFO, // Covers all info queries
];

export const INFO_TYPES = {
  ALL_MIDS: 'allMids',
  META: 'meta',
  OPEN_ORDERS: 'openOrders',
  FRONTEND_OPEN_ORDERS: 'frontendOpenOrders',
  USER_FILLS: 'userFills',
  USER_FILLS_BY_TIME: 'userFillsByTime',
  USER_RATE_LIMIT: 'userRateLimit',
  ORDER_STATUS: 'orderStatus',
  L2_BOOK: 'l2Book',
  CANDLE_SNAPSHOT: 'candleSnapshot',
  PERPS_META_AND_ASSET_CTXS: 'metaAndAssetCtxs',
  PERPS_CLEARINGHOUSE_STATE: 'clearinghouseState',
  USER_FUNDING: 'userFunding',
  USER_NON_FUNDING_LEDGER_UPDATES: 'userNonFundingLedgerUpdates',
  FUNDING_HISTORY: 'fundingHistory',
  SPOT_META: 'spotMeta',
  SPOT_CLEARINGHOUSE_STATE: 'spotClearinghouseState',
  SPOT_META_AND_ASSET_CTXS: 'spotMetaAndAssetCtxs',
};

export const EXCHANGE_TYPES = {
  ORDER: 'order',
  CANCEL: 'cancel',
  CANCEL_BY_CLOID: 'cancelByCloid',
  SCHEDULE_CANCEL: 'scheduleCancel',
  MODIFY: 'modify',
  BATCH_MODIFY: 'batchModify',
  UPDATE_LEVERAGE: 'updateLeverage',
  UPDATE_ISOLATED_MARGIN: 'updateIsolatedMargin',
  USD_SEND: 'usdSend',
  SPOT_SEND: 'spotSend',
  WITHDRAW: 'withdraw3',
  SPOT_USER: 'spotUser',
  VAULT_TRANSFER: 'vaultTransfer',
  SET_REFERRER: 'setReferrer',
};

export const WEBSOCKET = {
  MAINNET_URL: 'wss://api.hyperliquid.xyz/ws',
  TESTNET_URL: 'wss://api.hyperliquid-testnet.xyz/ws',
};

export type Tif = 'Alo' | 'Ioc' | 'Gtc';
export type Tpsl = 'tp' | 'sl';
export type LimitOrderType = { tif: Tif };
export type TriggerOrderType = { triggerPx: number; isMarket: boolean; tpsl: Tpsl };
export type Grouping = 'na' | 'normalTpsl' | 'positionTpsl';
export type OrderType = { limit?: LimitOrderType; trigger?: TriggerOrderType };
export type Cloid = string;
export type OidOrCloid = number | Cloid;

export interface AllMids {
  [coin: string]: string;
}

export interface Meta {
  universe: {
    name: string;
    szDecimals: number;
    maxLeverage: number;
    index: number;
    onlyIsolated: boolean;
    tokens: [number, number];
  }[];
  tokens: {
    name: string;
    index: number;
    szDecimals: number;
  }[];
}

export interface ClearinghouseState {
  assetPositions: {
    position: {
      coin: string;
      cumFunding: {
        allTime: string;
        sinceChange: string;
        sinceOpen: string;
      };
      entryPx: string;
      leverage: {
        rawUsd: string;
        type: string;
        value: number;
      };
      liquidationPx: string;
      marginUsed: string;
      maxLeverage: number;
      positionValue: string;
      returnOnEquity: string;
      szi: string;
      unrealizedPnl: string;
    };
    type: string;
  }[];
  crossMaintenanceMarginUsed: string;
  crossMarginSummary: {
    accountValue: string;
    totalMarginUsed: string;
    totalNtlPos: string;
    totalRawUsd: string;
  };
  marginSummary: {
    accountValue: string;
    totalMarginUsed: string;
    totalNtlPos: string;
    totalRawUsd: string;
  };
  time: number;
  withdrawable: string;
}

export interface UserFills {
  closedPnl: string;
  coin: string;
  crossed: boolean;
  dir: string;
  hash: string;
  oid: number;
  px: string;
  side: string;
  startPosition: string;
  sz: string;
  time: number;
}

[];

export interface OrderResponse {
  status: string;
  response: {
    type: string;
    data: {
      statuses: Array<{
        resting?: { oid: number };
        filled?: { oid: number };
      }>;
    };
  };
}

export interface WsTrade {
  coin: string;
  side: string;
  px: string;
  sz: string;
  hash: string;
  time: number;
  tid: number;
}

export interface WsBook {
  coin: string;
  levels: [Array<WsLevel>, Array<WsLevel>];
  time: number;
}

export interface WsLevel {
  px: string;
  sz: string;
  n: number;
}

export interface WsOrder {
  order: {
    coin: string;
    side: string;
    limitPx: string;
    sz: string;
    oid: number;
    timestamp: number;
    origSz: string;
  };
  status: string;
  statusTimestamp: number;
  user: string;
}

export type WsUserEvent = (WsFill[] | WsUserFunding | WsLiquidation | WsNonUserCancel[]) & { user: string };

export interface WsFill {
  coin: string;
  px: string;
  sz: string;
  side: string;
  time: number;
  startPosition: string;
  dir: string;
  closedPnl: string;
  hash: string;
  oid: number;
  crossed: boolean;
  fee: string;
  tid: number;
}

export interface WsUserFunding {
  time: number;
  coin: string;
  usdc: string;
  szi: string;
  fundingRate: string;
}

export interface WsLiquidation {
  lid: number;
  liquidator: string;
  liquidated_user: string;
  liquidated_ntl_pos: string;
  liquidated_account_value: string;
}

export interface WsNonUserCancel {
  coin: string;
  oid: number;
}

export interface SpotClearinghouseState {
  balances: {
    coin: string;
    token?: number;
    hold: string;
    total: string;
    entryNtl?: string;
  }[];
}

export interface FrontendOpenOrders {
  coin: string;
  isPositionTpsl: boolean;
  isTrigger: boolean;
  limitPx: string;
  oid: number;
  orderType: string;
  origSz: string;
  reduceOnly: boolean;
  side: string;
  sz: string;
  timestamp: number;
  triggerCondition: string;
  triggerPx: string;
}

[];

export interface UserFills {
  closedPnl: string;
  coin: string;
  crossed: boolean;
  dir: string;
  hash: string;
  oid: number;
  px: string;
  side: string;
  startPosition: string;
  sz: string;
  time: number;
}

[];

export interface UserRateLimit {
  [key: string]: any;
}

export interface OrderStatus {
  [key: string]: any;
}

export interface L2Book {
  levels: [
    {
      px: string;
      sz: string;
      n: number;
    }[],
    {
      px: string;
      sz: string;
      n: number;
    }[],
  ];
}

export interface CandleSnapshot {
  T: number;
  c: string;
  h: string;
  i: string;
  l: string;
  n: number;
  o: string;
  s: string;
  t: number;
  v: string;
}

[];

export interface AssetCtx {
  coin?: string;
  dayNtlVlm: string;
  dayBaseVlm?: string;
  funding: string;
  impactPxs: [string, string];
  markPx: string;
  midPx: string;
  openInterest: string;
  oraclePx: string;
  premium: string;
  prevDayPx: string;
}

export interface MetaAndAssetCtxs {
  meta: Meta;
  assetCtxs: AssetCtx[];
}

export interface UserFundingDelta {
  coin: string;
  fundingRate: string;
  szi: string;
  type: 'funding';
  usdc: string;
}

export interface UserFundingEntry {
  delta: UserFundingDelta;
  hash: string;
  time: number;
}

export type UserFunding = UserFundingEntry[];

export interface UserNonFundingLedgerDelta {
  coin: string;
  type: 'deposit' | 'withdraw' | 'transfer' | 'liquidation';
  usdc: string;
}

export interface UserNonFundingLedgerEntry {
  delta: UserNonFundingLedgerDelta;
  hash: string;
  time: number;
}

export type UserNonFundingLedgerUpdates = UserNonFundingLedgerEntry[];

export interface FundingHistoryEntry {
  coin: string;
  fundingRate: string;
  premium: string;
  time: number;
}

export type FundingHistory = FundingHistoryEntry[];

export interface SpotToken {
  name: string;
  szDecimals: number;
  weiDecimals: number;
  index: number;
  tokenId: string;
  isCanonical: boolean;
}

export interface SpotMarket {
  name: string;
  tokens: [number, number]; // Indices of base and quote tokens
  index: number;
  isCanonical: boolean;
}

export interface SpotMeta {
  tokens: SpotToken[];
  universe: SpotMarket[];
}

export interface SpotAssetCtx {
  dayNtlVlm: string;
  dayBaseVlm?: string;
  markPx: string;
  midPx: string;
  prevDayPx: string;
}

export interface SpotMetaAndAssetCtxs {
  meta: SpotMeta;
  assetCtxs: SpotAssetCtx[];
}

export interface UserOpenOrder {
  coin: string;
  limitPx: string;
  oid: number;
  side: string;
  sz: string;
  timestamp: number;
}

export type UserOpenOrders = UserOpenOrder[];

export interface OrderRequest {
  coin: string;
  is_buy: boolean;
  sz: number;
  limit_px: number;
  order_type: OrderType;
  reduce_only: boolean;
  cloid?: Cloid;
}

export interface OrderWire {
  a: number;
  b: boolean;
  p: string;
  s: string;
  r: boolean;
  t: OrderType;
  c?: string;
}

export interface CancelOrderRequest {
  coin: string;
  o: number;
}

export type CancelOrderRequests = {
  a: number;
  o: number;
}[];

export interface CancelByCloidRequest {
  coin: string;
  cloid: Cloid;
}

export interface ModifyRequest {
  oid: OidOrCloid;
  order: OrderRequest;
}

export interface ModifyWire {
  oid: number;
  order: OrderWire;
}

export interface ScheduleCancelAction {
  type: 'scheduleCancel';
  time?: number | null;
}

export interface Signature {
  r: string;
  s: string;
  v: number;
}

export interface Notification {
  notification: string;
  user: string;
}

export interface Candle {
  t: number; // open time
  T: number; // close time
  s: string; // symbol
  i: string; // interval
  o: string; // open
  c: string; // close
  h: string; // high
  l: string; // low
  v: string; // volume
  n: number; // number of trades
  coin: string;
  interval: string;
}

export interface HyperliquidCandle {
  t: number; // open time
  T: number; // close time
  s: string; // symbol
  i: string; // interval
  o: string; // open
  c: string; // close
  h: string; // high
  l: string; // low
  v: string; // volume
  n: number; // number of trades
  coin: string;
  interval: string;
}

export interface WsUserFill {
  coin: string;
  px: string;
  sz: string;
  side: string;
  time: number;
  startPosition: string;
  dir: string;
  closedPnl: string;
  hash: string;
  oid: number;
  crossed: boolean;
  fee: string;
  tid: number;
}

export type WsUserFills = {
  isSnapshot: boolean;
  fills: WsUserFill[];
  user: string;
};

export interface WsUserFunding {
  time: number;
  coin: string;
  usdc: string;
  szi: string;
  fundingRate: string;
}

export type WsUserFundings = {
  isSnapshot: boolean;
  fundings: WsUserFunding[];
  user: string;
};

export interface WsUserNonFundingLedgerUpdate {
  time: number;
  coin: string;
  usdc: string;
  type: 'deposit' | 'withdraw' | 'transfer' | 'liquidation';
}

export type WsUserNonFundingLedgerUpdates = {
  isSnapshot: boolean;
  updates: WsUserNonFundingLedgerUpdate[];
  user: string;
};
