import type { Axios } from 'axios';
import type { Store } from '../../../store/store.interface';

import rateLimit from 'axios-rate-limit';
import dayjs, { ManipulateType } from 'dayjs';

import { numberToString, parsePrecision } from '@tealstreet/ccxt-compat/src/functions/ccxt_exchange';
import {
  DECIMAL_PLACES,
  decimalToPrecision,
  NO_PADDING,
  precisionFromString,
  ROUND,
  SIGNIFICANT_DIGITS,
} from '@tealstreet/ccxt-compat/src/functions/number';
import { safeString } from '@tealstreet/ccxt-compat/src/functions/type';
import {
  Balance,
  Candle,
  CoinBalance,
  ExchangeName,
  ExchangeOptions,
  LogSeverity,
  MarginType,
  Market,
  OHLCVOptions,
  Order,
  OrderBook,
  OrderSide,
  OrderTimeInForce,
  OrderType,
  PlaceOrderOpts,
  PositionMode,
  PositionSide,
  Ticker,
  UpdateOrderOpts,
} from '../../../types';
import { createPositionId } from '../../../utils/createPositionId';
import { createWalletFromPrivateKey } from '../../../utils/ethereum/createWalletFromPrivateKey';
import { validateWalletAndPrivateKey } from '../../../utils/ethereum/validateWalletAndPrivateKey';
import { v } from '../../../utils/get-key';
import { loop } from '../../../utils/loop';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseExchange } from '../../../exchanges/base';
import { createAPI } from './hyperliquid.api';
import { hyperliquidClientOrderIdRule, mintHyperliquidCloid } from '../../../exchanges/hyperliquid/hyperliquid.cloid';
import {
  orderRequestToOrderWire,
  orderWiresToNormalTPSLAction,
  orderWiresToOrderAction,
  signL1Action,
} from './hyperliquid.signing';
import { ENDPOINTS } from '../../../exchanges/hyperliquid/hyperliquid.types';
import { calculatePricePrecision } from './hyperliquid.utils';
import {
  AssetCtx,
  GRAFANA_ENDPOINTS_EVERY,
  GRAFANA_ENDPOINTS_PERIODIC,
  Meta,
  SpotClearinghouseState,
} from './hyperliquidspot.types';
import { HyperliquidSpotPrivateWebSocket } from './hyperliquidspot.ws-private';
import { HyperliquidSpotPublicWebSocket } from './hyperliquidspot.ws-public';

export class HyperliquidSpotExchange extends BaseExchange {
  // Same cloid contract as perps (`hyperliquid.cloid.ts`); spot has no WS order handler to read records.
  protected readonly clientOrderIdRule = hyperliquidClientOrderIdRule;

  xhr: Axios;
  unlimitedXHR: Axios;
  isSpot = true;
  private spotBalanceRefreshInFlight: Promise<void> | null = null;
  private lastSpotBalanceRefreshAt = 0;
  private lastForcedSpotBalanceRefreshAt = 0;
  private static readonly SPOT_BALANCE_REFRESH_MS = 10_000;
  private static readonly SPOT_BALANCE_FORCE_REFRESH_COOLDOWN_MS = 1_500;

  public publicWebsocket: HyperliquidSpotPublicWebSocket;
  public privateWebsocket: HyperliquidSpotPrivateWebSocket;

  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;
  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;

  public activeAssetData: Map<
    string,
    {
      user: string;
      coint: string;
      leverage: {
        type: 'cross' | 'isolated';
        value: number;
      };
      maxTradeSzs: [number, number];
      availableToTrade: [string, string];
    }
  > = new Map();

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    this.xhr = rateLimit(createAPI(opts, this), { maxRPS: 3 });
    this.unlimitedXHR = createAPI(opts, this);

