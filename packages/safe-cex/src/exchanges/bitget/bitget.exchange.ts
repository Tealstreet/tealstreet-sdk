import type { Axios } from 'axios';
import type { ManipulateType } from 'dayjs';
import type { Store } from '../../store/store.interface';

import rateLimit from 'axios-rate-limit';
import dayjs from 'dayjs';
import chunk from 'lodash/chunk';
import flatten from 'lodash/flatten';
import groupBy from 'lodash/groupBy';
import omit from 'lodash/omit';
import partition from 'lodash/partition';
import times from 'lodash/times';
import uniqBy from 'lodash/uniqBy';
import { mapSeries } from 'p-iteration';

import {
  Balance,
  Candle,
  ExchangeName,
  ExchangeOptions,
  Execution,
  ExecutionsOptions,
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
} from '../../types';
import { createPositionId } from '../../utils/createPositionId';
import { inverseObj } from '../../utils/inverseObj';
import { loop } from '../../utils/loop';
import { omitUndefined } from '../../utils/omitUndefined';
import { adjust, multiply, subtract } from '../../utils/safe-math';
import { uuid } from '../../utils/uuid';
import { virtualClock } from '../../utils/virtualClock';
import { BaseExchange } from '../base';
import { createAPI } from './bitget.api';
import {
  ENDPOINTS,
  FETCH_POSITIONS_V2_RESPONSE,
  FETCH_SINGLE_ACCOUNT_RESPONSE,
  GRAFANA_ENDPOINTS_EVERY,
  GRAFANA_ENDPOINTS_PERIODIC,
  INTERVAL,
  ORDER_SIDE,
  ORDER_STATUS,
  ORDER_TYPE,
  POSITION_SIDE,
  TIME_IN_FORCE,
} from './bitget.types';
import { BitgetPrivateWebsocket } from './bitget.ws-private';
import { BitgetPublicWebsocket } from './bitget.ws-public';
import { BitgetSecondaryPublicWebsocket } from './bitget.ws-secondary-public';

export class BitgetExchange extends BaseExchange {
  xhr: Axios;
  unlimitedXHR: Axios;

  publicWebsocket: BitgetPublicWebsocket;
  secondsTickersWebsocket: BitgetSecondaryPublicWebsocket;
  thirdTickersWebsocket: BitgetSecondaryPublicWebsocket;
  privateWebsocket: BitgetPrivateWebsocket;

  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;
  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;

  // UTA vs Classic detection: v2 set-leverage couples long+short on Bitget; only
  // v3 honors per-side via `posSide`, but v3 endpoints return 40084 "Classic
  // Account mode" on non-UTA accounts. We can't distinguish UTA from Classic
  // from BALANCE/account responses (`assetMode` and `unionAvailable` are
  // populated on Classic too). Set definitively by `_detectAccountType()`
  // which probes a cheap v3 endpoint once during `start()`. Every v3-routing
  // method (current: setLeverage; future: anything else under /api/v3/) reads
  // this flag — keep it the single source of truth.
  protected isUta = false;

  // Short-lived dedup + cache for resolveSymbolPositioning's account-endpoint
  // fallback. A single order resolves BOTH position-mode (in formatCreateOrder)
  // and margin-type (in placeOrderBatch), and placeOrders() runs many
  // formatCreateOrder() in parallel — each fallback hits the 3-RPS-rate-limited
  // account endpoint. In-flight dedup collapses concurrent calls per symbol; the
  // short TTL lets the mode+margin pair reuse one fetch. Only successful (both
  // fields present) results are cached so a transient failure isn't sticky.
  private _positioningCache: Record<
    string,
    { at: number; value: { positionMode?: PositionMode; marginType?: MarginType } }
  > = {};
  private _positioningInflight: Record<string, Promise<{ positionMode?: PositionMode; marginType?: MarginType }>> = {};
  private static POSITIONING_CACHE_TTL_MS = 5_000;

  // private leverageHash: Record<string, number> = {};

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    this.xhr = this.createXhrWithSecondaryKey(opts, createAPI, { rateLimit: (xhr) => rateLimit(xhr, { maxRPS: 3 }) });
    this.unlimitedXHR = createAPI(opts, this);

