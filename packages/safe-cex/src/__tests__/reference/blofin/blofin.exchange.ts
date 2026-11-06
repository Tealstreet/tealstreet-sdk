import type { Axios } from 'axios';
import type { ManipulateType } from 'dayjs';
import type { Store } from '../../../store/store.interface';

import dayjs from 'dayjs';
import chunk from 'lodash/chunk';
import flatten from 'lodash/flatten';
import partition from 'lodash/partition';
import sumBy from 'lodash/sumBy';
import times from 'lodash/times';
import uniqBy from 'lodash/uniqBy';
import { nanoid } from 'nanoid';
import { forEachSeries, mapSeries } from 'p-iteration';

import { sleep } from '@tealstreet/sleep/src/sleep';
import { mapBlofinPositionHistory } from '../../../journal/blofin.position-adapter';
import {
  Balance,
  Candle,
  ClosedPnlEntry,
  ExchangeName,
  ExchangeOptions,
  Execution,
  ExecutionsOptions,
  FetchedLeverage,
  HistoricalOrder,
  HistoricalTradeOptions,
  IncomeHistoryOptions,
  IncomeRecord,
  LogSeverity,
  MarginType,
  OHLCVOptions,
  Order,
  OrderBook,
  OrderHistoryOptions,
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
import { inverseObj } from '../../../utils/inverseObj';
import { loop } from '../../../utils/loop';
import { normalizeExecutionPositionSide } from '../../../utils/normalizeExecutionPositionSide';
import { oldestCursorId } from '../../../utils/oldestCursorId';
import { omitUndefined } from '../../../utils/omitUndefined';
import { roundUSD } from '../../../utils/roundUSD';
import { add, adjust, multiply, subtract } from '../../../utils/safe-math';
import { uuid } from '../../../utils/uuid';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseExchange, CallableMethodsInWorker } from '../../../exchanges/base';
import { patternClientOrderIdRule } from '../../../exchanges/clientOrderId';
import { createAPI } from './blofin.api';
import { mapBlofinClosedPosition, mapBlofinFillToIncomeRecords } from '../../../exchanges/blofin/blofin.income';
import { mapBlofinHistoricalOrder } from './blofin.order-history';
import {
  BROKER_ID,
  ENDPOINTS,
  FETCH_BATCH_LEV_RESPONSE,
  FETCH_MARGIN_MODE_RESPONSE,
  FETCH_POSITION_RESPONSE,
  GRAFANA_ENDPOINTS_EVERY,
  GRAFANA_ENDPOINTS_PERIODIC,
  INTERVAL,
  ORDER_SIDE,
  ORDER_STATUS,
  ORDER_TYPE,
  ORDERS_RESPONSE,
  POSITION_MODE_RESPONSE,
  POSITION_SIDE,
  REVERSE_ORDER_TYPE,
  USE_NON_BATCH_ENDPOINT_FOR_REDUCE_ONLY,
} from '../../../exchanges/blofin/blofin.types';
import { BlofinPrivateWebsocket } from './blofin.ws-private';
import { BlofinPublicWebsocket } from './blofin.ws-public';

const BLOFIN_SUPPORTED_INST_TYPE = 'SWAP';
const BLOFIN_SUPPORTED_CONTRACT_TYPE = 'linear';
const BLOFIN_ACTIVE_ORDERS_PAGE_SIZE = 100;
const BLOFIN_MULTI_TRADE_POSITION_SELECTION_CODE = '102143';
const BLOFIN_MULTI_TRADE_UNSUPPORTED_MESSAGE =
  'BloFin Multi-Position mode is not supported in Tealstreet yet. Turn off Multi-Position mode on BloFin, then retry from Tealstreet.';
// Safety cap on order pagination. BloFin caps orders-pending / orders-tpsl-pending
// at ~20 rows per page regardless of the requested `limit`, so we page via the
// `after` cursor until an empty page. This bounds the loop if the cursor ever
// fails to advance (e.g. exchange ignores `after`). 50 pages is far beyond any
// realistic open-order count.
const BLOFIN_MAX_ORDER_PAGES = 50;

const getNormalizedInstType = (payload: Record<string, any>) => String(payload.instType || '').toUpperCase();

const isBlofinLinearSwapMarket = (payload: Record<string, any>) =>
  getNormalizedInstType(payload) === BLOFIN_SUPPORTED_INST_TYPE &&
  String(payload.contractType || '').toLowerCase() === BLOFIN_SUPPORTED_CONTRACT_TYPE;

const isKnownNonSwapInstrument = (payload: Record<string, any>) => {
  const instType = getNormalizedInstType(payload);
  return !!instType && instType !== BLOFIN_SUPPORTED_INST_TYPE;
};

export class BlofinExchange extends BaseExchange {
  protected readonly clientOrderIdRule = patternClientOrderIdRule('BloFin', /^[A-Za-z0-9]{1,32}$/, {
    mint: () => `${BROKER_ID}${uuid()}`.slice(0, 32),
  });

  xhr: Axios;

  publicWebsocket: BlofinPublicWebsocket;
  privateWebsocket: BlofinPrivateWebsocket;

  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;
  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;

