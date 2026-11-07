import type { Axios } from 'axios';
import type { Store } from '../../../store/store.interface';

import rateLimit from 'axios-rate-limit';
import sumBy from 'lodash/sumBy';
import uniqBy from 'lodash/uniqBy';
import { forEachSeries } from 'p-iteration';

import { getExchangeMetadata } from '../../../exchangeMetadata';
import {
  Balance,
  Candle,
  ExchangeName,
  ExchangeOptions,
  Execution,
  ExecutionsOptions,
  LogSeverity,
  MarginType,
  Market,
  OHLCVOptions,
  Order,
  OrderBook,
  OrderSide,
  OrderStatus,
  OrderStopTriggerType,
  OrderTimeInForce,
  OrderType,
  PlaceOrderOpts,
  Position,
  PositionMode,
  PositionSide,
  Ticker,
  UpdateOrderOpts,
  Writable,
} from '../../../types';
import { createPositionId } from '../../../utils/createPositionId';
import { loop } from '../../../utils/loop';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseExchange } from '../../../exchanges/base';
import type { ClientOrderIdRule } from '../../../exchanges/clientOrderId';
import { createAPI } from './backpack.api';
import {
  BackpackAccount,
  BackpackCollateral,
  BackpackFill,
  BackpackKline,
  BackpackMarket,
  BackpackMarkPrice,
  BackpackOrder,
  BackpackOrderRequest,
  BackpackPosition,
  BackpackTicker,
  ENDPOINTS,
  GRAFANA_ENDPOINTS_EVERY,
  GRAFANA_ENDPOINTS_PERIODIC,
} from './backpack.types';
import { BackpackPrivateWebSocket } from './backpack.ws-private';
import { BackpackPublicWebSocket } from './backpack.ws-public';

export class BackpackExchange extends BaseExchange {
  // Backpack's clientId is a uint32; only the canonical decimal form round-trips as the same string.
  protected readonly clientOrderIdRule: ClientOrderIdRule = {
    label: 'Backpack',
    requirement: 'be a canonical decimal uint32 between 1 and 4294967295',
    isValid: (clientOrderId) => /^[1-9]\d{0,9}$/.test(clientOrderId) && Number(clientOrderId) <= 0xffff_ffff,
  };

  xhr: Axios;

  publicWebsocket: BackpackPublicWebSocket;
  privateWebsocket: BackpackPrivateWebSocket;

  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;
  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;

  // Account-level leverage limit (fetched from account info)
  private accountLeverageLimit: number = 1;

  private nextBatchClientId = (): number => {
    if (globalThis.crypto?.getRandomValues) {
      return globalThis.crypto.getRandomValues(new Uint32Array(1))[0];
    }

    return Math.floor(Math.random() * 0x1_0000_0000);
  };

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    // Rate limit to a conservative 10 RPS
    this.xhr = this.createXhrWithSecondaryKey(opts, createAPI, { rateLimit: (xhr) => rateLimit(xhr, { maxRPS: 10 }) });

