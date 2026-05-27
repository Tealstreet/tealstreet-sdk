import type { Axios } from 'axios';
import type { Store } from '../../store/store.interface';
import type { DydxSDKClient } from './dydx.sdk';

import rateLimit from 'axios-rate-limit';
import sumBy from 'lodash/sumBy';
import { forEachSeries } from 'p-iteration';

import {
  Balance,
  Candle,
  ExchangeName,
  ExchangeOptions,
  LogSeverity,
  MarginType,
  Market,
  OHLCVOptions,
  Order,
  OrderBook,
  OrderSide,
  OrderStatus,
  OrderTimeInForce,
  OrderType,
  PlaceOrderOpts,
  Position,
  PositionMode,
  PositionSide,
  Ticker,
  UpdateOrderOpts,
  Writable,
} from '../../types';
import { createPositionId } from '../../utils/createPositionId';
import { BaseExchange } from '../base';
import { createAPI } from './dydx.api';
import { initializeDydxSDK } from './dydx.sdk';
import {
  ASSET_TYPES,
  CandleResolution,
  DydxCandle,
  DydxMarket,
  DydxMarketsResponse,
  DydxOrderFlags,
  DydxOrderSide,
  DydxOrderStatus,
  DydxOrderType,
  DydxPositionSide,
  DydxTimeInForce,
  ENDPOINTS,
  GRAFANA_ENDPOINTS_EVERY,
  GRAFANA_ENDPOINTS_PERIODIC,
} from './dydx.types';
import { DydxPrivateWebSocket } from './dydx.ws-private';
import { DydxPublicWebSocket } from './dydx.ws-public';

export class DydxExchange extends BaseExchange {
  xhr: Axios;
  publicWebsocket: DydxPublicWebSocket;
  privateWebsocket: DydxPrivateWebSocket;
  sdkClient: DydxSDKClient | null = null;

  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;
  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;