  private _accountMarginType: MarginType | null = null;
  private _accountPositionMode: PositionMode | null = null;

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    this.xhr = this.createXhrWithSecondaryKey(opts, createAPI);
    this.publicWebsocket = new BlofinPublicWebsocket(this);
    this.privateWebsocket = new BlofinPrivateWebsocket(this);
  }

  validateAccount = async () => {
    let balance: Balance | undefined;
    try {
      const { data } = await this.xhr.get(ENDPOINTS.BALANCE, {
        params: {
          accountType: 'futures',
        },
      });
      if (data.code !== '0') {
        throw new Error(data.msg);
      }
      // Same BALANCE response fetchBalanceAndPositions parses: collateral = sum of
      // available + frozen (no positions at validate time, so used/upnl are 0).
      const bal = data?.data;
      if (Array.isArray(bal)) {
        const total = add(
          roundUSD(sumBy(bal, (b: any) => parseFloat(b.available))),
          roundUSD(sumBy(bal, (b: any) => parseFloat(b.frozen))),
        );
        if (Number.isFinite(total)) balance = { total, free: total, used: 0, upnl: 0 };
      }
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return { error: err?.response?.data?.msg || err?.message || err?.message || 'Invalid API key or secret' };
    }
    return { error: '', balance };
  };

  dispose = () => {
    super.dispose();
    this.publicWebsocket.dispose();
    this.privateWebsocket.dispose();
  };

  start = async () => {
    const useExternalPublicData = await this.initExternalPublicData();
    const publicDataOnly = this.isPublicDataOnly;

    if (!useExternalPublicData) {
      const [, rawTickers] = await Promise.all([
        this.fetchMarkets().then((markets) => {
          this.store.update({
            markets,
            loaded: { ...this.store.loaded, markets: true },
          });

          // start public websocket streams as soon as markets loaded.
          this.publicWebsocket.connectAndSubscribe();

          return markets;
        }),
        this.fetchTickersRaw(),
      ]);

      if (this.isDisposed) return;
      const tickers = this.formatTickers(rawTickers);
      this.store.update({
        tickers,
        loaded: { ...this.store.loaded, tickers: true },
      });
    }

    if (publicDataOnly) return;
    this.privateWebsocket.connectAndSubscribe();

    await Promise.all([this.getOrFetchMarginType(undefined, true), this.getOrFetchPositionMode(undefined, true)]);

    await Promise.all([
      this.fetchBalanceAndPositions().then(({ balance, positions }) => {
        this.store.update({
          balance,
          positions,
          loaded: { ...this.store.loaded, balance: true, positions: true },
        });
      }),
      this.fetchOrders().then((orders) => {
        this.store.update({
          orders,
          loaded: { ...this.store.loaded, orders: true },
        });
      }),
    ]);

    if (this.authFailureStreak > 0) {
      loop(() => this.tick(), this.getTickInterval() ?? 10_000);
      return;
    }

    // Start the tick loop for periodic updates
    await this.tick();
  };

  tick = async (count = 0) => {
    if (!this.isDisposed) {
      // Fetch balance and positions
      try {
        const { balance, positions } = await this.fetchBalanceAndPositions();
        if (this.isDisposed) return;

        // Use addOrUpdatePositionsFull instead of a wholesale replace so that closed-symbol
        // position records (which the WS layer keeps around to retain leverage / margin context)
        // aren't wiped every 10 seconds. This previously caused the place-order % slider to
        // jitter because the React fallback would briefly land on 1x between tick and WS update.
        this.store.addOrUpdatePositionsFull(positions);
        this.store.update({
          balance,
          loaded: {
            ...this.store.loaded,
            balance: true,
            positions: true,
          },
        });
      } catch (err: any) {
        this.emitAndLogError(err?.response?.data?.msg || err?.message);
        // Continue to orders fetch even if balance/positions failed
      }

      // Fetch orders (separately to ensure one failure doesn't block the other)
      try {
        const orders = await this.fetchOrders();
        if (this.isDisposed) return;
        // Only update if we got valid orders (fetchOrders now returns existing on failure)
        if (orders.length > 0 || this.store.orders.length === 0) {
          this.store.update({ orders });
        }
      } catch (err: any) {
        this.emitAndLogError(err?.response?.data?.msg || err?.message);
        // Don't update orders on complete failure to preserve existing state
      }

      // getTickInterval() is the metadata 10s, backed off exponentially while
      // the credentials are being rejected. A hardcoded interval here polls a
      // dead API key forever at full rate.
      if (!this.isDisposed) {
        loop(() => this.tick(count + 1), this.getTickInterval() ?? 10_000);
      }
    }
  };

  fetchMarkets = async () => {
    try {
      const {
        data: { data },
      } = await this.xhr.get<{ data: Array<Record<string, any>> }>(`${ENDPOINTS.MARKETS}?${nanoid()}`);

      return data.filter(isBlofinLinearSwapMarket).map((m) => {
        const maxAmount = Math.min(parseFloat(m.maxLimitSize), parseFloat(m.maxMarketSize));
        const contractValue = parseFloat(m.contractValue);
        const lotSize = m.lotSize ? parseFloat(m.lotSize) : parseFloat(m.minSize);

        return {
          id: m.instId,
          symbol: m.instId.replace(/-SWAP$/, '').replace(/-/g, ''),
          base: m.baseCurrency,
          quote: m.quoteCurrency,
          active: m.state === 'live',
          precision: {
            // Native order size precision (contracts).
            amount: lotSize,
            price: parseFloat(m.tickSize),
          },
          limits: {
            amount: {
              min: parseFloat(m.minSize) * contractValue,
              max: maxAmount,
            },
            leverage: {
              min: 1,
              max: parseFloat(m.maxLeverage),
            },
          },
          contractSize: contractValue,
          info: {
            ...m,
            // Keep raw instrument values for exchange-specific sizing logic.
            contractValue: contractValue,
            lotSize: lotSize,
          },
        };
      });
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return this.store.markets;
    }
  };

  fetchTickers = async () => {
    const rawTickers = await this.fetchTickersRaw();
    return this.formatTickers(rawTickers);
  };

  fetchTickersRaw = async () => {
    try {
      const {
        data: { data },
      } = await this.xhr.get(ENDPOINTS.TICKERS);

      return data;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return [];
    }
  };

  formatTickers = (data: any) => {
    try {
      const tickers: Ticker[] = data.reduce((acc: Ticker[], t: Record<string, any>) => {
        const market = this.store.marketsById[t.instId];

        if (!market) return acc;

        const open = parseFloat(t.open24h);
        const last = parseFloat(t.last);
        const percentage = roundUSD(((last - open) / open) * 100);

        const ticker = {
          id: market.id,
          symbol: market.symbol,
          bid: parseFloat(t.bidPrice),
          ask: parseFloat(t.askPrice),
          last,
          mark: last,
          index: last,
          percentage,
          fundingRate: 0,
          fundingTime: 0,
          volume: parseFloat(t.volCurrency24h),
          quoteVolume: parseFloat(t.vol24h),
          openInterest: 0,
          info: t,
        };

        return [...acc, ticker];
      }, []);

      return tickers;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return this.store.tickers;
    }
  };

  fetchPositions = async (symbols?: string[]) => {
    const openPositions = await this.fetchPositionsHelper();
    if (this.authFailureStreak > 0) {
      return symbols ? this.store.positions.filter((p) => symbols.includes(p.symbol)) : openPositions;
    }
    if (symbols) {
      const openPositionsForSymbols = openPositions.filter((p) => symbols.includes(p.symbol));
      const remainingSymbols = symbols.filter((s) => !openPositionsForSymbols.some((p) => p.symbol === s));
      let fakePositions: Position[] = [];
      if (remainingSymbols.length > 0) {
        // refetch margin type and pos mode for newest data...
        const [marginType, positionMode] = await Promise.all([
          this.getOrFetchMarginType(undefined, true),
          this.getOrFetchPositionMode(undefined, true),
        ]);
        const positionMetadatas = await this.fetchPositionMetadata(remainingSymbols);
        fakePositions = positionMetadatas.map((p) => ({
          positionMode,
          marginType,
          ...p,
          id: createPositionId(p.symbol, p.side, p.positionMode || positionMode),
          entryPrice: 0,
          leverage: p.leverage,
          notional: 0,
          unrealizedPnl: 0,
          contracts: 0,
          liquidationPrice: 0,
          info: {},
        }));
      }
      // uniqBy should not be needed...
      return uniqBy([...openPositionsForSymbols, ...fakePositions], 'id');
    } else {
      return openPositions;
    }
  };

  // WARNING will return [] for symbol with no open position
  fetchPositionsHelper = async (symbol?: string) => {
    try {
      const { data } = await this.xhr.get<FETCH_POSITION_RESPONSE>(ENDPOINTS.POSITIONS, {
        params: symbol ? { instId: this.symbolToMarketId(symbol) } : {},
      });
      if (!data.data) {
        this.emitAndLogError(data.msg || 'No data found');
        return symbol ? this.store.positions.filter((p) => p.symbol === symbol) : this.store.positions;
      }
      return this.mapPositions(data.data);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return symbol ? this.store.positions.filter((p) => p.symbol === symbol) : this.store.positions;
    }
  };

  fetchBalanceAndPositions = async () => {
    try {
      const positions = await this.fetchPositions();

      const {
        data: { data: bal },
      } = await this.xhr.get<{
        data: Array<Record<string, any>>;
      }>(ENDPOINTS.BALANCE, {
        params: {
          accountType: 'futures',
        },
      });

      const totalCollateral = add(
        roundUSD(sumBy(bal, (b) => parseFloat(b.available))),
        roundUSD(sumBy(bal, (b) => parseFloat(b.frozen))),
      );
      const used = roundUSD(sumBy(positions, (p) => parseFloat(p.info.maintenanceMargin)));
      const upnl = roundUSD(sumBy(positions, (p) => p.unrealizedPnl));

      const balance: Balance = {
        used,
        free: subtract(totalCollateral, used),
        total: subtract(totalCollateral, upnl),
        upnl,
        info: bal,
      };

      return {
        balance,
        positions,
      };
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return {
        balance: this.store.balance,
        positions: this.store.positions,
      };
    }
  };

  symbolToMarketId = (symbol: string) => {
    const market = this.store.marketsBySymbol[symbol];
    return market ? market.id : symbol;
  };

  marketIdToSymbol = (marketId: string) => {
    const market = this.store.marketsById[marketId];
    return market ? market.symbol : marketId;
  };

  getOrFetchPositionMode = async (symbol?: string, refetch = false): Promise<PositionMode> => {
    if (symbol) {
      const position = this.store.positions.find((p) => p.symbol === symbol);
      if (position?.positionMode) return position.positionMode;
    }
    if (this._accountPositionMode && !refetch) return this._accountPositionMode;
    const { data } = await this.xhr.get<POSITION_MODE_RESPONSE>(ENDPOINTS.POSITION_MODE);
    if (data.code !== '0') {
      this.emitAndLogError(data.msg);
      return PositionMode.OneWay;
    }
    this._accountPositionMode = data.data.positionMode === 'net_mode' ? PositionMode.OneWay : PositionMode.Hedged;
    return this._accountPositionMode;
  };

  getOrFetchMarginType = async (symbol?: string, refetch = false): Promise<MarginType> => {
    if (symbol) {
      const position = this.store.positions.find((p) => p.symbol === symbol && p.contracts > 0);
      if (position?.marginType) return position.marginType;
    }
    if (this._accountMarginType && !refetch) return this._accountMarginType;
    const { data } = await this.xhr.get<FETCH_MARGIN_MODE_RESPONSE>(ENDPOINTS.MARGIN_MODE);
    if (data.code !== '0') {
      this.emitAndLogError(data.msg);
      return MarginType.Cross;
    }
    this._accountMarginType = data.data.marginMode === 'isolated' ? MarginType.Isolated : MarginType.Cross;
    return this._accountMarginType;
  };

  fetchPositionMetadata = async (
    symbols: string[],
  ): Promise<Pick<Position, 'symbol' | 'side' | 'leverage' | 'positionMode'>[]> => {
    try {
      const [marginType, positionMode] = await Promise.all([
        this.getOrFetchMarginType(),
        this.getOrFetchPositionMode(),
      ]);
      const res = flatten(
        await mapSeries(chunk(symbols, 20), async (batch) => {
          if (this.isDisposed) return [];

          const { data } = await this.xhr.get<FETCH_BATCH_LEV_RESPONSE>(ENDPOINTS.LEVERAGE, {
            params: {
              instId: batch.map((symbol) => this.symbolToMarketId(symbol)).join(','),
              marginMode: marginType === MarginType.Isolated ? 'isolated' : 'cross',
            },
          });

          // {
          //   "code": "152406",
          //   "msg": "Your IP is not included in your API key's IP whitelist"
          // }
          if (data.code !== '0') {
            this.emitAndLogError(data.msg);
          } else if (!data.data) console.warn('No leverage data found');

          return (data.data || []).map((d: FETCH_BATCH_LEV_RESPONSE['data'][0]) => ({
            symbol: this.marketIdToSymbol(d.instId),
            leverage: parseFloat(d.leverage),
            marginType: d.marginMode === 'cross' ? MarginType.Cross : MarginType.Isolated,
            positionMode: d.positionSide === 'net' ? PositionMode.OneWay : PositionMode.Hedged,
            side: d.positionSide === 'net' || d.positionSide === 'long' ? PositionSide.Long : PositionSide.Short,
          }));
        }),
      );

      return res.filter((r) => r.positionMode === positionMode);
    } catch (err: any) {
      this.emitAndLogError(err, err?.response?.data?.msg || err?.message);
      return [];
    }
  };

  fetchLeverageForSymbol = async (symbol: string): Promise<FetchedLeverage | null> => {
    try {
      const market = this.store.marketsBySymbol[symbol];
      if (!market) {
        this.log(`Market ${symbol} not found`, LogSeverity.Warning);
        return null;
      }

      const marginType = await this.getOrFetchMarginType();
      const { data } = await this.xhr.get<FETCH_BATCH_LEV_RESPONSE>(ENDPOINTS.LEVERAGE, {
        params: {
          instId: market.id,
          marginMode: marginType === MarginType.Isolated ? 'isolated' : 'cross',
        },
      });

      if (data.code !== '0') {
        this.emitAndLogError(data.msg);
        return null;
      }

      const rows = data.data || [];
      if (rows.length === 0) {
        return null;
      }

      // Hedged isolated returns separate `long`/`short` rows; net mode returns a single `net` row.
      const netRow = rows.find((r) => r.positionSide === 'net');
      const longRow = rows.find((r) => r.positionSide === 'long') || netRow || rows[0];
      const shortRow = rows.find((r) => r.positionSide === 'short') || netRow || rows[0];

      const buyLeverage = parseFloat(longRow.leverage);
      const sellLeverage = parseFloat(shortRow.leverage);

      if (!Number.isFinite(buyLeverage) || !Number.isFinite(sellLeverage)) {
        return null;
      }

      const leverageObj: FetchedLeverage = {
        buyLeverage,
        sellLeverage,
        marginType: (longRow.marginMode || shortRow.marginMode || netRow?.marginMode) === 'isolated'
          ? MarginType.Isolated
          : MarginType.Cross,
        positionMode: netRow ? PositionMode.OneWay : PositionMode.Hedged,
      };

      this.store.update({
        fetchedLeverage: {
          ...this.store.fetchedLeverage,
          [symbol]: leverageObj,
        },
      });

      return leverageObj;
    } catch (err: any) {
      this.log(
        `Error fetching leverage for ${symbol}: ${err?.response?.data?.msg || err?.message || err}`,
        LogSeverity.Warning,
      );
      return null;
    }
  };

  fetchOrders = async (_symbols?: string[]) => {
    return this.fetchOrdersWithRetry(3);
  };

  private fetchOrdersWithRetry = async (retries: number): Promise<Order[]> => {
    let lastError: Error | null = null;

    for (let attempt = 1; attempt <= retries; attempt++) {
      try {
        const [orders, algoOrders, triggerOrders] = await Promise.all([
          this.fetchNormalOrders(),
          this.fetchAlgoOrders(),
          this.fetchTriggerOrders(),
        ]);
        const allOrders = this.addInlineTpSlFallbackOrders([...orders, ...algoOrders, ...triggerOrders]);

        // If all fetches returned empty but we had orders before, something might be wrong
        // Log a warning but still return the result (empty is valid if user has no orders)
        if (allOrders.length === 0 && this.store.orders.length > 0) {
          this.log(
            'Warning: fetchOrders returned empty but store had orders. Could be valid or API issue.',
            LogSeverity.Warning,
          );
        }

        return allOrders;
      } catch (err: any) {
        lastError = err;
        if (attempt < retries) {
          this.log(`fetchOrders attempt ${attempt} failed, retrying...`, LogSeverity.Warning);
          await sleep(1000 * attempt); // Exponential-ish backoff
        }
      }
    }

    // All retries failed
    this.emitAndLogError(lastError?.message || 'fetchOrders failed after retries');
    // On complete failure, return existing orders to avoid losing data
    return this.store.orders;
  };

  private fetchPaginatedOrderRows = async (
    endpoint: string,
    cursorKey: 'orderId' | 'tpslId' | 'algoId',
    family: 'normal' | 'TPSL' | 'trigger',
    extraParams: Record<string, string> = {},
  ): Promise<Array<Record<string, any>>> => {
    const recursiveFetch = async (
      orders: Array<Record<string, any>> = [],
      page = 0,
    ): Promise<Array<Record<string, any>>> => {
      const cursor = orders.length ? orders[orders.length - 1][cursorKey] : undefined;
      const { data } = await this.xhr.get<ORDERS_RESPONSE>(endpoint, {
        params: {
          after: cursor,
          limit: BLOFIN_ACTIVE_ORDERS_PAGE_SIZE,
          ...extraParams,
        },
      });

      if (!data.data) {
        const message = data.msg || `No ${family} order data found`;
        this.emitAndLogError(message);
        throw new Error(message);
      }
      if (data.data.length === 0) return orders;

      // BloFin caps pending-order responses below the requested limit, so the
      // only complete snapshot is one that reaches an empty page.
      const nextCursor = (data.data[data.data.length - 1] as Record<string, any>)[cursorKey];
      if (typeof nextCursor !== 'string' || nextCursor.length === 0) {
        const message = `BloFin ${family} order pagination returned an invalid ${cursorKey}`;
        this.emitAndLogError(message);
        throw new Error(message);
      }
      if (cursor !== undefined && nextCursor === cursor) {
        const message = `BloFin ${family} order pagination cursor did not advance`;
        this.emitAndLogError(message);
        throw new Error(message);
      }

      const merged = [...orders, ...data.data];
      if (page + 1 < BLOFIN_MAX_ORDER_PAGES) {
        return recursiveFetch(merged, page + 1);
      }

      const message = `BloFin ${family} order pagination exceeded ${BLOFIN_MAX_ORDER_PAGES} pages`;
      this.emitAndLogError(message);
      throw new Error(message);
    };

    return recursiveFetch();
  };

  fetchNormalOrders = async () => {
    return this.mapOrders(await this.fetchPaginatedOrderRows(ENDPOINTS.UNFILLED_ORDERS, 'orderId', 'normal'));
  };

  fetchAlgoOrders = async () => {
    return this.mapOrders(await this.fetchPaginatedOrderRows(ENDPOINTS.UNFILLED_ALGO_ORDERS, 'tpslId', 'TPSL'));
  };

  fetchTriggerOrders = async () => {
    return this.mapOrders(
      await this.fetchPaginatedOrderRows(ENDPOINTS.UNFILLED_TRIGGER_ORDERS, 'algoId', 'trigger', {
        orderType: 'trigger',
      }),
    );
  };

  /**
   * Ledger from `/api/v1/trade/fills-history`. BloFin has no bills endpoint
   * (`/api/v1/account/bills` answers 152404 "not supported"), so the fills carry it.
   * Funding is on neither endpoint, so that category is always empty here.
   */
  fetchIncomeHistory = async (opts?: IncomeHistoryOptions): Promise<IncomeRecord[]> => {
    if (this.isPublicDataOnly) {
      return [];
    }

    const market = opts?.symbol ? this.store.marketsBySymbol[opts.symbol] : undefined;
    if (opts?.symbol && !market) {
      return [];
    }

    const resolveSymbol = (instId: string) => this.store.marketsById[instId]?.symbol;
    const MAX_PAGES = 100;
    const PAGE_LIMIT = 100;
    const startTime = opts?.startTime;
    const endTime = opts?.endTime;

    try {
      const records: IncomeRecord[] = [];
      const seen = new Set<string>();
      // `after` means "earlier than this id" on BloFin, as on OKX. Using `before` pages
      // the wrong direction and silently returns nothing after the first page — the
      // ledger then truncates at exactly one page and looks like a small account.
      //
      // And `begin`/`end` are IGNORED on THIS endpoint, though `positions-history` on the
      // same venue honours them. So paging by cursor alone walks back through ALL history:
      // a 30-day request returned fills spanning six months. The window has to be enforced
      // client-side — both as a stop condition and as a filter.
      let after: string | undefined;
      let page = 0;

      for (; page < MAX_PAGES; page += 1) {
        const params = omitUndefined({
          instId: market?.id,
          // Sent anyway in case BloFin starts honouring them; the client-side window
          // above is what actually bounds the result today.
          begin: startTime,
          end: endTime,
          limit: PAGE_LIMIT,
          after,
        });
        const { data } = await this.xhr.get(ENDPOINTS.FILLS, { params });
        const rows: Array<Record<string, any>> = Array.isArray(data?.data) ? data.data : [];
        if (rows.length === 0) break;

        let added = 0;
        let reachedStart = false;
        for (const row of rows) {
          const ts = parseInt(String(row.ts ?? '0'), 10);
          if (startTime && ts && ts < startTime) {
            reachedStart = true;
            continue;
          }
          if (endTime && ts && ts > endTime) continue;

          const id = String(row.tradeId ?? '');
          if (id && seen.has(id)) continue;
          if (id) seen.add(id);
          added += 1;
          records.push(...mapBlofinFillToIncomeRecords(row, resolveSymbol));
        }

        // Rows are newest-first, so the first page containing anything older than the
        // window start means the window is exhausted.
        if (reachedStart || rows.length < PAGE_LIMIT) break;
        if (added === 0) break;
        const next = oldestCursorId(rows, 'tradeId') ?? '';
        if (!next || next === after) break;
        after = next;
      }

      if (page >= MAX_PAGES) {
        this.log(
          `fetchIncomeHistory stopped at the ${MAX_PAGES}-page cap; older fills were not fetched`,
          LogSeverity.Warning,
        );
      }

      return records.sort((a, b) => b.timestamp - a.timestamp);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return [];
    }
  };

  fetchIncomeHistoryRange = async (
    startTime: number,
    endTime: number,
    incomeType?: string,
  ): Promise<IncomeRecord[]> => {
    return await this.fetchIncomeHistory({ startTime, endTime, incomeType });
  };

  /**
   * Closed positions from `/api/v1/account/positions-history` — a DIFFERENT source from
   * the fills the ledger reads, which is what makes it a real cross-check.
   */
  /**
   * RAW positions-history rows. Public so the journal reads the endpoint through a method
   * rather than the harness scraping it off the URL. Dedupes on `historyId`, NOT
   * `positionId` — a live sample had 20 rows across 2 positionIds, so keying on the position
   * would delete 18 real closes. Same trap as okx's posId.
   */
  fetchClosedPositionRows = async (opts: HistoricalTradeOptions): Promise<Array<Record<string, any>>> => {
    if (this.isPublicDataOnly) return [];

    const market = opts.symbol ? this.store.marketsBySymbol[opts.symbol] : undefined;
    if (opts.symbol && !market) return [];

    const MAX_PAGES = 100;
    const PAGE_LIMIT = 100;
    const collected: Array<Record<string, any>> = [];
    const seen = new Set<string>();
    let after: string | undefined;

    for (let page = 0; page < MAX_PAGES; page += 1) {
      const params = omitUndefined({
        instId: market?.id,
        begin: opts.from,
        end: opts.to,
        limit: PAGE_LIMIT,
        after,
      });
      const { data } = await this.xhr.get(ENDPOINTS.POSITIONS_HISTORY, { params });
      const rows: Array<Record<string, any>> = Array.isArray(data?.data) ? data.data : [];
      if (rows.length === 0) break;

      let added = 0;
      for (const row of rows) {
        const id = String(row.historyId ?? '');
        if (id && seen.has(id)) continue;
        if (id) seen.add(id);
        added += 1;
        collected.push(row);
      }

      if (added === 0 || rows.length < PAGE_LIMIT) break;
      const next = oldestCursorId(rows, 'historyId') ?? '';
      if (!next || next === after) break;
      after = next;
    }

    return collected;
  };

  /** Journal records from the REPORTED closed axis — blofin folds to nothing from fills. */
  fetchHistoricalPositions = async (opts: { account: string; from?: number; to?: number }) => {
    const rows = await this.fetchClosedPositionRows({ from: opts.from, to: opts.to } as HistoricalTradeOptions);
    const records = [];
    for (const row of rows) {
      const record = mapBlofinPositionHistory(row, {
        account: opts.account,
        resolveSymbol: (instId: string) => this.store?.marketsById?.[instId]?.symbol,
        resolveSettleCurrency: (instId: string) => this.store?.marketsById?.[instId]?.quote,
      });
      if (record) records.push(record);
    }
    return records;
  };

  fetchClosedPnl = async (opts: HistoricalTradeOptions): Promise<ClosedPnlEntry[]> => {
    if (this.isPublicDataOnly) {
      return [];
    }

    const market = opts.symbol ? this.store.marketsBySymbol[opts.symbol] : undefined;
    if (opts.symbol && !market) {
      return [];
    }

    const resolveSymbol = (instId: string) => this.store.marketsById[instId]?.symbol;
    const MAX_PAGES = 100;
    const PAGE_LIMIT = 100;

    try {
      const entries: ClosedPnlEntry[] = [];
      // Dedupe on `historyId`, NOT `positionId`: a live sample had 20 rows across only 2
      // positionIds, so keying on the position would delete 18 real closes. Same trap as
      // okx's posId.
      const seen = new Set<string>();
      let after: string | undefined;
      let page = 0;

      for (; page < MAX_PAGES; page += 1) {
        const params = omitUndefined({
          instId: market?.id,
          begin: opts.from,
          end: opts.to,
          limit: PAGE_LIMIT,
          after,
        });
        const { data } = await this.xhr.get(ENDPOINTS.POSITIONS_HISTORY, { params });
        const rows: Array<Record<string, any>> = Array.isArray(data?.data) ? data.data : [];
        if (rows.length === 0) break;

        let added = 0;
        for (const row of rows) {
          const id = String(row.historyId ?? '');
          if (id && seen.has(id)) continue;
          if (id) seen.add(id);
          added += 1;
          const entry = mapBlofinClosedPosition(row, resolveSymbol, opts.accountName || '');
          if (entry) entries.push(entry);
        }

        if (added === 0 || rows.length < PAGE_LIMIT) break;
        const next = oldestCursorId(rows, 'historyId') ?? '';
        if (!next || next === after) break;
        after = next;
      }

      if (page >= MAX_PAGES) {
        this.log(
          `fetchClosedPnl stopped at the ${MAX_PAGES}-page cap; older positions were not fetched`,
          LogSeverity.Warning,
        );
      }

      return entries.sort((a, b) => b.closedTime - a.closedTime);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return [];
    }
  };

  /** Historical orders, including ones that never filled — see blofin.order-history.ts. */
  fetchHistoricalOrders = async (opts?: OrderHistoryOptions): Promise<HistoricalOrder[]> => {
    if (this.isPublicDataOnly) {
      return [];
    }

    const market = opts?.symbol ? this.store.marketsBySymbol[opts.symbol] : undefined;
    if (opts?.symbol && !market) {
      return [];
    }

    const resolveSymbol = (instId: string) => this.store.marketsById[instId]?.symbol;

    try {
      const { data } = await this.xhr.get(ENDPOINTS.ORDERS_HISTORY, {
        params: omitUndefined({
          instId: market?.id,
          begin: opts?.from,
          end: opts?.to,
          limit: Math.min(opts?.limit || 100, 100),
        }),
      });

      const rows: Array<Record<string, any>> = Array.isArray(data?.data) ? data.data : [];
      return rows
        .map((row) => mapBlofinHistoricalOrder(row, resolveSymbol))
        .filter((order): order is HistoricalOrder => order !== null)
        .sort((a, b) => b.createdTime - a.createdTime);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return [];
    }
  };

  fetchHistoricalFills = async (opts: ExecutionsOptions): Promise<Execution[]> => {
    if (this.isPublicDataOnly) {
      return [];
    }

    const { symbol, orderId, from, to, limit = 100 } = opts;
    if (!symbol) {
      return [];
    }

    const market = this.store.marketsBySymbol[symbol];
    if (!market) {
      return [];
    }

    const contractSize = market.contractSize || parseFloat(market.info?.contractValue || '1');
    const maxLimit = Math.min(Math.max(limit, 1), 100);

    const normalizeToMilliseconds = (value?: number) => {
      if (value === undefined || value === null || !Number.isFinite(value)) {
        return undefined;
      }
      return value > 1_000_000_000_000 ? Math.floor(value) : Math.floor(value * 1000);
    };

    const normalizeToSeconds = (value: unknown) => {
      const parsed = Number.parseInt(String(value ?? '0'), 10);
      if (!Number.isFinite(parsed) || parsed <= 0) {
        return undefined;
      }
      return parsed > 1_000_000_000_000 ? Math.floor(parsed / 1000) : parsed;
    };

    const fromMilliseconds = normalizeToMilliseconds(from);
    const toMilliseconds = normalizeToMilliseconds(to);

    const parseFirstNumeric = (...values: unknown[]) => {
      for (const value of values) {
        if (value === undefined || value === null || value === '') continue;
        const parsed = Number.parseFloat(String(value));
        if (Number.isFinite(parsed)) {
          return parsed;
        }
      }
      return undefined;
    };

    const mapRowsToExecutions = (rows: Array<Record<string, any>>) =>
      rows.reduce<Execution[]>((acc, row) => {
        if (isKnownNonSwapInstrument(row)) {
          return acc;
        }

        const normalizedSide = String(row.side || row.actualSide || '').toLowerCase();
        const side = ORDER_SIDE[normalizedSide];
        if (!side) {
          return acc;
        }

        const rowSymbol = this.marketIdToSymbol(row.instId || row.symbol || '');
        if (rowSymbol !== market.symbol) {
          return acc;
        }

        // Keep execution parsing strict to avoid treating generic order payload
        // fields (e.g. algo sentinel size = -1) as real fills.
        const price = parseFirstNumeric(
          row.fillPrice,
          row.fillPx,
          row.avgFillPrice,
          row.averagePrice,
          row.avgPx,
          row.execPrice,
          row.executedPrice,
        );
        const qtyNativeRaw = parseFirstNumeric(
          row.fillSize,
          row.filledSize,
          row.actualSize,
          row.fillSz,
          row.tradeQty,
          row.execQty,
          row.executedQty,
          row.accFillSz,
        );
        const qtyNative = qtyNativeRaw === undefined ? undefined : Math.abs(qtyNativeRaw);
        if (
          price === undefined ||
          qtyNative === undefined ||
          !Number.isFinite(price) ||
          !Number.isFinite(qtyNative) ||
          price <= 0 ||
          qtyNative <= 0
        ) {
          return acc;
        }

        const timestamp = normalizeToSeconds(
          row.fillTime || row.execTime || row.tradeTime || row.uTime || row.updateTime || row.ts || row.ctime,
        );
        if (!timestamp) {
          return acc;
        }

        const executionOrderId = String(row.orderId || row.ordId || row.algoId || row.tpslId || '');
        if (orderId && executionOrderId && executionOrderId !== String(orderId)) {
          return acc;
        }

        const qty = multiply(qtyNative, contractSize);
        if (!Number.isFinite(qty) || qty <= 0) {
          return acc;
        }

        const feeRaw = parseFirstNumeric(row.fee, row.fillFee, row.execFee, row.tradeFee, row.actualFee);
        const commission = feeRaw !== undefined && Number.isFinite(feeRaw) ? Math.abs(feeRaw) : undefined;
        const realizedPnl = parseFirstNumeric(row.fillPnl);
        acc.push(
          omitUndefined({
            // Per-fill dedupe key. Only /trade/fills-history carries it; rows from the
            // ORDERS_HISTORY fallback are order-level and correctly leave it undefined.
            id: row.tradeId ? String(row.tradeId) : undefined,
            timestamp,
            symbol: market.symbol,
            orderId: executionOrderId || undefined,
            price,
            qty,
            side,
            commission,
            commissionAsset: row.feeCurrency || row.feeCcy || row.feeAsset || row.ccy || undefined,
            positionSide: row.positionSide ? normalizeExecutionPositionSide(row.positionSide) : undefined,
            realizedPnl: realizedPnl !== undefined && Number.isFinite(realizedPnl) ? realizedPnl : undefined,
            // Blofin fills carry no maker/liquidity field, so `maker` stays absent.
          }),
        );
        return acc;
      }, []);

    const getErrorMessage = (error: any) =>
      error?.response?.data?.msg || error?.response?.data?.message || error?.message || 'Unknown Blofin error';

    const isUnsupportedOperationError = (error: any) =>
      `${getErrorMessage(error)}`.toLowerCase().includes('operation is not supported');

    const fetchRows = async (endpoint: string) => {
      const fillsParams = omitUndefined({
        instId: this.symbolToMarketId(symbol),
        orderId,
        begin: fromMilliseconds,
        end: toMilliseconds,
        limit: maxLimit,
      });
      const ordersHistoryParams = omitUndefined({
        instId: this.symbolToMarketId(symbol),
        state: 'filled',
        begin: fromMilliseconds,
        end: toMilliseconds,
        limit: maxLimit,
      });
      const params = endpoint === ENDPOINTS.FILLS ? fillsParams : ordersHistoryParams;
      const { data } = await this.xhr.get<any>(endpoint, { params });
      const isSuccess = data?.code === '0' || data?.code === 0;
      if (!isSuccess) {
        throw new Error(data?.msg || `Failed to fetch Blofin executions from ${endpoint}`);
      }
      return Array.isArray(data?.data) ? data.data : [];
    };

    try {
      let primaryRows: Array<Record<string, any>> = [];
      try {
        primaryRows = await fetchRows(ENDPOINTS.FILLS);
      } catch (error: any) {
        if (isUnsupportedOperationError(error)) {
          this.log(
            `Blofin fills endpoint not supported for this account: ${getErrorMessage(error)}`,
            LogSeverity.Warning,
          );
          primaryRows = [];
        } else {
          throw error;
        }
      }

      let fallbackRows: Array<Record<string, any>> = [];
      if (primaryRows.length === 0) {
        try {
          fallbackRows = await fetchRows(ENDPOINTS.ORDERS_HISTORY);
        } catch (error: any) {
          if (isUnsupportedOperationError(error)) {
            this.log(`Blofin execution fallback not supported: ${getErrorMessage(error)}`, LogSeverity.Warning);
            fallbackRows = [];
          } else {
            throw error;
          }
        }
      }

      const deduped = new Map<string, Execution>();

      for (const execution of [...mapRowsToExecutions(primaryRows), ...mapRowsToExecutions(fallbackRows)]) {
        if (fromMilliseconds && execution.timestamp * 1000 < fromMilliseconds) {
          continue;
        }
        if (toMilliseconds && execution.timestamp * 1000 > toMilliseconds) {
          continue;
        }
        // Without the fill id, two genuine fills of one order at the same timestamp,
        // price and size collapse into one — the exact data loss the id exists to stop.
        const key = execution.id
          ? `id:${execution.id}`
          : `${execution.orderId || 'no-order'}:${execution.timestamp}:${execution.price}:${execution.qty}:${execution.side}`;
        deduped.set(key, execution);
      }

      return [...deduped.values()].sort((a, b) => b.timestamp - a.timestamp).slice(0, maxLimit);
    } catch (err: any) {
      this.emitAndLogError(getErrorMessage(err));
      return [];
    }
  };

  fetchOHLCV = async (opts: OHLCVOptions) => {
    const limit = Math.min(opts.limit || 1440, 1440);
    const [, amount, unit] = opts.interval.split(/(\d+)/);

    const end = opts.to ? dayjs(opts.to) : dayjs();
    const start =
      !opts.limit && opts.from ? dayjs(opts.from) : end.subtract(parseFloat(amount) * limit, unit as ManipulateType);

    const market = this.store.marketsBySymbol[opts.symbol];

    if (market) {
      try {
        const {
          data: { data },
        } = await this.xhr.get(ENDPOINTS.KLINE, {
          params: {
            instId: market.id,
            bar: INTERVAL[opts.interval],
            // limit,
            before: start.valueOf(),
            after: end.valueOf(),
          },
        });

        const candles: Candle[] = data.map((c: string[]) => {
          return {
            timestamp: parseInt(c[0], 10) / 1000,
            open: parseFloat(c[1]),
            high: parseFloat(c[2]),
            low: parseFloat(c[3]),
            close: parseFloat(c[4]),
            volume: parseFloat(c[7]),
          };
        });

        candles.sort((a, b) => a.timestamp - b.timestamp);

        return candles;
      } catch (err: any) {
        this.emitAndLogError(err?.response?.data?.msg || err?.message);
        return [];
      }
    }

    this.emitAndLogError(`Market ${opts.symbol} not found on Blofin`);
    return [];
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    return this.publicWebsocket.listenOHLCV(opts, callback);
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    return this.publicWebsocket.listenOrderBook(symbol, callback);
  };

  updateOrder = async ({ order, update }: UpdateOrderOpts) => {
    if (order.type === OrderType.Conditional) {
      const raw = order.info as Record<string, any> | undefined;
      if (raw?.orderType !== 'trigger' || String(raw?.algoId || '') !== order.id) {
        const message = 'Cannot safely update a BloFin trigger without authoritative trigger-order provenance';
        this.emitAndLogError(message);
        throw new Error(message);
      }

      const rawOrderPrice = Number(raw.orderPrice);
      const attached = Array.isArray(raw.attachAlgoOrders) ? raw.attachAlgoOrders[0] : undefined;
      const replacement: PlaceOrderOpts = {
        symbol: order.symbol,
        type: OrderType.Conditional,
        side: order.side,
        amount: 'amount' in update ? update.amount : (order.extra?.remainingNative ?? order.amount),
        price: Number.isFinite(rawOrderPrice) && rawOrderPrice > 0 ? rawOrderPrice : undefined,
        triggerPrice: 'price' in update ? update.price : order.price,
        stopLoss: this.parsePositivePrice(attached?.slTriggerPrice),
        takeProfit: this.parsePositivePrice(attached?.tpTriggerPrice),
        reduceOnly: false,
        timeInForce: OrderTimeInForce.GoodTillCancel,
        slTriggerType: this.mapBlofinTriggerType(attached?.slTriggerPriceType),
        tpTriggerType: this.mapBlofinTriggerType(attached?.tpTriggerPriceType),
      };
      const payload = await this.formatTriggerOrder(replacement, raw);
      await this.cancelTriggerOrders([order]);
      return [await this.placeTriggerOrder(payload)];
    }

    const realId = order.id.split('__')[0];
    const relativeOrders = this.store.orders.filter((o) => o.id.startsWith(realId));
    const orderAmountNative = order.extra?.remainingNative ?? order.amount;

    let updatePrice;
    let updateAmount;
    if ('price' in update) updatePrice = update.price;
    if ('amount' in update) updateAmount = update.amount;

    const newOrder: Writable<PlaceOrderOpts> = {
      ...order,
      symbol: order.symbol,
      type: order.type,
      side: order.side,
      // BloFin place payload expects native contracts. Prefer mapped native remaining when available.
      amount: updateAmount ?? orderAmountNative,
      price: updatePrice ?? order.price,
      reduceOnly: order.reduceOnly || false,
    };

    if (relativeOrders.length > 1) {
      const parent = relativeOrders.find((o) => !o.id.includes('__'));
      const stoploss = relativeOrders.find((o) => o.id.endsWith('__stop_loss'));
      const takeprofit = relativeOrders.find((o) => o.id.endsWith('__take_profit'));
      if (parent && parent.status === OrderStatus.Open) {
        const parentAmountNative = parent.extra?.remainingNative ?? parent.amount;
        newOrder.type = parent.type;
        newOrder.side = parent.side;
        newOrder.amount = parentAmountNative ?? newOrder.amount;
        newOrder.price = parent.price;
        newOrder.reduceOnly = parent.reduceOnly;
      } else {
        newOrder.price = undefined;
        // When only virtual TP/SL legs exist (no parent normal order in store),
        // preserve the edited leg type instead of always coercing to StopLoss.
        if (order.id.endsWith('__take_profit')) {
          newOrder.type = OrderType.TakeProfit;
        } else if (order.id.endsWith('__stop_loss')) {
          newOrder.type = OrderType.StopLoss;
        }
      }
      if (stoploss?.price) newOrder.stopLoss = stoploss.price;
      if (stoploss?.slTriggerType) newOrder.slTriggerType = stoploss.slTriggerType;
      if (takeprofit?.price) newOrder.takeProfit = takeprofit.price;
      if (takeprofit?.tpTriggerType) newOrder.tpTriggerType = takeprofit.tpTriggerType;
      if (updatePrice !== undefined) {
        if (order.id.endsWith('__stop_loss')) newOrder.stopLoss = updatePrice;
        else if (order.id.endsWith('__take_profit')) newOrder.takeProfit = updatePrice;
        else newOrder.price = updatePrice;
      }
    }
    // if (newOrder.amount === 0 && (newOrder.type === OrderType.StopLoss || newOrder.type === OrderType.TakeProfit)) {
    //   newOrder.amount = -1;
    // }

    const isVirtualTpSl = order.id.includes('__');
    if (!isVirtualTpSl && order.type !== OrderType.Limit) {
      await this.cancelAlgoOrders(relativeOrders);
    } else {
      await this.cancelOrders(relativeOrders);
    }
    return await this.placeOrder(newOrder);
  };

  cancelOrders = async (_orders: Order[]) => {
    // Check if we're canceling a single TP/SL order with amount=0
    let orderToPreserve: Order | undefined;
    if (_orders.length === 1 && _orders[0].id.includes('__') && (!_orders[0].amount || _orders[0].amount === 0)) {
      const cancelingOrder = _orders[0];
      const realId = cancelingOrder.id.split('__')[0];
      const relativeOrders = this.store.orders.filter((o) => o.id.startsWith(realId));

      const isCancelingTp = cancelingOrder.id.endsWith('__take_profit');
      const isCancelingSl = cancelingOrder.id.endsWith('__stop_loss');

      if ((isCancelingTp || isCancelingSl) && relativeOrders.length > 1) {
        // Find the opposite order that should be preserved
        const oppositeOrder = relativeOrders.find((o) => {
          if (isCancelingTp) {
            return o.id.endsWith('__stop_loss') && (!o.amount || o.amount === 0);
          } else {
            return o.id.endsWith('__take_profit') && (!o.amount || o.amount === 0);
          }
        });

        if (oppositeOrder) {
          orderToPreserve = oppositeOrder;
        }
      }
    }

    // replace child orders with parent and remove dupes introduced
    const orders = uniqBy(
      _orders.map((o) => {
        const isVirtualTpSl = o.id.includes('__');
        if (isVirtualTpSl) {
          const realId = o.id.split('__')[0];
          return (
            this.store.orders.find((o) => o.id === realId) || {
              ...o,
              id: realId,
            }
          );
        }
        return o;
      }),
      'id',
    );

    const [triggerOrders, nonTriggerOrders] = partition(orders, (o) => o.type === OrderType.Conditional);
    const [algoOrders, normalOrders] = partition(nonTriggerOrders, (o) => this.isAlgoOrder(o.type));

    if (normalOrders.length) await this.cancelNormalOrders(normalOrders);
    if (algoOrders.length) await this.cancelAlgoOrders(algoOrders);
    if (triggerOrders.length) await this.cancelTriggerOrders(triggerOrders);

    // If we need to preserve an order, re-create it
    if (orderToPreserve) {
      const preserveOrder: PlaceOrderOpts = {
        symbol: orderToPreserve.symbol,
        side: orderToPreserve.side,
        type: orderToPreserve.type,
        price: orderToPreserve.price,
        amount: 0, // Full position amount
        reduceOnly: true,
      };

      // Add trigger type if available
      if (orderToPreserve.type === OrderType.StopLoss && orderToPreserve.slTriggerType) {
        (preserveOrder as any).slTriggerType = orderToPreserve.slTriggerType;
      } else if (orderToPreserve.type === OrderType.TakeProfit && orderToPreserve.tpTriggerType) {
        (preserveOrder as any).tpTriggerType = orderToPreserve.tpTriggerType;
      }

      // Place the preserved order after a short delay to ensure the cancellation is processed
      await sleep(500);
      await this.placeOrder(preserveOrder);
    }
  };

  cancelSymbolOrders = async (symbol: string) => {
    const orders = this.store.orders.filter((o) => o.symbol === symbol && o.status === OrderStatus.Open);
    await this.cancelOrders(orders);
  };

  setLeverage = async (symbol: string, inputLeverage: number, _side?: PositionSide) => {
    const market = this.store.marketsBySymbol[symbol];
    if (!market) throw new Error(`Market ${symbol} not found on Blofin`);

    const leverage = Math.min(Math.max(inputLeverage, market.limits.leverage.min), market.limits.leverage.max);

    // Skip the round-trip if we already know the requested leverage is set.
    // Prefer position records (live truth from WS); fall back to the per-symbol
    // fetchedLeverage cache when there's no open position for this symbol.
    const existingPositions = this.store.positions.filter((p) => p.symbol === symbol);
    if (existingPositions.length > 0 && existingPositions.every((p) => p.leverage === leverage)) {
      return;
    }
    const fetchedLev = this.store.fetchedLeverage[symbol];
    if (
      existingPositions.length === 0 &&
      fetchedLev &&
      !fetchedLev.isFetching &&
      fetchedLev.buyLeverage === leverage &&
      fetchedLev.sellLeverage === leverage
    ) {
      return;
    }

    // Both margin type and position mode are account-global on Blofin, so the
    // cached values populated during `start()` are reused here without REST.
    const [marginType, positionMode] = await Promise.all([this.getOrFetchMarginType(), this.getOrFetchPositionMode()]);

    const baseRequest = {
      instId: market.id,
      leverage: `${leverage}`,
      marginMode: marginType === MarginType.Isolated ? 'isolated' : 'cross',
    };

    let datas: Array<{ code: string; msg: string }>;
    if (marginType === MarginType.Isolated && positionMode === PositionMode.Hedged) {
      // Hedged isolated requires per-side leverage updates; fire them in parallel
      // to stay well within the worker-call timeout budget.
      const [longRes, shortRes] = await Promise.all([
        this.xhr.post(ENDPOINTS.SET_LEVERAGE, { ...baseRequest, positionSide: 'long' }),
        this.xhr.post(ENDPOINTS.SET_LEVERAGE, { ...baseRequest, positionSide: 'short' }),
      ]);
      datas = [longRes.data, shortRes.data];
    } else {
      const { data } = await this.xhr.post(ENDPOINTS.SET_LEVERAGE, omitUndefined(baseRequest));
      datas = [data];
    }

    // Surface API-level errors immediately, before any optimistic store mutation.
    for (const data of datas) {
      if (data.code !== '0') {
        this.emitAndLogError(data.msg);
        throw new Error(data.msg);
      }
    }

    // Optimistically patch the store so the UI reflects the new leverage right away.
    // The WS positions stream will deliver the authoritative value shortly, and the
    // fetchedLeverage cache covers the no-open-position fallback used by the place-order
    // % slider.
    if (existingPositions.length > 0) {
      const updates = existingPositions.map((p) => ({ ...p, leverage }));
      this.store.addOrUpdatePositions(updates);
    }
    this.store.update({
      fetchedLeverage: {
        ...this.store.fetchedLeverage,
        [symbol]: { buyLeverage: leverage, sellLeverage: leverage, marginType, positionMode },
      },
    });
  };

  placeOrder = async (opts: PlaceOrderOpts) => {
    this.resolveCallerClientOrderId(opts);
    if (opts.type === OrderType.Conditional) {
      return [await this.placeTriggerOrder(await this.formatTriggerOrder(opts))];
    }

    if (this.isAlgoOrder(opts.type)) {
      const payload = await this.formatAlgoOrder(opts);
      return await this.placeAlgoOrderBatch([payload]);
    }

    const payloads = await this.formatNormalOrder(opts);

    if (payloads.some((o) => parseFloat(o.size) === 0)) {
      this.emitAndLogError(`Order amount is too small`);
      throw new Error(`Order amount is too small`);
    }

    return await this.placeOrderBatch(payloads);
  };

  placeOrders = async (opts: PlaceOrderOpts[]) => {
    this.assertCallerClientOrderIds(opts);
    const [triggerOrders, nonTriggerOrders] = partition(opts, (o) => o.type === OrderType.Conditional);
    const [algoOrders, normalOrders] = partition(nonTriggerOrders, (o) => this.isAlgoOrder(o.type));

    // force fetch here before map
    await Promise.all([this.getOrFetchPositionMode(), this.getOrFetchMarginType()]);
    const normalOrdersOpts = (await Promise.all(normalOrders.map((o) => this.formatNormalOrder(o)))).flat();

    if (normalOrdersOpts.some((o) => parseFloat(o.size) === 0)) {
      this.emitAndLogError(`Total order amount is too small`);
      return [];
    }

    // Blofin supports TP/SL fields directly on normal order payloads.
    // Deriving extra algo orders from the same normal orders duplicates TP/SL
    // placement and can cause trigger validation failures in batch flow.
    const algoOrdersOpts = await Promise.all(algoOrders.map((o) => this.formatAlgoOrder(o)));

    // For batch orders, we don't have the original opts context, so we use the basic batch method
    const triggerOrderIds = await mapSeries(triggerOrders, async (order) =>
      this.placeTriggerOrder(await this.formatTriggerOrder(order)),
    );
    return [
      ...(await this.placeOrderBatch(normalOrdersOpts)),
      ...(await this.placeAlgoOrderBatch(algoOrdersOpts)),
      ...triggerOrderIds,
    ];
  };

  mapOrders = (orders: Array<Record<string, any>>) => {
    return orders.reduce<Order[]>((acc, o: Record<string, any>) => {
      if (isKnownNonSwapInstrument(o)) return acc;

      const market = this.store.marketsById[o.instId];
      if (!market) return acc;

      const tmpOrders: Order[] = [];
      // BloFin echoes inline TP/SL prices on both the normal order record
      // (orders-pending) and the auto-generated algo record
      // (orders-tpsl-pending). Emitting virtual TP/SL from both produces two
      // protective stops per placement (one keyed by orderId, one by tpslId).
      // Algo records are the authoritative source — they carry the tpslId
      // needed to cancel, and they map 1:1 with the trigger order BloFin
      // actually placed. Only emit from algo records.
      const triggerPrice = parseFloat(o.triggerPrice || '0');
      const isReduceOnlyTrigger = o.reduceOnly === true || o.reduceOnly === 'true';
      const isOpeningTriggerRecord =
        o.orderType === 'trigger' &&
        !!o.algoId &&
        !isReduceOnlyTrigger &&
        Number.isFinite(triggerPrice) &&
        triggerPrice > 0;
      const isAlgoRecord = !!o.tpslId && !isOpeningTriggerRecord;

      const orderSide = ORDER_SIDE[o.side];
      const inverseSide = orderSide === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy;
      // o.size is already the contract count from the API, convert to base currency amount
      const contractValue = market.contractSize || parseFloat(market.info.contractValue) || 1;
      const amountNative = parseFloat(o.size || '0');
      const amount = multiply(amountNative, contractValue);

      if (isOpeningTriggerRecord) {
        tmpOrders.push({
          id: String(o.algoId),
          status:
            o.state === 'canceled' || o.state === 'order_failed'
              ? OrderStatus.Canceled
              : o.state === 'effective'
                ? OrderStatus.Closed
                : OrderStatus.Open,
          symbol: market.symbol,
          type: OrderType.Conditional,
          side: orderSide,
          price: triggerPrice,
          amount,
          filled: 0,
          remaining: amount,
          extra: { remainingNative: amountNative },
          reduceOnly: false,
          lastUpdate: virtualClock.getCurrentTimeMs(),
          info: o,
        });
      }

      if (isAlgoRecord && o.tpTriggerPrice) {
        tmpOrders.push({
          id: `${o.tpslId}__take_profit`,
          status: OrderStatus.Open,
          symbol: market.symbol,
          type: OrderType.TakeProfit,
          side: o.orderId ? inverseSide : orderSide,
          price: parseFloat(o.tpTriggerPrice),
          amount: amount,
          filled: 0,
          remaining: amount,
          extra: {
            remainingNative: amountNative,
          },
          reduceOnly: true,
          lastUpdate: virtualClock.getCurrentTimeMs(),
          info: o,
        });
      }

      if (isAlgoRecord && o.slTriggerPrice) {
        tmpOrders.push({
          id: `${o.tpslId}__stop_loss`,
          status: OrderStatus.Open,
          symbol: market.symbol,
          type: OrderType.StopLoss,
          side: o.orderId ? inverseSide : orderSide,
          price: parseFloat(o.slTriggerPrice),
          amount: amount,
          filled: 0,
          remaining: amount,
          extra: {
            remainingNative: amountNative,
          },
          reduceOnly: true,
          lastUpdate: virtualClock.getCurrentTimeMs(),
          info: o,
        });
      }

      // Only emit the main Limit from true normal records. An algo record
      // (has tpslId) must never produce a Limit, even if its payload happens
      // to set orderCategory='normal' and carry an orderId reference.
      if (o.orderId && o.orderCategory === 'normal' && !isAlgoRecord) {
        // o.filledSize is already the contract count from the API, convert to base currency amount
        const filledNative = parseFloat(o.filledSize || '0');
        const filled = multiply(filledNative, contractValue);

        const remaining = subtract(amount, filled);
        const remainingNative = subtract(amountNative, filledNative);

        tmpOrders.push({
          id: o.orderId,
          status: ORDER_STATUS[o.state],
          symbol: market.symbol,
          type: ORDER_TYPE[o.orderType],
          side: ORDER_SIDE[o.side],
          price: parseFloat(o.price),
          amount,
          filled,
          remaining,
          extra: {
            remainingNative,
            filledInPlacementUnits: filledNative,
          },
          reduceOnly: o.reduceOnly === 'true',
          lastUpdate: virtualClock.getCurrentTimeMs(),
          timeInForce: o.orderType === 'post_only' ? OrderTimeInForce.PostOnly : OrderTimeInForce.GoodTillCancel,
          info: o,
        });
      }

      return [...acc, ...tmpOrders];
    }, []);
  };

  mapPositions = (data: FETCH_POSITION_RESPONSE['data']) => {
    const supportedPositions = data.filter((p) => !isKnownNonSwapInstrument(p));

    // check if iso and cross position open at same time for same symbol
    // const positionMode = this._accountPositionMode;
    // const marginMode = this._accountMarginType;
    if (
      supportedPositions.some((p) =>
        supportedPositions.some(
          (p2) =>
            (p2.marginMode !== p.marginMode || (p2.positionSide === 'net' && p.positionSide !== 'net')) &&
            p.instId === p2.instId &&
            p.positionSide === p2.positionSide &&
            p.positions !== p2.positions,
        ),
      )
    ) {
      this.emitAndLogError(
        `Isolated and cross positions open at the same time for ${data[0].instId}. Close one on Blofin site to use Tealstreet.`,
      );
    }

    return supportedPositions.reduce((acc: Position[], p) => {
      const market = this.store.marketsById[p.instId];
      if (!market) return acc;

      const pos = parseFloat(p.positions);
      // p.positions is already the contract count from the API, convert to base currency amount
      const contractValue = market.contractSize || parseFloat(market.info.contractValue) || 1;
      const contracts = pos && contractValue ? multiply(pos, contractValue) : 0;

      const markPrice = parseFloat(p.markPrice);
      const notionalUsd = multiply(contracts, markPrice);

      let side = POSITION_SIDE[p.positionSide];

      if (contracts > 0) side = PositionSide.Long;
      if (contracts < 0) side = PositionSide.Short;

      // FALLBACK TO LONG POSITION IF SENT FROM OKX
      // THIS SHOULD NOT OVERRIDE THE "VIRTUAL SHORT" POSITION
      if (!side) side = PositionSide.Long;
      const positionMode = p.positionSide === 'net' ? PositionMode.OneWay : PositionMode.Hedged;

      // force position side if api explicitly says if it's short or long
      side = p.positionSide === 'short' ? PositionSide.Short : p.positionSide === 'long' ? PositionSide.Long : side;

      const positionId = createPositionId(market.symbol, side, positionMode);
      const existingPosition = this.store.positions.find((sp) => sp.id === positionId);

      // Preserve a previously-known leverage when the payload omits it (or sends 0/NaN).
      // Blofin occasionally pushes partial position events without a usable `leverage`,
      // and writing 0 here causes the place-order % slider to jitter via the React fallback.
      const parsedLeverage = parseFloat(p.leverage);
      let leverage = Number.isFinite(parsedLeverage) && parsedLeverage > 0 ? parsedLeverage : 0;
      if (!leverage) {
        if (existingPosition && existingPosition.leverage > 0) {
          leverage = existingPosition.leverage;
        } else {
          const fetched = this.store.fetchedLeverage[market.symbol];
          if (fetched && !fetched.isFetching) {
            leverage = side === PositionSide.Long ? fetched.buyLeverage : fetched.sellLeverage;
          }
        }
      }
      const parsedLiquidationPrice = parseFloat(p.liquidationPrice);
      const liquidationPrice = Number.isFinite(parsedLiquidationPrice)
        ? parsedLiquidationPrice
        : existingPosition?.liquidationPrice || 0;

      const position: Position = {
        id: positionId,
        symbol: market.symbol,
        side,
        entryPrice: contracts ? parseFloat(p.averagePrice) : 0,
        notional: contracts ? Math.abs(adjust(parseFloat(String(notionalUsd)), market.precision.price)) : 0,
        leverage,
        unrealizedPnl: contracts ? adjust(parseFloat(p.unrealizedPnl), market.precision.price) : 0,
        contracts: Math.abs(contracts),
        liquidationPrice,
        marginType: p.marginMode === 'isolated' ? MarginType.Isolated : MarginType.Cross,
        positionMode,
        extra: {
          contractsNative: Math.abs(pos),
        },
        info: p,
      };

      return [...acc, position];
    }, []);
  };

  changePositionMode = async (hedged: boolean, _symbol?: string) => {
    const targetMode = hedged ? 'long_short_mode' : 'net_mode';
    this.log(`Attempting to change position mode to ${targetMode}`, LogSeverity.Info);
    this.log(
      `Current positions count: ${this.store.positions.filter((p) => p.contracts > 0).length}`,
      LogSeverity.Info,
    );
    this.log(`Current orders count: ${this.store.orders.length}`, LogSeverity.Info);

    const { data } = await this.xhr.post(ENDPOINTS.SET_POSITION_MODE, {
      positionMode: targetMode,
    });

    // {
    //   "code": "0",
    //   "msg": "success",
    //   "data": {
    //   "positionMode": "net_mode"
    // }
    // }

    // Check for errors BEFORE updating positions
    if (data.code !== '0') {
      this.log(`Failed to change position mode: ${data.msg} (code: ${data.code})`, LogSeverity.Error);
      this.emitAndLogError(`${data.msg} (BloFin error code: ${data.code})`);
      throw new Error(`${data.msg} (BloFin error code: ${data.code})`);
    }

    const confirmedMode = data.data?.positionMode === 'net_mode' ? PositionMode.OneWay : PositionMode.Hedged;
    this.applyConfirmedPositionMode(confirmedMode);
    this.log(`Position mode change successful: ${data.data?.positionMode ?? targetMode}`, LogSeverity.Info);
  };

  changeMarginType = async (isolated: boolean, symbol?: string) => {
    // if positions / orders open emit warning
    if (
      this.store.positions.filter((p) => p.contracts > 0).length ||
      this.store.orders.filter((o) => o.status === OrderStatus.Open).length
    ) {
      this.emitAndLogError(
        'Changing margin type with open positions / orders is allowed but can cause unexpected behavior.' +
          ' Symbols with open positions will remain synced to previous mode until position is closed.',
      );
    }

    const { data } = await this.xhr.post(ENDPOINTS.SET_MARGIN_MODE, {
      marginMode: isolated ? 'isolated' : 'cross',
    });

    if (data.code !== '0') {
      this.emitAndLogError(data.msg);
      throw new Error(data.msg);
    }

    const confirmedMarginType =
      data.data?.marginMode === 'isolated'
        ? MarginType.Isolated
        : data.data?.marginMode === 'cross'
          ? MarginType.Cross
          : isolated
            ? MarginType.Isolated
            : MarginType.Cross;
    this.applyConfirmedMarginType(confirmedMarginType);

    await this.getOrFetchMarginType(undefined, true);
    if (symbol) {
      const positions = await this.fetchPositions([symbol]);
      this.store.addOrUpdatePositions(positions);
    }

    // {
    //   "code": "0",
    //   "msg": "success",
    //   "data": {
    //   "marginMode": "isolated"
    // }
    // }
  };

  afterMethodCalledInWorker = <M extends CallableMethodsInWorker>(
    result: ReturnType<this[M]>,
    method: M,
    ...args: Parameters<this[M]>
  ) => {
    if (method === 'changePositionMode') {
      const confirmedMode =
        typeof result === 'object' &&
        result !== null &&
        'positionMode' in result &&
        (result as { positionMode?: PositionMode }).positionMode
          ? (result as { positionMode: PositionMode }).positionMode
          : args[0]
            ? PositionMode.Hedged
            : PositionMode.OneWay;
      this.applyConfirmedPositionMode(confirmedMode);
    } else if (method === 'changeMarginType') {
      this.applyConfirmedMarginType(args[0] ? MarginType.Isolated : MarginType.Cross);
    }
  };

  private applyConfirmedMarginType = (marginType: MarginType) => {
    this._accountMarginType = marginType;

    const fetchedLeverageEntries = Object.entries(this.store.fetchedLeverage);
    const positions = this.store.positions;
    const hasFlatPositionUpdates = positions.some(
      (position) => Number(position.contracts) === 0 && position.marginType !== marginType,
    );
    if (!fetchedLeverageEntries.length && !hasFlatPositionUpdates) {
      return;
    }

    this.store.update({
      ...(fetchedLeverageEntries.length
        ? {
            fetchedLeverage: Object.fromEntries(
              fetchedLeverageEntries.map(([symbol, leverage]) => [symbol, { ...leverage, marginType }]),
            ),
          }
        : {}),
      ...(hasFlatPositionUpdates
        ? {
            positions: positions.map((position) =>
              Number(position.contracts) === 0 ? { ...position, marginType } : position,
            ),
          }
        : {}),
    });
  };

  private applyConfirmedPositionMode = (positionMode: PositionMode) => {
    this._accountPositionMode = positionMode;

    const fetchedLeverageEntries = Object.entries(this.store.fetchedLeverage);
    const positions = this.store.positions;
    if (!fetchedLeverageEntries.length && !positions.length) {
      return;
    }

    this.store.update({
      ...(fetchedLeverageEntries.length
        ? {
            fetchedLeverage: Object.fromEntries(
              fetchedLeverageEntries.map(([symbol, leverage]) => [symbol, { ...leverage, positionMode }]),
            ),
          }
        : {}),
      ...(positions.length
        ? {
            positions: uniqBy(
              positions.map((position) => ({
                ...position,
                id: createPositionId(position.symbol, position.side, positionMode),
                positionMode,
              })),
              'id',
            ),
          }
        : {}),
    });
  };

  private parsePositivePrice = (value: unknown) => {
    const parsed = Number(value);
    return Number.isFinite(parsed) && parsed > 0 ? parsed : undefined;
  };

  private mapBlofinTriggerType = (value: unknown): OrderStopTriggerType | undefined => {
    if (value === 'last') return OrderStopTriggerType.LastPrice;
    if (value === 'mark') return OrderStopTriggerType.MarkPrice;
    if (value === 'index') return OrderStopTriggerType.IndexPrice;
    return undefined;
  };

  private toBlofinTriggerType = (value: OrderStopTriggerType | undefined) => {
    if (value === OrderStopTriggerType.MarkPrice) return 'mark';
    if (value === OrderStopTriggerType.IndexPrice) return 'index';
    return 'last';
  };

  private formatTriggerOrder = async (opts: PlaceOrderOpts, source?: Record<string, any>) => {
    const market = this.store.marketsBySymbol[opts.symbol];
    if (!market) {
      throw new Error(`Market ${opts.symbol} not found on Blofin`);
    }
    if (opts.reduceOnly || opts.close) {
      throw new Error('BloFin opening trigger orders cannot be reduce-only or close-only');
    }
    if (opts.timeInForce && opts.timeInForce !== OrderTimeInForce.GoodTillCancel) {
      throw new Error('BloFin opening trigger orders support only GoodTillCancel semantics');
    }

    const lotSize = market.info.lotSize ? Number(market.info.lotSize) : Number(market.info.minSize);
    const amount = adjust(opts.amount, lotSize);
    const triggerPrice = opts.triggerPrice ? adjust(opts.triggerPrice, market.precision.price) : 0;
    const orderPrice = opts.price ? adjust(opts.price, market.precision.price) : -1;
    if (!Number.isFinite(amount) || amount <= 0) {
      throw new Error('BloFin opening trigger amount must be positive');
    }
    if (!Number.isFinite(triggerPrice) || triggerPrice <= 0) {
      throw new Error(`Trigger price is required for conditional BloFin orders (${opts.symbol})`);
    }

    const sourceInstId = source?.instId;
    if (source && sourceInstId !== market.id) {
      throw new Error('Cannot replace a BloFin trigger whose instrument identity changed');
    }
    const marginMode =
      source?.marginMode === 'cross' || source?.marginMode === 'isolated'
        ? source.marginMode
        : await this.getOrFetchMarginType(market.symbol);
    const positionSide =
      source?.positionSide === 'net' || source?.positionSide === 'long' || source?.positionSide === 'short'
        ? source.positionSide
        : await this.getPositionSide(opts);
    const triggerPriceType = ['last', 'mark', 'index'].includes(source?.triggerPriceType)
      ? source!.triggerPriceType
      : 'last';

    const attached: Record<string, string> = {};
    if (opts.takeProfit) {
      attached.tpTriggerPrice = `${adjust(opts.takeProfit, market.precision.price)}`;
      attached.tpOrderPrice = '-1';
      attached.tpTriggerPriceType = this.toBlofinTriggerType(opts.tpTriggerType);
    }
    if (opts.stopLoss) {
      attached.slTriggerPrice = `${adjust(opts.stopLoss, market.precision.price)}`;
      attached.slOrderPrice = '-1';
      attached.slTriggerPriceType = this.toBlofinTriggerType(opts.slTriggerType);
    }

    return omitUndefined({
      instId: market.id,
      marginMode,
      positionSide,
      side: inverseObj(ORDER_SIDE)[opts.side],
      size: `${amount}`,
      clientOrderId: `${BROKER_ID}${uuid()}`.slice(0, 32),
      orderPrice: `${orderPrice}`,
      orderType: 'trigger',
      triggerPrice: `${triggerPrice}`,
      triggerPriceType,
      ...(Object.keys(attached).length ? { attachAlgoOrders: [attached] } : {}),
      ...(process.env.NODE_ENV === 'production' || this.options.tealstreetProxy ? { brokerId: BROKER_ID } : {}),
    });
  };

  private placeTriggerOrder = async (payload: Record<string, any>) => {
    const { data } = await this.xhr.post<{
      code: string;
      msg?: string;
      data?: { algoId?: string; clientOrderId?: string; code?: string; msg?: string };
    }>(ENDPOINTS.PLACE_TRIGGER_ORDER, payload);
    const row = data?.data;
    const message = row?.msg || data?.msg || 'BloFin trigger placement returned an invalid response';
    if (data?.code !== '0' || (row?.code !== undefined && row.code !== '0') || !row?.algoId) {
      this.emitAndLogError(message);
      throw new Error(message);
    }
    return String(row.algoId);
  };

  private formatAlgoOrder = async (opts: PlaceOrderOpts) => {
    const market = this.store.marketsBySymbol[opts.symbol];

    if (!market) {
      throw new Error(`Market ${opts.symbol} not found on Blofin`);
    }

    // PlaceOrderOpts.amount is native contracts in app order flow.
    const lotSize = market.info.lotSize ? parseFloat(market.info.lotSize) : parseFloat(market.info.minSize);
    const amount = adjust(opts.amount, lotSize);

    const pPrice = market.precision.price;
    const price = opts.price ? adjust(opts.price, pPrice) : null;

    const positionSide = await this.getPositionSide(opts);
    const req: Record<string, any> = omitUndefined({
      instId: market.id,
      marginMode: await this.getOrFetchMarginType(market.symbol),
      clientOrderId: `${BROKER_ID}${uuid()}`.slice(0, 32),
      ...(process.env.NODE_ENV === 'production' || this.options.tealstreetProxy
        ? {
            brokerId: BROKER_ID,
          }
        : {}),
      side: inverseObj(ORDER_SIDE)[opts.side],
      positionSide,
      size: `${amount === 0 ? -1 : amount}`,
    });

    if (opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit) {
      req.reduceOnly = 'true';
    }

    if (opts.stopLoss) {
      req.slTriggerPrice = `${opts.stopLoss}`;
      req.slOrderPrice = '-1';
    } else if (opts.type === OrderType.StopLoss) {
      req.slTriggerPrice = `${price}`;
      req.slOrderPrice = '-1';
    }

    if (opts.takeProfit) {
      req.tpTriggerPrice = `${opts.takeProfit}`;
      req.tpOrderPrice = '-1';
    } else if (opts.type === OrderType.TakeProfit) {
      req.tpTriggerPrice = `${price}`;
      req.tpOrderPrice = '-1';
    }

    if (!opts.amount) {
      // if existing opposite algo order on same symbol, cancel and place as single order
      const oppositeType = opts.type === OrderType.StopLoss ? OrderType.TakeProfit : OrderType.StopLoss;
      const oppositeOrders = this.store.orders.filter(
        (o) =>
          o.symbol === opts.symbol &&
          o.type === oppositeType &&
          o.side === opts.side &&
          o.status === OrderStatus.Open &&
          !o.amount,
      );
      if (oppositeOrders.length) {
        await this.cancelAlgoOrders(oppositeOrders);
        const oppositeOrder = oppositeOrders[0];
        if (oppositeOrder.type === OrderType.StopLoss) {
          req.slTriggerPrice = `${oppositeOrder.price}`;
          req.slOrderPrice = '-1';
        } else {
          req.tpTriggerPrice = `${oppositeOrder.price}`;
          req.tpOrderPrice = '-1';
        }
      }
    }

    return req;
  };

  private formatNormalOrder = async (opts: PlaceOrderOpts) => {
    const market = this.store.marketsBySymbol[opts.symbol];

    if (!market) {
      throw new Error(`Market ${opts.symbol} not found on Blofin`);
    }

    const maxSize = market.limits.amount.max;
    const pPrice = market.precision.price;

    // PlaceOrderOpts.amount is native contracts in app order flow.
    const lotSize = market.info.lotSize ? parseFloat(market.info.lotSize) : parseFloat(market.info.minSize);
    const amount = adjust(opts.amount, lotSize);
    const price = opts.price ? adjust(opts.price, pPrice) : null;
    const marginMode = await this.getOrFetchMarginType(market.symbol);

    const callerClientOrderId = this.resolveCallerClientOrderId(opts);
    const req: Record<string, any> = omitUndefined({
      instId: market.id,
      clientOrderId: callerClientOrderId ?? `${BROKER_ID}${uuid()}`.slice(0, 32),
      side: inverseObj(ORDER_SIDE)[opts.side],
      orderType:
        opts.timeInForce === OrderTimeInForce.PostOnly && opts.type === 'limit'
          ? 'post_only'
          : REVERSE_ORDER_TYPE[opts.type],
      size: `${amount}`,
      price: opts.type === OrderType.Limit ? `${price}` : undefined,
      reduceOnly: opts.reduceOnly || undefined,
      positionSide: await this.getPositionSide(opts),
      marginMode,
      ...(process.env.NODE_ENV === 'production' || this.options.tealstreetProxy
        ? {
            brokerId: BROKER_ID,
          }
        : {}),
    });
    const attachAlgoOrder: Record<string, string> = {};

    if (opts.stopLoss) {
      req.slTriggerPrice = `${opts.stopLoss}`;
      req.slOrderPrice = `-1`;
      attachAlgoOrder.slTriggerPrice = `${opts.stopLoss}`;
      attachAlgoOrder.slOrderPrice = '-1';
    }

    if (opts.takeProfit) {
      req.tpTriggerPrice = `${opts.takeProfit}`;
      req.tpOrderPrice = `-1`;
      attachAlgoOrder.tpTriggerPrice = `${opts.takeProfit}`;
      attachAlgoOrder.tpOrderPrice = '-1';
    }

    if (Object.keys(attachAlgoOrder).length > 0) {
      // BloFin has evolved TP/SL attachment schemas over time.
      // Keep legacy top-level fields above for compatibility, and also send
      // the newer attachAlgoOrders shape so attached protection is not dropped.
      req.attachAlgoOrders = [attachAlgoOrder];
    }

    // If the order fits within maxSize, return a single payload
    if (amount <= maxSize) {
      return [
        {
          ...req,
          size: `${amount}`,
          clientOrderId: req.clientOrderId,
        },
      ];
    }

    // Otherwise, split into multiple orders
    const lots = Math.ceil(amount / maxSize);
    const rest = adjust(amount % maxSize, lotSize);
    const lotSizePerBatch = adjust((amount - rest) / lots, lotSize);

    const payloads: Array<Record<string, any>> = times(lots, () => {
      return {
        ...req,
        size: `${lotSizePerBatch}`,
        clientOrderId: `${BROKER_ID}${uuid()}`.slice(0, 32),
      };
    });

    if (rest)
      payloads.push({
        ...req,
        size: `${rest}`,
        clientOrderId: `${BROKER_ID}${uuid()}`.slice(0, 32),
      });

    this.assertSingleNativeClientOrderId(callerClientOrderId, payloads.length);
    return payloads;
  };

  private placeOrderBatch = async (payloads: Array<Record<string, any>>) => {
    // Keep batch placement for normal orders (including attached TP/SL) to avoid
    // slow sequential submissions in scaled ladders. Only force single-order
    // endpoint where BloFin requires it (reduce-only compatibility switch).
    const shouldUseSingleOrderEndpoint = (payload: Record<string, any>) =>
      USE_NON_BATCH_ENDPOINT_FOR_REDUCE_ONLY && !!payload.reduceOnly;

    const eligiblePayloads = payloads.filter((p) => !shouldUseSingleOrderEndpoint(p));
    const noneligiblePayloads = payloads.filter((p) => shouldUseSingleOrderEndpoint(p));
    const isSuccessfulPlacementMessage = (msg: unknown): boolean => {
      if (msg === undefined || msg === null) {
        return true;
      }
      const normalized = String(msg).trim().toLowerCase();
      return normalized === 'success' || normalized === 'order placed';
    };

    const responses = await mapSeries(chunk(eligiblePayloads, 20), async (batch) => {
      const {
        data: { data, code, msg },
      } = await this.xhr.post<{ code: string; msg: string; data: Array<Record<string, any>> }>(
        ENDPOINTS.PLACE_ORDERS_BATCH,
        batch,
      );

      if (code !== '0') {
        this.emitAndLogError(data?.[0]?.msg || msg);
        throw new Error(data?.[0]?.msg || msg);
      }

      return data.reduce((acc: string[], o) => {
        if (!isSuccessfulPlacementMessage(o.msg)) {
          this.emitAndLogError(o.msg);
          throw new Error(o.msg);
        }
        if (o.orderId) {
          return [...acc, o.orderId];
        }

        return acc;
      }, []);
    });

    // executing reduce-only separately from batch now
    for (const payload of noneligiblePayloads) {
      const {
        data: { data },
      } = await this.xhr.post<{ data: Array<Record<string, any>> }>(ENDPOINTS.PLACE_ORDERS, payload);

      responses.push(
        data.reduce((acc: string[], o) => {
          if (o.orderId) {
            if (!isSuccessfulPlacementMessage(o.msg)) {
              this.emitAndLogError(o.msg);
              throw new Error(o.msg);
            }
            return [...acc, o.orderId];
          }

          this.emitAndLogError(o.msg);
          throw new Error(o.msg);
        }, [] as string[]),
      );
    }

    const flatResponses = responses.flat();
    return flatResponses.filter((res: any): res is string => {
      if (typeof res === 'object') throw new Error(res.msg);
      return typeof res === 'string';
    });
  };

  private placeAlgoOrderBatch = async (payloads: Array<Record<string, any>>) => {
    // blofin doesn't support batched algo orders, so we do 1 by 1

    const responses: string[] = [];
    for (const payload of payloads) {
      const {
        data: { data, code, msg },
      } = await this.xhr.post<{ code: string; msg: string; data: any }>(ENDPOINTS.PLACE_TPSL_ORDER, payload);

      if (code !== '0') {
        const errorMessage =
          code === BLOFIN_MULTI_TRADE_POSITION_SELECTION_CODE
            ? BLOFIN_MULTI_TRADE_UNSUPPORTED_MESSAGE
            : data?.[0]?.msg || msg;
        this.emitAndLogError(errorMessage);
        throw new Error(errorMessage);
      }

      responses.push(data?.tpslId);
    }
    return responses;
  };

  private addInlineTpSlFallbackOrders = (orders: Order[]): Order[] => {
    const hasProtectiveByParentAndType = new Set<string>();

    const markProtectiveOrder = (order: Order) => {
      if (order.type !== OrderType.TakeProfit && order.type !== OrderType.StopLoss) {
        return;
      }
      const parentOrderId = String((order.info as Record<string, any> | undefined)?.orderId || '').trim();
      if (parentOrderId) {
        hasProtectiveByParentAndType.add(`${parentOrderId}:${order.type}`);
      }
      if (order.id.includes('__')) {
        const [rawParentId] = order.id.split('__');
        if (rawParentId) {
          hasProtectiveByParentAndType.add(`${rawParentId}:${order.type}`);
        }
      }
    };
    orders.forEach(markProtectiveOrder);

    const fallbackOrders: Order[] = [];
    for (const order of orders) {
      if (order.type !== OrderType.Limit || !order.id) {
        continue;
      }
      const rawInfo = (order.info || {}) as Record<string, any>;
      const isNormalOrderRecord = !rawInfo.tpslId && rawInfo.orderCategory === 'normal';
      if (!isNormalOrderRecord) {
        continue;
      }

      const remainingAmount =
        Number.isFinite(Number(order.remaining)) && Number(order.remaining) > 0 ? order.remaining : order.amount;
      if (!Number.isFinite(Number(remainingAmount)) || Number(remainingAmount) <= 0) {
        continue;
      }
      const remainingNative =
        Number.isFinite(Number(order.extra?.remainingNative)) && Number(order.extra?.remainingNative) > 0
          ? Number(order.extra?.remainingNative)
          : Number(remainingAmount);
      const protectiveSide = order.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy;
      const parentOrderId = String(order.id);

      const tpTriggerRaw = rawInfo.tpTriggerPrice;
      const tpTriggerPrice = Number.parseFloat(String(tpTriggerRaw ?? ''));
      if (Number.isFinite(tpTriggerPrice) && tpTriggerPrice > 0) {
        const key = `${parentOrderId}:${OrderType.TakeProfit}`;
        if (!hasProtectiveByParentAndType.has(key)) {
          fallbackOrders.push({
            id: `${parentOrderId}__take_profit`,
            status: OrderStatus.Open,
            symbol: order.symbol,
            type: OrderType.TakeProfit,
            side: protectiveSide,
            price: tpTriggerPrice,
            amount: remainingAmount,
            filled: 0,
            remaining: remainingAmount,
            extra: {
              remainingNative,
            },
            reduceOnly: true,
            lastUpdate: virtualClock.getCurrentTimeMs(),
            info: rawInfo,
          });
          hasProtectiveByParentAndType.add(key);
        }
      }

      const slTriggerRaw = rawInfo.slTriggerPrice;
      const slTriggerPrice = Number.parseFloat(String(slTriggerRaw ?? ''));
      if (Number.isFinite(slTriggerPrice) && slTriggerPrice > 0) {
        const key = `${parentOrderId}:${OrderType.StopLoss}`;
        if (!hasProtectiveByParentAndType.has(key)) {
          fallbackOrders.push({
            id: `${parentOrderId}__stop_loss`,
            status: OrderStatus.Open,
            symbol: order.symbol,
            type: OrderType.StopLoss,
            side: protectiveSide,
            price: slTriggerPrice,
            amount: remainingAmount,
            filled: 0,
            remaining: remainingAmount,
            extra: {
              remainingNative,
            },
            reduceOnly: true,
            lastUpdate: virtualClock.getCurrentTimeMs(),
            info: rawInfo,
          });
          hasProtectiveByParentAndType.add(key);
        }
      }
    }

    return [...orders, ...fallbackOrders];
  };

  // A blofin cancel row can come back "failed" only because the order is already
  // gone on the exchange — filled / triggered / canceled / does-not-exist (per-row
  // code "1000"). That's not a real failure: the order is no longer open, so it must
  // be reconciled out of the store rather than thrown. Otherwise a stale store order
  // makes flatten/cancel-all loops spin forever re-issuing cancels that always
  // "fail" (the order never leaves the store), which is exactly how the e2e cleanup
  // step timed out on blofin.
  private isAlreadyGoneCancelRow = (row?: { code?: string; msg?: string }) =>
    row?.code === '1000' || /filled, triggered, canceled or does not exist/i.test(row?.msg ?? '');

  private cancelNormalOrders = async (orders: Order[]) => {
    const batches = orders
      .filter((o) => !this.isAlgoOrder(o.type))
      .reduce((acc: Array<Record<string, any>>, o) => {
        const market = this.store.marketsBySymbol[o.symbol];
        if (!market) return acc;
        return [...acc, { instId: market.id, orderId: o.id }];
      }, []);

    await forEachSeries(chunk(batches, 20), async (batch) => {
      let res;
      try {
        res = await this.xhr.post(ENDPOINTS.CANCEL_ORDERS, batch);
      } catch (err: any) {
        this.emitAndLogError(err?.response?.data?.msg || err?.message);
        throw new Error(err?.response?.data?.msg || err?.message);
      }

      if (res?.data.code !== '0') {
        const rows: Array<{ orderId?: string; code?: string; msg?: string }> = res?.data?.data ?? [];
        // Rows the exchange reports as already gone *and that we can map back to a
        // store order* are removed and treated as canceled. Everything else —
        // genuine errors, and already-gone rows with no orderId we could reconcile —
        // throws, so we never silently swallow a row while leaving a stale order open.
        const realFailures: typeof rows = [];
        for (const row of rows) {
          if (this.isAlreadyGoneCancelRow(row) && row.orderId) {
            this.store.removeOrders([{ id: row.orderId }]);
          } else {
            realFailures.push(row);
          }
        }
        if (rows.length === 0 || realFailures.length > 0) {
          const msg = realFailures[0]?.msg || res?.data?.data?.[0]?.msg || res?.data?.msg;
          this.emitAndLogError(msg);
          throw new Error(msg);
        }
      }
    });
  };

  private cancelAlgoOrders = async (orders: Order[]) => {
    const batches = orders
      .filter((o) => this.isAlgoOrder(o.type))
      .reduce((acc: Array<Record<string, any>>, o) => {
        const market = this.store.marketsBySymbol[o.symbol];
        if (!market) return acc;
        const tpslId = o.id.split('__')[0];
        return [...acc, { instId: market.id, tpslId }];
      }, []);

    await forEachSeries(chunk(batches, 10), async (batch) => {
      let res;
      try {
        res = await this.xhr.post(ENDPOINTS.CANCEL_ALGO_ORDER, batch);
      } catch (err: any) {
        this.emitAndLogError(err?.response?.data?.msg || err?.message);
        throw new Error(err?.response?.data?.msg || err?.message);
      }
      if (res?.data.code !== '0') {
        const rows: Array<{ tpslId?: string; algoId?: string; code?: string; msg?: string }> = res?.data?.data ?? [];
        // Algo orders the exchange reports as already gone are reconciled out of the
        // store. Store ids are `${tpslId}__take_profit` / `__stop_loss`; match on the
        // exact tpslId segment (`id.split('__')[0]`), not a raw prefix, so `12` can't
        // sweep away `123`'s legs. Genuine errors and already-gone rows with no
        // reconcile id throw.
        const realFailures: typeof rows = [];
        for (const row of rows) {
          const goneId = row.tpslId || row.algoId;
          if (this.isAlreadyGoneCancelRow(row) && goneId) {
            const storeIds = this.store.orders.filter((o) => o.id.split('__')[0] === goneId).map((o) => ({ id: o.id }));
            if (storeIds.length) this.store.removeOrders(storeIds);
          } else {
            realFailures.push(row);
          }
        }
        if (rows.length === 0 || realFailures.length > 0) {
          const msg = realFailures[0]?.msg || res?.data?.data?.[0]?.msg || res?.data?.msg;
          this.emitAndLogError(msg);
          throw new Error(msg);
        }
      }
      await sleep(1_000);
    });
  };

  private cancelTriggerOrders = async (orders: Order[]) => {
    const payloads = orders.map((order) => {
      const market = this.store.marketsBySymbol[order.symbol];
      const raw = order.info as Record<string, any> | undefined;
      const algoId = String(raw?.algoId || order.id || '');
      if (!market || raw?.orderType !== 'trigger' || raw?.instId !== market.id || algoId !== order.id) {
        throw new Error(`Cannot safely cancel BloFin trigger order ${order.id}: authoritative provenance is missing`);
      }
      return { instId: market.id, algoId };
    });

    await forEachSeries(payloads, async (payload) => {
      let response;
      try {
        response = await this.xhr.post<{
          code: string;
          msg?: string;
          data?: { algoId?: string; clientOrderId?: string; code?: string; msg?: string };
        }>(ENDPOINTS.CANCEL_TRIGGER_ORDER, payload);
      } catch (err: any) {
        const message = err?.response?.data?.msg || err?.message;
        this.emitAndLogError(message);
        throw new Error(message);
      }

      const row = response.data?.data;
      if (this.isAlreadyGoneCancelRow(row) && row?.algoId === payload.algoId) {
        this.store.removeOrders([{ id: payload.algoId }]);
      } else if (
        response.data?.code !== '0' ||
        (row?.code !== undefined && row.code !== '0') ||
        row?.algoId !== payload.algoId
      ) {
        const message = row?.msg || response.data?.msg || 'BloFin trigger cancellation returned an invalid response';
        this.emitAndLogError(message);
        throw new Error(message);
      }

      // BloFin's documented trade limit is 30 requests per 10 seconds per user.
      await sleep(350);
    });
  };

  private isAlgoOrder = (orderType: OrderType) => {
    return (
      orderType === OrderType.StopLoss || orderType === OrderType.TakeProfit || orderType === OrderType.TrailingStopLoss
    );
  };

  private getPositionSide = async (opts: Pick<PlaceOrderOpts, 'reduceOnly' | 'side' | 'type' | 'symbol'>) => {
    if ((await this.getOrFetchPositionMode(opts.symbol)) === PositionMode.OneWay) return 'net';

    let side = opts.side === OrderSide.Buy ? 'long' : 'short';

    if (
      opts.type === OrderType.StopLoss ||
      opts.type === OrderType.TakeProfit ||
      opts.type === OrderType.TrailingStopLoss ||
      opts.reduceOnly
    ) {
      side = side === 'long' ? 'short' : 'long';
    }

    return side;
  };
}
