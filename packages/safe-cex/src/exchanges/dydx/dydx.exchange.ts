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
  CandleResolution,
  DydxCandle,
  DydxFill,
  DydxMarketsResponse,
  DydxOrder,
  DydxOrderFlags,
  DydxOrderSide,
  DydxOrderStatus,
  DydxOrderType,
  DydxPerpetualPosition,
  DydxPositionSide,
  DydxSubaccount,
  DydxTimeInForce,
  DydxWsSubaccountUpdate,
  DYDX_SHORT_TERM_CANCEL_BLOCKS,
  DYDX_SHORT_TERM_ORDER_BLOCKS,
  DYDX_STATEFUL_ORDER_DURATION_SECONDS,
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
  private marketSymbolsByClobPairId: Map<string, string> = new Map();
  private lastClientId: number = 0;
  private makerFee = 0.00002;
  private takerFee = 0.0005;

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

  private dydxTimeInForceToSafeCex(timeInForce: DydxTimeInForce, postOnly: boolean): OrderTimeInForce {
    if (postOnly) return OrderTimeInForce.PostOnly;

    const mapping: Record<DydxTimeInForce, OrderTimeInForce> = {
      [DydxTimeInForce.GTT]: OrderTimeInForce.GoodTillCancel,
      [DydxTimeInForce.IOC]: OrderTimeInForce.ImmediateOrCancel,
      [DydxTimeInForce.FOK]: OrderTimeInForce.FillOrKill,
    };

    return mapping[timeInForce] || OrderTimeInForce.GoodTillCancel;
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

  private mapOrder = (dydxOrder: DydxOrder): Order | undefined => {
    const symbol = this.marketSymbolsByClobPairId.get(String(dydxOrder.clobPairId));
    if (!symbol) {
      this.log(`Skipping order ${dydxOrder.id}: unknown CLOB pair ${dydxOrder.clobPairId}`, LogSeverity.Warning);
      return undefined;
    }

    const filled = Number(dydxOrder.totalFilled || '0');
    const amount = Number(dydxOrder.size);
    const lastUpdate = Date.parse(dydxOrder.updatedAt || dydxOrder.createdAt || '');

    return {
      id: dydxOrder.id,
      symbol,
      side: this.dydxOrderSideToSafeCex(dydxOrder.side),
      type: this.dydxOrderTypeToSafeCex(dydxOrder.type),
      status: this.dydxOrderStatusToSafeCex(dydxOrder.status as DydxOrderStatus),
      price: Number(dydxOrder.price),
      amount,
      filled,
      remaining: amount - filled,
      reduceOnly: dydxOrder.reduceOnly,
      timeInForce: this.dydxTimeInForceToSafeCex(dydxOrder.timeInForce, dydxOrder.postOnly),
      lastUpdate: Number.isFinite(lastUpdate) ? lastUpdate : 0,
      info: {
        ...dydxOrder,
        clientId: dydxOrder.clientId,
        orderFlags: Number(dydxOrder.orderFlags),
      },
    };
  };

  private mapPosition = (dydxPos: DydxPerpetualPosition): Position => {
    const side =
      dydxPos.side === DydxPositionSide.LONG
        ? PositionSide.Long
        : dydxPos.side === DydxPositionSide.SHORT
          ? PositionSide.Short
          : PositionSide.Long;
    const contracts = Math.abs(Number(dydxPos.size));
    const entryPrice = Number(dydxPos.entryPrice);

    return {
      id: createPositionId(dydxPos.market, side, PositionMode.OneWay),
      symbol: dydxPos.market,
      side,
      contracts,
      entryPrice,
      notional: contracts * entryPrice,
      leverage: 10,
      unrealizedPnl: Number(dydxPos.unrealizedPnl || '0'),
      realizedPnl: Number(dydxPos.realizedPnl || '0'),
      liquidationPrice: 0,
      marginType: MarginType.Cross,
      positionMode: PositionMode.OneWay,
      lastUpdate: Date.parse(dydxPos.createdAt),
      info: dydxPos,
    };
  };

  private mapSubaccountBalance = (subaccount: Partial<DydxSubaccount>): Balance | undefined => {
    const total = Number(subaccount.equity);
    const free = Number(subaccount.freeCollateral);
    if (!Number.isFinite(total) || !Number.isFinite(free)) return undefined;

    return {
      total,
      free,
      used: Math.max(total - free, 0),
      upnl: sumBy(this.store.positions, 'unrealizedPnl'),
    };
  };

  handleSubaccountUpdate = (data: DydxWsSubaccountUpdate) => {
    if (Array.isArray(data.orders)) {
      const orders = data.orders.map(this.mapOrder).filter((order): order is Order => order !== undefined);
      this.store.addOrUpdateOrders(orders);
    }

    if (Array.isArray(data.positions)) {
      this.store.addOrUpdatePositions(data.positions.map(this.mapPosition));
    }

    if (data.subaccount) {
      const balance = this.mapSubaccountBalance(data.subaccount);
      if (balance) {
        this.store.update({ balance, loaded: { ...this.store.loaded, balance: true } });
      }
    }

    if (Array.isArray(data.fills)) {
      for (const fill of data.fills) this.emitPrivateFill(fill);
    }
  };

  private emitPrivateFill = (fill: DydxFill) => {
    const market = this.store.marketsById[fill.market];
    const price = Number(fill.price);
    const amount = Number(fill.size);
    if (!market || !Number.isFinite(price) || !Number.isFinite(amount)) return;

    this.emitter.emit('fill', {
      id: fill.id,
      symbol: market.symbol,
      side: this.dydxOrderSideToSafeCex(fill.side),
      price,
      amount,
    });
  };

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
      this.log(`Error getting dYdX block height: ${err}`, LogSeverity.Error);
      throw new Error('Unable to get the current dYdX block height; short-term order was not submitted', {
        cause: err,
      });
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

      try {
        const feeTier = await this.sdkClient.getUserFeeTier(address);
        if (feeTier?.tier) {
          this.makerFee = feeTier.tier.makerFeePpm / 1_000_000;
          this.takerFee = feeTier.tier.takerFeePpm / 1_000_000;
          for (const market of this.store.markets) {
            this.store.updateMarket(market, { makerFee: this.makerFee, takerFee: this.takerFee });
          }
        }
      } catch (feeErr) {
        this.log(`Could not load the account fee tier; keeping display-only defaults: ${feeErr}`, LogSeverity.Warning);
      }

      this.log(
        `dYdX account validated successfully${!hasSubaccount ? ' (empty account - no deposits yet)' : ''}`,
        LogSeverity.Info,
      );
      return { error: '' };
    } catch (err: any) {
      this.emitAndLogError(err?.message || 'Failed to validate dYdX account');
      return { error: err?.message || 'Invalid dYdX credentials' };
    }
  };

  start = async () => {
    try {
      this.log('Starting dYdX exchange...', LogSeverity.Info);

      const useExternalPublicData = await this.initExternalPublicData();

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

      if (this.isPublicDataOnly) {
        await this._startPublicDataOnly();
        return;
      }

      if (!this.sdkClient) {
        const validation = await this.validateAccount();
        if (validation.error) throw new Error(validation.error);
      }

      // Connect private WebSocket and subscribe to subaccount
      if (this.options.key) {
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

  _startPublicDataOnly = async () => {
    if (this.isDisposed) return;

    this.store.update({
      orders: [],
      positions: [],
      balance: { total: 0, free: 0, used: 0, upnl: 0 },
      loaded: { ...this.store.loaded, orders: true, positions: true, balance: true },
    });
    this.log('Ready to watch public data on dYdX', LogSeverity.Info);
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
        this.marketSymbolsByClobPairId.set(String(sourceMarket.clobPairId), ticker);

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
          // Public-only mode uses display fallbacks. Private startup replaces
          // them with the wallet's governance-controlled fee tier.
          takerFee: this.takerFee,
          makerFee: this.makerFee,
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
      try {
        const response = await this.sdkClient.getSubaccount(address, subaccount);
        const balance = this.mapSubaccountBalance(response?.subaccount || {});
        if (!balance) throw new Error('dYdX subaccount response omitted equity or freeCollateral');
        return balance;
      } catch (err: any) {
        if (err?.response?.status === 404 || err?.message?.includes('404')) {
          this.log(
            `No subaccount found for ${address}/${subaccount}. This account has never deposited USDC or traded on dYdX. Balance is zero.`,
            LogSeverity.Info,
          );
          return { total: 0, free: 0, used: 0, upnl: 0 };
        }
        throw err;
      }
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

        const position = this.mapPosition(dydxPos);

        // Skip positions with 0 contracts
        if (position.contracts === 0) {
          continue;
        }

        positions.push(position);
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

        const order = this.mapOrder(dydxOrder);
        if (order) orders.push(order);
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

      const isConditional = [
        DydxOrderType.STOP_LIMIT,
        DydxOrderType.STOP_MARKET,
        DydxOrderType.TAKE_PROFIT_LIMIT,
        DydxOrderType.TAKE_PROFIT_MARKET,
      ].includes(type);
      const isShortTerm = type === DydxOrderType.MARKET || timeInForce !== DydxTimeInForce.GTT;

      let currentHeight: number | undefined;
      let goodTilBlock: number | undefined;
      if (isShortTerm) {
        currentHeight = await this.getCurrentBlockHeight();
        goodTilBlock = currentHeight + DYDX_SHORT_TERM_ORDER_BLOCKS;
      }

      this.log(
        `Placing ${type} order for ${opts.symbol}: ${opts.side} ${opts.amount} @ ${opts.price || 'market'}`,
        LogSeverity.Info,
      );

      // Use appropriate order placement method based on type
      let tx;
      if (type === DydxOrderType.LIMIT && isShortTerm && goodTilBlock) {
        // IOC/FOK limits are short-term orders. Resting GTT limits use the
        // stateful path below so they are not silently pruned after 20 blocks.
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
          goodTilTimeInSeconds:
            isConditional || (type === DydxOrderType.LIMIT && timeInForce === DydxTimeInForce.GTT)
              ? DYDX_STATEFUL_ORDER_DURATION_SECONDS
              : undefined,
          currentHeight,
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
      const orderFlags = Number((order.info as any)?.orderFlags ?? DydxOrderFlags.SHORT_TERM);
      const market = this.store.marketsBySymbol[order.symbol];

      if (!market) {
        throw new Error(`Market ${order.symbol} not found`);
      }

      this.log(`Cancelling order ${order.id} (clientId: ${clientId})`, LogSeverity.Debug);

      const isStateful =
        orderFlags === DydxOrderFlags.LONG_TERM || orderFlags === DydxOrderFlags.CONDITIONAL;
      const goodTilBlock = isStateful
        ? 0
        : (await this.getCurrentBlockHeight()) + DYDX_SHORT_TERM_CANCEL_BLOCKS;

      await this.sdkClient.cancelOrder({
        clientId,
        orderFlags,
        marketId: market.id,
        goodTilBlock,
        goodTilTimeInSeconds: isStateful ? DYDX_STATEFUL_ORDER_DURATION_SECONDS : undefined,
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
