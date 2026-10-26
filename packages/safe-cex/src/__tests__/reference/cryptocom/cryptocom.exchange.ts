import type { Axios } from 'axios';
import type { Store } from '../../../store/store.interface';
import type {
  CryptocomBalanceSummary,
  CryptocomAccountsResponse,
  CryptocomBook,
  CryptocomCandlestick,
  CryptocomInstrument,
  CryptocomOrder,
  CryptocomOrderRequest,
  CryptocomPosition,
  CryptocomTicker,
} from './cryptocom.types';

import rateLimit from 'axios-rate-limit';
import sumBy from 'lodash/sumBy';
import uniqBy from 'lodash/uniqBy';
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
import { virtualClock } from '../../../utils/virtualClock';
import { BaseExchange } from '../../../exchanges/base';
import { patternClientOrderIdRule } from '../../../exchanges/clientOrderId';
import { createAPI, isCryptocomAuthenticationErrorCode, isCryptocomAuthenticationFailureMessage } from './cryptocom.api';
import { ENDPOINTS, GRAFANA_ENDPOINTS_EVERY, GRAFANA_ENDPOINTS_PERIODIC, mapCandlestickInterval } from './cryptocom.types';
import { CryptocomPrivateWebSocket } from './cryptocom.ws-private';
import { CryptocomPublicWebSocket } from './cryptocom.ws-public';

export class CryptocomExchange extends BaseExchange {
  protected readonly clientOrderIdRule = patternClientOrderIdRule('Crypto.com', /^.{1,36}$/);

  xhr: Axios;

  declare publicWebsocket: CryptocomPublicWebSocket;
  declare privateWebsocket: CryptocomPrivateWebSocket;

  private _suppressCryptocomValidationErrorEmission = false;

  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;
  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;

  get suppressCryptocomValidationErrorEmission(): boolean {
    return this._suppressCryptocomValidationErrorEmission;
  }

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    // Rate limit conservatively (100 RPS according to docs, but we'll use 50)
    this.xhr = this.createXhrWithSecondaryKey(opts, createAPI, { rateLimit: (xhr) => rateLimit(xhr, { maxRPS: 50 }) });