  // Market data cache
  private marketsCache: Map<string, DydxMarket> = new Map();
  private lastClientId: number = 0;

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    this.xhr = rateLimit(createAPI(opts, this), { maxRPS: 10 });
    this.publicWebsocket = new DydxPublicWebSocket(this);
    this.privateWebsocket = new DydxPrivateWebSocket(this);
  }

  /**
   * Generate a unique client ID for order placement
   * Client IDs must be unique uint32 values (0 to 2^32 - 1)
   */
  private generateClientId(): number {
    const now = Date.now();
    // Use timestamp-based approach with counter for uniqueness
    this.lastClientId = Math.max(now % 0xffffffff, this.lastClientId + 1);
    return this.lastClientId;
  }

  /**
   * Get the trading address for balance/position queries
   * Uses dydxAddress if provided (for separate trading wallet), otherwise falls back to API wallet (key)
   */
  private getTradingAddress(): string {
    return this.options.dydxAddress || this.options.key;
  }

  /**
   * Convert safe-cex interval to dYdX CandleResolution
   */
  private convertInterval(interval: string): CandleResolution {
    const mapping: Record<string, CandleResolution> = {
      '1m': CandleResolution.ONE_MIN,
      '5m': CandleResolution.FIVE_MINS,
      '15m': CandleResolution.FIFTEEN_MINS,
      '30m': CandleResolution.THIRTY_MINS,
      '1h': CandleResolution.ONE_HOUR,
      '4h': CandleResolution.FOUR_HOURS,
      '1d': CandleResolution.ONE_DAY,
    };

    return mapping[interval] || CandleResolution.ONE_MIN;
  }

  /**
   * Convert dYdX OrderSide to safe-cex OrderSide
   */
  private dydxOrderSideToSafeCex(side: DydxOrderSide): OrderSide {
    return side === DydxOrderSide.BUY ? OrderSide.Buy : OrderSide.Sell;
  }

  /**
   * Convert safe-cex OrderSide to dYdX OrderSide
   */
  private safeCexOrderSideToDydx(side: OrderSide): DydxOrderSide {
    return side === OrderSide.Buy ? DydxOrderSide.BUY : DydxOrderSide.SELL;
  }

  /**
   * Convert safe-cex OrderType to dYdX OrderType
   */
  private safeCexOrderTypeToDydx(type: OrderType): DydxOrderType {
    const mapping: Record<OrderType, DydxOrderType> = {
      [OrderType.Market]: DydxOrderType.MARKET,
      [OrderType.Limit]: DydxOrderType.LIMIT,
      [OrderType.Conditional]: DydxOrderType.STOP_MARKET,
      [OrderType.StopLoss]: DydxOrderType.STOP_MARKET,
      [OrderType.TakeProfit]: DydxOrderType.TAKE_PROFIT_MARKET,
      [OrderType.TrailingStopLoss]: DydxOrderType.STOP_MARKET,
    };

    return mapping[type] || DydxOrderType.LIMIT;
  }

  /**
   * Convert dYdX OrderType to safe-cex OrderType
   */
  private dydxOrderTypeToSafeCex(type: DydxOrderType): OrderType {
    const mapping: Record<DydxOrderType, OrderType> = {
      [DydxOrderType.MARKET]: OrderType.Market,
      [DydxOrderType.LIMIT]: OrderType.Limit,
      [DydxOrderType.STOP_MARKET]: OrderType.StopLoss,
      [DydxOrderType.STOP_LIMIT]: OrderType.StopLoss,
      [DydxOrderType.TAKE_PROFIT_MARKET]: OrderType.TakeProfit,
      [DydxOrderType.TAKE_PROFIT_LIMIT]: OrderType.TakeProfit,
    };

    return mapping[type] || OrderType.Limit;
  }

  /**
   * Convert safe-cex OrderTimeInForce to dYdX TimeInForce
   */
  private safeCexTimeInForceToDydx(tif?: OrderTimeInForce): DydxTimeInForce {
    if (!tif) return DydxTimeInForce.GTT;

    const mapping: Record<OrderTimeInForce, DydxTimeInForce> = {
      [OrderTimeInForce.GoodTillCancel]: DydxTimeInForce.GTT,
      [OrderTimeInForce.ImmediateOrCancel]: DydxTimeInForce.IOC,
      [OrderTimeInForce.FillOrKill]: DydxTimeInForce.FOK,
      [OrderTimeInForce.PostOnly]: DydxTimeInForce.GTT, // POST_ONLY is handled via execution flag
    };

    return mapping[tif] || DydxTimeInForce.GTT;
  }

  /**
   * Convert dYdX OrderStatus to safe-cex OrderStatus
   */
  private dydxOrderStatusToSafeCex(status: DydxOrderStatus): OrderStatus {
    const mapping: Record<DydxOrderStatus, OrderStatus> = {
      [DydxOrderStatus.OPEN]: OrderStatus.Open,
      [DydxOrderStatus.PARTIALLY_FILLED]: OrderStatus.Open,
      [DydxOrderStatus.FILLED]: OrderStatus.Closed,
      [DydxOrderStatus.CANCELED]: OrderStatus.Canceled,
      [DydxOrderStatus.BEST_EFFORT_CANCELED]: OrderStatus.Canceled,
      [DydxOrderStatus.UNTRIGGERED]: OrderStatus.Open,
    };

    return mapping[status] || OrderStatus.Open;
  }

  /**
   * Get current block height from SDK
   */
  private async getCurrentBlockHeight(): Promise<number> {
    if (!this.sdkClient) {
      throw new Error('SDK client not initialized');
    }

    try {
      return await this.sdkClient.getCurrentBlockHeight();
    } catch (err) {
      this.log(`Error getting block height: ${err}`, LogSeverity.Error);
      // Return a safe default (current time-based estimate)
      return Math.floor(Date.now() / 1000);
    }
  }

  validateAccount = async () => {
    try {
      // Initialize SDK client
      this.log('Initializing dYdX SDK client...', LogSeverity.Info);
      this.sdkClient = await initializeDydxSDK(this.options);

      if (!this.sdkClient) {
        throw new Error('SDK client initialization returned null/undefined');
      }

      // Verify we can fetch subaccount data (404 is OK - means empty account)
      const address = this.getTradingAddress();
      const subaccount = this.options.dydxSubaccountNumber || 0;

      this.log(`Verifying subaccount ${address}/${subaccount}...`, LogSeverity.Debug);
      let hasSubaccount = false;
      try {
        const subaccountData = await this.sdkClient.getSubaccount(address, subaccount);
        hasSubaccount = true;
        this.log(`Subaccount found with equity: ${subaccountData?.subaccount?.equity || 0}`, LogSeverity.Info);
      } catch (subaccountErr: any) {
        // 404 is expected for new/empty accounts - not a validation error
        if (subaccountErr?.response?.status === 404 || subaccountErr?.message?.includes('404')) {
          this.log(
            `No subaccount found for ${address}/${subaccount}. Account is valid but has never deposited or traded on dYdX.`,
            LogSeverity.Info,
          );
          this.log('To use this account: deposit USDC via https://trade.dydx.exchange/', LogSeverity.Info);
        } else {
          // Other errors should be thrown
          throw subaccountErr;
        }
      }

      this.log(
        `dYdX account validated successfully${!hasSubaccount ? ' (empty account - no deposits yet)' : ''}`,
        LogSeverity.Info,
      );
      return '';
    } catch (err: any) {
      this.emitAndLogError(err?.message || 'Failed to validate dYdX account');
      return err?.message || 'Invalid dYdX credentials';
    }
  };

  start = async () => {
    try {
      this.log('Starting dYdX exchange...', LogSeverity.Info);

      const useExternalPublicData = await this.initExternalPublicData();

      // Ensure SDK is initialized
      if (!this.sdkClient) {
        await this.validateAccount();
      }

      if (!useExternalPublicData) {
        // Fetch markets and tickers
        this.log('Fetching markets...', LogSeverity.Debug);
        const markets = await this.fetchMarkets();
        if (this.isDisposed) return;
        this.log(`Fetched ${markets.length} markets`, LogSeverity.Debug);

        this.store.update({
          markets,
          loaded: { ...this.store.loaded, markets: true },
        });

        this.log('Fetching tickers...', LogSeverity.Debug);
        const tickers = await this.fetchTickers();
        if (this.isDisposed) return;
        this.log(`Fetched ${tickers.length} tickers`, LogSeverity.Debug);

        this.store.update({
          tickers,
          loaded: { ...this.store.loaded, tickers: true },
        });

        // Connect public WebSocket
        this.log('Connecting to public WebSocket...', LogSeverity.Debug);
        this.publicWebsocket.connectAndSubscribe();
      }

      // Connect private WebSocket and subscribe to subaccount
      if (!this.isPublicDataOnly && this.options.key) {
        this.log('Connecting to private WebSocket...', LogSeverity.Debug);
        const address = this.getTradingAddress();
        const subaccount = this.options.dydxSubaccountNumber || 0;
        this.privateWebsocket.setCredentials(address, subaccount);
        this.privateWebsocket.connectAndSubscribe();
      }

      // Fetch initial private data
      this.log('Fetching initial positions and balance...', LogSeverity.Debug);
      await this.tick();
      if (this.isDisposed) return;

      this.log(`Ready to trade on dYdX`, LogSeverity.Info);
    } catch (error) {
      this.log(`Error starting dYdX exchange: ${error}`, LogSeverity.Error);
      throw error;
    }
  };

  tick = async (count = 0) => {
    if (!this.isDisposed) {
      try {
        this.log(`Tick ${count}: Fetching positions...`, LogSeverity.Debug);
        const positions = await this.fetchPositions();
        if (this.isDisposed) return;
        this.log(`Tick ${count}: Fetched ${positions.length} positions`, LogSeverity.Debug);
        this.store.update({ positions, loaded: { ...this.store.loaded, positions: true } });

        this.log(`Tick ${count}: Fetching balance...`, LogSeverity.Debug);
        const balance = await this.fetchBalance();
        if (this.isDisposed) return;
        this.log(`Tick ${count}: Balance fetched - total: ${balance.total}, free: ${balance.free}`, LogSeverity.Debug);

        const upnl = positions.length > 0 ? sumBy(positions, 'unrealizedPnl') : 0;
        (balance as Writable<Balance>).upnl = upnl;

        this.store.update({
          balance,
          loaded: { ...this.store.loaded, balance: true },
        });

        if (count % 4 === 0) {
          this.log(`Tick ${count}: Fetching orders (periodic refresh)...`, LogSeverity.Debug);
          const orders = await this.fetchOpenOrders();
          if (this.isDisposed) return;
          this.log(`Tick ${count}: Fetched ${orders.length} orders`, LogSeverity.Debug);
          this.store.update({ orders, loaded: { ...this.store.loaded, orders: true } });
        }
      } catch (err: any) {
        this.log(`Error in tick ${count}: ${err?.message}`, LogSeverity.Error);
      }

      if (!this.isDisposed) {
        setTimeout(() => this.tick(count + 1), 10_000);
      }
    }
  };

  dispose = () => {
    this.isDisposed = true;
    this.publicWebsocket.dispose();
    this.privateWebsocket.dispose();
  };

  async getPositionMode(): Promise<PositionMode> {
    // dYdX uses one-way position mode only
    return PositionMode.OneWay;
  }

  fetchMarkets = async (): Promise<Market[]> => {
    try {
      this.log('Fetching markets from API...', LogSeverity.Debug);
      const { data } = await this.xhr.get<DydxMarketsResponse>(ENDPOINTS.PERPETUAL_MARKETS);

      if (!data || !data.markets) {
        this.log('Markets API returned invalid data', LogSeverity.Error);
        return [];
      }

      const markets: Market[] = [];

      for (const [ticker, sourceMarket] of Object.entries(data.markets)) {
        // Cache market data
        this.marketsCache.set(ticker, sourceMarket);

        // Calculate precision from tickSize and stepSize
        const tickSize = parseFloat(sourceMarket.tickSize);
        const stepSize = parseFloat(sourceMarket.stepSize);

        const pricePrecision = tickSize;
        const amountPrecision = stepSize;

        markets.push({
          id: ticker,
          symbol: ticker,
          base: sourceMarket.baseAsset,
          quote: sourceMarket.quoteAsset,
          active: sourceMarket.status === 'ACTIVE',
          precision: {
            amount: amountPrecision,
            price: pricePrecision,
          },
          limits: {
            amount: {
              min: amountPrecision,
              max: Number.MAX_SAFE_INTEGER,
            },
            leverage: { min: 1, max: 20 }, // dYdX default max leverage
          },
          contractSize: 1,
          takerFee: 0.0005, // 0.05% taker fee
          makerFee: 0.00002, // 0.002% maker fee
          info: sourceMarket,
        });
      }

      this.log(`Fetched ${markets.length} dYdX markets`, LogSeverity.Info);
      return markets;
    } catch (err) {
      this.log(`Error fetching markets: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  fetchTickers = async (): Promise<Ticker[]> => {
    try {
      this.log('Fetching tickers from API...', LogSeverity.Debug);
      const { data } = await this.xhr.get<DydxMarketsResponse>(ENDPOINTS.PERPETUAL_MARKETS);

      if (!data || !data.markets) {
        this.log('Tickers API returned invalid data', LogSeverity.Error);
        return [];
      }

      const tickers: Ticker[] = [];

      for (const [ticker, sourceMarket] of Object.entries(data.markets)) {
        // Calculate 24h price change percentage
        const priceChange24H = parseFloat(sourceMarket.priceChange24H) || 0;

        tickers.push({
          id: ticker,
          symbol: ticker,
          bid: 0, // Not provided in markets endpoint
          ask: 0, // Not provided in markets endpoint
          last: 0, // Would need to fetch from trades
          mark: 0, // Would need to fetch from market data
          index: 0, // Oracle price
          percentage: priceChange24H,
          fundingRate: parseFloat(sourceMarket.nextFundingRate) || 0,
          fundingTime: 0,
          volume: parseFloat(sourceMarket.volume24H) || 0,
          quoteVolume: parseFloat(sourceMarket.volume24H) || 0,
          openInterest: parseFloat(sourceMarket.openInterest) || 0,
          info: sourceMarket,
        });
      }

      this.log(`Fetched ${tickers.length} dYdX tickers`, LogSeverity.Info);
      return tickers;
    } catch (err) {
      this.log(`Error fetching tickers: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  fetchBalance = async (): Promise<Balance> => {
    try {
      if (!this.sdkClient) {
        throw new Error('SDK client not initialized');
      }

      const address = this.getTradingAddress();
      const subaccount = this.options.dydxSubaccountNumber || 0;

      // Find USDC asset position
      let total = 0;
      let used = 0;
      let unrealizedPnl = 0;

      // Fetch asset positions (handle 404 for empty accounts)
      try {
        this.log(`Fetching asset positions for ${address} subaccount ${subaccount}...`, LogSeverity.Debug);
        const assetPositions = await this.sdkClient.getAssetPositions(address, subaccount);

        if (assetPositions && assetPositions.positions) {
          for (const position of assetPositions.positions) {
            // Check for USDC asset
            if (position.assetId === ASSET_TYPES.USDC) {
              total = parseFloat(position.size || '0');
              this.log(`Found USDC balance: ${total}`, LogSeverity.Info);
              break;
            }
          }
        }

        if (total === 0) {
          this.log('No USDC balance found in asset positions', LogSeverity.Info);
        }
      } catch (assetErr: any) {
        // 404 means account has no subaccounts yet (never deposited/traded)
        if (assetErr?.response?.status === 404 || assetErr?.message?.includes('404')) {
          this.log(
            `No subaccount found for ${address}/${subaccount}. This account has never deposited USDC or traded on dYdX. Balance is zero.`,
            LogSeverity.Info,
          );
        } else {
          throw assetErr;
        }
      }

      // Calculate used margin from perpetual positions (handle 404 for empty accounts)
      try {
        const perpPositions = await this.sdkClient.getPerpetualPositions(address, subaccount);

        if (perpPositions && perpPositions.positions) {
          for (const position of perpPositions.positions) {
            unrealizedPnl += parseFloat(position.unrealizedPnl || '0');
            // Estimate used margin (simplified)
            const positionValue = Math.abs(parseFloat(position.size) * parseFloat(position.entryPrice));
            used += positionValue / 10; // Assume ~10x leverage average
          }
        }
      } catch (perpErr: any) {
        // 404 means account has no perpetual positions (empty account)
        if (perpErr?.response?.status === 404 || perpErr?.message?.includes('404')) {
          this.log('No perpetual positions found (empty account)', LogSeverity.Debug);
        } else {
          throw perpErr;
        }
      }

      const free = total - used;

      this.log(`Balance: total=${total}, free=${free}, used=${used}, upnl=${unrealizedPnl}`, LogSeverity.Debug);

      return {
        total,
        free: Math.max(free, 0),
        used: Math.max(used, 0),
        upnl: unrealizedPnl,
      };
    } catch (err) {
      this.log(`Error fetching balance: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  fetchPositions = async (symbols?: string[]): Promise<Position[]> => {
    try {
      if (!this.sdkClient) {
        throw new Error('SDK client not initialized');
      }

      const address = this.getTradingAddress();
      const subaccount = this.options.dydxSubaccountNumber || 0;

      this.log('Fetching perpetual positions...', LogSeverity.Debug);

      let response;
      try {
        response = await this.sdkClient.getPerpetualPositions(address, subaccount);
      } catch (err: any) {
        // 404 means account has no positions (empty account)
        if (err?.response?.status === 404 || err?.message?.includes('404')) {
          this.log('No positions found (empty account)', LogSeverity.Debug);
          return [];
        }
        throw err;
      }

      if (!response || !response.positions) {
        this.log('No positions found', LogSeverity.Debug);
        return [];
      }

      const positions: Position[] = [];

      for (const dydxPos of response.positions) {
        // Filter by symbols if provided
        if (symbols && !symbols.includes(dydxPos.market)) {
          continue;
        }

        // Determine position side
        const side =
          dydxPos.side === DydxPositionSide.LONG
            ? PositionSide.Long
            : dydxPos.side === DydxPositionSide.SHORT
              ? PositionSide.Short
              : PositionSide.Long; // Default to Long for NONE

        const contracts = Math.abs(parseFloat(dydxPos.size));
        const entryPrice = parseFloat(dydxPos.entryPrice);
        const unrealizedPnl = parseFloat(dydxPos.unrealizedPnl || '0');

        // Skip positions with 0 contracts
        if (contracts === 0) {
          continue;
        }

        positions.push({
          id: createPositionId(dydxPos.market, side, PositionMode.OneWay),
          symbol: dydxPos.market,
          side,
          contracts,
          entryPrice,
          notional: contracts * entryPrice,
          leverage: 10, // Default leverage (dYdX doesn't provide this directly)
          unrealizedPnl,
          realizedPnl: parseFloat(dydxPos.realizedPnl || '0'),
          liquidationPrice: 0, // Not directly provided
          marginType: MarginType.Cross, // dYdX uses cross margin
          positionMode: PositionMode.OneWay,
          lastUpdate: new Date(dydxPos.createdAt).getTime(),
          info: dydxPos,
        });
      }

      this.log(`Fetched ${positions.length} dYdX positions`, LogSeverity.Debug);
      return positions;
    } catch (err) {
      this.log(`Error fetching positions: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  fetchOpenOrders = async (market?: string): Promise<Order[]> => {
    try {
      if (!this.sdkClient) {
        throw new Error('SDK client not initialized');
      }

      const address = this.getTradingAddress();
      const subaccount = this.options.dydxSubaccountNumber || 0;

      this.log(`Fetching orders${market ? ` for ${market}` : ''}...`, LogSeverity.Debug);

      let response;
      try {
        response = await this.sdkClient.getOrders(address, subaccount, market);
      } catch (err: any) {
        // 404 means account has no orders (empty account)
        if (err?.response?.status === 404 || err?.message?.includes('404')) {
          this.log('No orders found (empty account)', LogSeverity.Debug);
          return [];
        }
        throw err;
      }

      if (!response || !Array.isArray(response)) {
        this.log('No orders found', LogSeverity.Debug);
        return [];
      }

      const orders: Order[] = [];

      for (const dydxOrder of response) {
        // Only include open orders
        if (
          ![DydxOrderStatus.OPEN, DydxOrderStatus.PARTIALLY_FILLED, DydxOrderStatus.UNTRIGGERED].includes(
            dydxOrder.status as DydxOrderStatus,
          )
        ) {
          continue;
        }

        const side = this.dydxOrderSideToSafeCex(dydxOrder.side);
        const type = this.dydxOrderTypeToSafeCex(dydxOrder.type);
        const status = this.dydxOrderStatusToSafeCex(dydxOrder.status as DydxOrderStatus);
        const filled = parseFloat(dydxOrder.totalFilled || '0');
        const amount = parseFloat(dydxOrder.size);

        orders.push({
          id: dydxOrder.id,
          symbol: dydxOrder.clobPairId, // Map to market symbol
          side,
          type,
          status,
          price: parseFloat(dydxOrder.price),
          amount,
          filled,
          remaining: amount - filled,
          reduceOnly: dydxOrder.reduceOnly,
          timeInForce: undefined, // Not directly mapped
          lastUpdate: new Date(dydxOrder.createdAtHeight).getTime(),
          info: { ...dydxOrder, clientId: dydxOrder.clientId }, // Store clientId in info
        });
      }

      this.log(`Fetched ${orders.length} open dYdX orders`, LogSeverity.Debug);
      return orders;
    } catch (err) {
      this.log(`Error fetching orders: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  fetchOrders = async (symbols?: string[]): Promise<Order[]> => {
    if (symbols && symbols.length === 1) {
      return this.fetchOpenOrders(symbols[0]);
    }
    return this.fetchOpenOrders();
  };

  fetchOHLCV = async (opts: OHLCVOptions): Promise<Candle[]> => {
    try {
      const resolution = this.convertInterval(opts.interval);
      const market = this.store.marketsBySymbol[opts.symbol];

      if (!market) {
        this.log(`Market ${opts.symbol} not found`, LogSeverity.Warning);
        return [];
      }

      const params: any = {
        resolution,
      };

      if (opts.from) {
        params.fromISO = new Date(opts.from).toISOString();
      }
      if (opts.to) {
        params.toISO = new Date(opts.to).toISOString();
      }
      if (opts.limit) {
        params.limit = opts.limit;
      }

      const endpoint = `${ENDPOINTS.CANDLES}/${market.id}`;
      this.log(`Fetching OHLCV from ${endpoint}`, LogSeverity.Debug);

      const { data } = await this.xhr.get<{ candles: DydxCandle[] }>(endpoint, { params });

      if (!data || !data.candles) {
        this.log('OHLCV API returned invalid data', LogSeverity.Warning);
        return [];
      }

      const candles: Candle[] = data.candles.map((c) => ({
        timestamp: new Date(c.startedAt).getTime(),
        open: parseFloat(c.open),
        high: parseFloat(c.high),
        low: parseFloat(c.low),
        close: parseFloat(c.close),
        volume: parseFloat(c.baseTokenVolume),
      }));

      this.log(`Fetched ${candles.length} candles for ${opts.symbol}`, LogSeverity.Info);
      return candles;
    } catch (err) {
      this.log(`Error fetching OHLCV: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    return this.publicWebsocket.listenOHLCV(opts, callback);
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    return this.publicWebsocket.listenOrderBook(symbol, callback);
  };

  placeOrder = async (opts: PlaceOrderOpts): Promise<string[]> => {
    try {
      if (!this.sdkClient) {
        throw new Error('SDK client not initialized');
      }

      const market = this.store.marketsBySymbol[opts.symbol];
      if (!market) {
        throw new Error(`Market ${opts.symbol} not found`);
      }

      // Generate unique client ID
      const clientId = this.generateClientId();

      // Convert order parameters
      const side = this.safeCexOrderSideToDydx(opts.side);
      const type = this.safeCexOrderTypeToDydx(opts.type);
      const timeInForce = this.safeCexTimeInForceToDydx(opts.timeInForce);
      const postOnly = opts.timeInForce === OrderTimeInForce.PostOnly;

      // For short-term orders, calculate goodTilBlock
      let goodTilBlock: number | undefined;
      if (type === DydxOrderType.LIMIT || type === DydxOrderType.MARKET) {
        const currentHeight = await this.getCurrentBlockHeight();
        goodTilBlock = currentHeight + 20; // Expire in 20 blocks (~20 seconds)
      }

      this.log(
        `Placing ${type} order for ${opts.symbol}: ${opts.side} ${opts.amount} @ ${opts.price || 'market'}`,
        LogSeverity.Info,
      );

      // Use appropriate order placement method based on type
      let tx;
      if (type === DydxOrderType.LIMIT && goodTilBlock) {
        // Short-term limit order - use SDK method directly
        tx = await this.sdkClient.placeShortTermOrder({
          market: opts.symbol,
          side: side as any, // SDK expects its own OrderSide type
          price: opts.price || 0,
          size: opts.amount,
          clientId,
          goodTilBlock,
          timeInForce: timeInForce as any, // SDK expects its own TimeInForce type
          reduceOnly: opts.reduceOnly || false,
        });
      } else {
        // Standard order - use SDK method with type conversion
        tx = await this.sdkClient.placeOrder({
          market: opts.symbol,
          type: type as any, // SDK expects its own OrderType
          side: side as any, // SDK expects its own OrderSide type
          price: opts.price || 0,
          size: opts.amount,
          clientId,
          timeInForce: timeInForce as any, // SDK expects its own TimeInForce type
          postOnly,
          reduceOnly: opts.reduceOnly || false,
          triggerPrice: opts.triggerPrice,
          goodTilBlock,
        });
      }

      this.log(`Order placed successfully: clientId=${clientId}, tx=${JSON.stringify(tx)}`, LogSeverity.Debug);

      // Return client ID as order ID
      return [clientId.toString()];
    } catch (err: any) {
      this.log(`Error placing order: ${err.message}`, LogSeverity.Error);
      this.emitAndLogError(err.message || 'Failed to place order');
      throw err;
    }
  };

  updateOrder = async (opts: UpdateOrderOpts): Promise<string[]> => {
    // dYdX doesn't support order modification
    // Cancel and replace with new order
    const { order, update } = opts;

    this.log(`Updating order ${order.id} via cancel-and-replace`, LogSeverity.Debug);

    // Build new order from existing order
    const newOrder: Writable<PlaceOrderOpts> = {
      symbol: order.symbol,
      type: order.type,
      side: order.side,
      price: order.price,
      amount: order.amount,
      reduceOnly: order.reduceOnly || false,
    };

    // Apply updates
    if ('price' in update) {
      newOrder.price = update.price;
    }
    if ('amount' in update) {
      newOrder.amount = update.amount;
    }

    try {
      // Cancel old order
      await this.cancelOrder(order);
      this.log(`Cancelled old order ${order.id}`, LogSeverity.Debug);

      // Place new order
      const orderIds = await this.placeOrder(newOrder);
      this.log(`Placed new order ${orderIds[0]} to replace ${order.id}`, LogSeverity.Info);

      return orderIds;
    } catch (err) {
      this.log(`Error in cancel-and-replace: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  cancelOrder = async (order: Order): Promise<void> => {
    try {
      if (!this.sdkClient) {
        throw new Error('SDK client not initialized');
      }

      // Extract order information
      const clientId = parseInt((order.info as any)?.clientId || order.id);
      const orderFlags = DydxOrderFlags.SHORT_TERM; // Default to short-term
      const market = this.store.marketsBySymbol[order.symbol];

      if (!market) {
        throw new Error(`Market ${order.symbol} not found`);
      }

      const marketInfo = this.marketsCache.get(order.symbol);
      const clobPairId = marketInfo?.clobPairId || order.symbol;

      this.log(`Cancelling order ${order.id} (clientId: ${clientId})`, LogSeverity.Debug);

      const currentHeight = await this.getCurrentBlockHeight();

      await this.sdkClient.cancelOrder({
        clientId,
        orderFlags,
        clobPairId,
        goodTilBlock: currentHeight + 10,
      });

      this.log(`Order ${order.id} cancelled successfully`, LogSeverity.Info);
    } catch (err) {
      this.log(`Error cancelling order ${order.id}: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  cancelOrders = async (orders: Order[]): Promise<void> => {
    await forEachSeries(orders, async (order) => {
      try {
        await this.cancelOrder(order);
      } catch (err) {
        this.log(`Error cancelling order ${order.id}: ${err}`, LogSeverity.Warning);
      }
    });
  };

  cancelAllOrders = async (): Promise<void> => {
    const orders = await this.fetchOpenOrders();
    await this.cancelOrders(orders);
  };

  cancelSymbolOrders = async (symbol: string): Promise<void> => {
    const orders = await this.fetchOpenOrders(symbol);
    await this.cancelOrders(orders);
  };

  setLeverage = async (symbol: string, _leverage: number, _side?: PositionSide): Promise<void> => {
    // dYdX doesn't support per-market leverage adjustment via API
    // Leverage is managed through position sizing and margin
    this.log(
      `dYdX does not support setting leverage via API for ${symbol}. Leverage is managed through position sizing.`,
      LogSeverity.Warning,
    );
  };

  placeOrders = async (orders: PlaceOrderOpts[]): Promise<string[]> => {
    const results: string[] = [];
    await forEachSeries(orders, async (order) => {
      try {
        const orderIds = await this.placeOrder(order);
        results.push(...orderIds);
      } catch (err) {
        this.log(`Error placing order: ${err}`, LogSeverity.Error);
      }
    });
    return results;
  };
}
