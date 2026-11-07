import type { Store } from '../../../store/store.interface';
import type {
  Candle,
  ExchangeAccount,
  ExchangeName,
  ExchangeOptions,
  Execution,
  ExecutionsOptions,
  Market,
  OHLCVOptions,
  Order,
  OrderBook,
  PlaceOrderOpts,
  Position,
  PublicTrade,
  Ticker,
  Timeframe,
  UpdateOrderOpts,
  ValidateAccountResult,
  Writable,
} from '../../../types';
import type {
  AsterDEXAccountResponse,
  AsterDEXAPIVersion,
  AsterDEXDepthResponse,
  AsterDEXKlineResponse,
  AsterDEXMarketResponse,
  AsterDEXOpenInterestResponse,
  AsterDEXOrderResponse,
  AsterDEXPlaceOrderOpts,
  AsterDEXPositionModeResponse,
  AsterDEXPositionResponse,
  AsterDEXStpMode,
  AsterDEXTickerResponse,
  AsterDEXTradeResponse,
  AsterDEXUserTradeResponse,
} from '../../../exchanges/asterdex/asterdex.types';

import { AxiosInstance } from 'axios';
import uniqBy from 'lodash/uniqBy';

import {
  LogSeverity,
  MarginType,
  OrderSide,
  OrderStatus,
  OrderStopTriggerType,
  OrderType,
  PositionMode,
  PositionSide,
} from '../../../types';
import { precisionToString } from '../../../utils/ccxt_helpers/precision';
import { createPositionId } from '../../../utils/createPositionId';
import { loop } from '../../../utils/loop';
import { adjust } from '../../../utils/safe-math';
import { BaseExchange, CallableMethodsInWorker } from '../../../exchanges/base';
import { createAPIInstances } from '../../../exchanges/asterdex/asterdex.api';
import { getAsterBuilderFeeRateForOrder, resolveAsterBuilderFeeConfig } from '../../../exchanges/asterdex/asterdex.builder-fee';
import {
  ASTERDEX_MAIN_WALLET_REQUIRED_MESSAGE,
  asterdexOrderSideMap,
  asterdexOrderTypeMap,
  asterdexTimeInForceMap,
  getAsterDEXFapiPath,
  hasAsterDEXV3AuthContext,
  isAsterDEXAddress,
  isAsterDEXStpMode,
  isAsterDEXV3PrivateKey,
  normalizeAsterDEXRealizedPnlDeltas,
  resolveAsterDEXApiVersion,
  resolveAsterDEXV3AuthContext,
} from '../../../exchanges/asterdex/asterdex.types';
import { asterDEXVirtualClock } from '../../../exchanges/asterdex/asterdex.virtualClock';
import { AsterDEXPrivateWebsocket } from './asterdex.ws-private';
import { AsterDEXPublicWebsocket } from './asterdex.ws-public';

const PRIVATE_STATE_HEALTH_CHECK_INTERVAL_MS = 3_000;
const BALANCE_RECONCILIATION_INTERVAL_MS = 25_000;
const POSITION_SAFETY_RECONCILIATION_INTERVAL_MS = 25_000;
const ORDER_SAFETY_RECONCILIATION_INTERVAL_MS = 75_000;

const isAsterDEXUnknownOrderError = (error: unknown): boolean => {
  const message = error instanceof Error ? error.message : String(error);
  return message.includes('code: -2011') && message.toLowerCase().includes('unknown order');
};

export class AsterDEXExchange extends BaseExchange {
  private static readonly OPEN_INTEREST_POLL_INTERVAL_MS = 15000;
  private static readonly OPEN_INTEREST_BATCH_SIZE = 25;

  primaryAPI!: AxiosInstance;
  private secondaryAPI!: AxiosInstance;
  private unlimitedAPI!: AxiosInstance;
  private publicAPI!: AxiosInstance;
  readonly apiVersion: AsterDEXAPIVersion;