    this.publicWebsocket = new CryptocomPublicWebSocket(this);
    this.privateWebsocket = new CryptocomPrivateWebSocket(this);
  }

  dispose = () => {
    this.isDisposed = true;
    this.publicWebsocket.dispose();
    this.privateWebsocket.dispose();
  };

  async getPositionMode(): Promise<PositionMode> {
    // Crypto.com uses hedged mode for perpetuals
    return PositionMode.Hedged;
  }

  getAccount = async () => {
    const { data } = await this.xhr.post<CryptocomAccountsResponse>(ENDPOINTS.ACCOUNTS, {});
    return {
      userId: data.this_account.uuid,
      affiliateId: null,
    };
  };

  validateAccount = async () => {
    const previousSuppressValidationErrorEmission = this._suppressCryptocomValidationErrorEmission;
    this._suppressCryptocomValidationErrorEmission = true;
    try {
      await this.xhr.post(ENDPOINTS.BALANCE, {});
      return { error: '', balance: await this.fetchBalance().catch(() => undefined) };
    } catch (err: any) {
      const errorMessage = err?.response?.data?.message || err?.message || 'Invalid API key or secret';
      const isValidationAuthError =
        isCryptocomAuthenticationErrorCode(err?.response?.data?.code) ||
        isCryptocomAuthenticationFailureMessage(errorMessage);

      if (!isValidationAuthError) {
        this.emitAndLogError(errorMessage);
      }

      return { error: errorMessage };
    } finally {
      this._suppressCryptocomValidationErrorEmission = previousSuppressValidationErrorEmission;
    }
  };

  start = async () => {
    const useExternalPublicData = await this.initExternalPublicData();

    if (!useExternalPublicData) {
      const markets = await this.fetchMarkets();
      if (this.isDisposed) return;

      this.store.update({
        markets,
        loaded: { ...this.store.loaded, markets: true },
      });

      const tickers = await this.fetchTickers();
      if (this.isDisposed) return;

      this.log(`Loaded ${Math.min(tickers.length, markets.length)} Crypto.com markets`);

      this.store.update({
        tickers,
        loaded: { ...this.store.loaded, tickers: true },
      });

      // Connect WebSockets
      this.publicWebsocket.connectAndSubscribe();
    }

    const publicDataOnly = this.isPublicDataOnly;
    if (publicDataOnly) {
      this.store.update({
        loaded: { ...this.store.loaded, balance: true, positions: true, orders: true },
      });
      return;
    }

    this.privateWebsocket.connectAndSubscribe();

    // Enqueue the private channels up-front. The subscribe helpers are
    // idempotent: when called before the WS has authenticated, they record
    // the channel in `subscriptions` and `resubscribe()` flushes them all on
    // the auth ack. Without this, order/position frames never fired and the
    // REST tick (10s) was the only path for live updates.
    this.privateWebsocket.subscribeToOrderUpdates();
    this.privateWebsocket.subscribeToPositionUpdates();
    this.privateWebsocket.subscribeToBalanceUpdates();

    // Wait for WebSocket connections
    await new Promise((resolve) => setTimeout(resolve, 1000));

    if (!useExternalPublicData) {
      // Subscribe to ticker + mark/index/current-funding channels in bounded
      // batches. Order book and trades remain lazy per consumer. The bounded
      // helper also preserves batching when the socket reconnects.
      //
      // Gated on `!useExternalPublicData` because `BaseWebSocket.send()` will
      // auto-connect a public WS when the socket isn't OPEN — calling this
      // without the guard would defeat external public data mode by silently
      // opening a direct exchange WS in the background.
      this.publicWebsocket.subscribeToMarketData(
        this.store.markets.map((market) => ({
          instrumentName: market.id,
          indexInstrumentName: market.info?.underlyingSymbol,
        })),
      );
    }

    await this.tick();
    if (this.isDisposed) return;

    this.log(`Ready to trade on Crypto.com`);

    const orders = await this.fetchOrders();
    if (this.isDisposed) return;

    this.log(`Loaded Crypto.com orders`);

    this.store.update({
      orders,
      loaded: { ...this.store.loaded, orders: true },
    });
  };

  tick = async (count = 0) => {
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

        this.store.update({
          balance,
          loaded: { ...this.store.loaded, balance: true },
        });

        if (count % 4 === 0) {
          const orders = await this.fetchOrders();
          if (this.isDisposed) return;
          this.store.update({ orders, loaded: { ...this.store.loaded, orders: true } });
        }
      } catch (err: any) {
        this.log(`Error in tick: ${err?.message}`, LogSeverity.Error);
      }

      if (!this.isDisposed) {
        setTimeout(() => this.tick(count + 1), 10_000);
      }
    }
  };

  fetchMarkets = async (): Promise<Market[]> => {
    try {
      const { data } = await this.xhr.post<{ data: CryptocomInstrument[] }>(ENDPOINTS.INSTRUMENTS, {});

      return data.data
        .filter((m) => m.inst_type === 'PERPETUAL_SWAP' && m.tradable)
        .map((m) => {
          const qtyTick = parseFloat(m.qty_tick_size);
          const priceTick = parseFloat(m.price_tick_size);
          return {
            id: m.symbol,
            symbol: m.symbol,
            base: m.base_ccy,
            quote: m.quote_ccy,
            active: true,
            type: 'swap',
            spot: false,
            swap: true,
            future: false,
            contract: true,
            linear: true,
            inverse: false,
            contractSize: parseFloat(m.contract_size) || 1,
            info: { underlyingSymbol: m.underlying_symbol },
            precision: {
              amount: qtyTick || Math.pow(10, -m.quantity_decimals),
              price: priceTick || Math.pow(10, -m.quote_decimals),
              base: m.quantity_decimals,
              quote: m.quote_decimals,
            },
            limits: {
              amount: {
                min: qtyTick || 0,
                max: 0,
              },
              price: {
                min: 0,
                max: 0,
              },
              cost: { min: 0, max: 0 },
              leverage: {
                min: 1,
                max: parseFloat(m.max_leverage),
              },
            },
          };
        });
    } catch (err) {
      this.log(`Error fetching markets: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  fetchTickers = async (): Promise<Ticker[]> => {
    try {
      const { data } = await this.xhr.post<{ data: CryptocomTicker[] }>(ENDPOINTS.TICKERS, {});

      return data.data
        .filter((t) => t.i.includes('PERP'))
        .map((t) => ({
          id: `${t.i}-${virtualClock.getCurrentTimeMs()}`,
          symbol: t.i,
          bid: parseFloat(t.b),
          ask: parseFloat(t.k),
          last: parseFloat(t.a),
          mark: 0,
          index: 0,
          percentage: parseFloat(t.c) * 100,
          fundingRate: 0,
          fundingTime: 0,
          volume: parseFloat(t.v),
          quoteVolume: parseFloat(t.vv),
          openInterest: parseFloat(t.oi) || 0,
        }));
    } catch (err) {
      this.log(`Error fetching tickers: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  fetchOHLCV = async (opts: OHLCVOptions): Promise<Candle[]> => {
    try {
      const params: any = {
        instrument_name: opts.symbol,
        timeframe: mapCandlestickInterval(opts.interval),
      };

      if (opts.from) params.start_ts = opts.from;
      if (opts.to) params.end_ts = opts.to;

      const { data } = await this.xhr.post<{ data: CryptocomCandlestick[] }>(ENDPOINTS.CANDLESTICK, params);

      return data.data.map((k) => ({
        timestamp: k.t,
        open: parseFloat(k.o),
        high: parseFloat(k.h),
        low: parseFloat(k.l),
        close: parseFloat(k.c),
        volume: parseFloat(k.v),
      }));
    } catch (err) {
      this.log(`Error fetching OHLCV: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  fetchOrderBook = async (symbol: string): Promise<OrderBook> => {
    try {
      const { data } = await this.xhr.post<CryptocomBook>(ENDPOINTS.BOOK, {
        instrument_name: symbol,
        depth: 50,
      });

      const bookData = data.data?.[0];
      if (!bookData) {
        return { bids: [], asks: [] };
      }

      return {
        bids: bookData.bids.map(([price, quantity]) => ({
          id: price,
          price: parseFloat(price),
          amount: parseFloat(quantity),
          total: 0,
        })),
        asks: bookData.asks.map(([price, quantity]) => ({
          id: price,
          price: parseFloat(price),
          amount: parseFloat(quantity),
          total: 0,
        })),
      };
    } catch (err) {
      this.log(`Error fetching order book: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  fetchOrders = async (): Promise<Order[]> => {
    try {
      // v1 returns `{ result: { data: [...] } }`; interceptor unwraps `result`.
      const [{ data: regular }, { data: advanced }] = await Promise.all([
        this.xhr.post<{ data: CryptocomOrder[] }>(ENDPOINTS.OPEN_ORDERS, {}),
        this.xhr.post<{ data: CryptocomOrder[] }>(ENDPOINTS.ADVANCED_OPEN_ORDERS, {}),
      ]);
      return [...(regular.data || []), ...(advanced.data || [])].map((o) => this.mapOrder(o));
    } catch (err) {
      this.log(`Error fetching orders: ${err}`, LogSeverity.Error);
      return [];
    }
  };

  fetchPositions = async (): Promise<Position[]> => {
    try {
      // v1 returns `{ result: { data: [...] } }`; interceptor unwraps `result`.
      // Old code read `data.position_list` which is undefined on v1 and silently
      // returned no positions even when the account had open exposure.
      const { data } = await this.xhr.post<{ data: CryptocomPosition[] }>(ENDPOINTS.POSITIONS, {});

      const positionMode = await this.getPositionMode();

      // v1 encodes side in the sign of quantity (negative = short). The legacy
      // `type: LONG_POSITION | SHORT_POSITION` field is gone — every row now
      // reports `type: "PERPETUAL_SWAP"`.
      const actualPositions: Position[] = (data.data || [])
        .map((p) => {
          const quantity = parseFloat(p.quantity);
          const cost = parseFloat(p.cost);
          return { p, quantity, cost };
        })
        .filter(({ quantity }) => quantity !== 0)
        .map(({ p, quantity, cost }) => {
          const side = quantity > 0 ? PositionSide.Long : PositionSide.Short;
          const contracts = Math.abs(quantity);
          const entryPrice = cost / contracts || 0;

          return {
            id: createPositionId(p.instrument_name, side, positionMode),
            symbol: p.instrument_name,
            side,
            entryPrice,
            notional: Math.abs(cost),
            // The current REST get-positions schema does not expose leverage
            // or liquidation price; live user.positions frames enrich these.
            leverage: 1,
            unrealizedPnl: parseFloat(p.open_position_pnl) || 0,
            contracts,
            liquidationPrice: 0,
            marginType: p.isolation_type === 'ISOLATED_MARGIN' ? MarginType.Isolated : MarginType.Cross,
            positionMode,
            info: { isolationId: p.isolation_id },
            lastUpdate: p.update_timestamp_ms || virtualClock.getCurrentTimeMs(),
          };
        });

      // Create fake positions for all perpetual markets
      const existingSymbols = new Set(actualPositions.map((p) => p.symbol));
      const fakePositions: Position[] = [];

      for (const market of this.store.markets) {
        if (market.active && !market.isSpot && !existingSymbols.has(market.symbol)) {
          // Create both long and short fake positions (hedged mode)
          fakePositions.push(
            {
              id: createPositionId(market.symbol, PositionSide.Long, positionMode),
              symbol: market.symbol,
              side: PositionSide.Long,
              entryPrice: 0,
              notional: 0,
              leverage: 1,
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
              leverage: 1,
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

      return uniqBy([...actualPositions, ...fakePositions], 'id');
    } catch (err) {
      this.log(`Error fetching positions: ${err}`, LogSeverity.Error);
      return [];
    }
  };

  placeOrder = async (opts: PlaceOrderOpts): Promise<string[]> => {
    const callerClientOrderId = this.resolveCallerClientOrderId(opts);
    try {
      const market = this.store.marketsBySymbol[opts.symbol];
      if (!market) {
        throw new Error(`Market not found: ${opts.symbol}`);
      }

      const orderRequest: CryptocomOrderRequest = {
        instrument_name: market.id,
        side: opts.side === OrderSide.Buy ? 'BUY' : 'SELL',
        type: this.mapOrderType(opts.type),
        ...(callerClientOrderId !== undefined && { client_oid: callerClientOrderId }),
      };
      const isTrigger = opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit;

      // Quantity is valid for both MARKET sides. `notional` is a MARKET BUY
      // by-cost alternative only and cannot safely represent SELL/close flows.
      orderRequest.quantity = opts.amount.toString();
      if (!isTrigger && opts.type !== OrderType.Market && opts.price) {
        orderRequest.price = opts.price.toString();
      }

      if (isTrigger) {
        const triggerPrice = opts.triggerPrice ?? opts.price;
        if (!Number.isFinite(triggerPrice) || triggerPrice! <= 0) {
          throw new Error(`A positive trigger price is required for ${opts.type}`);
        }
        orderRequest.ref_price = triggerPrice!.toString();
        orderRequest.ref_price_type = this.mapStopTriggerType(
          opts.type === OrderType.StopLoss ? opts.slTriggerType : opts.tpTriggerType,
        );
      }

      if (opts.reduceOnly) orderRequest.exec_inst = ['REDUCE_ONLY'];

      if (opts.timeInForce && opts.type === OrderType.Limit) {
        orderRequest.time_in_force = this.mapTimeInForce(opts.timeInForce);
      }

      const endpoint = isTrigger ? ENDPOINTS.ADVANCED_CREATE_ORDER : ENDPOINTS.CREATE_ORDER;
      const { data } = await this.xhr.post<{ order_id: string }>(endpoint, orderRequest);

      return [data.order_id];
    } catch (err: any) {
      this.log(`Error placing order: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  cancelOrder = async (order: Order): Promise<void> => {
    try {
      if (order.type === OrderType.StopLoss || order.type === OrderType.TakeProfit) {
        await this.xhr.post(ENDPOINTS.ADVANCED_CANCEL_ORDER, { order_id: order.id });
      } else {
        await this.xhr.post(ENDPOINTS.CANCEL_ORDER, {
          instrument_name: order.symbol,
          order_id: order.id,
        });
      }
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
      await this.xhr.post(ENDPOINTS.CANCEL_ALL_ORDERS, {
        instrument_name: symbol,
      });
    } catch (err) {
      this.log(`Error canceling symbol orders: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  updateOrder = async (opts: UpdateOrderOpts): Promise<string[]> => {
    try {
      const isTrigger = opts.order.type === OrderType.StopLoss || opts.order.type === OrderType.TakeProfit;
      const updateParams = {
        order_id: opts.order.id,
        new_quantity: ('amount' in opts.update ? opts.update.amount : opts.order.amount).toString(),
        ...(isTrigger
          ? { new_ref_price: ('price' in opts.update ? opts.update.price : opts.order.price).toString() }
          : {
              instrument_name: opts.order.symbol,
              new_price: ('price' in opts.update ? opts.update.price : opts.order.price).toString(),
            }),
      };

      const endpoint = isTrigger ? ENDPOINTS.ADVANCED_AMEND_ORDER : ENDPOINTS.AMEND_ORDER;
      const { data } = await this.xhr.post<{ order_id: string }>(endpoint, updateParams);

      return [data.order_id];
    } catch (err: any) {
      this.log(`Error updating order: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  setLeverage = async (symbol: string, leverage: number, _side?: PositionSide): Promise<void> => {
    try {
      await this.xhr.post(ENDPOINTS.CHANGE_LEVERAGE, {
        instrument_name: symbol,
        leverage: leverage.toString(),
      });
    } catch (err) {
      this.log(`Error setting leverage: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  cancelAllOrders = async (): Promise<void> => {
    try {
      await this.xhr.post(ENDPOINTS.CANCEL_ALL_ORDERS, {});
    } catch (err) {
      this.log(`Error canceling all orders: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  changePositionMode = async (_hedged: boolean): Promise<void> => {
    // Crypto.com uses hedged mode by default
    this.log('Position mode is always hedged on Crypto.com', LogSeverity.Warning);
  };

  changeMarginType = async (isolated: boolean, symbol?: string): Promise<void> => {
    throw new Error(
      `Crypto.com does not support switching ${symbol ?? 'a position'} to ${isolated ? 'isolated' : 'cross'} margin through Exchange v1`,
    );
  };

  placeOrders = async (orders: PlaceOrderOpts[]): Promise<string[]> => {
    this.assertCallerClientOrderIds(orders);
    const orderIds: string[] = [];

    for (const order of orders) {
      try {
        const ids = await this.placeOrder(order);
        orderIds.push(...ids);
      } catch (err) {
        this.log(`Error placing order in batch: ${err}`, LogSeverity.Error);
      }
    }

    return orderIds;
  };

  fetchBalance = async (): Promise<Balance> => {
    try {
      // v1 `private/user-balance` returns `{ result: { data: [summary] } }`.
      // The response interceptor unwraps `result`, so we see `{ data: [summary] }`.
      // The old shape we used to consume (`{ position_balances: [...] }` at the
      // top level) silently produced an empty list and zeroed the balance.
      const { data } = await this.xhr.post<{ data: CryptocomBalanceSummary[] }>(ENDPOINTS.BALANCE, {});
      const summary = data.data?.[0];

      const balances: { [asset: string]: { available: number; locked: number } } = {};
      for (const pb of summary?.position_balances || []) {
        const quantity = parseFloat(pb.quantity);
        const free = parseFloat(pb.max_withdrawal_balance);
        balances[pb.instrument_name] = {
          available: free,
          locked: Math.max(0, quantity - free),
        };
      }

      if (!summary) {
        return { total: 0, free: 0, used: 0, upnl: 0, currencies: balances as any };
      }

      return {
        total: parseFloat(summary.total_margin_balance) || 0,
        free: parseFloat(summary.total_available_balance) || 0,
        used: parseFloat(summary.total_initial_margin) || 0,
        // tick() overwrites this with sumBy(positions, 'unrealizedPnl'); the
        // exchange-reported session uPnL is a fallback when the position
        // fetch hasn't filled in yet.
        upnl: parseFloat(summary.total_session_unrealized_pnl) || 0,
        currencies: balances as any,
      };
    } catch (err) {
      this.log(`Error fetching balance: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    return this.publicWebsocket.listenOHLCV(opts, callback);
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    return this.publicWebsocket.listenOrderBook(symbol, callback);
  };

  private mapOrder = (o: CryptocomOrder): Order => {
    // v1 returns numerics as strings (`quantity`, `limit_price`, `cumulative_*`),
    // renamed `order_status` to `status`, and `time_created`/`time_updated` to
    // `create_time`/`update_time`. WS frames may still carry the legacy names,
    // so we read either.
    const quantity = parseFloat(o.quantity) || 0;
    const filled = o.cumulative_quantity ? parseFloat(o.cumulative_quantity) || 0 : 0;
    const price = parseFloat(o.ref_price ?? o.trigger_price ?? o.limit_price ?? '0') || 0;
    const status = o.status ?? o.order_status ?? 'ACTIVE';
    const updated = o.update_time ?? o.time_updated;
    const created = o.create_time ?? o.time_created;
    return {
      id: o.order_id,
      symbol: o.instrument_name,
      type: this.mapCryptocomOrderType(o.order_type),
      side: o.side === 'BUY' ? OrderSide.Buy : OrderSide.Sell,
      price,
      amount: quantity,
      filled,
      remaining: Math.max(0, quantity - filled),
      status: this.mapOrderStatus(status),
      lastUpdate: updated || created || virtualClock.getCurrentTimeMs(),
      reduceOnly: o.exec_inst?.includes('REDUCE_ONLY') ?? false,
      timeInForce: this.mapCryptocomTimeInForce(o.time_in_force),
    };
  };

  private mapOrderStatus = (status: string): OrderStatus => {
    switch (status) {
      case 'ACTIVE':
      case 'PENDING':
        return OrderStatus.Open;
      case 'FILLED':
        return OrderStatus.Closed;
      case 'CANCELED':
      case 'EXPIRED':
      case 'REJECTED':
        return OrderStatus.Canceled;
      default:
        return OrderStatus.Open;
    }
  };

  private mapOrderType = (
    type: OrderType,
  ): 'LIMIT' | 'MARKET' | 'STOP_LOSS' | 'STOP_LIMIT' | 'TAKE_PROFIT' | 'TAKE_PROFIT_LIMIT' => {
    switch (type) {
      case OrderType.Market:
        return 'MARKET';
      case OrderType.Limit:
        return 'LIMIT';
      case OrderType.StopLoss:
        return 'STOP_LOSS';
      case OrderType.TakeProfit:
        return 'TAKE_PROFIT';
      default:
        return 'LIMIT';
    }
  };

  private mapCryptocomOrderType = (type: string): OrderType => {
    switch (type) {
      case 'MARKET':
        return OrderType.Market;
      case 'LIMIT':
        return OrderType.Limit;
      case 'STOP_LOSS':
      case 'STOP_LIMIT':
        return OrderType.StopLoss;
      case 'TAKE_PROFIT':
      case 'TAKE_PROFIT_LIMIT':
        return OrderType.TakeProfit;
      default:
        return OrderType.Limit;
    }
  };

  private mapTimeInForce = (
    tif?: OrderTimeInForce,
  ): 'GOOD_TILL_CANCEL' | 'FILL_OR_KILL' | 'IMMEDIATE_OR_CANCEL' | 'POST_ONLY' => {
    switch (tif) {
      case OrderTimeInForce.ImmediateOrCancel:
        return 'IMMEDIATE_OR_CANCEL';
      case OrderTimeInForce.FillOrKill:
        return 'FILL_OR_KILL';
      case OrderTimeInForce.PostOnly:
        return 'POST_ONLY';
      default:
        return 'GOOD_TILL_CANCEL';
    }
  };

  mapStopTriggerType = (type?: OrderStopTriggerType): 'MARK_PRICE' | 'INDEX_PRICE' | 'LAST_PRICE' => {
    switch (type) {
      case OrderStopTriggerType.IndexPrice:
        return 'INDEX_PRICE';
      case OrderStopTriggerType.LastPrice:
        return 'LAST_PRICE';
      default:
        return 'MARK_PRICE';
    }
  };

  private mapCryptocomTimeInForce = (tif: string): OrderTimeInForce => {
    switch (tif) {
      case 'IMMEDIATE_OR_CANCEL':
        return OrderTimeInForce.ImmediateOrCancel;
      case 'FILL_OR_KILL':
        return OrderTimeInForce.FillOrKill;
      case 'POST_ONLY':
        return OrderTimeInForce.PostOnly;
      default:
        return OrderTimeInForce.GoodTillCancel;
    }
  };
}
