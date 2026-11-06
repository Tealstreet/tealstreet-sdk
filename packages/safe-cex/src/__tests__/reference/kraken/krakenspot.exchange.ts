import type { Axios } from 'axios';
import type { Store } from '../../../store/store.interface';

import rateLimit from 'axios-rate-limit';
import { customAlphabet } from 'nanoid';

import {
  Balance,
  Candle,
  ExchangeName,
  ExchangeOptions,
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
  ValidateAccountResult,
} from '../../../types';
import { loop } from '../../../utils/loop';
import { adjust } from '../../../utils/safe-math';
import { BaseExchange } from '../../../exchanges/base';
import { createAPI } from './krakenspot.api';
import {
  ENDPOINTS,
  GRAFANA_ENDPOINTS_EVERY,
  GRAFANA_ENDPOINTS_PERIODIC,
  INTERVAL,
  normalizeAsset,
  ORDER_TYPE,
} from './krakenspot.types';
import { KrakenSpotPrivateWebsocket } from './krakenspot.ws-private';
import { KrakenSpotPublicWebsocket } from './krakenspot.ws-public';

const KRAKEN_SPOT_CL_ORD_ID_LENGTH = 32;
const KRAKEN_SPOT_CL_ORD_ID_PATTERN = /^[a-f0-9]{32}$/;
const createKrakenSpotClientOrderId = customAlphabet('0123456789abcdef', KRAKEN_SPOT_CL_ORD_ID_LENGTH);

export class KrakenSpotExchange extends BaseExchange {
  xhr: Axios;
  unlimitedXHR: Axios;

  isSpot = true;

