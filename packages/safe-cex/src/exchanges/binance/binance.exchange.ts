import type { Axios } from 'axios';
import type { ManipulateType } from 'dayjs';
import type { Store } from '../../store/store.interface';

import rateLimit from 'axios-rate-limit';
import dayjs from 'dayjs';
import chunk from 'lodash/chunk';
import groupBy from 'lodash/groupBy';
import times from 'lodash/times';
import { forEachSeries } from 'p-iteration';

import { createApiWithSecondaryKey } from '../../createApiWithSecondaryKey';
import {
  Balance,
  Candle,
  ExchangeName,
  ExchangeOptions,
  Execution,
  ExecutionsOptions,
  HistoricalTradeOptions,
  IncomeHistoryOptions,
  IncomeRecord,
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
  PublicTrade,
  Ticker,
  UpdateOrderOpts,
  Writable,
} from '../../types';
import { createPositionId } from '../../utils/createPositionId';
import { v } from '../../utils/get-key';
import { inverseObj } from '../../utils/inverseObj';
import { loop } from '../../utils/loop';
import { omitUndefined } from '../../utils/omitUndefined';
import { adjust, subtract } from '../../utils/safe-math';
import { uuid } from '../../utils/uuid';
import { virtualClock } from '../../utils/virtualClock';
import { BaseExchange } from '../base';
import { createAPI, createPublicAPI } from './binance.api';
import {
  BinanceIncomeRecord,
  ENDPOINTS,
  GET_POSITION_RISK_RESPONSE,
  GRAFANA_ENDPOINTS_EVERY,
  GRAFANA_ENDPOINTS_PERIODIC,
  ORDER_SIDE,
  ORDER_TRIGGER_TYPE,
  ORDER_TYPE,
  POSITION_SIDE,
  TIME_IN_FORCE,
} from './binance.types';
import { BinancePrivateWebsocket } from './binance.ws-private';
import { BinancePublicWebsocket } from './binance.ws-public';

// TODO: translate our err messages
export class BinanceExchange extends BaseExchange {
  private hasLoggedBalanceAggregation = false;

  // Helper to detect if availableBalance is 0 but user has stablecoin balances
  detectBalanceIssue(_accountData: any): boolean {
    // This method is deprecated since we now always aggregate from individual assets
    // Keeping it for backwards compatibility but it always returns false
    return false;
  }
  xhr: Axios;
  _primaryXhr: Axios;
  _secondaryXhr: Axios;
  unlimitedXHR: Axios;
  publicXhr: Axios;

  publicWebsocket: BinancePublicWebsocket;
  privateWebsocket: BinancePrivateWebsocket;

  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;
  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    const [xhr, primaryXhr, secondaryXhr] = createApiWithSecondaryKey(opts, this, createAPI, {
      rateLimit: (xhr) =>
        rateLimit(xhr, {
          maxRPS: 3,
        }),
      // secondaryRateLimit: (xhr) =>
      //   rateLimit(xhr, {
      //     maxRPS: 3,
      //   }),
    });

    this.xhr = xhr;
    this._primaryXhr = primaryXhr;
    this._secondaryXhr = secondaryXhr;

    this.publicXhr = createPublicAPI(opts, this);

    this.unlimitedXHR = createAPI(opts, this);

    this.publicWebsocket = new BinancePublicWebsocket(this);
    this.privateWebsocket = new BinancePrivateWebsocket(this);
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
    let balance: Balance | undefined;
    try {
      const { data } = await this._primaryXhr.get(ENDPOINTS.ACCOUNT);
      // /fapi/v2/account reports USD totals at the top level. Good enough for the
      // new-account balance gate; full per-asset aggregation needs live tickers.
      const total = parseFloat(data?.totalWalletBalance);
      if (Number.isFinite(total)) {
        const free = parseFloat(data?.availableBalance) || 0;
        const upnl = parseFloat(data?.totalUnrealizedProfit) || 0;
        balance = { total, free, used: total - free, upnl };
      }
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return { error: err?.response?.data?.msg || err?.message || 'Invalid API key or secret' };
    }

