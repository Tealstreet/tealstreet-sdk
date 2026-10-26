import type { Axios } from 'axios';
import type { ManipulateType } from 'dayjs';
import type { Store } from '../../../store/store.interface';

import rateLimit from 'axios-rate-limit';
import dayjs from 'dayjs';
import flatten from 'lodash/flatten';
import partition from 'lodash/partition';
import uniqBy from 'lodash/uniqBy';
import { mapSeries } from 'p-iteration';

import {
  Balance,
  Candle,
  ExchangeName,
  ExchangeOptions,
  MarginType,
  Market,
  OHLCVOptions,
  Order,
  OrderBook,
  OrderSide,
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
import { omitUndefined } from '../../../utils/omitUndefined';
import { adjust, divide, multiply, subtract } from '../../../utils/safe-math';
import { toBinance } from '../../../utils/symbolConverter';
import { uuid } from '../../../utils/uuid';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseExchange } from '../../../exchanges/base';
import { patternClientOrderIdRule } from '../../../exchanges/clientOrderId';
import { createAPI } from './kucoin.api';
import {
  ENDPOINTS,
  GRAFANA_ENDPOINTS_EVERY,
  GRAFANA_ENDPOINTS_PERIODIC,
  INTERVAL,
  ORDER_STATUS,
  ORDER_TYPE,
  TIME_IN_FORCE,
} from './kucoin.types';
import { KucoinPrivateWebsocket } from './kucoin.ws-private';
import { createKucoinSnapshotWebsockets, KucoinPublicWebsocket } from './kucoin.ws-public';

export class KucoinExchange extends BaseExchange {
  protected readonly clientOrderIdRule = patternClientOrderIdRule('KuCoin', /^[A-Za-z0-9_-]{1,40}$/);

  xhr: Axios;
  unlimitedXHR: Axios;

  // 'detail' socket: orderbook + candles + the viewed symbol's bid/ask + mark.
  publicWebsocket: KucoinPublicWebsocket;
  // 'snapshot' sockets: bulk last/24h%/funding/volume sharded across the ~600
  // markets, since KuCoin caps each connection at 400 topic subscriptions.
  snapshotWebsockets: KucoinPublicWebsocket[] = [];
  privateWebsocket: KucoinPrivateWebsocket;

  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;
  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;

  // private leverageHash: Record<string, number> = {};

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    this.xhr = this.createXhrWithSecondaryKey(opts, createAPI, { rateLimit: (xhr) => rateLimit(xhr, { maxRPS: 3 }) });
    this.unlimitedXHR = createAPI(opts, this);