    this.publicWebsocket = new BackpackPublicWebSocket(this);
    this.privateWebsocket = new BackpackPrivateWebSocket(this);
  }

  dispose = () => {
    this.isDisposed = true;
    this.publicWebsocket.dispose();
    this.privateWebsocket.dispose();
  };

  async getPositionMode(): Promise<PositionMode> {
    // Backpack uses hedge mode (can have both long and short positions)
    return PositionMode.Hedged;
  }

  getAccount = async () => {
    try {
      const { data } = await this.xhr.get(ENDPOINTS.ACCOUNT);
      return {
        userId: data.accountId || data.userId,
        affiliateId: null,
      };
    } catch (err) {
      this.log(`Error fetching account: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  validateAccount = async () => {
    try {
      await this.xhr.get(ENDPOINTS.CAPITAL);
      return { error: '', balance: await this.fetchBalance().catch(() => undefined) };
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return { error: err?.response?.data?.msg || err?.message || 'Invalid API key or secret' };
    }
  };

  start = async () => {
    const markets = await this.fetchMarkets();
    if (this.isDisposed) return;

    this.store.update({
      markets,
      loaded: { ...this.store.loaded, markets: true },
    });

    const tickers = await this.fetchTickers();
    if (this.isDisposed) return;

    this.log(`Loaded ${Math.min(tickers.length, markets.length)} Backpack markets`);

    this.store.update({
      tickers,
      loaded: { ...this.store.loaded, tickers: true },
    });

    // Connect public WebSocket for real-time market data
    this.publicWebsocket.connectAndSubscribe();
    this.subscribeToAllMarkets(markets);

    if (this.isPublicDataOnly) {
      this.store.update({
        loaded: { ...this.store.loaded, balance: true, positions: true, orders: true },
      });
      return;
    }

    // Fetch initial data via REST API first (positions, balance, orders)
    await this.tick();
    if (this.isDisposed) return;

    const orders = await this.fetchOrders();
    if (this.isDisposed) return;

    this.log(`Loaded Backpack orders`);

    this.store.update({
      orders,
      loaded: { ...this.store.loaded, orders: true },
    });

    this.log(`Ready to trade on Backpack`);

    // Connect private WebSocket for order/position updates
    this.privateWebsocket.connectAndSubscribe();
  };

  private subscribeToAllMarkets = (markets: Market[]) => {
    // Delay subscriptions to allow WebSocket to connect
    setTimeout(() => {
      if (this.isDisposed) return;

      for (const market of markets) {
        try {
          this.publicWebsocket.subscribeToTicker(market.id);
          this.publicWebsocket.subscribeToTrades(market.id);
          this.publicWebsocket.subscribeToBookTicker(market.id);
        } catch (err) {
          // Log but don't fail - WebSocket subscriptions are for real-time updates
          // REST API polling in tick() will still work
          this.log(`Failed to subscribe to ${market.id}: ${err}`, LogSeverity.Warning);
        }
      }
    }, 2000);
  };

  tick = async () => {
    if (!this.isDisposed) {
      if (this.isPublicDataOnly) return;

      try {
        // Fetch account info first to get leverage limit
        await this.fetchAccountInfo();
        if (this.isDisposed) return;

        const positions = await this.fetchPositions();
        if (this.isDisposed) return;
        this.store.update({ positions, loaded: { ...this.store.loaded, positions: true } });

        const balance = await this.fetchBalance();
        if (this.isDisposed) return;

        const upnl = positions.length > 0 ? sumBy(positions, 'unrealizedPnl') : 0;
        (balance as Writable<Balance>).upnl = upnl;

        this.store.update({
          balance,
          loaded: { ...this.store.loaded, balance: true },
        });

        const orders = await this.fetchOrders();
        if (this.isDisposed) return;
        this.store.update({ orders, loaded: { ...this.store.loaded, orders: true } });
      } catch (err: any) {
        this.log(`Error in tick: ${err?.message}`, LogSeverity.Error);
      }

      if (!this.isDisposed) {
        loop(() => this.tick(), 10_000);
      }
    }
  };

  /**
   * Fetch account info to get the leverage limit
   */
  fetchAccountInfo = async (): Promise<void> => {
    try {
      const { data } = await this.xhr.get<BackpackAccount>(ENDPOINTS.ACCOUNT);
      const leverageLimit = parseFloat(data.leverageLimit) || 1;
      this.accountLeverageLimit = leverageLimit;
    } catch (err) {
      // Don't fail on account fetch error - just log and use default
      this.log(`Error fetching account info: ${err}`, LogSeverity.Warning);
    }
  };

  fetchMarkets = async (): Promise<Market[]> => {
    try {
      const { data } = await this.xhr.get<BackpackMarket[]>(ENDPOINTS.MARKETS);

      return data
        .filter((m) => m.symbol.includes('_') && m.visible !== false) // Filter out invalid and non-visible symbols
        .map((m) => {
          const [base, quote] = m.symbol.split('_');
          const isPerpetual = m.marketType === 'PERP';

          // Calculate max leverage from IMF (Initial Margin Fraction)
          // leverage = 1 / imf, e.g., imf = 0.05 means max leverage = 20x
          const imfBase = m.imfFunction?.base ? parseFloat(m.imfFunction.base) : 1;
          const maxLeverage = imfBase > 0 ? Math.round(1 / imfBase) : 1;

          return {
            id: m.symbol,
            symbol: m.symbol,
            base,
            quote,
            active: true,
            type: isPerpetual ? 'swap' : 'spot',
            spot: !isPerpetual,
            swap: isPerpetual,
            future: false,
            contract: isPerpetual,
            linear: isPerpetual,
            inverse: false,
            contractSize: 1,
            isSpot: !isPerpetual,
            precision: {
              amount: parseFloat(m.quantityIncrement || m.filters?.quantity?.stepSize || '0.001'),
              price: parseFloat(m.priceIncrement || m.filters?.price?.tickSize || '0.01'),
              base: 8,
              quote: 8,
            },
            limits: {
              amount: {
                min: parseFloat(m.minQuantity || m.filters?.quantity?.minQuantity || '0'),
                max: parseFloat(m.maxQuantity || m.filters?.quantity?.maxQuantity || '0'),
              },
              price: {
                min: parseFloat(m.filters?.price?.minPrice || '0'),
                max: parseFloat(m.filters?.price?.maxPrice || '0'),
              },
              cost: {
                min: parseFloat(m.minNotional || '0'),
                max: parseFloat(m.maxNotional || '0'),
              },
              leverage: {
                min: 1,
                max: isPerpetual ? maxLeverage : 1,
              },
            },
            info: m,
          };
        });
    } catch (err) {
      this.log(`Error fetching markets: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  fetchTickers = async (): Promise<Ticker[]> => {
    try {
      // Fetch tickers and mark prices in parallel
      const [tickersResponse, markPricesResponse] = await Promise.all([
        this.xhr.get<BackpackTicker[]>(ENDPOINTS.TICKERS),
        this.xhr.get<BackpackMarkPrice[]>(ENDPOINTS.MARK_PRICES).catch(() => ({ data: [] })),
      ]);

      const tickers = tickersResponse.data;
      const markPrices = markPricesResponse.data;

      // Create a map of mark prices by symbol for quick lookup (only PERP markets have mark prices)
      const markPriceMap = new Map<string, BackpackMarkPrice>();
      for (const mp of markPrices) {
        markPriceMap.set(mp.symbol, mp);
      }

      // Include ALL markets. Perps carry mark/index/funding from the mark-price
      // endpoint; spot markets have no mark price, so they fall back to last
      // price for mark/index and carry no funding. Previously this filtered to
      // `markPriceMap.has(t.symbol)`, which dropped every spot ticker — so spot
      // markets never appeared in the screener / market dropdown (both iterate
      // store.tickers, and the WS ticker handler bails when the ticker is
      // missing from the store).
      return tickers.map((t) => {
        const markPrice = markPriceMap.get(t.symbol);
        const last = parseFloat(t.lastPrice);

        return {
          id: `${t.symbol}-${virtualClock.getCurrentTimeMs()}`,
          symbol: t.symbol,
          bid: 0, // Will be updated from WebSocket
          ask: 0, // Will be updated from WebSocket
          last,
          mark: markPrice ? parseFloat(markPrice.markPrice) : last,
          index: markPrice ? parseFloat(markPrice.indexPrice) : last,
          percentage: parseFloat(t.priceChangePercent) * 100,
          fundingRate: markPrice ? parseFloat(markPrice.fundingRate) : 0,
          fundingTime: markPrice ? markPrice.nextFundingTimestamp : 0,
          volume: parseFloat(t.volume),
          quoteVolume: parseFloat(t.quoteVolume),
          openInterest: 0,
          info: markPrice ? { ...t, ...markPrice } : { ...t },
        };
      });
    } catch (err) {
      this.log(`Error fetching tickers: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  fetchOHLCV = async (opts: OHLCVOptions): Promise<Candle[]> => {
    try {
      const params: any = {
        symbol: opts.symbol,
        interval: opts.interval,
      };

      // Backpack API expects timestamps in seconds, not milliseconds
      if (opts.from) params.startTime = Math.floor(opts.from / 1000);
      if (opts.to) params.endTime = Math.floor(opts.to / 1000);

      const { data } = await this.xhr.get<BackpackKline[]>(ENDPOINTS.KLINES, { params });

      // Defensive check - API may return non-array on error or empty result
      if (!Array.isArray(data)) {
        this.log(`OHLCV response is not an array: ${JSON.stringify(data)}`, LogSeverity.Warning);
        return [];
      }

      return data.map((k) => ({
        // Parse ISO date string to timestamp (e.g., "2025-12-15 13:28:00" -> ms)
        timestamp: new Date(k.start + 'Z').getTime(),
        open: parseFloat(k.open),
        high: parseFloat(k.high),
        low: parseFloat(k.low),
        close: parseFloat(k.close),
        volume: parseFloat(k.volume),
      }));
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

  fetchOrderBook = async (symbol: string): Promise<OrderBook> => {
    try {
      const { data } = await this.xhr.get(ENDPOINTS.DEPTH, {
        params: { symbol },
      });

      return {
        bids: data.bids.map(([price, amount]: [string, string]) => ({
          id: price,
          price: parseFloat(price),
          amount: parseFloat(amount),
          total: 0,
        })),
        asks: data.asks.map(([price, amount]: [string, string]) => ({
          id: price,
          price: parseFloat(price),
          amount: parseFloat(amount),
          total: 0,
        })),
      };
    } catch (err) {
      this.log(`Error fetching order book: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  fetchHistoricalFills = async (opts: ExecutionsOptions): Promise<Execution[]> => {
    if (this.isPublicDataOnly || !opts.symbol) {
      return [];
    }

    const market = this.store.marketsBySymbol[opts.symbol];
    if (!market) {
      return [];
    }

    const toMilliseconds = (value?: number) => {
      if (value === undefined || !Number.isFinite(value)) return undefined;
      return value >= 1_000_000_000_000 ? Math.floor(value) : Math.floor(value * 1000);
    };
    const toSeconds = (value: string) => {
      const parsedDate = Date.parse(value);
      if (Number.isFinite(parsedDate)) return Math.floor(parsedDate / 1000);

      const numeric = Number(value);
      if (!Number.isFinite(numeric) || numeric <= 0) return undefined;
      return numeric >= 1_000_000_000_000 ? Math.floor(numeric / 1000) : Math.floor(numeric);
    };

    const { data } = await this.xhr.get<BackpackFill[]>(ENDPOINTS.FILLS_HISTORY, {
      params: {
        symbol: market.id,
        orderId: opts.orderId,
        from: toMilliseconds(opts.from),
        to: toMilliseconds(opts.to),
        limit: Math.min(Math.max(opts.limit ?? 100, 1), 1000),
        sortDirection: 'Desc',
      },
    });

    if (!Array.isArray(data)) {
      throw new Error('Backpack fill history response is not an array');
    }

    return data.reduce<Execution[]>((executions, fill) => {
      if (fill.symbol !== market.id) return executions;
      if (opts.orderId && fill.orderId !== opts.orderId) return executions;

      const timestamp = toSeconds(fill.timestamp);
      const price = parseFloat(fill.price);
      const qty = parseFloat(fill.quantity);
      const side = fill.side === 'Bid' ? OrderSide.Buy : fill.side === 'Ask' ? OrderSide.Sell : undefined;
      if (!timestamp || !Number.isFinite(price) || price <= 0 || !Number.isFinite(qty) || qty <= 0 || !side) {
        return executions;
      }

      const commission = parseFloat(fill.fee);
      executions.push({
        id: String(fill.tradeId),
        timestamp,
        symbol: market.symbol,
        orderId: fill.orderId,
        price,
        qty,
        side,
        commission: Number.isFinite(commission) ? Math.abs(commission) : undefined,
        commissionAsset: fill.feeSymbol || undefined,
      });
      return executions;
    }, []);
  };

  fetchOrders = async (symbols?: string[]): Promise<Order[]> => {
    try {
      // Try to fetch all orders at once (orderQueryAll)
      // According to API docs, GET /api/v1/orders should return all open orders
      const { data } = await this.xhr.get<BackpackOrder[]>(ENDPOINTS.ORDERS);

      // Filter by symbols if specified
      const orders = symbols ? data.filter((o) => symbols.includes(o.symbol)) : data;

      // Fix any trigger orders that are missing the triggerBy parameter
      // This ensures orders trigger correctly when price crosses the trigger level
      await this.fixTriggerOrdersWithoutTriggerBy(orders);

      return orders.map((o) => this.mapOrder(o));
    } catch (err) {
      this.log(`Error fetching orders: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  /** Detects trigger orders with missing triggerBy and recreates them with the advertised default reference. */
  private fixTriggerOrdersWithoutTriggerBy = async (orders: BackpackOrder[]): Promise<void> => {
    // Find trigger orders with null triggerBy
    const ordersToFix = orders.filter((o) => {
      const isTriggerOrder = o.triggerPrice !== undefined && o.triggerPrice !== null;
      const hasMissingTriggerBy = o.triggerBy === null || o.triggerBy === undefined;
      return isTriggerOrder && hasMissingTriggerBy;
    });

    if (ordersToFix.length === 0) {
      return;
    }

    this.log(`Found ${ordersToFix.length} trigger order(s) with missing triggerBy - fixing them`, LogSeverity.Info);

    for (const order of ordersToFix) {
      try {
        const side = order.side === 'Bid' ? OrderSide.Buy : OrderSide.Sell;
        const triggerPrice = parseFloat(order.triggerPrice!);

        // Determine if this is a TP or SL order
        const orderType = this.determineTriggerOrderType(order.symbol, side, triggerPrice);

        const triggerBy = this.getTriggerBy(orderType === OrderType.TakeProfit ? 'tp' : 'sl');

        this.log(
          `Fixing order ${order.id}: ${order.symbol} ${side} @ ${order.triggerPrice} (${orderType === OrderType.TakeProfit ? 'TP' : 'SL'}) -> triggerBy=${triggerBy}`,
          LogSeverity.Info,
        );

        // Cancel the old order
        await this.xhr.delete(ENDPOINTS.ORDER, {
          data: {
            symbol: order.symbol,
            orderId: order.id,
          },
        });

        // Get the amount from triggerQuantity (for trigger orders) or quantity
        const amount = order.triggerQuantity || order.quantity;

        // Recreate with correct triggerBy
        const newOrderRequest: BackpackOrderRequest = {
          symbol: order.symbol,
          side: order.side,
          orderType: order.orderType,
          triggerPrice: order.triggerPrice,
          triggerQuantity: amount,
          triggerBy,
          reduceOnly: true,
        };

        // If it was a limit order, include the price
        if (order.orderType === 'Limit' && order.price) {
          newOrderRequest.price = order.price;
        }

        const { data: newOrder } = await this.xhr.post<BackpackOrder>(ENDPOINTS.ORDER, newOrderRequest);

        this.log(`Fixed order ${order.id} -> new order ${newOrder.id} with triggerBy=${triggerBy}`, LogSeverity.Info);

        // Update the order in the array so the caller gets the updated data
        const idx = orders.findIndex((o) => o.id === order.id);
        if (idx >= 0) {
          orders[idx] = newOrder;
        }
      } catch (err) {
        this.log(`Error fixing trigger order ${order.id}: ${err}`, LogSeverity.Warning);
        // Continue with other orders even if one fails
      }
    }
  };

  fetchPositions = async (): Promise<Position[]> => {
    try {
      let data: BackpackPosition[] = [];

      try {
        const response = await this.xhr.get<BackpackPosition[]>(ENDPOINTS.POSITIONS);
        data = response.data;
      } catch (err: any) {
        // Positions endpoint might not be available for all accounts
        // Return empty array with fake positions only
        if (err.message?.includes('not found')) {
          this.log('Positions endpoint not available, using fake positions only', LogSeverity.Info);
        } else {
          throw err;
        }
      }

      const positionMode = await this.getPositionMode();
      // Use account-level leverage limit for all positions
      const leverage = this.accountLeverageLimit;

      // Map actual positions (with non-zero size)
      // netQuantity: positive = Long, negative = Short
      const actualPositions: Position[] = data
        .filter((p) => parseFloat(p.netQuantity) !== 0)
        .map((p) => {
          const netQuantity = parseFloat(p.netQuantity);
          const entryPrice = parseFloat(p.entryPrice);
          const side = netQuantity > 0 ? PositionSide.Long : PositionSide.Short;

          return {
            id: createPositionId(p.symbol, side, positionMode),
            symbol: p.symbol,
            side,
            entryPrice,
            notional: parseFloat(p.netExposureNotional) || Math.abs(netQuantity * entryPrice),
            leverage, // Use account-level leverage
            unrealizedPnl: parseFloat(p.pnlUnrealized),
            contracts: Math.abs(netQuantity),
            liquidationPrice: parseFloat(p.estLiquidationPrice) || 0,
            lastUpdate: virtualClock.getCurrentTimeMs(),
            // Backpack is always cross-margin hedged mode
            positionMode,
            marginType: MarginType.Cross,
          };
        });

      // Create fake positions for all perpetual markets (hedged mode: both long and short)
      const existingSymbols = new Set(actualPositions.map((p) => p.symbol));
      const fakePositions: Position[] = [];

      for (const market of this.store.markets) {
        // Only create fake positions for active non-spot markets
        if (market.active && !market.isSpot && !existingSymbols.has(market.symbol)) {
          // Create both long and short positions (hedged mode)
          fakePositions.push(
            {
              id: createPositionId(market.symbol, PositionSide.Long, positionMode),
              symbol: market.symbol,
              side: PositionSide.Long,
              entryPrice: 0,
              notional: 0,
              leverage, // Use account-level leverage
              unrealizedPnl: 0,
              contracts: 0,
              liquidationPrice: 0,
              lastUpdate: virtualClock.getCurrentTimeMs(),
              positionMode,
              marginType: MarginType.Cross,
            },
            {
              id: createPositionId(market.symbol, PositionSide.Short, positionMode),
              symbol: market.symbol,
              side: PositionSide.Short,
              entryPrice: 0,
              notional: 0,
              leverage, // Use account-level leverage
              unrealizedPnl: 0,
              contracts: 0,
              liquidationPrice: 0,
              lastUpdate: virtualClock.getCurrentTimeMs(),
              positionMode,
              marginType: MarginType.Cross,
            },
          );
        }
      }

      // Combine and deduplicate by ID (actual positions override fakes)
      return uniqBy([...actualPositions, ...fakePositions], 'id');
    } catch (err) {
      this.log(`Error fetching positions: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  private getTriggerBy = (
    kind: 'tp' | 'sl',
    explicitTriggerType?: OrderStopTriggerType,
  ): NonNullable<BackpackOrderRequest['triggerBy']> => {
    const metadata = getExchangeMetadata('backpack');
    const triggerType =
      explicitTriggerType ?? (kind === 'tp' ? metadata.defaultTPTriggerType : metadata.defaultSLTriggerType);

    switch (triggerType) {
      case OrderStopTriggerType.LastPrice:
        return 'LastPrice';
      case OrderStopTriggerType.IndexPrice:
        return 'IndexPrice';
      case OrderStopTriggerType.MarkPrice:
      default:
        return 'MarkPrice';
    }
  };

  private isOrdinaryLimitOrder = (opts: PlaceOrderOpts): boolean =>
    opts.type === OrderType.Limit &&
    opts.triggerPrice === undefined &&
    opts.takeProfit === undefined &&
    opts.stopLoss === undefined &&
    opts.tpTriggerType === undefined &&
    opts.slTriggerType === undefined &&
    opts.close !== true;

  private canUseNativeBatch = (opts: PlaceOrderOpts): boolean => {
    // Batch pairs mint their own correlation clientIds; a caller ID goes through placeOrder.
    if (!this.isOrdinaryLimitOrder(opts) || opts.clientOrderId !== undefined) return false;

    try {
      this.buildBatchLimitOrderRequest(opts, 1);
      return true;
    } catch {
      return false;
    }
  };

  private buildBatchLimitOrderRequest = (opts: PlaceOrderOpts, clientId: number): BackpackOrderRequest => {
    if (!this.isOrdinaryLimitOrder(opts)) {
      throw new Error('Backpack native batches only support ordinary LIMIT orders');
    }
    if (!Number.isInteger(clientId) || clientId <= 0 || clientId > 0xffff_ffff) {
      throw new Error(`Invalid Backpack batch clientId: ${clientId}`);
    }

    const market = this.store.marketsBySymbol[opts.symbol];
    if (!market) {
      throw new Error(`Market not found: ${opts.symbol}`);
    }
    if (!market.active || market.isReadOnly) {
      throw new Error(`Market is not independently placeable: ${opts.symbol}`);
    }
    if (!Number.isFinite(opts.amount) || opts.amount <= 0) {
      throw new Error(`Invalid order amount: ${opts.amount}`);
    }
    if (opts.amount < market.limits.amount.min || opts.amount > market.limits.amount.max) {
      throw new Error(
        `Order amount ${opts.amount} is outside ${opts.symbol} bounds ${market.limits.amount.min}-${market.limits.amount.max}`,
      );
    }
    if (!Number.isFinite(opts.price) || (opts.price ?? 0) <= 0) {
      throw new Error(`Invalid limit order price: ${opts.price}`);
    }

    const formatValue = (value: number, precision: number, field: string): string => {
      if (!Number.isFinite(precision) || precision <= 0) {
        throw new Error(`Invalid ${field} precision for ${opts.symbol}: ${precision}`);
      }

      const factor = 1 / precision;
      const rounded = Math.round(value * factor) / factor;
      const precisionString = precision.toString();
      const decimalIndex = precisionString.indexOf('.');
      const decimals = precision >= 1 || decimalIndex === -1 ? 0 : precisionString.length - decimalIndex - 1;
      const formatted = decimals > 0 ? rounded.toFixed(decimals) : Math.round(rounded).toString();

      if (!Number.isFinite(Number(formatted)) || Number(formatted) <= 0) {
        throw new Error(`Invalid formatted ${field} for ${opts.symbol}: ${formatted}`);
      }
      return formatted;
    };

    const isPostOnly = opts.timeInForce === OrderTimeInForce.PostOnly;
    const request: BackpackOrderRequest = {
      symbol: market.id,
      side: opts.side === OrderSide.Buy ? 'Bid' : 'Ask',
      orderType: 'Limit',
      price: formatValue(opts.price!, market.precision.price, 'price'),
      quantity: formatValue(opts.amount, market.precision.amount, 'amount'),
      timeInForce: isPostOnly ? 'GTC' : this.mapTimeInForce(opts.timeInForce),
      postOnly: isPostOnly,
      clientId,
    };

    if (opts.reduceOnly) {
      request.reduceOnly = true;
    }

    return request;
  };

  private createBatchClientIds = (count: number): number[] => {
    const ids = new Set<number>();

    for (let attempts = 0; ids.size < count && attempts < 64; attempts += 1) {
      const clientId = this.nextBatchClientId();
      if (Number.isInteger(clientId) && clientId > 0 && clientId <= 0xffff_ffff) {
        ids.add(clientId);
      }
    }

    if (ids.size !== count) {
      throw new Error(`Unable to generate ${count} unique Backpack uint32 clientIds`);
    }

    return [...ids];
  };

  private decodeBatchOrderResponse = (data: unknown, requests: readonly BackpackOrderRequest[]): string[] => {
    if (!Array.isArray(data) || data.length !== requests.length) {
      throw new Error(`Backpack batch response count mismatch: expected ${requests.length}`);
    }

    const expectedByClientId = new Map<number, BackpackOrderRequest>();
    for (const request of requests) {
      if (!Number.isInteger(request.clientId) || request.clientId! <= 0 || request.clientId! > 0xffff_ffff) {
        throw new Error('Backpack batch request is missing a valid uint32 clientId');
      }
      if (expectedByClientId.has(request.clientId!)) {
        throw new Error(`Duplicate Backpack batch request clientId: ${request.clientId}`);
      }
      expectedByClientId.set(request.clientId!, request);
    }

    const orderIdByClientId = new Map<number, string>();
    const exchangeIds = new Set<string>();

    for (const row of data) {
      if (!row || typeof row !== 'object' || Array.isArray(row)) {
        throw new Error('Malformed Backpack batch response row');
      }

      const result = row as Record<string, unknown>;
      if (result.operation !== 'Ok') {
        throw new Error(`Backpack batch row failed with operation ${String(result.operation)}`);
      }

      const clientId = result.clientId;
      if (!Number.isInteger(clientId) || (clientId as number) <= 0 || (clientId as number) > 0xffff_ffff) {
        throw new Error('Backpack batch response row is missing a valid uint32 clientId');
      }

      const expected = expectedByClientId.get(clientId as number);
      if (!expected) {
        throw new Error(`Unknown Backpack batch response clientId: ${clientId}`);
      }
      if (orderIdByClientId.has(clientId as number)) {
        throw new Error(`Duplicate Backpack batch response clientId: ${clientId}`);
      }
      if (
        result.symbol !== expected.symbol ||
        result.side !== expected.side ||
        result.orderType !== expected.orderType
      ) {
        throw new Error(`Backpack batch response identity mismatch for clientId ${clientId}`);
      }

      const exchangeId = typeof result.id === 'string' ? result.id.trim() : '';
      if (!exchangeId) {
        throw new Error(`Backpack batch response clientId ${clientId} is missing an exchange order id`);
      }
      if (exchangeIds.has(exchangeId)) {
        throw new Error(`Duplicate Backpack batch exchange order id: ${exchangeId}`);
      }

      exchangeIds.add(exchangeId);
      orderIdByClientId.set(clientId as number, exchangeId);
    }

    return requests.map((request) => orderIdByClientId.get(request.clientId!)!);
  };

  private placeBatchPair = async (orders: readonly [PlaceOrderOpts, PlaceOrderOpts]): Promise<string[]> => {
    const clientIds = this.createBatchClientIds(orders.length);
    // Build both rows before making the only write. A failure in either row
    // therefore cannot leave a partially preflighted pair on the exchange.
    const requests = orders.map((order, index) => this.buildBatchLimitOrderRequest(order, clientIds[index]));
    const { data } = await this.xhr.post<unknown>(ENDPOINTS.ORDERS, requests);
    return this.decodeBatchOrderResponse(data, requests);
  };

  placeOrder = async (opts: PlaceOrderOpts): Promise<string[]> => {
    const callerClientOrderId = this.resolveCallerClientOrderId(opts);
    try {
      const market = this.store.marketsBySymbol[opts.symbol];
      if (!market) {
        throw new Error(`Market not found: ${opts.symbol}`);
      }

      const isPostOnly = opts.timeInForce === OrderTimeInForce.PostOnly;
      const isStopOrTP = opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit;
      // A trigger order is either a stop/TP order or has an explicit triggerPrice
      const isTriggerOrder = isStopOrTP || opts.triggerPrice !== undefined;
      const isLimitOrder = opts.type === OrderType.Limit;
      const hasTPSL = opts.takeProfit !== undefined || opts.stopLoss !== undefined;

      // Helper to count decimal places from precision (e.g., 0.0001 -> 4)
      const getDecimalPlaces = (precision: number): number => {
        if (precision >= 1) return 0;
        const str = precision.toString();
        const decimalIndex = str.indexOf('.');
        if (decimalIndex === -1) return 0;
        return str.length - decimalIndex - 1;
      };

      // Helper to format price to market precision
      const formatPrice = (price: number): string => {
        const precision = market.precision.price;
        const factor = 1 / precision;
        const rounded = Math.round(price * factor) / factor;
        const decimals = getDecimalPlaces(precision);
        // Use toFixed to ensure proper decimal formatting (avoids floating point representation issues)
        return decimals > 0 ? rounded.toFixed(decimals) : Math.round(rounded).toString();
      };

      // Helper to format amount to market precision
      const formatAmount = (amount: number): string => {
        const precision = market.precision.amount;
        const factor = 1 / precision;
        const rounded = Math.round(amount * factor) / factor;
        const decimals = getDecimalPlaces(precision);
        // Use toFixed to ensure proper decimal formatting (avoids floating point representation issues)
        return decimals > 0 ? rounded.toFixed(decimals) : Math.round(rounded).toString();
      };

      // Determine the order type for the API
      // For Stop/TP orders, use Market type so they execute immediately when triggered
      let apiOrderType: 'Limit' | 'Market';
      if (isStopOrTP) {
        apiOrderType = 'Market';
      } else if (opts.type === OrderType.Market) {
        apiOrderType = 'Market';
      } else {
        apiOrderType = 'Limit';
      }

      const orderRequest: BackpackOrderRequest = {
        symbol: market.id,
        side: opts.side === OrderSide.Buy ? 'Bid' : 'Ask',
        orderType: apiOrderType,
        ...(callerClientOrderId !== undefined && { clientId: Number(callerClientOrderId) }),
      };

      // PostOnly and timeInForce only apply to Limit orders, not Market orders
      if (apiOrderType === 'Limit') {
        orderRequest.timeInForce = isPostOnly ? 'GTC' : this.mapTimeInForce(opts.timeInForce);
        orderRequest.postOnly = isPostOnly;
      }

      // For trigger orders (including Stop/TP), use triggerQuantity instead of quantity
      if (isTriggerOrder) {
        // For Stop/TP, use price as trigger price if triggerPrice not explicitly set
        const triggerPrice = opts.triggerPrice ?? opts.price;
        if (triggerPrice) {
          orderRequest.triggerPrice = formatPrice(triggerPrice);
        }
        orderRequest.triggerQuantity = formatAmount(opts.amount);
        orderRequest.triggerBy = this.getTriggerBy(
          opts.type === OrderType.TakeProfit ? 'tp' : 'sl',
          opts.type === OrderType.TakeProfit ? opts.tpTriggerType : opts.slTriggerType,
        );
        // Stop/TP orders should always be reduce-only to close positions
        if (isStopOrTP) {
          orderRequest.reduceOnly = true;
        }
      } else {
        orderRequest.quantity = formatAmount(opts.amount);
      }

      // Set price for limit orders (not Stop/TP which use Market)
      if (opts.type === OrderType.Limit && opts.price) {
        orderRequest.price = formatPrice(opts.price);
      }

      // Set reduceOnly if specified (and not already set for Stop/TP)
      if (opts.reduceOnly && !isStopOrTP) {
        orderRequest.reduceOnly = true;
      }

      // For limit orders with TP/SL, place separate orders instead of attached params
      // (attached TP/SL only works for orders that fill immediately like market orders)
      if (isLimitOrder && hasTPSL) {
        // Place the main limit order first (without TP/SL params)
        const { data } = await this.xhr.post<BackpackOrder>(ENDPOINTS.ORDER, orderRequest);
        const orderIds: string[] = [data.id];

        // Opposite side for TP/SL orders (entry buy -> exit sell, entry sell -> exit buy)
        const exitSide = opts.side === OrderSide.Buy ? 'Ask' : 'Bid';

        // Place separate TP order if specified
        if (opts.takeProfit !== undefined) {
          try {
            const tpRequest: BackpackOrderRequest = {
              symbol: market.id,
              side: exitSide,
              orderType: 'Market', // Trigger orders use Market type
              triggerPrice: formatPrice(opts.takeProfit),
              triggerQuantity: formatAmount(opts.amount),
              triggerBy: this.getTriggerBy('tp', opts.tpTriggerType),
              reduceOnly: true,
            };
            const { data: tpData } = await this.xhr.post<BackpackOrder>(ENDPOINTS.ORDER, tpRequest);
            orderIds.push(tpData.id);
          } catch (err) {
            this.log(`Error placing TP order: ${err}`, LogSeverity.Warning);
          }
        }

        // Place separate SL order if specified
        if (opts.stopLoss !== undefined) {
          try {
            const slRequest: BackpackOrderRequest = {
              symbol: market.id,
              side: exitSide,
              orderType: 'Market', // Trigger orders use Market type
              triggerPrice: formatPrice(opts.stopLoss),
              triggerQuantity: formatAmount(opts.amount),
              triggerBy: this.getTriggerBy('sl', opts.slTriggerType),
              reduceOnly: true,
            };
            const { data: slData } = await this.xhr.post<BackpackOrder>(ENDPOINTS.ORDER, slRequest);
            orderIds.push(slData.id);
          } catch (err) {
            this.log(`Error placing SL order: ${err}`, LogSeverity.Warning);
          }
        }

        return orderIds;
      }

      // For market orders with TP/SL, use attached params (they fill immediately)
      if (opts.takeProfit !== undefined) {
        orderRequest.takeProfitTriggerPrice = formatPrice(opts.takeProfit);
        orderRequest.takeProfitLimitPrice = formatPrice(opts.takeProfit);
        orderRequest.takeProfitTriggerBy = this.getTriggerBy('tp', opts.tpTriggerType);
      }

      if (opts.stopLoss !== undefined) {
        orderRequest.stopLossTriggerPrice = formatPrice(opts.stopLoss);
        orderRequest.stopLossLimitPrice = formatPrice(opts.stopLoss);
        orderRequest.stopLossTriggerBy = this.getTriggerBy('sl', opts.slTriggerType);
      }

      const { data } = await this.xhr.post<BackpackOrder>(ENDPOINTS.ORDER, orderRequest);

      return [data.id];
    } catch (err: any) {
      this.log(`Error placing order: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  cancelOrder = async (order: Order): Promise<void> => {
    try {
      const market = this.store.marketsBySymbol[order.symbol];
      if (!market) {
        throw new Error(`Market not found: ${order.symbol}`);
      }

      await this.xhr.delete(ENDPOINTS.ORDER, {
        data: {
          symbol: market.id,
          orderId: order.id,
        },
      });
    } catch (err) {
      this.log(`Error canceling order: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  cancelOrders = async (orders: Order[]): Promise<void> => {
    await forEachSeries(orders, async (order) => {
      try {
        await this.cancelOrder(order);
      } catch (_err) {
        // Continue canceling other orders
      }
    });
  };

  cancelSymbolOrders = async (symbol: string): Promise<void> => {
    try {
      const market = this.store.marketsBySymbol[symbol];
      if (!market) return;

      await this.xhr.delete(ENDPOINTS.ORDERS, {
        data: { symbol: market.id },
      });
    } catch (err) {
      this.log(`Error canceling symbol orders: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  updateOrder = async (opts: UpdateOrderOpts): Promise<string[]> => {
    // Backpack doesn't support order modification
    // Cancel and replace instead
    const existingOrder = this.store.orders.find((o) => o.id === opts.order.id);
    if (!existingOrder) {
      throw new Error(`Order not found: ${opts.order.id}`);
    }

    const isStopOrTP = existingOrder.type === OrderType.StopLoss || existingOrder.type === OrderType.TakeProfit;

    // Build new order from existing order + updates
    const newAmount = 'amount' in opts.update ? opts.update.amount : existingOrder.amount;
    const newPrice = 'price' in opts.update ? opts.update.price : existingOrder.price;

    // Validate amount before cancelling (prevent placing order with invalid amount)
    if (!newAmount || newAmount <= 0) {
      throw new Error(`Invalid order amount: ${newAmount}. Original amount: ${existingOrder.amount}`);
    }

    this.log(
      `Updating order ${existingOrder.id}: type=${existingOrder.type}, amount=${existingOrder.amount}->${newAmount}, price=${existingOrder.price}->${newPrice}`,
      LogSeverity.Info,
    );

    // Cancel the existing order
    await this.cancelOrder(existingOrder);

    // Build new order options
    const newOrderOpts: PlaceOrderOpts = {
      symbol: existingOrder.symbol,
      type: existingOrder.type,
      side: existingOrder.side,
      amount: newAmount,
      price: newPrice,
      timeInForce: existingOrder.timeInForce,
      reduceOnly: isStopOrTP || existingOrder.reduceOnly,
    };

    try {
      const result = await this.placeOrder(newOrderOpts);
      this.log(`Order updated successfully: ${existingOrder.id} -> ${result.join(', ')}`, LogSeverity.Info);
      return result;
    } catch (err) {
      // Log the failure - order has been cancelled but new order failed
      this.log(`Failed to place replacement order after cancelling ${existingOrder.id}: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  setLeverage = async (_symbol: string, leverage: number, _side?: PositionSide): Promise<void> => {
    // Backpack sets leverage at the account level via leverageLimit
    // This is an account-wide maximum leverage setting, not per-symbol
    try {
      await this.xhr.patch(ENDPOINTS.ACCOUNT, {
        leverageLimit: leverage.toString(),
      });

      // Update stored leverage limit
      this.accountLeverageLimit = leverage;

      this.log(`Account leverage limit set to ${leverage}x`, LogSeverity.Info);

      // Refetch positions to update all positions with the new leverage
      const positions = await this.fetchPositions();
      this.store.update({ positions });
    } catch (err) {
      const errorMessage = `Failed to set leverage: ${err}`;
      this.emitter.emit('error', new Error(errorMessage));
      this.log(errorMessage, LogSeverity.Error);
      throw err;
    }
  };

  cancelAllOrders = async (): Promise<void> => {
    try {
      // Cancel orders for all markets
      await forEachSeries(this.store.markets, async (market) => {
        try {
          await this.xhr.delete(ENDPOINTS.ORDERS, {
            data: { symbol: market.id },
          });
        } catch (_err) {
          // Continue with other markets
        }
      });
    } catch (err) {
      this.log(`Error canceling all orders: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  changePositionMode = async (_hedged: boolean): Promise<void> => {
    // Backpack only supports hedged mode (both long and short positions allowed)
    // There is no API endpoint to change this setting
    const msg = 'Backpack only supports Hedged position mode - this cannot be changed';
    this.emitter.emit('error', new Error(msg));
    this.log(msg, LogSeverity.Warning);
  };

  changeMarginType = async (_isolated: boolean, _symbol?: string): Promise<void> => {
    // Backpack only supports cross-margin mode
    // There is no API endpoint to change this setting
    const msg = 'Backpack only supports Cross margin mode - this cannot be changed';
    this.emitter.emit('error', new Error(msg));
    this.log(msg, LogSeverity.Warning);
  };

  placeOrders = async (orders: PlaceOrderOpts[]): Promise<string[]> => {
    this.assertCallerClientOrderIds(orders);
    const orderIds: string[] = [];

    for (let index = 0; index < orders.length;) {
      const first = orders[index];
      const second = orders[index + 1];

      if (second && this.canUseNativeBatch(first) && this.canUseNativeBatch(second)) {
        try {
          const ids = await this.placeBatchPair([first, second]);
          orderIds.push(...ids);
        } catch (err) {
          this.emitAndLogError(err as Error, 'Failed to place Backpack native order pair');
        }
        index += 2;
        continue;
      }

      try {
        const ids = await this.placeOrder(first);
        orderIds.push(...ids);
      } catch (err) {
        this.log(`Error placing order in batch: ${err}`, LogSeverity.Error);
        // Continue with other orders
      }
      index += 1;
    }

    return orderIds;
  };

  fetchBalance = async (): Promise<Balance> => {
    try {
      // Use collateral endpoint for perps margin balance
      const { data } = await this.xhr.get<BackpackCollateral>(ENDPOINTS.CAPITAL_COLLATERAL);

      // Use netEquity values for the main balance
      // Backpack is cross-margin, so we use top-level values directly
      // (skip currencies to let getBalanceSlice fall back to top-level balance)
      const total = parseFloat(data.netEquity ?? '0');
      const free = parseFloat(data.netEquityAvailable ?? '0');
      const used = parseFloat(data.netEquityLocked ?? '0');
      const upnl = parseFloat(data.pnlUnrealized ?? '0');

      return {
        total,
        free,
        used,
        upnl,
      };
    } catch (err) {
      this.log(`Error fetching balance: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  private mapOrder = (o: BackpackOrder): Order => {
    const side = o.side === 'Bid' ? OrderSide.Buy : OrderSide.Sell;
    const triggerPrice = o.triggerPrice ? parseFloat(o.triggerPrice) : undefined;
    const isTriggerOrder = triggerPrice !== undefined;

    // Determine order type - detect trigger orders (Market + triggerPrice)
    let orderType: OrderType;
    if (o.orderType === 'Market' && isTriggerOrder) {
      // This is a trigger order - determine if it's TP or SL based on side and price
      orderType = this.determineTriggerOrderType(o.symbol, side, triggerPrice);
    } else if (o.orderType === 'Market') {
      orderType = OrderType.Market;
    } else {
      orderType = OrderType.Limit;
    }

    // For trigger orders, use triggerPrice as the display price
    const displayPrice = triggerPrice ?? parseFloat(o.price);

    // For trigger orders, use triggerQuantity if available, otherwise fall back to quantity
    // The API may return the amount in either field depending on order type
    const rawAmount = isTriggerOrder && o.triggerQuantity ? o.triggerQuantity : o.quantity;
    const amount = parseFloat(rawAmount) || 0;

    // Log warning if trigger order has zero amount (helps debug issues)
    if (isTriggerOrder && amount === 0) {
      this.log(
        `Warning: Trigger order ${o.id} has zero amount. quantity=${o.quantity}, triggerQuantity=${o.triggerQuantity}`,
        LogSeverity.Warning,
      );
    }

    return {
      id: o.id,
      symbol: o.symbol,
      type: orderType,
      side,
      price: displayPrice,
      amount,
      filled: parseFloat(o.executedQuantity || '0'),
      remaining: amount - parseFloat(o.executedQuantity || '0'),
      status: this.mapOrderStatus(o.status),
      lastUpdate: o.createdAt,
      reduceOnly: o.reduceOnly ?? isTriggerOrder,
      timeInForce: this.mapBackpackTimeInForce(o.timeInForce),
    };
  };

  /**
   * Determines if a trigger order is a StopLoss or TakeProfit based on the side
   * and trigger price relative to current market price.
   *
   * StopLoss: Triggers when price moves against you
   *   - Long position (sell): trigger below current price
   *   - Short position (buy): trigger above current price
   *
   * TakeProfit: Triggers when price moves in your favor
   *   - Long position (sell): trigger above current price
   *   - Short position (buy): trigger below current price
   */
  private determineTriggerOrderType = (symbol: string, side: OrderSide, triggerPrice: number): OrderType => {
    // Get current market price from ticker
    const ticker = this.store.tickers.find((t) => t.symbol === symbol);
    const currentPrice = ticker?.last ?? ticker?.mark;

    if (!currentPrice) {
      // If we don't have a current price, default to StopLoss
      return OrderType.StopLoss;
    }

    if (side === OrderSide.Sell) {
      // Selling: below current = StopLoss (protect long), above = TakeProfit
      return triggerPrice < currentPrice ? OrderType.StopLoss : OrderType.TakeProfit;
    } else {
      // Buying: above current = StopLoss (protect short), below = TakeProfit
      return triggerPrice > currentPrice ? OrderType.StopLoss : OrderType.TakeProfit;
    }
  };

  private mapOrderStatus = (status: string): OrderStatus => {
    switch (status) {
      case 'New':
      case 'PartiallyFilled':
        return OrderStatus.Open;
      case 'Filled':
        return OrderStatus.Closed;
      case 'Cancelled':
      case 'Expired':
        return OrderStatus.Canceled;
      default:
        return OrderStatus.Open;
    }
  };

  private mapTimeInForce = (tif?: OrderTimeInForce): 'GTC' | 'IOC' | 'FOK' => {
    switch (tif) {
      case OrderTimeInForce.ImmediateOrCancel:
        return 'IOC';
      case OrderTimeInForce.FillOrKill:
        return 'FOK';
      // PostOnly is handled via separate postOnly field, not timeInForce
      default:
        return 'GTC';
    }
  };

  private mapBackpackTimeInForce = (tif: string): OrderTimeInForce => {
    switch (tif) {
      case 'IOC':
        return OrderTimeInForce.ImmediateOrCancel;
      case 'FOK':
        return OrderTimeInForce.FillOrKill;
      default:
        return OrderTimeInForce.GoodTillCancel;
    }
  };
}