    if (this.options.secondary_key) {
      try {
        await this._secondaryXhr.get(ENDPOINTS.ACCOUNT);
      } catch (err: any) {
        return { error: `Secondary Key: ${err?.response?.data?.msg || err?.message}` };
      }
    }
    return { error: '', balance };
  };

  start = async () => {
    const useExternalPublicData = await this.initExternalPublicData();

    if (!useExternalPublicData) {
      // load initial market data
      // then we can poll for live data
      const markets = await this.fetchMarkets();
      if (this.isDisposed) return;

      this.store.update({
        markets,
        loaded: { ...this.store.loaded, markets: true },
      });

      // load initial tickers data
      // then we use websocket for live data
      const tickers = await this.fetchTickers();
      if (this.isDisposed) return;

      this.log(`Loaded ${Math.min(tickers.length, markets.length)} Binance markets`);

      this.store.update({
        tickers,
        loaded: { ...this.store.loaded, tickers: true },
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

    // fetch current position mode (Hedge/One-way)
    this.store.setSetting('isHedged', await this.fetchPositionMode());

    // start ticking live data
    // balance, tickers, positions
    await this.tick();
    if (this.isDisposed) return;

    this.log(`Ready to trade on Binance`);

    // fetch unfilled orders
    const orders = await this.fetchOrders();
    if (this.isDisposed) return;

    this.log(`Loaded Binance orders`);

    this.store.update({
      orders,
      loaded: { ...this.store.loaded, orders: true },
    });

    this.fetchPositions();
  };

  _startPublicDataOnly = async () => {
    // start ticking live data
    // balance, tickers, positions
    if (this.isDisposed) return;

    this.log(`Ready to watch public data on Binance`);
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

  tick = async (count = 0) => {
    if (!this.isDisposed) {
      try {
        if (count % 16 === 0) {
          const positionsWithRisk = await this.fetchPositions();
          this.store.update({ positions: positionsWithRisk });
          if (this.isDisposed) return;
        }
        const { balance, positions } = await this.fetchPositionsAndBalance();
        if (this.isDisposed) return;

        this.store.update({
          balance,
          positions,
          loaded: {
            ...this.store.loaded,
            balance: true,
            positions: true,
          },
        });
      } catch (err: any) {
        this.emitAndLogError(err?.message);
      }

      if (!this.isDisposed) {
        loop(() => this.tick(count + 1), 5_000);
      }
    }
  };

  fetchMarkets = async () => {
    try {
      const {
        data: { symbols },
      } = await this.publicXhr.get<{ symbols: Array<Record<string, any>> }>(ENDPOINTS.MARKETS);

      const { data } = this.isPublicDataOnly
        ? { data: [] }
        : await this.xhr.get<Array<Record<string, any>>>(ENDPOINTS.LEVERAGE_BRACKET);

      const markets: Market[] = symbols
        .filter(
          (m) =>
            ['PERPETUAL', 'TRADIFI_PERPETUAL'].includes(v(m, 'contractType')) &&
            ['USDT', 'USDC'].includes(v(m, 'marginAsset')),
        )
        .map((m) => {
          const p = m.filters.find((f: any) => v(f, 'filterType') === 'PRICE_FILTER');

          const amt = m.filters.find((f: any) => v(f, 'filterType') === 'LOT_SIZE');

          const mAmt = m.filters.find((f: any) => v(f, 'filterType') === 'MARKET_LOT_SIZE');
          const brackets = data.find((b) => b.symbol === m.symbol)?.brackets;
          const baseAsset = v(m, 'baseAsset');
          const quoteAsset = v(m, 'quoteAsset');
          const marginAsset = v(m, 'marginAsset');

          return {
            id: `${baseAsset}/${quoteAsset}:${marginAsset}`,
            symbol: m.symbol,
            base: baseAsset,
            quote: quoteAsset,
            active: m.status === 'TRADING',
            precision: {
              amount: parseFloat(v(amt, 'stepSize')),
              price: parseFloat(v(p, 'tickSize')),
            },
            limits: {
              amount: {
                min: Math.max(parseFloat(v(amt, 'minQty')), parseFloat(v(mAmt, 'minQty'))),
                max: Math.min(parseFloat(v(amt, 'maxQty')), parseFloat(v(mAmt, 'maxQty'))),
              },
              leverage: {
                min: 1,
                max: v(brackets?.[0], 'initialLeverage') || 1,
              },
            },
          };
        });

      return markets;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return this.store.markets;
    }
  };

  fetchTickers = async () => {
    try {
      const { data: dailys } = await this.publicXhr.get<Array<Record<string, any>>>(ENDPOINTS.TICKERS_24H);

      const { data: books } = await this.publicXhr.get<Array<Record<string, any>>>(ENDPOINTS.TICKERS_BOOK);

      const { data: prices } = await this.publicXhr.get<Array<Record<string, any>>>(ENDPOINTS.TICKERS_PRICE);

      // Use O(1) hashmap lookup instead of O(n) find
      const marketsBySymbol = this.store.marketsBySymbol;
      return books.reduce((acc: Ticker[], book) => {
        const market = marketsBySymbol[book.symbol];

        const daily = dailys.find((d) => d.symbol === book.symbol)!;
        const price = prices.find((p) => p.symbol === book.symbol)!;

        if (!market || !daily || !price) return acc;

        const ticker = {
          id: market.id,
          symbol: market.symbol,
          bid: parseFloat(v(book, 'bidPrice')),
          ask: parseFloat(v(book, 'askPrice')),
          last: parseFloat(v(daily, 'lastPrice')),
          mark: parseFloat(v(price, 'markPrice')),
          index: parseFloat(v(price, 'indexPrice')),
          percentage: parseFloat(v(daily, 'priceChangePercent')),
          fundingRate: parseFloat(v(price, 'lastFundingRate')),
          fundingTime: parseFloat(v(price, 'nextFundingTime')),
          volume: parseFloat(daily.volume),
          quoteVolume: parseFloat(v(daily, 'quoteVolume')),
          openInterest: 0, // Binance doesn't provides all tickers data
        };

        return [...acc, ticker];
      }, []);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return this.store.tickers;
    }
  };

  fetchPositions = async () => {
    // const { positions } = await this.fetchPositionsAndBalance();
    // return positions;
    try {
      const { data } = await this.xhr.get<GET_POSITION_RISK_RESPONSE>(ENDPOINTS.POSITION_RISK);
      const formattedPositions = data.map((p) => {
        // const foundPosition = this.store.positions.find((pos) => pos.symbol === p.symbol);
        const pSide = p.positionSide;
        const contracts = parseFloat(p.positionAmt);
        const side =
          (pSide in POSITION_SIDE && POSITION_SIDE[pSide]) || (contracts > 0 ? PositionSide.Long : PositionSide.Short);
        const positionMode = p.positionSide === 'BOTH' ? PositionMode.OneWay : PositionMode.Hedged;
        const marginType = p.marginType === 'cross' ? MarginType.Cross : MarginType.Isolated;
        // const positionMode = p.positionSide === 'BOTH' ? PositionMode.OneWay : PositionMode.Hedged;
        // const marginType = p.marginType === 'isolated' ? MarginType.Isolated : MarginType.Cross;
        const notional = parseFloat(p.notional);
        const leverage = parseFloat(p.leverage);
        const isolatedMargin = parseFloat(v(p, 'isolatedMargin') || '0');
        const marginSize =
          isolatedMargin > 0 ? isolatedMargin : leverage > 0 ? Math.abs(notional) / leverage : undefined;
        const res: Position = {
          id: createPositionId(p.symbol, side, positionMode),
          symbol: p.symbol,
          side,
          entryPrice: parseFloat(p.entryPrice),
          notional,
          leverage,
          unrealizedPnl: parseFloat(p.unRealizedProfit),
          contracts: Math.abs(contracts),
          liquidationPrice: parseFloat(p.liquidationPrice),
          marginSize,
          marginType,
          positionMode,
          info: p,
        };
        return res;
      });
      return formattedPositions;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return this.store.positions;
    }
  };

  fetchPositionsAndBalance = async () => {
    try {
      const { data } = await this.xhr.get<Record<string, any>>(ENDPOINTS.ACCOUNT);
      //
      // {
      //   "symbol": "FETUSDT",
      //   "initialMargin": "0",
      //   "maintMargin": "0",
      //   "unrealizedProfit": "0.00000000",
      //   "positionInitialMargin": "0",
      //   "openOrderInitialMargin": "0",
      //   "leverage": "20",
      //   "isolated": false,
      //   "entryPrice": "0.0",
      //   "breakEvenPrice": "0.0",
      //   "maxNotional": "25000",
      //   "positionSide": "BOTH",
      //   "positionAmt": "0",
      //   "notional": "0",
      //   "isolatedWallet": "0",
      //   "updateTime": 0,
      //   "bidNotional": "0",
      //   "askNotional": "0"
      // }

      // Define list of stablecoins to aggregate
      const STABLECOINS = ['USDT', 'USDC', 'BUSD', 'FDUSD', 'BFUSD', 'LDUSDT', 'BNFCR'];

      // Initialize balance values
      let total = 0;
      let free = 0;
      let upnl = 0;
      let used = 0;

      // Track details for logging
      const balanceDetails: {
        asset: string;
        available: number;
        wallet: number;
        initial: number;
        unrealized: number;
        usdValue?: number;
        price?: number;
      }[] = [];

      try {
        for (const asset of data?.assets || []) {
          const assetWallet = parseFloat(asset.walletBalance || '0');
          const assetAvailable = parseFloat(asset.availableBalance || '0');
          const assetInitial = parseFloat(asset.initialMargin || '0');
          const assetUnrealized = parseFloat(asset.unrealizedProfit || '0');

          // Skip assets with no balance
          if (assetWallet === 0 && assetAvailable === 0 && assetInitial === 0 && Math.abs(assetUnrealized) === 0) {
            continue;
          }

          if (STABLECOINS.includes(asset.asset)) {
            // Direct aggregation for stablecoins (1:1 USD value)
            total += assetWallet;
            free += assetAvailable;
            used += assetInitial;
            upnl += assetUnrealized;

            balanceDetails.push({
              asset: asset.asset,
              available: assetAvailable,
              wallet: assetWallet,
              initial: assetInitial,
              unrealized: assetUnrealized,
              usdValue: assetWallet,
            });
          } else {
            // For non-stablecoins, only look for price if there's actual balance
            let usdPrice = 0;
            let marketSymbol = '';

            // Only search for markets if the asset has some balance
            if (assetWallet > 0 || assetAvailable > 0 || assetInitial > 0 || Math.abs(assetUnrealized) > 0) {
              // First try to find a direct USDT pair
              const directMarket = this.store.markets.find(
                (m) => m.base === asset.asset && (m.quote === 'USDT' || m.quote === 'USDC' || m.quote === 'BUSD'),
              );

              if (directMarket) {
                marketSymbol = directMarket.symbol;
                // Use O(1) hashmap lookup instead of O(n) find
                const ticker = this.store.tickersBySymbol[marketSymbol];
                if (ticker && ticker.last > 0) {
                  usdPrice = ticker.last;
                }
              }

              // If no direct pair, try common routing pairs (e.g., XXXBTC -> BTCUSDT)
              if (usdPrice === 0 && asset.asset !== 'BTC') {
                const btcMarket = this.store.markets.find((m) => m.base === asset.asset && m.quote === 'BTC');
                if (btcMarket) {
                  // Use O(1) hashmap lookup instead of O(n) find
                  const btcTicker = this.store.tickersBySymbol[btcMarket.symbol];
                  const btcUsdtTicker = this.store.tickersBySymbol['BTCUSDT'] || this.store.tickersBySymbol['BTCUSDC'];

                  if (btcTicker && btcTicker.last > 0 && btcUsdtTicker && btcUsdtTicker.last > 0) {
                    usdPrice = btcTicker.last * btcUsdtTicker.last;
                    marketSymbol = `${btcMarket.symbol} -> ${btcUsdtTicker.symbol}`;
                  }
                }
              }
            }

            // If we have balance and found a price, add USD value to totals
            if (usdPrice > 0) {
              const walletUsdValue = assetWallet * usdPrice;
              const availableUsdValue = assetAvailable * usdPrice;
              const initialUsdValue = assetInitial * usdPrice;
              const unrealizedUsdValue = assetUnrealized * usdPrice;

              total += walletUsdValue;
              free += availableUsdValue;
              used += initialUsdValue;
              upnl += unrealizedUsdValue;

              balanceDetails.push({
                asset: asset.asset,
                available: assetAvailable,
                wallet: assetWallet,
                initial: assetInitial,
                unrealized: assetUnrealized,
                usdValue: walletUsdValue,
                price: usdPrice,
              });
            } else if (assetWallet > 0 || assetAvailable > 0 || assetInitial > 0 || Math.abs(assetUnrealized) > 0) {
              // Has balance but no price found
              balanceDetails.push({
                asset: asset.asset,
                available: assetAvailable,
                wallet: assetWallet,
                initial: assetInitial,
                unrealized: assetUnrealized,
                usdValue: 0,
                price: 0,
              });
            }
            // If no balance at all, we skip it entirely (already handled by the continue statement above)
          }
        }

        // Log balance aggregation details periodically
        if (!this.hasLoggedBalanceAggregation && balanceDetails.length > 0) {
          this.hasLoggedBalanceAggregation = true;

          const stablecoins = balanceDetails.filter((d) => STABLECOINS.includes(d.asset) && d.wallet > 0);
          const nonStablecoins = balanceDetails.filter((d) => !STABLECOINS.includes(d.asset) && d.wallet > 0);

          let logMessage = `Balance aggregation:\n`;

          if (stablecoins.length > 0) {
            logMessage += `  Stablecoins: ${stablecoins.map((d) => `${d.asset}(${d.wallet})`).join(', ')}\n`;
          }

          if (nonStablecoins.length > 0) {
            logMessage += `  Non-stablecoins: ${nonStablecoins
              .map((d) => `${d.asset}(${d.wallet} @ $${d.price?.toFixed(2)} = $${d.usdValue?.toFixed(2)})`)
              .join(', ')}\n`;
          }

          logMessage += `  Total: $${total.toFixed(2)}, Free: $${free.toFixed(2)}, Used: $${used.toFixed(2)}, UPnL: $${upnl.toFixed(2)}`;

          this.log(logMessage, LogSeverity.Info);
        }
      } catch (e) {
        this.log(`Error aggregating balances: ${e}`, LogSeverity.Error);
        // Fallback to top-level values if available
        total = parseFloat(data.totalWalletBalance || '0');
        free = parseFloat(data.availableBalance || '0');
        upnl = parseFloat(data.totalUnrealizedProfit || '0');
        used = parseFloat(data.totalInitialMargin || '0');
      }

      const { positions: _positions, ...rest } = data;
      const balance = { total, free, used, upnl, info: rest };

      // We need to filter out positions that corresponds to
      // markets that are not supported by safe-cex

      const supportedPositions =
        data.positions?.filter((p: any) => this.store.markets.some((m) => m.symbol === p.symbol)) || [];

      const positions: Position[] = supportedPositions.map((p: any): Position => {
        const entryPrice = parseFloat(v(p, 'entryPrice'));
        const contracts = parseFloat(v(p, 'positionAmt'));
        const upnl = parseFloat(v(p, 'unrealizedProfit'));
        const pSide = v(p, 'positionSide');

        // If account is not on hedge mode,
        // we need to define the side of the position with the contracts amount
        const side =
          (pSide in POSITION_SIDE && POSITION_SIDE[pSide]) || (contracts > 0 ? PositionSide.Long : PositionSide.Short);
        const positionMode = p.positionSide === 'BOTH' ? PositionMode.OneWay : PositionMode.Hedged;
        const marginType = p.isolated ? MarginType.Isolated : MarginType.Cross;
        const id = createPositionId(p.symbol, side, positionMode);
        const foundPosition = this.store.positions.find((pos) => pos.id === id);
        const notional = Math.abs(contracts * entryPrice + upnl);
        const leverage = parseFloat(p.leverage);
        // /fapi/v2/account returns positionInitialMargin (and initialMargin including
        // open-order margin). For isolated positions we can also use isolatedWallet.
        // Fall back to notional / leverage so cross positions still get a meaningful
        // value when the API returns 0.
        const apiMargin = parseFloat(v(p, 'positionInitialMargin') || v(p, 'initialMargin') || '0');
        const marginSize = apiMargin > 0 ? apiMargin : leverage > 0 ? notional / leverage : undefined;

        return {
          liquidationPrice: parseFloat(v(p, 'liquidationPrice') || '0'),
          ...foundPosition,
          id,
          symbol: p.symbol,
          side,
          entryPrice,
          notional, // this fixes wrong calculation for shorts notional
          leverage,
          unrealizedPnl: upnl,
          contracts: Math.abs(contracts),
          marginSize,
          marginType,
          positionMode,
          info: {
            ...foundPosition?.info,
            ...p,
          },
        };
      });

      return { balance, positions };
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      return { balance: this.store.balance, positions: this.store.positions };
    }
  };

  fetchOrders = async () => {
    try {
      // Fetch both regular orders and algo orders in parallel
      const [regularResponse, algoResponse] = await Promise.all([
        this.xhr.get<Array<Record<string, any>>>(ENDPOINTS.OPEN_ORDERS),
        this.xhr.get<Array<Record<string, any>>>(ENDPOINTS.OPEN_ALGO_ORDERS, {
          params: { algoType: 'CONDITIONAL' },
        }),
      ]);

      const regularData = regularResponse.data;
      const algoData = algoResponse.data;

      // Log the response if it's not an array
      if (!Array.isArray(regularData)) {
        console.error('[Binance] fetchOrders received non-array response:', regularData);
        this.log(`fetchOrders received non-array response: ${JSON.stringify(regularData)}`, LogSeverity.Error);
      }

      // Ensure data is an array (in case API returns something else when no orders)
      const ordersData = Array.isArray(regularData) ? regularData : [];

      const orders: Order[] = ordersData.map((o) => {
        return {
          id: v(o, 'orderId')?.toString(),
          status: OrderStatus.Open,
          symbol: o.symbol,
          type: ORDER_TYPE[o.type],
          side: ORDER_SIDE[o.side],
          price: parseFloat(o.price) || parseFloat(v(o, 'stopPrice')),
          amount: parseFloat(v(o, 'origQty')),
          reduceOnly: v(o, 'reduceOnly') || false,
          close: v(o, 'closePosition') || false,
          filled: parseFloat(v(o, 'executedQty')),
          lastUpdate: virtualClock.getCurrentTimeMs(),
          remaining: subtract(v(o, 'origQty'), v(o, 'executedQty')),
          info: o,
          ...(ORDER_TYPE[o.type] === OrderType.TakeProfit && {
            tpTriggerType: ORDER_TRIGGER_TYPE[v(o, 'workingType')],
          }),
          ...(ORDER_TYPE[o.type] === OrderType.StopLoss && { slTriggerType: ORDER_TRIGGER_TYPE[v(o, 'workingType')] }),
        };
      });

      // Parse algo orders (stop/TP/trailing orders) - response is a direct array
      const algoOrdersData = Array.isArray(algoData) ? algoData : [];
      const algoOrders: Order[] = algoOrdersData.map((o: Record<string, any>) => {
        const orderType = ORDER_TYPE[v(o, 'orderType')];
        // Always use algoId (Binance's order ID) - clientAlgoId from website-placed orders is unusable
        const algoId = v(o, 'algoId');
        return {
          id: algoId?.toString(),
          status: OrderStatus.Open,
          symbol: o.symbol,
          type: orderType,
          side: ORDER_SIDE[o.side],
          price: parseFloat(v(o, 'triggerPrice')) || 0,
          amount: parseFloat(v(o, 'quantity')) || 0,
          reduceOnly: v(o, 'reduceOnly') || false,
          close: v(o, 'closePosition') || false,
          filled: 0,
          lastUpdate: virtualClock.getCurrentTimeMs(),
          remaining: parseFloat(v(o, 'quantity')) || 0,
          // Explicitly include algoId for consistent detection in cancelOrders
          // (matches WebSocket handler format for algo order identification)
          info: { algoId, ...o },
          ...(orderType === OrderType.TakeProfit && {
            tpTriggerType: ORDER_TRIGGER_TYPE[v(o, 'workingType')],
          }),
          ...(orderType === OrderType.StopLoss && {
            slTriggerType: ORDER_TRIGGER_TYPE[v(o, 'workingType')],
          }),
        };
      });

      return [...orders, ...algoOrders];
    } catch (error: any) {
      console.error('[Binance] fetchOrders error:', error);
      console.error('[Binance] fetchOrders error response:', error?.response?.data);
      this.log(`fetchOrders error: ${error?.message || error}`, LogSeverity.Error);

      // Return empty array on error
      return [];
    }
  };

  fetchHistoricalTrades = async (opts: HistoricalTradeOptions) => {
    const { data } = await this.xhr.get<any[]>(ENDPOINTS.USER_TRADES, {
      params: {
        symbol: opts.symbol,
        limit: 1000,
      },
    });

    // Ensure data is an array before mapping
    const trades = Array.isArray(data) ? data : [];

    return trades.map((entry) => ({
      symbol: entry.symbol,
      orderId: entry.orderId?.toString(),
      price: +entry.price,
      qty: +entry.qty,
      quoteQty: entry.quoteQty,
      realizedPnl: +entry.realizedPnl,
      positionSide: entry.positionSide.toLocaleLowerCase(),
      side: entry.side.toLocaleLowerCase(),
      buyer: entry.buyer,
      maker: entry.maker,
      commission: +entry.commission,
      commissionAsset: entry.commissionAsset,
      time: +entry.time,
    }));
  };

  fetchExecutions = async (opts: ExecutionsOptions): Promise<Execution[]> => {
    // Return empty array if in public data only mode
    if (this.isPublicDataOnly) {
      return [];
    }

    const { symbol, orderId, from, to, limit = 1000 } = opts;

    if (!symbol) {
      // Binance requires symbol for userTrades queries
      return [];
    }

    try {
      const params: Record<string, any> = {
        symbol,
        limit: Math.min(limit, 1000),
      };

      if (orderId) {
        params.orderId = orderId;
      }
      if (from) {
        params.startTime = from;
      }
      if (to) {
        params.endTime = to;
      }

      const { data } = await this.xhr.get<any[]>(ENDPOINTS.USER_TRADES, { params });

      // Ensure data is an array before mapping
      const trades = Array.isArray(data) ? data : [];

      return trades.map((entry) => ({
        timestamp: Math.floor(entry.time / 1000), // Convert from milliseconds to seconds
        symbol: entry.symbol,
        orderId: entry.orderId?.toString(),
        price: parseFloat(entry.price),
        qty: parseFloat(entry.qty),
        side: ORDER_SIDE[entry.side] || OrderSide.Buy,
        commission: parseFloat(entry.commission || '0'),
        commissionAsset: entry.commissionAsset,
      }));
    } catch (error: any) {
      this.log(`Error fetching executions: ${error.message}`, LogSeverity.Error);
      return [];
    }
  };

  /**
   * Fetch income history from Binance Futures.
   * Returns realized PnL, funding fees, commissions, etc.
   *
   * Rate limit: 30 requests per minute
   * Max: 1000 records per request, up to 7 days per request
   *
   * @param opts - Options for filtering income history
   * @returns Array of normalized income records
   */
  fetchIncomeHistory = async (opts?: IncomeHistoryOptions): Promise<IncomeRecord[]> => {
    // Return empty array if in public data only mode
    if (this.isPublicDataOnly) {
      return [];
    }

    try {
      const params: Record<string, any> = {
        limit: Math.min(opts?.limit || 1000, 1000),
      };

      if (opts?.symbol) {
        params.symbol = opts.symbol;
      }
      if (opts?.incomeType) {
        params.incomeType = opts.incomeType;
      }
      if (opts?.startTime) {
        params.startTime = opts.startTime;
      }
      if (opts?.endTime) {
        params.endTime = opts.endTime;
      }

      const { data } = await this.xhr.get<BinanceIncomeRecord[]>(ENDPOINTS.INCOME, { params });

      // Ensure data is an array before mapping
      const records = Array.isArray(data) ? data : [];

      return records.map((record) => ({
        symbol: record.symbol,
        incomeType: record.incomeType,
        income: parseFloat(record.income),
        asset: record.asset,
        timestamp: record.time,
        tranId: record.tranId,
        tradeId: record.tradeId,
      }));
    } catch (error: any) {
      this.log(`Error fetching income history: ${error.message}`, LogSeverity.Error);
      return [];
    }
  };

  /**
   * Fetch income history for a date range, handling pagination and 7-day limit.
   * Binance limits income queries to 7 days per request, so we chunk the range.
   *
   * @param startTime - Start timestamp in ms
   * @param endTime - End timestamp in ms
   * @param incomeType - Optional filter by income type (e.g., 'REALIZED_PNL')
   * @returns Array of all income records in the range
   */
  fetchIncomeHistoryRange = async (
    startTime: number,
    endTime: number,
    incomeType?: string,
  ): Promise<IncomeRecord[]> => {
    const allRecords: IncomeRecord[] = [];
    const SEVEN_DAYS_MS = 7 * 24 * 60 * 60 * 1000;

    let currentStart = startTime;

    while (currentStart < endTime) {
      const currentEnd = Math.min(currentStart + SEVEN_DAYS_MS, endTime);

      const records = await this.fetchIncomeHistory({
        startTime: currentStart,
        endTime: currentEnd,
        incomeType: incomeType as any,
        limit: 1000,
      });

      allRecords.push(...records);

      // If we got 1000 records, there might be more - paginate within this chunk
      if (records.length === 1000) {
        // Use the last record's timestamp as the new start
        const lastTimestamp = records[records.length - 1].timestamp;
        // Fetch remaining records in this chunk
        let moreRecords = await this.fetchIncomeHistory({
          startTime: lastTimestamp + 1,
          endTime: currentEnd,
          incomeType: incomeType as any,
          limit: 1000,
        });

        while (moreRecords.length > 0) {
          allRecords.push(...moreRecords);
          if (moreRecords.length < 1000) break;

          const nextStart = moreRecords[moreRecords.length - 1].timestamp + 1;
          moreRecords = await this.fetchIncomeHistory({
            startTime: nextStart,
            endTime: currentEnd,
            incomeType: incomeType as any,
            limit: 1000,
          });
        }
      }

      currentStart = currentEnd;

      // Add a small delay to respect rate limits (30 req/min = 2 sec between requests)
      await new Promise((resolve) => setTimeout(resolve, 100));
    }

    return allRecords;
  };

  fetchOHLCV = async (opts: OHLCVOptions) => {
    const interval = opts.interval;
    const limit = Math.min(opts.limit || 500, 1500);
    const [, amount, unit] = opts.interval.split(/(\d+)/);

    const end = opts.to ? dayjs(opts.to) : dayjs();
    const start =
      !opts.limit && opts.from ? dayjs(opts.from) : end.subtract(parseFloat(amount) * limit, unit as ManipulateType);

    const { data } = await this.publicXhr.get<any[][]>(ENDPOINTS.KLINE, {
      params: {
        symbol: opts.symbol,
        interval,
        startTime: start.valueOf(),
        endTime: end.valueOf(),
        ...(opts.limit
          ? {
              limit,
            }
          : {}),
      },
    });

    if (!Array.isArray(data)) {
      return [];
    }

    const candles: Candle[] = data.map(([time, open, high, low, close, volume]) => {
      return {
        timestamp: time / 1000,
        open: parseFloat(open),
        high: parseFloat(high),
        low: parseFloat(low),
        close: parseFloat(close),
        volume: parseFloat(volume),
      };
    });

    return candles;
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    if (this.externalPublicDataProvider) {
      return this.externalPublicDataProvider.subscribeCandle(opts, callback);
    }

    let disposed = false;
    const unsubscribe = this.publicWebsocket.listenOHLCV(opts, callback);

    // Binance kline WS normally updates every 250ms, but the first pushed
    // frame can lag after a fresh subscribe. Seed the realtime path so the
    // chart current candle/countdown is live immediately after history loads.
    void this.fetchOHLCV({ ...opts, limit: 1 })
      .then((candles) => {
        if (disposed || this.isDisposed) return;
        const latest = candles[candles.length - 1];
        if (latest) {
          callback(latest);
        }
      })
      .catch((err: any) => {
        this.log(`Unable to seed Binance live candle: ${err?.message || err}`, LogSeverity.Debug);
      });

    return () => {
      disposed = true;
      unsubscribe();
    };
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    if (this.externalPublicDataProvider) {
      return this.externalPublicDataProvider.subscribeOrderbook(symbol, callback);
    }

    return this.publicWebsocket.listenOrderBook(symbol, callback);
  };

  fetchPublicTrades = async (symbol: string, limit?: number): Promise<PublicTrade[]> => {
    // Binance max limit is 1000, default is 500
    const maxLimit = 1000;
    const defaultLimit = 1000;
    const actualLimit = limit ? Math.min(limit, maxLimit) : defaultLimit;

    const { data } = await this.publicXhr.get(ENDPOINTS.TRADES, {
      params: {
        symbol,
        limit: actualLimit,
      },
    });

    // publicXhr skips the negative-`code` rejection interceptor that
    // `xhr` has (see createPublicAPI in binance.api.ts), so an unknown
    // symbol returns a 200 with `{ code, msg }` instead of throwing.
    if (!Array.isArray(data)) {
      return [];
    }

    return data.map(
      (trade: any): PublicTrade => ({
        id: trade.id.toString(),
        symbol,
        price: parseFloat(trade.price),
        qty: parseFloat(trade.qty),
        quoteQty: parseFloat(trade.quoteQty),
        timestamp: trade.time,
        side: trade.isBuyerMaker ? OrderSide.Sell : OrderSide.Buy, // isBuyerMaker means sell order was in the book (taker was buyer)
      }),
    );
  };

  fetchPositionMode = async () => {
    const { data } = await this.xhr.get(ENDPOINTS.HEDGE_MODE);
    return data.dualSidePosition === true;
  };

  changePositionMode = async (hedged: boolean) => {
    if (this.store.positions.filter((p) => p.contracts > 0).length > 0) {
      this.emitAndLogError('Please close all positions before switching position mode');
      return;
    }

    try {
      await this.xhr.post(ENDPOINTS.HEDGE_MODE, {
        dualSidePosition: hedged ? 'true' : 'false',
      });
      this.store.setSetting('isHedged', hedged);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
    }
  };

  changeMarginType = async (isolated: boolean, symbol?: string) => {
    if (!symbol) {
      this.emitAndLogError('Symbol is required');
      throw new Error('Symbol is required');
    }

    try {
      await this.xhr.post(ENDPOINTS.SET_MARGIN_TYPE, {
        symbol,
        marginType: isolated ? 'ISOLATED' : 'CROSSED',
      });
    } catch (e: any) {
      this.emitAndLogError(e?.response?.data?.msg || e?.message);
      throw new Error(e?.response?.data?.msg || e?.message);
    }
  };

  setLeverage = async (symbol: string, inputLeverage: number) => {
    // Use O(1) hashmap lookup instead of O(n) find
    const market = this.store.marketsBySymbol[symbol];
    const position = this.store.positions.find((p) => p.symbol === symbol);

    if (!market) throw new Error(`Market ${symbol} not found`);
    if (!position) throw new Error(`Position ${symbol} not found`);

    const leverage = Math.min(Math.max(inputLeverage, market.limits.leverage.min), market.limits.leverage.max);

    if (position.leverage !== leverage) {
      try {
        await this.xhr.post(ENDPOINTS.SET_LEVERAGE, {
          symbol,
          leverage,
        });

        this.store.updatePositions([
          [{ id: createPositionId(symbol, PositionSide.Long) }, { leverage }],
          [{ id: createPositionId(symbol, PositionSide.Short) }, { leverage }],
        ]);
      } catch (err: any) {
        this.emitAndLogError(err?.response?.data?.msg || err?.message);
      }
    }
  };

  cancelOrders = async (orders: Order[]) => {
    try {
      // Separate regular orders from algo orders
      // For stop/TP/trailing orders, always try algo endpoint first since Tealstreet places them via algo API
      // The order.id IS the algoId for algo orders (set in fetchOrders and WebSocket handler)
      // Regular orders (limit/market) go to the regular endpoint
      const algoOrders = orders.filter((o) => this.isAlgoOrderType(o.type));
      const regularOrders = orders.filter((o) => !this.isAlgoOrderType(o.type));

      // Cancel regular orders (supports batching)
      if (regularOrders.length > 0) {
        const groupedBySymbol = groupBy(regularOrders, 'symbol');
        const requests = Object.entries(groupedBySymbol).map(([symbol, symbolOrders]) => ({
          symbol,
          orders: symbolOrders,
          orderIdList: symbolOrders.map((o) => o.id),
        }));

        await forEachSeries(requests, async (request) => {
          try {
            if (request.orderIdList.length === 1) {
              await this.xhr.delete(ENDPOINTS.ORDER, {
                params: {
                  symbol: request.symbol,
                  orderId: request.orderIdList[0],
                },
              });
            } else {
              const lots = chunk(request.orderIdList, 10);
              await forEachSeries(lots, async (lot) => {
                // Binance expects LIST<LONG> format: [123,456] without string quotes
                // Construct manually to avoid precision loss from parseInt on large IDs
                const orderIdListStr = `[${lot.join(',')}]`;
                await this.xhr.delete(ENDPOINTS.BATCH_ORDERS, {
                  params: {
                    symbol: request.symbol,
                    orderIdList: orderIdListStr,
                  },
                });
              });
            }

            this.store.updateOrdersStatus(
              request.orders.map((o) => ({ id: o.id })),
              OrderStatus.Canceled,
            );
          } catch (err: any) {
            // -2011 "Unknown order sent" means order is already gone - treat as success
            if (err?.code === -2011) {
              this.store.updateOrdersStatus(
                request.orders.map((o) => ({ id: o.id })),
                OrderStatus.Canceled,
              );
              return;
            }
            // Re-throw other errors
            throw err;
          }
        });
      }

      // Cancel algo orders (must be cancelled one at a time)
      if (algoOrders.length > 0) {
        for (const order of algoOrders) {
          try {
            // order.id is the algoId - use camelCase 'algoId' as required by Binance API
            const algoId = order.id;

            await this.xhr.delete(ENDPOINTS.ALGO_ORDER, {
              params: { algoId },
            });

            this.store.updateOrderStatus({ id: order.id }, OrderStatus.Canceled);
          } catch (err: any) {
            // -2011 "Unknown order sent" means order is already gone - treat as success
            if (err?.code === -2011) {
              this.store.updateOrderStatus({ id: order.id }, OrderStatus.Canceled);
              continue;
            }
            this.emitAndLogError(err?.response?.data?.msg || err?.message);
            throw err;
          }
        }
      }
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      throw err;
    }
  };

  cancelSymbolOrders = async (symbol: string) => {
    try {
      await this.xhr.delete(ENDPOINTS.CANCEL_SYMBOL_ORDERS, {
        params: { symbol },
      });

      this.store.updateOrdersStatus(
        this.store.orders.filter((o) => o.symbol === symbol),
        OrderStatus.Canceled,
      );
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      throw err;
    }
  };

  updateOrder = async ({ order, update }: UpdateOrderOpts) => {
    const newOrder: Writable<PlaceOrderOpts> = {
      symbol: order.symbol,
      type: order.type,
      side: order.side,
      price: order.price,
      amount: order.amount,
      reduceOnly: order.reduceOnly || false,
      close: order.close || false,
    };

    if ('tpTriggerType' in order) newOrder.tpTriggerType = order.tpTriggerType;
    if ('slTriggerType' in order) newOrder.slTriggerType = order.slTriggerType;

    if ('price' in update) newOrder.price = update.price;
    if ('amount' in update) newOrder.amount = update.amount;

    await this.cancelOrders([order]);
    return await this.placeOrder(newOrder);
  };

  placeOrder = async (opts: PlaceOrderOpts) => {
    // Cancel existing closePosition orders if placing a new closePosition order
    // Binance only allows ONE closePosition order per direction per symbol
    await this.cancelExistingClosePositionOrders(opts);

    const { regular, algo } = this.formatCreateOrder(opts);
    const orderIds: string[] = [];

    // Place regular orders (limit/market)
    if (regular.length > 0) {
      const regularIds = await this.placeRegularOrderBatch(regular);
      orderIds.push(...regularIds);
    }

    // Place algo orders (stop/TP/trailing) - must be placed one at a time
    if (algo.length > 0) {
      const algoIds = await this.placeAlgoOrders(algo);
      orderIds.push(...algoIds);
    }

    return orderIds;
  };

  // Cancel existing closePosition orders that would conflict with a new closePosition order
  // Binance only allows one closePosition order per direction per symbol (uses GTE time-in-force internally)
  private cancelExistingClosePositionOrders = async (opts: PlaceOrderOpts) => {
    const isStopOrTP = opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit;
    if (!isStopOrTP) return;

    // Check if this new order wants to be a closePosition order
    // Use O(1) hashmap lookup instead of O(n) find
    const market = this.store.marketsBySymbol[opts.symbol];
    if (!market) return;

    const pAmount = market.precision.amount;
    const amount = adjust(opts.amount, pAmount);
    const wantsClosePosition = opts.close || !amount;

    if (!wantsClosePosition) return;

    // Use the local store — `cancelOrders` already silently absorbs -2011 ("Unknown order sent"),
    // so a stale row triggers a harmless no-op cancel instead of a REST roundtrip on every placement.
    // The previous fetchOrders here was a major contributor to Binance's 2400 RPM IP rate limit
    // (each call = 2 parallel REST requests, amplified per placement across all accounts on this IP).
    const existingCloseOrders = this.store.orders.filter((o) => {
      return (
        o.symbol === opts.symbol &&
        o.close === true &&
        !o.amount && // Only cancel infinite closePosition orders (no size)
        o.type === opts.type &&
        o.side === opts.side
      );
    });

    if (existingCloseOrders.length > 0) {
      this.log(
        `Canceling ${existingCloseOrders.length} existing closePosition order(s) before placing new one`,
        LogSeverity.Info,
      );
      await this.cancelOrders(existingCloseOrders);
    }
  };

  placeOrders = async (orders: PlaceOrderOpts[]) => {
    const allRegular: any[] = [];
    const allAlgo: any[] = [];

    for (const o of orders) {
      const { regular, algo } = this.formatCreateOrder(o);
      allRegular.push(...regular);
      allAlgo.push(...algo);
    }

    const orderIds: string[] = [];

    // Place regular orders in batches
    if (allRegular.length > 0) {
      const regularIds = await this.placeRegularOrderBatch(allRegular);
      orderIds.push(...regularIds);
    }

    // Place algo orders one at a time
    if (allAlgo.length > 0) {
      const algoIds = await this.placeAlgoOrders(allAlgo);
      orderIds.push(...algoIds);
    }

    return orderIds;
  };

  // Check if order type requires the Algo Order API (as of 2025-12-09)
  private isAlgoOrderType = (type: OrderType): boolean => {
    return type === OrderType.StopLoss || type === OrderType.TakeProfit || type === OrderType.TrailingStopLoss;
  };

  private isPostOnlyRejectionError = (err: any): boolean => {
    const errorCode = err?.response?.data?.code ?? err?.code;
    if (errorCode === -5022) {
      return true;
    }

    const errorMessage = String(err?.response?.data?.msg || err?.message || '').toLowerCase();
    return (
      errorMessage.includes('post only order will be rejected') ||
      errorMessage.includes('could not be executed as maker')
    );
  };

  private formatCreateOrder = (opts: PlaceOrderOpts): { regular: any[]; algo: any[] } => {
    // Use O(1) hashmap lookup instead of O(n) find
    const market = this.store.marketsBySymbol[opts.symbol];

    if (!market) {
      throw new Error(`Market ${opts.symbol} not found`);
    }

    const regularPayloads: Array<Record<string, any>> = [];
    const algoPayloads: Array<Record<string, any>> = [];

    // Handle trailing stop loss orders (always algo)
    if (opts.type === OrderType.TrailingStopLoss) {
      const trailingPayloads = this.formatCreateTrailingStopLossOrder(opts);
      algoPayloads.push(...trailingPayloads);
      return { regular: regularPayloads, algo: algoPayloads };
    }

    const isStopOrTP = opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit;

    const pSide = this.getOrderPositionSide(opts);

    const maxSize = market.limits.amount.max;
    const pPrice = market.precision.price;

    const pAmount = market.precision.amount;
    const amount = adjust(opts.amount, pAmount);

    // We use price only for limit orders
    // Market order should not define price
    const price = opts.price && opts.type !== OrderType.Market ? adjust(opts.price, pPrice) : undefined;

    const reduceOnly = !this.store.options.isHedged && opts.reduceOnly;
    const timeInForce = opts.timeInForce
      ? inverseObj(TIME_IN_FORCE)[opts.timeInForce]
      : inverseObj(TIME_IN_FORCE)[OrderTimeInForce.GoodTillCancel];

    // Determine if this should be a closePosition order
    // closePosition requires an existing position (Binance uses GTE time-in-force internally)
    const wantsClosePosition = isStopOrTP && (opts.close || !amount);

    // Check if position exists for closePosition orders
    const oppositePositionSide = opts.side === OrderSide.Buy ? PositionSide.Short : PositionSide.Long;
    const existingPosition = this.store.positions.find(
      (p) => p.symbol === opts.symbol && p.side === oppositePositionSide && Math.abs(p.contracts) > 0,
    );

    // Use closePosition only if requested AND position exists
    const useClosePosition = wantsClosePosition && existingPosition;

    // Build the main order
    if (isStopOrTP) {
      // Algo order format for stop/TP orders
      // closePosition: closes full position when triggered (requires existing position)
      // quantity + reduceOnly: closes specified amount (works without position for bracket orders)
      const algoReq = omitUndefined({
        algoType: 'CONDITIONAL',
        symbol: opts.symbol,
        positionSide: pSide,
        side: inverseObj(ORDER_SIDE)[opts.side],
        type: inverseObj(ORDER_TYPE)[opts.type],
        // If using closePosition, don't send quantity; otherwise send quantity
        quantity: useClosePosition ? undefined : amount ? `${amount}` : undefined,
        triggerPrice: price ? `${price}` : undefined,
        // closePosition and reduceOnly are mutually exclusive
        closePosition: useClosePosition ? 'true' : undefined,
        reduceOnly: useClosePosition ? undefined : reduceOnly ? 'true' : undefined,
        workingType:
          opts.type === OrderType.TakeProfit
            ? opts.tpTriggerType === OrderStopTriggerType.MarkPrice
              ? 'MARK_PRICE'
              : 'CONTRACT_PRICE'
            : opts.slTriggerType === OrderStopTriggerType.LastPrice
              ? 'CONTRACT_PRICE'
              : 'MARK_PRICE',
        clientAlgoId: 'x-s3vFFRQp' + uuid(),
      });

      // If closePosition requested but no position exists and no amount provided, error
      if (wantsClosePosition && !existingPosition && !amount) {
        const errMsg = 'Cannot place full stop order without existing position or specified amount';
        this.emitAndLogError(errMsg);
        throw new Error(errMsg);
      }

      algoPayloads.push(algoReq);
    } else {
      // Regular order format for limit/market orders
      const req = omitUndefined({
        symbol: opts.symbol,
        positionSide: pSide,
        side: inverseObj(ORDER_SIDE)[opts.side],
        type: inverseObj(ORDER_TYPE)[opts.type],
        quantity: amount ? `${amount}` : undefined,
        price: price ? `${price}` : undefined,
        timeInForce: opts.type === OrderType.Limit ? timeInForce : undefined,
        reduceOnly: reduceOnly ? 'true' : undefined,
      });

      const lots = amount > maxSize ? Math.ceil(amount / maxSize) : 1;
      const rest = amount > maxSize ? adjust(amount % maxSize, pAmount) : 0;

      const lotSize = adjust((amount - rest) / lots, pAmount);

      const payloads: Array<Record<string, any>> = times(lots, () => ({
        ...req,
        quantity: `${lotSize}`,
      }));

      if (rest) {
        payloads.push({ ...req, quantity: `${rest}` });
      }

      // Set orderId for each regular order
      for (const payload of payloads) {
        payload.newClientOrderId = 'x-s3vFFRQp' + uuid();
      }

      regularPayloads.push(...payloads);
    }

    // Handle attached stop loss (always algo order)
    // For bracket orders, position doesn't exist yet so we use quantity + reduceOnly
    // instead of closePosition (which requires existing position)
    if (opts.stopLoss) {
      const slPSide = this.getOrderPositionSide({
        ...opts,
        side: opts.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy,
        type: OrderType.StopLoss,
        reduceOnly: true,
      });
      algoPayloads.push(
        omitUndefined({
          algoType: 'CONDITIONAL',
          symbol: opts.symbol,
          positionSide: slPSide,
          side: inverseObj(ORDER_SIDE)[opts.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy],
          type: inverseObj(ORDER_TYPE)[OrderType.StopLoss],
          triggerPrice: `${adjust(opts.stopLoss, pPrice)}`,
          quantity: amount ? `${amount}` : undefined,
          // reduceOnly cannot be sent in Hedge Mode
          reduceOnly: this.store.options.isHedged ? undefined : 'true',
          workingType: opts.slTriggerType === OrderStopTriggerType.LastPrice ? 'CONTRACT_PRICE' : 'MARK_PRICE',
          clientAlgoId: 'x-s3vFFRQp' + uuid(),
        }),
      );
    }

    // Handle attached take profit (always algo order)
    // For bracket orders, position doesn't exist yet so we use quantity + reduceOnly
    if (opts.takeProfit) {
      const tpPSide = this.getOrderPositionSide({
        ...opts,
        side: opts.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy,
        type: OrderType.TakeProfit,
        reduceOnly: true,
      });
      algoPayloads.push(
        omitUndefined({
          algoType: 'CONDITIONAL',
          symbol: opts.symbol,
          positionSide: tpPSide,
          side: inverseObj(ORDER_SIDE)[opts.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy],
          type: inverseObj(ORDER_TYPE)[OrderType.TakeProfit],
          triggerPrice: `${adjust(opts.takeProfit, pPrice)}`,
          quantity: amount ? `${amount}` : undefined,
          // reduceOnly cannot be sent in Hedge Mode
          reduceOnly: this.store.options.isHedged ? undefined : 'true',
          workingType: opts.tpTriggerType === OrderStopTriggerType.MarkPrice ? 'MARK_PRICE' : 'CONTRACT_PRICE',
          clientAlgoId: 'x-s3vFFRQp' + uuid(),
        }),
      );
    }

    return { regular: regularPayloads, algo: algoPayloads };
  };

  private formatCreateTrailingStopLossOrder = (opts: PlaceOrderOpts) => {
    // Use O(1) hashmap lookup instead of O(n) find
    const market = this.store.marketsBySymbol[opts.symbol];
    const ticker = this.store.tickersBySymbol[opts.symbol];

    const pSide = opts.side === OrderSide.Buy ? PositionSide.Short : PositionSide.Long;

    const position = this.store.positions.find((p) => p.symbol === opts.symbol && p.side === pSide);

    if (!market) throw new Error(`Market ${opts.symbol} not found`);
    if (!ticker) throw new Error(`Ticker ${opts.symbol} not found`);

    if (!position) {
      throw new Error(`Position ${opts.symbol} and side ${pSide} not found`);
    }

    const priceDistance = adjust(
      Math.max(ticker.last, opts.price!) - Math.min(ticker.last, opts.price!),
      market.precision.price,
    );

    const distancePercentage = Math.round(((priceDistance * 100) / ticker.last) * 10) / 10;

    // Use Algo Order format for trailing stop (as of 2025-12-09)
    const payload = {
      algoType: 'CONDITIONAL',
      symbol: opts.symbol,
      positionSide: this.getOrderPositionSide(opts),
      side: inverseObj(ORDER_SIDE)[opts.side],
      type: inverseObj(ORDER_TYPE)[OrderType.TrailingStopLoss],
      quantity: `${position.contracts}`,
      callbackRate: `${distancePercentage}`,
      priceProtect: 'TRUE',
      clientAlgoId: 'x-s3vFFRQp' + uuid(),
    };

    return [payload];
  };

  private getOrderPositionSide = (opts: PlaceOrderOpts) => {
    let positionSide = 'BOTH';

    // We need to specify side of the position to interract with
    // if we are in hedged mode on the binance account
    if (this.store.options.isHedged) {
      positionSide = opts.side === OrderSide.Buy ? 'LONG' : 'SHORT';

      if (
        opts.type === OrderType.StopLoss ||
        opts.type === OrderType.TakeProfit ||
        opts.type === OrderType.TrailingStopLoss ||
        opts.reduceOnly
      ) {
        positionSide = positionSide === 'LONG' ? 'SHORT' : 'LONG';
      }
    }

    return positionSide;
  };

  // Place regular orders (limit/market) - supports batching
  private placeRegularOrderBatch = async (payloads: any[]) => {
    const lots = chunk(payloads, 5);
    const orderIds = [] as string[];

    for (const lot of lots) {
      if (lot.length === 1) {
        try {
          const { data } = await this.unlimitedXHR.post(ENDPOINTS.ORDER, lot[0]);
          // Use exchange's orderId (not clientOrderId) to match WebSocket events
          orderIds.push(data.orderId?.toString());
        } catch (err: any) {
          if (!this.isPostOnlyRejectionError(err)) {
            this.emitAndLogError(err?.response?.data?.msg || err?.message);
          }
          throw err;
        }
      }

      if (lot.length > 1) {
        try {
          const { data } = await this.unlimitedXHR.post(ENDPOINTS.BATCH_ORDERS, {
            batchOrders: JSON.stringify(lot),
          });

          data?.forEach?.((o: any) => {
            if (o.code) {
              if (!this.isPostOnlyRejectionError(o)) {
                this.emitAndLogError(o.msg);
              }
            } else {
              // Use exchange's orderId (not clientOrderId) to match WebSocket events
              orderIds.push(o.orderId?.toString());
            }
          });
        } catch (err: any) {
          if (!this.isPostOnlyRejectionError(err)) {
            this.emitAndLogError(err?.response?.data?.msg || err?.message);
          }
          throw err;
        }
      }
    }

    return orderIds;
  };

  // Place algo orders (stop/TP/trailing) - must be placed one at a time (no batch support)
  private placeAlgoOrders = async (payloads: any[]) => {
    const orderIds = [] as string[];

    for (const payload of payloads) {
      try {
        const { data } = await this.unlimitedXHR.post(ENDPOINTS.ALGO_ORDER, payload);
        // Use exchange's algoId (not clientAlgoId) to match WebSocket events
        orderIds.push(data.algoId?.toString());
      } catch (err: any) {
        this.emitAndLogError(err?.response?.data?.msg || err?.message);
        throw err;
      }
    }

    return orderIds;
  };
}