    this.publicWebsocket = new BitgetPublicWebsocket(this);
    this.privateWebsocket = new BitgetPrivateWebsocket(this);
    this.secondsTickersWebsocket = new BitgetSecondaryPublicWebsocket(this, 1);
    this.thirdTickersWebsocket = new BitgetSecondaryPublicWebsocket(this, 2);
  }

  get apiProductType() {
    return this.options.testnet ? 'susdt-futures' : 'usdt-futures';
  }

  get apiMarginCoin() {
    return this.options.testnet ? 'SUSDT' : 'USDT';
  }

  private toFiniteNumber(value: unknown, fallback = 0): number {
    if (typeof value === 'number') {
      return Number.isFinite(value) ? value : fallback;
    }
    if (typeof value === 'string') {
      if (value.trim() === '') return fallback;
      const parsed = Number.parseFloat(value);
      return Number.isFinite(parsed) ? parsed : fallback;
    }
    return fallback;
  }

  calculateUpnlFromLastPrice(positions: Position[] = this.store.positions): number {
    return positions.reduce((acc, position) => {
      const ticker = this.store.tickersBySymbol[position.symbol];
      const contracts = Math.abs(this.toFiniteNumber(position.contracts));
      const entryPrice = this.toFiniteNumber(position.entryPrice);
      const apiUnrealizedPnl = this.toFiniteNumber(position.unrealizedPnl);

      if (contracts === 0) {
        return acc;
      }

      if (!ticker || this.toFiniteNumber(ticker.last) === 0) {
        return acc + apiUnrealizedPnl;
      }

      if (position.side !== PositionSide.Long && position.side !== PositionSide.Short) {
        return acc + apiUnrealizedPnl;
      }

      const lastPrice = this.toFiniteNumber(ticker.last);
      const direction = position.side === PositionSide.Long ? 1 : -1;
      const recalculatedUpnl = direction * (lastPrice - entryPrice) * contracts;

      return acc + this.toFiniteNumber(recalculatedUpnl, apiUnrealizedPnl);
    }, 0);
  }

  calculateAccountUpnl = (apiUpnl: number): number => {
    const normalizedApiUpnl = this.toFiniteNumber(apiUpnl);
    const hasOpenPositions = this.store.positions.some(
      (position) => Math.abs(this.toFiniteNumber(position.contracts)) > 0,
    );

    if (!hasOpenPositions) {
      return normalizedApiUpnl;
    }

    return this.calculateUpnlFromLastPrice(this.store.positions);
  };

  getAccount = async () => {
    const {
      data: { data },
    } = await this.xhr.get(ENDPOINTS.ACCOUNT_SPOT);

    return {
      userId: `${data.parentId || data.userId}`,
      subId: data.parentId ? `${data.userId}` : null,
      affiliateId: data.inviterId ? `${data.inviterId}` : null,
    };
  };

  validateAccount = async () => {
    try {
      // fetchBalance hits the BALANCE endpoint (validating the key) and throws on
      // auth failure, so it doubles as the validation call — one round-trip.
      const balance = await this.fetchBalance();
      return { error: '', balance };
    } catch (err: any) {
      const errorMsg = err?.response?.data?.msg || err?.message;

      // Detect environment mismatch and provide helpful message
      if (errorMsg?.toLowerCase().includes('environment is incorrect')) {
        const currentMode = this.options.testnet ? 'Demo/Testnet' : 'Live/Production';
        const suggestedMode = this.options.testnet ? 'Live/Production' : 'Demo/Testnet';
        const helpfulMsg = `API key environment mismatch: Your API key appears to be for ${suggestedMode} trading, but Tealstreet is configured for ${currentMode}. Please check your Testnet setting.`;
        this.emitAndLogError(helpfulMsg);
        return { error: helpfulMsg };
      }

      this.emitAndLogError(errorMsg);
      return { error: errorMsg || 'Invalid API key or secret' };
    }
  };

  dispose = () => {
    super.dispose();
    this.publicWebsocket.dispose();
    this.privateWebsocket.dispose();
    this.secondsTickersWebsocket.dispose();
    this.thirdTickersWebsocket.dispose();
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

      // start public websocket streams as soon as markets loaded.
      this.publicWebsocket.connectAndSubscribe();

      // loop fetch leverage brackets
      // this.fetchLeverageBrackets();

      // loop fetch positions leverage
      // this.fetchCurrentLeverage();

      const tickers = await this.fetchTickers();
      if (this.isDisposed) return;

      this.log(`Loaded ${Math.min(tickers.length, markets.length)} Bitget markets`);

      this.store.update({
        tickers,
        loaded: { ...this.store.loaded, tickers: true },
      });

      // start additional public ticker websocket streams
      this.secondsTickersWebsocket.connectAndSubscribe();
      this.thirdTickersWebsocket.connectAndSubscribe();
    }

    if (this.isPublicDataOnly) {
      this.store.update({
        loaded: { ...this.store.loaded, balance: true, positions: true, orders: true },
      });
      return;
    }

    this.privateWebsocket.connectAndSubscribe();

    await Promise.all([this.tick(), this._detectAccountType()]);
    if (this.isDisposed) return;

    this.log(`Ready to trade on Bitget`);

    const orders = await this.fetchOrders();
    if (this.isDisposed) return;

    this.log(`Loaded Bitget orders`);

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
        this.store.addOrUpdatePositionsFull(positions);

        let balance;
        if (count % 2 === 0) {
          balance = await this.fetchBalance();
          if (this.isDisposed) return;
        }

        // virtual TPSL orders on limits dont stream via ws
        // bitget caches rest responses for a few seconds so best to not overwrite limit orders here as
        // ws updates are newer...
        let orders;
        if (count % 4 === 0) {
          const fetchedNormalOrders = await this.fetchNormalOrders();
          const virtualTPSLOrders = fetchedNormalOrders.filter(
            (o) => o.type === OrderType.StopLoss || o.type === OrderType.TakeProfit,
          );
          const existingNonVirtualTPSLOrders = this.store.orders.filter(
            (o) => !o.id.endsWith('__stop_loss') && !o.id.endsWith('__take_profit'),
          );
          orders = [...existingNonVirtualTPSLOrders, ...virtualTPSLOrders];
        }

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

  // One-time probe: GET a cheap v3 endpoint and observe whether Bitget routes
  // the call (UTA) or returns 40084 "Classic Account mode" (Classic). Result
  // is sticky for the worker's lifetime; a probe failure other than 40084
  // (network, timeout) leaves `isUta=false` so we degrade safely to v2.
  private _detectAccountType = async (): Promise<void> => {
    try {
      const { data } = await this.xhr.get(ENDPOINTS.ACCOUNT_SETTINGS_V3);
      this.isUta = parseInt(data?.code || '0') === 0;
    } catch (err: any) {
      const code = err?.response?.data?.code;
      if (code === '40084') {
        this.isUta = false;
      } else {
        // Network or transient error — leave default (false) so we don't
        // mistakenly route Classic accounts to v3.
        this.log(`Bitget account type probe failed; assuming Classic: ${err?.message ?? err}`);
      }
    }
  };

  fetchBalance = async ({ preferFast = false }: { preferFast?: boolean } = {}) => {
    const client = preferFast ? this.unlimitedXHR : this.xhr;
    const {
      data: { data },
    } = await client.get<{ data: Array<Record<string, any>> }>(ENDPOINTS.BALANCE, {
      params: { productType: this.apiProductType },
    });

    const usdt = data.find((b) => b.marginCoin === this.apiMarginCoin);
    if (!usdt) return this.store.balance;

    // {
    //   "used": null,
    //   "free": 347.2238322,
    //   "total": 347.2238322,
    //   "upnl": 8.349379922,
    //   "info": {
    //     "marginCoin": "USDT",
    //     "locked": "11.24980215",
    //     "available": "347.2238322",
    //     "crossedMaxAvailable": "337.46804868",
    //     "isolatedMaxAvailable": "337.46804868",
    //     "maxTransferOut": "329.11866876",
    //     "accountEquity": "355.57321213",
    //     "usdtEquity": "355.573212130794",
    //     "btcEquity": "0.00328947755",
    //     "crossedRiskRate": "0.015549806351",
    //     "unrealizedPL": "8.349379922",
    //     "coupon": "0",
    //     "crossedUnrealizedPL": "",
    //     "isolatedUnrealizedPL": "",
    //     "grant": "0",
    //     "unionTotalMargin": "355.57321213",
    //     "unionAvailable": "337.46804868",
    //     "unionMm": "5.5290946",
    //     "assetList": [],
    //     "assetMode": "single",
    //     "isolatedMargin": "0",
    //     "crossedMargin": "13.5547525"
    //   }

    const accountEquity = this.toFiniteNumber(usdt.unionTotalMargin, this.toFiniteNumber(usdt.accountEquity));
    const available = this.toFiniteNumber(usdt.unionAvailable, this.toFiniteNumber(usdt.available));
    const apiUpnl = this.toFiniteNumber(
      usdt.unrealizedPL,
      this.toFiniteNumber(usdt.crossedUnrealizedPL) + this.toFiniteNumber(usdt.isolatedUnrealizedPL),
    );

    // Bitget's account-level unrealizedPL can intermittently be empty/stale.
    // Recalculate from current positions + last price for margin UI consistency.
    const upnl = this.calculateAccountUpnl(apiUpnl);

    const balance: Balance = {
      used: subtract(accountEquity, available),
      free: available,
      total: subtract(accountEquity, upnl),
      upnl,
      info: usdt,
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

            const productType = this.apiProductType;
            const marginCoin = this.apiMarginCoin;

            const { data } = await this.xhr.get<FETCH_SINGLE_ACCOUNT_RESPONSE>(ENDPOINTS.FETCH_SINGLE_ACCOUNT, {
              params: {
                symbol: symbol,
                productType,
                marginCoin,
              },
            });

            if (this.isDisposed) return [];

            if (parseInt(data.code || '0') !== 0) {
              this.emitAndLogError(data.msg);
              return [];
            }

            const response = data.data;
            const positionMode = response.posMode === 'hedge_mode' ? PositionMode.Hedged : PositionMode.OneWay;
            const marginType = response.marginMode === 'crossed' ? MarginType.Cross : MarginType.Isolated;

            const buyIsolatedLeverage = parseInt(response.isolatedLongLever);
            const sellIsolatedLeverage = parseInt(response.isolatedShortLever);
            const crossLeverage = parseInt(response.crossedMarginLeverage);

            if (
              positionMode === PositionMode.OneWay &&
              marginType === MarginType.Isolated &&
              buyIsolatedLeverage !== sellIsolatedLeverage
            ) {
              this.emitAndLogError(`Mismatched one-way isolated leverages for ${symbol}. Please set them to be equal.`);
            }

            if (positionMode === PositionMode.OneWay) {
              return [
                {
                  symbol,
                  side: PositionSide.Long,
                  leverage: marginType === MarginType.Cross ? crossLeverage : buyIsolatedLeverage,
                  positionMode,
                  marginType,
                },
              ];
            } else {
              return [
                {
                  symbol,
                  side: PositionSide.Long,
                  leverage: marginType === MarginType.Cross ? crossLeverage : buyIsolatedLeverage,
                  positionMode,
                  marginType,
                },
                {
                  symbol,
                  side: PositionSide.Short,
                  leverage: marginType === MarginType.Cross ? crossLeverage : sellIsolatedLeverage,
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
    const symbolsNeedingMetadata = symbols.filter((symbol) => {
      const symbolPositions = openPositionsForSymbols.filter((p) => p.symbol === symbol);
      if (symbolPositions.length === 0) return true;
      if (!symbolPositions.some((p) => p.positionMode === PositionMode.Hedged)) return false;

      const hasLong = symbolPositions.some((p) => p.side === PositionSide.Long);
      const hasShort = symbolPositions.some((p) => p.side === PositionSide.Short);
      return !hasLong || !hasShort;
    });
    const positionMetadatas = await this.fetchPositionMetadata(symbolsNeedingMetadata);
    const fakePositions = positionMetadatas
      .filter(
        (metadata) =>
          !openPositionsForSymbols.some(
            (position) =>
              position.symbol === metadata.symbol &&
              position.side === metadata.side &&
              position.positionMode === metadata.positionMode,
          ),
      )
      .map((metadata) => {
        return {
          id: createPositionId(metadata.symbol, metadata.side, metadata.positionMode),
          ...metadata,
          entryPrice: 0,
          notional: 0,
          unrealizedPnl: 0,
          realizedPnl: 0,
          contracts: 0,
          liquidationPrice: 0,
        };
      });
    return uniqBy([...openPositionsForSymbols, ...fakePositions], (position) => position.id);
  };

  fetchOpenPositions = async () => {
    const { data } = await this.xhr.get<FETCH_POSITIONS_V2_RESPONSE>(ENDPOINTS.POSITIONS_V2, {
      params: {
        productType: this.apiProductType,
        marginCoin: this.apiMarginCoin,
      },
    });

    if (parseInt(data.code || '0') !== 0) {
      // Detect environment mismatch and provide helpful message
      if (data.msg?.toLowerCase().includes('environment is incorrect')) {
        const currentMode = this.options.testnet ? 'Demo/Testnet' : 'Live/Production';
        const suggestedMode = this.options.testnet ? 'Live/Production' : 'Demo/Testnet';
        this.emitAndLogError(
          `API key environment mismatch: Your API key is for ${suggestedMode}, but configured for ${currentMode}. Check Testnet setting.`,
        );
      } else {
        this.emitAndLogError(data.msg);
      }
      return [];
    }

    const positions: Position[] = data.data.map((p) => {
      const contracts = parseFloat(p.total);
      const price = parseFloat(p.markPrice);

      const symbol = p.symbol.replace(`_${this.apiProductType.toUpperCase()}`, '');
      const positionMode = p.posMode === 'hedge_mode' ? PositionMode.Hedged : PositionMode.OneWay;
      const marginMode = p.marginMode === 'isolated' ? MarginType.Isolated : MarginType.Cross;
      const side = POSITION_SIDE[p.holdSide];
      const positionId = createPositionId(symbol, side, positionMode);

      const marginSize = p.marginSize != null ? parseFloat(p.marginSize) : undefined;
      const position: Position = {
        id: positionId,
        symbol,
        side,
        entryPrice: parseFloat(p.openPriceAvg),
        notional: multiply(contracts, price),
        leverage: parseFloat(p.leverage),
        unrealizedPnl: parseFloat(p.unrealizedPL || '0'),
        contracts,
        realizedPnl:
          (p.achievedProfits ? parseFloat(p.achievedProfits) : 0) +
          (p.totalFee ? parseFloat(p.totalFee) : 0) -
          (p.deductedFee ? parseFloat(p.deductedFee) : 0),
        liquidationPrice: this.resolveLiquidationPrice(positionId, p.liquidationPrice),
        marginSize: marginSize && marginSize > 0 ? marginSize : undefined,
        positionMode,
        marginType: marginMode,
        info: p,
        lastUpdate: virtualClock.getCurrentTimeMs(),
      };

      return position;
    });
    return positions;
  };

  protected resolveLiquidationPrice = (positionId: string, rawLiquidationPrice: unknown): number => {
    const parsedLiquidationPrice = parseFloat(String(rawLiquidationPrice ?? ''));
    if (Number.isFinite(parsedLiquidationPrice)) return parsedLiquidationPrice;
    return this.store.positions.find((position) => position.id === positionId)?.liquidationPrice || 0;
  };

  /**
   * Resolve market from symbol, handling various symbol formats.
   * Supports: BTCUSDT (native), BTC-PERP, BTC-USDT, etc.
   */
  private _resolveMarket(symbol: string): Market | undefined {
    // Try direct lookup first
    let market = this.store.marketsBySymbol[symbol];
    if (market) return market;

    // Normalize symbol: remove common suffixes and separators
    const normalized = symbol.toUpperCase().replace(/[-_]/g, '').replace(/PERP$/, '').replace(/SWAP$/, '');

    // Try normalized symbol
    market = this.store.marketsBySymbol[normalized];
    if (market) return market;

    // Try adding USDT suffix if not present
    if (!normalized.endsWith('USDT') && !normalized.endsWith('USD')) {
      market = this.store.marketsBySymbol[`${normalized}USDT`];
      if (market) return market;
    }

    // Extract base currency and find by base + quote
    const base = normalized.replace(/USDT$/, '').replace(/USD$/, '').replace(/BUSD$/, '');

    // Find market with matching base and expected quote
    return this.store.markets.find((m) => m.base === base && m.quote === this.apiMarginCoin);
  }

  fetchExecutions = async (opts: ExecutionsOptions): Promise<Execution[]> => {
    try {
      const { symbol, from, to } = opts;
      if (!symbol) {
        throw new Error('Symbol is required for fetchExecutions');
      }
      const normalizeTimestampToSeconds = (value: unknown): number => {
        const parsed = Number.parseInt(`${value ?? '0'}`, 10);
        if (!Number.isFinite(parsed) || parsed <= 0) {
          return 0;
        }
        return parsed >= 1_000_000_000_000 ? Math.floor(parsed / 1000) : parsed;
      };
      const market = this._resolveMarket(symbol);
      if (!market) {
        throw new Error(`Market ${symbol} not found`);
      }

      // Use the resolved market symbol (e.g., "BTCUSDT") for API calls
      const marketSymbol = market.symbol;

      const { data } = await this.xhr.get(ENDPOINTS.ORDER_HISTORY, {
        params: {
          productType: this.apiProductType,
          symbol: marketSymbol,
          limit: 100,
        },
      });

      if (parseInt(data.code || '0') !== 0) {
        this.emitAndLogError(data.msg);
        return [];
      }

      if (!data.data?.entrustedList || !Array.isArray(data.data.entrustedList)) {
        return [];
      }

      // Filter for filled orders (handle both 'state' and 'status' fields)
      let fills = data.data.entrustedList.filter((order: any) => order.state === 'filled' || order.status === 'filled');

      // Filter by time range if provided
      const fromSeconds = from ? normalizeTimestampToSeconds(from) : undefined;
      const toSeconds = to ? normalizeTimestampToSeconds(to) : undefined;
      if (from) {
        fills = fills.filter((fill: any) => {
          const timestamp = normalizeTimestampToSeconds(fill.uTime || fill.cTime || '0');
          return fromSeconds !== undefined ? timestamp >= fromSeconds : true;
        });
      }
      if (to) {
        fills = fills.filter((fill: any) => {
          const timestamp = normalizeTimestampToSeconds(fill.uTime || fill.cTime || '0');
          return toSeconds !== undefined ? timestamp <= toSeconds : true;
        });
      }

      const result = fills.map((order: any) => {
        const side = order.side === 'buy' ? OrderSide.Buy : OrderSide.Sell;
        const contracts = parseFloat(order.size || order.baseVolume || '0');
        const avgPrice = parseFloat(order.priceAvg || order.price || '0');
        const fee = parseFloat(order.fee || '0');
        // Prefer fill/update time over create time, and normalize to seconds for chart markers.
        const timestamp = normalizeTimestampToSeconds(order.uTime || order.cTime || '0');

        return {
          timestamp,
          symbol: marketSymbol,
          orderId: order.orderId,
          price: avgPrice,
          qty: contracts,
          side,
          commission: Math.abs(fee),
          commissionAsset: this.apiMarginCoin,
        } as Execution;
      });

      return result;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return [];
    }
  };

  fetchMarkets = async () => {
    const {
      data: { data },
    } = await this.xhr.get<{ data: Array<Record<string, any>> }>(ENDPOINTS.MARKETS, {
      params: { productType: this.apiProductType },
    });

    const markets: Market[] = data
      .filter((m) => m.quoteCoin === this.apiMarginCoin)
      .map((m) => {
        // Calculate price precision based on pricePlace and priceEndStep
        const priceDecimals = 10 / 10 ** (parseFloat(m.pricePlace) + 1);
        const pricePrecision = priceDecimals * parseFloat(m.priceEndStep);

        return {
          id: m.symbol,
          symbol: m.symbol,
          base: m.baseCoin,
          quote: m.quoteCoin,
          active: m.symbolStatus === 'normal' || m.symbolStatus === 'listed' || m.symbolStatus === 'limit_open',
          precision: {
            amount: parseFloat(m.sizeMultiplier),
            price: pricePrecision,
          },
          limits: {
            amount: {
              min: parseFloat(m.minTradeNum) || 0,
              max: parseFloat(m.maxTradeNum) || Infinity,
            },
            leverage: {
              min: parseFloat(m.minLever) || 1,
              max: parseFloat(m.maxLever) || 20,
            },
          },
          info: m,
        };
      });

    return markets;
  };

  // fetchCurrentLeverage = async () => {
  //   // 1. fetch all existing positions with v1 endpoint
  //   // this will tell us the current leverage for symbols where we had a position
  //   const {
  //     data: { data: allPositions },
  //   } = await this.xhr.get<{ data: Array<Record<string, any>> }>(ENDPOINTS.POSITIONS, {
  //     params: {
  //       productType: this.apiProductType,
  //       marginCoin: this.apiMarginCoin,
  //     },
  //   });
  //
  //   const symbolLeverage = uniqBy(
  //     allPositions.map((d) => ({ symbol: d.symbol, leverage: d.leverage })),
  //     'symbol',
  //   );
  //
  //   symbolLeverage.forEach((d) => {
  //     this.leverageHash[d.symbol] = d.leverage;
  //   });
  //
  //   // use allPositions opportunity to set if we are in hedge mode
  //   if (allPositions.length > 0) {
  //     const isHedged = allPositions.some((p) => p.holdMode === 'double_hold');
  //     this.store.setSetting('isHedged', isHedged);
  //   }
  //
  //   // 2. fetch all the remaining markets on account endpoint
  //   // this will tell us the current leverage for symbols where we never had a position
  //   const remainingMarkets = this.store.markets.filter((m) => !symbolLeverage.find((d) => d.symbol === m.symbol));
  //
  //   for (const market of remainingMarkets) {
  //     if (!this.isDisposed) {
  //       try {
  //         const {
  //           data: { data },
  //         } = await this.xhr.get(ENDPOINTS.ACCOUNT, {
  //           params: {
  //             symbol: market.id,
  //             marginCoin: this.apiMarginCoin,
  //           },
  //         });
  //
  //         this.leverageHash[market.id] = data.crossMarginLeverage;
  //       } catch (err: any) {
  //         this.emitAndLogError(err?.response?.data?.msg || err?.message);
  //       }
  //     }
  //   }
  // };
  //
  // fetchLeverageBrackets = async () => {
  //   for (const market of this.store.markets) {
  //     if (!this.isDisposed) {
  //       try {
  //         const {
  //           data: { data },
  //         } = await this.xhr.get(ENDPOINTS.SYMBOL_LEVERAGE, {
  //           params: { symbol: market.id },
  //         });
  //         this.store.updateMarket(market, {
  //           limits: {
  //             ...market.limits,
  //             leverage: {
  //               min: parseInt(data.minLeverage, 10),
  //               max: parseInt(data.maxLeverage, 10),
  //             },
  //           },
  //         });
  //       } catch (err: any) {
  //         this.emitAndLogError(err?.response?.data?.msg || err?.message);
  //       }
  //     }
  //   }
  // };

  fetchTickers = async () => {
    const {
      data: { data },
    } = await this.xhr.get(ENDPOINTS.TICKERS, {
      params: { productType: this.apiProductType },
    });

    const tickers: Ticker[] = data.reduce((acc: Ticker[], t: Record<string, any>) => {
      const market = this.store.marketsById[t.symbol];

      if (!market) return acc;

      const last = parseFloat(t.lastPr);

      const ticker: Ticker = {
        id: market.id,
        symbol: market.symbol,
        bid: parseFloat(t.bidPr),
        ask: parseFloat(t.askPr),
        last,
        mark: last,
        index: parseFloat(t.indexPrice),
        percentage: parseFloat(t.changeUtc24h) * 100,
        fundingRate: parseFloat(t.fundingRate),
        fundingTime: 0, // not provided by the API, will be fetched with WS
        openInterest: 0, // not provided by the API, will be fetched with WS
        volume: parseFloat(t.baseVolume),
        quoteVolume: parseFloat(t.quoteVolume),
      };

      return [...acc, ticker];
    }, []);

    return tickers;
  };

  fetchOrders = async () => {
    const normalOrders = await this.fetchNormalOrders();
    const algoOrders = await this.fetchAlgoOrders();
    const tslOrders = await this.fetchTrailingStopOrders();

    return [...normalOrders, ...algoOrders, ...tslOrders];
  };

  fetchNormalOrders = async () => {
    const allOrders: Array<Record<string, any>> = [];
    let endId: string | undefined;
    const maxIterations = 20; // Safety limit to prevent infinite loops
    let iterations = 0;

    while (iterations < maxIterations) {
      const params: Record<string, any> = {
        productType: this.apiProductType,
        limit: '200', // Maximum allowed by Bitget API
      };

      if (endId) {
        params.idLessThan = endId;
      }

      const {
        data: { data },
      } = await this.xhr.get<{ data: { entrustedList: Array<Record<string, any>>; endId: string } }>(ENDPOINTS.ORDERS, {
        params,
      });

      if (!Array.isArray(data?.entrustedList) || data.entrustedList.length === 0) {
        break;
      }

      allOrders.push(...data.entrustedList);

      // If endId is not provided or is the same, we've reached the end
      if (!data.endId || data.endId === endId) {
        break;
      }

      endId = data.endId;
      iterations++;
    }

    return allOrders.flatMap(this.mapOrder);
  };

  fetchAlgoOrders = async () => {
    // Bitget V2 orders-plan-pending with planType=profit_loss returns both
    // sized TP/SL (loss_plan/profit_plan) and position-level TP/SL (pos_loss/pos_profit).
    const allOrders: Array<Record<string, any>> = [];
    let endId: string | undefined;
    const maxIterations = 20;
    let iterations = 0;

    while (iterations < maxIterations) {
      const params: Record<string, any> = {
        productType: this.apiProductType,
        planType: 'profit_loss',
        limit: '200',
      };

      if (endId) {
        params.idLessThan = endId;
      }

      const {
        data: { data },
      } = await this.xhr.get<{ data: { entrustedList: Array<Record<string, any>>; endId?: string } }>(
        ENDPOINTS.ALGO_ORDERS,
        {
          params,
        },
      );

      if (!Array.isArray(data?.entrustedList) || data.entrustedList.length === 0) {
        break;
      }

      allOrders.push(...data.entrustedList);

      if (!data.endId || data.endId === endId) {
        break;
      }

      endId = data.endId;
      iterations++;
    }

    return allOrders.flatMap(this.mapOrder);
  };

  fetchTrailingStopOrders = async () => {
    const allOrders: Array<Record<string, any>> = [];
    let endId: string | undefined;
    const maxIterations = 20; // Safety limit to prevent infinite loops
    let iterations = 0;

    while (iterations < maxIterations) {
      const params: Record<string, any> = {
        productType: this.apiProductType,
        planType: 'track_plan',
        limit: '200', // Maximum allowed by Bitget API
      };

      if (endId) {
        params.idLessThan = endId;
      }

      const {
        data: { data },
      } = await this.xhr.get<{
        data: Array<Record<string, any>> | { entrustedList: Array<Record<string, any>>; endId?: string };
      }>(ENDPOINTS.ALGO_ORDERS, {
        params,
      });

      // Handle both response formats (array or object with entrustedList)
      let orders: Array<Record<string, any>> = [];
      if (Array.isArray(data)) {
        orders = data;
        // Array response doesn't support pagination, so we just return what we get
        return orders.flatMap(this.mapOrder);
      } else if (data?.entrustedList) {
        orders = data.entrustedList;

        if (orders.length === 0) {
          break;
        }

        allOrders.push(...orders);

        // If endId is not provided or is the same, we've reached the end
        if (!data.endId || data.endId === endId) {
          break;
        }

        endId = data.endId;
      } else {
        break;
      }

      iterations++;
    }

    return allOrders.flatMap(this.mapOrder);
  };

  fetchOHLCV = async (opts: OHLCVOptions) => {
    const limit = Math.min(opts.limit || 500, 1000);
    const interval = INTERVAL[opts.interval];
    const [, amount, unit] = opts.interval.split(/(\d+)/);

    const end = opts.to ? dayjs(opts.to) : dayjs();
    const start =
      !opts.limit && opts.from ? dayjs(opts.from) : end.subtract(parseFloat(amount) * limit, unit as ManipulateType);

    const {
      data: { data },
    } = await this.xhr.get(ENDPOINTS.KLINE, {
      params: {
        symbol: opts.symbol,
        granularity: interval,
        endTime: end.valueOf(),
        ...(end.valueOf() === start.valueOf() && { startTime: start.valueOf() }),
        productType: this.apiProductType,
        limit,
      },
    });

    if (!data) {
      return [];
    }

    const candles: Candle[] = data.map((c: string[]) => {
      return {
        timestamp: parseInt(c[0], 10) / 1000,
        open: parseFloat(c[1]),
        high: parseFloat(c[2]),
        low: parseFloat(c[3]),
        close: parseFloat(c[4]),
        volume: parseFloat(c[6]),
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
      await this.unlimitedXHR.post(ENDPOINTS.CANCEL_ALL_ORDERS, {
        productType: this.apiProductType,
        marginCoin: this.apiMarginCoin,
      });
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
    }
  };

  cancelOrders = async (orders: Order[]) => {
    const [virtualTPSLOrders, otherOrders] = partition<Order>(orders, this.isVirtualTPSLOrder);
    const [algoOrders, normalOrders] = partition<Order>(otherOrders, this.isAlgoOrder);
    const normalAndVirtualTPSL = [...normalOrders, ...virtualTPSLOrders];

    if (normalAndVirtualTPSL.length) await this.cancelNormalOrders(normalAndVirtualTPSL);
    if (algoOrders.length) await this.cancelAlgoOrders(algoOrders);
  };

  cancelNormalOrders = async (orders: Order[]) => {
    // might include virtual TPSL
    orders = orders.map((o) => ({
      ...o,
      id: o.id.split('__')[0],
    }));
    orders = uniqBy(orders, 'id');

    const grouped = groupBy(orders, 'symbol');

    for (const [key, symbolOrders] of Object.entries(grouped)) {
      const symbol = `${key}`;
      const lots = chunk(symbolOrders, 50);
      for (const lot of lots) {
        const orderIdList = lot.map((o) => ({ orderId: o.id }));

        try {
          await this.unlimitedXHR.post(ENDPOINTS.CANCEL_ORDERS, {
            symbol,
            orderIdList,
            marginCoin: this.apiMarginCoin,
            productType: this.apiProductType,
          });
        } catch (err: any) {
          this.emitAndLogError(err?.response?.data?.msg || err?.message);
          throw err;
        }
      }
    }
  };

  cancelAlgoOrders = async (orders: Order[]) => {
    for (const order of orders) {
      try {
        const planType = this.getCancelPlanType(order);
        await this.unlimitedXHR.post(ENDPOINTS.CANCEL_ALGO_ORDER, {
          orderId: order.id,
          symbol: `${order.symbol}`,
          marginCoin: this.apiMarginCoin,
          productType: this.apiProductType,
          planType,
        });
      } catch (err: any) {
        this.emitAndLogError(err?.response?.data?.msg || err?.message);
        throw err;
      }
    }
  };

  cancelSymbolOrders = async (symbol: string) => {
    try {
      const symbolOrders = this.store.orders.filter((o) => o.symbol === symbol);
      await this.cancelOrders(symbolOrders);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      throw err;
    }
  };

  // Resolve position-mode + margin-type for a symbol. Prefers an existing
  // position (authoritative), else falls back to fetchPositionMetadata, which
  // reads posMode/marginMode off the account endpoint WITHOUT needing an open
  // position. The fallback is load-bearing in hosted-exchange-server mode:
  // order writes run on the never-started main-thread mirror, whose
  // store.loaded.positions is false, so getOrFetchPositionsForSymbol returns []
  // for a flat symbol. The old code then threw before any order POST — and
  // because the throw was raw (not via emitAndLogError), it produced no toast
  // and no log, so orders silently never reached the exchange. bitget is the
  // only adapter that gates every order on this resolution (woo/bitmex don't).
  private async resolveSymbolPositioning(
    symbol: string,
  ): Promise<{ positionMode?: PositionMode; marginType?: MarginType }> {
    const positionsForSymbol = await this.getOrFetchPositionsForSymbol(symbol);
    const firstPosition = positionsForSymbol[0];
    if (firstPosition?.positionMode && firstPosition?.marginType) {
      return { positionMode: firstPosition.positionMode, marginType: firstPosition.marginType };
    }

    const cached = this._positioningCache[symbol];
    if (cached && Date.now() - cached.at < BitgetExchange.POSITIONING_CACHE_TTL_MS) {
      return cached.value;
    }
    const inflight = this._positioningInflight[symbol];
    if (inflight) return inflight;

    const promise = (async () => {
      const [metadata] = await this.fetchPositionMetadata([symbol]);
      const value = { positionMode: metadata?.positionMode, marginType: metadata?.marginType };
      if (value.positionMode && value.marginType) {
        this._positioningCache[symbol] = { at: Date.now(), value };
      }
      return value;
    })();
    this._positioningInflight[symbol] = promise;
    try {
      return await promise;
    } finally {
      delete this._positioningInflight[symbol];
    }
  }

  async getOrFetchPositionMode(symbol: string): Promise<PositionMode> {
    const { positionMode } = await this.resolveSymbolPositioning(symbol);
    if (!positionMode) {
      const msg = `Could not determine position mode for symbol ${symbol}`;
      this.emitAndLogError(msg);
      throw new Error(msg);
    }
    return positionMode;
  }

  async getOrFetchMarginType(symbol: string): Promise<MarginType> {
    const { marginType } = await this.resolveSymbolPositioning(symbol);
    if (!marginType) {
      const msg = `Could not determine margin type for symbol ${symbol}`;
      this.emitAndLogError(msg);
      throw new Error(msg);
    }
    return marginType;
  }

  placeOrder = async (opts: PlaceOrderOpts) => {
    if (this.isAlgoOrder(opts)) {
      const payload = await this.formatAlgoOrder(opts);
      return await this.placeAlgoOrders([payload]);
    }

    const payloads = await this.formatCreateOrder(opts);
    return await this.placeOrderBatch(payloads);
  };

  placeOrders = async (orders: PlaceOrderOpts[]) => {
    const [algoOrders, normalOrders] = partition(orders, this.isAlgoOrder);

    const normalOrdersOptsWithEmpty = (await Promise.all(normalOrders.map((o) => this.formatCreateOrder(o)))).flat();
    const normalOrdersOpts = normalOrdersOptsWithEmpty.filter((o) => o.size > 0);

    // const derivedAlgoOrders = this.deriveAlgoOrdersFromNormalOrdersOpts(orders);
    const algoOrdersOpts = algoOrders.map(this.formatAlgoOrder);

    return [...(await this.placeOrderBatch(normalOrdersOpts)), ...(await this.placeAlgoOrders(algoOrdersOpts))];
  };

  formatCreateOrder = async (opts: PlaceOrderOpts) => {
    // there are some hacks in here like TIF parsing, etc to support v2 quickly

    const market = this.store.marketsBySymbol[opts.symbol];

    if (!market) {
      throw new Error(`Market ${opts.symbol} not found`);
    }
    const positionMode = await this.getOrFetchPositionMode(opts.symbol);

    const [side, tradeSide] = this.getOrderSideV2(opts, positionMode);

    const maxSize = market.limits.amount.max;
    const pPrice = market.precision.price;

    const pAmount = market.precision.amount;

    const price = opts.price && opts.type !== OrderType.Market ? adjust(opts.price, pPrice) : undefined;

    const timeInForce = opts.timeInForce
      ? inverseObj(TIME_IN_FORCE)[opts.timeInForce]
      : inverseObj(TIME_IN_FORCE)[OrderTimeInForce.GoodTillCancel];

    const amount = adjust(opts.amount, pAmount);

    const req = omitUndefined({
      symbol: market.symbol,
      size: amount ? `${amount}` : undefined,
      price: price ? `${price}` : undefined,
      side,
      tradeSide,
      orderType: inverseObj(ORDER_TYPE)[opts.type],
      ...(opts.type !== OrderType.Market ? { force: timeInForce === 'normal' ? 'gtc' : timeInForce } : {}),
      reduceOnly: positionMode === PositionMode.Hedged ? undefined : opts.reduceOnly ? 'YES' : 'NO',
      presetStopSurplusPrice: opts.takeProfit ? `${adjust(opts.takeProfit, pPrice)}` : undefined,
      presetStopLossPrice: opts.stopLoss ? `${adjust(opts.stopLoss, pPrice)}` : undefined,
    });

    // Only split into lots when maxSize is a valid positive size. Guard against
    // maxSize <= 0: a zero max (seen on the hosted-server mirror, whose streamed
    // market can carry limits.amount.max === 0) made lots = ceil(amount / 0) =
    // Infinity, and lodash `times(Infinity)` returns [] — so formatCreateOrder
    // produced ZERO payloads and the order silently never POSTed.
    const shouldSplit = maxSize > 0 && amount > maxSize;
    const lots = shouldSplit ? Math.ceil(amount / maxSize) : 1;
    const rest = shouldSplit ? adjust(amount % maxSize, pAmount) : 0;
    const lotSize = adjust((amount - rest) / lots, pAmount);

    const payloads: Array<Record<string, any>> = times(lots, () => ({
      ...req,
      size: `${lotSize}`,
    }));

    if (rest) {
      // bitget's order field is `size`, not `quantity` — the remainder lot must
      // override `size` (req already carries the full `amount` as `size`), else
      // the remainder would re-place the full amount.
      payloads.push({ ...req, size: `${rest}` });
    }

    for (const payload of payloads) {
      payload.clientOid = uuid();
    }

    return payloads;
  };

  formatAlgoOrder = async (opts: PlaceOrderOpts) => {
    const market = this.store.marketsBySymbol[opts.symbol];
    if (!market) throw new Error(`Market ${opts.symbol} not found`);

    const payload: Record<string, any> = {
      marginCoin: this.apiMarginCoin,
      symbol: market.id,
      clientOid: uuid(),
      productType: this.apiProductType,
    };

    const positionMode = await this.getOrFetchPositionMode(opts.symbol);

    // If user is in oneWay mode, always set reduceOnly to "YES"
    if (positionMode === PositionMode.OneWay) {
      payload.reduceOnly = 'YES';
    }

    if (opts.type === OrderType.TakeProfit && opts.price) {
      const price = adjust(opts.price, market.precision.price);
      payload.triggerPrice = `${price}`;
      payload.holdSide = this.getAlgoOrderSide(opts, positionMode);
      const [planType, amount] = this.getOrderPlanTypeAndAmount(opts);
      payload.planType = planType;
      if (amount) payload.size = `${adjust(opts.amount, market.precision.amount)}`;
      // Bitget V2 API: mark_price (default) or fill_price (last price)
      payload.triggerType = opts.tpTriggerType === OrderStopTriggerType.LastPrice ? 'fill_price' : 'mark_price';
    }

    if (opts.type === OrderType.StopLoss && opts.price) {
      const price = adjust(opts.price, market.precision.price);
      payload.triggerPrice = `${price}`;
      payload.holdSide = this.getAlgoOrderSide(opts, positionMode);
      const [planType, amount] = this.getOrderPlanTypeAndAmount(opts);
      payload.planType = planType;
      if (amount) payload.size = `${adjust(opts.amount, market.precision.amount)}`;
      // Bitget V2 API: mark_price (default) or fill_price (last price)
      payload.triggerType = opts.slTriggerType === OrderStopTriggerType.LastPrice ? 'fill_price' : 'mark_price';
    }

    if (opts.type === OrderType.TrailingStopLoss && opts.price) {
      const price = adjust(opts.price, market.precision.price);
      const amount = adjust(opts.amount, market.precision.amount);

      const ticker = this.store.tickersBySymbol[opts.symbol];
      if (!ticker) throw new Error(`Ticker ${opts.symbol} not found`);

      const priceDistance = adjust(
        Math.max(ticker.last, opts.price!) - Math.min(ticker.last, opts.price!),
        market.precision.price,
      );

      const distancePercentage = Math.round(((priceDistance * 100) / ticker.last) * 10) / 10;

      payload.triggerPrice = `${price}`;
      payload.size = `${amount}`;
      payload.planType = 'track_plan';
      const marginType = await this.getOrFetchMarginType(opts.symbol);
      payload.marginMode = marginType === MarginType.Cross ? 'crossed' : 'isolated';
      payload.orderType = 'market';
      payload.triggerType = opts.slTriggerType === OrderStopTriggerType.LastPrice ? 'fill_price' : 'mark_price';
      payload.side = opts.side === OrderSide.Buy ? 'buy' : 'sell';
      if (positionMode === PositionMode.Hedged) payload.tradeSide = 'close';
      payload.callbackRatio = `${distancePercentage}`;
    }

    return payload;
  };

  placeOrderBatch = async (payloads: Array<Record<string, any>>) => {
    const newOrderIds: string[] = [];
    const grouped = groupBy(payloads, 'symbol');

    for (const [symbol, orders] of Object.entries(grouped)) {
      const marginType = await this.getOrFetchMarginType(symbol);
      const lots = chunk(orders, 50);
      for (const lot of lots) {
        try {
          const {
            data: { data },
          } = await this.unlimitedXHR.post(ENDPOINTS.BATCH_ORDERS, {
            symbol,
            productType: this.apiProductType,
            marginCoin: this.apiMarginCoin,
            marginMode: marginType,
            orderList: lot.map((o) => omit(o, 'symbol')),
          });

          const oIds = data?.successList?.map?.((obj: any) => obj.orderId);
          if (oIds) newOrderIds.push(...oIds);

          data?.failureList?.forEach?.((obj: any) => {
            this.emitAndLogError(obj.errorMsg);
            throw new Error(obj.errorMsg);
          });
        } catch (err: any) {
          this.emitAndLogError(err?.response?.data?.msg || err?.message);
          throw err;
        }
      }
    }

    return newOrderIds;
  };

  placeAlgoOrders = async (payloads: Array<Record<string, any>>) => {
    const newOrderIds: string[] = [];

    for (const payload of payloads) {
      try {
        const endpoint =
          payload.planType === 'track_plan' ? ENDPOINTS.PLACE_TRAILING_STOP_ORDER : ENDPOINTS.PLACE_ALGO_ORDER;

        const { data } = await this.unlimitedXHR.post(endpoint, payload);

        if (parseInt(data.code || '0') !== 0) {
          throw new Error(data.msg);
        }

        if (data?.data?.orderId) {
          newOrderIds.push(data.data.orderId);
        }
      } catch (err: any) {
        this.emitAndLogError(err, err?.response?.data?.msg || err?.message);
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

    const newFormattedPayloads = await this.formatCreateOrder({ ...parentOrder });

    const payload: Record<string, any> = omitUndefined({
      ...newFormattedPayloads[0],
      price: `${parentOrder.price}`,
      size: `${parentOrder.amount}`,
      presetStopSurplusPrice: takeProfitOrder ? `${takeProfitOrder.price}` : undefined,
      presetStopLossPrice: stopLossOrder ? `${stopLossOrder.price}` : undefined,
    });
    if ('amount' in update) {
      const amount = adjust(update.amount, market.precision.amount);
      payload.size = `${amount}`;
    }
    // if amending parent
    if (order.id === parentOrder.id) {
      if ('price' in update) {
        const price = adjust(update.price, market.precision.price);
        payload.price = `${price}`;
      }
    } else if (order.id.endsWith('__stop_loss')) {
      if ('price' in update) {
        const price = adjust(update.price, market.precision.price);
        payload.presetStopLossPrice = `${price}`;
      }
    } else if (order.id.endsWith('__take_profit')) {
      if ('price' in update) {
        const price = adjust(update.price, market.precision.price);
        payload.presetStopSurplusPrice = `${price}`;
      }
    }
    await this.cancelOrders([parentOrder]);
    return await this.placeOrderBatch([payload]);
  };

  private tryAmendLimitOrder = async (order: Order, update: UpdateOrderOpts['update']): Promise<string[] | null> => {
    const market = this.store.marketsBySymbol[order.symbol];
    if (!market) return null;

    // Keep parent+virtual TP/SL flow on existing cancel+replace path.
    const hasVirtualChildren = this.store.orders.some((o) => o.id.startsWith(`${order.id}__`));
    if (hasVirtualChildren) return null;

    const payload: Record<string, any> = {
      orderId: order.id,
      symbol: order.symbol,
      productType: this.apiProductType,
      newClientOid: uuid(),
    };

    // Bitget modify-order expects price and size together.
    // For single-field updates (drag price only / size only), pass the unchanged side explicitly.
    const nextPriceRaw = 'price' in update ? update.price : order.price;
    const nextSizeRaw = 'amount' in update ? update.amount : order.amount;

    if (!(nextPriceRaw > 0) || !(nextSizeRaw > 0)) {
      return [];
    }

    payload.newPrice = `${adjust(nextPriceRaw, market.precision.price)}`;
    payload.newSize = `${adjust(nextSizeRaw, market.precision.amount)}`;

    try {
      const { data } = await this.unlimitedXHR.post(ENDPOINTS.UPDATE_ORDER, payload);
      if (parseInt(data?.code || '0') !== 0) {
        return null;
      }
      return [data?.data?.orderId || order.id];
    } catch {
      return null;
    }
  };

  updateOrder = async ({ order, update }: UpdateOrderOpts) => {
    // Check for virtual TPSL orders (preset TP/SL on limit orders) first
    if (this.isVirtualTPSLOrder(order)) {
      const parentId = order.id.split('__')[0];
      const relatedOrders = this.store.orders.filter((o) => o.id.split('__')[0] === parentId);
      if (relatedOrders.length > 1) {
        return this.updateVirtualTpSlOrder({ order, update }, relatedOrders);
      }
    }

    // Standalone algo orders (not attached to a limit order)
    if (this.isAlgoOrder(order)) {
      return this.updateAlgoOrder({ order, update });
    }

    const shouldUseLegacyBitgetDragUpdate = Boolean(order.info?.__tealstreetUseLegacyBitgetDragUpdate);
    if (order.type === OrderType.Limit && !shouldUseLegacyBitgetDragUpdate) {
      const amendedOrderIds = await this.tryAmendLimitOrder(order, update);
      if (amendedOrderIds !== null) {
        return amendedOrderIds;
      }
    }

    const newOrder: Writable<PlaceOrderOpts> = {
      symbol: order.symbol,
      type: order.type,
      side: order.side,
      price: order.price,
      amount: order.amount,
      reduceOnly: order.reduceOnly || false,
      timeInForce: order.timeInForce,
    };

    if ('tpTriggerType' in order) newOrder.tpTriggerType = order.tpTriggerType;
    if ('slTriggerType' in order) newOrder.slTriggerType = order.slTriggerType;

    if ('price' in update) newOrder.price = update.price;
    if ('amount' in update) newOrder.amount = update.amount;

    await this.cancelOrders([order]);
    return await this.placeOrder(newOrder);
  };

  updateAlgoOrder = async ({ order, update }: UpdateOrderOpts) => {
    const market = this.store.marketsBySymbol[order.symbol];
    if (!market) throw new Error(`Market ${order.symbol} not found`);

    // Strip virtual TPSL suffix (e.g., "123__stop_loss" -> "123") to get actual order ID
    const orderId = order.id.split('__')[0];

    const payload: Record<string, any> = {
      orderId,
      marginCoin: this.apiMarginCoin,
      symbol: market.id,
      productType: this.apiProductType,
      //planType: order?.info?.planType || 'pos_loss',
    };

    if ('price' in update) {
      const price = adjust(update.price, market.precision.price);
      payload.triggerPrice = `${price}`;
    }

    if ('amount' in update && Boolean(update.amount) && +update.amount > 0) {
      const amount = adjust(update.amount, market.precision.amount);
      payload.size = `${amount}`;
    } else if (order.amount && order.amount > 0) {
      // Preserve the order's current size to avoid converting sized stops to position-level
      const amount = adjust(order.amount, market.precision.amount);
      payload.size = `${amount}`;
    } else {
      // Position-level orders (amount=0) require empty size
      payload.size = '';
    }

    try {
      const {
        data: { data },
      } = await this.unlimitedXHR.post(ENDPOINTS.UPDATE_ALGO_ORDER, payload);
      return data?.orderId ? [data.orderId] : [];
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return [];
    }
  };

  setLeverage = async (symbol: string, inputLeverage: number, side?: PositionSide) => {
    const market = this.store.marketsBySymbol[symbol];
    const position =
      (side ? this.store.positions.find((p) => p.symbol === symbol && p.side === side) : undefined) ??
      this.store.positions.find((p) => p.symbol === symbol);

    if (!market) throw new Error(`Market ${symbol} not found`);
    if (!position) throw new Error(`Position ${symbol} not found`);

    const leverage = Math.min(Math.max(inputLeverage, market.limits.leverage.min), market.limits.leverage.max);

    if (position.leverage !== leverage) {
      try {
        // Per-side ISO leverage in hedge mode is the only case where the long/short
        // distinction matters. The v2 endpoint silently ignores `holdSide` on UTA
        // accounts (couples both sides); v3 honors per-side via `posSide`. Route to
        // v3 only when both conditions hold; v2 stays the default for cross / one-way
        // / classic accounts.
        const isPerSide = position.positionMode === PositionMode.Hedged && position.marginType === MarginType.Isolated;
        const useV3 = this.isUta && isPerSide;

        const { data } = useV3
          ? await this.xhr.post(ENDPOINTS.SET_LEVERAGE_V3, {
              symbol: market.id,
              coin: this.apiMarginCoin,
              category: this.apiProductType.toUpperCase(),
              leverage: String(leverage),
              posSide: side === PositionSide.Short ? 'short' : 'long',
            })
          : await this.xhr.post(ENDPOINTS.SET_LEVERAGE, {
              symbol: market.id,
              marginCoin: this.apiMarginCoin,
              productType: this.apiProductType,
              leverage,
              ...(isPerSide ? { holdSide: side === PositionSide.Short ? 'short' : 'long' } : {}),
            });

        if (parseInt(data.code || '0') !== 0) {
          throw new Error(data.msg);
        }

        const positions = await this.fetchPositions([symbol]);
        this.store.addOrUpdatePositions(positions);

        // this.leverageHash[market.id] = leverage;
        // this.store.updatePositions([
        //   [{ id: createPositionId(symbol, PositionSide.Long) }, { leverage }],
        //   [{ id: createPositionId(symbol, PositionSide.Short) }, { leverage }],
        // ]);
      } catch (err: any) {
        this.emitAndLogError(err, err?.response?.data?.msg || err?.message);
        throw err;
      }
    }
  };

  changeMarginType = async (isolated: boolean, symbol?: string) => {
    if (!symbol) {
      this.emitAndLogError('Symbol is required');
      throw new Error('Symbol is required');
    }
    try {
      const { data } = await this.xhr.post(ENDPOINTS.SET_MARGIN_MODE, {
        productType: this.apiProductType,
        marginCoin: this.apiMarginCoin,
        marginMode: isolated ? 'isolated' : 'crossed',
        symbol,
      });
      if (parseInt(data.code || '0') !== 0) {
        throw new Error(data.msg);
      }
      // Margin type is per-symbol — drop only this symbol's cached positioning
      // so the next order re-resolves the new marginMode (a flat account has no
      // store row to override the cache otherwise).
      delete this._positioningCache[symbol];
      delete this._positioningInflight[symbol];
      const positions = await this.fetchPositions([symbol]);
      this.store.addOrUpdatePositions(positions);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      throw err;
    }
  };

  changePositionMode = async (hedged: boolean) => {
    try {
      const { data } = await this.xhr.post(ENDPOINTS.SET_POSITION_MODE, {
        productType: this.apiProductType,
        posMode: hedged ? 'hedge_mode' : 'one_way_mode',
      });

      if (parseInt(data.code || '0') !== 0) {
        throw new Error(data.msg);
      }
      // Position mode is global (positionModeIsGlobal) — flush the whole
      // positioning cache so the next order re-resolves the new posMode. On a
      // flat account there's no store position row to override stale values.
      this._positioningCache = {};
      this._positioningInflight = {};
      const positions = await this.fetchPositions();
      this.store.update({ positions });
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
    const order: Writable<Order> = {
      id: o.orderId || o.ordId || o.id,
      status: ORDER_STATUS[o.status || o.state || o.planStatus],
      symbol: (o.symbol || o.instId).replace(`_${this.apiProductType.toUpperCase()}`, ''),
      type: ORDER_TYPE[o.orderType || o.ordType || o.planType],
      side: ORDER_SIDE[o.tradeSide || o.tS || o.side] || o.side,
      price: parseFloat(o.price || o.px || '0'),
      amount: parseFloat(o.size || o.sz || '0'),
      filled: parseFloat(o.baseVolume || o.filledQty || o.accFillSz || '0'),
      lastUpdate: virtualClock.getCurrentTimeMs(),
      reduceOnly: o.reduceOnly?.toUpperCase?.() === 'YES',
      remaining: subtract(
        parseFloat(o.size || o.sz || '0'),
        parseFloat(o.baseVolume || o.filledQty || o.accFillSz || '0'),
      ),
      timeInForce: TIME_IN_FORCE[o.force || o.timeInForce || o.tiF] || OrderTimeInForce.GoodTillCancel,
      info: o,
    };

    if (o.planType) {
      order.type = ORDER_TYPE[o.planType];
      order.price = parseFloat(o.triggerPrice) || parseFloat(o.triggerPx);
      order.reduceOnly = true;

      // Parse trigger type from Bitget response: mark_price or fill_price (last price)
      const triggerType =
        o.triggerType === 'fill_price' ? OrderStopTriggerType.LastPrice : OrderStopTriggerType.MarkPrice;
      if (order.type === OrderType.TakeProfit) {
        order.tpTriggerType = triggerType;
      } else if (order.type === OrderType.StopLoss) {
        order.slTriggerType = triggerType;
      }

      if (order.info?.posMode === 'hedge_mode') {
        order.side = o.side === 'buy' ? OrderSide.Sell : OrderSide.Buy;
      }
    } else if (order.info?.reduceOnly === 'YES' && order.info?.posMode === 'hedge_mode') {
      order.side = o.side === 'buy' ? OrderSide.Sell : OrderSide.Buy;
    }

    let orders = [order];
    if (order.type === OrderType.Limit) {
      if (o.presetStopLossPrice || o.stopLossTriggerPrice) {
        orders.push({
          ...order,
          id: `${order.id}__stop_loss`,
          side: order.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy,
          type: OrderType.StopLoss,
          price: parseFloat(o.presetStopLossPrice || o.stopLossTriggerPrice),
        });
      }
      if (o.presetStopSurplusPrice || o.stopSurplusTriggerPrice || o.presetTakeProfitPrice) {
        orders.push({
          ...order,
          id: `${order.id}__take_profit`,
          side: order.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy,
          type: OrderType.TakeProfit,
          price: parseFloat(o.presetStopSurplusPrice || o.stopSurplusTriggerPrice || o.presetTakeProfitPrice),
        });
      }
    }
    return orders;
  };

  mapPosition = (p: Record<string, any>): Position => {
    // Guard against undefined/invalid payloads
    if (!p || typeof p !== 'object') {
      const fallbackSymbol = '';
      const side = PositionSide.Long;
      const positionMode = PositionMode.OneWay;
      return {
        id: createPositionId(fallbackSymbol, side, positionMode),
        symbol: fallbackSymbol,
        side,
        entryPrice: 0,
        notional: 0,
        leverage: 1,
        unrealizedPnl: 0,
        realizedPnl: 0,
        contracts: 0,
        liquidationPrice: 0,
        positionMode,
        marginType: MarginType.Cross,
        info: p,
        lastUpdate: virtualClock.getCurrentTimeMs(),
      };
    }

    // Log warning if required fields are missing
    if (!p.total || !p.openPriceAvg) {
      console.log(`[Bitget] mapPosition received incomplete data: total=${p.total}, openPriceAvg=${p.openPriceAvg}`);
    }

    // Handle cases where total might be undefined or missing
    const contracts = p.total ? parseFloat(p.total) : 0;
    const price = p.openPriceAvg ? parseFloat(p.openPriceAvg) : 0;

    // Handle different field names between REST API and WebSocket
    const symbol = (p.instId || p.symbol || '').replace(`_${this.apiProductType.toUpperCase()}`, '');
    const positionMode = p.posMode === 'hedge_mode' ? PositionMode.Hedged : PositionMode.OneWay;
    const marginMode = p.marginMode === 'crossed' ? MarginType.Cross : MarginType.Isolated;
    const side = POSITION_SIDE[p.holdSide];

    const marginSize = p.marginSize != null ? parseFloat(p.marginSize) : undefined;
    const positionId = createPositionId(symbol, side, positionMode);
    const position: Position = {
      id: positionId,
      symbol,
      side,
      entryPrice: price,
      notional: multiply(contracts, price),
      leverage: p.leverage ? parseFloat(p.leverage) : 1,
      unrealizedPnl: p.unrealizedPL ? parseFloat(p.unrealizedPL) : 0,
      realizedPnl:
        (p.achievedProfits ? parseFloat(p.achievedProfits) : 0) +
        (p.totalFee ? parseFloat(p.totalFee) : 0) -
        (p.deductedFee ? parseFloat(p.deductedFee) : 0),
      contracts,
      liquidationPrice: this.resolveLiquidationPrice(positionId, p.liquidationPrice),
      marginSize: marginSize && marginSize > 0 ? marginSize : undefined,
      positionMode,
      marginType: marginMode,
      info: p,
      lastUpdate: virtualClock.getCurrentTimeMs(),
    };

    return position;
  };

  private getOrderSideV2 = (opts: PlaceOrderOpts, positionMode: PositionMode) => {
    let side = opts.side === OrderSide.Buy ? 'buy' : 'sell';
    let tradeSide = undefined;
    if (positionMode === PositionMode.Hedged) {
      if (opts.reduceOnly) {
        tradeSide = 'close';
        side = opts.side === OrderSide.Buy ? 'sell' : 'buy';
      } else {
        tradeSide = 'open';
      }
    }
    return [side, tradeSide];
  };

  private getAlgoOrderSide = (opts: PlaceOrderOpts, positionMode: PositionMode) => {
    // For two-way (hedged) position mode
    if (positionMode === PositionMode.Hedged) {
      if (opts.type === OrderType.TakeProfit) {
        if (opts.side === OrderSide.Buy) return 'short';
        if (opts.side === OrderSide.Sell) return 'long';
      }

      if (opts.type === OrderType.StopLoss) {
        if (opts.side === OrderSide.Buy) return 'short';
        if (opts.side === OrderSide.Sell) return 'long';
      }

      if (opts.type === OrderType.TrailingStopLoss) {
        if (opts.side === OrderSide.Buy) return 'short';
        if (opts.side === OrderSide.Sell) return 'long';
      }
    }
    // For one-way position mode — Bitget V2 uses 'buy'/'sell' for holdSide
    else {
      if (opts.type === OrderType.TakeProfit) {
        if (opts.side === OrderSide.Buy) return 'sell';
        if (opts.side === OrderSide.Sell) return 'buy';
      }

      if (opts.type === OrderType.StopLoss) {
        if (opts.side === OrderSide.Buy) return 'sell';
        if (opts.side === OrderSide.Sell) return 'buy';
      }

      if (opts.type === OrderType.TrailingStopLoss) {
        if (opts.side === OrderSide.Buy) return 'sell';
        if (opts.side === OrderSide.Sell) return 'buy';
      }
    }

    throw new Error(`Unknown algo order side: ${opts.type} - ${opts.side}`);
  };

  private isVirtualTPSLOrder = (order: Order) => {
    return order.id.endsWith('__stop_loss') || order.id.endsWith('__take_profit');
  };

  private isAlgoOrder = (opts: PlaceOrderOpts) => {
    return (
      opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit || opts.type === OrderType.TrailingStopLoss
    );
  };

  // Get the correct planType for canceling an algo order.
  // Uses the raw planType from order.info (which comes from the API/WS response)
  // rather than computing it from position state, which is unreliable if the
  // position has been closed before the cancel request.
  private getCancelPlanType = (order: Order) => {
    const rawPlanType = order.info?.planType;
    if (rawPlanType) {
      // Map WS short codes to REST-compatible planType values
      const planTypeMap: Record<string, string> = {
        psl: 'pos_loss',
        ptp: 'pos_profit',
        pos_loss: 'pos_loss',
        pos_profit: 'pos_profit',
        loss_plan: 'loss_plan',
        profit_plan: 'profit_plan',
        track_plan: 'track_plan',
        sl: 'loss_plan',
        tp: 'profit_plan',
      };
      if (planTypeMap[rawPlanType]) return planTypeMap[rawPlanType];
    }
    // Fallback to computed planType
    const [planType] = this.getOrderPlanTypeAndAmount(order);
    return planType;
  };

  private getOrderPlanTypeAndAmount = (opts: PlaceOrderOpts): [string, number] => {
    if (opts.type === OrderType.TrailingStopLoss) return ['track_plan', opts.amount];

    let infiniteStop = true;
    if (opts.amount !== 0) {
      const foundPosition = this.store.positions.find(
        (p) =>
          p.symbol === opts.symbol &&
          p.side === (opts.side === OrderSide.Buy ? PositionSide.Short : PositionSide.Long) &&
          p.contracts >= opts.amount,
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
