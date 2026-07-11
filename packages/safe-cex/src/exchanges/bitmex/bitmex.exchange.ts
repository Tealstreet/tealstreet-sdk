import type { Axios } from 'axios';
import type { ManipulateType } from 'dayjs';
import type { Store } from '../../store/store.interface';

import rateLimit from 'axios-rate-limit';
import dayjs from 'dayjs';
import chunk from 'lodash/chunk';
import groupBy from 'lodash/groupBy';
import omit from 'lodash/omit';
import { forEachSeries } from 'p-iteration';

import {
  Balance,
  Candle,
  ExchangeName,
  ExchangeOptions,
  Execution,
  ExecutionsOptions,
  HistoricalTrade,
  HistoricalTradeOptions,
  LogSeverity,
  MarginType,
  Market,
  OHLCVOptions,
  Order,
  OrderBook,
  OrderRejectReason,
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
import { createPositionId } from '../../utils/createPositionId';
import { v } from '../../utils/get-key';
import { inverseObj } from '../../utils/inverseObj';
import { loop } from '../../utils/loop';
import { omitUndefined } from '../../utils/omitUndefined';
import { parsePrecision } from '../../utils/parsePrecision';
import { add, adjust, handleRoundingError, multiply, round, subtract } from '../../utils/safe-math';
import { getPublicCorsProxy } from '../../utils/transportResolution';
import { virtualClock } from '../../utils/virtualClock';
import { BaseExchange } from '../base';
import { createAPI } from './bitmex.api';
import { aggregateBitmexCandles, getBitmexOHLCVPlan } from './bitmex.ohlcv-helpers';
import type { BitmexStrategy } from './bitmex.types';
import {
  Currency,
  ENDPOINTS,
  GRAFANA_ENDPOINTS_EVERY,
  GRAFANA_ENDPOINTS_PERIODIC,
  ORDER_SIDE,
  ORDER_STATUS,
  TIME_IN_FORCE,
} from './bitmex.types';
import { BitmexPrivateWebsocket } from './bitmex.ws-private';
import { BitmexPublicWebsocket } from './bitmex.ws-public';
import { BITMEX_EXECUTION, BITMEX_MARKET, BITMEX_ORDER, BITMEX_POSITION, SupportedTimeframes } from './types';

export const SATOSHI_PER_BTC = 100000000;
type BitmexPlaceOrderOpts = PlaceOrderOpts & {
  maxSlippagePct?: number;
};

export class BitmexExchange extends BaseExchange {
  xhr: Axios;
  unlimitedXHR: Axios;

  publicWebsocket: BitmexPublicWebsocket;
  privateWebsocket: BitmexPrivateWebsocket;

  convertInputsToMicro = true;
  convertOutputsFromMicro = true;

  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;
  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;

  private currencies: { [code: string]: Currency } = {};
  private lastExecutionHistorySignatureWarningAt = 0;
  private maxSlippagePct?: number;
  private accountPositionMode?: PositionMode;

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    this.xhr = this.createXhrWithSecondaryKey(opts, createAPI, { rateLimit: (xhr) => rateLimit(xhr, { maxRPS: 3 }) });
    this.unlimitedXHR = createAPI(opts, this);

    this.publicWebsocket = new BitmexPublicWebsocket(this);
    this.privateWebsocket = new BitmexPrivateWebsocket(this);
  }

  dispose = () => {
    super.dispose();
    this.publicWebsocket.dispose();
    this.privateWebsocket.dispose();
  };

  getAccount = async () => {
    const {
      data: [{ accountAlias }],
    } = await this.xhr.get(ENDPOINTS.BALANCE);

    return { userId: accountAlias };
  };

  validateAccount = async () => {
    try {
      const { data } = await this.xhr.get(ENDPOINTS.ACCOUNT);
      if ((data as any)?.error) {
        const msg = this.normalizeBitmexValidationError(this.parseError(data));
        this.emitAndLogError(msg);
        return { error: msg };
      }
    } catch (err: any) {
      const msg = this.normalizeBitmexValidationError(this.parseError(err));
      this.emitAndLogError(msg);
      return { error: msg };
    }
    // Key validated; balance is best-effort for the new-account gate.
    return { error: '', balance: await this.fetchBalance().catch(() => undefined) };
  };

  start = async () => {
    const useExternalPublicData = await this.initExternalPublicData();

    // Always fetch currencies — needed for balance precision conversion (satoshi→BTC)
    // and doesn't require authentication
    this.currencies = await this.fetchCurrencies();
    if (this.isDisposed) return;

    if (!useExternalPublicData) {
      // load initial market data
      // then we can poll for live data
      const { markets, tickers } = await this.fetchMarketsAndTickers();
      if (this.isDisposed) return;

      this.store.update({
        markets,
        loaded: { ...this.store.loaded, markets: true },
      });

      this.log(`Loaded ${markets.length} BitMex markets`);

      this.store.update({
        tickers,
        loaded: { ...this.store.loaded, tickers: true },
      });

      this.store.update({
        orders: [],
        loaded: { ...this.store.loaded, orders: true },
      });

      // start websocket streams
      this.publicWebsocket.connectAndSubscribe();
    }

    if (!this.isPublicDataOnly) {
      await this._startPrivate();
    } else {
      await this._startPublicDataOnly();
    }
  };

  _startPrivate = async () => {
    this.privateWebsocket.connectAndSubscribe();
    void this.seedBitmexMaxSlippagePctFromRecentOrder();

    // Resolve the account-level mode before announcing readiness. Order
    // submission also gates on this value in case the response has no rows.
    await this.fetchOpenPositions();
    if (this.isDisposed) return;

    // Start the REST tick loop in the background. Private WS snapshots can
    // hydrate balance/positions first, so a slow HOW request should not make
    // startup look frozen.
    void this.tick();
    if (this.isDisposed) return;

    this.log(`Ready to trade on BitMEX`);

    // // fetch unfilled orders
    // const orders = await this.fetchOrders();
    // if (this.isDisposed) return;

    // this.log(`Loaded Binance orders`);

    // this.store.update({
    //   orders,
    //   loaded: { ...this.store.loaded, orders: true },
    // });
  };

  trackBitmexMaxSlippagePct = (rawValue: unknown) => {
    const parsedValue = parseFloat(String(rawValue ?? ''));
    if (!Number.isFinite(parsedValue) || parsedValue <= 0) {
      return;
    }
    this.maxSlippagePct = parsedValue;
  };

  private getConfiguredBitmexMaxSlippagePct = (): number | undefined => {
    const configuredValue = parseFloat(String((this.options.extra as any)?.bitmexMaxSlippagePct ?? ''));
    if (Number.isFinite(configuredValue) && configuredValue > 0) {
      return configuredValue;
    }
    return undefined;
  };

  private resolveMaxSlippagePctForOrder = (opts: BitmexPlaceOrderOpts): number | undefined => {
    const orderValue = parseFloat(String(opts.maxSlippagePct ?? ''));
    if (Number.isFinite(orderValue) && orderValue > 0) {
      return orderValue;
    }
    return this.maxSlippagePct ?? this.getConfiguredBitmexMaxSlippagePct();
  };

  private seedBitmexMaxSlippagePctFromRecentOrder = async () => {
    if (this.isDisposed || this.isPublicDataOnly) {
      return;
    }
    try {
      const { data } = await this.xhr.get<Array<BITMEX_ORDER>>(ENDPOINTS.ORDER, {
        params: {
          reverse: true,
          count: 1,
        },
      });
      if ((data as any).error) {
        throw new Error(this.parseError(data));
      }
      if (Array.isArray(data) && data[0]) {
        this.trackBitmexMaxSlippagePct(v(data[0], 'maxSlippagePct'));
      }
    } catch (err: any) {
      const msg = this.parseError(err);
      const isAuthSignatureIssue = typeof msg === 'string' && msg.toLowerCase().includes('signature not valid');
      if (!isAuthSignatureIssue) {
        this.log(`BitMEX max slippage seed skipped: ${msg}`, LogSeverity.Warning);
      }
    }
  };

  _startPublicDataOnly = async () => {
    // start ticking live data
    // balance, tickers, positions
    if (this.isDisposed) return;

    this.log(`Ready to watch public data on BitMEX`);
    this.store.update({
      orders: [],
      balance: {
        used: 0,
        free: 0,
        total: 0,
        upnl: 0,
      },
      positions: [],
      loaded: { ...this.store.loaded, orders: true, balance: true, positions: true },
    });
  };

  tick = async () => {
    if (!this.isDisposed) {
      try {
        const balance = await this.fetchBalance();
        if (this.isDisposed) return;

        const positions = await this.fetchPositions();
        if (this.isDisposed) return;
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
        const msg = this.parseError(err);
        this.emitAndLogError(err, msg);
      }

      if (!this.isDisposed) {
        loop(() => this.tick(), this.getTickInterval());
      }
    }
  };

  fetchCurrencies = async () => {
    try {
      const { data } = await this.xhr.get<Array<Record<string, any>>>(ENDPOINTS.WALLET_ASSETS);
      if ((data as any).error) throw new Error(this.parseError(data));
      const result: typeof this.currencies = {};
      for (const currency of data) {
        result[currency.currency] = {
          precision: parsePrecision(currency.scale),
          majorCurrency: currency.majorCurrency,
        };
      }
      return result;
    } catch (err: any) {
      this.emitAndLogError(this.parseError(err));
      return {};
    }
  };

  fetchBalance = async () => {
    // [
    //   {
    //     "account": 2169568,
    //     "currency": "USDt",
    //     "riskLimit": 500000000000000,
    //     "amount": 26824733,
    //     "prevRealisedPnl": 0,
    //     "grossComm": 245922,
    //     "grossOpenCost": 0,
    //     "grossOpenPremium": 0,
    //     "grossExecCost": 0,
    //     "grossMarkValue": 130598900,
    //     "riskValue": 130598900,
    //     "initMargin": 0,
    //     "maintMargin": 2057519,
    //     "targetExcessMargin": 0,
    //     "realisedPnl": -245922,
    //     "unrealisedPnl": -3206350,
    //     "walletBalance": 26578811,
    //     "marginBalance": 23372461,
    //     "marginLeverage": 5.587725657131271,
    //     "marginUsedPcnt": 0.088,
    //     "excessMargin": 21314942,
    //     "availableMargin": 21314942,
    //     "withdrawableMargin": 21314942,
    //     "timestamp": "2024-02-29T20:27:15.718Z"
    //   },
    //   {
    //     "account": 2169568,
    //     "currency": "XBt",
    //     "riskLimit": 1000000000000,
    //     "amount": 151612,
    //     "prevRealisedPnl": 0,
    //     "grossComm": 1359,
    //     "grossOpenCost": 0,
    //     "grossOpenPremium": 0,
    //     "grossExecCost": 0,
    //     "grossMarkValue": 963360,
    //     "riskValue": 963360,
    //     "initMargin": 0,
    //     "maintMargin": 10467,
    //     "targetExcessMargin": 0,
    //     "realisedPnl": -1359,
    //     "unrealisedPnl": -17748,
    //     "walletBalance": 150253,
    //     "marginBalance": 132505,
    //     "marginLeverage": 7.270367155956379,
    //     "marginUsedPcnt": 0.079,
    //     "excessMargin": 122038,
    //     "availableMargin": 122038,
    //     "withdrawableMargin": 122038,
    //     "timestamp": "2024-02-29T20:27:15.718Z"
    //   }
    // ]
    try {
      const { data } = await this.xhr.get<Array<Record<string, any>>>(ENDPOINTS.ACCOUNT, {
        params: {
          currency: 'all',
        },
      });

      if ((data as any).error) throw new Error(this.parseError(data));

      return this.parseBalanceSnapshot(data);
    } catch (err: any) {
      const msg = this.parseError(err);
      this.emitAndLogError(err, msg);
      if (this.isBitmexAuthError(msg)) {
        throw new Error(msg);
      }
      return this.store.balance;
    }
  };

  parseBalanceSnapshot = (snapshot: Array<Record<string, any>>): Balance => {
    let total = 0;
    let free = 0;
    let upnl = 0;
    let used = 0;

    const assets: { [key: string]: Balance } = {};

    for (const balance of snapshot) {
      const precisionCurrency = this.resolveBitmexCurrencyPrecisionKey(balance.currency);
      const currency = this.currencies[precisionCurrency];
      if (!currency) continue;
      const normalizedCurrency = this.normalizeBitmexCurrencyCode(balance.currency);
      const majorCurrency = this.formatCoin(currency.majorCurrency || precisionCurrency);
      const walletBalanceRaw = parseFloat(balance.walletBalance ?? 0);
      const availableMarginRaw = parseFloat(balance.availableMargin ?? 0);
      const marginBalanceRaw = parseFloat(balance.marginBalance ?? balance.walletBalance ?? 0);
      const initMarginRaw = parseFloat(balance.initMargin ?? 0);
      const usedMarginRaw = Math.max(Math.max(marginBalanceRaw - availableMarginRaw, 0), initMarginRaw || 0);
      const unrealisedPnlRaw = parseFloat(balance.unrealisedPnl ?? 0);

      const walletBalanceReal = this.convertToRealAmount(precisionCurrency, walletBalanceRaw);
      const availableMarginReal = this.convertToRealAmount(precisionCurrency, availableMarginRaw);
      const usedMarginReal = this.convertToRealAmount(precisionCurrency, usedMarginRaw);
      const unrealisedPnlReal = this.convertToRealAmount(precisionCurrency, unrealisedPnlRaw);

      if (normalizedCurrency === 'USDT' || normalizedCurrency === 'USD') {
        total += walletBalanceReal;
        free += availableMarginReal;
        upnl += unrealisedPnlReal;
        used += usedMarginReal;
      } else if (normalizedCurrency === 'XBT') {
        total += this.convertToUSD(walletBalanceReal);
        free += this.convertToUSD(availableMarginReal);
        upnl += this.convertToUSD(unrealisedPnlReal);
        used += this.convertToUSD(usedMarginReal);
      }

      assets[majorCurrency] = {
        total: walletBalanceReal,
        free: availableMarginReal,
        upnl: unrealisedPnlReal,
        used: usedMarginReal,
      };
    }

    const parsedAggregateBalance: Balance = { total, free, used, upnl, info: snapshot, currencies: assets };
    const mamBalance = snapshot.find((b) => this.normalizeBitmexCurrencyCode(b.currency) === 'MAMUSD');
    if (mamBalance) {
      const mamPrecisionCurrency = this.resolveBitmexCurrencyPrecisionKey(mamBalance.currency);
      const walletBalanceRaw = parseFloat(mamBalance.walletBalance ?? 0);
      const availableMarginRaw = parseFloat(mamBalance.availableMargin ?? 0);
      const marginBalanceRaw = parseFloat(mamBalance.marginBalance ?? mamBalance.walletBalance ?? 0);
      const initMarginRaw = parseFloat(mamBalance.initMargin ?? 0);
      const usedMarginRaw = Math.max(Math.max(marginBalanceRaw - availableMarginRaw, 0), initMarginRaw || 0);
      const unrealisedPnlRaw = parseFloat(mamBalance.unrealisedPnl ?? 0);
      const hasActiveMamCollateral = [
        walletBalanceRaw,
        availableMarginRaw,
        marginBalanceRaw,
        initMarginRaw,
        unrealisedPnlRaw,
      ].some((value) => Number.isFinite(value) && value !== 0);

      if (hasActiveMamCollateral) {
        return {
          total: this.convertToRealAmount(mamPrecisionCurrency, walletBalanceRaw),
          free: this.convertToRealAmount(mamPrecisionCurrency, availableMarginRaw),
          upnl: this.convertToRealAmount(mamPrecisionCurrency, unrealisedPnlRaw),
          used: this.convertToRealAmount(mamPrecisionCurrency, usedMarginRaw),
          info: snapshot,
          currencies: assets,
        };
      }
    }

    return parsedAggregateBalance;
  };

  fetchMarketsAndTickers = async () => {
    try {
      const { data } = await this.xhr.get<Array<BITMEX_MARKET>>(ENDPOINTS.MARKETS);

      if ((data as any).error) throw new Error(this.parseError(data));

      const btcPrice = data.find((m) => m.symbol === '.BXBT')?.markPrice || 0;

      const markets: Market[] = data
        .filter(
          (m) =>
            (m.isInverse === false || m.symbol === 'XBTUSD' || m.symbol === 'XBTUSDT' || m.symbol === 'XBTEUR') &&
            m.underlying + m.quoteCurrency === m.symbol,
        )
        .map((m: BITMEX_MARKET) => {
          const baseAsset = v(m, 'underlying');
          const quoteAsset = v(m, 'quoteCurrency');
          const marginAsset = v(m, 'settlCurrency');

          const amountPrecisionRaw =
            m.isQuanto || m.isInverse
              ? 1
              : 1 /
                (parseFloat(v(m, 'underlyingToPositionMultiplier')) ||
                  parseFloat(v(m, 'underlyingToSettleMultiplier')));

          const lotSize = parseFloat(v(m, 'lotSize')) || 1;

          let contractSize: number | undefined = undefined;
          if (m.isQuanto) {
            const multiplier = m.multiplier / SATOSHI_PER_BTC;
            const price = m.markPrice;

            const contractValueInBtc = multiplier * price;
            const contractValueInUsd = contractValueInBtc * btcPrice;
            const contractValueInBase = round(contractValueInUsd / price);
            // safety to ensure it's not 0
            contractSize = contractValueInBase <= 0 ? 1 : contractValueInBase;
          }
          const amountPrecision: number = handleRoundingError(
            m.isQuanto ? (contractSize || 1) * lotSize : amountPrecisionRaw * lotSize,
          );
          const initMargin = v(m, 'initMargin') || 0.01;

          return {
            id: `${baseAsset}/${quoteAsset}:${marginAsset}`,
            symbol: m.symbol,
            base: this.formatCoin(baseAsset),
            quote: this.formatCoin(quoteAsset),
            active: true,
            contractSize,
            isInverse: m.isInverse,
            precision: {
              amount: amountPrecision,
              price: parseFloat(v(m, 'tickSize')),
            },
            limits: {
              amount: {
                min: 0,
                max: v(m, 'maxOrderQty'),
              },
              leverage: {
                min: 1,
                max: 1 / initMargin,
              },
            },
            info: m,
          };
        });

      const tickers: Ticker[] = data.reduce((acc: Ticker[], m) => {
        const market = markets.find(({ symbol }) => symbol === m.symbol);

        if (!market) return acc;

        return [
          ...acc,
          {
            id: market.id,
            symbol: market.symbol,
            bid: m.bidPrice,
            ask: m.askPrice,
            last: m.lastPrice,
            mark: m.markPrice,
            index: m.fairPrice,
            percentage: ((m.lastPrice - m.prevClosePrice) / m.prevClosePrice) * 100,
            openInterest: m.openValue,
            fundingRate: m.fundingRate,
            fundingTime: 0,
            volume: m.homeNotional24h,
            quoteVolume: m.foreignNotional24h,
          },
        ];
      }, []);

      return { markets, tickers: tickers };
    } catch (err: any) {
      this.emitAndLogError(this.parseError(err));
      return { markets: this.store.markets, tickers: this.store.tickers };
    }
  };

  observePositionMode = (positions: Array<Pick<BITMEX_POSITION, 'strategy'>>) => {
    const strategies = positions.map((position) => position.strategy);
    let positionMode = strategies.some((strategy) => strategy === 'Long' || strategy === 'Short')
      ? PositionMode.Hedged
      : strategies.some((strategy) => strategy === 'OneWay')
        ? PositionMode.OneWay
        : undefined;
    if (!positions.length && this.accountPositionMode === undefined) {
      positionMode = this.store.options?.isHedged ? PositionMode.Hedged : PositionMode.OneWay;
    }

    if (positionMode !== undefined) {
      this.accountPositionMode = positionMode;
      this.store.setSetting('isHedged', positionMode === PositionMode.Hedged);
    }
  };

  private ensurePositionModeResolved = async () => {
    if (this.accountPositionMode === undefined) {
      await this.fetchOpenPositions();
    }
    if (this.accountPositionMode === undefined) {
      throw new Error('Unable to determine BitMEX position mode from the /position response');
    }
  };

  private getOrderStrategy = (opts: Pick<PlaceOrderOpts, 'side' | 'reduceOnly'>): BitmexStrategy => {
    const positionMode =
      this.accountPositionMode ?? (this.store.options?.isHedged ? PositionMode.Hedged : PositionMode.OneWay);
    if (positionMode === PositionMode.OneWay) {
      return 'OneWay';
    }
    if (opts.reduceOnly) {
      return opts.side === OrderSide.Buy ? 'Short' : 'Long';
    }
    return opts.side === OrderSide.Buy ? 'Long' : 'Short';
  };

  fetchPositionMetadata = async (
    symbols: string[],
  ): Promise<
    {
      symbol: string;
      side: PositionSide;
      leverage: number;
      positionMode: PositionMode;
      marginType: MarginType;
    }[]
  > => {
    const positionMode =
      this.accountPositionMode ?? (this.store.options?.isHedged ? PositionMode.Hedged : PositionMode.OneWay);
    const sides =
      positionMode === PositionMode.Hedged ? [PositionSide.Long, PositionSide.Short] : [PositionSide.Long];
    return symbols.flatMap((symbol) =>
      sides.map((side) => ({
        symbol,
        side,
        leverage: this.store?.marketsBySymbol[symbol]?.limits?.leverage?.max || 100,
        positionMode,
        marginType: MarginType.Cross,
      })),
    );
  };

  fetchPositions = async (symbols?: string[]) => {
    try {
      const openPositions = await this.fetchOpenPositions();
      if (this.isDisposed) return [];

      const targetSymbols = symbols || [
        ...new Set([
          ...openPositions.map((p) => p.symbol),
          ...this.store.orders.filter((o) => o.status === OrderStatus.Open).map((o) => o.symbol),
        ]),
      ];

      if (!targetSymbols.length) {
        return openPositions;
      }

      const openPositionsForSymbols = openPositions.filter((p) => targetSymbols.includes(p.symbol));
      const missingSymbols = targetSymbols.filter(
        (symbol) => !openPositionsForSymbols.some((p) => p.symbol === symbol),
      );
      const positionMetadatas = await this.fetchPositionMetadata(missingSymbols);
      const fakePositions: Position[] = positionMetadatas.map((metadata) => {
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

      if (symbols) {
        return [...openPositionsForSymbols, ...fakePositions];
      }

      return [...openPositions, ...fakePositions];
    } catch (err: any) {
      const msg = this.parseError(err);
      this.emitAndLogError(err, msg);
      if (this.isBitmexAuthError(msg)) {
        throw new Error(msg);
      }
      return this.store.positions;
    }
  };

  fetchOpenPositions = async () => {
    try {
      const { data } = await this.xhr.get<Array<BITMEX_POSITION>>(ENDPOINTS.POSITIONS);
      if ((data as any).error) throw new Error(this.parseError(data));
      this.observePositionMode(data);
      // [
      //   {
      //     "account": 2169568,
      //     "symbol": "XBTUSD",
      //     "currency": "XBt",
      //     "underlying": "XBT",
      //     "quoteCurrency": "USD",
      //     "commission": 0.00075,
      //     "initMarginReq": 0.01,
      //     "maintMarginReq": 0.0035,
      //     "riskLimit": 20000000000,
      //     "leverage": 100,
      //     "crossMargin": true,
      //     "deleveragePercentile": 1,
      //     "rebalancedPnl": 0,
      //     "prevRealisedPnl": 0,
      //     "prevUnrealisedPnl": 0,
      //     "openingQty": 100,
      //     "openOrderBuyQty": 0,
      //     "openOrderBuyCost": 0,
      //     "openOrderBuyPremium": 0,
      //     "openOrderSellQty": 0,
      //     "openOrderSellCost": 0,
      //     "openOrderSellPremium": 0,
      //     "currentQty": 100,
      //     "currentCost": -164165,
      //     "currentComm": 175,
      //     "realisedCost": -144,
      //     "unrealisedCost": -164021,
      //     "grossOpenPremium": 0,
      //     "isOpen": true,
      //     "markPrice": 61199.17,
      //     "markValue": -163401,
      //     "riskValue": 163401,
      //     "homeNotional": 0.00163401,
      //     "foreignNotional": -100,
      //     "posCost": -164165,
      //     "posCross": 52,
      //     "posComm": 125,
      //     "posLoss": 52,
      //     "posMargin": 1767,
      //     "posMaint": 827,
      //     "initMargin": 0,
      //     "maintMargin": 2387,
      //     "realisedPnl": -31,
      //     "unrealisedPnl": 620,
      //     "unrealisedPnlPcnt": 0.0047,
      //     "unrealisedRoePcnt": 0.4654,
      //     "avgCostPrice": 60914.5,
      //     "avgEntryPrice": 60967.8,
      //     "breakEvenPrice": 60979.5,
      //     "marginCallPrice": 34455.5,
      //     "liquidationPrice": 34455.5,
      //     "bankruptPrice": 34372,
      //     "timestamp": "2024-02-28T22:15:40.729Z"
      //   }
      // ]
      return data
        .filter(this.filterBitmexPosition)
        .map(this.mapPosition)
        .filter((p) => p !== undefined) as Position[];
    } catch (err: any) {
      const msg = this.parseError(err);
      this.emitAndLogError(err, msg);
      if (this.isBitmexAuthError(msg)) {
        throw new Error(msg);
      }
      return this.store.positions;
    }
  };

  filterBitmexPosition = (p: BITMEX_POSITION) => {
    const normalizedCurrency = this.normalizeBitmexCurrencyCode(p.currency);
    return ['XBT', 'USDT', 'USD'].includes(normalizedCurrency) && !!this.store.marketsBySymbol[p.symbol];
  };

  convertFromMicroValue = (value: number, market: Market) => {
    const m = market.info;
    if (m.isQuanto || m.isInverse) {
      return value;
    }
    const divisor =
      parseFloat(v(m, 'underlyingToPositionMultiplier')) || parseFloat(v(m, 'underlyingToSettleMultiplier'));
    return handleRoundingError(value / divisor);
  };

  convertToMicroValue = (value: number, market: Market) => {
    const m = market.info;
    if (m.isQuanto || m.isInverse) {
      return value;
    }
    const mult = parseFloat(v(m, 'underlyingToPositionMultiplier')) || parseFloat(v(m, 'underlyingToSettleMultiplier'));

    return handleRoundingError(value * mult);
  };

  private getQuantoContractSize = (market: Market) => {
    const contractSize = Number(market.contractSize);
    return Number.isFinite(contractSize) && contractSize > 0 ? contractSize : 1;
  };

  private convertQuantoContractsToBase = (contracts: number, market: Market) =>
    round(multiply(contracts, this.getQuantoContractSize(market)));

  private getBitmexLotSize = (market: Market) => {
    const lotSize = parseFloat(v(market.info, 'lotSize'));
    return Number.isFinite(lotSize) && lotSize > 0 ? lotSize : 1;
  };

  private convertBaseToQuantoContracts = (baseAmount: number, market: Market) =>
    adjust(baseAmount / this.getQuantoContractSize(market), this.getBitmexLotSize(market));

  mapPosition = (p: Record<string, any>): Position | undefined => {
    const market = this.store.marketsBySymbol[p.symbol];
    if (!market) return undefined;
    const entryPrice = parseFloat(v(p, 'avgCostPrice'));
    let contracts = parseFloat(v(p, 'currentQty'));
    if (this.convertOutputsFromMicro) {
      contracts = this.convertFromMicroValue(contracts, market);
    }
    let contractsInBase = contracts;
    if (market.info.isQuanto) {
      contractsInBase = this.convertQuantoContractsToBase(contracts, market);
    } else if (market.isInverse) {
      const ticker = this.store.tickersBySymbol[p.symbol];
      const referencePrice = ticker?.mark || ticker?.last || 0;
      if (referencePrice) {
        contractsInBase = round(contracts / referencePrice);
      }
    }
    const markPrice = parseFloat(v(p, 'markPrice'));
    const markBasedUnrealizedPnl = multiply(contractsInBase, subtract(markPrice, entryPrice));
    const exchangeUnrealizedPnlSource = v(p, 'unrealisedPnl') ?? v(p, 'unrealizedPnl');
    const exchangeUnrealizedPnlRaw =
      exchangeUnrealizedPnlSource === undefined ||
      exchangeUnrealizedPnlSource === null ||
      exchangeUnrealizedPnlSource === ''
        ? undefined
        : parseFloat(exchangeUnrealizedPnlSource);
    const hasExchangeUnrealizedPnl =
      exchangeUnrealizedPnlRaw !== undefined && Number.isFinite(exchangeUnrealizedPnlRaw);
    const unrealizedPnl = hasExchangeUnrealizedPnl
      ? this.convertBitmexPnlToUsd(exchangeUnrealizedPnlRaw, v(p, 'currency'))
      : markBasedUnrealizedPnl;
    const normalizedCurrency = this.normalizeBitmexCurrencyCode(v(p, 'currency'));
    const unrealizedPnlNative =
      normalizedCurrency === 'XBT'
        ? hasExchangeUnrealizedPnl
          ? this.convertToRealAmount('XBt', exchangeUnrealizedPnlRaw)
          : this.convertToBTC(unrealizedPnl)
        : 0;
    // BitMEX linear markets can report realisedPnl=0 while rebalancedPnl carries useful realized value.
    // Prefer rebalancedPnl when explicit realized is missing or exactly zero.
    const explicitRealizedSource = v(p, 'realisedPnl') ?? v(p, 'realizedPnl');
    const explicitRealizedRaw =
      explicitRealizedSource === undefined || explicitRealizedSource === null || explicitRealizedSource === ''
        ? undefined
        : parseFloat(explicitRealizedSource);
    const rebalancedPnlSource = v(p, 'rebalancedPnl');
    const rebalancedPnlRaw =
      rebalancedPnlSource === undefined || rebalancedPnlSource === null || rebalancedPnlSource === ''
        ? undefined
        : parseFloat(rebalancedPnlSource);
    const shouldPreferRebalancedPnl =
      !market.isInverse &&
      Number.isFinite(rebalancedPnlRaw) &&
      rebalancedPnlRaw !== 0 &&
      (explicitRealizedRaw === undefined || explicitRealizedRaw === 0);
    const realizedPnlRaw = shouldPreferRebalancedPnl ? rebalancedPnlRaw : explicitRealizedRaw;
    const realizedPnl =
      realizedPnlRaw === undefined || Number.isNaN(realizedPnlRaw) || realizedPnlRaw === 0
        ? undefined
        : this.convertBitmexPnlToUsd(realizedPnlRaw, v(p, 'currency'));

    let notional: number;
    if (market.isInverse) {
      notional = contracts;
    } else if (market.info.isQuanto) {
      notional = round(contractsInBase * parseFloat(v(p, 'markPrice')));
    } else if (v(p, 'currency') === 'USDt') {
      notional = round(contractsInBase * parseFloat(v(p, 'markPrice')));
    } else {
      const baseNotional = multiply(Math.abs(contracts), entryPrice);
      notional = add(baseNotional, unrealizedPnl);
    }

    const strategy = v(p, 'strategy') as BitmexStrategy | undefined;
    const positionMode =
      strategy === 'Long' || strategy === 'Short' ? PositionMode.Hedged : PositionMode.OneWay;
    const side =
      strategy === 'Long'
        ? PositionSide.Long
        : strategy === 'Short'
          ? PositionSide.Short
          : contracts > 0
            ? PositionSide.Long
            : PositionSide.Short;

    return {
      id: createPositionId(p.symbol, side, positionMode),
      symbol: p.symbol,
      side,
      entryPrice,
      notional: Math.abs(notional),
      leverage: parseFloat(p.leverage),
      unrealizedPnl,
      ...(realizedPnl !== undefined ? { realizedPnl } : {}),
      contracts: Math.abs(contractsInBase),
      liquidationPrice: parseFloat(v(p, 'liquidationPrice')),
      positionMode,
      marginType: p.crossMargin ? MarginType.Cross : MarginType.Isolated,
      extra: {
        contractsNative: Math.abs(contracts),
        unrealizedPnlNative,
      },
      info: p,
    };
  };

  fetchOrders = async () => {
    // {
    //   "orderID": "91e9a8e6-2318-419d-b061-3b1e2aa0d2da",
    //   "account": 2169568,
    //   "symbol": "XBTUSD",
    //   "side": "Buy",
    //   "orderQty": 100,
    //   "price": 58000,
    //   "currency": "USD",
    //   "settlCurrency": "XBt",
    //   "ordType": "Limit",
    //   "timeInForce": "GoodTillCancel",
    //   "ordStatus": "New",
    //   "workingIndicator": true,
    //   "leavesQty": 100,
    //   "cumQty": 0,
    //   "text": "Submission from www.bitmex.com",
    //   "transactTime": "2024-03-01T16:50:02.118Z",
    //   "timestamp": "2024-03-01T16:50:02.118Z"
    // }
    try {
      const { data } = await this.xhr.get<Array<BITMEX_ORDER>>(ENDPOINTS.ORDER, {
        params: {
          reverse: true,
          filter: JSON.stringify({
            open: true,
          }),
        },
      });
      if ((data as any).error) throw new Error(this.parseError(data));
      const orders: Order[] = data.map(this.mapOrder);
      return orders;
    } catch (err: any) {
      const msg = this.parseError(err);
      this.emitAndLogError(err, msg);
      // Throw instead of returning the stale cached orders. fetchOrders
      // only feeds wholesale store replacement (fetchAndSetPrivateSnapshot
      // on reconnect, fetchAndSetOrders) — replacing the store with its
      // own stale set re-affirms phantom orders (canceled/filled rows the
      // live WS already dropped), marks orders loaded, and defeats the
      // snapshot retry loop, which only retries when fetchOrders throws.
      // Auth errors already threw here; transient errors now do too, so a
      // failed reconnect snapshot retries cleanly rather than locking in
      // phantom orders that a later `cancel` could act on (TSBUG-103).
      throw new Error(msg);
    }
  };

  mapOrder = (o: BITMEX_ORDER): Order => {
    this.trackBitmexMaxSlippagePct(v(o, 'maxSlippagePct'));
    // "pegOffsetValue": 200,
    //   "pegPriceType": "TrailingStopPeg",
    const price = parseFloat(String(o.price)) || parseFloat(v(o, 'stopPx'));
    let type = OrderType.Limit;

    if (o.ordType === 'Stop') {
      type = OrderType.StopLoss;
    } else if (o.ordType === 'MarketIfTouched') {
      type = OrderType.TakeProfit;
    } else if (o.ordType === 'Market') {
      type = OrderType.Market;
    }

    const market = this.store.marketsBySymbol[o.symbol];
    const rawOrderQty = parseFloat(v(o, 'orderQty') || 0);
    const rawLeavesQty = parseFloat(v(o, 'leavesQty') || 0);
    const orderQty =
      this.convertOutputsFromMicro && market ? this.convertFromMicroValue(rawOrderQty, market) : rawOrderQty;
    const remaining =
      this.convertOutputsFromMicro && market ? this.convertFromMicroValue(rawLeavesQty, market) : rawLeavesQty;

    let amount = orderQty;
    let filled = subtract(orderQty, remaining);
    let remainingInBase = remaining;
    let remainingNotional: number | undefined = undefined;
    if (market?.info.isQuanto) {
      amount = this.convertQuantoContractsToBase(orderQty, market);
      filled = this.convertQuantoContractsToBase(subtract(orderQty, remaining), market);
      remainingInBase = this.convertQuantoContractsToBase(remaining, market);
    } else if (market?.isInverse) {
      remainingNotional = remaining;
      const referencePrice = price;
      if (referencePrice) {
        remainingInBase = round(remaining / referencePrice);
      }
    }

    let timeInForce = TIME_IN_FORCE[o.timeInForce] || OrderTimeInForce.GoodTillCancel;
    let reduceOnly = false;
    const execInst = v(o, 'execInst') || '';
    if (execInst.includes('ParticipateDoNotInitiate')) {
      timeInForce = OrderTimeInForce.PostOnly;
    }
    if (execInst.includes('ReduceOnly') || execInst.includes('Close')) {
      reduceOnly = true;
    }

    let rejectReason: OrderRejectReason | undefined = undefined;
    if (o.ordStatus === 'Canceled' && o.text.includes('Canceled: Order had execInst of ParticipateDoNotInitiate')) {
      rejectReason = OrderRejectReason.PostOnly;
    } else if (
      o.ordStatus === 'Canceled' &&
      o.text.includes('Canceled: Order had execInst of Close or ReduceOnly but current position is 0')
    ) {
      rejectReason = OrderRejectReason.ReduceOnly;
    }

    return {
      id: v(o, 'orderID'),
      status: ORDER_STATUS[o.ordStatus],
      symbol: o.symbol,
      type,
      side: ORDER_SIDE[o.side],
      price,
      amount,
      reduceOnly: reduceOnly,
      timeInForce,
      filled,
      lastUpdate: virtualClock.getCurrentTimeMs(),
      remaining: remainingInBase,
      extra: {
        remainingNative: remaining,
        remainingNotional,
        rejectReason,
      },
      info: o,
      ...(type === OrderType.TakeProfit && {
        tpTriggerType: execInst.includes('MarkPrice')
          ? OrderStopTriggerType.MarkPrice
          : execInst.includes('IndexPrice')
            ? OrderStopTriggerType.IndexPrice
            : OrderStopTriggerType.LastPrice,
      }),
      ...(type === OrderType.StopLoss && {
        slTriggerType: execInst.includes('MarkPrice')
          ? OrderStopTriggerType.MarkPrice
          : execInst.includes('IndexPrice')
            ? OrderStopTriggerType.IndexPrice
            : OrderStopTriggerType.LastPrice,
      }),
    };
  };

  private convertBitmexPnlToUsd = (rawPnl: number, currency: string | undefined): number => {
    if (!Number.isFinite(rawPnl)) {
      return 0;
    }
    const normalizedCurrency = this.normalizeBitmexCurrencyCode(currency);
    if (normalizedCurrency === 'XBT') {
      return this.convertToUSD(this.convertToRealAmount('XBt', rawPnl));
    }
    if (normalizedCurrency === 'USDT') {
      return this.convertToRealAmount('USDt', rawPnl);
    }
    if (normalizedCurrency === 'USD') {
      return this.convertToRealAmount('USD', rawPnl);
    }
    return rawPnl;
  };

  private parseBitmexExecution = (execution: BITMEX_EXECUTION) => {
    this.trackBitmexMaxSlippagePct(v(execution, 'maxSlippagePct'));
    const symbol = execution.symbol;
    if (!symbol) {
      return undefined;
    }

    const market = this.store.marketsBySymbol[symbol];
    const rawQty = parseFloat(String(v(execution, 'lastQty') ?? 0));
    const rawPrice = parseFloat(String(v(execution, 'lastPx') ?? v(execution, 'price') ?? 0));
    const sideRaw = String(v(execution, 'side') || '').toLowerCase();
    const timeSource = v(execution, 'transactTime') || v(execution, 'timestamp');
    const parsedTime = typeof timeSource === 'number' ? timeSource : Date.parse(String(timeSource || ''));

    const qty =
      this.convertOutputsFromMicro && market
        ? Math.abs(this.convertFromMicroValue(Math.abs(rawQty), market))
        : Math.abs(rawQty);
    const price = Number.isFinite(rawPrice) ? rawPrice : 0;
    const side = sideRaw === 'buy' ? OrderSide.Buy : sideRaw === 'sell' ? OrderSide.Sell : undefined;
    const time = Number.isFinite(parsedTime) ? parsedTime : undefined;

    if (!Number.isFinite(qty) || qty <= 0 || !Number.isFinite(price) || price <= 0 || !side || !time) {
      return undefined;
    }

    return { symbol, qty, price, side, time };
  };

  private fetchExecutionTradeHistory = async (symbol: string, count: number): Promise<BITMEX_EXECUTION[]> => {
    const boundedCount = Math.min(Math.max(count, 1), 500);
    const query = new URLSearchParams({
      symbol,
      reverse: 'true',
      count: String(boundedCount),
    });
    try {
      const { data } = await this.xhr.get<BITMEX_EXECUTION[]>(`${ENDPOINTS.EXECUTIONS}?${query.toString()}`);
      if ((data as any)?.error) {
        throw new Error(this.parseError(data));
      }
      return Array.isArray(data) ? data.slice(0, boundedCount) : [];
    } catch (primaryError: any) {
      const parsedPrimaryError = this.parseError(primaryError);
      const shouldTryUserExecutionHistory =
        typeof parsedPrimaryError === 'string' &&
        (parsedPrimaryError.toLowerCase().includes('signature not valid') ||
          parsedPrimaryError.toLowerCase().includes('not found'));
      if (!shouldTryUserExecutionHistory) {
        throw primaryError;
      }

      const { data } = await this.xhr.get<BITMEX_EXECUTION[]>(
        `${ENDPOINTS.USER_EXECUTION_HISTORY}?${query.toString()}`,
      );
      if ((data as any)?.error) {
        throw new Error(this.parseError(data));
      }
      return Array.isArray(data) ? data.slice(0, boundedCount) : [];
    }
  };

  private filterTradeExecutions = (entries: BITMEX_EXECUTION[], orderId?: string): BITMEX_EXECUTION[] => {
    return entries.filter((entry) => {
      if (orderId && String(entry.orderID) !== String(orderId)) {
        return false;
      }
      return String(v(entry, 'execType') || '').toLowerCase() === 'trade';
    });
  };

  private parseExecutionCommissionUsd = (entry: BITMEX_EXECUTION): number | undefined => {
    const feeRaw = parseFloat(String(v(entry, 'execComm') ?? 0));
    if (!Number.isFinite(feeRaw) || feeRaw === 0) {
      return undefined;
    }
    const feeCurrency =
      v(entry as unknown as Record<string, any>, 'execCommCcy') || this.resolveExecutionSettlementCurrency(entry);
    return Math.abs(this.convertBitmexPnlToUsd(feeRaw, feeCurrency));
  };

  private parseExecutionRealizedPnlUsd = (entry: BITMEX_EXECUTION): number | undefined => {
    const pnlSource = v(entry, 'realisedPnl') ?? v(entry, 'realizedPnl') ?? v(entry, 'closedPnl');
    const pnlRaw =
      pnlSource === undefined || pnlSource === null || pnlSource === '' ? undefined : parseFloat(String(pnlSource));
    if (pnlRaw === undefined || Number.isNaN(pnlRaw)) {
      return undefined;
    }
    const pnlCurrency =
      v(entry as unknown as Record<string, any>, 'realisedPnlCcy') ||
      v(entry as unknown as Record<string, any>, 'realizedPnlCcy') ||
      this.resolveExecutionSettlementCurrency(entry);
    return this.convertBitmexPnlToUsd(pnlRaw, pnlCurrency);
  };

  private resolveExecutionSettlementCurrency = (entry: BITMEX_EXECUTION): string | undefined => {
    const market = this.store.marketsBySymbol[entry.symbol];
    const marketSettlementCurrency = market?.info ? v(market.info as Record<string, any>, 'settlCurrency') : undefined;
    if (marketSettlementCurrency) {
      return marketSettlementCurrency as string;
    }

    const entrySettlementCurrency = entry.settlCurrency || entry.currency;
    const normalizedEntrySettlementCurrency = this.normalizeBitmexCurrencyCode(entrySettlementCurrency);
    if (entrySettlementCurrency && normalizedEntrySettlementCurrency !== 'USD') {
      return entrySettlementCurrency;
    }

    const normalizedSymbol = String(entry.symbol || '').toUpperCase();
    if (normalizedSymbol.endsWith('USDT')) {
      return 'USDt';
    }
    if (normalizedSymbol.endsWith('USD')) {
      return 'XBt';
    }

    // For some products, execution payload currency fields can be quote currency while values are
    // still scaled in settlement currency units. Prefer market settlement currency when available.
    return entrySettlementCurrency;
  };

  private handleExecutionHistoryError = <T>(err: any): T[] => {
    const parsedError = this.parseError(err);
    const isSignatureError =
      typeof parsedError === 'string' && parsedError.toLowerCase().includes('signature not valid');
    if (isSignatureError) {
      const now = Date.now();
      if (now - this.lastExecutionHistorySignatureWarningAt > 30_000) {
        this.lastExecutionHistorySignatureWarningAt = now;
        this.log(
          `BitMEX execution history request failed auth; skipping optional history fetch: ${parsedError}`,
          LogSeverity.Warning,
        );
      }
      return [];
    }
    this.emitAndLogError(parsedError);
    return [];
  };

  fetchExecutions = async (opts: ExecutionsOptions): Promise<Execution[]> => {
    if (this.isPublicDataOnly || !opts.symbol) {
      return [];
    }

    try {
      const count = Math.min(Math.max(opts.limit || 200, 1), 500);

      const rawEntries = await this.fetchExecutionTradeHistory(opts.symbol, count);
      const tradeEntries = this.filterTradeExecutions(rawEntries, opts.orderId);

      const executions = tradeEntries
        .map((entry): Execution | null => {
          const parsed = this.parseBitmexExecution(entry);
          if (!parsed) {
            return null;
          }

          const commission = this.parseExecutionCommissionUsd(entry);

          return omitUndefined({
            timestamp: Math.floor(parsed.time / 1000),
            symbol: parsed.symbol,
            orderId: entry.orderID ? String(entry.orderID) : undefined,
            price: parsed.price,
            qty: parsed.qty,
            side: parsed.side,
            commission,
            commissionAsset: commission !== undefined ? 'USD' : undefined,
          });
        })
        .filter((entry): entry is Execution => !!entry);

      return executions;
    } catch (err: any) {
      return this.handleExecutionHistoryError<Execution>(err);
    }
  };

  fetchHistoricalTrades = async (opts: HistoricalTradeOptions): Promise<HistoricalTrade[]> => {
    if (this.isPublicDataOnly || !opts.symbol) {
      return [];
    }

    try {
      const count = Math.min(Math.max(opts.limit || 500, 1), 500);

      const rawEntries = await this.fetchExecutionTradeHistory(opts.symbol, count);
      const tradeEntries = this.filterTradeExecutions(rawEntries, opts.orderId);

      const historicalTrades = tradeEntries
        .map((entry): HistoricalTrade | null => {
          const parsed = this.parseBitmexExecution(entry);
          if (!parsed) {
            return null;
          }

          const realizedPnl = this.parseExecutionRealizedPnlUsd(entry);
          const commission = this.parseExecutionCommissionUsd(entry);

          const orderId = entry.orderID
            ? String(entry.orderID)
            : String(entry.execID || `${parsed.symbol}-${parsed.time}`);

          return omitUndefined({
            symbol: parsed.symbol,
            orderId,
            price: parsed.price,
            qty: parsed.qty,
            realizedPnl,
            positionSide: 'both' as const,
            side: parsed.side,
            commission,
            commissionAsset: commission !== undefined ? 'USD' : undefined,
            time: parsed.time,
          });
        })
        .filter((entry): entry is HistoricalTrade => !!entry);

      return historicalTrades;
    } catch (err: any) {
      return this.handleExecutionHistoryError<HistoricalTrade>(err);
    }
  };

  fetchOHLCV = async (opts: OHLCVOptions) => {
    const interval = opts.interval;
    const plan = getBitmexOHLCVPlan(interval);
    if (!plan) {
      throw new Error(`Unsupported interval: ${interval}`);
    }

    if (!plan.isNative) {
      const sourceLimit = opts.limit
        ? Math.min(opts.limit * plan.sourcePerTarget + plan.sourcePerTarget, 1500)
        : undefined;

      const sourceCandles = await this.fetchOHLCV({
        ...opts,
        interval: plan.sourceInterval,
        ...(sourceLimit ? { limit: sourceLimit } : {}),
      });

      const aggregated = aggregateBitmexCandles(sourceCandles, interval);
      const filtered = aggregated.filter((candle) => {
        if (opts.from && candle.timestamp < dayjs(opts.from).unix()) return false;
        if (opts.to && candle.timestamp > dayjs(opts.to).unix()) return false;
        return true;
      });

      if (opts.limit) {
        return filtered.slice(-opts.limit);
      }
      return filtered;
    }

    const resolution = SupportedTimeframes[interval];
    if (!resolution) {
      throw new Error(`Unsupported interval: ${interval}`);
    }
    const limit = Math.min(opts.limit || 500, 1500);
    const [, amount, unit] = opts.interval.split(/(\d+)/);

    const end = opts.to ? dayjs(opts.to) : dayjs();
    const start =
      !opts.limit && opts.from ? dayjs(opts.from) : end.subtract(parseFloat(amount) * limit, unit as ManipulateType);

    // Use proxy endpoint for UDF data
    const baseUrl = this.options.testnet
      ? `${getPublicCorsProxy()}/https://testnet.bitmex.com/api/udf`
      : `${getPublicCorsProxy()}/https://www.bitmex.com/api/udf`;

    const params = new URLSearchParams({
      symbol: opts.symbol,
      resolution,
      from: start.unix().toString(),
      to: end.unix().toString(),
      ...(opts.limit ? { limit: limit.toString() } : {}),
    });

    const url = `${baseUrl}/history?${params.toString()}`;

    try {
      const response = await fetch(url);
      const data = (await response.json()) as {
        t: number[];
        o: number[];
        h: number[];
        l: number[];
        c: number[];
        v: number[];
        s?: string;
      };

      const candles: Candle[] = [];

      // Validate response structure before accessing arrays
      if (!data || !data.t || !Array.isArray(data.t)) {
        this.log(`Invalid OHLCV response from BitMEX for ${opts.symbol}: ${JSON.stringify(data)}`, LogSeverity.Warning);
        return candles; // Return empty array instead of crashing
      }

      for (let i = 0; i < data.t.length; i++) {
        const o = data.o[i];
        const h = data.h[i];
        const l = data.l[i];
        const c = data.c[i];
        candles.push({
          timestamp: data.t[i],
          open: o,
          high: Math.max(h, o, c),
          low: Math.min(l, o, c),
          close: c,
          volume: data.v[i],
        });
      }

      return candles;
    } catch (err: any) {
      this.log(`Failed to fetch OHLCV data from BitMEX: ${err.message}`, LogSeverity.Error);
      return [];
    }
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    return this.publicWebsocket.listenOHLCV(opts, callback);
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    return this.publicWebsocket.listenOrderBook(symbol, callback);
  };

  setLeverage = async (symbol: string, inputLeverage: number) => {
    const market = this.store.marketsBySymbol[symbol];
    const position = this.store.positions.find((p) => p.symbol === symbol);

    if (!market) throw new Error(`Market ${symbol} not found`);
    if (!position) throw new Error(`Position ${symbol} not found`);

    const leverage = Math.min(Math.max(inputLeverage, market.limits.leverage.min), market.limits.leverage.max);

    if (position.leverage !== leverage) {
      try {
        const { data } = await this.xhr.post(ENDPOINTS.SET_LEVERAGE, {
          symbol,
          leverage,
        });

        if ((data as any).error) throw new Error(this.parseError(data));

        const updatedPosition = this.mapPosition(data);
        if (updatedPosition) {
          this.store.addOrUpdatePosition(updatedPosition);
        }
      } catch (err: any) {
        this.emitAndLogError(this.parseError(err));
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
      const { data } = await this.xhr.post(ENDPOINTS.SET_ISOLATE, {
        symbol,
        enabled: isolated,
      });

      if ((data as any).error) throw new Error(this.parseError(data));

      const updatedPosition = this.mapPosition(data);
      if (updatedPosition) {
        this.store.addOrUpdatePosition(updatedPosition);
      }
    } catch (err: any) {
      this.emitAndLogError(this.parseError(err));
      throw err;
    }
  };

  changePositionMode = async (hedged: boolean) => {
    try {
      const { data } = await this.xhr.post(
        ENDPOINTS.SET_POSITION_MODE,
        hedged ? { positionMode: 'MultiWay' } : {},
      );
      if (data?.error) throw new Error(this.parseError(data));

      this.accountPositionMode = hedged ? PositionMode.Hedged : PositionMode.OneWay;
      this.store.setSetting('isHedged', hedged);
      this.store.update({ positions: [] });
      const positions = await this.fetchPositions();
      if (!this.isDisposed) {
        this.store.addOrUpdatePositionsFull(positions);
      }
    } catch (err: unknown) {
      this.emitAndLogError(this.parseError(err));
      throw err;
    }
  };

  cancelOrders = async (orders: Order[]) => {
    try {
      const groupedBySymbol = groupBy(orders, 'symbol');
      const requests = Object.entries(groupedBySymbol).map(([symbol, symbolOrders]) => ({
        symbol,
        orderID: symbolOrders.map((o) => o.id),
      }));
      const errs: string[] = [];

      await forEachSeries(requests, async (request) => {
        if (request.orderID.length === 1) {
          const { data } = await this.xhr.delete(ENDPOINTS.ORDER, {
            params: {
              orderID: request.orderID[0],
            },
          });
          for (const d of data) {
            if (d.error) {
              errs.push(d.error);
            }
          }
        } else {
          const lots = chunk(request.orderID, 10);
          await forEachSeries(lots, async (lot) => {
            const { data } = await this.xhr.delete(ENDPOINTS.ORDER, {
              params: {
                orderID: JSON.stringify(lot),
              },
            });
            for (const d of data) {
              if (d.error) {
                errs.push(d.error);
              }
            }
          });
        }
      });

      if (errs.length > 0) {
        throw new Error(errs.map((e) => this.parseError(e)).join(', '));
      }
    } catch (err: any) {
      const errMsg = this.parseError(err);
      this.emitAndLogError(err, errMsg);
      throw new Error(errMsg);
    }
  };

  cancelSymbolOrders = async (symbol: string) => {
    try {
      const { data } = await this.xhr.delete(ENDPOINTS.ORDER, {
        params: { symbol },
      });

      if ((data as any).error) throw new Error(this.parseError(data));

      this.store.updateOrdersStatus(
        this.store.orders.filter((o) => o.symbol === symbol),
        OrderStatus.Canceled,
      );
    } catch (err: any) {
      this.emitAndLogError(this.parseError(err));
      throw err;
    }
  };

  updateOrder = async ({ order, update }: UpdateOrderOpts) => {
    const market = this.store.marketsBySymbol[order.symbol];
    if (!market) throw new Error(`Market ${order.symbol} not found`);

    const isStopOrTP = order.type === OrderType.StopLoss || order.type === OrderType.TakeProfit;

    // Use native PUT /order amend
    const amendPayload: Record<string, any> = { orderID: order.id };

    if ('price' in update && update.price !== undefined) {
      const pPrice = market.precision.price;
      const priceField = isStopOrTP ? 'stopPx' : 'price';
      amendPayload[priceField] = adjust(update.price, pPrice);
    }

    if ('amount' in update && update.amount !== undefined) {
      const pAmount = market.precision.amount;
      const isQuanto = market.info?.isQuanto;
      let amount: number;
      if (isQuanto) {
        amount = this.convertBaseToQuantoContracts(update.amount, market);
      } else if (market.isInverse) {
        amount = adjust(update.amount, pAmount);
      } else if (this.convertInputsToMicro) {
        const adjustedAmount = adjust(update.amount, pAmount);
        amount = adjust(this.convertToMicroValue(adjustedAmount, market), pAmount);
      } else {
        amount = adjust(update.amount, pAmount);
      }
      amendPayload.orderQty = amount;
    }

    try {
      const { data } = await this.unlimitedXHR.put(ENDPOINTS.ORDER, amendPayload);

      if ((data as any)?.error) {
        throw new Error(this.parseError(data));
      }

      return [order.id];
    } catch (err: any) {
      this.emitAndLogError(err?.message || String(err));
      throw err;
    }
  };

  placeOrder = async (opts: PlaceOrderOpts) => {
    await this.ensurePositionModeResolved();
    const payloads = this.formatCreateOrder(opts);
    return await this.placeOrderBatch(payloads);
  };

  placeOrders = async (orders: PlaceOrderOpts[]) => {
    await this.ensurePositionModeResolved();
    const requests = orders.flatMap((o) => this.formatCreateOrder(o));
    return await this.placeOrderBatch(requests);
  };

  private formatCreateOrder = (opts: BitmexPlaceOrderOpts) => {
    const market = this.store.marketsBySymbol[opts.symbol];

    if (!market) {
      throw new Error(`Market ${opts.symbol} not found`);
    }

    const isStopOrTP = opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit;
    const pPrice = market.precision.price;
    const price = opts.price && opts.type !== OrderType.Market ? adjust(opts.price, pPrice) : undefined;
    const pAmount = market.precision.amount;
    const isQuanto = market.info?.isQuanto;
    let amount: number = 0;
    if (isQuanto) {
      amount = this.convertBaseToQuantoContracts(opts.amount, market);
    } else if (market.isInverse) {
      amount = adjust(opts.amount, pAmount);
    } else if (this.convertInputsToMicro) {
      const adjustedAmount = adjust(opts.amount, pAmount);
      amount = adjust(this.convertToMicroValue(adjustedAmount, market), pAmount);
    }

    const priceField = isStopOrTP ? 'stopPx' : 'price';
    const ordType =
      opts.type === OrderType.Market
        ? 'Market'
        : opts.type === OrderType.Limit
          ? 'Limit'
          : opts.type === OrderType.StopLoss
            ? 'StopMarket'
            : 'MarketIfTouched';
    const maxSlippagePct = this.resolveMaxSlippagePctForOrder(opts);
    const shouldSendMaxSlippagePct =
      maxSlippagePct !== undefined &&
      (ordType === 'Market' || ordType === 'StopMarket' || ordType === 'MarketIfTouched');
    const limitTimeInForce =
      opts.type !== OrderType.Limit
        ? undefined
        : opts.timeInForce === OrderTimeInForce.PostOnly || !opts.timeInForce
          ? OrderTimeInForce.GoodTillCancel
          : opts.timeInForce;

    const req = omitUndefined({
      symbol: opts.symbol,
      side: inverseObj(ORDER_SIDE)[opts.side],
      strategy: this.getOrderStrategy(opts),
      ordType,
      orderQty: amount ? `${amount}` : undefined,
      [priceField]: price ? `${price}` : undefined,
      maxSlippagePct: shouldSendMaxSlippagePct ? `${maxSlippagePct}` : undefined,
      timeInForce: limitTimeInForce,
      text: 'Tealstreet',
    });

    //const lots = amount > maxSize ? Math.ceil(amount / maxSize) : 1;
    //const rest = amount > maxSize ? adjust(amount % maxSize, pAmount) : 0;

    // const lotSize = adjust((amount - rest) / lots, pAmount);

    const execInstValues = [] as string[];
    if (opts.reduceOnly) {
      if (opts.type === OrderType.Market || opts.type === OrderType.Limit) {
        execInstValues.push('ReduceOnly');
      } else {
        execInstValues.push('Close');
      }
    }
    if (opts.timeInForce === OrderTimeInForce.PostOnly && opts.type === OrderType.Limit) {
      execInstValues.push('ParticipateDoNotInitiate');
    }
    if (opts.type === OrderType.TakeProfit && opts.tpTriggerType) {
      execInstValues.push(
        opts.tpTriggerType === OrderStopTriggerType.MarkPrice
          ? 'MarkPrice'
          : opts.tpTriggerType === OrderStopTriggerType.IndexPrice
            ? 'IndexPrice'
            : 'LastPrice',
      );
    }
    if (opts.type === OrderType.StopLoss && opts.slTriggerType) {
      execInstValues.push(
        opts.slTriggerType === OrderStopTriggerType.MarkPrice
          ? 'MarkPrice'
          : opts.slTriggerType === OrderStopTriggerType.IndexPrice
            ? 'IndexPrice'
            : 'LastPrice',
      );
    }

    const execInst = execInstValues.length > 0 ? execInstValues.join(',') : undefined;

    const payloads: Array<Record<string, any>> = [
      omitUndefined({
        ...req,
        execInst,
      }),
    ];

    if (opts.stopLoss) {
      payloads.push({
        ...omit(req, 'price'),
        side: inverseObj(ORDER_SIDE)[opts.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy],
        ordType: 'StopMarket',
        stopPx: `${adjust(opts.stopLoss, pPrice)}`,
        execInst: 'ReduceOnly',
        ...(maxSlippagePct !== undefined ? { maxSlippagePct: `${maxSlippagePct}` } : {}),
      });
    }

    if (opts.takeProfit) {
      payloads.push({
        ...omit(req, 'price'),
        side: inverseObj(ORDER_SIDE)[opts.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy],
        ordType: 'MarketIfTouched',
        stopPx: `${adjust(opts.takeProfit, pPrice)}`,
        execInst: 'ReduceOnly',
        ...(maxSlippagePct !== undefined ? { maxSlippagePct: `${maxSlippagePct}` } : {}),
      });
    }

    return payloads;
  };

  private placeOrderBatch = async (payloads: any[]) => {
    const lots = chunk(payloads, 3);
    const orderIds = [] as string[];

    for (const lot of lots) {
      let errToThrow: any;
      await Promise.all(
        lot.map(async (payload) => {
          try {
            const { data } = await this.unlimitedXHR.post(ENDPOINTS.ORDER, payload);
            if ((data as any).error) throw new Error(this.parseError(data));
            this.trackBitmexMaxSlippagePct((data as any)?.maxSlippagePct);
            orderIds.push(data.orderID);
          } catch (err: any) {
            const msg = this.parseError(err);
            this.emitAndLogError(err, msg);
            errToThrow = err;
          }
        }),
      );
      if (errToThrow) {
        throw errToThrow;
      }
    }

    // BitMEX private WS can briefly flap (code 1006) in some regions.
    // Refresh private snapshots after successful placement so UI reflects
    // the latest position/balance even when WS updates are delayed.
    try {
      const [balance, positions] = await Promise.all([this.fetchBalance(), this.fetchPositions()]);
      if (!this.isDisposed) {
        this.store.addOrUpdatePositionsFull(positions);
        this.store.update({
          balance,
          loaded: {
            ...this.store.loaded,
            balance: true,
            positions: true,
          },
        });
      }
    } catch (err: any) {
      this.emitAndLogError(this.parseError(err));
    }

    return orderIds;
  };

  // private convertFromRealAmount(currency: string | number, amount: string | number) {
  //   const precision = this.currencies[currency].precision!;
  //   return divide(+amount, +precision);
  // }

  private normalizeBitmexCurrencyCode(currency: unknown): string {
    return String(currency || '').toUpperCase();
  }

  private resolveBitmexCurrencyPrecisionKey(currency: unknown): string {
    const rawCurrency = String(currency || '');
    if (this.currencies[rawCurrency]) {
      return rawCurrency;
    }

    const normalizedCurrency = this.normalizeBitmexCurrencyCode(rawCurrency);
    if (normalizedCurrency === 'XBT' && this.currencies.XBt) {
      return 'XBt';
    }
    if (normalizedCurrency === 'USDT' && this.currencies.USDt) {
      return 'USDt';
    }
    if (normalizedCurrency === 'MAMUSD' && this.currencies.MAMUSd) {
      return 'MAMUSd';
    }
    if (normalizedCurrency === 'USD' && this.currencies.USD) {
      return 'USD';
    }
    if (this.currencies[normalizedCurrency]) {
      return normalizedCurrency;
    }

    return rawCurrency;
  }

  private convertToRealAmount(currency: string | number, amount: string | number) {
    const currencyKey = this.resolveBitmexCurrencyPrecisionKey(currency);
    const currencyData = this.currencies[currencyKey];
    if (currencyData?.precision) {
      return multiply(+amount, +currencyData.precision);
    }

    // Execution-history conversion must remain safe even when /wallet/assets fails or races startup.
    const normalizedCurrency = this.normalizeBitmexCurrencyCode(currencyKey || currency);
    if (normalizedCurrency === 'XBT') {
      return multiply(+amount, 0.00000001);
    }
    if (normalizedCurrency === 'USDT' || normalizedCurrency === 'MAMUSD') {
      return multiply(+amount, 0.000001);
    }
    if (normalizedCurrency === 'USD') {
      return +amount;
    }

    return +amount;
  }

  private getBtcUsdPrice() {
    const priceCandidates = [
      this.store.tickersBySymbol['XBTUSDT']?.last,
      this.store.tickersBySymbol['XBTUSDT']?.mark,
      this.store.tickersBySymbol['XBTUSD']?.last,
      this.store.tickersBySymbol['XBTUSD']?.mark,
      this.store.tickersBySymbol['XBTUSD']?.index,
    ];

    for (const candidate of priceCandidates) {
      if (Number.isFinite(candidate) && candidate > 0) {
        return candidate;
      }
    }

    return 0;
  }

  private convertToUSD(amount: number) {
    return amount * this.getBtcUsdPrice();
  }

  private convertToBTC(amount: number) {
    const btcUsdPrice = this.getBtcUsdPrice();
    return btcUsdPrice > 0 ? amount / btcUsdPrice : 0;
  }

  private formatCoin(coin: string) {
    return coin.replace('XBT', 'BTC');
  }

  private isBitmexAuthError = (err: unknown): boolean => {
    const msg = String(err || '').toLowerCase();
    return [
      'invalid api key',
      'invalid api secret',
      'signature not valid',
      'signature is not valid',
      'unauthorized',
      'forbidden',
      'access denied',
      'permission denied',
      'not whitelisted',
      'ip address',
      'api key expired',
      'expired api key',
      'this key has insufficient permissions',
    ].some((pattern) => msg.includes(pattern));
  };

  private normalizeBitmexValidationError = (err: unknown): string => {
    const raw = String(err || '').trim();
    if (!raw) {
      return 'Invalid API Key';
    }

    const msg = raw.toLowerCase();
    if (msg.includes('signature not valid') || msg.includes('signature is not valid')) {
      return 'Invalid API key or secret';
    }
    if (msg.includes('not whitelisted') || msg.includes('ip address')) {
      return 'API key IP whitelist does not include the Tealstreet proxy/tunnel IP';
    }
    if (msg.includes('permission denied') || msg.includes('insufficient permissions')) {
      return 'API key does not have the required BitMEX permissions';
    }
    if (msg.includes('api key expired') || msg.includes('expired api key')) {
      return 'BitMEX API key has expired';
    }
    if (msg.includes('unauthorized') || msg.includes('forbidden')) {
      return 'Invalid API key or secret';
    }

    return raw;
  };

  parseError = (err: any) => {
    const message =
      err?.response?.data?.error?.message ||
      err?.response?.data?.msg ||
      err?.response?.error?.message ||
      err?.message ||
      err?.error?.message ||
      err?.error ||
      err;

    const requestUrl = String(err?.response?.config?.url || err?.config?.url || '').toLowerCase();
    const normalizedMessage = String(message || '').toLowerCase();
    if (normalizedMessage.includes('access denied') && requestUrl.includes('/order')) {
      return 'Access Denied when placing BitMEX order. Verify API key has Order (create) permission (not only Order Cancel), and that key restrictions allow trading.';
    }

    return message;
  };
}
