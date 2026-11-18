// Generated from safe-cex-types/src/store_model.rs. Do not edit.
import type { DisplayedMargin } from './DisplayedMargin';
import type { FetchedLeverage } from './FetchedLeverage';
import type { PositionSettings } from './PositionSettings';
import type { StoreDataLoaded } from './StoreDataLoaded';
import type { StoreOptions } from './StoreOptions';
import type { StoreUpdateIds } from './StoreUpdateIds';
import type { AccountReconnectDiagnostics } from '@tealstreet/http-over-websocket/src/connectionDiagnostics';
import type { MessageHandlersStats } from '../exchanges/types';
import type { ActiveChaser, ActiveTWAP, Balance, DisplayedPosition, Market, MessageHandlerStatsType, Order, OrderRiskProfile, Position, RequestOutcomeCounts, Ticker } from '../types';
export type StoreData = {
    readonly latency: number | null;
    readonly balance: Balance;
    readonly markets: Array<Market>;
    readonly tickers: Array<Ticker>;
    readonly orders: Array<Order>;
    readonly starkConfig?: {
        usdtConfig?: unknown;
        usdcConfig?: unknown;
        positionId?: unknown;
        takerFeePerToken?: unknown;
    };
    readonly zkLinkConfig?: {
        contractConfig?: unknown;
        l2Key?: unknown;
        ethereumAddress?: unknown;
        id?: unknown;
        spotAccount?: unknown;
        spotWallets?: unknown;
        contractAccount?: unknown;
        contractWallets?: unknown;
    };
    readonly userData?: unknown;
    readonly positions: Array<Position>;
    readonly positionSettings: Record<string, PositionSettings>;
    readonly displayedPositions: DisplayedPosition[];
    readonly displayedMargin?: DisplayedMargin;
    readonly ordersRisk: Record<string, OrderRiskProfile[]>;
    readonly activeChasers: ActiveChaser[];
    readonly activeTWAPs: ActiveTWAP[];
    readonly loaded: StoreDataLoaded;
    readonly options: StoreOptions;
    readonly updateIds: StoreUpdateIds;
    readonly messageHandlerStats?: Partial<Record<MessageHandlerStatsType, MessageHandlersStats>>;
    readonly requestStats?: RequestOutcomeCounts;
    readonly fetchedLeverage: Record<string, FetchedLeverage>;
    readonly socketHealth?: {
        publicHealthy: boolean;
        privateHealthy: boolean;
        publicInitialized: boolean;
        privateInitialized: boolean;
        publicLastMessage?: number;
        privateLastMessage?: number;
        reconnectDiagnostics?: AccountReconnectDiagnostics;
        publicCloseCode?: number;
        privateCloseCode?: number;
        publicCloseCountTotal?: number;
        privateCloseCountTotal?: number;
        publicCloseStreak?: number;
        privateCloseStreak?: number;
        publicReconnectPaused?: boolean;
        privateReconnectPaused?: boolean;
        privateDegradedReason?: string;
    };
};

export const initialStoreData: StoreData = {
  "latency": null,
  "balance": {
    "used": 0,
    "free": 0,
    "total": 0,
    "upnl": 0
  },
  "markets": [],
  "tickers": [],
  "orders": [],
  "positions": [],
  "positionSettings": {},
  "displayedPositions": [],
  "displayedMargin": {
    "longExposure": 0,
    "shortExposure": 0,
    "totalNotional": 0
  },
  "ordersRisk": {},
  "activeChasers": [],
  "activeTWAPs": [],
  "loaded": {
    "balance": false,
    "orders": false,
    "markets": false,
    "tickers": false,
    "positions": false
  },
  "options": {
    "isHedged": false
  },
  "updateIds": {
    "positions": 0,
    "orders": 0
  },
  "messageHandlerStats": {},
  "requestStats": {
    "ok": 0,
    "rateLimit": 0,
    "auth": 0,
    "network": 0,
    "other": 0
  },
  "fetchedLeverage": {},
  "socketHealth": {
    "publicHealthy": false,
    "privateHealthy": false,
    "publicInitialized": false,
    "privateInitialized": false
  }
};