  publicWebsocket: AsterDEXPublicWebsocket;
  privateWebsocket: AsterDEXPrivateWebsocket;
  private positionStateVersion = 0;
  private balanceStateVersion = 0;
  private lastBalanceReconciliationAt = 0;
  private lastPositionSafetyReconciliationAt = 0;
  private lastOrderSafetyReconciliationAt = 0;
  private openInterestCursor = 0;

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);
    this.apiVersion = resolveAsterDEXApiVersion(opts);
    this.initializeAPIs();
    this.publicWebsocket = new AsterDEXPublicWebsocket(this, store);
    this.privateWebsocket = new AsterDEXPrivateWebsocket(this, store);
  }

  protected override get canSign(): boolean {
    if (!this.options.key) {
      return false;
    }

    if (this.options.remoteSigner) {
      return true;
    }

    if (this.apiVersion === 'v3') {
      return hasAsterDEXV3AuthContext(this.options);
    }

    return Boolean(this.options.secret);
  }

  markPositionStateUpdatedByWebsocket = (): void => {
    this.positionStateVersion += 1;
  };

  getPositionStateVersion = (): number => this.positionStateVersion;

  markBalanceStateUpdatedByWebsocket = (): number => {
    this.balanceStateVersion += 1;
    return this.balanceStateVersion;
  };

  getBalanceStateVersion = (): number => this.balanceStateVersion;

  reconcileBalanceAfterPrivateUpdate = async (balanceStateVersion: number): Promise<boolean> => {
    try {
      const balance = await this.fetchBalance();
      if (this.isDisposed || balanceStateVersion !== this.balanceStateVersion) return false;

      this.store.update({ balance, loaded: { ...this.store.loaded, balance: true } });
      this.lastBalanceReconciliationAt = Date.now();
      return true;
    } catch (error: any) {
      this.log(`Balance reconciliation failed: ${error.message}`, LogSeverity.Warning);
      return true;
    }
  };

  getFapiPath = (endpoint: string, versionOverride?: AsterDEXAPIVersion | 'v4'): string => {
    return getAsterDEXFapiPath(endpoint, versionOverride ?? this.apiVersion);
  };

  private getAccountEndpoint = (): string => {
    if (this.apiVersion === 'v3') {
      return this.getFapiPath('account');
    }
    return this.getFapiPath('account', 'v4');
  };

  private initializeAPIs() {
    const apis = createAPIInstances(this);
    this.primaryAPI = apis.primary;
    this.secondaryAPI = apis.secondary;
    this.unlimitedAPI = apis.unlimited;
    this.publicAPI = apis.public;
  }

  start = async (): Promise<void> => {
    try {
      this.log(`Starting AsterDEX exchange (API ${this.apiVersion})...`, LogSeverity.Info);

      const useExternalPublicData = await this.initExternalPublicData();

      // Sync time with AsterDEX server
      await asterDEXVirtualClock.start(this.options.testnet, this.apiVersion);
      this.log('Time synced with AsterDEX server', LogSeverity.Info);

      // Validate account credentials (skip for public-only mode)
      if (!this.isPublicDataOnly) {
        await this.validateAccount();
        await this.syncPositionMode();
      }

      if (!useExternalPublicData) {
        // Fetch initial market data
        const markets = await this.fetchMarkets();
        this.log(`Fetched ${markets.length} markets`, LogSeverity.Info);

        // Log a sample market to check precision values
        if (markets.length > 0) {
          const sampleMarket = markets.find((m) => m.symbol === 'BTCUSDT') || markets[0];
          this.log(
            `Sample market (${sampleMarket.symbol}): pricePrecision=${sampleMarket.precision.price} (decimal), amountPrecision=${sampleMarket.precision.amount} (decimal)`,
            LogSeverity.Info,
          );
        }

        this.store.update({ markets, loaded: { ...this.store.loaded, markets: true } });

        // Fetch all tickers
        const tickers = await this.fetchTickers();
        this.log(`Fetched ${tickers.length} tickers`, LogSeverity.Info);

        this.store.update({ tickers, loaded: { ...this.store.loaded, tickers: true } });

        // Backfill OI shortly after ticker bootstrap so screener OI is not stuck at 0.
        this.startOpenInterestPolling();
      }

      // Fetch initial positions and orders if we have credentials
      if (!this.isPublicDataOnly) {
        try {
          const positions = await this.fetchPositions();
          this.store.update({ positions, loaded: { ...this.store.loaded, positions: true } });
          this.lastPositionSafetyReconciliationAt = Date.now();
          this.log(`Loaded ${positions.length} positions`, LogSeverity.Info);
        } catch (error: any) {
          this.log(`Failed to fetch initial positions: ${error.message}`, LogSeverity.Warning);
          // Continue startup even if positions fetch fails
        }

        try {
          const orders = await this.fetchOrders();
          this.store.update({ orders, loaded: { ...this.store.loaded, orders: true } });
          this.lastOrderSafetyReconciliationAt = Date.now();
          this.log(`Loaded ${orders.length} open orders`, LogSeverity.Info);
        } catch (error: any) {
          this.log(`Failed to fetch initial orders: ${error.message}`, LogSeverity.Warning);
          // Continue startup even if orders fetch fails
        }
      }

      // Start WebSocket connections
      if (!useExternalPublicData) {
        this.publicWebsocket.connectAndSubscribe();
      }

      // Connect private WebSocket (it will handle its own listen key)
      if (!this.isPublicDataOnly) {
        this.privateWebsocket.connectAndSubscribe();
      }

      // Start tick loop for periodic updates
      if (!this.isPublicDataOnly) {
        this.tick();
      }

      this.log('AsterDEX exchange started successfully', LogSeverity.Info);
    } catch (error: any) {
      this.log(`Failed to start AsterDEX exchange: ${error.message}`, LogSeverity.Error);
      throw error;
    }
  };

  private tick = async (): Promise<void> => {
    if (this.isDisposed) return;

    const now = Date.now();
    const shouldReconcileBalance =
      this.lastBalanceReconciliationAt === 0 ||
      now - this.lastBalanceReconciliationAt >= BALANCE_RECONCILIATION_INTERVAL_MS;
    const shouldReconcilePositions =
      this.lastPositionSafetyReconciliationAt === 0 ||
      now - this.lastPositionSafetyReconciliationAt >= POSITION_SAFETY_RECONCILIATION_INTERVAL_MS;
    const shouldReconcileOrders =
      this.lastOrderSafetyReconciliationAt === 0 ||
      now - this.lastOrderSafetyReconciliationAt >= ORDER_SAFETY_RECONCILIATION_INTERVAL_MS;

    if (shouldReconcileBalance) {
      await this.reconcileBalanceAfterPrivateUpdate(this.balanceStateVersion);
    }

    if (shouldReconcilePositions) {
      const positionStateVersion = this.getPositionStateVersion();
      try {
        const positions = await this.fetchPositions();
        if (this.isDisposed) return;
        if (positionStateVersion === this.getPositionStateVersion()) {
          this.store.addOrUpdatePositionsFull(positions);
          this.store.update({ loaded: { ...this.store.loaded, positions: true } });
        }
        this.lastPositionSafetyReconciliationAt = Date.now();
      } catch (error: any) {
        this.log(`Position safety reconciliation failed: ${error.message}`, LogSeverity.Warning);
      }
    }

    if (shouldReconcileOrders) {
      try {
        const orders = await this.fetchOrders();
        if (this.isDisposed) return;
        this.store.update({ orders, loaded: { ...this.store.loaded, orders: true } });
        this.lastOrderSafetyReconciliationAt = Date.now();
      } catch (error: any) {
        this.log(`Order safety reconciliation failed: ${error.message}`, LogSeverity.Warning);
      }
    }

    if (!this.isDisposed) {
      loop(() => this.tick(), PRIVATE_STATE_HEALTH_CHECK_INTERVAL_MS);
    }
  };

  private fetchBalance = async () => {
    try {
      const response = await this.primaryAPI.get<AsterDEXAccountResponse>(this.getAccountEndpoint());
      const account = response.data;

      // Use account-level fields for balance calculation
      const totalBalance = parseFloat(account.totalWalletBalance || '0');
      const availableBalance = parseFloat(account.availableBalance || '0');
      const unrealizedPnl = parseFloat(account.totalUnrealizedProfit || '0');

      // Calculate used margin from initial margins (similar to Bybit)
      const usedBalance =
        parseFloat(account.totalInitialMargin || '0') + parseFloat(account.totalOpenOrderInitialMargin || '0');

      return {
        total: totalBalance,
        free: availableBalance,
        used: usedBalance,
        upnl: unrealizedPnl,
      };
    } catch (error: any) {
      this.log(`Failed to fetch balance: ${error.message}`, LogSeverity.Error);
      throw error;
    }
  };

  getAccount = async (): Promise<ExchangeAccount> => {
    try {
      const response = await this.primaryAPI.get<AsterDEXAccountResponse>(this.getAccountEndpoint());
      const account = response.data;

      return {
        userId: account.accountAlias || this.options.key || '', // Use accountAlias from response
        subId: undefined,
      };
    } catch (error: any) {
      this.log(`Failed to get account: ${error.message}`, LogSeverity.Error);
      throw error;
    }
  };

  validateAccount = async (): Promise<ValidateAccountResult> => {
    try {
      if (!this.options.key) {
        throw new Error('Missing required credentials: API key');
      }

      if (!this.options.remoteSigner) {
        const auth = resolveAsterDEXV3AuthContext(this.options);
        const keyLooksLikeWalletAddress = isAsterDEXAddress(this.options.key);
        const secretLooksLikePrivateKey = isAsterDEXV3PrivateKey(this.options.secret);
        if (this.apiVersion === 'v1' && keyLooksLikeWalletAddress && secretLooksLikePrivateKey && !auth.userAddress) {
          throw new Error(ASTERDEX_MAIN_WALLET_REQUIRED_MESSAGE);
        }

        if (this.apiVersion === 'v3') {
          if (!auth.userAddress || !auth.signerAddress || !auth.signerPrivateKey) {
            throw new Error(
              'Missing required Aster v3 auth fields: Main Wallet Address, API Wallet Address, and Generated Private Key',
            );
          }
          if (!isAsterDEXV3PrivateKey(auth.signerPrivateKey)) {
            throw new Error('Invalid Aster API Wallet private key format (expected 64-byte hex)');
          }
        } else if (!this.options.secret) {
          throw new Error('Missing required credentials: API secret');
        }
      }

      await this.getAccount();
      this.log(`Account validated successfully`, LogSeverity.Info);
      return { error: '' };
    } catch (error: any) {
      this.log(`Account validation failed: ${error.message}`, LogSeverity.Error);
      throw error;
    }
  };

  fetchPositionMode = async (): Promise<boolean> => {
    const { data } = await this.primaryAPI.get<AsterDEXPositionModeResponse>(this.getFapiPath('positionSide/dual'));
    const value = data.dualSidePosition;

    if (value === true || value === 'true') return true;
    if (value === false || value === 'false') return false;

    throw new Error(`Unexpected position mode response: ${JSON.stringify(data)}`);
  };

  private syncPositionMode = async (): Promise<void> => {
    const isHedged = await this.fetchPositionMode();
    this.store.setSetting('isHedged', isHedged);
    this.log(`Position mode loaded: ${isHedged ? 'Hedge' : 'One-Way'}`, LogSeverity.Info);
  };

  private getAccountPositionMode = (): PositionMode => {
    return this.store.options.isHedged ? PositionMode.Hedged : PositionMode.OneWay;
  };

  fetchPositions = async (symbols?: string[]): Promise<Position[]> => {
    try {
      const response = await this.primaryAPI.get<AsterDEXPositionResponse[]>(this.getFapiPath('positionRisk'));
      const positions = response.data;
      const leverageBySymbol = new Map<string, number>();
      const accountPositionMode = this.getAccountPositionMode();

      // positionRisk includes leverage for symbols even when positionAmt is 0.
      // Preserve that leverage so quick-order sizing reflects the configured symbol leverage.
      positions.forEach((pos) => {
        const parsedLeverage = Math.abs(parseFloat(pos.leverage || '0'));
        if (Number.isFinite(parsedLeverage) && parsedLeverage > 0) {
          leverageBySymbol.set(pos.symbol, parsedLeverage);
        }
      });

      // Get actual positions (non-zero)
      const actualPositions = positions
        .filter((pos) => !symbols || symbols.includes(pos.symbol))
        .filter((pos) => parseFloat(pos.positionAmt) !== 0)
        .map((pos) => this.transformPosition(pos));

      // Create fake positions (0 contracts) for all tradeable markets so UI metadata
      // reflects the account-wide mode even before there is an open position.
      const fakePositions: Position[] = [];

      for (const market of this.store.markets) {
        if (market.active && !market.isSpot) {
          // Skip if symbols filter is provided and doesn't include this market
          if (symbols && !symbols.includes(market.symbol)) {
            continue;
          }

          // Create both long and short fake positions
          fakePositions.push({
            id: createPositionId(market.symbol, PositionSide.Long, accountPositionMode),
            symbol: market.symbol,
            side: PositionSide.Long,
            entryPrice: 0,
            notional: 0,
            leverage: leverageBySymbol.get(market.symbol) ?? 0,
            unrealizedPnl: 0,
            contracts: 0,
            liquidationPrice: 0,
            marginType: MarginType.Cross, // AsterDEX default
            positionMode: accountPositionMode,
            info: {},
          });

          fakePositions.push({
            id: createPositionId(market.symbol, PositionSide.Short, accountPositionMode),
            symbol: market.symbol,
            side: PositionSide.Short,
            entryPrice: 0,
            notional: 0,
            leverage: leverageBySymbol.get(market.symbol) ?? 0,
            unrealizedPnl: 0,
            contracts: 0,
            liquidationPrice: 0,
            marginType: MarginType.Cross, // AsterDEX default
            positionMode: accountPositionMode,
            info: {},
          });
        }
      }

      return uniqBy([...actualPositions, ...fakePositions], 'id');
    } catch (error: any) {
      this.log(`Failed to fetch positions: ${error.message}`, LogSeverity.Error);
      throw error;
    }
  };

  fetchOrders = async (symbols?: string[]): Promise<Order[]> => {
    try {
      const response = await this.primaryAPI.get<AsterDEXOrderResponse[]>(this.getFapiPath('openOrders'));
      const orders = response.data;

      return orders
        .filter((order) => !symbols || symbols.includes(order.symbol))
        .map((order) => this.transformOrder(order));
    } catch (error: any) {
      this.log(`Failed to fetch orders: ${error.message}`, LogSeverity.Error);
      throw error;
    }
  };

  fetchMarkets = async (): Promise<Market[]> => {
    try {
      const response = await this.publicAPI.get<{ symbols: AsterDEXMarketResponse[] }>(
        this.getFapiPath('exchangeInfo'),
      );
      const markets = response.data.symbols;

      // Log a sample raw market to verify the data structure
      if (markets && markets.length > 0) {
        const sampleRaw = markets.find((m) => m.symbol === 'BTCUSDT') || markets[0];
        this.log(
          `Raw market data (${sampleRaw.symbol}): pricePrecision=${sampleRaw.pricePrecision}, quantityPrecision=${sampleRaw.quantityPrecision}`,
          LogSeverity.Info,
        );
      }

      // Fetch leverage brackets for all symbols
      const leverageBrackets = await this.fetchLeverageBrackets();

      const transformedMarkets = markets
        .filter((market) => market.status === 'TRADING')
        .map((market) => this.transformMarket(market, leverageBrackets));

      return transformedMarkets;
    } catch (error: any) {
      this.log(`Failed to fetch markets: ${error.message}`, LogSeverity.Error);
      throw error;
    }
  };

  private async fetchLeverageBrackets(): Promise<Map<string, number>> {
    const leverageMap = new Map<string, number>();

    try {
      const response = await this.primaryAPI.get<
        | Array<{
            symbol: string;
            brackets: Array<{
              bracket: number;
              initialLeverage: number;
              notionalCap: number;
              notionalFloor: number;
              maintMarginRatio: number;
            }>;
          }>
        | {
            symbol: string;
            brackets: Array<{
              bracket: number;
              initialLeverage: number;
              notionalCap: number;
              notionalFloor: number;
              maintMarginRatio: number;
            }>;
          }
      >(this.getFapiPath('leverageBracket'));

      const bracketData = Array.isArray(response.data) ? response.data : [response.data];

      // Process each symbol's leverage brackets
      bracketData.forEach((item) => {
        // Get the maximum initial leverage from the first bracket (highest leverage tier)
        if (item.brackets && item.brackets.length > 0) {
          const maxLeverage = item.brackets[0].initialLeverage;
          leverageMap.set(item.symbol, maxLeverage);
        }
      });

      this.log(`Loaded leverage brackets for ${leverageMap.size} symbols`, LogSeverity.Info);
    } catch (error: any) {
      this.log(
        `Failed to fetch leverage brackets: ${error.message}. Using default leverage of 10`,
        LogSeverity.Warning,
      );
      // Return empty map, will use default in transformMarket
    }

    return leverageMap;
  }

  fetchTickers = async (): Promise<Ticker[]> => {
    try {
      const response = await this.publicAPI.get<AsterDEXTickerResponse[]>(this.getFapiPath('ticker/24hr'));
      const tickers = response.data;

      // Startup path must stay quick to avoid exchange init timeout.
      // Try only bulk OI fetch here; per-symbol fallback is background-only.
      const openInterestMap = await this.fetchOpenInterestMap({
        symbols: tickers.map((ticker) => ticker.symbol),
        allowPerSymbolFallback: false,
      });
      const transformedTickers = tickers.map((ticker) => this.transformTicker(ticker, openInterestMap));

      return transformedTickers;
    } catch (error: any) {
      this.log(`Failed to fetch tickers: ${error.message}`, LogSeverity.Error);
      throw error;
    }
  };

  fetchPublicTrades = async (symbol: string, limit: number = 500): Promise<PublicTrade[]> => {
    try {
      const response = await this.publicAPI.get<AsterDEXTradeResponse[]>(this.getFapiPath('trades'), {
        params: { symbol, limit },
      });

      return response.data.map((trade) => ({
        id: trade.id.toString(),
        symbol,
        price: parseFloat(trade.price),
        qty: parseFloat(trade.qty),
        timestamp: trade.time,
        side: trade.isBuyerMaker ? ('SELL' as OrderSide) : ('BUY' as OrderSide),
      }));
    } catch (error: any) {
      this.log(`Failed to fetch public trades: ${error.message}`, LogSeverity.Error);
      throw error;
    }
  };

  setLeverage = async (symbol: string, leverage: number, _side?: PositionSide): Promise<void> => {
    try {
      const market = this.store.marketsBySymbol[symbol];
      if (!market) {
        throw new Error(`Market ${symbol} not found, cannot set leverage`);
      }
      const minLeverage = market.limits?.leverage?.min || 1;
      const maxLeverage = market.limits.leverage.max;
      const normalizedLeverage = Number.isFinite(leverage) ? Math.round(leverage) : NaN;

      if (!Number.isFinite(normalizedLeverage)) {
        throw new Error(`Invalid leverage value for ${symbol}: ${leverage}`);
      }

      const clampedLeverage = Math.min(Math.max(normalizedLeverage, minLeverage), maxLeverage);

      await this.primaryAPI.post(this.getFapiPath('leverage'), {
        symbol,
        leverage: clampedLeverage,
      });

      this.log(`Leverage set to ${clampedLeverage}x for ${symbol}`, LogSeverity.Info);
    } catch (error: any) {
      this.log(`Failed to set leverage: ${error.message}`, LogSeverity.Error);
      throw error;
    }
  };

  changePositionMode = async (hedged: boolean, _symbol?: string): Promise<void> => {
    try {
      await this.primaryAPI.post(this.getFapiPath('positionSide/dual'), {
        dualSidePosition: hedged ? 'true' : 'false',
      });

      this.store.setSetting('isHedged', hedged);
      this.log(`Position mode changed to ${hedged ? 'Hedge' : 'One-Way'}`, LogSeverity.Info);

      try {
        const positions = await this.fetchPositions();
        this.store.update({ positions });
      } catch (error: any) {
        this.log(`Failed to refresh positions after position mode change: ${error.message}`, LogSeverity.Warning);
      }
    } catch (error: any) {
      this.log(`Failed to change position mode: ${error.message}`, LogSeverity.Error);
      throw error;
    }
  };

  afterMethodCalledInWorker = <M extends CallableMethodsInWorker>(
    _result: ReturnType<this[M]>,
    method: M,
    ...args: Parameters<this[M]>
  ) => {
    if (method === 'changePositionMode') {
      this.store.setSetting('isHedged', Boolean(args[0]));
    }
  };

  changeMarginType = async (isolated: boolean, symbol?: string): Promise<void> => {
    try {
      if (!symbol) {
        throw new Error('Symbol is required for changing margin type');
      }

      await this.primaryAPI.post(this.getFapiPath('marginType'), {
        symbol,
        marginType: isolated ? 'ISOLATED' : 'CROSSED',
      });

      this.log(`Margin type changed to ${isolated ? 'Isolated' : 'Cross'} for ${symbol}`, LogSeverity.Info);
    } catch (error: any) {
      this.log(`Failed to change margin type: ${error.message}`, LogSeverity.Error);
      throw error;
    }
  };

  fetchSymbolMaxLeverage = async (symbol: string): Promise<number> => {
    try {
      const markets = await this.fetchMarkets();
      const market = markets.find((m) => m.symbol === symbol);

      if (!market) {
        throw new Error(`Market not found for symbol: ${symbol}`);
      }

      return market.limits?.leverage?.max || 10;
    } catch (error: any) {
      this.log(`Failed to fetch max leverage: ${error.message}`, LogSeverity.Error);
      throw error;
    }
  };

  private assertV3Feature = (feature: string): void => {
    if (this.apiVersion === 'v3') return;

    const message = `AsterDEX ${feature} requires API v3`;
    this.emitAndLogError(message);
    throw new Error(message);
  };

  fetchStpMode = async (): Promise<AsterDEXStpMode> => {
    this.assertV3Feature('self-trade prevention');

    const response = await this.primaryAPI.get<{ stpMode?: unknown }>(this.getFapiPath('stpMode'));
    if (!isAsterDEXStpMode(response.data?.stpMode)) {
      const message = 'AsterDEX returned an invalid self-trade-prevention mode';
      this.emitAndLogError(message);
      throw new Error(message);
    }

    return response.data.stpMode;
  };

  changeStpMode = async (stpMode: AsterDEXStpMode): Promise<void> => {
    this.assertV3Feature('self-trade prevention');
    if (!isAsterDEXStpMode(stpMode)) {
      const message = `Invalid AsterDEX self-trade-prevention mode: ${String(stpMode)}`;
      this.emitAndLogError(message);
      throw new Error(message);
    }

    await this.primaryAPI.post(this.getFapiPath('stpMode'), { stpMode });
  };

  placeOrder = async (opts: AsterDEXPlaceOrderOpts, skipAlgoCapacityCheck = false): Promise<string[]> => {
    this.resolveCallerClientOrderId(opts);
    // Aster requires callbackRate for TRAILING_STOP_MARKET, but shared order
    // inputs do not expose it. Reject this before any capacity checks or REST
    // calls so programmatic callers cannot submit an invalid order.
    if (opts.type === OrderType.TrailingStopLoss) {
      const message = 'AsterDEX trailing stop-loss orders are unsupported because callbackRate is unavailable';
      this.emitAndLogError(message);
      throw new Error(message);
    }

    if (opts.stpMode !== undefined) {
      this.assertV3Feature('per-order self-trade prevention');
      if (!isAsterDEXStpMode(opts.stpMode)) {
        const message = `Invalid AsterDEX self-trade-prevention mode: ${String(opts.stpMode)}`;
        this.emitAndLogError(message);
        throw new Error(message);
      }
    }

    // Handle bracket orders (TP/SL attached to entry orders)
    // If takeProfit or stopLoss are provided with a limit/market order, place them as separate orders
    const hasBracketOrders =
      (opts.takeProfit || opts.stopLoss) && (opts.type === OrderType.Limit || opts.type === OrderType.Market);

    if (hasBracketOrders) {
      this.log(
        `Placing bracket order: ${opts.symbol} ${opts.side} ${opts.type} with TP=${opts.takeProfit || 'N/A'} SL=${opts.stopLoss || 'N/A'}`,
        LogSeverity.Info,
      );

      const newAlgoOrders = Number(Boolean(opts.stopLoss)) + Number(Boolean(opts.takeProfit));
      this.assertAlgoOrderCapacity(opts.symbol, newAlgoOrders);

      // Create array of orders: main order + TP + SL
      const ordersToPlace: AsterDEXPlaceOrderOpts[] = [
        // Main entry order (without TP/SL params to avoid recursion)
        {
          ...opts,
          takeProfit: undefined,
          stopLoss: undefined,
        },
      ];

      // Add stop loss order
      if (opts.stopLoss) {
        ordersToPlace.push({
          symbol: opts.symbol,
          side: opts.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy,
          type: OrderType.StopLoss,
          amount: opts.amount,
          triggerPrice: opts.stopLoss,
          reduceOnly: true,
          slTriggerType: opts.slTriggerType,
        });
      }

      // Add take profit order
      if (opts.takeProfit) {
        ordersToPlace.push({
          symbol: opts.symbol,
          side: opts.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy,
          type: OrderType.TakeProfit,
          amount: opts.amount,
          triggerPrice: opts.takeProfit,
          reduceOnly: true,
          tpTriggerType: opts.tpTriggerType,
        });
      }

      // Place all orders using placeOrders (places sequentially).
      // Capacity was already validated above, so skip per-order re-checks.
      return await this.placeOrders(ordersToPlace, true);
    }

    if (!skipAlgoCapacityCheck && (opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit)) {
      this.assertAlgoOrderCapacity(opts.symbol, 1);
    }

    try {
      const orderType = this.mapOrderType(opts.type);
      const side = asterdexOrderSideMap[opts.side];

      // Find market to get precision
      const market = this.store.marketsBySymbol[opts.symbol];
      if (!market) {
        throw new Error(`Market ${opts.symbol} not found`);
      }

      // Split oversized regular orders to respect symbol max quantity filters.
      // This avoids exchange rejections like "-4005 quantity greater than max qty".
      const maxQty = market.limits?.amount?.max;
      const canSplitRegularOrder =
        (opts.type === OrderType.Limit || opts.type === OrderType.Market) &&
        Number.isFinite(maxQty) &&
        (maxQty as number) > 0 &&
        opts.amount > (maxQty as number);

      if (canSplitRegularOrder) {
        const adjustedTotalAmount = adjust(opts.amount, market.precision.amount);
        const adjustedMaxQty = adjust(maxQty as number, market.precision.amount);
        if (adjustedTotalAmount > adjustedMaxQty) {
          const splitOrders: PlaceOrderOpts[] = [];
          let remainingAmount = adjustedTotalAmount;

          while (remainingAmount > adjustedMaxQty) {
            splitOrders.push({
              ...opts,
              amount: adjustedMaxQty,
            });
            remainingAmount = adjust(remainingAmount - adjustedMaxQty, market.precision.amount);
          }

          if (remainingAmount > 0) {
            splitOrders.push({
              ...opts,
              amount: remainingAmount,
            });
          }

          this.log(
            `Splitting oversized order for ${opts.symbol}: ${adjustedTotalAmount} into ${splitOrders.length} chunk(s) (max ${adjustedMaxQty})`,
            LogSeverity.Info,
          );

          return await this.placeOrders(splitOrders, skipAlgoCapacityCheck);
        }
      }

      const orderParams: any = {
        symbol: opts.symbol,
        side,
        type: orderType,
      };
      if (opts.stpMode !== undefined) {
        orderParams.stpMode = opts.stpMode;
      }
      if (this.apiVersion === 'v3') {
        const builderFeeConfig = resolveAsterBuilderFeeConfig(this.options);
        if (builderFeeConfig) {
          const feeRate = getAsterBuilderFeeRateForOrder(
            {
              type: opts.type,
              timeInForce: opts.timeInForce,
              postOnly: (opts as { readonly postOnly?: boolean }).postOnly,
            },
            orderType,
            builderFeeConfig,
          );
          if (feeRate) {
            orderParams.builder = builderFeeConfig.builderAddress;
            orderParams.feeRate = feeRate;
          }
        }
      }
      const positionSide = this.getOrderPositionSide(opts);
      if (positionSide !== 'BOTH') {
        orderParams.positionSide = positionSide;
      }

      // Handle close position orders (amount = 0 for TP/SL market orders)
      const isClosePosition =
        opts.amount === 0 && (opts.type === OrderType.TakeProfit || opts.type === OrderType.StopLoss);

      if (isClosePosition) {
        // For close position orders, set closePosition flag instead of quantity
        // No need for reduceOnly when closePosition is set
        orderParams.closePosition = 'true';
      } else {
        // Normal orders require quantity - use market precision
        const adjustedAmount = adjust(opts.amount, market.precision.amount);
        orderParams.quantity = adjustedAmount.toString();

        // Add reduce only flag (not needed when closePosition is set)
        if (opts.reduceOnly && !this.store.options.isHedged) {
          orderParams.reduceOnly = 'true';
        }
      }

      // Add timeInForce only for order types that support it.
      // STOP_MARKET / TAKE_PROFIT_MARKET / TRAILING_STOP_MARKET should not include timeInForce.
      if (orderType === 'LIMIT' || orderType === 'STOP' || orderType === 'TAKE_PROFIT') {
        const timeInForce = opts.timeInForce ? asterdexTimeInForceMap[opts.timeInForce] : 'GTC';
        orderParams.timeInForce = timeInForce;
      }

      // Get tick size from PRICE_FILTER for price adjustments
      const tickSize = this.getTickSize(market);

      // Add price for limit orders - use tick size from PRICE_FILTER
      if (opts.type === OrderType.Limit && opts.price) {
        const adjustedPrice = adjust(opts.price, tickSize);
        orderParams.price = adjustedPrice.toString();
      }

      // Add stop price for stop loss and take profit orders - use tick size from PRICE_FILTER
      if ((opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit) && opts.triggerPrice) {
        const adjustedStopPrice = adjust(opts.triggerPrice, tickSize);
        orderParams.stopPrice = adjustedStopPrice.toString();

        // Add workingType for trigger type (MARK_PRICE or CONTRACT_PRICE)
        // Match Binance defaults: TP defaults to LastPrice (CONTRACT_PRICE), SL defaults to MarkPrice (MARK_PRICE)
        if (opts.type === OrderType.TakeProfit) {
          orderParams.workingType =
            opts.tpTriggerType === OrderStopTriggerType.MarkPrice ? 'MARK_PRICE' : 'CONTRACT_PRICE';
        } else {
          orderParams.workingType =
            opts.slTriggerType === OrderStopTriggerType.LastPrice ? 'CONTRACT_PRICE' : 'MARK_PRICE';
        }
      }

      // Client order ID is not supported in PlaceOrderOpts

      const response = await this.secondaryAPI.post<AsterDEXOrderResponse>(this.getFapiPath('order'), orderParams);
      const rawResponse = response.data as { orderId?: string | number; data?: { orderId?: string | number } };
      const orderId: string | number | undefined = rawResponse.orderId ?? rawResponse.data?.orderId;
      if (orderId === undefined || orderId === null || orderId === '') {
        throw new Error('AsterDEX order response did not include orderId');
      }

      this.log(`Order placed: ${orderId}`, LogSeverity.Info);
      return [orderId.toString()];
    } catch (error: any) {
      this.log(`Failed to place order: ${error.message}`, LogSeverity.Error);
      throw error;
    }
  };

  placeOrders = async (orders: AsterDEXPlaceOrderOpts[], skipAlgoCapacityCheck = false): Promise<string[]> => {
    this.assertCallerClientOrderIds(orders);
    const orderIds: string[] = [];
    let errMsg = '';

    for (const opts of orders) {
      try {
        const result = await this.placeOrder(opts, skipAlgoCapacityCheck);
        if (result && result.length > 0) {
          orderIds.push(...result);
        }
      } catch (error: any) {
        errMsg = error.message || String(error);
        this.log(`Failed to place order: ${errMsg}`, LogSeverity.Error);
        // Continue placing remaining orders even if one fails
      }
    }

    if (errMsg !== '' && orderIds.length === 0) {
      // Only throw if all orders failed
      throw new Error(errMsg);
    }

    if (orderIds.length > 0) {
      this.log(`Orders placed: ${orderIds.join(', ')}`, LogSeverity.Info);
    }

    return orderIds;
  };

  updateOrder = async (opts: UpdateOrderOpts): Promise<string[]> => {
    // AsterDEX doesn't support direct order updates via PUT /fapi/<version>/order
    // Instead, we cancel the old order and place a new one from scratch

    // Create a completely new order with the updated values
    const newOrder: Writable<PlaceOrderOpts> = {
      symbol: opts.order.symbol,
      type: opts.order.type,
      side: opts.order.side,
      amount: 'amount' in opts.update ? opts.update.amount : opts.order.amount,
      reduceOnly: opts.order.reduceOnly || false,
    };

    // Handle price based on order type
    if (opts.order.type === OrderType.TakeProfit || opts.order.type === OrderType.StopLoss) {
      // For TP/SL orders, the Order.price field contains the trigger price
      // These map to STOP_MARKET and TAKE_PROFIT_MARKET which don't have a limit price
      const triggerPrice = 'price' in opts.update ? opts.update.price : opts.order.price;
      newOrder.triggerPrice = triggerPrice;
      // No price field needed for market stop orders
    } else {
      // For regular orders (limit/market), use the price field normally
      newOrder.price = 'price' in opts.update ? opts.update.price : opts.order.price;
    }

    // Add trigger types if they exist on the original order
    if (opts.order.tpTriggerType) {
      newOrder.tpTriggerType = opts.order.tpTriggerType;
    }
    if (opts.order.slTriggerType) {
      newOrder.slTriggerType = opts.order.slTriggerType;
    }

    // Add time in force if it exists on the original order
    if (opts.order.timeInForce) {
      newOrder.timeInForce = opts.order.timeInForce;
    }

    // Cancel the old order and place the new one
    await this.cancelOrders([opts.order]);
    return await this.placeOrder(newOrder);
  };

  cancelOrders = async (orders: Order[]): Promise<void> => {
    // Cancel orders individually instead of using batch endpoint for better reliability
    // Following BingX approach
    if (orders.length === 0) return;

    let errMsg = '';
    const cancelledIds: string[] = [];

    for (const order of orders) {
      try {
        await this.secondaryAPI.delete(this.getFapiPath('order'), {
          __suppressAsterUnknownOrderError: true,
          params: {
            symbol: order.symbol,
            orderId: order.id,
          },
        } as any);
        cancelledIds.push(order.id);
      } catch (error: any) {
        if (isAsterDEXUnknownOrderError(error)) {
          this.log(`Order ${order.id} was already gone on AsterDEX; treating cancel as complete`, LogSeverity.Info);
          cancelledIds.push(order.id);
          continue;
        }

        errMsg = error.message || String(error);
        this.log(`Failed to cancel order ${order.id}: ${errMsg}`, LogSeverity.Error);
        // Continue cancelling remaining orders even if one fails
      }
    }

    if (errMsg !== '' && cancelledIds.length === 0) {
      // Only throw if all cancellations failed
      throw new Error(errMsg);
    }

    if (cancelledIds.length > 0) {
      this.log(`Orders cancelled: ${cancelledIds.join(', ')}`, LogSeverity.Info);
    }
  };

  fetchOHLCV = async (opts: OHLCVOptions): Promise<Candle[]> => {
    try {
      const interval = this.mapTimeframe(opts.interval);
      const params: any = {
        symbol: opts.symbol,
        interval,
        limit: opts.limit || 500,
      };

      if (opts.from) {
        params.startTime = opts.from;
      }

      // AsterDEX returns klines as arrays like Binance
      // [openTime, open, high, low, close, volume, closeTime, quoteAssetVolume, trades, takerBuyBaseAssetVolume, takerBuyQuoteAssetVolume, ignore]
      const response = await this.publicAPI.get<AsterDEXKlineResponse[]>(this.getFapiPath('klines'), { params });

      return response.data.map((candle) => ({
        timestamp: candle[0], // openTime
        open: parseFloat(candle[1]),
        high: parseFloat(candle[2]),
        low: parseFloat(candle[3]),
        close: parseFloat(candle[4]),
        volume: parseFloat(candle[5]),
      }));
    } catch (error: any) {
      this.log(`Failed to fetch OHLCV: ${error.message}`, LogSeverity.Error);
      throw error;
    }
  };

  fetchHistoricalFills = async (opts: ExecutionsOptions): Promise<Execution[]> => {
    if (this.isPublicDataOnly) {
      return [];
    }

    const { symbol, orderId, from, to, limit = 500 } = opts;
    if (!symbol) {
      return [];
    }

    try {
      const params: Record<string, string | number> = {
        symbol,
        limit: Math.min(limit, 1000),
      };

      if (orderId) {
        params.orderId = orderId;
      }
      if (from) params.startTime = from;
      if (to) params.endTime = to;

      const response = await this.primaryAPI.get<AsterDEXUserTradeResponse[]>(this.getFapiPath('userTrades'), {
        params,
      });
      const trades = Array.isArray(response.data) ? response.data : [];
      // Ported from `fetchHistoricalTrades`, which read the SAME `userTrades`
      // rows through a second mapper that had drifted ahead of this one. The
      // venue reports realized PnL as a running total rather than per fill, so
      // the deltas must be normalized — reading the raw field per row would
      // attribute the whole position's PnL to every fill in it.
      const normalizedRealizedPnls = normalizeAsterDEXRealizedPnlDeltas(trades);
      const executions: Execution[] = [];

      for (const [index, trade] of trades.entries()) {
        const price = parseFloat(trade.price);
        const qty = parseFloat(trade.qty);
        if (!Number.isFinite(price) || !Number.isFinite(qty) || qty <= 0) {
          continue;
        }

        const rawPositionSide = String(trade.positionSide || '').toUpperCase();
        const positionSide: PositionSide | 'both' =
          rawPositionSide === 'LONG' ? PositionSide.Long : rawPositionSide === 'SHORT' ? PositionSide.Short : 'both';

        executions.push({
          // Per-fill dedupe key. orderId cannot serve: one order fills across many rows.
          id: trade.id != null ? String(trade.id) : undefined,
          // Chart execution marks expect seconds-based timestamps.
          timestamp: Math.floor(trade.time / 1000),
          symbol: trade.symbol,
          orderId: trade.orderId?.toString(),
          price,
          qty,
          side: trade.side === 'BUY' ? OrderSide.Buy : OrderSide.Sell,
          realizedPnl: normalizedRealizedPnls[index],
          positionSide,
          maker: trade.maker,
          commission: trade.commission ? parseFloat(trade.commission) : undefined,
          commissionAsset: trade.commissionAsset,
        });
      }

      return executions;
    } catch (error: any) {
      this.log(`Failed to fetch executions: ${error.message}`, LogSeverity.Error);
      return [];
    }
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void): (() => void) => {
    // This will be implemented via WebSocket
    const interval = this.mapTimeframe(opts.interval);
    const stream = `${opts.symbol.toLowerCase()}@kline_${interval}`;

    if (this.publicWebsocket) {
      this.publicWebsocket.subscribeToKline(stream, callback);
    }

    return () => {
      if (this.publicWebsocket) {
        this.publicWebsocket.unsubscribeFromKline(stream);
      }
    };
  };

  fetchOrderBook = async (symbol: string, limit: number = 100): Promise<OrderBook> => {
    try {
      const params: any = {
        symbol,
        limit, // Valid limits: 5, 10, 20, 50, 100, 500, 1000
      };

      const response = await this.publicAPI.get<AsterDEXDepthResponse>(this.getFapiPath('depth'), { params });
      const depth = response.data;

      // Parse and sort bids (highest price first)
      const bids = depth.bids
        .map(([price, qty]) => ({
          price: parseFloat(price),
          amount: parseFloat(qty),
          total: 0, // Will be calculated after sorting
        }))
        .sort((a, b) => b.price - a.price); // Sort descending

      // Parse and sort asks (lowest price first)
      const asks = depth.asks
        .map(([price, qty]) => ({
          price: parseFloat(price),
          amount: parseFloat(qty),
          total: 0, // Will be calculated after sorting
        }))
        .sort((a, b) => a.price - b.price); // Sort ascending

      // Calculate cumulative totals after sorting
      let bidTotal = 0;
      bids.forEach((bid) => {
        bidTotal += bid.price * bid.amount;
        bid.total = bidTotal;
      });

      let askTotal = 0;
      asks.forEach((ask) => {
        askTotal += ask.price * ask.amount;
        ask.total = askTotal;
      });

      const orderBook: OrderBook = {
        bids,
        asks,
      };

      return orderBook;
    } catch (error: any) {
      this.log(`Failed to fetch order book: ${error.message}`, LogSeverity.Error);
      throw error;
    }
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void): (() => void) => {
    return this.publicWebsocket.listenOrderBook(symbol, callback);
  };

  // Helper methods for data transformation
  private transformPosition(pos: AsterDEXPositionResponse): Position {
    const amount = parseFloat(pos.positionAmt);
    const parsedLeverage = Math.abs(parseFloat(pos.leverage || '0')) || 1;
    const positionMode = pos.positionSide === 'BOTH' || !pos.positionSide ? PositionMode.OneWay : PositionMode.Hedged;

    // In one-way mode, positionSide is "BOTH", so determine side from amount sign
    let side: PositionSide;
    if (positionMode === PositionMode.OneWay) {
      side = amount > 0 ? PositionSide.Long : PositionSide.Short;
    } else {
      side = pos.positionSide === 'LONG' ? PositionSide.Long : PositionSide.Short;
    }

    const positionId = createPositionId(pos.symbol, side, positionMode);
    const existingPosition = this.store.positions.find((p) => p.id === positionId);

    return {
      id: positionId,
      symbol: pos.symbol,
      side,
      contracts: Math.abs(amount),
      entryPrice: parseFloat(pos.entryPrice),
      notional: parseFloat(pos.notional),
      leverage: parsedLeverage,
      unrealizedPnl: parseFloat(pos.unRealizedProfit),
      // positionRisk endpoint does not expose realized PnL; preserve the latest WS-derived value.
      realizedPnl: existingPosition?.realizedPnl ?? 0,
      marginType: pos.marginType === 'ISOLATED' ? MarginType.Isolated : MarginType.Cross,
      liquidationPrice: parseFloat(pos.liquidationPrice || '0'),
      positionMode,
      info: pos, // Store raw position data
    };
  }

  private transformOrder(order: AsterDEXOrderResponse): Order {
    // For stop orders, use stopPrice as the trigger price
    // For regular orders, use price
    const price =
      order.stopPrice && parseFloat(order.stopPrice) > 0 ? parseFloat(order.stopPrice) : parseFloat(order.price);

    return {
      id: order.orderId.toString(),
      symbol: order.symbol,
      type: this.reverseMapOrderType(order.type),
      side: order.side === 'BUY' ? OrderSide.Buy : OrderSide.Sell,
      price,
      amount: parseFloat(order.origQty),
      filled: parseFloat(order.executedQty),
      remaining: parseFloat(order.origQty) - parseFloat(order.executedQty),
      status: this.reverseMapOrderStatus(order.status),
      reduceOnly: order.closePosition || order.reduceOnly || false,
      lastUpdate: order.time,
      info: order, // Store raw order data
    };
  }

  private transformMarket(market: AsterDEXMarketResponse, leverageBrackets: Map<string, number>): Market {
    const pricePrecision = market.pricePrecision;
    const amountPrecision = market.quantityPrecision;

    // Convert integer precision to decimal (e.g., 2 -> 0.01)
    const pricePrecisionDecimal = parseFloat(precisionToString(pricePrecision));
    const amountPrecisionDecimal = parseFloat(precisionToString(amountPrecision));

    // Get max leverage from brackets, default to 10 if not found
    const maxLeverage = leverageBrackets.get(market.symbol) || 10;

    return {
      id: market.symbol,
      symbol: market.symbol,
      base: market.baseAsset,
      quote: market.quoteAsset,
      active: market.status === 'TRADING',
      contractSize: 1,
      precision: {
        price: pricePrecisionDecimal,
        amount: amountPrecisionDecimal,
      },
      limits: {
        amount: {
          min: this.extractFilterValue(market.filters, 'LOT_SIZE', 'minQty'),
          max: this.extractFilterValue(market.filters, 'LOT_SIZE', 'maxQty'),
        },
        leverage: {
          min: 1,
          max: maxLeverage,
        },
      },
      info: market, // Store raw market data for reference
    };
  }

  private getOrderPositionSide = (
    opts: Pick<PlaceOrderOpts, 'side' | 'type' | 'reduceOnly'>,
  ): 'BOTH' | 'LONG' | 'SHORT' => {
    if (!this.store.options.isHedged) return 'BOTH';

    let positionSide: 'LONG' | 'SHORT' = opts.side === OrderSide.Buy ? 'LONG' : 'SHORT';

    if (
      opts.type === OrderType.StopLoss ||
      opts.type === OrderType.TakeProfit ||
      opts.type === OrderType.TrailingStopLoss ||
      opts.reduceOnly
    ) {
      positionSide = positionSide === 'LONG' ? 'SHORT' : 'LONG';
    }

    return positionSide;
  };

  private fetchOpenInterestMap = async ({
    symbols,
    allowPerSymbolFallback = true,
  }: {
    symbols?: string[];
    allowPerSymbolFallback?: boolean;
  } = {}): Promise<Map<string, number>> => {
    const map = new Map<string, number>();
    const endpoint = this.getFapiPath('openInterest');

    try {
      // Prefer a single all-symbol request when supported.
      const response = await this.publicAPI.get<any>(endpoint, { timeout: 2000 });
      this.collectOpenInterestFromPayload(response.data, map);
    } catch (error: any) {
      this.log(`Bulk open-interest fetch failed: ${error.message}`, LogSeverity.Warning);
    }

    if (map.size > 0 || !allowPerSymbolFallback || !symbols?.length) {
      return map;
    }

    const limitedSymbols = symbols.filter(Boolean);
    for (const symbol of limitedSymbols) {
      try {
        const response = await this.publicAPI.get<AsterDEXOpenInterestResponse>(endpoint, {
          params: { symbol },
          timeout: 2000,
        });
        this.collectOpenInterestFromPayload(response.data, map);
      } catch {
        // Keep best-effort behavior; missing OI for one symbol should not block the rest.
      }
    }

    return map;
  };

  private collectOpenInterestFromPayload = (payload: unknown, map: Map<string, number>): void => {
    const collect = (item: any) => {
      const symbol = typeof item?.symbol === 'string' ? item.symbol : '';
      const openInterest = Number(item?.openInterest);
      if (symbol && Number.isFinite(openInterest)) {
        map.set(symbol, openInterest);
      }
    };

    if (Array.isArray(payload)) {
      payload.forEach(collect);
      return;
    }

    if (payload && typeof payload === 'object') {
      const root = payload as any;
      if (Array.isArray(root.data)) {
        root.data.forEach(collect);
        return;
      }
      if (root.data && typeof root.data === 'object') {
        collect(root.data);
        return;
      }
      collect(root);
    }
  };

  private updateStoreOpenInterest = (openInterestBySymbol: Map<string, number>): void => {
    if (openInterestBySymbol.size === 0) {
      return;
    }

    for (const [symbol, openInterest] of openInterestBySymbol.entries()) {
      const ticker = this.store.tickersBySymbol[symbol];
      if (!ticker) continue;
      if (ticker.openInterest === openInterest) continue;
      this.store.updateTicker(ticker, { openInterest });
    }
  };

  private pollOpenInterest = async (): Promise<void> => {
    if (this.isDisposed) return;

    try {
      const symbols = this.store.tickers.map((ticker) => ticker.symbol).filter(Boolean);
      if (symbols.length === 0) {
        return;
      }

      let openInterestBySymbol = await this.fetchOpenInterestMap({ allowPerSymbolFallback: false });

      // If bulk endpoint is unavailable, degrade to a rotating symbol slice.
      if (openInterestBySymbol.size === 0) {
        const batchSize = Math.min(AsterDEXExchange.OPEN_INTEREST_BATCH_SIZE, symbols.length);
        const start = this.openInterestCursor % symbols.length;
        const end = start + batchSize;
        const batch =
          end <= symbols.length
            ? symbols.slice(start, end)
            : [...symbols.slice(start), ...symbols.slice(0, end - symbols.length)];
        this.openInterestCursor = (start + batchSize) % symbols.length;
        openInterestBySymbol = await this.fetchOpenInterestMap({
          symbols: batch,
          allowPerSymbolFallback: true,
        });
      }

      this.updateStoreOpenInterest(openInterestBySymbol);
    } catch (error: any) {
      this.log(`Open-interest poll failed: ${error.message}`, LogSeverity.Warning);
    } finally {
      if (!this.isDisposed) {
        loop(() => this.pollOpenInterest(), AsterDEXExchange.OPEN_INTEREST_POLL_INTERVAL_MS);
      }
    }
  };

  private startOpenInterestPolling = (): void => {
    void this.pollOpenInterest();
  };

  private transformTicker(ticker: AsterDEXTickerResponse, openInterestBySymbol?: Map<string, number>): Ticker {
    const openInterest = openInterestBySymbol?.get(ticker.symbol) ?? 0;
    return {
      id: ticker.symbol,
      symbol: ticker.symbol,
      bid: 0, // Will be updated from orderbook depth
      ask: 0, // Will be updated from orderbook depth
      last: parseFloat(ticker.lastPrice),
      mark: 0, // Will be updated from mark price stream
      index: 0, // Will be updated from index price stream
      percentage: parseFloat(ticker.priceChangePercent),
      fundingRate: 0, // Will be updated from funding rate stream
      fundingTime: 0,
      openInterest,
      volume: parseFloat(ticker.volume),
      quoteVolume: parseFloat(ticker.quoteVolume),
      info: ticker, // Store raw ticker data for reference
    };
  }

  private extractFilterValue(filters: any[], filterType: string, field: string): number {
    const filter = filters.find((f) => f.filterType === filterType);
    return filter && filter[field] ? parseFloat(filter[field]) : 0;
  }

  private getTickSize(market: Market): number {
    // Extract tick size from PRICE_FILTER in market.info
    if (market.info && market.info.filters) {
      const priceFilter = market.info.filters.find((f: any) => f.filterType === 'PRICE_FILTER');
      if (priceFilter && priceFilter.tickSize) {
        return parseFloat(priceFilter.tickSize);
      }
    }
    // Fallback to market.precision.price if filter not found
    return market.precision.price;
  }

  private getMaxAlgoOrdersLimit(market?: Market): number | null {
    if (!market?.info?.filters || !Array.isArray(market.info.filters)) {
      return null;
    }

    const filter = market.info.filters.find((f: any) => f.filterType === 'MAX_NUM_ALGO_ORDERS');
    if (!filter) {
      return null;
    }

    const limit = Number(filter.limit);
    return Number.isFinite(limit) ? limit : null;
  }

  private isAlgoOrder(order: Order): boolean {
    if (
      order.type === OrderType.StopLoss ||
      order.type === OrderType.TakeProfit ||
      order.type === OrderType.TrailingStopLoss
    ) {
      return true;
    }

    const rawType = typeof order.info?.type === 'string' ? order.info.type : '';
    return (
      rawType === 'STOP' ||
      rawType === 'STOP_MARKET' ||
      rawType === 'TAKE_PROFIT' ||
      rawType === 'TAKE_PROFIT_MARKET' ||
      rawType === 'TRAILING_STOP_MARKET'
    );
  }

  private getOpenAlgoOrdersCount(symbol: string): number {
    return this.store.orders.filter(
      (order) => order.symbol === symbol && order.status === OrderStatus.Open && this.isAlgoOrder(order),
    ).length;
  }

  private assertAlgoOrderCapacity(symbol: string, newAlgoOrders: number): void {
    if (newAlgoOrders <= 0) {
      return;
    }

    const market = this.store.marketsBySymbol[symbol];
    const maxAlgoOrders = this.getMaxAlgoOrdersLimit(market);
    if (!maxAlgoOrders) {
      return;
    }

    const existingAlgoOrders = this.getOpenAlgoOrdersCount(symbol);
    const projectedTotal = existingAlgoOrders + newAlgoOrders;

    if (projectedTotal > maxAlgoOrders) {
      throw new Error(
        `AsterDEX max algo order limit reached for ${symbol}: ${existingAlgoOrders}/${maxAlgoOrders} open, ${newAlgoOrders} new requested`,
      );
    }
  }

  private mapOrderType(type: OrderType): string {
    if (type === OrderType.TrailingStopLoss) {
      throw new Error('AsterDEX trailing stop-loss orders are unsupported because callbackRate is unavailable');
    }
    return asterdexOrderTypeMap[type] || type;
  }

  private reverseMapOrderType(type: string): OrderType {
    const mapping: Record<string, OrderType> = {
      LIMIT: OrderType.Limit,
      MARKET: OrderType.Market,
      STOP: OrderType.StopLoss,
      STOP_MARKET: OrderType.StopLoss,
      TAKE_PROFIT: OrderType.TakeProfit,
      TAKE_PROFIT_MARKET: OrderType.TakeProfit,
      TRAILING_STOP: OrderType.TrailingStopLoss,
      TRAILING_STOP_MARKET: OrderType.TrailingStopLoss,
    };
    return mapping[type] || (type as OrderType);
  }

  private reverseMapOrderStatus(status: string): OrderStatus {
    const mapping: Record<string, OrderStatus> = {
      NEW: OrderStatus.Open,
      PARTIALLY_FILLED: OrderStatus.Open,
      FILLED: OrderStatus.Closed,
      CANCELED: OrderStatus.Canceled,
      REJECTED: OrderStatus.Canceled,
      EXPIRED: OrderStatus.Canceled,
    };
    return mapping[status] || OrderStatus.Open;
  }

  private reverseMapTimeInForce(tif: string): string {
    // TimeInForce is just a string in this codebase
    return tif || 'GTC';
  }

  private mapTimeframe(timeframe: Timeframe): string {
    const mapping: Record<Timeframe, string> = {
      '1m': '1m',
      '3m': '3m',
      '5m': '5m',
      '15m': '15m',
      '30m': '30m',
      '1h': '1h',
      '2h': '2h',
      '4h': '4h',
      '6h': '6h',
      // '8h': '8h', // Not in Timeframe type
      '12h': '12h',
      '1d': '1d',
      '1w': '1w',
    };
    return mapping[timeframe] || '1h';
  }

  dispose = (): void => {
    // Stop the virtual clock
    asterDEXVirtualClock.stop();

    if (this.publicWebsocket) {
      this.publicWebsocket.dispose();
    }

    if (this.privateWebsocket) {
      this.privateWebsocket.dispose();
    }

    super.dispose();
  };
}
