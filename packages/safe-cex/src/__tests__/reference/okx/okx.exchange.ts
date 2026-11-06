import type { Axios } from 'axios';
import type { ManipulateType } from 'dayjs';
import type { Store } from '../../../store/store.interface';
import type { ClientOrderLookup } from '../../../exchanges/clientOrderId';
import type { OkxInstrumentType, OkxRegion } from '../../../exchanges/okx/okx.types';

import BigNumber from 'bignumber.js';
import dayjs from 'dayjs';
import chunk from 'lodash/chunk';
import flatten from 'lodash/flatten';
import partition from 'lodash/partition';
import sumBy from 'lodash/sumBy';
import { forEachSeries, map, mapSeries } from 'p-iteration';

import { sleep } from '@tealstreet/sleep/src/sleep';
import { mapOkxPositionHistory } from '../../../journal/okx.position-adapter';
import {
  Balance,
  Candle,
  ClosedPnlEntry,
  ExchangeName,
  ExchangeOptions,
  Execution,
  ExecutionsOptions,
  HistoricalOrder,
  HistoricalTradeOptions,
  IncomeHistoryOptions,
  IncomeRecord,
  LogSeverity,
  MarginType,
  Market,
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
  PublicTrade,
  Ticker,
  UpdateOrderOpts,
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
import { AccountMarketOverlay, BaseExchange, CallableMethodsInWorker } from '../../../exchanges/base';
import { orderCarriesClientOrderId, patternClientOrderIdRule } from '../../../exchanges/clientOrderId';
import { parseAccountEquity } from './okx.account-equity';
import { createAPI } from './okx.api';
import { mapOkxClosedPosition } from '../../../exchanges/okx/okx.closed-pnl';
import { mapOkxBillToIncomeRecords, OKX_BILL_TYPE_BY_INCOME_TYPE } from './okx.income';
import { mapOkxHistoricalOrder } from './okx.order-history';
import {
  BROKER_ID,
  ENDPOINTS,
  GRAFANA_ENDPOINTS_EVERY,
  GRAFANA_ENDPOINTS_PERIODIC,
  INTERVAL,
  ORDER_SIDE,
  ORDER_STATUS,
  ORDER_TYPE,
  REVERSE_ORDER_TYPE,
} from '../../../exchanges/okx/okx.types';
import { OKXBusinessWebsocket } from './okx.ws-business';
import { OKXPrivateWebsocket } from './okx.ws-private';
import { OKXPublicWebsocket } from './okx.ws-public';

const OKX_ORDER_DOES_NOT_EXIST = '51603';
const OKX_LOOKUP_STATUS: Record<string, OrderStatus> = {
  filled: OrderStatus.Closed,
  canceled: OrderStatus.Canceled,
  mmp_canceled: OrderStatus.Canceled,
};
// mapOrders only accepts the live states it knows; feed it one, then restore the real status.
const OKX_LOOKUP_LIVE_STATE: Record<string, string> = { live: 'live', partially_filled: 'partially_filled' };

/**
 * Resolve the USD mark price for a balance currency by matching the exact
 * OKX instrument, not a prefix. OKX tickers carry both an instId `id`
 * (e.g. `ETH-USDT-SWAP`) and a compact `symbol` (e.g. `ETHUSDT`); a prefix
 * match (`symbol.startsWith(ccy)`) collides `ETH` with `ETHFI`, `SOL` with
 * `SOLO`, etc. and multiplies equity by an unrelated instrument's mark.
 * Falls back to 1 for stablecoins / currencies with no linear instrument.
 */
export const findTickerMarkForCurrency = (
  tickers: Array<{ id?: string; symbol?: string; mark?: number }>,
  ccy: string,
): number => {
  const candidates = [`${ccy}-USDT-SWAP`, `${ccy}-USD-SWAP`, `${ccy}USDT`, `${ccy}USD`];
  const ticker = tickers.find(
    (t) => (t.id != null && candidates.includes(t.id)) || (t.symbol != null && candidates.includes(t.symbol)),
  );
  return ticker && typeof ticker.mark === 'number' ? ticker.mark : 1;
};

const getOKXTriggerPxType = (triggerType?: OrderStopTriggerType) => {
  if (triggerType === OrderStopTriggerType.IndexPrice) return 'index';
  if (triggerType === OrderStopTriggerType.MarkPrice) return 'mark';
  return 'last';
};

const parseOKXTriggerPxType = (triggerPxType?: string) => {
  if (triggerPxType === 'index') return OrderStopTriggerType.IndexPrice;
  if (triggerPxType === 'mark') return OrderStopTriggerType.MarkPrice;
  return OrderStopTriggerType.LastPrice;
};

const tickerNumber = (value: unknown): number =>
  value === '' || value === undefined || value === null ? 0 : Number.parseFloat(String(value));

export class OKXExchange extends BaseExchange {
  protected readonly clientOrderIdRule = patternClientOrderIdRule('OKX', /^[A-Za-z0-9]{1,32}$/, {
    mint: () => `${BROKER_ID}${uuid()}`.slice(0, 32),
  });

  // Order details see filled and cancelled orders too, so OKX can say an ID was never placed.
  findOrderByClientOrderId = async (symbol: string, clientOrderId: string): Promise<ClientOrderLookup> => {
    const inStore = this.store.orders.find(
      (order) => order.symbol === symbol && orderCarriesClientOrderId(order, clientOrderId),
    );
    // Secondary GET snapshots may belong to another account; their cache cannot prove primary identity.
    if (inStore && !this.options.secondary_key) return { found: true, order: inStore };
    const market = this.store.marketsBySymbol[symbol];
    if (!market) return { found: false, definitive: false };

    try {
      const {
        data: { data },
      } = await (this._primaryXhr ?? this.xhr).get<{ data: Array<Record<string, any>> }>(ENDPOINTS.ORDER_DETAIL, {
        params: { instId: market.id, clOrdId: clientOrderId },
      });
      const row = data?.[0];
      // The shared status map covers live states only; a detail row may be closed.
      const [order] = row ? this.mapOrders([{ ...row, state: OKX_LOOKUP_LIVE_STATE[row.state] ?? 'live' }]) : [];
      if (!order) return { found: false, definitive: false };
      return { found: true, order: { ...order, status: OKX_LOOKUP_STATUS[row.state] ?? order.status } };
    } catch (error: any) {
      if (error?.response?.data?.code === OKX_ORDER_DOES_NOT_EXIST) return { found: false, definitive: true };
      throw error;
    }
  };

  xhr: Axios;
  region: OkxRegion;

  publicWebsocket: OKXPublicWebsocket;
  privateWebsocket: OKXPrivateWebsocket;
  businessWebsocket: OKXBusinessWebsocket;

  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;
  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;

  _accountPositionMode: PositionMode | null = null;

  // https://www.okx.com/docs-v5/en/#trading-account-rest-api-get-account-configuration
  // 1: Spot mode
  // 2: Futures mode
  // 3: Multi-currency margin
  // 4: Portfolio margin
  _accountLevel: number | null = null;

  private accountEquityUsd?: number;
  private accountEquityRows?: Array<Record<string, any>>;
  private accountEquityRevision = 0;

  private readonly markPriceMarketIds = new Set<string>();

  private readonly indexPriceMarketIds = new Set<string>();

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    this.region = exchangeName === 'okxeu' || exchangeName === 'okxeudemo' ? 'okxeu' : 'okx';

    this.xhr = this.createXhrWithSecondaryKey(opts, createAPI);
    this.publicWebsocket = new OKXPublicWebsocket(this);
    this.privateWebsocket = new OKXPrivateWebsocket(this);
    this.businessWebsocket = new OKXBusinessWebsocket(this);
  }

  getInstrumentTypes(): OkxInstrumentType[] {
    return ['SWAP'];
  }

  isSupportedMarketInstrument(market: Record<string, any>): boolean {
    return (
      this.getInstrumentTypes().includes(market.instType) && (market.ctType === 'linear' || market.ctType === 'inverse')
    );
  }

  getMarketInstrumentType(market: { info?: any; instType?: unknown }): OkxInstrumentType {
    const instType = String(market.info?.instType || market.instType || 'SWAP');
    return instType === 'FUTURES' ? 'FUTURES' : 'SWAP';
  }

  protected getMarketInstrumentsEndpoint(): string {
    return ENDPOINTS.MARKETS;
  }

  getIndexTickerInstId(market: Market): string {
    const underlying = String(market.info?.uly || '');
    if (underlying) return underlying;
    return market.id.replace(/-SWAP$/, '');
  }

  getMarketsForIndexTickerInstId(instId: string): Market[] {
    return this.store.markets.filter((market) => this.getIndexTickerInstId(market) === instId);
  }

  private formatMarketSymbol = (market: Record<string, any>) => {
    if (market.instType === 'FUTURES' && market.ruleType === 'xperp') {
      return String(market.instId || '').replace(/-/g, '');
    }

    return String(market.instId || '')
      .replace(/-SWAP$/, '')
      .replace(/-/g, '');
  };

  private formatMarketPrettySymbol = (market: Record<string, any>, base: string, quote: string) => {
    if (market.instType === 'FUTURES' && market.ruleType === 'xperp') {
      return `${base}/${quote} XPERP`;
    }

    return undefined;
  };

  validateAccount = async () => {
    try {
      await this.xhr.get(ENDPOINTS.BALANCE);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return { error: err?.response?.data?.msg || err?.message || err?.message };
    }
    // Key validated; balance is best-effort for the new-account gate (okx's balance
    // calc is store/ticker-dependent, so reuse the tested method rather than inline).
    let balance: Balance | undefined;
    try {
      balance = await this.fetchBalance();
    } catch {
      // best-effort
    }
    return { error: '', balance };
  };

  dispose = () => {
    super.dispose();
    this.publicWebsocket.dispose();
    this.privateWebsocket.dispose();
    this.businessWebsocket.dispose();
  };

  /** Account-scoped market data the keyless shared exchange cannot fetch; none on global OKX. */
  protected async fetchAccountMarketOverlay(): Promise<AccountMarketOverlay | null> {
    return null;
  }

  start = async () => {
    if (this.needsAccountMarketOverlay) {
      await this.loadAccountMarketOverlay(() => this.fetchAccountMarketOverlay());
      if (this.isDisposed) return;
    }

    const useExternalPublicData = await this.initExternalPublicData();
    const publicDataOnly = this.isPublicDataOnly;

    if (!useExternalPublicData) {
      const [markets, rawTickers] = await Promise.all([
        this.fetchMarkets().then((markets) => {
          this.store.update({
            markets,
            loaded: { ...this.store.loaded, markets: true },
          });
          return markets;
        }),
        this.fetchTickersRaw(),
      ]);

      if (this.isDisposed) return;
      const tickers = this.parseTickers(rawTickers);
      this.store.update({
        tickers,
        loaded: { ...this.store.loaded, tickers: true },
      });

      this.log(`Loaded ${Math.min(tickers.length, markets.length)} OKX markets`);

      // Start websocket
      this.publicWebsocket.connectAndSubscribe();
      this.businessWebsocket.connectAndSubscribe();
    }

    if (publicDataOnly) {
      this.log(`Public data only mode enabled`);
      return;
    }

    // we need to fetch leverage before positions
    // this means before ws connect and before balance/positions fetch

    // await this.getPositionMode();
    await this.getOrFetchPositionMode();

    this.privateWebsocket.connectAndSubscribe();

    await Promise.all([
      this.fetchBalanceAndPositions().then(({ balance, positions }) => {
        if (this.isDisposed) return;
        // Account pushes can arrive after the read's last await and before this commit.
        if (this.accountEquityUsd !== undefined) {
          balance = this.calculateBalanceUsedAvail(balance.info, positions, this.store.orders, this._accountLevel || 2);
        }
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

    this.log(`Ready to trade on OKX`);
    this.tick();
  };

  tick = async () => {
    if (!this.isDisposed) {
      const equityRevision = this.accountEquityRevision;
      const balance = await this.formatBalanceUsedAvail(
        this.store.balance.info,
        this.store.positions || [],
        this.store.orders || [],
      );
      if (this.isDisposed) return;
      if (equityRevision === this.accountEquityRevision) this.store.update({ balance });
      loop(() => this.tick(), this.getTickInterval());
    }
  };

  markPriceLoaded = (marketId: string) => {
    this.markPriceMarketIds.add(marketId);
  };

  indexPriceLoaded = (marketId: string) => {
    this.indexPriceMarketIds.add(marketId);
  };

  hasMarkPriceLoaded = (marketId: string) => {
    return this.markPriceMarketIds.has(marketId);
  };

  hasIndexPriceLoaded = (marketId: string) => {
    return this.indexPriceMarketIds.has(marketId);
  };

  fetchMarkets = async () => {
    try {
      const responses = await Promise.all(
        this.getInstrumentTypes().map((instType) =>
          this.xhr.get<{ data: Array<Record<string, any>> }>(this.getMarketInstrumentsEndpoint(), {
            params: { instType },
          }),
        ),
      );
      const data = responses.flatMap(({ data }) => data.data);

      return data
        .filter((m) => this.isSupportedMarketInstrument(m))
        .map((m) => {
          const isInverse = m.ctType === 'inverse';
          const [ulyBase, ulyQuote] = String(m.uly || m.instFamily || '')
            .split('-')
            .map((s) => s?.toUpperCase?.() || '');
          const maxAmount = Math.min(
            parseFloat(m.maxIcebergSz),
            parseFloat(m.maxLmtSz),
            parseFloat(m.maxMktSz),
            parseFloat(m.maxStopSz),
            parseFloat(m.maxTriggerSz),
            parseFloat(m.maxTwapSz),
          );
          const base = (m.baseCcy || (isInverse ? m.settleCcy : m.ctValCcy) || ulyBase || '').toUpperCase();
          const quote = (m.quoteCcy || (isInverse ? m.ctValCcy : m.settleCcy) || ulyQuote || '').toUpperCase();

          const prettySymbol = this.formatMarketPrettySymbol(m, base, quote);

          return {
            id: m.instId,
            symbol: this.formatMarketSymbol(m),
            ...(prettySymbol ? { prettySymbol } : {}),
            base,
            quote,
            isInverse,
            active: m.state === 'live',
            precision: {
              amount: parseFloat(m.lotSz),
              price: parseFloat(m.tickSz),
            },
            limits: {
              amount: {
                min: BigNumber(m.minSz).multipliedBy(m.ctVal).toNumber(),
                max: maxAmount,
              },
              leverage: {
                min: 1,
                max: parseFloat(m.lever),
              },
            },
            contractSize: parseFloat(m.ctVal),
            info: m,
          };
        });
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return this.store.markets;
    }
  };

  fetchTickers = async () => {
    const data = await this.fetchTickersRaw();
    return this.parseTickers(data);
  };

  fetchTickersRaw = async () => {
    try {
      const responses = await Promise.all(
        this.getInstrumentTypes().map((instType) =>
          this.xhr.get(ENDPOINTS.TICKERS, {
            params: {
              instType,
            },
          }),
        ),
      );
      const data = responses.flatMap(({ data }) => data.data);

      return data;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return [];
    }
  };

  parseTickers = (data: Record<string, any>) => {
    try {
      const tickers: Ticker[] = data.reduce((acc: Ticker[], t: Record<string, any>) => {
        const market = this.store.marketsById[t.instId];

        if (!market) return acc;

        const open = tickerNumber(t.open24h);
        const last = tickerNumber(t.last);
        const percentage = open === 0 ? 0 : roundUSD(((last - open) / open) * 100);

        const ticker = {
          id: market.id,
          symbol: market.symbol,
          bid: tickerNumber(t.bidPx),
          ask: tickerNumber(t.askPx),
          last,
          mark: last,
          index: last,
          percentage,
          fundingRate: 0,
          fundingTime: 0,
          volume: tickerNumber(t.volCcy24h),
          quoteVolume: tickerNumber(t.vol24h),
          openInterest: 0,
        };

        return [...acc, ticker];
      }, []);

      return tickers;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return this.store.tickers;
    }
  };

  fetchPositionMetadata = async (
    symbols: string[],
  ): Promise<Pick<Position, 'symbol' | 'side' | 'leverage' | 'positionMode' | 'marginType'>[]> => {
    const [positionMode, leverages] = await Promise.all([this.getOrFetchPositionMode(), this.fetchLeverages(symbols)]);

    return symbols.flatMap((s): Pick<Position, 'symbol' | 'side' | 'leverage' | 'positionMode' | 'marginType'>[] => {
      if (positionMode === PositionMode.OneWay) {
        return [
          {
            symbol: s,
            side: PositionSide.Long,
            leverage: leverages[s] || 0,
            positionMode: positionMode,
            marginType: MarginType.Cross,
          },
          {
            symbol: s,
            side: PositionSide.Short,
            leverage: leverages[s] || 0,
            positionMode: positionMode,
            marginType: MarginType.Cross,
          },
        ];
      } else {
        return [
          {
            symbol: s,
            side: PositionSide.Long,
            leverage: leverages[s] || 0,
            positionMode: positionMode,
            marginType: MarginType.Cross,
          },
          {
            symbol: s,
            side: PositionSide.Short,
            leverage: leverages[s] || 0,
            positionMode: positionMode,
            marginType: MarginType.Cross,
          },
        ];
      }
    });
  };

  fetchBalanceAndPositions = async (symbols?: string[]) => {
    const equityRevision = this.accountEquityRevision;
    await this.getOrFetchPositionMode(true);
    const { balance, positions: openPositions } = await this.fetchBalanceAndOpenPositions();
    if (!symbols)
      return {
        balance: equityRevision === this.accountEquityRevision ? balance : this.store.balance,
        positions: openPositions,
      };
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
      };
    });
    const positions = [...openPositionsForSymbols, ...fakePositions];
    return { balance: equityRevision === this.accountEquityRevision ? balance : this.store.balance, positions };
  };

  fetchBalanceAndOpenPositions = async () => {
    const equityRevision = this.accountEquityRevision;
    if (!this._accountPositionMode) {
      throw new Error('Position mode not loaded');
    }
    try {
      const {
        data: {
          data: [{ balData: bal }],
        },
      } = await this.xhr.get<{
        data: Array<Record<string, Array<Record<string, any>>>>;
      }>(ENDPOINTS.BALANCE, { params: { instType: this.getInstrumentTypes()[0] } });

      const positionResponses = await Promise.all(
        this.getInstrumentTypes().map((instType) =>
          this.xhr.get<{ data: Array<Record<string, any>> }>(ENDPOINTS.POSITIONS, {
            params: { instType },
          }),
        ),
      );
      const pData = positionResponses.flatMap(({ data }) => data.data);

      const positions: Position[] = this.mapPositions(pData);
      if (equityRevision === this.accountEquityRevision) {
        this.accountEquityUsd = undefined;
        this.accountEquityRows = undefined;
      }
      const balance = await this.formatBalanceUsedAvail(bal, positions, this.store.orders || []);

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

  applyAccountEquity = (data: unknown) => {
    const update = parseAccountEquity(data);
    if (!update || this.isDisposed) return;
    const rows = new Map<string, Record<string, any>>();
    const previous = this.accountEquityRows ?? this.store.balance.info;
    if (Array.isArray(previous))
      for (const row of previous) {
        if (typeof row?.ccy === 'string') rows.set(row.ccy, row);
      }
    for (const row of update.details) rows.set(row.ccy, { ...rows.get(row.ccy), ...row });
    this.accountEquityRows = [...rows.values()];
    this.accountEquityUsd = update.total;
    this.accountEquityRevision++;
    const balance = this.calculateBalanceUsedAvail(
      this.accountEquityRows,
      this.store.positions,
      this.store.orders,
      this._accountLevel || 2,
    );
    this.store.update({ balance });
  };

  formatBalanceUsedAvail = async (bal: any, positions: Position[], orders: Order[]) => {
    try {
      const accLevel = (await this.getOrFetchAccountLevel()) || 2;
      return this.calculateBalanceUsedAvail(bal, positions, orders, accLevel);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return this.store.balance;
    }
  };

  private calculateBalanceUsedAvail = (bal: any, positions: Position[], orders: Order[], accLevel: number) => {
    try {
      bal = this.accountEquityRows ?? bal;

      const pData = positions.map((p) => p.info).filter((p) => !!p);
      const oData = orders.filter(
        (o) => o.info && o.type === OrderType.Limit && o.status === OrderStatus.Open && !o.reduceOnly,
      );

      const eqNotional: { [currency: string]: number } = {};
      for (const b of bal) {
        const mark = findTickerMarkForCurrency(this.store.tickers, b.ccy);
        eqNotional[b.ccy] = this.toFiniteNumber(b.eq) * mark;
      }

      const totalCollateral = this.accountEquityUsd ?? roundUSD(sumBy(Object.values(eqNotional)));
      const usedP = roundUSD(
        sumBy(pData, (p) => {
          const market = this.store.marketsById[p.instId];
          return this.convertMarginValueToUsd(this.toFiniteNumber(p.imr), p.ccy, market, this.toFiniteNumber(p.markPx));
        }),
      );

      // hand wavy order imr mult
      const orderImrMult = 1.1;
      const usedO = roundUSD(
        sumBy(oData, (o) => {
          const leverage = Math.max(this.toFiniteNumber(o.info.lever || '1'), 1);
          return (o.remaining * o.price) / leverage;
        }) * orderImrMult,
      );

      const upnl = roundUSD(
        sumBy(pData, (p) => {
          const market = this.store.marketsById[p.instId];
          return this.convertMarginValueToUsd(this.toFiniteNumber(p.upl), p.ccy, market, this.toFiniteNumber(p.markPx));
        }),
      );

      // total used and upnl per margin currency
      const totalsPerMarginCurrency: { [currency: string]: { used: number; upnl: number } } = {};
      for (const p of pData) {
        const marginCcy = p.ccy;
        if (!totalsPerMarginCurrency[marginCcy]) {
          totalsPerMarginCurrency[marginCcy] = { used: 0, upnl: 0 };
        }
        totalsPerMarginCurrency[marginCcy].used = add(
          totalsPerMarginCurrency[marginCcy].used,
          this.toFiniteNumber(p.imr),
        );
        totalsPerMarginCurrency[marginCcy].upnl = add(
          totalsPerMarginCurrency[marginCcy].upnl,
          this.toFiniteNumber(p.upl),
        );
      }

      for (const o of oData) {
        const marginCcy = o.info.ccy;
        if (!totalsPerMarginCurrency[marginCcy]) {
          totalsPerMarginCurrency[marginCcy] = { used: 0, upnl: 0 };
        }
        const market = this.store.marketsBySymbol[o.symbol];
        const leverage = Math.max(this.toFiniteNumber(o.info.lever || '1'), 1);
        const usedAmountUsd = ((o.remaining * o.price) / leverage) * orderImrMult;
        const usedAmountInMarginCurrency = this.convertUsdValueToMarginCurrency(
          usedAmountUsd,
          marginCcy,
          market,
          this.toFiniteNumber(o.price),
        );
        totalsPerMarginCurrency[marginCcy].used = add(
          totalsPerMarginCurrency[marginCcy].used,
          usedAmountInMarginCurrency,
        );
      }

      const balance: Balance = {
        used: usedP,
        free: subtract(subtract(totalCollateral, usedP), usedO),
        total: subtract(totalCollateral, upnl),
        upnl,
        currencies:
          // only do per margin currency margin when in future aka non unified mode
          accLevel === 2
            ? bal.reduce((acc: { [currency: string]: Balance }, b: any) => {
                const currency = b.ccy;
                const equityInCurrency = this.toFiniteNumber(b.eq);
                const used = totalsPerMarginCurrency[currency]?.used || 0;
                const upnl = totalsPerMarginCurrency[currency]?.upnl || 0;
                acc[currency] = {
                  used,
                  free: subtract(equityInCurrency, used),
                  total: subtract(equityInCurrency, upnl),
                  upnl,
                };
                return acc;
              }, {})
            : {},
        info: bal,
      };

      return balance;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return this.store.balance;
    }
  };

  fetchPositions = async (symbols?: string[]) => {
    const { positions } = await this.fetchBalanceAndPositions(symbols);
    return positions;
  };

  fetchBalance = async () => {
    const { balance } = await this.fetchBalanceAndOpenPositions();
    return balance;
  };

  fetchLeverages = async (symbols: string[]) => {
    let batchNo = 0;
    const responses = flatten(
      await map(chunk(symbols, 20), async (batch) => {
        if (this.isDisposed) return [];

        const innerBatch = batchNo++;
        await sleep(innerBatch * 250);

        const {
          data: { data },
        } = await this.xhr.get(ENDPOINTS.LEVERAGE, {
          params: {
            instId: batch.map((m) => this.store.marketsBySymbol[m]!.id).join(','),
            mgnMode: 'cross',
          },
        });

        return data;
      }),
    );

    return responses.reduce<Record<string, number>>((acc, r) => {
      const symbol = this.store.marketsById[r.instId]!.symbol;
      acc[symbol] = parseFloat(r.lever);
      return acc;
    }, {});
  };

  fetchOrders = async () => {
    try {
      const orders = await this.fetchNormalOrders();
      const ocoOrders = await this.fetchAlgoOrders('oco');
      const conditionalOrders = await this.fetchAlgoOrders('conditional');
      const triggerOrders = await this.fetchAlgoOrders('trigger');
      const trailingOrders = await this.fetchAlgoOrders('move_order_stop');
      return [...orders, ...ocoOrders, ...trailingOrders, ...conditionalOrders, ...triggerOrders];
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return this.store.orders;
    }
  };

  fetchNormalOrders = async () => {
    const recursiveFetch = async (
      orders: Array<Record<string, any>> = [],
      instType: OkxInstrumentType,
    ): Promise<Array<Record<string, any>>> => {
      const {
        data: { data },
      } = await this.xhr.get<{ data: Array<Record<string, any>> }>(ENDPOINTS.UNFILLED_ORDERS, {
        params: {
          instType,
          after: orders.length ? orders[orders.length - 1].ordId : undefined,
        },
      });

      if (data.length === 100) {
        return recursiveFetch([...orders, ...data], instType);
      }

      return [...orders, ...data];
    };

    const okxOrders = flatten(await map(this.getInstrumentTypes(), (instType) => recursiveFetch([], instType)));
    return this.mapOrders(okxOrders);
  };

  fetchAlgoOrders = async (type: 'conditional' | 'trigger' | 'move_order_stop' | 'oco') => {
    const recursiveFetch = async (
      orders: Array<Record<string, any>> = [],
      instType: OkxInstrumentType,
    ): Promise<Array<Record<string, any>>> => {
      const {
        data: { data },
      } = await this.xhr.get<{ data: Array<Record<string, any>> }>(ENDPOINTS.UNFILLED_ALGO_ORDERS, {
        params: {
          instType,
          ordType: type,
          after: orders.length ? orders[orders.length - 1].algoId : undefined,
        },
      });

      if (data.length === 100) {
        return recursiveFetch([...orders, ...data], instType);
      }

      return [...orders, ...data];
    };

    const okxOrders = flatten(await map(this.getInstrumentTypes(), (instType) => recursiveFetch([], instType)));
    return this.mapOrders(okxOrders);
  };

  fetchPublicTrades = async (symbol: string, limit = 100): Promise<PublicTrade[]> => {
    const market = this.store.marketsBySymbol[symbol];
    if (!market) return [];
    try {
      const {
        data: { data },
      } = await this.xhr.get<{ data: Array<Record<string, string>> }>('/api/v5/market/trades', {
        params: { instId: market.id, limit: Math.min(Math.max(limit, 1), 500) },
      });
      const trades = new Map<string, PublicTrade>();
      for (const row of data) {
        const price = Number(row.px);
        const native = Number(row.sz);
        const timestamp = Number(row.ts);
        if (
          row.instId !== market.id ||
          !row.tradeId ||
          !['buy', 'sell'].includes(row.side) ||
          ![price, native, timestamp].every((n) => Number.isFinite(n) && n > 0)
        )
          continue;
        const qty = this.contractsToBase(native, market, price);
        trades.set(row.tradeId, {
          id: row.tradeId,
          symbol,
          price,
          qty,
          quoteQty: price * qty,
          timestamp,
          side: ORDER_SIDE[row.side],
        });
      }
      return [...trades.values()];
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
    const maxLimit = Math.min(Math.max(limit, 1), 100);

    const mapExecutions = (rows: Array<Record<string, any>>) => {
      return rows.reduce<Execution[]>((acc, row) => {
        const side = ORDER_SIDE[String(row.side || '').toLowerCase()];
        if (!side) {
          return acc;
        }

        const fillPrice = parseFloat(row.fillPx || row.px || row.avgPx || '0');
        const fillAmountNative = parseFloat(row.fillSz || row.sz || row.accFillSz || '0');
        if (
          !Number.isFinite(fillPrice) ||
          !Number.isFinite(fillAmountNative) ||
          fillPrice <= 0 ||
          fillAmountNative <= 0
        ) {
          return acc;
        }

        const nativeTime = Number.parseInt(String(row.fillTime || row.ts || row.uTime || row.cTime || '0'), 10);
        const timestamp = normalizeToSeconds(nativeTime);
        const timeMilliseconds = normalizeToMilliseconds(nativeTime);
        if (
          !timestamp ||
          timeMilliseconds === undefined ||
          (fromMilliseconds !== undefined && timeMilliseconds < fromMilliseconds) ||
          (toMilliseconds !== undefined && timeMilliseconds > toMilliseconds)
        ) {
          return acc;
        }

        const resolvedSymbol = this.store.marketsById[row.instId]?.symbol || market.symbol;
        if (resolvedSymbol !== market.symbol) {
          return acc;
        }

        const qty = this.contractsToBase(fillAmountNative, market, fillPrice);
        if (!Number.isFinite(qty) || qty <= 0) {
          return acc;
        }

        const executionOrderId = String(row.ordId || row.orderId || '');
        if (orderId && executionOrderId && executionOrderId !== String(orderId)) {
          return acc;
        }

        const commissionRaw = parseFloat(row.fee || row.fillFee || '0');
        const commission = Number.isFinite(commissionRaw) ? Math.abs(commissionRaw) : undefined;
        const realizedPnl = parseFloat(row.fillPnl ?? '');

        acc.push(
          omitUndefined({
            // One order fills across many rows, so orderId cannot identify a fill.
            // tradeId, NOT billId: OKX emits several bill rows per trade (fill plus
            // fee/rebate) that share ordId/fillTime/fillPx/fillSz, and keying on the
            // per-bill id would split them into duplicate fills and double the qty.
            // billId only stands in when tradeId is absent.
            id: row.tradeId ? String(row.tradeId) : row.billId ? String(row.billId) : undefined,
            timestamp,
            symbol: resolvedSymbol,
            orderId: executionOrderId || undefined,
            price: fillPrice,
            qty,
            side,
            commission,
            commissionAsset: row.feeCcy || row.ccy || undefined,
            positionSide: row.posSide ? normalizeExecutionPositionSide(row.posSide) : undefined,
            realizedPnl: Number.isFinite(realizedPnl) ? realizedPnl : undefined,
            // execType 'M' = maker, 'T' = taker.
            maker: row.execType ? String(row.execType).toUpperCase() === 'M' : undefined,
          }),
        );
        return acc;
      }, []);
    };

    const fetchByEndpoint = async (endpoint: string) => {
      const params = omitUndefined({
        instType: this.getMarketInstrumentType(market),
        instId: market.id,
        ordId: orderId,
        begin: fromMilliseconds,
        end: toMilliseconds,
        limit: maxLimit,
      });
      const {
        data: { data },
      } = await this.xhr.get<{ data: Array<Record<string, any>> }>(endpoint, { params });
      return Array.isArray(data) ? data : [];
    };

    try {
      const recentRows = await fetchByEndpoint(ENDPOINTS.FILLS);
      const historicalRows = recentRows.length >= maxLimit ? [] : await fetchByEndpoint(ENDPOINTS.FILLS_HISTORY);
      const dedupe = new Map<string, Execution>();

      for (const execution of [...mapExecutions(recentRows), ...mapExecutions(historicalRows)]) {
        // Without the fill id, two genuine fills of one order at the same timestamp,
        // price and size collapse into one — the exact data loss the id exists to stop.
        const key = execution.id
          ? `id:${execution.id}`
          : `${execution.orderId || 'no-order'}:${execution.timestamp}:${execution.price}:${execution.qty}:${execution.side}`;
        dedupe.set(key, execution);
      }

      return [...dedupe.values()].sort((a, b) => b.timestamp - a.timestamp).slice(0, maxLimit);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return [];
    }
  };

  /**
   * Account ledger (bills). Scoped to `instType: 'SWAP'` — this repo's journal is
   * position-based and perp-only, so spot/margin bills are deliberately excluded. The
   * consequence is that the returned ledger reconciles against swap-account flows, NOT
   * against total account equity.
   *
   * BILLS retains 7 days and BILLS_ARCHIVE 3 months; a window older than the chosen
   * endpoint's retention answers code=0 with an empty list rather than an error, so the
   * endpoint is picked from the requested window rather than discovered by failure.
   */
  fetchIncomeHistory = async (opts?: IncomeHistoryOptions): Promise<IncomeRecord[]> => {
    if (this.isPublicDataOnly) {
      return [];
    }

    const PAGE_LIMIT = 100;
    // A runaway-loop backstop, not a coverage bound. Bills come newest-first, so a cap
    // that bites silently drops the OLDEST rows of the window — a hole in the middle of
    // the ledger, which is exactly what makes reconciliation unfixable. Hitting it is
    // logged rather than swallowed.
    const MAX_PAGES = 200;
    const RECENT_WINDOW_MS = 6 * 24 * 60 * 60 * 1000;

    const market = opts?.symbol ? this.store.marketsBySymbol[opts.symbol] : undefined;
    if (opts?.symbol && !market) {
      return [];
    }

    const now = virtualClock.getCurrentTimeMs();
    const windowStart = opts?.startTime;
    const endpoint = windowStart && now - windowStart > RECENT_WINDOW_MS ? ENDPOINTS.BILLS_ARCHIVE : ENDPOINTS.BILLS;

    const resolveSymbol = (instId: string) => this.store.marketsById[instId]?.symbol;
    const maxRecords = opts?.limit && opts.limit > 0 ? opts.limit : Infinity;

    try {
      const dedupe = new Map<string, IncomeRecord>();
      let after: string | undefined;
      let exhausted = false;
      let rowsSeen = 0;
      let page = 0;

      for (; page < MAX_PAGES; page += 1) {
        const params = omitUndefined({
          instType: 'SWAP',
          instId: market?.id,
          type: opts?.incomeType ? OKX_BILL_TYPE_BY_INCOME_TYPE[opts.incomeType] : undefined,
          begin: opts?.startTime,
          end: opts?.endTime,
          limit: PAGE_LIMIT,
          after,
        });

        const {
          data: { data },
        } = await this.xhr.get<{ data: Array<Record<string, any>> }>(endpoint, { params });

        const rows = Array.isArray(data) ? data : [];
        if (rows.length === 0) {
          exhausted = true;
          break;
        }

        rowsSeen += rows.length;
        for (const row of rows) {
          for (const record of mapOkxBillToIncomeRecords(row, resolveSymbol)) {
            dedupe.set(record.tranId, record);
          }
        }

        // `maxRecords` bounds BILLS read, not records emitted — one bill yields up to
        // three records, so counting records would stop short of the caller's ask.
        if (rows.length < PAGE_LIMIT || rowsSeen >= maxRecords) {
          exhausted = true;
          break;
        }

        // `after` means "older than this bill id". Derived as the page's SMALLEST id
        // rather than its last element: taking the last is only correct while okx returns
        // newest-first, and if that ever flips the cursor walks the wrong way and the walk
        // silently ends one page in — on a venue that currently PASSES the gate.
        const nextAfter = oldestCursorId(rows, 'billId') ?? '';
        if (!nextAfter || nextAfter === after) {
          exhausted = true;
          break;
        }
        after = nextAfter;

        await sleep(100);
      }

      if (!exhausted) {
        this.log(
          `OKX bills paging hit the ${MAX_PAGES}-page cap after ${rowsSeen} rows; older rows in this window were not fetched`,
          LogSeverity.Warning,
        );
      }

      return [...dedupe.values()].sort((a, b) => b.timestamp - a.timestamp);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return [];
    }
  };

  /**
   * Historical orders, including ones that never filled — see `okx.order-history.ts`.
   *
   * Bounded to the 7-day `/orders-history` route on purpose. OKX's 3-month
   * `/orders-history-archive` returns FILLED ORDERS ONLY, so paging into it to reach
   * further back would silently drop every cancellation — the one population this axis
   * holds that the fills axis does not. Measured on a live account: `state=canceled`
   * returned 0 rows from the archive and 58 from this route, and a matched 2-day window
   * gave 100/0 filled/canceled there against 80/20 here.
   */
  fetchHistoricalOrders = async (opts?: OrderHistoryOptions): Promise<HistoricalOrder[]> => {
    if (this.isPublicDataOnly) {
      return [];
    }

    const market = opts?.symbol ? this.store.marketsBySymbol[opts.symbol] : undefined;
    if (opts?.symbol && !market) {
      return [];
    }

    const MAX_PAGES = 50;
    const PAGE_LIMIT = 100;

    try {
      const orders: HistoricalOrder[] = [];
      const seen = new Set<string>();
      // `after` is an ordId cursor on this endpoint — unlike /positions-history, where the
      // same-named param is a timestamp. Mixing the two conventions up returns the newest
      // page forever, which reads as a short history rather than as a broken loop.
      let after: string | undefined;
      let page = 0;

      for (; page < MAX_PAGES; page += 1) {
        const params = omitUndefined({
          instType: 'SWAP',
          instId: market?.id,
          begin: opts?.from,
          end: opts?.to,
          limit: PAGE_LIMIT,
          after,
        });

        const {
          data: { data },
        } = await this.xhr.get<{ data: Array<Record<string, any>> }>(ENDPOINTS.ORDERS_HISTORY, { params });

        const rows = Array.isArray(data) ? data : [];
        if (rows.length === 0) break;

        for (const row of rows) {
          const rowMarket = market ?? this.store.marketsById[String(row.instId ?? '')];
          const order = mapOkxHistoricalOrder(row, rowMarket, this.contractsToBase);
          if (!order || seen.has(order.id)) continue;
          seen.add(order.id);
          orders.push(order);
        }

        if (rows.length < PAGE_LIMIT) break;

        const nextAfter = oldestCursorId(rows, 'ordId') ?? '';
        if (!nextAfter || nextAfter === after) break;
        after = nextAfter;

        await sleep(100);
      }

      if (page >= MAX_PAGES) {
        // Newest-first, so a cap that bites drops the OLDEST orders of the window.
        this.log(
          `fetchHistoricalOrders stopped at the ${MAX_PAGES}-page cap; older orders were not fetched`,
          LogSeverity.Warning,
        );
      }

      const sorted = orders.sort((a, b) => b.createdTime - a.createdTime);
      return opts?.limit && opts.limit > 0 ? sorted.slice(0, opts.limit) : sorted;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return [];
    }
  };

  /**
   * Exchange-reported closed positions, from a DIFFERENT endpoint than the ledger — which
   * is the only reason it can cross-check it. See `okx.closed-pnl.ts` for the field shape
   * and the funding-inclusion difference from bybit.
   */
  /**
   * RAW `/api/v5/trade/closed-pnl` rows, paged.
   *
   * Public because the journal needs the unmapped row: `mapOkxPositionHistory` reads fields
   * `ClosedPnlEntry` does not carry, and the audit harness was obtaining them by INSTRUMENTING
   * the URL. Scraping is fine in a harness and wrong in production, and having two readers of
   * one endpoint is where they drift — so the paging lives here once and both consumers use it.
   */
  fetchClosedPositionRows = async (opts: HistoricalTradeOptions): Promise<Array<Record<string, any>>> => {
    if (this.isPublicDataOnly) return [];

    const market = opts.symbol ? this.store.marketsBySymbol[opts.symbol] : undefined;
    if (opts.symbol && !market) return [];

    const MAX_PAGES = 100;
    const PAGE_LIMIT = 100;
    const collected: Array<Record<string, any>> = [];
    let after: string | undefined = opts.to ? String(opts.to) : undefined;

    for (let page = 0; page < MAX_PAGES; page += 1) {
      const params = omitUndefined({
        instType: 'SWAP',
        instId: market?.id,
        before: opts.from,
        after,
        limit: PAGE_LIMIT,
      });

      const {
        data: { data },
      } = await this.xhr.get<{ data: Array<Record<string, any>> }>(ENDPOINTS.POSITIONS_HISTORY, { params });

      const rows = Array.isArray(data) ? data : [];
      if (rows.length === 0) break;
      collected.push(...rows);
      if (rows.length < PAGE_LIMIT) break;

      const oldest = rows.reduce((acc, row) => {
        const t = parseInt(String(row.uTime ?? '0'), 10);
        return Number.isFinite(t) && t > 0 ? Math.min(acc, t) : acc;
      }, Number.POSITIVE_INFINITY);
      if (!Number.isFinite(oldest)) break;

      const nextAfter = String(oldest - 1);
      if (nextAfter === after) break;
      after = nextAfter;
      await sleep(100);
    }

    return collected;
  };

  /**
   * Journal records from the REPORTED closed axis, not from folding fills.
   *
   * okx folds to NOTHING — measured 0 records against 134 real round trips — because its
   * records never came from fills. `BaseExchange` now throws rather than folding a venue that
   * reports closed positions; this is the override that throw asks for.
   */
  fetchHistoricalPositions = async (opts: { account: string; from?: number; to?: number }) => {
    const rows = await this.fetchClosedPositionRows({ from: opts.from, to: opts.to } as HistoricalTradeOptions);
    const records = [];

    for (const row of rows) {
      const market = this.store?.marketsById?.[String(row.instId ?? '')];
      // An unknown market cannot be converted from contracts, and an INVERSE one needs a
      // reference price `contractsToBase` lacks here — it returns 0, which the adapter drops
      // as a zero-size row. Skipped explicitly rather than silently mapped to nothing.
      if (!market || market.isInverse) continue;

      const record = mapOkxPositionHistory(row, {
        account: opts.account,
        toBase: (contracts: number, instId: string) =>
          this.contractsToBase(contracts, this.store?.marketsById?.[instId] ?? market),
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

    const MAX_PAGES = 100;
    const PAGE_LIMIT = 100;

    try {
      const entries: ClosedPnlEntry[] = [];
      let after: string | undefined = opts.to ? String(opts.to) : undefined;
      let page = 0;

      for (; page < MAX_PAGES; page += 1) {
        // `after`/`before` are TIMESTAMPS on this endpoint, not ids — unlike /bills, where
        // `after` is a billId. And `begin`/`end` are SILENTLY IGNORED here: passing them
        // returns the same unbounded 100 rows spanning nine months, which looks like a
        // working time filter until you check the oldest row. Verified live.
        const params = omitUndefined({
          instType: 'SWAP',
          instId: market?.id,
          before: opts.from,
          after,
          limit: PAGE_LIMIT,
        });

        const {
          data: { data },
        } = await this.xhr.get<{ data: Array<Record<string, any>> }>(ENDPOINTS.POSITIONS_HISTORY, { params });

        const rows = Array.isArray(data) ? data : [];
        if (rows.length === 0) break;

        for (const row of rows) {
          // NO posId dedupe. OKX reuses a posId across separate open->close cycles in net
          // mode, so every row here is a distinct close: a live window returned 87 rows
          // across only 3 posIds, and deduping on it destroyed 84 real closes and 96% of
          // the realized PnL. Third time an identity dedupe has deleted rows in this epic
          // (bybit tranId, hyperliquid boundary fills, now this) — the endpoint's row IS
          // the event; do not assume an id identifies one.
          const rowMarket = market ?? this.store.marketsById[String(row.instId ?? '')];
          const entry = mapOkxClosedPosition(row, rowMarket, this.contractsToBase.bind(this), opts.accountName || '');
          if (entry) entries.push(entry);
        }

        if (rows.length < PAGE_LIMIT) break;

        // Walk backwards by the oldest close time seen, minus 1ms so the boundary row is
        // not returned again.
        const oldest = rows.reduce((acc, row) => {
          const t = parseInt(String(row.uTime ?? '0'), 10);
          return Number.isFinite(t) && t > 0 ? Math.min(acc, t) : acc;
        }, Number.POSITIVE_INFINITY);
        if (!Number.isFinite(oldest)) break;

        const nextAfter = String(oldest - 1);
        if (nextAfter === after) break;
        after = nextAfter;

        await sleep(100);
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

  fetchIncomeHistoryRange = async (
    startTime: number,
    endTime: number,
    incomeType?: string,
  ): Promise<IncomeRecord[]> => {
    const SEVEN_DAYS_MS = 7 * 24 * 60 * 60 * 1000;
    const dedupe = new Map<string, IncomeRecord>();

    let currentStart = startTime;

    while (currentStart < endTime) {
      const currentEnd = Math.min(currentStart + SEVEN_DAYS_MS, endTime);

      const records = await this.fetchIncomeHistory({
        startTime: currentStart,
        endTime: currentEnd,
        incomeType,
      });

      for (const record of records) {
        dedupe.set(record.tranId, record);
      }

      // Exclusive, so consecutive chunks cannot both return a row on the boundary. The
      // tranId dedupe above is safe here only because OKX billIds are genuinely unique;
      // do not copy that pattern to a venue whose ids are not.
      currentStart = currentEnd + 1;
      await sleep(100);
    }

    return [...dedupe.values()].sort((a, b) => b.timestamp - a.timestamp);
  };

  fetchOHLCV = async (opts: OHLCVOptions) => {
    const limit = Math.min(opts.limit || 300, 300);
    const [, amount, unit] = opts.interval.split(/(\d+)/);

    const end = opts.to ? dayjs.utc(opts.to) : dayjs.utc(virtualClock.getCurrentTimeMs());
    const start =
      !opts.limit && opts.from
        ? dayjs.utc(opts.from)
        : end.subtract(parseFloat(amount) * limit, unit as ManipulateType);

    const market = this.store.marketsBySymbol[opts.symbol];

    if (market) {
      try {
        const fetchCandles = async (endpoint: string, requestLimit: number) => {
          const {
            data: { data },
          } = await this.xhr.get<{ data: string[][] }>(endpoint, {
            params: {
              instId: market.id,
              bar: INTERVAL[opts.interval],
              limit: requestLimit,
              before: start.valueOf(),
              after: end.valueOf(),
            },
          });

          return data;
        };

        const recentRows = await fetchCandles(ENDPOINTS.KLINE, limit);
        const recentWindowStart = dayjs
          .utc(virtualClock.getCurrentTimeMs())
          .subtract(parseFloat(amount) * 1_440, unit as ManipulateType);
        const needsHistoricalEndpoint = Boolean(opts.to && end.isBefore(recentWindowStart));
        const data =
          needsHistoricalEndpoint || recentRows.length === 0
            ? await fetchCandles(ENDPOINTS.KLINE_HISTORY, Math.min(limit, 100))
            : recentRows;

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

    this.emitAndLogError(`Market ${opts.symbol} not found on OKX`);
    return [];
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    return this.businessWebsocket.listenOHLCV(opts, callback);
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    return this.publicWebsocket.listenOrderBook(symbol, callback);
  };

  updateOrder = async ({ order, update }: UpdateOrderOpts) => {
    if (order.type !== OrderType.Limit) {
      return this.updateAlgoOrder({ order, update });
    }

    const market = this.store.marketsBySymbol[order.symbol];
    if (!market) throw new Error(`Market ${order.symbol} not found on OKX`);

    const payload: Record<string, any> = { instId: market.id, ordId: order.id };

    if ('price' in update) {
      payload.newPx = `${update.price}`;
    }

    if ('amount' in update && +order.amount !== 0) {
      const pAmount = market.precision.amount;
      const amount = adjust(update.amount, pAmount);

      if (amount === 0) {
        this.emitAndLogError('Order amount is too small');
        return [];
      }

      payload.newSz = `${amount}`;
    }

    try {
      await this.xhr.post(ENDPOINTS.UPDATE_ORDER, payload);
      return [order.id];
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return [];
    }
  };

  updateAlgoOrder = async ({ order, update }: UpdateOrderOpts) => {
    if (order.type === OrderType.Conditional) {
      return this.updateTriggerAlgoOrder({ order, update });
    }

    const orders = this.store.orders.filter((o) => o.id.startsWith(order.id.replace(/_[a-zA-Z]+$/, '')));

    const amount = 'amount' in update ? update.amount : order.amount;
    const price = 'price' in update ? update.price : order.price;

    const baseOrder = {
      ...order,
      symbol: order.symbol,
      type: order.type,
      side: order.side,
      amount,
      reduceOnly: order.reduceOnly || false,
    };

    const newOrder: PlaceOrderOpts = {
      ...baseOrder,
      price,
    };

    await this.cancelAlgoOrders(orders);
    return await this.placeOrder(newOrder);
  };

  private updateTriggerAlgoOrder = async ({ order, update }: UpdateOrderOpts) => {
    const market = this.store.marketsBySymbol[order.symbol];
    if (!market) throw new Error(`Market ${order.symbol} not found on OKX`);

    const payload: Record<string, any> = {
      instId: market.id,
      algoId: order.id.replace(/_[a-zA-Z]+$/, ''),
    };

    const triggerPrice = adjust('price' in update ? update.price : order.price, market.precision.price);
    if (!Number.isFinite(triggerPrice) || triggerPrice <= 0) {
      const message = 'Trigger price is required for OKX opening conditional orders';
      this.emitAndLogError(message);
      throw new Error(message);
    }

    const conditionalOrderPrice = this.toFiniteNumber(order.extra?.conditionalOrderPrice);
    payload.newTriggerPx = `${triggerPrice}`;
    payload.newOrdPx = conditionalOrderPrice > 0 ? `${adjust(conditionalOrderPrice, market.precision.price)}` : '-1';
    payload.newTriggerPxType = getOKXTriggerPxType(order.triggerType);

    if ('amount' in update && +order.amount !== 0) {
      const amount = adjust(update.amount, market.precision.amount);

      if (amount === 0) {
        const message = 'Order amount is too small';
        this.emitAndLogError(message);
        throw new Error(message);
      }

      payload.newSz = `${amount}`;
    }

    let responseBody: Record<string, any> | undefined;
    try {
      const response = await this.xhr.post(ENDPOINTS.UPDATE_ALGO_ORDER, payload);
      responseBody = response.data;
    } catch (err: any) {
      const message = err?.response?.data?.msg || err?.message || 'OKX trigger order amend failed';
      this.emitAndLogError(message);
      throw new Error(message);
    }

    if (responseBody?.code && responseBody.code !== '0') {
      const message = responseBody.msg || responseBody.code || 'OKX trigger order amend failed';
      this.emitAndLogError(message);
      throw new Error(message);
    }

    const data = responseBody?.data;
    const failed = data?.find?.((d: Record<string, any>) => (d.sCode && d.sCode !== '0') || d.sMsg);
    if (failed) {
      const message = failed.sMsg || failed.sCode || 'OKX trigger order amend failed';
      this.emitAndLogError(message);
      throw new Error(message);
    }

    return [order.id];
  };

  cancelOrders = async (orders: Order[]) => {
    const [algoOrders, normalOrders] = partition(orders, (o) => this.isAlgoOrder(o.type));

    if (normalOrders.length) await this.cancelNormalOrders(normalOrders);
    if (algoOrders.length) await this.cancelAlgoOrders(algoOrders);
  };

  cancelSymbolOrders = async (symbol: string) => {
    const orders = this.store.orders.filter((o) => o.symbol === symbol);
    await this.cancelOrders(orders);
  };

  setLeverage = async (symbol: string, inputLeverage: number) => {
    try {
      const market = this.store.marketsBySymbol[symbol];
      const position = this.store.positions.find((p) => p.symbol === symbol);

      if (!market) throw new Error(`Market ${symbol} not found on OKX`);
      if (!position) throw new Error(`Position ${symbol} not found on OKX`);

      const leverage = Math.min(Math.max(inputLeverage, market.limits.leverage.min), market.limits.leverage.max);

      if (position.leverage !== leverage) {
        await this.xhr.post(ENDPOINTS.SET_LEVERAGE, {
          instId: market.id,
          lever: `${leverage}`,
          mgnMode: 'cross',
        });

        const positions = await this.fetchPositions();
        this.store.update({ positions });
      }
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      const positions = await this.fetchPositions();
      this.store.addOrUpdatePositions(positions);
      throw new Error(err?.response?.data?.msg || err?.message);
    }
  };

  getOrFetchPositionModeAndAccLevel = async (force = false) => {
    if (this._accountPositionMode && this._accountLevel && !force) {
      return {
        positionMode: this._accountPositionMode,
        accountLevel: this._accountLevel,
      };
    }
    try {
      const {
        data: {
          data: [{ posMode, acctLv }],
        },
      } = await this.xhr.get(ENDPOINTS.ACCOUNT_CONFIG);
      const isHedged = posMode === 'long_short_mode';
      const positionMode = isHedged ? PositionMode.Hedged : PositionMode.OneWay;
      this._accountPositionMode = positionMode;
      this._accountLevel = parseFloat(acctLv || '2');
      return {
        positionMode: this._accountPositionMode,
        accountLevel: this._accountLevel,
      };
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      throw new Error(err?.response?.data?.msg || err?.message);
    }
  };

  getOrFetchPositionMode = async (force = false) => {
    const { positionMode } = await this.getOrFetchPositionModeAndAccLevel(force);
    return positionMode;
  };

  getOrFetchAccountLevel = async (force = false) => {
    const { accountLevel } = await this.getOrFetchPositionModeAndAccLevel(force);
    return accountLevel;
  };

  changeMarginType = async (_isolated: boolean, _symbol?: string) => {
    this.emitAndLogError(`Only cross mode is supported`);
  };

  changePositionMode = async (hedged: boolean) => {
    if (this.store.positions.filter((p) => p.contracts > 0).length > 0) {
      this.emitAndLogError('Please close all positions before switching position mode');
      return;
    }

    try {
      const { data } = await this.xhr.post(ENDPOINTS.SET_POSITION_MODE, {
        posMode: hedged ? 'long_short_mode' : 'net_mode',
      });
      if (data.msg === '') {
        this._accountPositionMode = hedged ? PositionMode.Hedged : PositionMode.OneWay;
        const positions = await this.fetchPositions();
        this.store.update({ positions });
      } else {
        throw new Error(data.msg);
      }
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      const positions = await this.fetchPositions();
      this.store.addOrUpdatePositions(positions);
      throw new Error(err?.response?.data?.msg || err?.message);
    }
  };

  afterMethodCalledInWorker = <M extends CallableMethodsInWorker>(
    result: ReturnType<this[M]>,
    method: M,
    ...args: Parameters<this[M]>
  ) => {
    if (method === 'changePositionMode') {
      this._accountPositionMode = args[0] ? PositionMode.Hedged : PositionMode.OneWay;
    }
  };

  placeOrder = async (opts: PlaceOrderOpts) => {
    return await this.placeOrders([opts]);
  };

  placeOrders = async (opts: PlaceOrderOpts[]) => {
    this.assertCallerClientOrderIds(opts);
    // need to ensure position mode is loaded
    await this.getOrFetchPositionMode();
    const [algoOrders, normalOrders] = partition(opts, (o) => this.isAlgoOrder(o.type));

    const normalOrdersOpts = normalOrders.flatMap((o) => this.formatCreateOrder(o));

    if (normalOrdersOpts.some((o) => parseFloat(o.sz) === 0)) {
      this.emitAndLogError(`Total order amount is too small`);
      return [];
    }

    const derivedAlogOrders = this.deriveAlgoOrdersFromNormalOrdersOpts(normalOrders);
    const algoOrdersOpts = [...algoOrders, ...derivedAlogOrders]
      .sort((a, b) => this.getAlgoOrderPlacementPriority(a.type) - this.getAlgoOrderPlacementPriority(b.type))
      .map(this.formatAlgoOrder);

    return [...(await this.placeOrderBatch(normalOrdersOpts)), ...(await this.placeAlgoOrderBatch(algoOrdersOpts))];
  };

  private getCtValCcy = (market: Market) => {
    return String(market.info?.ctValCcy || '').toUpperCase();
  };

  private toFiniteNumber = (value: unknown) => {
    const parsed = parseFloat(String(value ?? ''));
    return Number.isFinite(parsed) ? parsed : 0;
  };

  private resolveReferencePrice = (market: Market, referencePrice?: number) => {
    const fromReference = this.toFiniteNumber(referencePrice);
    if (fromReference > 0) return fromReference;

    const ticker = this.getTicker(market);
    const fromMark = this.toFiniteNumber(ticker?.mark);
    if (fromMark > 0) return fromMark;

    const fromLast = this.toFiniteNumber(ticker?.last);
    if (fromLast > 0) return fromLast;

    return 0;
  };

  private getTicker = (market: Market): Ticker | undefined => {
    return this.store.tickersById[market.id] || this.store.tickersBySymbol[market.symbol];
  };

  private hasExternalTriggerReferencePrice = (market: Market, source: 'mark' | 'index', ticker: Ticker | undefined) => {
    const config = this.options.externalPublicData;

    if (config?.type === 'in-process') {
      const methodName = source === 'mark' ? 'hasMarkPriceLoaded' : 'hasIndexPriceLoaded';
      const sourceExchange = config.source as Partial<Record<typeof methodName, (marketId: string) => boolean>>;
      if (sourceExchange[methodName]?.(market.id)) return true;
    }

    if (!config) return false;

    const sourcePrice = this.toFiniteNumber(ticker?.[source]);
    const last = this.toFiniteNumber(ticker?.last);
    return sourcePrice > 0 && last > 0 && sourcePrice !== last;
  };

  private resolveTriggerReferencePrice = (market: Market, triggerPxType?: string, fallbackPrice?: number) => {
    const ticker = this.getTicker(market);
    const fallback = this.toFiniteNumber(fallbackPrice);

    if (triggerPxType === 'index') {
      if (!this.indexPriceMarketIds.has(market.id) && !this.hasExternalTriggerReferencePrice(market, 'index', ticker)) {
        return 0;
      }
      const fromIndex = this.toFiniteNumber(ticker?.index);
      return fromIndex > 0 ? fromIndex : 0;
    }

    if (triggerPxType === 'mark') {
      if (!this.markPriceMarketIds.has(market.id) && !this.hasExternalTriggerReferencePrice(market, 'mark', ticker)) {
        return 0;
      }
      const fromMark = this.toFiniteNumber(ticker?.mark);
      return fromMark > 0 ? fromMark : 0;
    }

    const fromLast = this.toFiniteNumber(ticker?.last);
    if (fromLast > 0) return fromLast;
    return this.resolveReferencePrice(market, fallback);
  };

  convertMarginValueToUsd = (
    amount: number,
    marginCurrency: string | undefined,
    market?: Market,
    referencePrice?: number,
  ) => {
    const finiteAmount = this.toFiniteNumber(amount);
    if (!market || !marginCurrency) return finiteAmount;

    const normalizedMarginCcy = marginCurrency.toUpperCase();
    if (market.isInverse && normalizedMarginCcy === market.base.toUpperCase()) {
      const px = this.resolveReferencePrice(market, referencePrice);
      if (!px) return 0;
      return finiteAmount * px;
    }

    return finiteAmount;
  };

  private convertUsdValueToMarginCurrency = (
    usdAmount: number,
    marginCurrency: string | undefined,
    market?: Market,
    referencePrice?: number,
  ) => {
    const finiteUsdAmount = this.toFiniteNumber(usdAmount);
    if (!market || !marginCurrency) return finiteUsdAmount;

    const normalizedMarginCcy = marginCurrency.toUpperCase();
    if (market.isInverse && normalizedMarginCcy === market.base.toUpperCase()) {
      const px = this.resolveReferencePrice(market, referencePrice);
      if (!px) return 0;
      return finiteUsdAmount / px;
    }

    return finiteUsdAmount;
  };

  contractsToBase = (contractsNative: number, market: Market, referencePrice?: number) => {
    const contractSize = market.contractSize || 1;
    const ctValCcy = this.getCtValCcy(market);

    // Inverse (coin-margined): contract value is quote-denominated, convert quote exposure to base.
    if (market.isInverse && ctValCcy === market.quote.toUpperCase()) {
      const px = this.resolveReferencePrice(market, referencePrice);
      if (!px) return 0;
      return (contractsNative * contractSize) / px;
    }

    return multiply(contractsNative, contractSize);
  };

  private contractsToNotional = (contractsNative: number, market: Market, markPrice: number) => {
    const contractSize = market.contractSize || 1;
    const ctValCcy = this.getCtValCcy(market);

    // Inverse (coin-margined): notional is quote-denominated contract value.
    if (market.isInverse && ctValCcy === market.quote.toUpperCase()) {
      return contractsNative * contractSize;
    }

    return contractsNative * markPrice * contractSize;
  };

  mapOrders = (orders: Array<Record<string, any>>) => {
    return orders.reduce<Order[]>((acc, o: Record<string, any>) => {
      const market = this.store.marketsById[o.instId];
      if (!market) return acc;

      if (o.ordType === 'trigger') {
        const amount = parseFloat(o.sz || '0');
        const size = Math.abs(amount);
        const triggerPrice = parseFloat(o.triggerPx || '0');
        const orderPrice = parseFloat(o.ordPx || o.orderPx || '0');
        const referencePrice = parseFloat(o.last || o.px || o.ordPx || o.orderPx || o.triggerPx || '0');
        const sizeInBase = this.contractsToBase(size, market, referencePrice);
        const isReduceOnly = o.reduceOnly === 'true' || o.reduceOnly === true;
        const type = isReduceOnly ? this.getReduceOnlyTriggerOrderType(o, market) : OrderType.Conditional;
        const triggerType = parseOKXTriggerPxType(o.triggerPxType);
        const suffix = type === OrderType.StopLoss ? '_sl' : type === OrderType.TakeProfit ? '_tp' : '';
        const triggerTypeFields =
          type === OrderType.StopLoss
            ? { slTriggerType: triggerType }
            : type === OrderType.TakeProfit
              ? { tpTriggerType: triggerType }
              : { triggerType };

        return [
          ...acc,
          {
            id: `${o.algoId}${suffix}`,
            status: OrderStatus.Open,
            symbol: market.symbol,
            type,
            side: ORDER_SIDE[o.side],
            price: triggerPrice,
            amount: size,
            filled: 0,
            remaining: sizeInBase,
            extra: {
              remainingNative: size,
              conditionalOrderPrice: Number.isFinite(orderPrice) && orderPrice > 0 ? orderPrice : undefined,
            },
            reduceOnly: isReduceOnly,
            lastUpdate: virtualClock.getCurrentTimeMs(),
            ...triggerTypeFields,
            timeInForce: OrderTimeInForce.GoodTillCancel,
            info: o,
          },
        ];
      }

      if (o.ordType === 'oco' || o.ordType === 'conditional') {
        const newOrders: Order[] = [];

        const amount = parseFloat(o.sz || '0');

        const size = Math.abs(amount);
        const algoReferencePrice = parseFloat(o.last || o.px || o.tpTriggerPx || o.slTriggerPx || '0');
        const sizeInBase = this.contractsToBase(size, market, algoReferencePrice);

        if (parseFloat(o.slTriggerPx)) {
          newOrders.push({
            id: `${o.algoId}_sl`,
            status: OrderStatus.Open,
            symbol: market.symbol,
            type: OrderType.StopLoss,
            side: ORDER_SIDE[o.side],
            price: parseFloat(o.slTriggerPx),
            amount: size,
            filled: 0,
            remaining: sizeInBase,
            extra: {
              remainingNative: size,
            },
            reduceOnly: o.reduceOnly == 'true',
            lastUpdate: virtualClock.getCurrentTimeMs(),
            slTriggerType: parseOKXTriggerPxType(o.slTriggerPxType),
            timeInForce: OrderTimeInForce.GoodTillCancel,
            info: o,
          });
        }

        if (parseFloat(o.tpTriggerPx)) {
          newOrders.push({
            id: `${o.algoId}_tp`,
            status: OrderStatus.Open,
            symbol: market.symbol,
            type: OrderType.TakeProfit,
            side: ORDER_SIDE[o.side],
            price: parseFloat(o.tpTriggerPx),
            amount: size,
            filled: 0,
            remaining: sizeInBase,
            extra: {
              remainingNative: size,
            },
            reduceOnly: o.reduceOnly == 'true',
            lastUpdate: virtualClock.getCurrentTimeMs(),
            tpTriggerType: parseOKXTriggerPxType(o.tpTriggerPxType),
            timeInForce: OrderTimeInForce.GoodTillCancel,
            info: o,
          });
        }

        return [...acc, ...newOrders];
      }

      if (o.ordType === 'move_order_stop') {
        const side = ORDER_SIDE[o.side];
        const existingPosition = this.store.positions.find((pos) => {
          return (
            pos.symbol === market.symbol &&
            (side === OrderSide.Sell ? pos.side === PositionSide.Long : pos.side === PositionSide.Short)
          );
        });

        const callbackSpread = parseFloat(o.callbackSpread);
        const moveTriggerPx = existingPosition
          ? subtract(
              Math.min(existingPosition.entryPrice, callbackSpread),
              Math.max(existingPosition.entryPrice, callbackSpread),
            )
          : parseFloat(o.moveTriggerPx);

        const price = adjust(
          side === OrderSide.Buy ? subtract(parseFloat(o.last), moveTriggerPx) : add(parseFloat(o.last), moveTriggerPx),
          market.precision.price,
        );

        return [
          ...acc,
          {
            id: `${o.algoId}_tsl`,
            status: OrderStatus.Open,
            symbol: market.symbol,
            type: OrderType.TrailingStopLoss,
            side: ORDER_SIDE[o.side],
            price: price || callbackSpread,
            amount: 0,
            filled: 0,
            remaining: 0,
            extra: {
              remainingNative: 0,
            },
            reduceOnly: true,
            lastUpdate: virtualClock.getCurrentTimeMs(),
            timeInForce: OrderTimeInForce.GoodTillCancel,
          },
        ];
      }

      const amount = parseFloat(o.sz);
      const filled = parseFloat(o.accFillSz);
      const remainingNative = subtract(amount, filled);
      const orderReferencePrice = parseFloat(o.px || o.avgPx || o.fillPx || o.last || '0');
      const remaining = this.contractsToBase(remainingNative, market, orderReferencePrice);
      const filledInBase = this.contractsToBase(filled, market, orderReferencePrice);

      const order = {
        id: o.ordId,
        status: ORDER_STATUS[o.state],
        symbol: market.symbol,
        type: ORDER_TYPE[o.ordType],
        side: ORDER_SIDE[o.side],
        price: parseFloat(o.px || '0'),
        amount: amount,
        filled: filledInBase,
        remaining,
        extra: {
          remainingNative,
          filledInPlacementUnits: filled,
        },
        reduceOnly: o.reduceOnly === 'true',
        lastUpdate: virtualClock.getCurrentTimeMs(),
        timeInForce: o.ordType === 'post_only' ? OrderTimeInForce.PostOnly : OrderTimeInForce.GoodTillCancel,
        info: o,
      };

      return [...acc, order];
    }, []);
  };

  mapPositions = (data: Array<Record<string, any>>) => {
    if (!this._accountPositionMode) {
      throw new Error('Position mode not loaded');
    }
    return data.reduce((acc: Position[], p: Record<string, any>) => {
      const market = this.store.marketsById[p.instId];
      const ticker = this.store.tickersById[p.instId];
      if (!market) return acc;

      const pos = parseFloat(p.pos);
      const contracts = pos || 0;

      let side =
        p.posSide === 'short'
          ? PositionSide.Short
          : p.posSide === 'long'
            ? PositionSide.Long
            : pos < 0
              ? PositionSide.Short
              : PositionSide.Long;
      const markPrice = ticker?.mark || 0;
      // okx allows non whole number of contracts
      const contractsNative = Math.abs(contracts);
      const contractsReferencePrice = markPrice || parseFloat(p.avgPx || '0');
      const sizeNotional = this.contractsToNotional(contractsNative, market, markPrice);
      const sizeInBase = this.contractsToBase(contractsNative, market, contractsReferencePrice);
      const unrealizedPnlRaw = this.toFiniteNumber(p.upl);
      const unrealizedPnl = this.convertMarginValueToUsd(unrealizedPnlRaw, p.ccy, market, markPrice);

      const id = createPositionId(market.symbol, side, this._accountPositionMode!);
      const existingPosition = this.store.positions.find((pos) => pos.id === id);

      const position: Position = {
        id: createPositionId(market.symbol, side, this._accountPositionMode!),
        symbol: market.symbol,
        side,
        entryPrice: contracts ? parseFloat(p.avgPx) : 0,
        //notional: contracts ? Math.abs(parseFloat(p.notionalUsd)) : 0,
        notional: sizeNotional,
        // leverage: this.leverageHash[market.id] || 0,
        leverage: parseFloat(p.lever || existingPosition?.leverage || '0'),
        unrealizedPnl: contracts ? adjust(unrealizedPnl, market.precision.price) : 0,
        contracts: sizeInBase,
        liquidationPrice: parseFloat(p.liqPx || '0'),
        marginType: p.mgnMode === 'cross' ? MarginType.Cross : MarginType.Isolated,
        positionMode: this._accountPositionMode || PositionMode.OneWay,
        extra: {
          contractsNative,
        },
        info: p,
      };

      return [...acc, position];
    }, []);
  };

  private formatAlgoOrder = (opts: PlaceOrderOpts) => {
    const market = this.store.marketsBySymbol[opts.symbol];

    if (!market) {
      throw new Error(`Market ${opts.symbol} not found on OKX`);
    }

    const pAmount = market.precision.amount;

    const amount = adjust(opts.amount, pAmount);

    const pPrice = market.precision.price;
    const price = opts.price ? adjust(opts.price, pPrice) : null;

    const req: Record<string, any> = omitUndefined({
      instId: market.id,
      tdMode: 'cross',
      algoClOrdId: `${BROKER_ID}${uuid()}`.slice(0, 32),
      tag: BROKER_ID,
      side: inverseObj(ORDER_SIDE)[opts.side],
      posSide: this.getPositionSide(opts),
      ordType:
        opts.type === OrderType.Conditional
          ? 'trigger'
          : opts.type === OrderType.TrailingStopLoss
            ? 'move_order_stop'
            : 'conditional',
    });

    if (opts.type === OrderType.Conditional) {
      const triggerPrice = opts.triggerPrice ? adjust(opts.triggerPrice, pPrice) : 0;
      if (!Number.isFinite(triggerPrice) || triggerPrice <= 0) {
        const message = 'Trigger price is required for OKX opening conditional orders';
        this.emitAndLogError(message);
        throw new Error(message);
      }
      if (!Number.isFinite(amount) || amount <= 0) {
        const message = 'Order amount is too small';
        this.emitAndLogError(message);
        throw new Error(message);
      }

      req.sz = `${amount}`;
      req.triggerPx = `${triggerPrice}`;
      req.orderPx = price ? `${price}` : '-1';
      req.triggerPxType = getOKXTriggerPxType(opts.triggerType);
    }

    if (opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit) {
      if (+amount !== 0) {
        req.sz = `${amount}`;
      } else {
        req.closeFraction = '1';
      }
      // reduce regardless of one-way or hedged modes
      req.reduceOnly = true;
    }

    if (opts.type === OrderType.StopLoss) {
      req.slTriggerPx = `${price}`;
      req.slOrdPx = '-1';
      req.slTriggerPxType = getOKXTriggerPxType(opts.slTriggerType ?? OrderStopTriggerType.MarkPrice);
    }

    if (opts.type === OrderType.TakeProfit) {
      req.tpTriggerPx = `${price}`;
      req.tpOrdPx = '-1';
      req.tpTriggerPxType = getOKXTriggerPxType(opts.tpTriggerType);
    }

    if (opts.type === OrderType.TrailingStopLoss) {
      req.activePx = '';
      req.callbackRatio = '';
      req.callbackSpread = `${price}`;
      req.closeFraction = '1';
      //req.sz = `${amount}`;
    }

    return req;
  };

  private formatCreateOrder = (opts: PlaceOrderOpts) => {
    const market = this.store.marketsBySymbol[opts.symbol];

    if (!market) {
      throw new Error(`Market ${opts.symbol} not found on OKX`);
    }

    const maxSize = market.limits.amount.max;
    const pPrice = market.precision.price;

    const pAmount = market.precision.amount;

    if (!Number.isFinite(maxSize) || maxSize <= 0) {
      const message = `Market ${opts.symbol} maximum order size must be a finite positive number`;
      this.emitAndLogError(message);
      throw new Error(message);
    }

    if (!Number.isFinite(pAmount) || pAmount <= 0) {
      const message = `Market ${opts.symbol} amount precision must be a finite positive number`;
      this.emitAndLogError(message);
      throw new Error(message);
    }

    const amount = adjust(opts.amount, pAmount);
    if (!Number.isFinite(amount) || amount <= 0) {
      const message = `Order amount for ${opts.symbol} must be finite and positive after adjustment`;
      this.emitAndLogError(message);
      throw new Error(message);
    }

    const price = opts.price ? adjust(opts.price, pPrice) : null;

    const req = omitUndefined({
      instId: market.id,
      tag: BROKER_ID,
      tdMode: 'cross',
      side: inverseObj(ORDER_SIDE)[opts.side],
      ordType:
        opts.type === OrderType.Limit && opts.timeInForce === 'PostOnly' ? 'post_only' : REVERSE_ORDER_TYPE[opts.type],
      px: opts.type === OrderType.Limit ? `${price}` : undefined,
      reduceOnly: opts.reduceOnly || undefined,
      posSide: this.getPositionSide(opts),
    });

    const clientOrderId = this.resolveCallerClientOrderId(opts);
    const createPayload = (size: BigNumber) => ({
      ...req,
      clOrdId: clientOrderId ?? `${BROKER_ID}${uuid()}`.slice(0, 32),
      sz: size.toFixed(),
    });

    if (amount <= maxSize) {
      return [createPayload(new BigNumber(amount))];
    }

    const amountStep = new BigNumber(pAmount);
    const maxLotSize = new BigNumber(maxSize).dividedToIntegerBy(amountStep).multipliedBy(amountStep);
    if (maxLotSize.lte(0)) {
      const message = `Market ${opts.symbol} maximum order size is below its amount precision`;
      this.emitAndLogError(message);
      throw new Error(message);
    }

    const payloads: Array<Record<string, any>> = [];
    let remaining = new BigNumber(amount);
    while (remaining.gt(0)) {
      const lotSize = BigNumber.minimum(remaining, maxLotSize);
      payloads.push(createPayload(lotSize));
      remaining = remaining.minus(lotSize);
    }

    this.assertSingleNativeClientOrderId(clientOrderId, payloads.length);
    return payloads;
  };

  private placeOrderBatch = async (payloads: Array<Record<string, any>>) => {
    const responses = await mapSeries(chunk(payloads, 20), async (batch) => {
      try {
        const {
          data: { data },
        } = await this.xhr.post<{ data: Array<Record<string, any>> }>(ENDPOINTS.PLACE_ORDERS, batch);

        return data.reduce((acc: string[], o) => {
          if (o.ordId) {
            return [...acc, o.ordId];
          }

          throw new Error(o.sMsg);
        }, []);
      } catch (err: any) {
        const firstOrderError = err?.response?.data?.data?.[0];
        const orderMessage = firstOrderError?.sMsg
          ? `${firstOrderError.sMsg}${firstOrderError?.sCode ? ` (code ${firstOrderError.sCode})` : ''}`
          : '';
        const message = orderMessage || err?.response?.data?.msg || err?.message;
        this.emitAndLogError(message);
        throw new Error(message);
      }
    });

    return flatten(responses);
  };

  private placeAlgoOrderBatch = async (payloads: Array<Record<string, any>>) => {
    const responses = await mapSeries(payloads, async (payload) => {
      try {
        const {
          data: { data },
        } = await this.xhr.post<{ data: Array<Record<string, any>> }>(ENDPOINTS.PLACE_ALGO_ORDER, payload);

        return data.reduce((acc: string[], o) => {
          if (o.algoId) {
            return [...acc, o.algoId];
          }

          throw new Error(o.sMsg);
        }, []);
      } catch (err: any) {
        const firstOrderError = err?.response?.data?.data?.[0];
        const orderMessage = firstOrderError?.sMsg
          ? `${firstOrderError.sMsg}${firstOrderError?.sCode ? ` (code ${firstOrderError.sCode})` : ''}`
          : '';
        const message = orderMessage || err?.response?.data?.msg || err?.message;
        this.emitAndLogError(message);
        throw new Error(message);
      }
    });

    return flatten(responses);
  };

  private cancelNormalOrders = async (orders: Order[]) => {
    const batches = orders
      .filter((o) => !this.isAlgoOrder(o.type))
      .reduce((acc: Array<Record<string, any>>, o) => {
        const market = this.store.marketsBySymbol[o.symbol];
        if (!market) return acc;
        return [...acc, { instId: market.id, ordId: o.id }];
      }, []);

    await forEachSeries(chunk(batches, 20), async (batch) => {
      const {
        data: { data },
      } = await this.xhr.post(ENDPOINTS.CANCEL_ORDERS, batch);

      data.forEach((d: Record<string, any>) => {
        if (d.sMsg) this.emitAndLogError(d.sMsg);
      });
    });
  };

  private cancelAlgoOrders = async (orders: Order[]) => {
    const batches = orders
      .filter((o) => this.isAlgoOrder(o.type))
      .reduce((acc: Array<Record<string, any>>, o) => {
        const market = this.store.marketsBySymbol[o.symbol];
        if (!market) return acc;
        return [...acc, { instId: market.id, algoId: o.id.replace(/_[a-zA-Z]+$/, '') }];
      }, []);

    await forEachSeries(chunk(batches, 10), async (batch) => {
      const {
        data: { data },
      } = await this.xhr.post(ENDPOINTS.CANCEL_ALGO_ORDERS, batch);

      data.forEach((d: Record<string, any>) => {
        if (d.sMsg) this.emitAndLogError(d.sMsg);
      });
    });
  };

  private isAlgoOrder = (orderType: OrderType) => {
    return (
      orderType === OrderType.Conditional ||
      orderType === OrderType.StopLoss ||
      orderType === OrderType.TakeProfit ||
      orderType === OrderType.TrailingStopLoss
    );
  };

  private getAlgoOrderPlacementPriority = (orderType: OrderType) => {
    // Place protective stops before TP so partial failures do not leave positions unprotected.
    if (orderType === OrderType.StopLoss) return 0;
    if (orderType === OrderType.TrailingStopLoss) return 1;
    if (orderType === OrderType.TakeProfit) return 2;
    return 3;
  };

  private getPositionSide = (opts: Pick<PlaceOrderOpts, 'reduceOnly' | 'side' | 'type'>) => {
    if (!this._accountPositionMode) {
      throw new Error('Account position mode not loaded');
    }
    if (this._accountPositionMode === PositionMode.OneWay) return undefined;

    let side = opts.side === OrderSide.Buy ? 'long' : 'short';

    if (
      opts.type === OrderType.StopLoss ||
      opts.type === OrderType.TakeProfit ||
      opts.type === OrderType.TrailingStopLoss ||
      (opts.type !== OrderType.Conditional && opts.reduceOnly)
    ) {
      side = side === 'long' ? 'short' : 'long';
    }

    return side;
  };

  private getReduceOnlyTriggerOrderType = (order: Record<string, any>, market: Market) => {
    const side = ORDER_SIDE[order.side];
    const triggerPrice = parseFloat(order.triggerPx || '0');
    const referencePrice = this.resolveTriggerReferencePrice(
      market,
      order.triggerPxType,
      parseFloat(order.last || '0'),
    );

    if (!Number.isFinite(triggerPrice) || triggerPrice <= 0 || !referencePrice) {
      return OrderType.Conditional;
    }

    if (side === OrderSide.Sell) {
      return triggerPrice > referencePrice ? OrderType.TakeProfit : OrderType.StopLoss;
    }

    return triggerPrice < referencePrice ? OrderType.TakeProfit : OrderType.StopLoss;
  };
}