  declare publicWebsocket?: KrakenSpotPublicWebsocket;
  declare privateWebsocket?: KrakenSpotPrivateWebsocket;

  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;
  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    this.xhr = this.createXhrWithSecondaryKey(opts, createAPI, { rateLimit: (xhr) => rateLimit(xhr, { maxRPS: 5 }) });
    this.unlimitedXHR = createAPI(opts, this);
  }

  start = async () => {
    const useExternalPublicData = await this.initExternalPublicData();

    if (!useExternalPublicData) {
      const markets = await this.fetchMarkets();
      if (this.isDisposed) return;

      this.store.update({ markets, loaded: { ...this.store.loaded, markets: true } });

      const tickers = await this.fetchTickers();
      if (this.isDisposed) return;

      this.store.update({ tickers, loaded: { ...this.store.loaded, tickers: true } });

      // Start public WebSocket for live tickers and orderbook
      this.publicWebsocket = new KrakenSpotPublicWebsocket(this);
      this.publicWebsocket.connectAndSubscribe();
    }

    // Start private WebSocket if credentials are provided
    if (!this.isPublicDataOnly) {
      this.privateWebsocket = new KrakenSpotPrivateWebsocket(this);
      this.privateWebsocket.connectAndSubscribe();
    }

    if (this.isPublicDataOnly) {
      this.store.update({
        loaded: { ...this.store.loaded, balance: true, positions: true, orders: true },
      });
      return;
    }

    // Start tick cycle for periodic data refresh
    this.tick(0);
  };

  fetchMarkets = async (): Promise<Market[]> => {
    try {
      const { data } = await this.xhr.get(ENDPOINTS.ASSET_PAIRS);

      const pairs = data?.result || {};
      const markets: Market[] = [];

      for (const [pairName, info] of Object.entries(pairs)) {
        const p = info as any;

        // Skip darkpool pairs (they end with .d)
        if (pairName.endsWith('.d')) continue;

        // Use wsname for the symbol (e.g., "XBT/USD") — this matches WS v2 format
        const wsname = p.wsname;
        if (!wsname) continue;

        const [rawBase, rawQuote] = wsname.split('/');
        const base = normalizeAsset(rawBase);
        const quote = normalizeAsset(rawQuote);

        // Only include USD, USDT, USDC quoted pairs for now
        if (!['USD', 'USDT', 'USDC'].includes(quote)) continue;

        const pricePrecision = p.pair_decimals != null ? Number(`1e${-p.pair_decimals}`) : 0.01;
        const amountPrecision = p.lot_decimals != null ? Number(`1e${-p.lot_decimals}`) : 0.00000001;
        const minAmount = parseFloat(p.ordermin || '0');

        markets.push({
          id: `${base}/${quote}`,
          symbol: wsname, // Use wsname as symbol — matches WS v2 subscriptions
          prettySymbol: `${base}${quote}`,
          base,
          quote,
          active: p.status === 'online',
          precision: {
            amount: amountPrecision,
            price: pricePrecision,
          },
          limits: {
            amount: { min: minAmount, max: Number.MAX_SAFE_INTEGER },
            leverage: { min: 1, max: 1 }, // Spot = no leverage
          },
          contractSize: 1,
          isSpot: true,
          isInverse: false,
          info: { ...p, pairName }, // Store original pair name for REST API calls
        });
      }

      return markets;
    } catch (err: any) {
      console.trace(err);
      return this.store.markets;
    }
  };

  fetchTickers = async (): Promise<Ticker[]> => {
    try {
      // Fetch all tickers in one call
      const { data } = await this.xhr.get(ENDPOINTS.TICKER);

      const result = data?.result || {};
      const tickers: Ticker[] = [];

      for (const market of this.store.markets) {
        // Kraken REST ticker keys use the REST pair name, not wsname
        // We stored pairName in market.info
        const pairName = market.info?.pairName;
        if (!pairName) continue;

        const t = result[pairName];
        if (!t) continue;

        // Kraken Spot ticker format:
        // a = ask [price, whole_lot_vol, lot_vol]
        // b = bid [price, whole_lot_vol, lot_vol]
        // c = last [price, lot_vol]
        // v = volume [today, 24h]
        // p = vwap [today, 24h]
        // t = trades [today, 24h]
        // l = low [today, 24h]
        // h = high [today, 24h]
        // o = today's opening price
        const ask = parseFloat(t.a?.[0] || '0');
        const bid = parseFloat(t.b?.[0] || '0');
        const last = parseFloat(t.c?.[0] || '0');
        const volume = parseFloat(t.v?.[1] || '0'); // 24h volume
        const open = parseFloat(t.o || '0');

        const percentage = open > 0 ? ((last - open) / open) * 100 : 0;

        tickers.push({
          id: market.id,
          symbol: market.symbol,
          bid,
          ask,
          last,
          mark: last, // Spot has no mark price; use last
          index: last, // Spot has no index price; use last
          percentage,
          fundingRate: 0, // No funding on spot
          fundingTime: 0,
          volume,
          quoteVolume: volume * last,
          openInterest: 0,
          info: t,
        });
      }

      return tickers;
    } catch (err: any) {
      console.trace(err);
      return this.store.tickers;
    }
  };

  fetchOrders = async (): Promise<Order[]> => {
    try {
      const { data } = await this.xhr.post(ENDPOINTS.OPEN_ORDERS, {});

      const openOrders = data?.result?.open || {};
      const orders: Order[] = [];

      // OpenOrders describes pairs with the AssetPairs `altname` (for
      // example XBTUSD), while our canonical market symbol is the WebSocket
      // `wsname` (XBT/USD). Index every documented identity once so XBT pairs
      // reconcile to the same market as public data.
      const marketsByPairIdentity = new Map<string, Market>();
      for (const market of this.store.markets) {
        const identities = [market.symbol, market.info?.pairName, market.info?.altname, market.prettySymbol];
        for (const identity of identities) {
          if (identity) marketsByPairIdentity.set(String(identity), market);
        }
      }

      for (const [txid, orderData] of Object.entries(openOrders)) {
        const o = orderData as any;
        const descr = o.descr || {};

        // Find market by pair description
        const market = marketsByPairIdentity.get(descr.pair);

        const orderType = ORDER_TYPE[descr.ordertype] || OrderType.Limit;
        const side = descr.type === 'buy' ? OrderSide.Buy : OrderSide.Sell;
        const price = parseFloat(descr.price || '0');
        const volume = parseFloat(o.vol || '0');
        const executed = parseFloat(o.vol_exec || '0');

        let timeInForce = OrderTimeInForce.GoodTillCancel;
        if (o.oflags?.includes('post')) {
          timeInForce = OrderTimeInForce.PostOnly;
        }

        orders.push({
          id: txid,
          status: OrderStatus.Open,
          symbol: market?.symbol || descr.pair || '',
          type: orderType,
          side,
          price,
          amount: volume,
          filled: executed,
          remaining: Math.max(0, volume - executed),
          reduceOnly: false, // Spot doesn't have reduce-only
          info: o,
          timeInForce,
          lastUpdate: o.opentm ? o.opentm * 1000 : Date.now(),
        });
      }

      return orders;
    } catch (err: any) {
      console.trace(err);
      return this.store.orders || [];
    }
  };

  fetchAndSetOrders = async (): Promise<Order[]> => {
    const orders = await this.fetchOrders();
    this.store.addOrUpdateOrdersFull(orders);
    this.store.update({ loaded: { ...this.store.loaded, orders: true } });
    return orders;
  };

  placeOrder = async (opts: PlaceOrderOpts): Promise<string[]> => {
    this.resolveCallerClientOrderId(opts);
    try {
      const market = this.store.marketsBySymbol[opts.symbol];
      if (!market) {
        throw new Error(`Market ${opts.symbol} not found`);
      }

      // Kraken Spot AddOrder uses the REST pair name, not wsname
      const pair = market.info?.pairName;
      if (!pair) {
        throw new Error(`No REST pair name for ${opts.symbol}`);
      }

      // Map order type
      let ordertype = 'limit';
      if (opts.type === OrderType.Market) {
        ordertype = 'market';
      } else if (opts.type === OrderType.StopLoss) {
        ordertype = 'stop-loss';
      } else if (opts.type === OrderType.TakeProfit) {
        ordertype = 'take-profit';
      } else if (opts.type === OrderType.TrailingStopLoss) {
        ordertype = 'trailing-stop';
      }

      const volume = adjust(opts.amount, market.precision.amount);
      const clOrdId = createKrakenSpotClientOrderId();

      if (!KRAKEN_SPOT_CL_ORD_ID_PATTERN.test(clOrdId)) {
        throw new Error('Failed to generate a valid Kraken Spot client order ID');
      }

      const body: Record<string, any> = {
        pair,
        type: opts.side === OrderSide.Buy ? 'buy' : 'sell',
        ordertype,
        volume: String(volume),
        cl_ord_id: clOrdId,
      };

      // Add price for limit orders
      if (opts.type === OrderType.Limit && opts.price) {
        body.price = String(adjust(opts.price, market.precision.price));
      }

      // Add price for stop/TP orders (trigger price)
      if ((opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit) && opts.price) {
        body.price = String(adjust(opts.price, market.precision.price));
      }

      if (opts.type === OrderType.TrailingStopLoss) {
        const ticker = this.store.tickersBySymbol[opts.symbol];
        if (!ticker || !Number.isFinite(ticker.last) || ticker.last <= 0) {
          throw new Error(`Ticker ${opts.symbol} is required for a Kraken Spot trailing stop`);
        }
        if (!Number.isFinite(opts.price) || opts.price! <= 0) {
          throw new Error(`Trailing stop price for ${opts.symbol} must be a positive finite value`);
        }

        const deviation = adjust(Math.abs(ticker.last - opts.price!), market.precision.price);
        if (!Number.isFinite(deviation) || deviation <= 0) {
          throw new Error(`Trailing stop price for ${opts.symbol} must differ from the last price`);
        }

        // Kraken Spot requires a relative `+` price for trailing stops and
        // derives direction from the order side.
        body.price = `+${deviation}`;
      }

      // Time in force / order flags
      const oflags: string[] = [];
      if (opts.timeInForce === OrderTimeInForce.PostOnly) {
        oflags.push('post');
      }
      if (opts.timeInForce === OrderTimeInForce.ImmediateOrCancel) {
        body.timeinforce = 'IOC';
      }
      if (oflags.length > 0) {
        body.oflags = oflags.join(',');
      }

      const { data } = await this.unlimitedXHR.post(ENDPOINTS.ADD_ORDER, body);

      const txids = data?.result?.txid || [];

      // Place attached TP/SL as separate orders if present
      const ids: string[] = [...txids];
      if (opts.type === OrderType.Limit && (opts.stopLoss || opts.takeProfit)) {
        const derived = this.deriveAlgoOrdersFromNormalOrdersOpts([opts]);
        const extraIds = await this.placeOrders(derived);
        ids.push(...extraIds);
      }

      return ids;
    } catch (err: any) {
      const errorMessage = err?.message || 'Failed to place order';
      this.emitAndLogError(errorMessage, 'PlaceOrder Error');
      throw new Error(errorMessage);
    }
  };

  updateOrder = async (_opts: UpdateOrderOpts): Promise<string[]> => {
    try {
      const { order, update } = _opts;
      const market = this.store.marketsBySymbol[order.symbol];
      if (!market) {
        throw new Error(`Market ${order.symbol} not found`);
      }

      const rawOrderType = order.info?.descr?.ordertype || order.info?.order_type;
      const expectedOrderType = rawOrderType ? ORDER_TYPE[rawOrderType] : undefined;
      const supportsLimitAmend = rawOrderType === 'limit' && order.type === OrderType.Limit;
      const supportsTriggerAmend =
        (rawOrderType === 'stop-loss' && order.type === OrderType.StopLoss) ||
        (rawOrderType === 'take-profit' && order.type === OrderType.TakeProfit);

      if ((!supportsLimitAmend && !supportsTriggerAmend) || expectedOrderType !== order.type) {
        throw new Error(
          `Kraken Spot cannot safely amend ${rawOrderType || order.type}; only reconciled limit, stop-loss, and take-profit orders are supported`,
        );
      }

      if (supportsLimitAmend && order.timeInForce === OrderTimeInForce.ImmediateOrCancel) {
        throw new Error('Kraken Spot cannot amend an immediate-or-cancel order');
      }

      const body: Record<string, any> = {
        txid: order.id,
      };

      if ((update as any).amount !== undefined) {
        const amount = adjust((update as any).amount, market.precision.amount);
        if (!Number.isFinite(amount) || amount <= 0) {
          throw new Error('Kraken Spot amend quantity must be a positive finite amount');
        }
        body.order_qty = String(amount);
      }
      if ((update as any).price !== undefined) {
        const price = adjust((update as any).price, market.precision.price);
        if (!Number.isFinite(price) || price <= 0) {
          throw new Error('Kraken Spot amend price must be a positive finite amount');
        }

        if (supportsLimitAmend) {
          body.limit_price = String(price);
          if (order.timeInForce === OrderTimeInForce.PostOnly) body.post_only = true;
        } else {
          body.trigger_price = String(price);
        }
      }

      if (body.order_qty === undefined && body.limit_price === undefined && body.trigger_price === undefined) {
        throw new Error('Kraken Spot amend requires an amount or price change');
      }

      const { data } = await this.unlimitedXHR.post(ENDPOINTS.AMEND_ORDER, body);

      const amendId = data?.result?.amend_id;
      if (typeof amendId !== 'string' || amendId.length === 0) {
        throw new Error('Kraken Spot AmendOrder response did not include an amend_id');
      }

      // AmendOrder preserves the order identity. amend_id identifies only the
      // amend transaction and must not replace the canonical Kraken txid.
      return [order.id];
    } catch (err: any) {
      const errorMessage = err?.message || 'Failed to update order';
      this.emitAndLogError(errorMessage, 'UpdateOrder Error');
      throw new Error(errorMessage);
    }
  };

  cancelOrders = async (orders: Order[]): Promise<void> => {
    try {
      if (orders.length === 0) return;

      if (orders.length === 1) {
        await this.unlimitedXHR.post(ENDPOINTS.CANCEL_ORDER, { txid: orders[0].id });
        return;
      }

      // Cancel multiple orders one by one (Kraken Spot has no batch cancel by ID)
      for (const order of orders) {
        await this.unlimitedXHR.post(ENDPOINTS.CANCEL_ORDER, { txid: order.id });
      }
    } catch (err: any) {
      const errorMessage = err?.message || 'Failed to cancel orders';
      this.emitAndLogError(errorMessage, 'CancelOrders Error');
      throw new Error(errorMessage);
    }
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    if (!this.publicWebsocket) {
      this.publicWebsocket = new KrakenSpotPublicWebsocket(this);
      this.publicWebsocket.connectAndSubscribe();
    }
    return this.publicWebsocket.listenOrderBook(symbol, callback);
  };

  fetchOHLCV = async (opts: OHLCVOptions): Promise<Candle[]> => {
    try {
      const market = this.store.marketsBySymbol[opts.symbol];
      const pair = market?.info?.pairName || opts.symbol;

      const interval = INTERVAL[opts.interval as keyof typeof INTERVAL] || 1;

      const params: Record<string, any> = {
        pair,
        interval,
      };

      if (opts.from) {
        params.since = Math.floor(opts.from / 1000);
      }

      const { data } = await this.xhr.get(ENDPOINTS.OHLC, { params });

      const result = data?.result || {};
      // Result has pair key + "last" key; find the array
      const pairKey = Object.keys(result).find((k) => k !== 'last');
      if (!pairKey) return [];

      const rawCandles = result[pairKey] as any[];

      // Kraken OHLC format: [time, open, high, low, close, vwap, volume, count]
      const candles: Candle[] = rawCandles.map((c: any) => ({
        timestamp: c[0], // Already in seconds
        open: parseFloat(c[1]),
        high: parseFloat(c[2]),
        low: parseFloat(c[3]),
        close: parseFloat(c[4]),
        volume: parseFloat(c[6]),
      }));

      return candles.sort((a, b) => a.timestamp - b.timestamp);
    } catch (err: any) {
      console.error('[KrakenSpot] fetchOHLCV error:', err?.message);
      return [];
    }
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    // Kraken Spot supports OHLC via WebSocket v2
    if (!this.publicWebsocket) {
      this.publicWebsocket = new KrakenSpotPublicWebsocket(this);
      this.publicWebsocket.connectAndSubscribe();
    }
    return this.publicWebsocket.listenOHLCV(opts, callback);
  };

  fetchBalance = async (): Promise<Balance> => {
    try {
      const { data } = await this.xhr.post(ENDPOINTS.BALANCE, {});

      const balances = data?.result || {};
      const currencies: Record<string, any> = {};

      let totalUsd = 0;
      let freeUsd = 0;

      for (const [asset, balanceStr] of Object.entries(balances)) {
        const amount = parseFloat(String(balanceStr || '0'));
        if (amount <= 0) continue;

        // Skip yield-bearing (.B) and rewards (.F) and tokenized (.T) sub-balances
        if (asset.includes('.')) continue;

        const normalized = normalizeAsset(asset);

        currencies[normalized] = {
          free: amount,
          used: 0,
          total: amount,
          upnl: 0,
          info: { raw: asset, balance: balanceStr },
        };

        // Sum USD-equivalent balances
        if (['USD', 'USDT', 'USDC'].includes(normalized)) {
          totalUsd += amount;
          freeUsd += amount;
        }
      }

      // For non-USD assets, estimate USD value from tickers
      for (const [asset, info] of Object.entries(currencies)) {
        if (['USD', 'USDT', 'USDC'].includes(asset)) continue;

        const market = this.store.markets.find((market) => market.base === asset && ['USD', 'USDT'].includes(market.quote));
        const ticker = market ? this.store.tickersBySymbol[market.symbol] : undefined;

        if (ticker && ticker.last > 0) {
          const usdValue = info.total * ticker.last;
          totalUsd += usdValue;
          freeUsd += usdValue;
        }
      }

      return {
        free: freeUsd,
        used: 0,
        total: totalUsd,
        upnl: 0,
        currencies,
        info: balances,
      };
    } catch (err: any) {
      this.emitAndLogError(err?.message || 'Failed to fetch balance');
      return this.store.balance;
    }
  };

  refreshBalance = async () => {
    try {
      const balance = await this.fetchBalance();
      this.store.update({ balance, loaded: { ...this.store.loaded, balance: true } });
    } catch (_e) {
      // Will retry on next tick
    }
  };

  // Spot doesn't have positions — create fake zero-position entries for UI compatibility
  fetchPositions = async (): Promise<Position[]> => {
    const positions: Position[] = [];

    for (const market of this.store.markets) {
      if (!market.active) continue;

      positions.push({
        id: `${market.symbol}:long`,
        symbol: market.symbol,
        side: PositionSide.Long,
        entryPrice: 0,
        notional: 0,
        leverage: 1,
        unrealizedPnl: 0,
        contracts: 0,
        liquidationPrice: 0,
        marginType: 'isolated' as any,
        positionMode: PositionMode.OneWay,
        info: {},
      });

      positions.push({
        id: `${market.symbol}:short`,
        symbol: market.symbol,
        side: PositionSide.Short,
        entryPrice: 0,
        notional: 0,
        leverage: 1,
        unrealizedPnl: 0,
        contracts: 0,
        liquidationPrice: 0,
        marginType: 'isolated' as any,
        positionMode: PositionMode.OneWay,
        info: {},
      });
    }

    return positions;
  };

  // Fetch WS auth token via REST (used by private WebSocket)
  fetchWsToken = async (): Promise<string> => {
    const { data } = await this.xhr.post(ENDPOINTS.WS_TOKEN, {});
    const token = data?.result?.token;
    if (!token) {
      throw new Error('Failed to get WebSocket token');
    }
    return token;
  };

  validateAccount = async (): Promise<ValidateAccountResult> => {
    try {
      const { data } = await this.xhr.post(ENDPOINTS.BALANCE, {});

      if (data?.error?.length) {
        const errorStr = data.error.join('; ');
        const lower = errorStr.toLowerCase();

        if (lower.includes('invalid key')) {
          return { error: 'Invalid API key. Please verify your API key is correct and active.' };
        }
        if (lower.includes('invalid nonce')) {
          return { error: 'Invalid nonce. Please ensure your system time is synchronized.' };
        }
        if (lower.includes('invalid signature')) {
          return { error: 'Invalid API secret. Please verify your API secret is correct.' };
        }
        if (lower.includes('permission denied')) {
          return { error: 'API key lacks required permissions.' };
        }

        return { error: `Kraken Spot API Error: ${errorStr}` };
      }

      return { error: '', balance: await this.fetchBalance().catch(() => undefined) };
    } catch (err: any) {
      const errorMsg = err?.message || 'Failed to validate account';
      const lower = errorMsg.toLowerCase();

      if (err?.response?.status === 401 || err?.response?.status === 403) {
        return { error: 'Authentication failed. Please check your API key and secret.' };
      }
      if (err?.response?.status === 429) {
        return { error: 'Rate limit exceeded. Please wait and try again.' };
      }
      if (lower.includes('network') || lower.includes('timeout')) {
        return { error: 'Network error. Please check your internet connection.' };
      }

      return { error: `Validation failed: ${errorMsg}` };
    }
  };

  // Periodic data sync
  tick = async (count = 0) => {
    if (!this.isDisposed) {
      try {
        // Fetch on first tick and every 20 ticks (10 seconds at 500ms interval)
        if (count === 0 || count % 20 === 0) {
          await Promise.all([
            this.fetchOrders().then((orders) => {
              this.store.addOrUpdateOrdersFull(orders);
              this.store.update({ loaded: { ...this.store.loaded, orders: true } });
            }),
            this.fetchPositions().then((positions) => {
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

      if (!this.isDisposed) {
        loop(() => this.tick(count + 1), 500);
      }
    }
  };
}