    this.publicWebsocket = new HyperliquidSpotPublicWebSocket(this);
    this.privateWebsocket = new HyperliquidSpotPrivateWebSocket(this);
  }

  symbolOrIdToCoin = (symbolOrId: string): string => {
    if (symbolOrId.includes('-')) {
      return symbolOrId.split('-')[0];
    } else {
      return symbolOrId;
    }
  };

  validateAccount = async () => {
    try {
      if (!validateWalletAndPrivateKey(this.options.secret, this.options.key)) {
        throw new Error('Invalid private key & wallet address pair');
      }

      return { error: '' };
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return { error: err?.response?.data?.msg || err?.message || 'Invalid API key or secret' };
    }
  };

  start = async () => {
    const useExternalPublicData = await this.initExternalPublicData();
    const publicDataOnly = this.isPublicDataOnly;

    let markets: Market[] = [];
    let tickers: Ticker[] = [];

    if (!useExternalPublicData) {
      // Fetch all initial data
      [markets, tickers] = await this.fetchMarketsAndTickers();
      if (this.isDisposed) return;

      // Start websocket streams early - they can establish connections while we process data
      this.publicWebsocket.connectAndSubscribe();
    } else {
      markets = this.store.markets;
      tickers = this.store.tickers;
    }

    if (!publicDataOnly) {
      this.privateWebsocket.connectAndSubscribe();
    }

    // Generate positions directly from fetched markets (no store dependency)
    const positions = this.generateFakePositionsFromMarkets(markets);

    // Update store with all initial data in a single call
    const storeUpdate: any = {
      positions,
      orders: [],
      balance: {
        used: 0,
        free: 0,
        total: 0,
        upnl: 0,
      },
      loaded: {
        ...this.store.loaded,
        orders: true,
        balance: true,
        positions: true,
      },
    };

    if (!useExternalPublicData) {
      storeUpdate.markets = markets;
      storeUpdate.tickers = tickers;
      storeUpdate.loaded.markets = true;
      storeUpdate.loaded.tickers = true;
    }

    this.store.update(storeUpdate);

    if (publicDataOnly) {
      this.log(`Ready to watch public data on Hyperliquid Spot`);
      return;
    }

    // Prime spot balances via REST to avoid blank spot margin module while
    // waiting for websocket delivery of spotState.
    await this.refreshSpotBalance(true);

    this.log(`Ready to watch public data on Hyperliquid Spot`);

    await this.tick();
    if (this.isDisposed) return;
  };

  tick = async () => {
    if (!this.isDisposed) {
      if (this.isPublicDataOnly) return;

      // Spot balance stream can be flaky on some user setups, so keep a
      // lightweight REST fallback fresh in the tick loop.
      await this.refreshSpotBalance();
      loop(() => this.tick(), this.getTickInterval());
    }
  };

  dispose = () => {
    super.dispose();
    this.publicWebsocket.dispose();
    this.privateWebsocket.dispose();
  };

  private fetchMarketsAndTickers = async (): Promise<[Market[], Ticker[]]> => {
    const { data } = await this.xhr.post<[Meta, AssetCtx[]]>(
      ENDPOINTS.INFO,
      { type: 'spotMetaAndAssetCtxs' },
      {
        headers: {
          'Content-Type': 'application/json',
        },
      },
    );

    const [meta, assets] = data;

    const markets: Market[] = [];
    const tickers: Ticker[] = [];
    const positionalContexts = assets.length === meta.universe.length && assets.every((asset) => !asset.coin);

    for (let i = 0; i < meta.universe.length; i++) {
      const sourceMarket = meta.universe[i];
      // The documented contexts array is positional and may omit `coin`.
      const sourceTicker = assets.find((asset: any) => asset.coin === sourceMarket.name) ??
        (positionalContexts ? assets[i] : undefined);
      const token = meta.tokens.find((token) => token.index === meta.universe[i].tokens[0]);
      const quoteToken = meta.tokens.find((token) => token.index === sourceMarket.tokens[1]);

      if (!token || !quoteToken) {
        continue;
      }

      const baseAsset = v(sourceMarket, 'name');
      const quoteAsset = quoteToken.name;

      const amountPrecisionStr = safeString(token, 'szDecimals');
      const amountPrecision = parseInt(amountPrecisionStr as string);

      //console.log(token.name, sourceMarket, sourceTicker, token);

      if (!sourceTicker) {
        continue;
      }

      const price = parseFloat(v(sourceTicker, 'midPx'));
      // deprecated ticker skip
      if (!sourceTicker['midPx']) {
        continue;
      }
      const pricePrecision = calculatePricePrecision(price, amountPrecision, 8);
      const pricePrecisionStr = numberToString(pricePrecision);

      const id = baseAsset;
      let symbol = `${token.name}/${quoteAsset}`;

      // if (token.name === 'NOCEX') {
      //   symbol = 'HYPE/USDC';
      // }

      markets.push({
        id,
        symbol,
        //base: token.name === 'NOCEX' ? 'HYPE' : token.name,
        base: token.name,
        quote: quoteAsset,
        isSpot: true,
        active: true,
        precision: {
          amount: +(parsePrecision(amountPrecisionStr) || 0),
          price: +(parsePrecision(pricePrecisionStr) || 0),
        },
        limits: {
          amount: {
            min: +(parsePrecision(amountPrecisionStr) || 0),
            max: Number.MAX_SAFE_INTEGER,
          },
          leverage: {
            min: 1,
            max: 1,
          },
        },
        info: {
          // spot assets are shifted by 10000 for ordering
          hyperliquidId: +v(sourceMarket, 'index') + 10000,
          ...sourceMarket,
        },
      });

      const midPx = parseFloat(v(sourceTicker, 'midPx'));
      const markPx = parseFloat(v(sourceTicker, 'markPx')) || midPx;
      const prevDayPx = parseFloat(v(sourceTicker, 'prevDayPx'));
      const quoteVolume = parseFloat(v(sourceTicker, 'dayNtlVlm')) || 0;
      const dayBaseVolume = parseFloat(v(sourceTicker, 'dayBaseVlm'));
      const volume =
        Number.isFinite(dayBaseVolume) && dayBaseVolume > 0 ? dayBaseVolume : midPx > 0 ? quoteVolume / midPx : 0;

      tickers.push({
        id,
        symbol,
        bid: 0,
        ask: 0,
        last: midPx,
        mark: markPx,
        index: parseFloat(v(sourceTicker, 'oraclePx')) || 0,
        midPrice: midPx,
        percentage: prevDayPx > 0 ? ((markPx - prevDayPx) / prevDayPx) * 100 : 0,
        fundingRate: parseFloat(v(sourceTicker, 'funding')) || 0,
        fundingTime: 0,
        volume,
        quoteVolume,
        openInterest: 0,
        info: sourceTicker,
      });
    }

    return [markets, tickers];
  };

  private getSpotCoinMarkPrice = (coin: string, tokenIndex?: number): number => {
    const resolve = (name: string, index: number | undefined, visited: Set<string>, depth: number): number => {
      const canonical = index === undefined ? this.store.marketsById[name]?.base :
        this.store.markets.find((market) => market.info?.tokens?.[0] === index)?.base;
      const key = (canonical ?? name).trim().toUpperCase();
      if (['USDC', 'USDT', 'USD', 'USDH'].includes(key)) return 1;
      if (!key || visited.has(key) || depth >= 3) return 0;
      const seen = new Set(visited).add(key);
      const matches = this.store.markets.filter((candidate) => candidate.base?.toUpperCase() === key ||
        candidate.id?.toUpperCase() === key || (index !== undefined && candidate.info?.tokens?.[0] === index));
      // Prefer a direct USD source over an earlier pair quoted in another token.
      matches.sort((a, b) => Number(b.quote === 'USDC') - Number(a.quote === 'USDC'));
      for (const market of matches) {
        const ticker = this.store.tickersById[market.id] || this.store.tickersBySymbol[market.symbol];
        const price = Number(ticker?.mark) > 0 ? Number(ticker?.mark) : Number(ticker?.last);
        if (!Number.isFinite(price) || price <= 0) continue;
        const quoteUsd = resolve(market.quote, undefined, seen, depth + 1);
        const usd = price * quoteUsd;
        if (Number.isFinite(usd) && usd > 0) return usd;
      }
      return 0;
    };
    return resolve(`${coin || ''}`, tokenIndex, new Set(), 0);
  };

  private buildSpotBalance = (balances: Array<{ coin: string; hold: string; total: string; token?: number }>): Balance => {
    const currencies: { [asset: string]: { free: number; used: number; total: number; upnl: number } } = {};
    const coins: { [coin: string]: CoinBalance } = {};

    let total = 0;
    let used = 0;
    let free = 0;

    for (const balanceItem of balances) {
      const coin = `${balanceItem?.coin || ''}`;
      if (!coin) {
        continue;
      }

      const totalAmount = parseFloat(`${balanceItem?.total || 0}`);
      const usedAmount = parseFloat(`${balanceItem?.hold || 0}`);
      const freeAmount = Math.max(totalAmount - usedAmount, 0);

      currencies[coin] = {
        free: freeAmount,
        used: usedAmount,
        total: totalAmount,
        upnl: 0,
      };

      const markPrice = this.getSpotCoinMarkPrice(coin, balanceItem?.token);
      const totalNotional = totalAmount * markPrice;
      const usedNotional = usedAmount * markPrice;
      const freeNotional = freeAmount * markPrice;

      coins[coin] = {
        base: coin,
        amount: totalAmount,
        notional: totalNotional,
      };

      total += totalNotional;
      used += usedNotional;
      free += freeNotional;
    }

    return {
      total,
      used,
      free,
      upnl: 0,
      currencies,
      coins,
    };
  };

  public applySpotBalances = (balances: Array<{ coin: string; hold: string; total: string; token?: number }>) => {
    if (!Array.isArray(balances)) {
      return;
    }

    this.store.update({
      balance: this.buildSpotBalance(balances),
    });
  };

  private fetchSpotClearinghouseState = async (): Promise<SpotClearinghouseState | null> => {
    const user = this.options.vault || this.options.applicationId;
    if (!user) {
      return null;
    }

    const { data } = await this.xhr.post<SpotClearinghouseState>(
      ENDPOINTS.INFO,
      {
        type: 'spotClearinghouseState',
        user,
      },
      {
        headers: {
          'Content-Type': 'application/json',
        },
      },
    );

    return data || null;
  };

  public requestSpotBalanceRefresh = (force = false) => {
    void this.refreshSpotBalance(force);
  };

  private refreshSpotBalance = async (force = false) => {
    const now = Date.now();
    if (force) {
      if (now - this.lastForcedSpotBalanceRefreshAt < HyperliquidSpotExchange.SPOT_BALANCE_FORCE_REFRESH_COOLDOWN_MS) {
        return;
      }
      this.lastForcedSpotBalanceRefreshAt = now;
    } else if (now - this.lastSpotBalanceRefreshAt < HyperliquidSpotExchange.SPOT_BALANCE_REFRESH_MS) {
      return;
    }

    if (this.spotBalanceRefreshInFlight) {
      return this.spotBalanceRefreshInFlight;
    }

    this.lastSpotBalanceRefreshAt = now;
    this.spotBalanceRefreshInFlight = (async () => {
      try {
        const state = await this.fetchSpotClearinghouseState();
        if (this.isDisposed || !state?.balances) {
          return;
        }

        this.applySpotBalances(state.balances);
      } catch (err) {
        this.log(`Failed to refresh spot balance: ${err}`, LogSeverity.Warning);
      } finally {
        this.spotBalanceRefreshInFlight = null;
      }
    })();

    return this.spotBalanceRefreshInFlight;
  };

  fetchOHLCV = async (opts: OHLCVOptions) => {
    const interval = opts.interval;
    const limit = Math.min(opts.limit || 500, 1500);
    const [, amount, unit] = opts.interval.split(/(\d+)/);

    const end = opts.to ? dayjs(opts.to) : dayjs();
    const start =
      !opts.limit && opts.from ? dayjs(opts.from) : end.subtract(parseFloat(amount) * limit, unit as ManipulateType);

    const { data } = await this.xhr.post<
      { T: number; t: number; o: string; h: string; l: string; c: string; v: string }[]
    >(
      ENDPOINTS.INFO,
      {
        type: 'candleSnapshot',
        req: {
          // Use O(1) hashmap lookup instead of O(n) find
          coin: this.store.marketsBySymbol[opts.symbol]?.id,
          interval,
          startTime: start.valueOf(),
          endTime: end.valueOf(),
          // ...(opts.limit
          //   ? {
          //       limit,
          //     }
          //   : {}),
        },
      },
      {
        headers: {
          'Content-Type': 'application/json',
        },
      },
    );

    // Handle different response formats - sometimes the API returns wrapped data
    const candleData = Array.isArray(data) ? data : (data as any)?.data || (data as any)?.response || [];

    if (!Array.isArray(candleData)) {
      this.log(`Unexpected fetchOHLCV response format: ${JSON.stringify(data)}`, LogSeverity.Error);
      return [];
    }

    const candles: Candle[] = candleData.map(({ t, o, h, l, c, v }) => {
      return {
        timestamp: t / 1000,
        open: parseFloat(o),
        high: parseFloat(h),
        low: parseFloat(l),
        close: parseFloat(c),
        volume: parseFloat(v),
      };
    });

    return candles;
  };

  fetchOrders = async (_symbols?: string[]) => {
    return [];
  };

  // Spot symbol config is invariant (1x cross, one-way) so share a
  // frozen singleton instead of allocating per call. Callers always
  // spread the result into a new object — they don't mutate it.
  private static readonly SPOT_SYMBOL_CONFIG = Object.freeze({
    marginType: MarginType.Cross,
    positionMode: PositionMode.OneWay,
    leverage: 1,
  });

  _getSymbolConfigurationFromAssetData(_coin: string) {
    // Spot markets are always 1x and do not support margin mode toggles.
    return HyperliquidSpotExchange.SPOT_SYMBOL_CONFIG;
  }

  private generateFakePositionsFromMarkets = (markets: Market[]) => {
    const fakePositions = markets.map((market) => {
      const marketConfig = this._getSymbolConfigurationFromAssetData(market.id);
      return marketConfig.positionMode === PositionMode.OneWay
        ? [
            {
              id: createPositionId(market.symbol, PositionSide.Long, PositionMode.OneWay),
              symbol: market.symbol,
              side: PositionSide.Short,
              entryPrice: 0,
              notional: 0,
              unrealizedPnl: 0,
              contracts: 0,
              liquidationPrice: 0,
              ...marketConfig,
            },
          ]
        : [
            {
              id: createPositionId(market.symbol, PositionSide.Long, PositionMode.OneWay),
              symbol: market.symbol,
              side: PositionSide.Short,
              entryPrice: 0,
              notional: 0,
              unrealizedPnl: 0,
              contracts: 0,
              liquidationPrice: 0,
              ...marketConfig,
            },
            {
              id: createPositionId(market.symbol, PositionSide.Short, PositionMode.OneWay),
              symbol: market.symbol,
              side: PositionSide.Short,
              entryPrice: 0,
              notional: 0,
              unrealizedPnl: 0,
              contracts: 0,
              liquidationPrice: 0,
              ...marketConfig,
            },
          ];
    });
    return [...fakePositions.flat()];
  };

  getFakePositions = () => {
    return this.generateFakePositionsFromMarkets(this.store.markets);
  };

  fetchPositions = async (symbols?: string[]) => {
    if (this.store.positions.length > 0) {
      return [...this.store.positions];
    }
    const positions = this.getFakePositions();
    return positions.filter((position) => (!symbols ? position : symbols?.includes(position.symbol)));
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    return this.publicWebsocket.listenOHLCV(opts, callback);
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    return this.publicWebsocket.listenOrderBook(symbol, callback);
  };

  setLeverage = async (symbol: string, inputLeverage: number, _side?: PositionSide) => {
    // Use O(1) hashmap lookup instead of O(n) find
    const market = this.store.marketsBySymbol[symbol];
    if (!market) throw new Error(`Market ${symbol} not found`);
    const coin = market.id;
    const activeAssetData = this.activeAssetData.get(coin);
    const timestamp = virtualClock.getCurrentTimeMs();
    const action = {
      type: 'updateLeverage',
      asset: market.info.hyperliquidId,
      isCross: activeAssetData?.leverage.type === 'cross',
      leverage: +inputLeverage,
    };
    const signature = await signL1Action(createWalletFromPrivateKey(this.options.secret), action, null, timestamp);
    await this.xhr.post(
      ENDPOINTS.EXCHANGE,
      {
        action,
        nonce: timestamp,
        signature,
      },
      {
        headers: {
          'Content-Type': 'application/json',
        },
      },
    );
  };

  changeMarginType = async (isolated: boolean, symbol?: string) => {
    // Use O(1) hashmap lookup instead of O(n) find
    const market = this.store.marketsBySymbol[symbol!];
    if (!market) throw new Error(`Market ${symbol} not found`);
    const coin = market.id;
    const activeAssetData = this.activeAssetData.get(coin);
    const timestamp = virtualClock.getCurrentTimeMs();
    const action = {
      type: 'updateLeverage',
      asset: market.info.hyperliquidId,
      isCross: !isolated,
      leverage: activeAssetData?.leverage.value,
    };
    const signature = await signL1Action(createWalletFromPrivateKey(this.options.secret), action, null, timestamp);
    await this.xhr.post(
      ENDPOINTS.EXCHANGE,
      {
        action,
        nonce: timestamp,
        signature,
      },
      {
        headers: {
          'Content-Type': 'application/json',
        },
      },
    );
  };

  // spot order example
  // {
  //   "action": {
  //     "grouping": "na",
  //     "orders": [
  //       {
  //         "a": 10010,
  //         "b": true,
  //         "p": "0.00093722",
  //         "r": false,
  //         "s": "6208",
  //         "t": {
  //           "limit": {
  //             "tif": "FrontendMarket"
  //           }
  //         }
  //       }
  //     ],
  //     "type": "order"
  //   },
  //   "isFrontend": true,
  //   "nonce": 1736007324295,
  //   "signature": {
  //     "r": "0xb1bc77c6c681e7b4074f8da42b387971887880ce17c9dd03ba738f6b554c2995",
  //     "s": "0x03a89395ecffd3ec19e9c552bd312a6d1993e51ac6a7ec05455bb84a37ab7cbe",
  //     "v": 28
  //   },
  //   "vaultAddress": null
  // }

  placeOrder = async (opts: PlaceOrderOpts) => {
    // The caller's ID goes on the main wire verbatim; otherwise we mint one. Each TP/SL leg gets its own.
    const cloid = this.resolveCallerClientOrderId(opts) ?? mintHyperliquidCloid();
    if (opts.type === OrderType.TrailingStopLoss) {
      throw new Error('Not supported');
    }

    // Use O(1) hashmap lookup instead of O(n) find
    const market = this.store.marketsBySymbol[opts.symbol];

    if (!market) {
      throw new Error(`Market ${opts.symbol} not found`);
    }

    if (opts.type === OrderType.Market) {
      //   {
      //     "action": {
      //         "grouping": "na",
      //         "orders": [
      //             {
      //                 "a": 98,
      //                 "b": false,
      //                 "p": "3.0384",
      //                 "r": false,
      //                 "s": "4",
      //                 "t": {
      //                     "limit": {
      //                         "tif": "FrontendMarket"
      //                     }
      //                 }
      //             }
      //         ],
      //         "type": "order"
      //     },
      //     "isFrontend": true,
      //     "nonce": 1732059038287,
      //     "signature": {
      //         "r": "0x20fc85f12a44a2f8354553677023215acd77006f72f8f6b84b8d07bd305ced13",
      //         "s": "0x5a4bd1858897836e0cd9b7f139ae376a80d59b0c05b4d051dfb37c703a7acd83",
      //         "v": 28
      //     },
      //     "vaultAddress": null
      // }
      const assetIndex = market?.info.hyperliquidId;

      // Use O(1) hashmap lookup instead of O(n) find
      const ticker = this.store.tickersBySymbol[market.symbol];

      if (!ticker) {
        return;
      }

      const orderWire = orderRequestToOrderWire(
        {
          coin: market.symbol,
          is_buy: opts.side === OrderSide.Buy,
          sz: opts.amount,
          //limit_px: adjust((ticker?.last || 0) * (opts.side === OrderSide.Buy ? 1.05 : 0.95), pPrice),
          limit_px: +this.priceToPrecision(
            ticker.symbol,
            (ticker?.last || 0) * (opts.side === OrderSide.Buy ? 1.05 : 0.95),
          ),
          order_type: {
            limit: {
              tif: 'Gtc' as any,
            },
          },
          reduce_only: false,
          cloid,
        },
        assetIndex,
      );
      const action = orderWiresToOrderAction([orderWire]);

      const nonce = +Date.now();
      const signature = await signL1Action(createWalletFromPrivateKey(this.options.secret), action, null, nonce);

      const payload = { action, nonce, signature };
      const { data } = await this.xhr.post(ENDPOINTS.EXCHANGE, payload, {
        headers: {
          'Content-Type': 'application/json',
        },
      });
      this.requestSpotBalanceRefresh(true);
      return data.response.data.statuses.flatMap((status: any) => {
        const oid = status?.resting?.oid ?? status?.filled?.oid;
        return oid === undefined || oid === null ? [] : [String(oid)];
      });
    } else if (opts.type === OrderType.Limit) {
      // Limit with TP & SL
      //   {
      //     "action": {
      //         "grouping": "normalTpsl",
      //         "orders": [
      //             {
      //                 "a": 98,
      //                 "b": true,
      //                 "p": "3.2",
      //                 "r": false,
      //                 "s": "21",
      //                 "t": {
      //                     "limit": {
      //                         "tif": "Gtc"
      //                     }
      //                 }
      //             },
      //             {
      //                 "a": 98,
      //                 "b": false,
      //                 "p": "1.84",
      //                 "r": true,
      //                 "s": "21",
      //                 "t": {
      //                     "trigger": {
      //                         "isMarket": true,
      //                         "tpsl": "sl",
      //                         "triggerPx": "2"
      //                     }
      //                 }
      //             },
      //             {
      //                 "a": 98,
      //                 "b": false,
      //                 "p": "3.68",
      //                 "r": true,
      //                 "s": "21",
      //                 "t": {
      //                     "trigger": {
      //                         "isMarket": true,
      //                         "tpsl": "tp",
      //                         "triggerPx": "4"
      //                     }
      //                 }
      //             }
      //         ],
      //         "type": "order"
      //     },
      //     "isFrontend": true,
      //     "nonce": 1732047758486,
      //     "signature": {
      //         "r": "0xb35f9b10a234ecd7d9469c0a71548cfc97b3021656103a4124ffe37059d970f5",
      //         "s": "0x7c03e973a1d21c5ff375e1849bcfe36d57e01828e3b91917d938630b99e1b95f",
      //         "v": 28
      //     },
      //     "vaultAddress": null
      // }
      // Normal limit
      //   {
      //     "action": {
      //         "grouping": "na",
      //         "orders": [
      //             {
      //                 "a": 98,
      //                 "b": true,
      //                 "p": "3.2",
      //                 "r": false,
      //                 "s": "21",
      //                 "t": {
      //                     "limit": {
      //                         "tif": "Gtc"
      //                     }
      //                 }
      //             }
      //         ],
      //         "type": "order"
      //     },
      //     "isFrontend": true,
      //     "nonce": 1732047799936,
      //     "signature": {
      //         "r": "0x9ec9f3a93a0cef9f1823d72a7a891c657bf14639cc374e85fe5d24d3fde275dc",
      //         "s": "0x7c6bb426e249978c153651394e7f6ac6f97ced0bf440ff50b8dba9f1ef0bc150",
      //         "v": 27
      //     },
      //     "vaultAddress": null
      // }
      // const req = omitUndefined({
      //   category: this.getCategoryForSymbol(opts.symbol),
      //   symbol: opts.symbol,
      //   side: inverseObj(ORDER_SIDE)[opts.side],
      //   orderType: inverseObj(ORDER_TYPE)[type],
      //   qty: `${amount}`,
      //   price: opts.type === OrderType.Limit ? `${price}` : undefined,
      //   stopLoss: opts.stopLoss ? `${stopLoss}` : undefined,
      //   takeProfit: opts.takeProfit ? `${takeProfit}` : undefined,
      //   reduceOnly: opts.reduceOnly || false,
      //   triggerPrice: opts.triggerPrice ? `${triggerPrice}` : undefined,
      //   triggerDirection: triggerDirection ? `${triggerDirection}` : undefined,
      //   triggerBy:
      //     opts.type === OrderType.StopLoss
      //       ? opts.slTriggerType === OrderStopTriggerType.LastPrice
      //         ? 'LastPrice'
      //         : 'MarkPrice'
      //       : opts.type === OrderType.TakeProfit
      //         ? opts.tpTriggerType === OrderStopTriggerType.MarkPrice
      //           ? 'MarkPrice'
      //           : 'LastPrice'
      //         : undefined,
      //   slTriggerBy: opts.stopLoss
      //     ? opts.slTriggerType === OrderStopTriggerType.LastPrice
      //       ? 'LastPrice'
      //       : 'MarkPrice'
      //     : undefined,
      //   tpTriggerBy: opts.takeProfit
      //     ? opts.slTriggerType === OrderStopTriggerType.MarkPrice
      //       ? 'MarkPrice'
      //       : 'LastPrice'
      //     : undefined,
      //   timeInForce: opts.type === OrderType.Limit ? timeInForce : undefined,
      //   closeOnTrigger: false,
      //   positionIdx,
      // });

      const assetIndex = market?.info.hyperliquidId;

      const orderWire = orderRequestToOrderWire(
        {
          coin: market.symbol,
          is_buy: opts.side === OrderSide.Buy,
          sz: opts.amount,
          //limit_px: opts.price || 0,
          limit_px: +this.priceToPrecision(market.symbol, opts.price || 0),
          order_type: {
            limit: {
              tif: opts.timeInForce === OrderTimeInForce.PostOnly ? 'Alo' : 'Gtc',
            },
          },
          reduce_only: false,
          cloid,
        },
        assetIndex,
      );

      const orderWires = [orderWire];

      //                 "a": 98,
      //                 "b": false,
      //                 "p": "1.84",
      //                 "r": true,
      //                 "s": "21",
      //                 "t": {
      //                     "trigger": {
      //                         "isMarket": true,
      //                         "tpsl": "sl",
      //                         "triggerPx": "2"
      //                     }
      //                 }

      if (opts.stopLoss) {
        orderWires.push(
          orderRequestToOrderWire(
            {
              coin: market.symbol,
              is_buy: opts.side !== OrderSide.Buy,
              sz: opts.amount,
              //limit_px: opts.price || 0,
              limit_px: +this.priceToPrecision(market.symbol, opts.price || 0),
              order_type: {
                trigger: {
                  isMarket: true,
                  tpsl: 'sl',
                  triggerPx: opts.stopLoss,
                },
              },
              reduce_only: false,
              cloid: mintHyperliquidCloid(),
            },
            assetIndex,
          ),
        );
      }

      if (opts.takeProfit) {
        orderWires.push(
          orderRequestToOrderWire(
            {
              coin: market.symbol,
              is_buy: opts.side !== OrderSide.Buy,
              sz: opts.amount,
              //limit_px: opts.price || 0,
              limit_px: +this.priceToPrecision(market.symbol, opts.price || 0),
              order_type: {
                trigger: {
                  isMarket: true,
                  tpsl: 'tp',
                  triggerPx: opts.takeProfit,
                },
              },
              reduce_only: false,
              cloid: mintHyperliquidCloid(),
            },
            assetIndex,
          ),
        );
      }

      const action =
        opts.takeProfit || opts.stopLoss
          ? orderWiresToNormalTPSLAction(orderWires)
          : orderWiresToOrderAction(orderWires);

      const nonce = +Date.now();
      const signature = await signL1Action(createWalletFromPrivateKey(this.options.secret), action, null, nonce);

      const payload = { action, nonce, signature };
      const { data } = await this.xhr.post(ENDPOINTS.EXCHANGE, payload, {
        headers: {
          'Content-Type': 'application/json',
        },
      });
      this.requestSpotBalanceRefresh(true);
      return data.response.data.statuses.flatMap((status: any) => {
        const oid = status?.resting?.oid ?? status?.filled?.oid;
        return oid === undefined || oid === null ? [] : [String(oid)];
      });
    } else if (opts.type === OrderType.TakeProfit || opts.type === OrderType.StopLoss) {
      // Position TP/SL
      //   {
      //     "action": {
      //         "grouping": "positionTpsl",
      //         "orders": [
      //             {
      //                 "a": 5,
      //                 "b": false,
      //                 "p": "165.6",
      //                 "r": true,
      //                 "s": "0",
      //                 "t": {
      //                     "trigger": {
      //                         "isMarket": true,
      //                         "tpsl": "sl",
      //                         "triggerPx": "180"
      //                     }
      //                 }
      //             },
      //             {
      //                 "a": 5,
      //                 "b": false,
      //                 "p": "239.2",
      //                 "r": true,
      //                 "s": "0",
      //                 "t": {
      //                     "trigger": {
      //                         "isMarket": true,
      //                         "tpsl": "tp",
      //                         "triggerPx": "260"
      //                     }
      //                 }
      //             }
      //         ],
      //         "type": "order"
      //     },
      //     "isFrontend": true,
      //     "nonce": 1732047841729,
      //     "signature": {
      //         "r": "0x53976d9941a662557383848cf5424ad01f786cd42d144087cb677467a83f6df2",
      //         "s": "0x434a5f106f66fff488bd621ab2f77815cae25f69f1bda077b983ad723d5db96b",
      //         "v": 27
      //     },
      //     "vaultAddress": null
      // }

      // Single Position TP
      //   {
      //     "action": {
      //         "grouping": "positionTpsl",
      //         "orders": [
      //             {
      //                 "a": 0,
      //                 "b": false,
      //                 "p": "92000",
      //                 "r": true,
      //                 "s": "0",
      //                 "t": {
      //                     "trigger": {
      //                         "isMarket": true,
      //                         "tpsl": "tp",
      //                         "triggerPx": "100000"
      //                     }
      //                 }
      //             }
      //         ],
      //         "type": "order"
      //     },
      //     "isFrontend": true,
      //     "nonce": 1732048008175,
      //     "signature": {
      //         "r": "0x54ac036eb50cb71bd1910dd907ecf6d348b52c51d641cb04ae2dbf16386e9bec",
      //         "s": "0x3406757f696e8151512280aa80c205ea550fdf19259c7dbf3944c1f95bf7d055",
      //         "v": 28
      //     },
      //     "vaultAddress": null
      // }

      // Partial Position TP/SL
      //   {
      //     "grouping": "positionTpsl",
      //     "orders": [
      //         {
      //             "a": 98,
      //             "b": true,
      //             "p": "2.16",
      //             "r": true,
      //             "s": "1",
      //             "t": {
      //                 "trigger": {
      //                     "isMarket": true,
      //                     "tpsl": "tp",
      //                     "triggerPx": "2"
      //                 }
      //             }
      //         }
      //     ],
      //     "type": "order"
      // }
      throw new Error('Not supported');
    } else {
      throw new Error(`Unknown order type ${opts.type}`);
    }
  };

  // Limit Cancel
  //   {
  //     "action": {
  //         "cancels": [
  //             {
  //                 "a": 98,
  //                 "o": 48582502044
  //             }
  //         ],
  //         "type": "cancel"
  //     },
  //     "isFrontend": true,
  //     "nonce": 1732048930058,
  //     "signature": {
  //         "r": "0xcf986988d160b2f2377e065fcc491956a9fad5e76cb9c28ff8ca66c57489b7a0",
  //         "s": "0x6ce55cd0deb7e7de466afb97728b17434966f459779ab653ab1a86e16e33ad4b",
  //         "v": 28
  //     },
  //     "vaultAddress": null
  // }

  // TP/SL Cancel
  //   {
  //     "action": {
  //         "cancels": [
  //             {
  //                 "a": 98,
  //                 "o": 48371398850
  //             }
  //         ],
  //         "type": "cancel"
  //     },
  //     "isFrontend": true,
  //     "nonce": 1732048959055,
  //     "signature": {
  //         "r": "0x662415bab5157fe08d4a2ec03e6ad22d42df4fac4fecc2e2ef650a6e87aefa87",
  //         "s": "0x2098eebc76e5cde9e05521dc28c0951405029b91de304528d67cf18f7f6c9c6f",
  //         "v": 28
  //     },
  //     "vaultAddress": null
  // }

  cancelOrders = async (orders: Order[]) => {
    // Use O(1) hashmap lookup instead of O(n) find in loop
    const marketsBySymbol = this.store.marketsBySymbol;
    const cancelsWithIndices = orders.map((order) => {
      const market = marketsBySymbol[order.symbol];
      return { a: +market?.info.hyperliquidId, o: +order.id };
    });

    const action = {
      type: 'cancel',
      cancels: cancelsWithIndices.map(({ a, o }) => ({ a, o })),
    };

    const nonce = +Date.now();
    const signature = await signL1Action(createWalletFromPrivateKey(this.options.secret), action, null, nonce);

    const payload = { action, nonce, signature };
    await this.xhr.post(ENDPOINTS.EXCHANGE, payload, {
      headers: {
        'Content-Type': 'application/json',
      },
    });
    this.requestSpotBalanceRefresh(true);
  };

  public _deadOrders = new Set<string>();

  updateOrder = async ({ order, update }: UpdateOrderOpts) => {
    const newOrder = {
      symbol: order.symbol,
      type: order.type,
      side: order.side,
      price: order.price,
      amount: order.amount,
      reduceOnly: order.reduceOnly || false,
    };

    if ('price' in update) newOrder.price = update.price;
    if ('amount' in update) newOrder.amount = update.amount;

    await this.cancelOrders([order]);

    this._deadOrders.add(order.id);

    return await this.placeOrder(newOrder);
  };

  priceToPrecision = (symbol: string, price: any): string => {
    const priceStr = numberToString(price);
    if (!priceStr) {
      return '';
    }
    const integerPart = priceStr.split('.')[0];
    const significantDigits = Math.max(5, integerPart.length);
    const result = decimalToPrecision(price, ROUND, significantDigits, SIGNIFICANT_DIGITS, NO_PADDING);
    const maxDecimals = 8;
    // Use O(1) hashmap lookup instead of O(n) find
    const prAmount = this.store.marketsBySymbol[symbol]?.precision.amount;
    const subtractedValue = maxDecimals - precisionFromString(prAmount ? String(prAmount) : '1');
    return decimalToPrecision(result, ROUND, subtractedValue, DECIMAL_PLACES, NO_PADDING);
  };
}
