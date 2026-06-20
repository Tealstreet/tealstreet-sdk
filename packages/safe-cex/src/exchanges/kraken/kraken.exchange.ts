import type { Axios } from 'axios';
import type { Store } from '../../store/store.interface';

import rateLimit from 'axios-rate-limit';

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
  OrderStopTriggerType,
  OrderTimeInForce,
  OrderType,
  PlaceOrderOpts,
  Position,
  PositionMode,
  PositionSide,
  Ticker,
  UpdateOrderOpts,
} from '../../types';
import { precisionToString } from '../../utils/ccxt_helpers/precision';
import { v } from '../../utils/get-key';
import { loop } from '../../utils/loop';
import { adjust } from '../../utils/safe-math';
import { uuid } from '../../utils/uuid';
import { BaseExchange } from '../base';
import { createAPI } from './kraken.api';
import { ENDPOINTS } from './kraken.types';
import { KrakenPrivateWebsocket } from './kraken.ws-private';
import { KrakenPublicWebsocket } from './kraken.ws-public';

export class KrakenExchange extends BaseExchange {
  xhr: Axios;
  unlimitedXHR: Axios;
  declare publicWebsocket?: KrakenPublicWebsocket;
  declare privateWebsocket?: KrakenPrivateWebsocket;
  leverageSettings: Map<string, number> = new Map();
  marginTypePreferences: Map<string, MarginType> = new Map(); // Cache user's margin type choices

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    this.xhr = this.createXhrWithSecondaryKey(opts, createAPI, { rateLimit: (xhr) => rateLimit(xhr, { maxRPS: 5 }) });
    this.unlimitedXHR = createAPI(opts, this);
  }

  dispose = () => {
    // Clean up OHLCV polling
    for (const [, timeoutId] of this.ohlcvPollingTimeouts) {
      clearTimeout(timeoutId);
    }

    // Mark all polling as inactive
    for (const key of this.ohlcvPollingActive.keys()) {
      this.ohlcvPollingActive.set(key, false);
    }

    this.ohlcvPollingTimeouts.clear();
    this.ohlcvPollingActive.clear();
    this.lastCandleCache.clear();

    super.dispose();
  };

  start = async () => {
    const useExternalPublicData = await this.initExternalPublicData();

    if (!useExternalPublicData) {
      const markets = await this.fetchMarkets();
      if (this.isDisposed) return;

      this.store.update({ markets, loaded: { ...this.store.loaded, markets: true } });

      const tickers = await this.fetchTickers();
      if (this.isDisposed) return;

      this.store.update({ tickers, loaded: { ...this.store.loaded, tickers: true } });

      // Start public websocket for live tickers
      this.publicWebsocket = new KrakenPublicWebsocket(this);
      this.publicWebsocket.connectAndSubscribe();
    }

    // Start private websocket for orders/positions if credentials are provided
    if (!this.isPublicDataOnly) {
      this.privateWebsocket = new KrakenPrivateWebsocket(this);
      this.privateWebsocket.connectAndSubscribe();
    }

    if (this.isPublicDataOnly) {
      this.store.update({
        loaded: { ...this.store.loaded, balance: true, positions: true, orders: true },
      });
      return;
    }

    // Start the tick cycle to fetch initial private data and maintain updates
    // Pass 0 as initial count to trigger immediate fetch
    this.tick(0);
  };

  fetchMarkets = async () => {
    try {
      const { data } = await this.xhr.get<{ result: string; instruments: Array<Record<string, any>> }>(
        ENDPOINTS.MARKETS,
      );
      const symbols = (data?.instruments || []).filter((i) => {
        // Filter for tradeable instruments
        if (v(i, 'tradeable') === false) return false;

        // For Kraken, only include symbols that start with PF_ (perpetual futures)
        const symbol = v(i, 'symbol') || v(i, 'symbolName') || '';
        return symbol.startsWith('PF_');
      });

      const markets: Market[] = symbols.map((m) => {
        const symbol = v(m, 'symbol') || v(m, 'symbolName') || '';
        const base = v(m, 'base') || v(m, 'underlying') || '';
        const quote = v(m, 'quote') || v(m, 'quoteCurrency') || 'USD';

        // Get Kraken market data fields
        const contractValueTradePrecision = v(m, 'contractValueTradePrecision');
        const tickSize = v(m, 'tickSize');
        const contractSize = v(m, 'contractSize') || 1;

        // contractValueTradePrecision is the number of decimal places for order size
        // e.g., 0 means whole numbers (precision=1), 4 means 0.0001
        // Note: must use !== undefined/null check since 0 is a valid value (whole numbers)
        let amountPrecision = 1; // Default to whole numbers
        if (contractValueTradePrecision !== undefined && contractValueTradePrecision !== null) {
          const decimalPlaces = parseInt(contractValueTradePrecision);
          amountPrecision = parseFloat(precisionToString(decimalPlaces));
        }

        // For price: tickSize is a decimal (e.g., 0.1, 0.5, 1.0)
        let pricePrecision = tickSize || 0.01;

        const active = v(m, 'status') !== 'cancel_only';

        // Strip PF_ prefix for pretty display
        const prettySymbol = symbol.startsWith('PF_') ? symbol.slice(3) : symbol;

        // Calculate max leverage from marginLevels
        // maxLeverage = 1 / initialMargin at the lowest tier (contracts: 0)
        let maxLeverage = 100; // Default fallback
        const marginLevels = v(m, 'marginLevels');
        if (Array.isArray(marginLevels) && marginLevels.length > 0) {
          // Get the first tier (lowest contracts threshold)
          const firstTier = marginLevels[0];
          const initialMargin = parseFloat(v(firstTier, 'initialMargin') || 0.01);
          if (initialMargin > 0) {
            maxLeverage = Math.floor(1 / initialMargin);
          }
        }

        return {
          id: `${base}/${quote}`,
          symbol,
          prettySymbol,
          base,
          quote,
          active,
          precision: { amount: amountPrecision, price: pricePrecision },
          limits: {
            amount: { min: 0, max: Number.MAX_SAFE_INTEGER },
            leverage: { min: 1, max: maxLeverage },
          },
          contractSize,
          isExpiring: !!v(m, 'lastTradingTime'),
          isInverse: v(m, 'contractType') === 'inverse',
          isSpot: false,
          info: m,
        };
      });

      return markets;
    } catch (err: any) {
      console.trace(err);
      return this.store.markets;
    }
  };

  fetchTickers = async () => {
    try {
      const { data } = await this.xhr.get<{ result: string; tickers: Array<Record<string, any>> }>(ENDPOINTS.TICKERS);
      const tickers = (data?.tickers || []) as Array<Record<string, any>>;

      return tickers.reduce((acc: Ticker[], t) => {
        const market = this.store.marketsBySymbol[v(t, 'symbol')];
        if (!market) return acc;

        // Kraken Futures `/v3/tickers` field names are a mix of
        // camelCase (`markPrice`, `indexPrice`, `volumeQuote`,
        // `openInterest`, `change24h`, `fundingRate`) and snake_case
        // (`vol24h`). The `v()` helper tries literal / snake / camel
        // forms of each key, but none of those variants match
        // `markPrice` / `indexPrice` / `vol24h` / `volumeQuote`, so
        // we read each of those directly with the exact field name
        // Kraken publishes. Previously these silently returned 0 and
        // were only ever populated after the first WS tick.
        const mark = parseFloat(t.markPrice) || 0;
        const index = parseFloat(t.indexPrice) || 0;

        // For illiquid perps (zero 24h volume), Kraken omits the
        // `last` field entirely from `/v3/tickers` even though
        // bid/ask/markPrice are all live — the contract is
        // tradeable, there just hasn't been a trade in the ticker
        // window. Fall back to mark so the UI doesn't render "-"
        // for a pair whose fair price is perfectly well known; the
        // first real trade on the WS feed will overwrite this via
        // `store.updateTicker`.
        const realLast = parseFloat(v(t, 'last')) || 0;
        const last = realLast > 0 ? realLast : mark;

        // `change24h` is already a percentage (e.g. -1.15), no need
        // to compute it from `open24h` vs `last`.
        const percentage = parseFloat(t.change24h) || 0;

        // Kraken Futures exposes `fundingRate` as quote currency per
        // contract (e.g. 0.207 USD per PF_XBTUSD contract), NOT a
        // fractional rate. The matching fractional field
        // `relative_funding_rate` exists only on the WS `ticker` feed,
        // not on `/v3/tickers`, so we approximate it here with
        // `fundingRate / last`. The first WS tick on each symbol
        // overwrites this with the real value via
        // `KrakenPublicWebsocket.handleTicker`.
        const fundingRateAbsolute = parseFloat(v(t, 'fundingRate')) || 0;
        const fundingRate = last > 0 ? fundingRateAbsolute / last : 0;

        const finalTicker: Ticker = {
          id: market.id,
          symbol: market.symbol,
          bid: parseFloat(v(t, 'bid')) || 0,
          ask: parseFloat(v(t, 'ask')) || 0,
          last,
          mark: mark > 0 ? mark : last,
          index: index > 0 ? index : last,
          percentage,
          fundingRate,
          // REST does not expose the next funding timestamp — only
          // the WS `ticker` feed carries `next_funding_rate_time`.
          // Leave this at 0 here and let the first WS update fill
          // it in via `store.updateTicker`.
          fundingTime: 0,
          volume: parseFloat(t.vol24h) || 0,
          quoteVolume: parseFloat(t.volumeQuote) || 0,
          openInterest: parseFloat(v(t, 'openInterest')) || 0,
          info: t,
        };

        return [...acc, finalTicker];
      }, []);
    } catch (err: any) {
      console.trace(err);
      return this.store.tickers;
    }
  };

  // Stubs for futures functionality to be implemented next
  fetchOrders = async (): Promise<Order[]> => {
    try {
      const { data } = await this.xhr.get<{ result: string; openOrders: Array<Record<string, any>> }>(
        ENDPOINTS.OPEN_ORDERS,
      );

      const list = (data?.openOrders || []) as Array<Record<string, any>>;
      const orders: Order[] = list.map((o) => {
        const typ = String((o as any).orderType ?? (o as any).type ?? 'lmt').toLowerCase();
        const tif: OrderTimeInForce =
          typ === 'post'
            ? OrderTimeInForce.PostOnly
            : typ === 'ioc'
              ? OrderTimeInForce.ImmediateOrCancel
              : typ === 'fok'
                ? OrderTimeInForce.FillOrKill
                : OrderTimeInForce.GoodTillCancel;

        // Determine order type
        const orderTypeStr = String((o as any).orderType || (o as any).type || 'lmt').toLowerCase();
        let orderType: OrderType;
        if (orderTypeStr === 'stop' || orderTypeStr === 'stp') {
          orderType = OrderType.StopLoss;
        } else if (orderTypeStr === 'take_profit') {
          orderType = OrderType.TakeProfit;
        } else if (orderTypeStr.includes('lmt') || orderTypeStr === 'limit') {
          orderType = OrderType.Limit;
        } else {
          orderType = OrderType.Market;
        }

        return {
          id: String((o as any).order_id || (o as any).orderId || ''),
          status: OrderStatus.Open,
          symbol: String((o as any).instrument || (o as any).symbol || ''),
          type: orderType,
          side:
            (o as any).direction !== undefined
              ? (o as any).direction === 0 || (o as any).direction === 'buy'
                ? OrderSide.Buy
                : OrderSide.Sell
              : String((o as any).side || 'buy').toLowerCase() === 'buy'
                ? OrderSide.Buy
                : OrderSide.Sell,
          price:
            orderType === OrderType.StopLoss || orderType === OrderType.TakeProfit
              ? parseFloat((o as any).stopPrice ?? (o as any).stop_price ?? 0)
              : parseFloat((o as any).limitPrice ?? (o as any).limit_price ?? (o as any).price ?? 0),
          amount:
            parseFloat((o as any).unfilledSize ?? (o as any).qty ?? (o as any).quantity ?? (o as any).size ?? 0) +
            parseFloat((o as any).filledSize ?? (o as any).filled ?? 0),
          reduceOnly: !!(o as any).reduceOnly || !!(o as any).reduce_only,
          filled: parseFloat((o as any).filledSize ?? (o as any).filled ?? 0),
          lastUpdate: Date.parse(
            (o as any).lastUpdateTime || (o as any).lastUpdateTimestamp || new Date().toISOString(),
          ),
          remaining: parseFloat((o as any).unfilledSize ?? 0),
          info: o,
          timeInForce: tif,
        } as Order;
      });
      return orders;
    } catch (err) {
      console.trace(err);
      return this.store.orders || [];
    }
  };

  placeOrder = async (opts: PlaceOrderOpts): Promise<string[]> => {
    try {
      // Map order type/timeInForce
      let orderType = opts.type === OrderType.Limit ? 'lmt' : 'mkt';

      // Handle stop loss and take profit orders with specific order types
      if (opts.type === OrderType.StopLoss) {
        orderType = 'stp';
      } else if (opts.type === OrderType.TakeProfit) {
        orderType = 'take_profit';
      } else if (opts.type === OrderType.Limit) {
        if (opts.timeInForce === OrderTimeInForce.PostOnly) orderType = 'post';
        else if (opts.timeInForce === OrderTimeInForce.ImmediateOrCancel) orderType = 'ioc';
        else if (opts.timeInForce === OrderTimeInForce.FillOrKill) orderType = 'fok';
      }

      // Find the market to get precision
      const market = this.store.marketsBySymbol[opts.symbol];
      if (!market) {
        throw new Error(`Market ${opts.symbol} not found`);
      }

      // Round size to market tick size precision (e.g. precision.amount = 1 means whole numbers only)
      const size = adjust(opts.amount, market.precision.amount);

      const body: Record<string, any> = {
        orderType,
        symbol: opts.symbol,
        side: opts.side === OrderSide.Buy ? 'buy' : 'sell',
        size,
        cliOrdId: 'ts_' + uuid(),
        broker: 'AA73 N84G 7YOF J26Y',
      };

      // Add limitPrice for limit orders, rounded to tick size
      if (opts.type === OrderType.Limit && opts.price) {
        body.limitPrice = adjust(opts.price, market.precision.price);
      }

      // Handle stop/take-profit orders
      const toSignal = (t?: OrderStopTriggerType) =>
        t === OrderStopTriggerType.MarkPrice ? 'mark' : t === OrderStopTriggerType.IndexPrice ? 'index' : 'last';

      if (opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit) {
        // Stop and take profit orders must have stopPrice and are always reduceOnly
        if (opts.price) {
          body.stopPrice = adjust(opts.price, market.precision.price);
        }
        body.reduceOnly = true; // Always reduce-only for stop/take-profit

        const sig = opts.type === OrderType.StopLoss ? opts.slTriggerType : opts.tpTriggerType;
        if (sig) {
          body.triggerSignal = toSignal(sig);
        }
      } else if (opts.reduceOnly) {
        // Add reduceOnly for regular orders if specified
        body.reduceOnly = true;
      }

      const { data } = await this.unlimitedXHR.post(ENDPOINTS.ORDER, body);

      // Kraken returns result: 'success' just to acknowledge receipt.
      // The actual order status is in sendStatus.status
      if (data?.result !== 'success') {
        const errorMessage = data?.error || 'Order placement failed';
        throw new Error(errorMessage);
      }

      // Check the actual order status - 'placed' or 'filled' means success
      const sendStatus = data?.sendStatus?.status;
      if (sendStatus && sendStatus !== 'placed' && sendStatus !== 'filled') {
        // Kraken rejected the order (e.g. invalidSize, insufficientAvailableFunds, etc.)
        throw new Error(`Order rejected: ${sendStatus}`);
      }

      const id = String(data?.sendStatus?.order_id || '');

      const ids: string[] = id ? [id] : [];
      if (opts.type === OrderType.Limit && (opts.stopLoss || opts.takeProfit)) {
        const derived = this.deriveAlgoOrdersFromNormalOrdersOpts([opts]);
        const extraIds = await this.placeOrders(derived);
        ids.push(...extraIds);
      }
      return ids;
    } catch (err: any) {
      // Extract detailed error information
      const responseData = err?.response?.data;
      let errorMessage = err?.message || 'Failed to place order';

      if (responseData?.error) {
        errorMessage = responseData.error;
      } else if (responseData?.result === 'error') {
        errorMessage = responseData.error || 'Order placement failed';
      }

      // Provide more specific error messages for common Kraken rejection reasons
      const lowerError = errorMessage.toLowerCase();
      if (lowerError.includes('invalidsize')) {
        errorMessage = `Invalid order size for ${opts.symbol}. Check minimum order size and precision.`;
      } else if (lowerError.includes('insufficientavailablefunds') || lowerError.includes('insufficient')) {
        errorMessage = `Insufficient available funds to place this order.`;
      } else if (lowerError.includes('authentication') || lowerError.includes('apikey')) {
        errorMessage =
          'Authentication failed. Your API key may lack trading permissions. Please ensure General API is set to "Full access".';
      } else if (lowerError.includes('permission')) {
        errorMessage =
          'Permission denied. Please ensure General API is set to "Full access" for your Kraken Futures API key.';
      } else if (lowerError.includes('size') || lowerError.includes('quantity')) {
        errorMessage = `Invalid order size. ${errorMessage}`;
      } else if (lowerError.includes('price')) {
        errorMessage = `Invalid order price. ${errorMessage}`;
      } else if (lowerError.includes('margin') || lowerError.includes('balance')) {
        errorMessage = `Insufficient margin or balance. ${errorMessage}`;
      }

      this.emitAndLogError(errorMessage, 'PlaceOrder Error');
      throw new Error(errorMessage);
    }
  };

  updateOrder = async (_opts: UpdateOrderOpts): Promise<string[]> => {
    try {
      const { order, update } = _opts;

      // For stop loss and take profit orders, cancel and recreate
      if (order.type === OrderType.StopLoss || order.type === OrderType.TakeProfit) {
        // Cancel the existing order
        await this.cancelOrders([order]);

        // Create new order with updated parameters
        const newOrderOpts: PlaceOrderOpts = {
          symbol: order.symbol,
          side: order.side,
          type: order.type,
          amount: (update as any).amount !== undefined ? (update as any).amount : order.amount,
          price: (update as any).price !== undefined ? (update as any).price : order.price,
          reduceOnly: order.reduceOnly,
        };

        // Place the new order
        return await this.placeOrder(newOrderOpts);
      }

      // For regular limit orders, use the edit endpoint
      const market = this.store.marketsBySymbol[order.symbol];
      const body: Record<string, any> = { orderId: order.id };
      if ((update as any).amount !== undefined) {
        body.size = market ? adjust((update as any).amount, market.precision.amount) : (update as any).amount;
      }
      if ((update as any).price !== undefined) {
        body.limitPrice = market ? adjust((update as any).price, market.precision.price) : (update as any).price;
      }
      const { data } = await this.unlimitedXHR.post(ENDPOINTS.EDIT_ORDER, body);

      // Handle response format: { result: "success", editStatus: { orderId: "...", ... } }
      if (data?.result !== 'success') {
        const errorMessage = data?.error || 'Order update failed';
        throw new Error(errorMessage);
      }

      const editedId = String(data?.editStatus?.orderId || data?.editStatus?.order_id || order.id);
      return editedId ? [editedId] : [];
    } catch (err: any) {
      const errorMessage = err?.response?.data?.error || err?.message || 'Failed to update order';
      this.emitAndLogError(errorMessage, 'UpdateOrder Error');
      throw new Error(errorMessage);
    }
  };

  cancelOrders = async (orders: Order[]): Promise<void> => {
    try {
      if (orders.length === 1) {
        const { data } = await this.unlimitedXHR.post(ENDPOINTS.CANCEL_ORDER, { order_id: orders[0].id });
        if (data?.result !== 'success') {
          const errorMessage = data?.error || 'Order cancellation failed';
          throw new Error(errorMessage);
        }
        return;
      }
      const batchOrder = orders.map((o) => ({ order: 'cancel', order_id: o.id }));
      const { data } = await this.unlimitedXHR.post(ENDPOINTS.BATCH_ORDERS, { batchOrder });
      if (data?.result !== 'success') {
        const errorMessage = data?.error || 'Batch order cancellation failed';
        throw new Error(errorMessage);
      }
    } catch (err: any) {
      const errorMessage = err?.response?.data?.error || err?.message || 'Failed to cancel orders';
      this.emitAndLogError(errorMessage, 'CancelOrders Error');
      throw new Error(errorMessage);
    }
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    if (!this.publicWebsocket) {
      this.publicWebsocket = new KrakenPublicWebsocket(this);
      this.publicWebsocket.connectAndSubscribe();
    }
    return this.publicWebsocket.listenOrderBook(symbol, callback);
  };

  fetchPositions = async (): Promise<Position[]> => {
    try {
      const { data } = await this.xhr.get<{ result: string; openPositions: Array<Record<string, any>> }>(
        ENDPOINTS.POSITIONS,
      );

      const openPositions = (data?.openPositions || []) as Array<Record<string, any>>;

      // Map actual positions from API
      const actualPositions = openPositions.map((p) => {
        const symbol = String(p.symbol || '');

        // Detect margin mode based on cached preference first, then API response
        // Priority: 1) User's cached preference, 2) API maxFixedLeverage field
        const cachedMarginType = this.marginTypePreferences.get(symbol);
        const maxFixedLeverage = p.maxFixedLeverage !== undefined ? parseFloat(p.maxFixedLeverage) : undefined;

        // Determine margin type:
        // - If user has set a preference, use it
        // - Otherwise detect from API: maxFixedLeverage exists -> isolated, undefined -> cross
        const marginType =
          cachedMarginType ?? (maxFixedLeverage !== undefined ? MarginType.Isolated : MarginType.Cross);

        // Update cache if we detected from API (for consistency)
        if (!cachedMarginType && maxFixedLeverage !== undefined) {
          this.marginTypePreferences.set(symbol, MarginType.Isolated);
        } else if (!cachedMarginType && maxFixedLeverage === undefined) {
          this.marginTypePreferences.set(symbol, MarginType.Cross);
        }

        // Determine leverage:
        // - For isolated: use maxFixedLeverage from position
        // - For cross: use leverage from settings, or get max from market
        let leverage: number;
        if (marginType === MarginType.Isolated && maxFixedLeverage !== undefined) {
          leverage = maxFixedLeverage;
        } else {
          // Cross margin: use cached setting or get from market limits
          const cachedLeverage = this.leverageSettings.get(symbol);
          if (cachedLeverage !== undefined) {
            leverage = cachedLeverage;
          } else {
            // Get max leverage from market
            const market = this.store.marketsBySymbol[symbol];
            leverage = market?.limits.leverage.max ?? 5;
          }
        }

        return {
          id: `${p.symbol}:${p.side}`,
          symbol,
          side: String(p.side || 'long').toLowerCase() === 'long' ? PositionSide.Long : PositionSide.Short,
          entryPrice: parseFloat(p.price ?? 0),
          notional: parseFloat(p.pv ?? 0), // position value
          leverage,
          unrealizedPnl: parseFloat(p.pnl ?? 0),
          contracts: parseFloat(p.size ?? 0),
          liquidationPrice: parseFloat(p.liqPrice ?? 0),
          marginType,
          positionMode: PositionMode.OneWay, // Kraken only supports one-way mode
          info: p,
        };
      });

      // Create fake positions (0 contracts) for all tradeable perpetual markets
      // This allows the UI to show all available symbols even with no active positions
      const existingSymbols = new Set(actualPositions.map((p) => p.symbol));
      const fakePositions: Position[] = [];

      for (const market of this.store.markets) {
        // Only create fake positions for perpetual/futures markets (not spot)
        if (market.active && !market.isSpot && !existingSymbols.has(market.symbol)) {
          // Get margin type from cache, default to cross if not set
          const marginType = this.marginTypePreferences.get(market.symbol) ?? MarginType.Cross;

          // Determine leverage based on margin type
          const cachedLeverage = this.leverageSettings.get(market.symbol);
          const leverage = cachedLeverage !== undefined ? cachedLeverage : market.limits.leverage.max;

          // Create both long and short fake positions for hedged mode support
          fakePositions.push({
            id: `${market.symbol}:long`,
            symbol: market.symbol,
            side: PositionSide.Long,
            entryPrice: 0,
            notional: 0,
            leverage,
            unrealizedPnl: 0,
            contracts: 0,
            liquidationPrice: 0,
            marginType, // Use cached margin type preference
            positionMode: PositionMode.OneWay, // Kraken only supports one-way mode
            info: {},
          });

          fakePositions.push({
            id: `${market.symbol}:short`,
            symbol: market.symbol,
            side: PositionSide.Short,
            entryPrice: 0,
            notional: 0,
            leverage,
            unrealizedPnl: 0,
            contracts: 0,
            liquidationPrice: 0,
            marginType, // Use cached margin type preference
            positionMode: PositionMode.OneWay, // Kraken only supports one-way mode
            info: {},
          });
        }
      }

      // Combine actual and fake positions
      return [...actualPositions, ...fakePositions];
    } catch (err) {
      console.trace(err);
      return this.store.positions || [];
    }
  };

  fetchOHLCV = async (opts: OHLCVOptions): Promise<Candle[]> => {
    try {
      // Map timeframe to Kraken's format for charts API
      const intervalMap: Record<string, string> = {
        '1m': '1m',
        '5m': '5m',
        '15m': '15m',
        '30m': '30m',
        '1h': '1h',
        '4h': '4h',
        '12h': '12h',
        '1d': '1d',
        '1w': '1w',
      };

      const resolution = intervalMap[opts.interval] || '1h';
      const limit = Math.min(opts.limit || 500, 5000);

      // Calculate time range
      const to = opts.to || Date.now();
      let from = opts.from;

      if (!from && opts.limit) {
        // Calculate from based on interval and limit
        const intervalMs: Record<string, number> = {
          '1m': 60 * 1000,
          '5m': 5 * 60 * 1000,
          '15m': 15 * 60 * 1000,
          '30m': 30 * 60 * 1000,
          '1h': 60 * 60 * 1000,
          '4h': 4 * 60 * 60 * 1000,
          '12h': 12 * 60 * 60 * 1000,
          '1d': 24 * 60 * 60 * 1000,
          '1w': 7 * 24 * 60 * 60 * 1000,
        };
        from = to - (intervalMs[opts.interval] || intervalMs['1h']) * limit;
      }

      // Kraken charts API uses path parameters: /api/charts/v1/{tick_type}/{symbol}/{resolution}
      const tick_type = 'trade'; // Use trade data for OHLC

      // Charts API is at a different path than regular API
      // Regular API: https://futures.kraken.com/derivatives/api
      // Charts API: https://futures.kraken.com/api/charts/v1
      // We need to construct the full URL since it's outside the base path
      const baseHost = this.options.testnet ? 'https://demo-futures.kraken.com' : 'https://futures.kraken.com';
      const chartsUrl = `${baseHost}/api/charts/v1/${tick_type}/${opts.symbol}/${resolution}`;

      // Query parameters for time range
      const params: Record<string, any> = {};
      if (from) {
        params.from = Math.floor(from / 1000); // Convert to seconds
      }
      if (to) {
        params.to = Math.floor(to / 1000); // Convert to seconds
      }

      // Use full URL for charts API
      const { data } = await this.xhr.get<{
        candles: Array<{
          time: number;
          open: string;
          high: string;
          low: string;
          close: string;
          volume: string;
        }>;
        more_candles?: boolean;
      }>(chartsUrl, { params });

      // Note: Kraken charts API doesn't return a 'result' field like v3 endpoints
      if (!data?.candles) {
        console.error('[Kraken] No candles in response, full response:', data);
        throw new Error('No candles data received');
      }

      const candles: Candle[] = data.candles.map((candle) => ({
        timestamp: candle.time / 1000, // Convert ms to seconds
        open: parseFloat(candle.open),
        high: parseFloat(candle.high),
        low: parseFloat(candle.low),
        close: parseFloat(candle.close),
        volume: parseFloat(candle.volume),
      }));

      // Sort by timestamp ascending
      const sorted = candles.sort((a, b) => a.timestamp - b.timestamp);
      return sorted;
    } catch (err: any) {
      console.error('[Kraken] fetchOHLCV error:', err?.message, err);
      this.emitAndLogError(err?.message || 'Failed to fetch OHLCV');
      return [];
    }
  };

  // Store polling timeouts and active status
  private ohlcvPollingTimeouts = new Map<string, NodeJS.Timeout>();
  private ohlcvPollingActive = new Map<string, boolean>();
  private lastCandleCache = new Map<string, Candle>();

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    // Kraken Futures doesn't support OHLCV via WebSocket, so we poll the REST API

    const pollKey = `${opts.symbol}:${opts.interval}`;

    // Clear any existing polling for this key
    if (this.ohlcvPollingTimeouts.has(pollKey)) {
      clearTimeout(this.ohlcvPollingTimeouts.get(pollKey)!);
      this.ohlcvPollingTimeouts.delete(pollKey);
    }

    // Mark polling as active
    this.ohlcvPollingActive.set(pollKey, true);

    // Recursive polling function using setTimeout
    const pollCandle = async () => {
      // Check if polling is still active
      if (!this.ohlcvPollingActive.get(pollKey)) {
        return;
      }

      try {
        const now = Date.now();
        const candles = await this.fetchOHLCV({
          symbol: opts.symbol,
          interval: opts.interval,
          limit: 2, // Get last 2 candles to ensure we have the latest
          to: now,
        });

        if (candles.length > 0) {
          const latestCandle = candles[candles.length - 1];
          const previousCandle = this.lastCandleCache.get(pollKey);

          // Only emit if the candle has changed
          if (
            !previousCandle ||
            previousCandle.timestamp !== latestCandle.timestamp ||
            previousCandle.close !== latestCandle.close ||
            previousCandle.volume !== latestCandle.volume
          ) {
            callback(latestCandle);
            this.lastCandleCache.set(pollKey, latestCandle);
          }
        }
      } catch (err) {
        console.error('[Kraken] Error polling OHLCV:', err);
      }

      // Schedule next poll only if still active
      if (this.ohlcvPollingActive.get(pollKey)) {
        const timeoutId = setTimeout(pollCandle, 5000);
        this.ohlcvPollingTimeouts.set(pollKey, timeoutId);
      }
    };

    // Start polling
    pollCandle();

    // Return cleanup function
    return () => {
      // Mark as inactive
      this.ohlcvPollingActive.set(pollKey, false);

      // Clear timeout if exists
      if (this.ohlcvPollingTimeouts.has(pollKey)) {
        clearTimeout(this.ohlcvPollingTimeouts.get(pollKey)!);
        this.ohlcvPollingTimeouts.delete(pollKey);
      }

      // Clean up cache
      this.lastCandleCache.delete(pollKey);
    };
  };

  fetchBalance = async (): Promise<Balance> => {
    try {
      const { data } = await this.xhr.get<{ result: string; accounts: any }>(ENDPOINTS.ACCOUNT);

      if (data?.result !== 'success') {
        throw new Error(data?.result || 'Failed to fetch balance');
      }

      const accounts = data?.accounts || {};

      // Kraken Futures returns different account structures:
      // - 'cash' or 'cashAccount': actual currency balances
      // - 'flex': multi-collateral margin account
      // - 'fi_[symbol]': individual margin accounts per market

      let totalBalance = 0;
      let freeBalance = 0;
      let usedBalance = 0;
      let totalUpnl = 0;

      const currencies: Record<string, any> = {};

      // First, look for cash account balances
      const cashAccount = accounts.cash || accounts.cashAccount;
      if (cashAccount?.balances) {
        for (const [currency, balance] of Object.entries(cashAccount.balances)) {
          const amount = parseFloat(String(balance || 0));
          if (amount > 0) {
            currencies[currency.toUpperCase()] = {
              free: amount,
              used: 0,
              total: amount,
              upnl: 0,
              info: { balance: amount },
            };

            // Track USD-equivalent currencies
            const upperCurrency = currency.toUpperCase();
            if (upperCurrency === 'USD' || upperCurrency === 'USDT' || upperCurrency === 'USDC') {
              totalBalance += amount;
              freeBalance += amount;
            }
          }
        }
      }

      // Look for flex (multi-collateral) account
      const flexAccount = accounts.flex || accounts.multiCollateralMarginAccount;
      if (flexAccount && flexAccount.type === 'multiCollateralMarginAccount') {
        // Flex account structure from actual API response:
        // - currencies: object with each currency details
        // - portfolioValue: total portfolio value
        // - availableMargin: available margin for trading
        // - balanceValue: total balance value
        // - pnl: profit and loss
        // - initialMargin, maintenanceMargin: margin requirements

        const portfolioValue = parseFloat(flexAccount.portfolioValue || 0);
        const availableMargin = parseFloat(flexAccount.availableMargin || 0);
        const balanceValue = parseFloat(flexAccount.balanceValue || 0);
        const pnl = parseFloat(flexAccount.pnl || 0);
        const initialMargin = parseFloat(flexAccount.initialMargin || 0);
        const maintenanceMargin = parseFloat(flexAccount.maintenanceMargin || 0);

        // Parse individual currencies in flex account
        if (flexAccount.currencies) {
          for (const [currency, details] of Object.entries(flexAccount.currencies)) {
            if (typeof details === 'object' && details !== null) {
              const currDetails = details as any;
              const quantity = parseFloat(currDetails.quantity || 0);
              const value = parseFloat(currDetails.value || 0);
              const available = parseFloat(currDetails.available || 0);
              const collateral = parseFloat(currDetails.collateral || 0);

              if (quantity > 0 || value > 0) {
                currencies[currency.toUpperCase()] = {
                  free: available,
                  used: quantity - available,
                  total: quantity,
                  upnl: 0, // PnL is at account level, not per currency
                  value: value,
                  collateral: collateral,
                  info: currDetails,
                };
              }
            }
          }
        }

        // Set overall balance based on portfolio value
        if (portfolioValue > 0) {
          const used = initialMargin + maintenanceMargin;
          const free = Math.max(0, availableMargin);

          // Create/update USD summary entry with actual available margin
          // This is read by the margin slider via getBalanceSlice -> currencies["USD"].free
          currencies['USD'] = {
            ...currencies['USD'],
            free: free,
            used: used,
            total: portfolioValue,
            upnl: pnl,
            info: currencies['USD']?.info || {},
          };

          // The main balance should reflect the portfolio value
          totalBalance = portfolioValue;
          freeBalance = free;
          usedBalance = used;
          totalUpnl = pnl;
        } else if (balanceValue > 0) {
          // Fallback to balance value if portfolio value is 0
          const free = Math.max(0, availableMargin);
          const used = initialMargin + maintenanceMargin;

          currencies['USD'] = {
            ...currencies['USD'],
            free: free,
            used: used,
            total: balanceValue,
            upnl: pnl,
            info: currencies['USD']?.info || {},
          };

          totalBalance = balanceValue;
          freeBalance = free;
          usedBalance = used;
          totalUpnl = pnl;
        }
      }

      // Process individual margin accounts (fi_xbtusd, fi_ethusd, etc.)
      for (const [accountName, account] of Object.entries(accounts)) {
        if (accountName.startsWith('fi_') && typeof account === 'object') {
          const aux = (account as any).auxiliary || {};
          const pv = parseFloat(aux.pv || 0);
          const af = parseFloat(aux.af || 0);
          const pnl = parseFloat(aux.pnl || 0);

          if (pv > 0) {
            // These are usually denominated in the base currency of the market
            const _symbol = accountName.replace('fi_', '').toUpperCase();
            const currency = aux.currency || 'USD';

            if (!currencies[currency]) {
              currencies[currency] = {
                free: 0,
                used: 0,
                total: 0,
                upnl: 0,
                info: {},
              };
            }

            currencies[currency].total += pv;
            currencies[currency].free += af;
            currencies[currency].upnl += pnl;

            // Add to totals if USD-based
            if (currency === 'USD') {
              totalBalance += pv;
              freeBalance += af;
              totalUpnl += pnl;
            }
          }
        }
      }

      // Fallback: if no balances found, check for any auxiliary data
      if (totalBalance === 0) {
        for (const [_accountName, account] of Object.entries(accounts)) {
          if (typeof account === 'object' && (account as any).auxiliary) {
            const aux = (account as any).auxiliary;
            const pv = parseFloat(aux.pv || aux.portfolioValue || 0);
            const af = parseFloat(aux.af || aux.availableMargin || 0);

            if (pv > 0) {
              totalBalance = pv;
              freeBalance = af;
              usedBalance = pv - af;
              totalUpnl = parseFloat(aux.pnl || 0);

              currencies['USD'] = {
                free: freeBalance,
                used: usedBalance,
                total: totalBalance,
                upnl: totalUpnl,
                info: account,
              };
              break;
            }
          }
        }
      }

      return {
        free: freeBalance,
        used: usedBalance,
        total: totalBalance,
        upnl: totalUpnl,
        currencies,
        info: data?.accounts,
      };
    } catch (err: any) {
      this.emitAndLogError(err?.message || 'Failed to fetch balance');
      return this.store.balance;
    }
  };

  fetchLeverageSettings = async (): Promise<void> => {
    try {
      const { data } = await this.xhr.get<{
        result: string;
        leveragePreferences?: Array<{ symbol: string; maxLeverage: number }>;
      }>(ENDPOINTS.LEVERAGE_PREFERENCES);

      if (data?.result === 'success' && data?.leveragePreferences) {
        // Clear existing settings
        this.leverageSettings.clear();

        // Store leverage settings for each symbol
        for (const pref of data.leveragePreferences) {
          if (pref.symbol && pref.maxLeverage) {
            this.leverageSettings.set(pref.symbol, pref.maxLeverage);
          }
        }
      }
    } catch (err: any) {
      // If fetching leverage settings fails, just log and continue
      // Positions will use default leverage value
      console.error('[Kraken] Failed to fetch leverage settings:', err?.message);
    }
  };

  setLeverage = async (symbol: string, inputLeverage: number, _side?: PositionSide): Promise<void> => {
    try {
      // Find the market to get leverage limits
      const market = this.store.marketsBySymbol[symbol];
      if (!market) {
        throw new Error(`Market ${symbol} not found`);
      }

      // Clamp leverage to valid range
      const leverage = Math.min(Math.max(inputLeverage, market.limits.leverage.min), market.limits.leverage.max);

      // Set leverage using PUT /v3/leveragepreferences
      // Note: Kraken requires uppercase symbol
      const { data } = await this.xhr.put<{
        result: string;
        status?: string;
        errors?: Array<{ code: number; message: string }>;
      }>(ENDPOINTS.LEVERAGE_PREFERENCES, {
        symbol: symbol.toUpperCase(),
        maxLeverage: leverage,
      });

      // Check for error response
      if (data?.result === 'error' || data?.status === 'BAD_REQUEST') {
        const errorMessage = data?.errors?.[0]?.message || 'Failed to set leverage';
        throw new Error(errorMessage);
      }

      if (data?.result !== 'success') {
        throw new Error('Failed to set leverage');
      }

      // Update local cache
      this.leverageSettings.set(symbol, leverage);

      this.log(`Leverage set to ${leverage}x for ${symbol}`, LogSeverity.Info);
    } catch (err: any) {
      // Extract error message from Kraken's error response format
      const errorData = err?.response?.data;
      let errorMessage = err?.message || 'Failed to set leverage';

      if (errorData?.errors?.[0]?.message) {
        errorMessage = errorData.errors[0].message;
      } else if (errorData?.error) {
        errorMessage = errorData.error;
      }

      this.emitAndLogError(errorMessage, 'SetLeverage Error');
      throw new Error(errorMessage);
    }
  };

  changeMarginType = async (isolated: boolean, symbol?: string): Promise<void> => {
    if (!symbol) {
      this.emitAndLogError('Symbol is required when changing margin type');
      throw new Error('Symbol is required when changing margin type');
    }

    try {
      // Find the market to get leverage limits
      const market = this.store.marketsBySymbol[symbol];
      if (!market) {
        throw new Error(`Market ${symbol} not found`);
      }

      if (isolated) {
        // Set to ISOLATED: Set a maxLeverage preference
        // Use current leverage if available, otherwise use market max
        const currentPosition = this.store.positions.find((p) => p.symbol === symbol);
        const leverage = currentPosition?.leverage ?? market.limits.leverage.max;

        // Set leverage preference to switch to isolated margin
        const { data } = await this.xhr.put<{
          result: string;
          status?: string;
          errors?: Array<{ code: number; message: string }>;
        }>(ENDPOINTS.LEVERAGE_PREFERENCES, {
          symbol: symbol.toUpperCase(),
          maxLeverage: leverage,
        });

        // Check for error response
        if (data?.result === 'error' || data?.status === 'BAD_REQUEST') {
          const errorMessage = data?.errors?.[0]?.message || 'Failed to set margin type to isolated';
          throw new Error(errorMessage);
        }

        if (data?.result !== 'success') {
          throw new Error('Failed to set margin type to isolated');
        }

        // Update local caches
        this.leverageSettings.set(symbol, leverage);
        this.marginTypePreferences.set(symbol, MarginType.Isolated);
        this.log(`Margin type set to ISOLATED (${leverage}x) for ${symbol}`, LogSeverity.Info);
      } else {
        // Set to CROSS: Use PUT without maxLeverage parameter (or with symbol only)
        // Note: According to Kraken API docs, omitting maxLeverage sets cross margin mode
        const { data } = await this.xhr.put<{
          result: string;
          status?: string;
          errors?: Array<{ code: number; message: string }>;
        }>(ENDPOINTS.LEVERAGE_PREFERENCES, {
          symbol: symbol.toUpperCase(),
          // Not sending maxLeverage parameter to indicate cross margin
        });

        // Check for error response
        if (data?.result === 'error' || data?.status === 'BAD_REQUEST') {
          const errorMessage = data?.errors?.[0]?.message || 'Failed to set margin type to cross';
          throw new Error(errorMessage);
        }

        if (data?.result !== 'success') {
          throw new Error('Failed to set margin type to cross');
        }

        // Update local caches to indicate cross margin
        this.leverageSettings.delete(symbol);
        this.marginTypePreferences.set(symbol, MarginType.Cross);
        this.log(`Margin type set to CROSS for ${symbol}`, LogSeverity.Info);
      }

      // Refresh positions to reflect the margin type change
      const positions = await this.fetchPositions();
      this.store.addOrUpdatePositions(positions);
    } catch (err: any) {
      // Extract error message from Kraken's error response format
      const errorData = err?.response?.data;
      let errorMessage = err?.message || 'Failed to change margin type';

      if (errorData?.errors?.[0]?.message) {
        errorMessage = errorData.errors[0].message;
      } else if (errorData?.error) {
        errorMessage = errorData.error;
      }

      this.emitAndLogError(errorMessage, 'ChangeMarginType Error');
      throw new Error(errorMessage);
    }
  };

  validateAccount = async (): Promise<string> => {
    try {
      // Try to fetch account info to validate credentials
      const { data } = await this.xhr.get<{ result: string; error?: string[]; accounts?: any }>(ENDPOINTS.ACCOUNT);

      // Kraken returns result: 'success' on success
      if (data?.result !== 'success') {
        const errorMsg = data?.error?.[0] || data?.result || 'Account validation failed';

        // Check for common error patterns and provide helpful messages
        const lowerError = errorMsg.toLowerCase();
        if (lowerError.includes('invalid key') || lowerError.includes('apikey')) {
          return 'Invalid API key. Please verify your API key is correct and active.';
        }
        if (lowerError.includes('invalid nonce') || lowerError.includes('nonce')) {
          return 'Invalid nonce. Please ensure your system time is synchronized (NTP).';
        }
        if (lowerError.includes('permission') || lowerError.includes('denied')) {
          return 'API key lacks required permissions. Please ensure General API is set to "Full access" (not "Read only") on your Kraken Futures API key.';
        }
        if (lowerError.includes('signature') || lowerError.includes('authentication')) {
          return 'Invalid API secret or authentication failed. Please verify your API secret is correct.';
        }
        if (lowerError.includes('ip') || lowerError.includes('whitelist')) {
          return 'IP address not whitelisted. Please add your IP to the API key whitelist on Kraken.';
        }

        return `Kraken API Error: ${errorMsg}`;
      }

      // Check if the account has the required permissions
      // We need at least read permissions for basic operations
      const accounts = data?.accounts;
      if (!accounts || Object.keys(accounts).length === 0) {
        return 'No accounts found. API key may lack required permissions or account may be empty.';
      }

      // If we got here, credentials are valid
      return '';
    } catch (err: any) {
      const errorMsg =
        err?.response?.data?.error?.[0] ||
        err?.response?.data?.error ||
        err?.response?.data?.message ||
        err?.message ||
        'Failed to validate account';

      // Handle specific HTTP error codes
      if (err?.response?.status === 401) {
        return 'Authentication failed (401). Please check your API key and secret are correct.';
      }
      if (err?.response?.status === 403) {
        return 'Access forbidden (403). Your API key may lack required permissions or your IP address is not whitelisted.';
      }
      if (err?.response?.status === 429) {
        return 'Rate limit exceeded (429). Please wait a moment and try again.';
      }
      if (err?.response?.status === 400) {
        return 'Bad request (400). Please verify your API credentials are in the correct format.';
      }

      // Check error message content for more specific guidance
      const lowerError = errorMsg.toLowerCase();
      if (lowerError.includes('network') || lowerError.includes('timeout')) {
        return 'Network error. Please check your internet connection and try again.';
      }
      if (lowerError.includes('cors')) {
        return 'CORS error. Please enable "Use Tealstreet IPs" option.';
      }

      return `Validation failed: ${errorMsg}`;
    }
  };

  // Periodic sync method - fetches account data on first tick and every 10 seconds
  tick = async (count = 0) => {
    if (!this.isDisposed) {
      try {
        // Fetch on first tick (count === 0) or every 20 ticks
        // With tickInterval of 500ms from exchangeMetadata, 20 ticks = 10 seconds
        if (count === 0 || count % 20 === 0) {
          // Fetch leverage settings first so they're available for position mapping
          await this.fetchLeverageSettings();

          // Always fetch orders, positions, and balance
          // This ensures we stay in sync even if WebSocket misses updates
          await Promise.all([
            this.fetchOrders().then((orders) => {
              // Always update orders regardless of length
              this.store.addOrUpdateOrdersFull(orders);
              this.store.update({ loaded: { ...this.store.loaded, orders: true } });
            }),
            this.fetchPositions().then((positions) => {
              // Always update positions regardless of length
              this.store.addOrUpdatePositionsFull(positions);
              this.store.update({ loaded: { ...this.store.loaded, positions: true } });
            }),
            this.fetchBalance().then((balance) => {
              if (balance) {
                this.store.update({ balance, loaded: { ...this.store.loaded, balance: true } });
              }
            }),
          ]);
        }
      } catch (err: any) {
        this.emitAndLogError(err?.message || 'Tick data fetch failed');
      }

      // Schedule the next tick with incremented count
      // tickInterval from exchangeMetadata is 500ms for Kraken
      if (!this.isDisposed) {
        loop(() => this.tick(count + 1), 500);
      }
    }
  };
}
