import type { Axios } from 'axios';
import type { Store } from '../../store/store.interface';

import rateLimit from 'axios-rate-limit';
import sumBy from 'lodash/sumBy';
import uniqBy from 'lodash/uniqBy';
import { forEachSeries } from 'p-iteration';

import {
  Balance,
  Candle,
  ExchangeName,
  ExchangeOptions,
  FetchedLeverage,
  LogSeverity,
  Market,
  OHLCVOptions,
  Order,
  OrderBook,
  OrderSide,
  OrderStatus,
  OrderStopTriggerType,
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
import { loop } from '../../utils/loop';
import { omitUndefined } from '../../utils/omitUndefined';
import { adjust } from '../../utils/safe-math';
import { virtualClock } from '../../utils/virtualClock';
import { BaseExchange } from '../base';
import { createAPI } from './arkham.api';
import {
  ACCOUNT_RESPONSE,
  BALANCE_RESPONSE,
  ENDPOINTS,
  KLINE_RESPONSE,
  LEVERAGE_RESPONSE,
  ORDER_RESPONSE,
  ORDER_SIDE,
  ORDERBOOK_RESPONSE,
  POSITION_RESPONSE,
  TRIGGER_ORDER_RESPONSE,
} from './arkham.types';
import { ArkhamPrivateWebSocket } from './arkham.ws-private';
import { ArkhamPublicWebSocket } from './arkham.ws-public';

export class ArkhamExchange extends BaseExchange {
  xhr: Axios;

  publicWebsocket: ArkhamPublicWebSocket;
  privateWebsocket: ArkhamPrivateWebSocket;

  // Store leverage per account, not per position
  private defaultLeverage = 1;

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    // Rate limit based on tier (default to lowest tier)
    const rps = 20; // Default to lowest tier
    this.xhr = rateLimit(createAPI(opts, this), { maxRPS: rps });

    this.publicWebsocket = new ArkhamPublicWebSocket(this);
    this.privateWebsocket = new ArkhamPrivateWebSocket(this);
  }

  dispose = () => {
    this.isDisposed = true;
    this.publicWebsocket.dispose();
    this.privateWebsocket.dispose();
  };

  async getPositionMode(): Promise<PositionMode> {
    // Arkham uses one-way position mode
    return PositionMode.OneWay;
  }

  getAccount = async () => {
    try {
      const { data } = await this.xhr.get<ACCOUNT_RESPONSE>(ENDPOINTS.ACCOUNT);
      return {
        userId: data.account_id,
        affiliateId: null,
      };
    } catch (err) {
      this.log(`Error fetching account: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  validateAccount = async () => {
    try {
      await this.xhr.get(ENDPOINTS.ACCOUNT);
      return '';
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.error?.message || err?.message);
      return err?.response?.data?.error?.message || err?.message || 'Invalid API key or secret';
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

    this.log(`Loaded ${Math.min(tickers.length, markets.length)} Arkham markets`);

    this.store.update({
      tickers,
      loaded: { ...this.store.loaded, tickers: true },
    });

    this.publicWebsocket.connectAndSubscribe();

    if (this.isPublicDataOnly) {
      this.store.update({
        loaded: { ...this.store.loaded, balance: true, positions: true, orders: true },
      });
      return;
    }

    this.privateWebsocket.connectAndSubscribe();

    await this.tick();
    if (this.isDisposed) return;

    this.log(`Ready to trade on Arkham`);

    const orders = await this.fetchOrders();
    if (this.isDisposed) return;

    this.log(`Loaded Arkham orders`);

    this.store.update({
      orders,
      loaded: { ...this.store.loaded, orders: true },
    });
  };

  tick = async () => {
    if (!this.isDisposed) {
      if (this.isPublicDataOnly) return;

      try {
        const positions = await this.fetchPositions();
        if (this.isDisposed) return;
        this.store.update({ positions, loaded: { ...this.store.loaded, positions: true } });

        const balance = await this.fetchBalance();
        if (this.isDisposed) return;

        const upnl = positions.length > 0 ? sumBy(positions, 'unrealizedPnl') : 0;
        (balance as Writable<Balance>).upnl = upnl;

        // Calculate used margin by summing initial margin from all positions
        const usedMargin = positions.reduce((sum, p) => {
          // Get initial margin from position info if available
          const initialMargin = p.info?.initialMargin ? parseFloat(p.info.initialMargin) : 0;
          return sum + initialMargin;
        }, 0);

        if (usedMargin > 0) {
          (balance as Writable<Balance>).used = usedMargin;
          (balance as Writable<Balance>).free = balance.total - usedMargin;
        }

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

      loop(() => this.tick(), 10_000);
    }
  };

  fetchBalance = async (): Promise<Balance> => {
    try {
      const { data } = await this.xhr.get<BALANCE_RESPONSE[]>(ENDPOINTS.BALANCES);

      // The API returns an array directly, not { balances: [] }
      const balances = Array.isArray(data) ? data : [];

      if (!Array.isArray(balances)) {
        this.log(`Unexpected balance response format: ${JSON.stringify(data)}`, LogSeverity.Error);
        return {
          total: 0,
          free: 0,
          used: 0,
          upnl: 0,
        };
      }

      // Sum up USDT/USDC balances for total
      const usdBalances = balances.filter((b) => b.symbol === 'USDT' || b.symbol === 'USDC' || b.symbol === 'USD');

      const total = usdBalances.reduce((sum, b) => sum + parseFloat(b.balance), 0);
      const free = usdBalances.reduce((sum, b) => sum + parseFloat(b.free), 0);
      const used = total - free; // Calculate locked as difference

      return {
        total,
        free,
        used,
        upnl: 0, // Will be calculated from positions
      };
    } catch (err) {
      this.log(`Error fetching balance: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  fetchMarkets = async (): Promise<Market[]> => {
    try {
      this.log(`Fetching markets from ${ENDPOINTS.MARKETS}`, LogSeverity.Info);
      const response = await this.xhr.get(ENDPOINTS.MARKETS);
      const data = response.data;

      // Log the response structure for debugging
      this.log(`Markets response structure: ${JSON.stringify(Object.keys(data || {}))}`, LogSeverity.Debug);
      this.log(`Markets response type: ${typeof data}, isArray: ${Array.isArray(data)}`, LogSeverity.Debug);

      // Handle different possible response formats
      const markets = Array.isArray(data) ? data : data?.markets || data?.data || [];

      if (!Array.isArray(markets)) {
        this.log(`Unexpected markets response format: ${JSON.stringify(data)}`, LogSeverity.Error);
        return [];
      }

      return markets
        .filter((m: any) => !m.status || m.status === 'listed')
        .map((m: any) => ({
          id: m.symbol,
          symbol: m.symbol,
          base: m.baseSymbol,
          quote: m.quoteSymbol,
          active: m.status === 'listed',
          type: m.pairType === 'perp' ? 'swap' : 'spot',
          spot: m.pairType === 'spot',
          swap: m.pairType === 'perp',
          future: false,
          option: false,
          contract: m.pairType === 'perp',
          linear: m.pairType === 'perp',
          inverse: false,
          contractSize: 1,
          precision: {
            amount: parseFloat(m.minLotSize || m.minSize),
            price: parseFloat(m.minTickPrice),
            base: 8,
            quote: 8,
          },
          limits: {
            amount: {
              min: parseFloat(m.minSize),
              max: parseFloat(m.maxSize),
            },
            price: {
              min: parseFloat(m.minPrice || 0),
              max: parseFloat(m.maxPrice || 0),
            },
            cost: {
              min: parseFloat(m.minNotional || 0),
              max: 0,
            },
            leverage: {
              min: 1,
              max: parseFloat(m.maxLeverage || 1),
            },
          },
          fees: {
            maker: 0, // Not provided per-pair, use account tier fees
            taker: 0, // Not provided per-pair, use account tier fees
          },
          info: m,
        }));
    } catch (err: any) {
      this.log(`Error fetching markets: ${err.message || err}`, LogSeverity.Error);
      if (err.response) {
        this.log(
          `Response status: ${err.response.status}, data: ${JSON.stringify(err.response.data)}`,
          LogSeverity.Error,
        );
      }
      if (err.request) {
        this.log(`Request URL: ${err.config?.url}`, LogSeverity.Error);
      }
      // Return empty array instead of throwing - allow exchange to continue
      return [];
    }
  };

  fetchTickers = async (): Promise<Ticker[]> => {
    try {
      const response = await this.xhr.get(ENDPOINTS.TICKERS);
      const data = response.data;

      // Handle different possible response formats
      const tickers = Array.isArray(data) ? data : data?.tickers || data?.data || [];

      if (!Array.isArray(tickers)) {
        this.log(`Unexpected tickers response format: ${JSON.stringify(data)}`, LogSeverity.Error);
        return [];
      }

      return tickers.map((t: any) => {
        const currentPrice = parseFloat(t.price);
        const price24hAgo = parseFloat(t.price24hAgo);
        const priceChange24h = currentPrice - price24hAgo;
        const priceChangePercent = price24hAgo > 0 ? (priceChange24h / price24hAgo) * 100 : 0;

        return {
          id: `${t.symbol}-${Date.now()}`,
          symbol: t.symbol,
          bid: currentPrice, // Will be updated from orderbook via WebSocket
          ask: currentPrice, // Will be updated from orderbook via WebSocket
          last: currentPrice,
          mark: parseFloat(t.markPrice) || currentPrice,
          index: parseFloat(t.indexPrice) || currentPrice,
          percentage: priceChangePercent,
          fundingRate: parseFloat(t.fundingRate) || 0,
          fundingTime: t.nextFundingTime || 0,
          volume: parseFloat(t.volume24h),
          quoteVolume: parseFloat(t.quoteVolume24h),
          openInterest: parseFloat(t.openInterest) || 0,
          timestamp: virtualClock.getCurrentTimeMs(),
          info: t,
        };
      });
    } catch (err: any) {
      this.log(`Error fetching tickers: ${err.message || err}`, LogSeverity.Error);
      if (err.response) {
        this.log(
          `Response status: ${err.response.status}, data: ${JSON.stringify(err.response.data)}`,
          LogSeverity.Error,
        );
      }
      if (err.request) {
        this.log(`Request URL: ${err.config?.url}`, LogSeverity.Error);
      }
      // Return empty array instead of throwing - allow exchange to continue
      return [];
    }
  };

  fetchOHLCV = async (opts: OHLCVOptions): Promise<Candle[]> => {
    try {
      // Arkham has a maximum limit of 365 candles per request
      const maxLimit = 365;
      const duration = opts.interval;

      // Parse interval to milliseconds for limit calculation
      const parseInterval = (interval: string): number => {
        const unit = interval.slice(-1);
        const value = parseInt(interval.slice(0, -1), 10);
        const multipliers: Record<string, number> = {
          m: 60 * 1000,
          h: 60 * 60 * 1000,
          d: 24 * 60 * 60 * 1000,
          w: 7 * 24 * 60 * 60 * 1000,
        };
        return value * (multipliers[unit] || 60 * 1000); // Default to minutes if unknown
      };

      const durationMs = parseInterval(duration);

      // Calculate start/end times respecting the 365 candle limit
      let start: number | undefined;
      let end: number | undefined;

      if (opts.to) {
        end = opts.to * 1000; // Convert to microseconds
        // If from is specified, respect it, otherwise calculate based on limit
        if (opts.from) {
          start = opts.from * 1000;
          // Warn if range exceeds limit
          const requestedCandles = Math.ceil((opts.to - opts.from) / (durationMs / 1000));
          if (requestedCandles > maxLimit) {
            this.log(
              `Requested ${requestedCandles} candles, but Arkham supports max ${maxLimit}. Results may be incomplete.`,
              LogSeverity.Warning,
            );
          }
        } else {
          // Calculate start time to get maxLimit candles
          start = end - maxLimit * durationMs * 1000; // Convert duration to microseconds
        }
      } else if (opts.from) {
        start = opts.from * 1000;
        // Calculate end time to get maxLimit candles
        end = start + maxLimit * durationMs * 1000;
      }
      // If neither from nor to is specified, API will return recent candles

      const params = {
        symbol: opts.symbol,
        duration, // Arkham uses 'duration' not 'interval'
        start,
        end,
      };

      const { data } = await this.xhr.get<KLINE_RESPONSE[]>(ENDPOINTS.KLINE, { params });

      // Response is array of candles, not { candles: [] }
      const candles = Array.isArray(data) ? data : [];

      return candles.map((c) => ({
        timestamp: Math.floor(c.time / 1000), // Convert from microseconds to milliseconds
        open: parseFloat(c.open),
        high: parseFloat(c.high),
        low: parseFloat(c.low),
        close: parseFloat(c.close),
        volume: parseFloat(c.volume),
      }));
    } catch (err) {
      this.log(`Error fetching OHLCV: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  fetchOrders = async (): Promise<Order[]> => {
    try {
      // Fetch both regular orders and trigger orders in parallel
      const [regularResponse, triggerResponse] = await Promise.all([
        this.xhr.get<ORDER_RESPONSE[]>(ENDPOINTS.ORDERS),
        this.xhr.get<TRIGGER_ORDER_RESPONSE[]>(ENDPOINTS.TRIGGER_ORDERS),
      ]);

      // Process regular orders
      const orders = Array.isArray(regularResponse.data) ? regularResponse.data : [];
      if (!Array.isArray(orders)) {
        this.log(`Unexpected orders response format: ${JSON.stringify(regularResponse.data)}`, LogSeverity.Error);
        return [];
      }
      const mappedOrders = orders
        .filter((o) => o.status && ['new', 'booked', 'maker', 'taker'].includes(o.status))
        .map(this.mapOrder);

      // Process trigger orders
      const triggerOrders = Array.isArray(triggerResponse.data) ? triggerResponse.data : [];
      if (!Array.isArray(triggerOrders)) {
        this.log(
          `Unexpected trigger orders response format: ${JSON.stringify(triggerResponse.data)}`,
          LogSeverity.Warning,
        );
        return mappedOrders; // Return regular orders even if trigger orders fail
      }
      const mappedTriggerOrders = triggerOrders
        .filter((o) => o.status === 'staged') // Only include staged (active) trigger orders
        .map(this.mapTriggerOrder);

      return [...mappedOrders, ...mappedTriggerOrders];
    } catch (err) {
      this.log(`Error fetching orders: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  fetchPositions = async (): Promise<Position[]> => {
    try {
      const { data } = await this.xhr.get<POSITION_RESPONSE[]>(ENDPOINTS.POSITIONS);

      // The API returns an array directly, not { positions: [] }
      const positions = Array.isArray(data) ? data : [];

      if (!Array.isArray(positions)) {
        this.log(`Unexpected positions response format: ${JSON.stringify(data)}`, LogSeverity.Error);
        return [];
      }

      const parsedPositions: Position[] = positions.map((p) => {
        // Determine side based on base position (positive = long, negative = short)
        const baseAmount = parseFloat(p.base);
        const side = baseAmount >= 0 ? PositionSide.Long : PositionSide.Short;

        // Calculate leverage from initial margin: leverage = value / initialMargin
        const value = Math.abs(parseFloat(p.value));
        const initialMargin = parseFloat(p.initialMargin);
        let leverage = initialMargin > 0 ? value / initialMargin : this.defaultLeverage;

        // If leverage is not reasonable, try to get it from fetched leverage
        if (!leverage || leverage === 0 || !isFinite(leverage)) {
          const fetchedLeverage = this.store.fetchedLeverage[p.symbol];
          if (fetchedLeverage && !fetchedLeverage.isFetching) {
            leverage = side === PositionSide.Long ? fetchedLeverage.buyLeverage : fetchedLeverage.sellLeverage;
          } else {
            leverage = this.defaultLeverage; // Fallback to default leverage
          }
        }

        return {
          id: createPositionId(p.symbol, side),
          symbol: p.symbol,
          side,
          contracts: Math.abs(baseAmount),
          notional: Math.abs(parseFloat(p.quote)),
          unrealizedPnl: parseFloat(p.pnl),
          realizedPnl: 0, // Arkham doesn't provide realized PnL per position
          leverage,
          entryPrice: parseFloat(p.averageEntryPrice),
          liquidationPrice: 0, // Arkham doesn't provide liquidation price directly
          info: p,
          timestamp: Math.floor(p.lastUpdateTime / 1000), // Convert microseconds to milliseconds
        };
      });

      // Create fake positions for all markets (one-way mode)
      // In one-way mode, only create ONE position per symbol (long side by convention)
      const fakePositions: Position[] = this.store.markets
        .filter((m) => !m.isSpot) // Only for perpetual markets
        .map((m) => {
          // Get leverage from fetched leverage or fallback to default leverage
          const fetchedLeverage = this.store.fetchedLeverage[m.symbol];
          const leverage =
            fetchedLeverage && !fetchedLeverage.isFetching ? fetchedLeverage.buyLeverage : this.defaultLeverage;

          // For one-way mode, create a single fake long position
          return {
            id: createPositionId(m.symbol, PositionSide.Long),
            symbol: m.symbol,
            side: PositionSide.Long,
            entryPrice: 0,
            notional: 0,
            unrealizedPnl: 0,
            contracts: 0,
            leverage,
            liquidationPrice: 0,
          };
        });

      return uniqBy([...parsedPositions, ...fakePositions], 'id');
    } catch (err) {
      this.log(`Error fetching positions: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  fetchOrderBook = async (symbol: string, depth = 20): Promise<OrderBook> => {
    try {
      const { data } = await this.xhr.get<ORDERBOOK_RESPONSE>(ENDPOINTS.ORDERBOOK, {
        params: { symbol, depth },
      });

      return {
        bids: data.bids.map(([price, size]) => ({
          price: parseFloat(price),
          amount: parseFloat(size),
          total: parseFloat(price) * parseFloat(size),
        })),
        asks: data.asks.map(([price, size]) => ({
          price: parseFloat(price),
          amount: parseFloat(size),
          total: parseFloat(price) * parseFloat(size),
        })),
      };
    } catch (err) {
      this.log(`Error fetching orderbook: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  placeOrder = async (opts: PlaceOrderOpts): Promise<string[]> => {
    try {
      const side = opts.side === OrderSide.Buy ? 'buy' : 'sell';
      const isTriggerOrder = opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit;

      // Get market precision for size adjustment
      const market = this.store.marketsBySymbol[opts.symbol];
      if (!market) {
        throw new Error(`Market ${opts.symbol} not found`);
      }

      const pAmount = market.precision.amount;
      const pPrice = market.precision.price;
      const amount = adjust(opts.amount, pAmount);
      const price = opts.price ? adjust(opts.price, pPrice) : undefined;

      // For trigger orders (stop loss, take profit), use the trigger orders endpoint
      if (isTriggerOrder) {
        const triggerPrice = opts.price;
        if (!triggerPrice) {
          throw new Error('Trigger price is required for stop loss and take profit orders');
        }

        // Find position to determine if this is actually TP or SL based on entry price
        // For reduceOnly orders: BUY closes SHORT, SELL closes LONG
        const expectedPositionSide = side === 'buy' ? PositionSide.Short : PositionSide.Long;
        const position = this.store.positions.find(
          (p) => p.symbol === opts.symbol && p.side === expectedPositionSide && +p.contracts > 0,
        );

        // Determine triggerType based on position entry price vs trigger price
        // IMPORTANT: Arkham's API semantics:
        // - triggerType='stopLoss' → triggers when price goes DOWN (below trigger price)
        // - triggerType='takeProfit' → triggers when price goes UP (above trigger price)
        let triggerType: 'stopLoss' | 'takeProfit';
        let isActuallyTakeProfit = false;

        if (position) {
          // Compare trigger price with position entry price to determine intent
          if (side === 'sell') {
            // SELL order (closing LONG): compare trigger vs entry
            // If trigger > entry → TP (sell higher) → need UP trigger → 'takeProfit'
            // If trigger < entry → SL (sell lower) → need DOWN trigger → 'stopLoss'
            isActuallyTakeProfit = triggerPrice > position.entryPrice;
          } else {
            // BUY order (closing SHORT): compare trigger vs entry
            // If trigger < entry → TP (buy lower) → need DOWN trigger → 'stopLoss'
            // If trigger > entry → SL (buy higher) → need UP trigger → 'takeProfit'
            isActuallyTakeProfit = triggerPrice < position.entryPrice;
          }

          triggerType = isActuallyTakeProfit ? 'takeProfit' : 'stopLoss';

          this.log(
            `Placing trigger: ${opts.symbol} ${side} @ ${triggerPrice}, ` +
              `position entry: ${position.entryPrice}, ` +
              `isTP: ${isActuallyTakeProfit}, triggerType: ${triggerType}`,
          );
        } else {
          // No position found - use current mark price to determine TP vs SL
          const ticker = this.store.tickersBySymbol[opts.symbol];
          const markPrice = ticker?.mark || ticker?.last;

          if (markPrice) {
            // Compare trigger price with current mark price
            if (side === 'sell') {
              // SELL order: compare trigger vs mark
              // If trigger > mark → TP (sell higher) → need UP trigger → 'takeProfit'
              // If trigger < mark → SL (sell lower) → need DOWN trigger → 'stopLoss'
              isActuallyTakeProfit = triggerPrice > markPrice;
            } else {
              // BUY order: compare trigger vs mark
              // If trigger < mark → TP (buy lower) → need DOWN trigger → 'stopLoss'
              // If trigger > mark → SL (buy higher) → need UP trigger → 'takeProfit'
              isActuallyTakeProfit = triggerPrice < markPrice;
            }

            triggerType = isActuallyTakeProfit ? 'takeProfit' : 'stopLoss';

            this.log(
              `Placing trigger without position: ${opts.symbol} ${side} @ ${triggerPrice}, ` +
                `mark price: ${markPrice}, isTP: ${isActuallyTakeProfit}, triggerType: ${triggerType}`,
            );
          } else {
            // No ticker found - fall back to order type
            isActuallyTakeProfit = opts.type === OrderType.TakeProfit;
            if (opts.type === OrderType.StopLoss) {
              triggerType = side === 'buy' ? 'takeProfit' : 'stopLoss';
            } else {
              triggerType = side === 'buy' ? 'stopLoss' : 'takeProfit';
            }

            this.log(
              `Placing trigger without position/ticker: ${opts.symbol} ${side} @ ${triggerPrice}, ` +
                `using order type fallback, triggerType: ${triggerType}`,
            );
          }
        }

        const adjustedTriggerPrice = adjust(triggerPrice, pPrice);

        // Get trigger price type from opts or use default (mark price)
        // Use the actual determination (isActuallyTakeProfit) rather than opts.type
        const triggerPriceType = this.mapTriggerPriceType(
          isActuallyTakeProfit ? opts.tpTriggerType : opts.slTriggerType,
        );

        const triggerPayload = omitUndefined({
          symbol: opts.symbol,
          side,
          type: 'market', // Trigger orders execute as market orders when triggered
          size: amount.toString(),
          triggerPrice: adjustedTriggerPrice.toString(),
          triggerType,
          triggerPriceType,
          reduceOnly: opts.reduceOnly,
        });

        const { data } = await this.xhr.post<TRIGGER_ORDER_RESPONSE>(ENDPOINTS.PLACE_TRIGGER_ORDER, triggerPayload);

        return [data.triggerOrderId.toString()]; // Note: different field name from regular orders
      }

      // Regular orders (market, limit)
      const type = this.mapOrderType(opts.type, opts.timeInForce);

      // Arkham only supports reduceOnly for market orders
      // Do not include reduceOnly for limit orders
      const shouldIncludeReduceOnly = opts.type === OrderType.Market && opts.reduceOnly;

      const payload = omitUndefined({
        symbol: opts.symbol,
        side,
        type,
        size: amount.toString(),
        price: price?.toString(),
        reduceOnly: shouldIncludeReduceOnly ? opts.reduceOnly : undefined,
      });

      const { data } = await this.xhr.post<ORDER_RESPONSE>(ENDPOINTS.PLACE_ORDER, payload);

      return [data.orderId.toString()];
    } catch (err) {
      this.log(`Error placing order: ${err}`, LogSeverity.Error);
      throw err;
    }
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

  updateOrder = async (opts: UpdateOrderOpts): Promise<string[]> => {
    // Arkham doesn't support order updates, need to cancel and replace
    try {
      await this.cancelOrder(opts.order.id);

      const side = opts.order.side;
      const newPrice = 'price' in opts.update ? opts.update.price : opts.order.price;
      const newAmount = 'amount' in opts.update ? opts.update.amount : opts.order.amount;

      this.log(
        `[new order] ${JSON.stringify({
          symbol: opts.order.symbol,
          side,
          type: opts.order.type,
          amount: newAmount,
          price: newPrice,
          reduceOnly: opts.order.reduceOnly,
        })}`,
        LogSeverity.Debug,
      );

      return await this.placeOrder({
        symbol: opts.order.symbol,
        side,
        type: opts.order.type,
        amount: newAmount,
        price: newPrice,
        reduceOnly: opts.order.reduceOnly,
      });
    } catch (err) {
      this.log(`Error updating order: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  cancelOrder = async (orderId: string): Promise<void> => {
    try {
      // Check if this is a trigger order by looking at the store
      const order = this.store.orders.find((o) => o.id === orderId);
      const isTriggerOrder = order && (order.type === OrderType.StopLoss || order.type === OrderType.TakeProfit);

      // Convert string orderId to number for Arkham API
      const numericOrderId = parseInt(orderId, 10);

      if (isTriggerOrder) {
        // Trigger order cancellation requires symbol parameter
        if (!order.symbol) {
          throw new Error('Symbol is required to cancel trigger order');
        }
        // Use trigger order cancel endpoint with symbol
        await this.xhr.post(ENDPOINTS.CANCEL_TRIGGER_ORDER, {
          triggerOrderId: numericOrderId,
          symbol: order.symbol,
        });
      } else {
        // Use regular order cancel endpoint
        await this.xhr.post(ENDPOINTS.CANCEL_ORDER, { orderId: numericOrderId });
      }
    } catch (err) {
      this.log(`Error canceling order: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  cancelOrders = async (orders: Order[]): Promise<void> => {
    await forEachSeries(orders, async (order) => {
      try {
        await this.cancelOrder(order.id);
      } catch (err) {
        this.log(`Error canceling order ${order.id}: ${err}`, LogSeverity.Error);
      }
    });
  };

  cancelAllOrders = async (): Promise<void> => {
    try {
      await this.xhr.post(ENDPOINTS.CANCEL_ALL_ORDERS);
    } catch (err) {
      this.log(`Error canceling all orders: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  cancelSymbolOrders = async (symbol: string): Promise<void> => {
    try {
      await this.xhr.post(ENDPOINTS.CANCEL_ALL_ORDERS, { symbol });
    } catch (err) {
      this.log(`Error canceling symbol orders: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  changeMarginType = async (_isolated: boolean, _symbol?: string): Promise<void> => {
    // Arkham uses a dynamic margin schedule system rather than isolated/cross margin modes
    // The margin requirements are automatically adjusted based on position size
    this.log(
      'Arkham uses dynamic margin schedules. Isolated/Cross margin modes are not applicable.',
      LogSeverity.Warning,
    );
  };

  setLeverage = async (symbol: string, inputLeverage: number, _side?: PositionSide): Promise<void> => {
    try {
      const market = this.store.marketsBySymbol[symbol];
      if (!market) {
        throw new Error(`Market not found for symbol: ${symbol}`);
      }

      // Note: Arkham uses dynamic leverage based on position size and margin schedules
      // This sets position limits but actual leverage may vary based on position size
      await this.xhr.post(ENDPOINTS.SET_LEVERAGE, {
        symbol: market.id,
        leverage: inputLeverage.toString(),
      });

      this.defaultLeverage = inputLeverage;

      // Update the fetchedLeverage cache
      this.store.update({
        fetchedLeverage: {
          ...this.store.fetchedLeverage,
          [symbol]: {
            buyLeverage: inputLeverage,
            sellLeverage: inputLeverage,
          },
        },
      });
    } catch (err) {
      this.log(`Error setting leverage for ${symbol}: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  fetchLeverageForSymbol = async (symbol: string): Promise<FetchedLeverage | null> => {
    try {
      const market = this.store.marketsBySymbol[symbol];
      if (!market) {
        throw new Error(`Market not found for symbol: ${symbol}`);
      }

      const { data } = await this.xhr.get<LEVERAGE_RESPONSE[]>(ENDPOINTS.LEVERAGE, {
        params: {
          symbol: market.id,
        },
      });

      // Response is an array, find the matching symbol
      const leverageData = data.find((item) => item.symbol === market.id);

      // If data is empty array or leverageData not found, use default leverage
      if (!leverageData || data.length === 0) {
        const leverageObj: FetchedLeverage = {
          buyLeverage: this.defaultLeverage,
          sellLeverage: this.defaultLeverage,
        };

        this.log(`[LEVERAGE] Using default ${JSON.stringify(leverageObj)}`, LogSeverity.Debug);

        this.store.update({
          fetchedLeverage: {
            ...this.store.fetchedLeverage,
            [symbol]: leverageObj,
          },
        });

        return leverageObj;
      }

      const leverageObj: FetchedLeverage = {
        buyLeverage: leverageData.leverage,
        sellLeverage: leverageData.leverage,
      };

      this.log(`[LEVERAGE] ${JSON.stringify(leverageObj)}`, LogSeverity.Debug);

      this.store.update({
        fetchedLeverage: {
          ...this.store.fetchedLeverage,
          [symbol]: leverageObj,
        },
      });

      return leverageObj;
    } catch (err) {
      this.log(`Error fetching leverage for ${symbol}: ${err}`, LogSeverity.Warning);
      return null;
    }
  };

  private mapOrder = (o: ORDER_RESPONSE): Order => {
    const side = ORDER_SIDE[o.side];
    const type = this.reverseMapOrderType(o.type);
    const status = this.mapOrderStatus(o.status || 'new');

    return {
      id: o.orderId.toString(),
      symbol: o.symbol,
      side,
      type,
      status,
      price: o.price ? parseFloat(o.price) : 0,
      amount: parseFloat(o.size),
      filled: o.filledSize ? parseFloat(o.filledSize) : 0,
      remaining: o.remainingSize ? parseFloat(o.remainingSize) : parseFloat(o.size),
      reduceOnly: false,
      timeInForce: undefined,
      info: o,
      lastUpdate: Math.floor(o.time / 1000), // Convert microseconds to milliseconds
    };
  };

  private mapTriggerOrderType = (triggerType: string, _side: OrderSide): OrderType => {
    // Map triggerType to OrderType based on Arkham's actual semantics:
    // - triggerType='stopLoss' → triggers when price goes DOWN
    // - triggerType='takeProfit' → triggers when price goes UP
    //
    // SELL + 'takeProfit' → selling on price rise → TP for long → OrderType.TakeProfit
    // SELL + 'stopLoss' → selling on price drop → SL for long → OrderType.StopLoss
    // BUY + 'stopLoss' → buying on price drop → TP for short → OrderType.TakeProfit
    // BUY + 'takeProfit' → buying on price rise → SL for short → OrderType.StopLoss
    if (triggerType === 'stopLoss') {
      return OrderType.StopLoss;
    } else if (triggerType === 'takeProfit') {
      return OrderType.TakeProfit;
    }
    return OrderType.StopLoss; // Default fallback
  };

  private mapTriggerOrder = (o: TRIGGER_ORDER_RESPONSE): Order => {
    const side = ORDER_SIDE[o.side];
    const type = this.mapTriggerOrderType(o.triggerType, side);

    // Map trigger order status to OrderStatus
    const status =
      o.status === 'triggered'
        ? OrderStatus.Closed
        : o.status === 'cancelled'
          ? OrderStatus.Canceled
          : o.status === 'failed'
            ? OrderStatus.Canceled
            : OrderStatus.Open; // 'staged' maps to Open

    return {
      id: o.triggerOrderId.toString(),
      symbol: o.symbol,
      side,
      type,
      status,
      price: parseFloat(o.triggerPrice), // Use triggerPrice for trigger orders
      amount: parseFloat(o.size),
      filled: 0, // Trigger orders haven't filled yet (staged status)
      remaining: parseFloat(o.size),
      reduceOnly: o.reduceOnly,
      timeInForce: undefined,
      info: o,
      lastUpdate: Math.floor(o.time / 1000), // Convert microseconds to milliseconds
    };
  };

  private mapOrderType = (type: OrderType, timeInForce?: string): string => {
    if (type === OrderType.Market) return 'market';
    if (type === OrderType.Limit) {
      if (timeInForce === 'IOC') return 'limitIoc';
      if (timeInForce === 'FOK') return 'limitFok';
      return 'limitGtc';
    }
    return 'limitGtc';
  };

  private reverseMapOrderType = (type: string): OrderType => {
    const mapping: Record<string, OrderType> = {
      market: OrderType.Market,
      limitGtc: OrderType.Limit,
      limitIoc: OrderType.Limit,
      limitFok: OrderType.Limit,
    };
    return mapping[type] || OrderType.Limit;
  };

  private mapOrderStatus = (status: string): OrderStatus => {
    const mapping: Record<string, OrderStatus> = {
      new: OrderStatus.Open,
      taker: OrderStatus.Open,
      booked: OrderStatus.Open,
      maker: OrderStatus.Open,
      cancelled: OrderStatus.Canceled,
      closed: OrderStatus.Closed,
    };
    return mapping[status] || OrderStatus.Open;
  };

  private mapTriggerPriceType = (triggerType?: OrderStopTriggerType): 'last' | 'mark' | 'index' => {
    // Map OrderStopTriggerType enum to Arkham API format
    // Default to 'mark' as specified in exchangeMetadata
    if (!triggerType) {
      return 'mark';
    }

    const mapping: Record<OrderStopTriggerType, 'last' | 'mark' | 'index'> = {
      [OrderStopTriggerType.MarkPrice]: 'mark',
      [OrderStopTriggerType.LastPrice]: 'last',
      [OrderStopTriggerType.IndexPrice]: 'index',
    };

    return mapping[triggerType] || 'mark';
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    return this.publicWebsocket.listenOHLCV(opts, callback);
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    return this.publicWebsocket.listenOrderBook(symbol, callback);
  };
}