    this.publicWebsocket = new KucoinPublicWebsocket(this, { role: 'detail' });
    this.privateWebsocket = new KucoinPrivateWebsocket(this);
  }

  get apiMarginCoin() {
    return this.options.testnet ? 'SUSDT' : 'USDT';
  }

  toKucoinSymbol = (symbol?: string) => {
    if (!symbol) return '';
    return symbol;
  };

  toInternalSymbol = (symbol?: string) => {
    const kucoinSymbol = this.toKucoinSymbol(symbol);
    return kucoinSymbol ? toBinance(kucoinSymbol, 'kucoin') : '';
  };

  toApiSymbol = (symbol?: string) => {
    if (!symbol) return '';
    return this.store.marketsBySymbol[symbol]?.id || this.toKucoinSymbol(symbol);
  };

  // KuCoin Futures reports sizes as integer CONTRACT counts; 1 contract =
  // market.contractSize base units (e.g. XBTUSDTM = 0.001 BTC). The rest of the
  // app expects position/order sizes in BASE units and `notional` in quote
  // (USD), matching OKX/BitMEX/BloFin. Convert native↔base at the store
  // boundary with this helper. Note: order PLACEMENT still takes native
  // contracts (opts.amount, same as OKX) — only store OUTPUTS are base.
  private contractSizeFor = (internalSymbol?: string) =>
    (internalSymbol && this.store.marketsBySymbol[internalSymbol]?.contractSize) || 1;

  // Cancel-replace starts from an Order read from the store, whose amount is
  // base-sized. KuCoin placement takes native contract counts. Prefer the raw
  // native remainder carried by mapped orders, with a base->native fallback
  // for callers/orders that predate that metadata.
  private nativeAmountForCancelReplace = (order: Order) =>
    order.extra?.remainingNative ?? divide(order.remaining, this.contractSizeFor(order.symbol));

  private parseMarginType = (marginMode?: unknown, crossMode?: unknown) => {
    const mode = String(marginMode || '').toUpperCase();
    if (mode === 'ISOLATED' || mode === 'FIXED') return MarginType.Isolated;
    if (mode === 'CROSS' || mode === 'CROSSED') return MarginType.Cross;
    if (crossMode === false) return MarginType.Isolated;
    return MarginType.Cross;
  };

  private parsePositionMode = (positionMode?: unknown) => {
    const mode = String(positionMode || '').toUpperCase();
    return mode === '1' || mode === 'HEDGE_MODE' || mode === 'HEDGE' || mode === 'DOUBLE_HOLD'
      ? PositionMode.Hedged
      : PositionMode.OneWay;
  };

  private parseRowPositionMode = (row: Record<string, any>) => {
    if (row.positionSide && row.positionSide !== 'BOTH') return PositionMode.Hedged;
    return this.parsePositionMode(row.positionMode || row.holdMode);
  };

  getAccount = async () => {
    try {
      const { data } = await this.xhr.get('/api/v1/user-info');

      if (data.code !== '200000') {
        this.emitAndLogError(data.msg || 'Failed to get account info');
        return { userId: '', subId: null, affiliateId: null };
      }

      return {
        userId: data.data.userId || '',
        subId: data.data.subUserId || null,
        affiliateId: data.data.referrerUserId || null,
      };
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return { userId: '', subId: null, affiliateId: null };
    }
  };

  validateAccount = async () => {
    try {
      // Try to get account information to validate access
      const { data } = await this.xhr.get('/api/v1/account-overview');

      if (data.code !== '200000') {
        const errorMessage = data.msg || 'Failed to validate account';
        this.emitAndLogError(errorMessage);
        return { error: errorMessage };
      }

      return { error: '', balance: await this.fetchBalance().catch(() => undefined) }; // valid + best-effort balance
    } catch (err: any) {
      const errorMessage = err?.response?.data?.msg || err?.message || 'Unknown error validating account';
      this.emitAndLogError(errorMessage);
      return { error: errorMessage };
    }
  };

  dispose = () => {
    super.dispose();
    this.publicWebsocket.dispose();
    this.snapshotWebsockets.forEach((ws) => ws.dispose());
    this.snapshotWebsockets = [];
    this.privateWebsocket.dispose();
  };

  start = async () => {
    try {
      const useExternalPublicData = await this.initExternalPublicData();

      if (!useExternalPublicData) {
        // Fetch and load markets
        const markets = await this.fetchMarkets();
        if (this.isDisposed) return;

        this.store.update({
          markets,
          loaded: { ...this.store.loaded, markets: true },
        });

        this.log(`Loaded ${markets.length} KuCoin Futures markets`);

        // Fetch ticker data
        const tickers = await this.fetchTickers();
        if (this.isDisposed) return;

        this.store.update({
          tickers,
          loaded: { ...this.store.loaded, tickers: true },
        });

        this.log(`Loaded ${tickers.length} KuCoin Futures tickers`);

        // Connect to WebSocket streams for real-time data. The detail socket
        // handles orderbook/candles/active-ticker; the snapshot pool bulk-feeds
        // last/24h%/funding/volume for every market, sharded under the 400-topic
        // per-connection cap.
        this.publicWebsocket.connectAndSubscribe();
        this.snapshotWebsockets = createKucoinSnapshotWebsockets(this, this.store.markets.length);
        this.snapshotWebsockets.forEach((ws) => ws.connectAndSubscribe());
      }

      const publicDataOnly = this.isPublicDataOnly;
      if (publicDataOnly) {
        this.store.update({
          loaded: { ...this.store.loaded, balance: true, positions: true, orders: true },
        });
        return;
      }

      this.privateWebsocket.connectAndSubscribe();

      // Fetch initial account data
      await this.tick();
      if (this.isDisposed) return;

      // Load any existing orders
      const orders = await this.fetchOrders();
      if (this.isDisposed) return;

      this.store.update({
        orders,
        loaded: { ...this.store.loaded, orders: true },
      });

      this.log(`Loaded ${orders.length} KuCoin Futures orders`);
      this.log(`Ready to trade on KuCoin Futures`);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message || 'Failed to start KuCoin exchange');
    }
  };

  tick = async (count = 0) => {
    if (!this.isDisposed) {
      if (this.isPublicDataOnly) return;

      try {
        const positions = await this.fetchPositions();
        if (this.isDisposed) return;
        this.store.addOrUpdatePositionsFull(positions);

        let balance;
        if (count % 4 === 0) {
          balance = await this.fetchBalance();
          if (this.isDisposed) return;
        }

        // virtual TPSL orders on limits dont stream via ws
        // bitget caches rest responses for a few seconds so best to not overwrite limit orders here as
        // ws updates are newer...
        // let orders;
        // if (count % 8 === 0) {
        // const fetchedNormalOrders = await this.fetchNormalOrders();
        // const virtualTPSLOrders = fetchedNormalOrders.filter(
        //   (o) => o.type === OrderType.StopLoss || o.type === OrderType.TakeProfit,
        // );
        // const existingNonVirtualTPSLOrders = this.store.orders.filter(
        //   (o) => !o.id.endsWith('__stop_loss') && !o.id.endsWith('__take_profit'),
        // );
        // orders = [...existingNonVirtualTPSLOrders, ...virtualTPSLOrders];

        const orders = await this.fetchOrders();
        if (this.isDisposed) return;
        //}

        this.store.update({
          ...(balance && { balance }),
          ...(orders && { orders }),
          loaded: {
            ...this.store.loaded,
            ...(balance && { balance: true }),
            positions: true,
            ...(orders && { orders: true }),
          },
        });
      } catch (err: any) {
        this.emitAndLogError(err?.response?.data?.msg || err?.message);
      }

      if (!this.isDisposed) {
        loop(() => this.tick(count + 1), this.getTickInterval());
      }
    }
  };

  fetchBalance = async () => {
    const { data } = await this.xhr.get(ENDPOINTS.BALANCE, {
      params: { currency: this.apiMarginCoin },
    });

    if (data.code !== '200000') {
      this.emitAndLogError(data.msg || 'Failed to fetch balance');
      return this.store.balance;
    }

    const accountData = data.data;
    if (!accountData) return this.store.balance;

    const balance: Balance = {
      free: parseFloat(accountData.availableBalance || '0'),
      used: parseFloat(accountData.orderMargin || '0'),
      total: parseFloat(accountData.accountEquity || '0'),
      upnl: parseFloat(accountData.unrealisedPNL || '0'),
    };

    return balance;
  };

  fetchPositionMetadata = async (
    symbols: string[],
  ): Promise<Pick<Position, 'symbol' | 'side' | 'leverage' | 'positionMode' | 'marginType'>[]> => {
    try {
      return flatten(
        await mapSeries(
          symbols,
          async (symbol): Promise<Pick<Position, 'symbol' | 'side' | 'leverage' | 'positionMode' | 'marginType'>[]> => {
            if (this.isDisposed) return [];

            const market = this.store.marketsBySymbol[symbol];
            if (!market) return [];

            const [{ data: marginModeData }, { data: positionModeData }] = await Promise.all([
              this.xhr.get(ENDPOINTS.GET_MARGIN_MODE, { params: { symbol: market.id } }),
              this.xhr.get(ENDPOINTS.GET_POSITION_MODE),
            ]);

            if (this.isDisposed) return [];

            if (marginModeData.code !== '200000') {
              this.emitAndLogError(marginModeData.msg || `Failed to fetch margin mode for ${symbol}`);
              return [];
            }

            if (positionModeData.code !== '200000') {
              this.emitAndLogError(positionModeData.msg || 'Failed to fetch position mode');
              return [];
            }

            const positionMode = this.parsePositionMode(positionModeData.data?.positionMode);
            const marginType = this.parseMarginType(marginModeData.data?.marginMode);
            let leverage = 1;
            if (marginType === MarginType.Cross) {
              const { data: leverageData } = await this.xhr.get(ENDPOINTS.POSITION_LEVERAGE, {
                params: { symbol: market.id },
              });
              if (leverageData.code === '200000') {
                leverage = parseFloat(leverageData.data?.leverage || '1');
              }
            }

            if (positionMode === PositionMode.OneWay) {
              return [
                {
                  symbol,
                  side: PositionSide.Long,
                  leverage,
                  positionMode,
                  marginType,
                },
              ];
            } else {
              return [
                {
                  symbol,
                  side: PositionSide.Long,
                  leverage,
                  positionMode,
                  marginType,
                },
                {
                  symbol,
                  side: PositionSide.Short,
                  leverage,
                  positionMode,
                  marginType,
                },
              ];
            }
          },
        ),
      );
    } catch (err: any) {
      this.emitAndLogError(err, err?.response?.data?.msg || err?.message);
      return [];
    }
  };

  fetchPositions = async (symbols?: string[]) => {
    const openPositions = await this.fetchOpenPositions();
    if (!symbols) return openPositions;
    const openPositionsForSymbols = openPositions.filter((p) => symbols.includes(p.symbol));
    const missingSymbols = symbols.filter((s) => !openPositionsForSymbols.some((p) => p.symbol === s));
    const positionMetadatas = await this.fetchPositionMetadata(missingSymbols);
    const fakePositions = positionMetadatas.map((metadata) => {
      return {
        id: createPositionId(metadata.symbol, metadata.side, metadata.positionMode),
        ...metadata,
        entryPrice: 0,
        notional: 0,
        unrealizedPnl: 0,
        contracts: 0,
        liquidationPrice: 0,
        info: {
          ...metadata,
        },
      };
    });
    return [...openPositionsForSymbols, ...fakePositions];
  };

  fetchOpenPositions = async () => {
    const { data } = await this.xhr.get(ENDPOINTS.POSITIONS_V2);

    if (data.code !== '200000') {
      this.emitAndLogError(data.msg || 'Failed to fetch positions');
      return [];
    }

    const positions: Position[] = [];

    // KuCoin returns array of positions
    if (Array.isArray(data.data)) {
      for (const p of data.data) {
        // Skip positions with 0 size
        const contracts = parseFloat(p.currentQty || '0');
        if (contracts === 0) continue;

        const symbol = this.toInternalSymbol(p.symbol);
        const price = parseFloat(p.markPrice || '0');
        const contractSize = this.contractSizeFor(symbol);
        const contractsNative = Math.abs(contracts);
        const contractsBase = multiply(contractsNative, contractSize);

        // Determine position side
        let side: PositionSide;
        if (p.positionSide === 'LONG') {
          side = PositionSide.Long;
        } else if (p.positionSide === 'SHORT') {
          side = PositionSide.Short;
        } else if (contracts > 0) {
          side = PositionSide.Long;
        } else {
          side = PositionSide.Short;
        }

        // Determine position mode
        const positionMode = this.parseRowPositionMode(p);
        const marginType = this.parseMarginType(p.marginMode, p.crossMode);

        const position: Position = {
          id: createPositionId(symbol, side, positionMode),
          symbol,
          side,
          entryPrice: parseFloat(p.avgEntryPrice || '0'),
          notional: Math.abs(multiply(contractsBase, price)),
          leverage: parseInt(p.leverage || '1'),
          unrealizedPnl: parseFloat(p.unrealisedPnl || '0'),
          contracts: contractsBase,
          liquidationPrice: parseFloat(p.liquidationPrice || '0'),
          positionMode,
          marginType,
          extra: { contractsNative },
          info: p,
        };

        positions.push(position);
      }
    }

    return positions;
  };

  fetchMarkets = async () => {
    const { data } = await this.xhr.get(ENDPOINTS.MARKETS);

    if (data.code !== '200000') {
      this.emitAndLogError(data.msg || 'Failed to fetch markets');
      return [];
    }

    const markets: Market[] = data.data.map((m: any) => {
      const quote = m.settleCurrency || m.quoteCurrency || m.quoteCoin;
      const base = m.baseCurrency || m.rootSymbol;

      return {
        id: m.symbol,
        symbol: this.toInternalSymbol(m.symbol),
        base: base === 'XBT' ? 'BTC' : base,
        quote,
        active: m.status === 'Open' || m.status === 'ONLINE',
        precision: {
          amount: parseFloat(m.lotSize || '1'),
          price: parseFloat(m.tickSize || '0.01'),
        },
        limits: {
          amount: {
            min: parseFloat(m.minOrderQty || m.lotSize || '1'),
            max: parseFloat(m.maxOrderQty || Infinity),
          },
          leverage: {
            min: 1,
            max: parseInt(m.maxLeverage || 20),
          },
        },
        isExpiring: Boolean(m.expireDate),
        isInverse: Boolean(m.isInverse),
        contractSize: parseFloat(m.multiplier || '1'),
        // KuCoin marks multiplier-based LINEAR contracts (XBTUSDTM and ~11
        // others) as `isQuanto: true`, but that is NOT the BitMEX-style quanto
        // the app means (one needing a separate BTC index price). KuCoin's
        // multiplier is already captured by `contractSize`, so force the flag
        // off — otherwise the app's `info.isQuanto` sizing path runs
        // `getBtcPriceForQuanto`, fails to find a `.BXBT`/`XBTUSDT` ticker, and
        // sizes everything against a bogus 100000 fallback price (which broke
        // order/TP-SL sizing + chart lines).
        info: { ...m, isQuanto: false },
      };
    });

    return markets.filter((m) => m.quote === this.apiMarginCoin && !m.isExpiring);
  };

  fetchTickers = async () => {
    const { data } = await this.xhr.get(ENDPOINTS.TICKERS);

    if (data.code !== '200000') {
      this.emitAndLogError(data.msg || 'Failed to fetch tickers');
      return [];
    }

    const tickersData = data.data;
    if (!tickersData || typeof tickersData !== 'object') {
      return [];
    }

    // Get market info for mapping
    const marketsById = this.store.marketsById;
    const tickers: Ticker[] = [];

    const tickerRows = Array.isArray(tickersData) ? tickersData : Object.values(tickersData);
    for (const ticker of tickerRows as Array<Record<string, any>>) {
      const market = marketsById[ticker.symbol];
      if (!market) continue;

      const price = parseFloat(ticker.price || '0');

      tickers.push({
        id: market.id,
        symbol: market.symbol,
        bid: parseFloat(ticker.bestBidPrice || '0'),
        ask: parseFloat(ticker.bestAskPrice || '0'),
        last: price,
        mark: parseFloat(ticker.markPrice || ticker.price || '0'),
        index: parseFloat(ticker.indexPrice || ticker.price || '0'),
        percentage: 0,
        fundingRate: 0,
        fundingTime: 0,
        openInterest: 0,
        volume: parseFloat(ticker.volumeOf24h || ticker.volume24h || ticker.size || '0'),
        quoteVolume: parseFloat(ticker.turnoverOf24h || ticker.turnover24h || '0'),
      });
    }

    return tickers;
  };

  fetchOrders = async (): Promise<Order[]> => {
    // KuCoin serves orders from TWO books: /api/v1/orders (limit/market/
    // conditional) and /api/v1/stopOrders (untriggered TP/SL). The tick loop
    // FULL-REPLACES store.orders with this combined result every poll, and each
    // book is fetched independently and can fail on its own (rate limit / tunnel
    // hiccup). The sub-fetchers now signal failure with `null` (vs `[]` for a
    // genuinely empty book).
    //
    // CRITICAL: on a book's failure we PRESERVE the existing store orders of
    // that category instead of dropping them. Previously a failed stop-book
    // fetch returned `[]`, so the full-replace wiped EVERY TP/SL line off the
    // chart while limits (from the working normal book) kept rendering — the
    // reported "limit orders show but dragged TP/SL don't", with no error toast.
    // The WS stream (advancedOrders) only delivers a stop once, so once the tick
    // wiped it there was nothing to re-add it. Preserving the category keeps the
    // WS-delivered stop alive; the next successful poll refreshes it.
    const normalOrders = await this.fetchNormalOrders();
    const stopOrders = await this.fetchAlgoOrders();

    const isStopType = (o: Order) =>
      o.type === OrderType.StopLoss || o.type === OrderType.TakeProfit || o.type === OrderType.TrailingStopLoss;

    const normalPart = normalOrders ?? this.store.orders.filter((o) => !isStopType(o));
    const stopPart = stopOrders ?? this.store.orders.filter(isStopType);

    return this.reconcileFetchedOrderRemainders([...normalPart, ...stopPart]);
  };

  // The private order stream includes remainSize, while the REST order-list
  // response can only provide size and filledSize. `tick` replaces the whole
  // store snapshot, so a stale REST row must never increase a same-id order's
  // cancel-replace remainder. A later REST observation can still report more
  // progress, hence keep the smallest valid native remainder. KuCoin
  // cancel-replace yields a new order id, so unrelated/new rows deliberately
  // do not inherit a previous order's remainder.
  private reconcileFetchedOrderRemainders = (fetchedOrders: Order[]): Order[] => {
    const existingOrdersById = new Map(this.store.orders.map((order) => [order.id, order]));

    return fetchedOrders.map((fetchedOrder) => {
      const previousRemainingNative = existingOrdersById.get(fetchedOrder.id)?.extra?.remainingNative;
      const fetchedRemainingNative = fetchedOrder.extra?.remainingNative;

      if (
        !Number.isFinite(previousRemainingNative) ||
        previousRemainingNative! < 0 ||
        !Number.isFinite(fetchedRemainingNative) ||
        fetchedRemainingNative! < 0
      ) {
        return fetchedOrder;
      }

      const remainingNative = Math.min(previousRemainingNative!, fetchedRemainingNative!);
      if (remainingNative === fetchedRemainingNative) return fetchedOrder;

      return {
        ...fetchedOrder,
        remaining: multiply(remainingNative, this.contractSizeFor(fetchedOrder.symbol)),
        extra: { ...fetchedOrder.extra, remainingNative },
      };
    });
  };

  // Returns `null` on FAILURE so fetchOrders can preserve the category; `[]`
  // only for a genuinely empty book.
  fetchNormalOrders = async (): Promise<Order[] | null> => {
    try {
      const { data } = await this.xhr.get(ENDPOINTS.ORDERS, {
        params: {
          status: 'active', // Get only active orders
        },
      });

      if (data.code !== '200000') {
        this.emitAndLogError(data.msg || 'Failed to fetch orders');
        return null;
      }

      if (!Array.isArray(data.data.items)) {
        return [];
      }

      return data.data.items.flatMap(this.mapOrder);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return null;
    }
  };

  // /api/v1/stopOrders returns every untriggered stop — TP/SL AND trailing — and
  // mapOrder classifies each by type, so this single call covers what a separate
  // trailing fetch used to re-request from the same endpoint (and double-count).
  // Returns `null` on failure (see fetchOrders preservation). KuCoin trailing-stop
  // PLACEMENT throws as unsupported, so trailing rows are effectively never
  // present, but mapping them here is free and correct.
  fetchAlgoOrders = async (): Promise<Order[] | null> => {
    try {
      const { data } = await this.xhr.get(ENDPOINTS.ALGO_ORDERS, {
        params: {
          status: 'active', // Get only active orders
        },
      });

      if (data.code !== '200000') {
        this.emitAndLogError(data.msg || 'Failed to fetch algo orders');
        return null;
      }

      if (!Array.isArray(data.data.items)) {
        return [];
      }

      return data.data.items.flatMap(this.mapOrder);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return null;
    }
  };

  fetchOHLCV = async (opts: OHLCVOptions) => {
    const limit = Math.min(opts.limit || 500, 1000);
    const interval = INTERVAL[opts.interval];
    const [, amount, unit] = opts.interval.split(/(\d+)/);

    const end = opts.to ? dayjs(opts.to) : dayjs();
    const start =
      !opts.limit && opts.from ? dayjs(opts.from) : end.subtract(parseFloat(amount) * limit, unit as ManipulateType);

    const { data } = await this.xhr.get(ENDPOINTS.KLINE, {
      params: {
        symbol: this.store.marketsBySymbol[opts.symbol]?.id || opts.symbol,
        granularity: interval,
        from: start.valueOf(),
        to: end.valueOf(),
      },
    });

    const candles: Candle[] = data.data.map((c: any) => {
      return {
        timestamp: parseInt(c[0], 10) / 1000,
        open: parseFloat(c[1]),
        high: parseFloat(c[2]),
        low: parseFloat(c[3]),
        close: parseFloat(c[4]),
        volume: parseFloat(c[5]),
      };
    });

    return candles;
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    return this.publicWebsocket.listenOHLCV(opts, callback);
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    return this.publicWebsocket.listenOrderBook(symbol, callback);
  };

  cancelAllOrders = async () => {
    try {
      const { data } = await this.unlimitedXHR.delete(ENDPOINTS.CANCEL_ALL_ORDERS);

      if (data.code !== '200000') {
        this.emitAndLogError(data.msg || 'Failed to cancel all orders');
      }

      const { data: stopData } = await this.unlimitedXHR.delete(ENDPOINTS.CANCEL_ALL_STOP_ORDERS);

      if (stopData.code !== '200000') {
        this.emitAndLogError(stopData.msg || 'Failed to cancel all stop orders');
      }
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
    }
  };

  cancelOrders = async (orders: Order[]) => {
    // App-placed orders — INCLUDING standalone TP/SL — are POSTed to
    // `/api/v1/orders` (placeAlgoOrders routes every stop-flagged payload to
    // PLACE_ORDER, and trailing stops throw), so they ALL cancel via
    // DELETE `/api/v1/orders/{id}`. Routing TP/SL to `/api/v1/stopOrders/{id}`
    // returns 404 and orphans the order (confirmed live). Virtual TP/SL legs
    // resolve to their parent id inside cancelNormalOrders.
    const [virtualTPSLOrders, otherOrders] = partition<Order>(orders, this.isVirtualTPSLOrder);
    await this.cancelNormalOrders([...otherOrders, ...virtualTPSLOrders]);
  };

  cancelNormalOrders = async (orders: Order[]) => {
    // Filter out virtual TP/SL orders
    orders = orders.map((o) => ({
      ...o,
      id: o.id.split('__')[0], // Strip any suffixes
    }));
    orders = uniqBy(orders, 'id');

    for (const order of orders) {
      try {
        // KuCoin cancels orders individually
        const { data } = await this.unlimitedXHR.delete(`${ENDPOINTS.CANCEL_ORDERS}/${order.id}`, {
          params: {
            symbol: this.toApiSymbol(order.symbol),
          },
        });

        if (data.code !== '200000') {
          this.emitAndLogError(data.msg || `Failed to cancel order ${order.id}`);
        }
      } catch (err: any) {
        // Defensive: if KuCoin ever filed this id on the stop-order book rather
        // than the regular order book, retry there before surfacing the error.
        if (err?.response?.status === 404) {
          try {
            const { data } = await this.unlimitedXHR.delete(ENDPOINTS.CANCEL_ALGO_ORDER.replace('{orderId}', order.id));
            if (data.code === '200000') continue;
          } catch {
            /* fall through to surface the original error */
          }
        }
        this.emitAndLogError(err?.response?.data?.msg || err?.message);
        throw err;
      }
    }
  };

  cancelSymbolOrders = async (symbol: string) => {
    try {
      const { data } = await this.unlimitedXHR.delete(ENDPOINTS.CANCEL_SYMBOL_ORDERS, {
        params: {
          symbol: this.toApiSymbol(symbol),
        },
      });

      if (data.code !== '200000') {
        this.emitAndLogError(data.msg || `Failed to cancel orders for symbol ${symbol}`);
      }
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      throw err;
    }
  };

  async getOrFetchPositionMode(symbol: string): Promise<PositionMode> {
    const positionsForSymbol = await this.getOrFetchPositionsForSymbol(symbol);
    if (positionsForSymbol.length === 0) {
      throw new Error(`No positions found for symbol ${symbol} needed to determine position mode`);
    }
    const firstPosition = positionsForSymbol[0];
    if (!firstPosition.positionMode) {
      throw new Error(`Position missing position mode for symbol ${symbol}`);
    }
    return firstPosition.positionMode;
  }

  async getOrFetchMarginType(symbol: string): Promise<MarginType> {
    const positionsForSymbol = await this.getOrFetchPositionsForSymbol(symbol);
    if (positionsForSymbol.length === 0) {
      throw new Error(`No positions found for symbol ${symbol} needed to determine margin type`);
    }
    const firstPosition = positionsForSymbol[0];
    if (!firstPosition.marginType) {
      throw new Error(`Position missing margin type for symbol ${symbol}`);
    }
    return firstPosition.marginType;
  }

  placeOrder = async (opts: PlaceOrderOpts) => {
    this.resolveCallerClientOrderId(opts);
    if (this.isAlgoOrder(opts)) {
      const payload = this.formatAlgoOrder(opts);
      return await this.placeAlgoOrders([payload]);
    }

    const payloads = await this.formatCreateOrder(opts);
    return await this.placeOrderBatch(
      payloads,
      opts.stopLoss || opts.takeProfit ? ENDPOINTS.PLACE_ALGO_ORDER : ENDPOINTS.PLACE_ORDER,
    );
  };

  formatCreateOrder = async (opts: PlaceOrderOpts) => {
    const market = this.store.marketsBySymbol[opts.symbol];

    if (!market) {
      throw new Error(`Market ${opts.symbol} not found`);
    }

    const side = opts.side === OrderSide.Buy ? 'buy' : 'sell';

    // Get leverage from the position data
    const position = this.getPositionForOrder(opts);
    const leverage = position?.leverage || 1; // Default to 1 if not found

    const pPrice = market.precision.price;
    const pAmount = market.precision.amount;
    const amount = adjust(opts.amount, pAmount);
    const price = opts.price && opts.type !== OrderType.Market ? adjust(opts.price, pPrice) : undefined;

    // KuCoin uses "limit" and "market" for orderType
    const orderType = opts.type === OrderType.Market ? 'market' : 'limit';
    const triggerStopUpPrice = opts.side === OrderSide.Buy ? opts.takeProfit : opts.stopLoss;
    const triggerStopDownPrice = opts.side === OrderSide.Buy ? opts.stopLoss : opts.takeProfit;
    const hasAttachedStop = Boolean(triggerStopUpPrice || triggerStopDownPrice);

    // Time in force mapping for KuCoin
    let timeInForce;
    if (opts.timeInForce === OrderTimeInForce.GoodTillCancel) {
      timeInForce = 'GTC';
    } else if (opts.timeInForce === OrderTimeInForce.ImmediateOrCancel) {
      timeInForce = 'IOC';
    } else if (opts.timeInForce === OrderTimeInForce.FillOrKill) {
      timeInForce = 'FOK';
    } else if (opts.timeInForce === OrderTimeInForce.PostOnly) {
      // KuCoin Futures rejects timeInForce='POST_ONLY' ("Time in Force type
      // invalid"). Post-only is conveyed via the separate `postOnly` boolean
      // (set below); timeInForce stays GTC.
      timeInForce = 'GTC';
    } else {
      timeInForce = 'GTC'; // Default
    }

    const req = omitUndefined({
      symbol: market.id,
      type: orderType,
      side: side,
      price: price ? `${price}` : undefined,
      size: amount ? `${amount}` : undefined,
      leverage: leverage,
      reduceOnly: opts.reduceOnly,
      marginMode: this.getKucoinMarginMode(position),
      positionSide: this.getKucoinPositionSide(opts, position),
      timeInForce: orderType === 'limit' ? timeInForce : undefined,
      clientOid: this.resolveCallerClientOrderId(opts) ?? uuid(),
      triggerStopUpPrice: triggerStopUpPrice ? `${adjust(triggerStopUpPrice, pPrice)}` : undefined,
      triggerStopDownPrice: triggerStopDownPrice ? `${adjust(triggerStopDownPrice, pPrice)}` : undefined,
      stopPriceType: hasAttachedStop ? this.mapStopPriceType(opts.tpTriggerType || opts.slTriggerType) : undefined,
      // For KuCoin Futures
      postOnly: opts.timeInForce === OrderTimeInForce.PostOnly ? true : undefined,
    });

    return [req];
  };

  formatAlgoOrder = (opts: PlaceOrderOpts) => {
    const market = this.store.marketsBySymbol[opts.symbol];
    if (!market) throw new Error(`Market ${opts.symbol} not found`);

    // A 0 amount means "close the whole position" — the Full Position / infinity
    // TP/SL mode (PositionLineManager sends amount:0 when supportsInfinityStops is
    // on). KuCoin's /api/v1/orders rejects size:"0" (and the rejection used to be
    // swallowed in placeAlgoOrders, so the stop silently never placed). The
    // documented way to close the full position is the `closeOrder` flag with NO
    // size; with a real amount we send integer native contracts as usual.
    const isFullPositionStop = !opts.amount;
    const payload: Record<string, any> = {
      symbol: market.id,
      clientOid: uuid(),
      side: opts.side === OrderSide.Buy ? 'buy' : 'sell',
      type: 'market',
      reduceOnly: opts.reduceOnly ?? true,
      ...(isFullPositionStop ? { closeOrder: true } : { size: `${adjust(opts.amount, market.precision.amount)}` }),
    };

    if (opts.type === OrderType.TakeProfit && opts.price) {
      const price = adjust(opts.price, market.precision.price);
      const position = this.getPositionForOrder(opts);
      payload.stop = this.getKucoinStopDirection(opts);
      payload.stopPrice = `${price}`;
      payload.stopPriceType = this.mapStopPriceType(opts.tpTriggerType);
      payload.marginMode = this.getKucoinMarginMode(position);
      payload.positionSide = this.getKucoinPositionSide(opts, position);
    }

    if (opts.type === OrderType.StopLoss && opts.price) {
      const price = adjust(opts.price, market.precision.price);
      const position = this.getPositionForOrder(opts);
      payload.stop = this.getKucoinStopDirection(opts);
      payload.stopPrice = `${price}`;
      payload.stopPriceType = this.mapStopPriceType(opts.slTriggerType);
      payload.marginMode = this.getKucoinMarginMode(position);
      payload.positionSide = this.getKucoinPositionSide(opts, position);
    }

    if (opts.type === OrderType.TrailingStopLoss) {
      throw new Error('KuCoin Futures trailing stop placement is not exposed in the current REST documentation');
    }

    return payload;
  };

  private getOrderPlanType = (opts: { type: OrderType }) => {
    if (opts.type === OrderType.StopLoss) return 'pos_loss';
    if (opts.type === OrderType.TakeProfit) return 'pos_profit';
    if (opts.type === OrderType.TrailingStopLoss) return 'track_plan';

    throw new Error(`Unknown order type: ${opts.type}`);
  };

  placeOrderBatch = async (payloads: Array<Record<string, any>>, endpoint = ENDPOINTS.PLACE_ORDER) => {
    const newOrderIds: string[] = [];

    // KuCoin doesn't support true batch ordering in the same way
    // We'll process them individually
    for (const payload of payloads) {
      try {
        const { data } = await this.unlimitedXHR.post(endpoint, payload);

        // KuCoin returns HTTP 200 with a non-200000 `code` on rejection (same
        // contract as placeAlgoOrders). Returning without a throw here made a
        // rejected order resolve "successfully" with no ids, so awaiting
        // callers couldn't distinguish rejection from success.
        if (data?.code && data.code !== '200000') {
          throw new Error(data.msg || `KuCoin order rejected (code ${data.code})`);
        }

        if (data?.data?.orderId) {
          newOrderIds.push(data.data.orderId);
        }
      } catch (err: any) {
        this.emitter.emit('error', err?.response?.data?.msg || err?.message);
        throw err;
      }
    }

    return newOrderIds;
  };

  placeAlgoOrders = async (payloads: Array<Record<string, any>>) => {
    const newOrderIds: string[] = [];

    for (const payload of payloads) {
      try {
        const endpoint = payload.stop ? ENDPOINTS.PLACE_ORDER : ENDPOINTS.PLACE_ALGO_ORDER;

        const { data: body } = await this.unlimitedXHR.post(endpoint, payload);

        // KuCoin returns HTTP 200 with a non-200000 `code` on rejection (e.g. a
        // bad/`0` size). Previously we only read `body.data.orderId`, so a
        // rejection produced no id, no throw and no toast — the exact silent
        // failure that made full-position stops "disappear". Surface it so the
        // catch below emits and the order builder rejects.
        if (body?.code && body.code !== '200000') {
          throw new Error(body.msg || `KuCoin order rejected (code ${body.code})`);
        }

        if (body?.data?.orderId) {
          newOrderIds.push(body.data.orderId);
        }
      } catch (err: any) {
        this.emitter.emit('error', err?.response?.data?.msg || err?.message);
        throw err;
      }
    }

    return newOrderIds;
  };

  updateVirtualTpSlOrder = async ({ order, update }: UpdateOrderOpts, relatedOrders: Order[]) => {
    const market = this.store.marketsBySymbol[order.symbol];
    if (!market) throw new Error(`Market ${order.symbol} not found`);

    const parentOrder = relatedOrders.find((o) => !o.id.includes('__'));
    if (!parentOrder) {
      this.emitAndLogError(`Parent order not found for ${order.id}`);
      throw new Error(`Parent order not found for ${order.id}`);
    }

    const stopLossOrder = relatedOrders.find((o) => o.id.endsWith('__stop_loss'));
    const takeProfitOrder = relatedOrders.find((o) => o.id.endsWith('__take_profit'));

    try {
      // KuCoin doesn't support modifying TP/SL directly
      // We need to cancel the original order and place a new one
      await this.cancelOrders([parentOrder]);

      // Prepare the parameters for the new order
      let price = parentOrder.price;
      let amount = this.nativeAmountForCancelReplace(parentOrder);
      let stopLossPrice = stopLossOrder?.price;
      let takeProfitPrice = takeProfitOrder?.price;

      // Update the relevant parameter based on which order is being modified
      if (order.id === parentOrder.id) {
        if ('price' in update) price = update.price;
        if ('amount' in update) amount = update.amount;
      } else if (order.id.endsWith('__stop_loss') && 'price' in update) {
        stopLossPrice = update.price;
      } else if (order.id.endsWith('__take_profit') && 'price' in update) {
        takeProfitPrice = update.price;
      }

      // Attached TP/SL belongs to KuCoin's st-orders family. Rebuild through
      // the same formatter and endpoint used for initial attached placement.
      const payloads = await this.formatCreateOrder({
        symbol: parentOrder.symbol,
        side: parentOrder.side,
        type: parentOrder.type,
        price,
        amount,
        timeInForce: parentOrder.timeInForce,
        reduceOnly: parentOrder.reduceOnly,
        stopLoss: stopLossPrice,
        takeProfit: takeProfitPrice,
      });
      return await this.placeOrderBatch(payloads, ENDPOINTS.PLACE_ALGO_ORDER);
    } catch (err: any) {
      this.emitAndLogError(err, err?.response?.data?.msg || err?.message);
      throw err;
    }
  };

  updateOrder = async ({ order, update }: UpdateOrderOpts) => {
    const parentId = order.id.split('__')[0];
    const relatedOrders = this.store.orders.filter((o) => o.id.split('__')[0] === parentId);

    if (relatedOrders.length > 1) {
      return this.updateVirtualTpSlOrder({ order, update }, relatedOrders);
    }

    if (this.isAlgoOrder(order)) {
      return this.updateAlgoOrder({ order, update });
    }

    const market = this.store.marketsBySymbol[order.symbol];
    if (!market) throw new Error(`Market ${order.symbol} not found`);

    try {
      // KuCoin doesn't support direct order updates - cancel and replace instead
      await this.cancelOrders([order]);

      // Prepare a new order with the updated fields
      const newOrder: PlaceOrderOpts = {
        symbol: order.symbol,
        side: order.side,
        type: order.type,
        price: 'price' in update ? update.price : order.price,
        amount: 'amount' in update ? update.amount : this.nativeAmountForCancelReplace(order),
        timeInForce: order.timeInForce,
        reduceOnly: order.reduceOnly,
      };

      // Place the new order
      return await this.placeOrder(newOrder);
    } catch (err: any) {
      this.emitAndLogError(err, err?.response?.data?.msg || err?.message);
      throw err;
    }
  };

  updateAlgoOrder = async ({ order, update }: UpdateOrderOpts) => {
    const market = this.store.marketsBySymbol[order.symbol];
    if (!market) throw new Error(`Market ${order.symbol} not found`);

    try {
      // KuCoin doesn't support updating algo orders directly
      // Need to cancel and replace
      await this.cancelOrders([order]);

      // Create new algo order with updated values
      const newOrder: PlaceOrderOpts = {
        symbol: order.symbol,
        side: order.side,
        type: order.type,
        price: 'price' in update ? update.price : order.price,
        amount: 'amount' in update ? update.amount : this.nativeAmountForCancelReplace(order),
        reduceOnly: true,
      };

      return await this.placeOrder(newOrder);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      throw err;
    }
  };

  setLeverage = async (symbol: string, inputLeverage: number, _side?: PositionSide) => {
    const market = this.store.marketsBySymbol[symbol];
    if (!market) throw new Error(`Market ${symbol} not found`);

    // Clamp leverage to market limits
    const leverage = Math.min(Math.max(inputLeverage, market.limits.leverage.min), market.limits.leverage.max);

    try {
      const marginType = await this.getOrFetchMarginType(symbol);

      if (marginType !== MarginType.Cross) {
        throw new Error('KuCoin Futures only exposes cross-margin leverage changes through the current REST API');
      }

      const { data } = await this.xhr.post(ENDPOINTS.SET_LEVERAGE, {
        symbol: market.id,
        leverage: leverage.toString(),
      });

      if (data.code !== '200000') {
        throw new Error(data.msg || 'Failed to set leverage');
      }

      // Fetch updated positions to confirm the change
      const positions = await this.fetchPositions([symbol]);
      this.store.addOrUpdatePositions(positions);
    } catch (err: any) {
      this.emitAndLogError(err, err?.response?.data?.msg || err?.message);
      throw err;
    }
  };

  changeMarginType = async (isolated: boolean, symbol?: string) => {
    if (!symbol) {
      this.emitAndLogError('Symbol is required');
      throw new Error('Symbol is required');
    }

    try {
      const market = this.store.marketsBySymbol[symbol];
      if (!market) {
        throw new Error(`Market ${symbol} not found`);
      }

      const { data } = await this.xhr.post(ENDPOINTS.SET_MARGIN_MODE, {
        symbol: market.id,
        marginMode: isolated ? 'ISOLATED' : 'CROSS',
      });

      if (data.code !== '200000') {
        throw new Error(data.msg || 'Failed to change margin type');
      }

      // Fetch positions to get updated margin type
      const positions = await this.fetchPositions([symbol]);
      this.store.addOrUpdatePositions(positions);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      throw err;
    }
  };

  changePositionMode = async (hedged: boolean) => {
    try {
      // First check if there are open positions
      const positions = await this.fetchPositions();
      if (positions.some((position) => position.contracts !== 0)) {
        const error = 'Cannot update position mode with open positions';
        this.emitAndLogError(new Error(error), error);
        throw new Error(error);
      }

      const positionMode = hedged ? '1' : '0';

      const { data } = await this.xhr.post(ENDPOINTS.SET_POSITION_MODE, {
        positionMode: positionMode,
      });

      if (data.code !== '200000') {
        throw new Error(data.msg || 'Failed to change position mode');
      }

      // Fetch updated positions
      const updatedPositions = await this.fetchPositions();
      this.store.update({ positions: updatedPositions });
    } catch (err: any) {
      this.emitAndLogError(err, err?.response?.data?.msg || err?.message);
      throw err;
    }
  };

  // "info": {
  //   "symbol": "AAVEUSDT_UMCBL",
  //   "size": 1.2,
  //   "orderId": "1173546359219429379",
  //   "clientOid": "ZXjNIAJ2fTpei6R2VSrX",
  //   "filledQty": 0,
  //   "fee": 0,
  //   "price": 79.629,
  //   "state": "new",
  //   "side": "buy_single",
  //   "timeInForce": "normal",
  //   "totalProfits": 0,
  //   "posSide": "long",
  //   "marginCoin": "USDT",
  //   "presetStopLossPrice": 76.978,
  //   "filledAmount": 0,
  //   "orderType": "limit",
  //   "leverage": "10",
  //   "marginMode": "crossed",
  //   "reduceOnly": false,
  //   "enterPointSource": "API",
  //   "tradeSide": "buy_single",
  //   "holdMode": "single_hold",
  //   "orderSource": "normal",
  //   "cTime": "1715521176158",
  //   "uTime": "1715521176266"
  // }

  mapOrder = (o: Record<string, any>): Order[] => {
    const rawTimestamp = o.updateTime || o.ts || o.orderTime || o.createdAt;
    const lastUpdate = rawTimestamp ? parseInt(`${rawTimestamp}`.slice(0, 13), 10) : virtualClock.getCurrentTimeMs();
    const rawOrderType = o.orderType || o.type;

    const symbol = this.toInternalSymbol(o.symbol) || o.symbol;
    const contractSize = this.contractSizeFor(symbol);
    const sizeNative = parseFloat(o.size || '0');
    const filledNative = parseFloat(o.filledSize || '0');
    // Private order frames carry KuCoin's authoritative native remainder in
    // `remainSize`. Preserve explicit zero; derive it only when the field is
    // absent (REST order rows commonly omit it).
    const remainingNative =
      o.remainSize !== null && o.remainSize !== undefined
        ? parseFloat(`${o.remainSize}`)
        : subtract(sizeNative, filledNative);

    // Basic order mapping. KuCoin reports sizes as native contract counts;
    // expose amount/filled/remaining in BASE units (see contractSizeFor), with
    // the raw native remainder kept on `extra.remainingNative` (OKX pattern).
    const order: Writable<Order> = {
      id: o.id || o.orderId,
      status: ORDER_STATUS[o.status] || ORDER_STATUS.new,
      symbol,
      type: ORDER_TYPE[rawOrderType] || (rawOrderType === 'market' ? OrderType.Market : OrderType.Limit),
      side: o.side === 'buy' ? OrderSide.Buy : OrderSide.Sell,
      price: parseFloat(o.price || o.orderPrice || '0'),
      amount: multiply(sizeNative, contractSize),
      filled: multiply(filledNative, contractSize),
      lastUpdate,
      reduceOnly: o.reduceOnly === true,
      remaining: multiply(remainingNative, contractSize),
      timeInForce: TIME_IN_FORCE[o.timeInForce] || OrderTimeInForce.GoodTillCancel,
      extra: { remainingNative, filledInPlacementUnits: filledNative },
      info: o,
    };

    // Handle stop orders differently. NB: KuCoin returns `stop: ""` (empty
    // string) for NON-stop orders, so a bare `typeof o.stop === 'string'` check
    // misclassifies normal limit orders as stops and overwrites their price with
    // stopPrice (0) — require a non-empty stop value.
    if (o.stop === true || (typeof o.stop === 'string' && o.stop !== '') || rawOrderType === 'stop') {
      const stopType = String(o.stopType || o.stop || '').toLowerCase();
      if (stopType === 'down' || stopType === 'loss' || stopType === 'sl') {
        order.type = OrderType.StopLoss;
      } else if (stopType === 'up' || stopType === 'profit' || stopType === 'tp') {
        order.type = OrderType.TakeProfit;
      }
      order.price = parseFloat(o.stopPrice || '0');
    }

    // Check if this is a trailing stop
    if (o.stop === true && o.stopPriceType === 'TP') {
      order.type = OrderType.TrailingStopLoss;
    }

    let orders = [order];

    // Attached st-orders use triggerStopUpPrice/triggerStopDownPrice. Map them
    // by parent side; retain legacy field fallbacks for already-cached rows.
    const attachedStopLoss =
      o.stopLoss || (order.side === OrderSide.Buy ? o.triggerStopDownPrice : o.triggerStopUpPrice);
    const attachedTakeProfit =
      o.takeProfit || (order.side === OrderSide.Buy ? o.triggerStopUpPrice : o.triggerStopDownPrice);

    if (attachedStopLoss) {
      orders.push({
        ...order,
        id: `${order.id}__stop_loss`,
        side: order.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy,
        type: OrderType.StopLoss,
        price: parseFloat(attachedStopLoss),
        reduceOnly: true,
      });
    }

    if (attachedTakeProfit) {
      orders.push({
        ...order,
        id: `${order.id}__take_profit`,
        side: order.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy,
        type: OrderType.TakeProfit,
        price: parseFloat(attachedTakeProfit),
        reduceOnly: true,
      });
    }

    return orders;
  };

  private getOrderSideV2 = (opts: PlaceOrderOpts, positionMode: PositionMode) => {
    let side = opts.side === OrderSide.Buy ? 'Buy' : 'Sell';
    let tradeSide = undefined;
    if (positionMode === PositionMode.Hedged) {
      if (opts.reduceOnly) {
        tradeSide = 'Close';
        side = opts.side === OrderSide.Buy ? 'Sell' : 'Buy';
      } else {
        tradeSide = 'Open';
      }
    }
    return [side, tradeSide];
  };

  private getOrderSide = (opts: PlaceOrderOpts, positionMode: PositionMode) => {
    if (positionMode === PositionMode.OneWay) {
      return opts.side === OrderSide.Buy ? 'buy_single' : 'sell_single';
    }
    if (opts.reduceOnly) {
      if (opts.side === OrderSide.Buy) return 'close_short';
      if (opts.side === OrderSide.Sell) return 'close_long';
    }

    if (opts.side === OrderSide.Buy) return 'open_long';
    if (opts.side === OrderSide.Sell) return 'open_short';

    throw new Error(`Unknown order side: ${opts.side}`);
  };

  private getAlgoOrderSide = (opts: PlaceOrderOpts) => {
    if (opts.type === OrderType.TakeProfit) {
      if (opts.side === OrderSide.Buy) return 'short';
      if (opts.side === OrderSide.Sell) return 'long';
    }

    if (opts.type === OrderType.StopLoss) {
      if (opts.side === OrderSide.Buy) return 'short';
      if (opts.side === OrderSide.Sell) return 'long';
    }

    if (opts.type === OrderType.TrailingStopLoss) {
      if (opts.side === OrderSide.Buy) return 'close_short';
      if (opts.side === OrderSide.Sell) return 'close_long';
    }

    throw new Error(`Unknown algo order side: ${opts.type} - ${opts.side}`);
  };

  private getPositionForOrder = (opts: PlaceOrderOpts) => {
    const targetSide =
      opts.reduceOnly || this.isAlgoOrder(opts)
        ? opts.side === OrderSide.Buy
          ? PositionSide.Short
          : PositionSide.Long
        : opts.side === OrderSide.Buy
          ? PositionSide.Long
          : PositionSide.Short;

    return this.store.positions.find((p) => p.symbol === opts.symbol && p.side === targetSide);
  };

  private getKucoinMarginMode = (position?: Position) => {
    if (position?.marginType === MarginType.Cross) return 'CROSS';
    if (position?.marginType === MarginType.Isolated) return 'ISOLATED';
    return undefined;
  };

  private getKucoinPositionSide = (opts: PlaceOrderOpts, position?: Position) => {
    if (position?.positionMode !== PositionMode.Hedged) return undefined;

    const targetSide =
      opts.reduceOnly || this.isAlgoOrder(opts)
        ? opts.side === OrderSide.Buy
          ? PositionSide.Short
          : PositionSide.Long
        : opts.side === OrderSide.Buy
          ? PositionSide.Long
          : PositionSide.Short;

    return targetSide === PositionSide.Long ? 'LONG' : 'SHORT';
  };

  private mapStopPriceType = (triggerType?: OrderStopTriggerType) => {
    if (triggerType === OrderStopTriggerType.MarkPrice) return 'MP';
    if (triggerType === OrderStopTriggerType.IndexPrice) return 'IP';
    return 'TP';
  };

  private getKucoinStopDirection = (opts: PlaceOrderOpts) => {
    if (opts.type === OrderType.StopLoss) {
      return opts.side === OrderSide.Buy ? 'up' : 'down';
    }
    if (opts.type === OrderType.TakeProfit) {
      return opts.side === OrderSide.Buy ? 'down' : 'up';
    }
    throw new Error(`Unsupported KuCoin stop direction for order type: ${opts.type}`);
  };

  private isVirtualTPSLOrder = (order: Order) => {
    return order.id.endsWith('__stop_loss') || order.id.endsWith('__take_profit');
  };

  private isAlgoOrder = (opts: PlaceOrderOpts) => {
    return (
      opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit || opts.type === OrderType.TrailingStopLoss
    );
  };

  private getOrderPlanTypeAndAmount = (opts: PlaceOrderOpts) => {
    // todo: is this infinity?
    if (opts.type === OrderType.TrailingStopLoss) return ['track_plan', opts.amount];

    let infiniteStop = true;
    if (opts.amount !== 0) {
      // opts.amount is native contracts; position.contracts is base units
      // (see contractSizeFor) — compare in the same (base) unit.
      const optsAmountBase = multiply(opts.amount, this.contractSizeFor(opts.symbol));
      const foundPosition = this.store.positions.find(
        (p) =>
          p.symbol === opts.symbol &&
          p.side === (opts.side === OrderSide.Buy ? PositionSide.Short : PositionSide.Long) &&
          p.contracts >= optsAmountBase,
      );
      if (foundPosition) {
        infiniteStop = false;
      }
    }

    if (infiniteStop) {
      if (opts.type === OrderType.StopLoss) return ['pos_loss', 0];
      if (opts.type === OrderType.TakeProfit) return ['pos_profit', 0];
    } else {
      if (opts.type === OrderType.StopLoss) return ['loss_plan', opts.amount];
      if (opts.type === OrderType.TakeProfit) return ['profit_plan', opts.amount];
    }

    throw new Error(`Unknown order type: ${opts.type}`);
  };
}
