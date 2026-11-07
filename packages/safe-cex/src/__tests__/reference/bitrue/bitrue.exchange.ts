import type { Axios } from 'axios';
import type { Store } from '../../../store/store.interface';
import type { ExchangeName } from '../../../types';

import rateLimit from 'axios-rate-limit';
import chunk from 'lodash/chunk';
import times from 'lodash/times';
import { mapSeries } from 'p-iteration';

import { sleep } from '@tealstreet/sleep/src/sleep';
import {
  Balance,
  Candle,
  ExchangeOptions,
  Execution,
  ExecutionsOptions,
  HistoricalTrade,
  HistoricalTradeOptions,
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
} from '../../../types';
import { createPositionId } from '../../../utils/createPositionId';
import { inverseObj } from '../../../utils/inverseObj';
import { loop } from '../../../utils/loop';
import { omitUndefined } from '../../../utils/omitUndefined';
import { adjust } from '../../../utils/safe-math';
import { uuid } from '../../../utils/uuid';
import { BaseExchange } from '../../../exchanges/base';
import { patternClientOrderIdRule } from '../../../exchanges/clientOrderId';
import { createAPI } from './bitrue.api';
import {
  ENDPOINTS,
  FETCH_ACCOUNT_RESPONSE,
  FETCH_CONTRACTS_RESPONSE,
  FETCH_LEVERAGE_BRACKET_RESPONSE,
  FETCH_MY_TRADES_RESPONSE,
  FETCH_ORDERS_RESPONSE,
  GRAFANA_ENDPOINTS_EVERY,
  GRAFANA_ENDPOINTS_PERIODIC,
  ORDER_SIDE,
  ORDER_STATUS,
  ORDER_TYPE,
} from './bitrue.types';
import { reverseWsSymbol } from './bitrue.utils';
import { BitruePrivateWebsocket } from './bitrue.ws-private';
import { BitruePublicWebsocket } from './bitrue.ws-public';

// const TEST_SYMBOL = 'E-BTC-USDT';

export class BitrueExchange extends BaseExchange {
  // Bitrue documents clientOrderId as shorter than 32 characters.
  protected readonly clientOrderIdRule = patternClientOrderIdRule('Bitrue', /^.{1,31}$/);

  xhr: Axios;
  unlimitedXHR: Axios;

  publicWebsocket: BitruePublicWebsocket;
  privateWebsocket: BitruePrivateWebsocket;

  prioritySymbols = new Set<string>();
  inProgressOrderFetchId: number = 0;

  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;
  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;

  recentWsRequests: Record<
    string,
    {
      time: number;
      data: Candle[];
    }
  > = {};
  private fastPrivateRefreshTimeoutId: NodeJS.Timeout | undefined;
  private pendingOrderRefreshSymbols = new Set<string>();
  private isFastPrivateRefreshRunning = false;
  // `/fapi/v2/account` is the only documented source for a position's
  // leverage and margin type. Keep the last account snapshot for callers
  // asking about a symbol, but never infer either value by submitting an
  // order.
  private positionMetadataBySymbol = new Map<
    string,
    Pick<Position, 'symbol' | 'side' | 'leverage' | 'positionMode' | 'marginType'>
  >();

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    this.xhr = this.createXhrWithSecondaryKey(opts, createAPI, { rateLimit: (xhr) => rateLimit(xhr, { maxRPS: 5 }) });
    this.unlimitedXHR = createAPI(opts, this);

    this.publicWebsocket = new BitruePublicWebsocket(this);
    this.privateWebsocket = new BitruePrivateWebsocket(this);
    this.emitter.on('fill', this.handleFillForFastRefresh);
  }

  private isSuccessCode = (code: unknown) => code === 0 || code === '0' || code === 200 || code === '200';

  private parseMaxLeverageFromErrorMessage = (message?: string): number | null => {
    if (!message) {
      return null;
    }
    const matched = message.match(/maximum leverage does not exceed\s+(\d+(?:\.\d+)?)/i);
    if (!matched) {
      return null;
    }
    const parsed = Number(matched[1]);
    if (!Number.isFinite(parsed) || parsed <= 0) {
      return null;
    }
    return parsed;
  };

  private setMarketMaxLeverage = (symbol: string, maxLeverage: number) => {
    if (!Number.isFinite(maxLeverage) || maxLeverage <= 0) {
      return;
    }
    const market = this.store.marketsBySymbol[symbol];
    if (!market) {
      return;
    }
    const currentMax = Number(market.limits?.leverage?.max);
    if (Number.isFinite(currentMax) && Math.abs(currentMax - maxLeverage) < 1e-8) {
      return;
    }
    this.store.updateMarket(market, {
      limits: {
        ...market.limits,
        leverage: {
          ...market.limits.leverage,
          max: maxLeverage,
        },
      },
    });
  };

  private parseMaxLeverageFromBracketResponse = (
    response?: FETCH_LEVERAGE_BRACKET_RESPONSE,
    symbol?: string,
  ): number | null => {
    const rawData = response?.data;
    const bracketData = Array.isArray(rawData) ? rawData : rawData ? [rawData] : [];
    if (bracketData.length === 0) {
      return null;
    }
    const targetBracketData = (symbol && bracketData.find((entry) => entry?.contractName === symbol)) || bracketData[0];
    const brackets = Array.isArray(targetBracketData?.brackets) ? targetBracketData.brackets : [];
    const candidates = brackets
      .map((entry) => Number(entry.initialLeverage))
      .filter((value) => Number.isFinite(value) && value > 0);
    if (candidates.length === 0) {
      return null;
    }
    return Math.max(...candidates);
  };

  private fetchAndSyncSymbolMaxLeverage = async (symbol: string): Promise<number | null> => {
    if (!symbol || this.isPublicDataOnly) {
      return null;
    }
    try {
      const { data } = await this.xhr.get<FETCH_LEVERAGE_BRACKET_RESPONSE>(ENDPOINTS.LEVERAGE_BRACKET, {
        params: { contractName: symbol },
      });
      if (!this.isSuccessCode(data?.code)) {
        return null;
      }
      const maxLeverage = this.parseMaxLeverageFromBracketResponse(data, symbol);
      if (maxLeverage === null) {
        return null;
      }
      this.setMarketMaxLeverage(symbol, maxLeverage);
      return maxLeverage;
    } catch {
      return null;
    }
  };

  private normalizeTimestampMs = (ts?: number) => {
    if (ts === undefined || ts === null || !Number.isFinite(ts)) {
      return undefined;
    }
    return ts > 1_000_000_000_000 ? Math.floor(ts) : Math.floor(ts * 1000);
  };

  private normalizeTradeSymbol = (rawSymbol?: unknown) => {
    const value = String(rawSymbol || '').toUpperCase();
    if (!value) {
      return '';
    }
    const normalized = value.includes('_') ? reverseWsSymbol(value) : value;
    return normalized;
  };

  private pickTradeOrderId = (entry: FETCH_MY_TRADES_RESPONSE['data'][number]) => {
    const side = String(entry.side || '').toUpperCase();
    const requireId = (value: unknown, label: string) => {
      if (
        !['string', 'number', 'bigint'].includes(typeof value) ||
        (typeof value === 'number' && !Number.isSafeInteger(value)) ||
        !/^\d+$/.test(String(value)) ||
        !/[1-9]/.test(String(value))
      ) {
        throw new Error(`Malformed Bitrue ${label}`);
      }
      return String(value);
    };
    requireId(entry.tradeId, 'trade ID');
    if (side !== 'BUY' && side !== 'SELL') throw new Error('Malformed Bitrue execution side');
    // The other side identifies a counterparty's order, never our fallback.
    return requireId(side === 'BUY' ? entry.bidId : entry.askId, 'order ID');
  };

  private resolveTradeQty = (
    entry: FETCH_MY_TRADES_RESPONSE['data'][number],
    symbol: string,
    parsedPrice: number,
  ): number => {
    const rawQty = Number(entry.qty ?? 0);
    const rawAmount = Number(entry.amount ?? 0);
    const multiplier = Number(this.store.marketsBySymbol[symbol]?.info?.multiplier || 1);
    const qtyFromAmount = parsedPrice > 0 && rawAmount > 0 ? rawAmount / parsedPrice : 0;

    if (rawQty <= 0) {
      return qtyFromAmount > 0 ? qtyFromAmount : 0;
    }

    if (qtyFromAmount <= 0) {
      return rawQty;
    }

    const epsilon = 1e-8;
    const isRawQtyConsistent = Math.abs(rawQty - qtyFromAmount) <= epsilon * Math.max(1, qtyFromAmount);
    if (isRawQtyConsistent) {
      return rawQty;
    }

    const scaledQty = rawQty * multiplier;
    const isScaledQtyConsistent = Math.abs(scaledQty - qtyFromAmount) <= epsilon * Math.max(1, qtyFromAmount);
    if (isScaledQtyConsistent) {
      return scaledQty;
    }

    return rawQty;
  };

  private fetchBitrueTradeRows = async (opts: HistoricalTradeOptions) => {
    if (this.isPublicDataOnly || !opts.symbol) {
      return [] as FETCH_MY_TRADES_RESPONSE['data'];
    }

    const params = omitUndefined({
      contractName: opts.symbol,
      fromId: opts.fromId,
      limit: Math.min(Math.max(opts.limit || 200, 1), 1000),
      startTime: this.normalizeTimestampMs(opts.from),
      endTime: this.normalizeTimestampMs(opts.to),
    });

    const { data } = await this.xhr.get<FETCH_MY_TRADES_RESPONSE>(ENDPOINTS.MY_TRADES, { params });
    if (!this.isSuccessCode(data?.code)) {
      throw new Error(String(data?.msg || 'Failed to fetch my trades'));
    }

    return Array.isArray(data?.data) ? data.data : [];
  };

  dispose = () => {
    if (this.fastPrivateRefreshTimeoutId) {
      clearTimeout(this.fastPrivateRefreshTimeoutId);
      this.fastPrivateRefreshTimeoutId = undefined;
    }
    this.emitter.off('fill', this.handleFillForFastRefresh);
    super.dispose();
    this.publicWebsocket.dispose();
    this.privateWebsocket.dispose();
  };

  private handleFillForFastRefresh = (fill: { symbol?: string }) => {
    this.queueFastPrivateRefresh(fill?.symbol);
  };

  queueFastPrivateRefresh = (symbol?: string) => {
    if (symbol) {
      this.pendingOrderRefreshSymbols.add(symbol);
    }
    if (this.fastPrivateRefreshTimeoutId) {
      clearTimeout(this.fastPrivateRefreshTimeoutId);
    }
    this.fastPrivateRefreshTimeoutId = setTimeout(() => {
      this.fastPrivateRefreshTimeoutId = undefined;
      this.runFastPrivateRefresh();
    }, 350);
  };

  private runFastPrivateRefresh = async () => {
    if (this.isDisposed || this.isPublicDataOnly || this.isFastPrivateRefreshRunning) {
      return;
    }
    this.isFastPrivateRefreshRunning = true;
    try {
      const { balance, positions } = await this.fetchBalanceAndPositions();
      if (this.isDisposed) return;
      this.store.addOrUpdatePositionsFull(positions);
      this.store.update({ balance, loaded: { ...this.store.loaded, balance: true, positions: true } });

      const symbolsToRefresh = Array.from(this.pendingOrderRefreshSymbols);
      this.pendingOrderRefreshSymbols.clear();
      if (symbolsToRefresh.length) {
        await this.fetchAndSetOrders(symbolsToRefresh);
      }
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
    } finally {
      this.isFastPrivateRefreshRunning = false;
      if (this.pendingOrderRefreshSymbols.size) {
        this.queueFastPrivateRefresh();
      }
    }
  };

  validateAccount = async () => {
    const { data } = await this.xhr.get<FETCH_ACCOUNT_RESPONSE>(ENDPOINTS.ACCOUNT);
    if (!this.isSuccessCode(data?.code)) {
      return { error: data?.msg || 'Invalid API Key or Secret' };
    }
    return {
      error: '',
      balance: await this.fetchBalanceAndPositions()
        .then((r) => r.balance)
        .catch(() => undefined),
    };
  };

  start = async () => {
    const useExternalPublicData = await this.initExternalPublicData();
    const publicDataOnly = this.isPublicDataOnly;

    if (!useExternalPublicData) {
      const markets = await this.fetchMarkets();
      if (this.isDisposed) return;
      // create empty ticker stubs so we can use deltas right away in ws
      const tickers: Ticker[] = markets.map((m) => ({
        id: m.symbol,
        symbol: m.symbol,
        bid: 0,
        ask: 0,
        last: 0,
        mark: 0,
        index: 0,
        percentage: 0,
        openInterest: 0,
        fundingRate: 0,
        fundingTime: 0,
        volume: 0,
        quoteVolume: 0,
      }));
      this.store.update({
        markets,
        tickers,
        loaded: { ...this.store.loaded, markets: true, tickers: true },
      });

      this.publicWebsocket.connectAndSubscribe();
    }

    if (!publicDataOnly) {
      this.privateWebsocket.connectAndSubscribe();
      const { balance, positions } = await this.fetchBalanceAndPositions();
      if (this.isDisposed) return;
      this.store.update({ balance, positions, loaded: { ...this.store.loaded, balance: true, positions: true } });
      const orders = await this.fetchOrders();
      if (this.isDisposed) return;
      this.store.update({ orders, loaded: { ...this.store.loaded, orders: true } });
      this.tick();
      // this.fetchAndEmitOrders();
    }
  };

  updateOrder = async ({ order, update }: UpdateOrderOpts) => {
    const newOrder = {
      ...order,
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
    return await this.placeOrder(newOrder);
  };

  tick = async () => {
    if (!this.isDisposed) {
      try {
        const { balance, positions } = await this.fetchBalanceAndPositions();
        if (this.isDisposed) return;

        // check if any positions are now closed (contracts = 0) but were open on previous tick
        const closedOnTick = this.store.positions.filter((p) => {
          if (p.contracts === 0) {
            return false;
          }
          const found = positions.find((sp) => sp.id === p.id);
          if (!found) {
            return true;
          }

          return found.contracts === 0;
        });

        this.store.addOrUpdatePositionsFull(positions);
        this.store.update({ balance, loaded: { ...this.store.loaded, balance: true, positions: true } });

        // if any positions were closed, fetch orders for those symbols stops should be removed, but dont come over ws
        if (closedOnTick.length) {
          await this.fetchAndSetOrders(closedOnTick.map((p) => p.symbol));
        }
      } catch (err: any) {
        this.emitAndLogError(err?.message);
      }

      if (!this.isDisposed) {
        loop(() => this.tick(), this.getTickInterval());
      }
    }
  };

  fetchBalanceAndPositionsRaw = async (): Promise<FETCH_ACCOUNT_RESPONSE | undefined> => {
    try {
      const { data } = await this.xhr.get<FETCH_ACCOUNT_RESPONSE>(ENDPOINTS.ACCOUNT);

      if (!this.isSuccessCode(data?.code)) {
        throw new Error(String(data?.msg || 'Failed to fetch account'));
      }
      return data;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
    }
  };

  fetchBalance = async () => {
    const data = await this.fetchBalanceAndPositionsRaw();
    return this.parseBalance(data);
  };

  fetchOpenPositions = async () => {
    const data = await this.fetchBalanceAndPositionsRaw();
    return await this.parsePositions(data);
  };

  /**
   * Bitrue documents an empty `positionVos` array for an account with no
   * positions. Anything else missing or malformed is an incomplete account
   * snapshot, not evidence that positions were closed.
   */
  private getPositionAccounts = (data?: FETCH_ACCOUNT_RESPONSE) => {
    const accounts = data?.data?.account;
    if (!Array.isArray(accounts)) {
      return undefined;
    }
    const futuresAccounts = accounts.filter((account) => account && ['USDT', 'USDC'].includes(account.marginCoin));
    if (futuresAccounts.length === 0) {
      return undefined;
    }

    const hasValidPositionsSchema = futuresAccounts.every(
      (account) =>
        Array.isArray(account.positionVos) &&
        account.positionVos.every(
          (positionVo) =>
            positionVo &&
            typeof positionVo.contractName === 'string' &&
            positionVo.contractName.length > 0 &&
            Array.isArray(positionVo.positions) &&
            positionVo.positions.every(
              (position) =>
                position &&
                (position.side === 'BUY' || position.side === 'SELL') &&
                Number.isFinite(Number(position.volume)) &&
                Number.isFinite(Number(position.openPrice)) &&
                Number.isFinite(Number(position.leverageLevel)) &&
                Number(position.leverageLevel) > 0 &&
                Number.isFinite(Number(position.positionType)),
            ),
        ),
    );
    return hasValidPositionsSchema ? futuresAccounts : undefined;
  };

  private getAccountPositionSnapshots = (data?: FETCH_ACCOUNT_RESPONSE) => {
    const accounts = this.getPositionAccounts(data);
    if (!accounts) return undefined;

    return accounts.flatMap((account) =>
      account.positionVos!.flatMap((positionVo) =>
        positionVo.positions.map((position) => ({
          symbol: positionVo.contractName,
          side: position.side === 'BUY' ? PositionSide.Long : PositionSide.Short,
          leverageLevel: position.leverageLevel,
          positionType: position.positionType,
        })),
      ),
    );
  };

  fetchPositionMetadata = async (
    symbols: string[],
    currentPositions?: Array<{
      symbol: string;
      side: PositionSide;
      leverageLevel: number;
      positionType: number;
    }>,
  ): Promise<Pick<Position, 'symbol' | 'side' | 'leverage' | 'positionMode' | 'marginType'>[]> => {
    if (!currentPositions && symbols.some((symbol) => !this.positionMetadataBySymbol.has(symbol))) {
      currentPositions = this.getAccountPositionSnapshots(await this.fetchBalanceAndPositionsRaw());
    }

    if (currentPositions) {
      const nextMetadata = new Map<
        string,
        Pick<Position, 'symbol' | 'side' | 'leverage' | 'positionMode' | 'marginType'>
      >();
      for (const position of currentPositions) {
        const leverage = Number(position.leverageLevel);
        if (!position.symbol || !Number.isFinite(leverage) || leverage <= 0) {
          continue;
        }
        nextMetadata.set(position.symbol, {
          symbol: position.symbol,
          side: position.side,
          leverage,
          positionMode: PositionMode.OneWay,
          marginType: position.positionType === 2 ? MarginType.Isolated : MarginType.Cross,
        });
      }
      this.positionMetadataBySymbol = nextMetadata;
    }

    if (this.isDisposed) return [];
    return symbols.flatMap((symbol) => {
      const metadata = this.positionMetadataBySymbol.get(symbol);
      return metadata ? [metadata] : [];
    });
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
      };
    });
    return [...openPositionsForSymbols, ...fakePositions];
  };

  fetchBalanceAndPositions = async () => {
    const data = await this.fetchBalanceAndPositionsRaw();
    const positions = await this.parsePositions(data);
    const balance = this.parseBalance(data);
    return { balance, positions };
  };

  parsePositions = async (data?: FETCH_ACCOUNT_RESPONSE) => {
    const accounts = this.getPositionAccounts(data);
    const currentPositions = this.getAccountPositionSnapshots(data);
    if (!accounts || !currentPositions) {
      return this.store.positions;
    }
    const symbols = currentPositions.map((position) => position.symbol);
    const positionMetaData = await this.fetchPositionMetadata(symbols, currentPositions);

    return accounts
      .map((account) => {
        // positionVos?: {
        //   contractId: number;
        //   contractName: string;
        //   contractSymbol: string;
        //   adlEnabled: boolean;
        //   positions: {
        //     id: number;
        //     uid: number;
        //     contractId: number;
        //     positionType: number;
        //     side: string;
        //     volume: number;
        //     openPrice: number;
        //     avgPrice: number;
        //     closePrice: number;
        //     leverageLevel: number;
        //     openAmount: number;
        //     holdAmount: number;
        //     closeVolume: number;
        //     pendingCloseVolume: number;
        //     realizedAmount: number;
        //     historyRealizedAmount: number;
        //     forceLiquidationVolume: number;
        //     forceLiquidationPrice: number;
        //     tradeFee: number;
        //     capitalFee: number;
        //     closeProfit: number;
        //     shareAmount: number;
        //     freezeLock: number;
        //     status: number;
        //     ctime: string;
        //     mtime: string;
        //     brokerId: number;
        //     usingAccountType: number;
        //     preShareAmount: number;
        //     marginRate: number;
        //     reducePrice: number;
        //     returnRate: number;
        //     unRealizedAmount: number;
        //     openRealizedAmount: number;
        //     positionBalance: number;
        //     settleProfit: number;
        //     keepRate: number;
        //     maxFeeRate: number;
        //     version: number;
        //     indexPrice: number;
        //   }[];
        // }[];
        return account
          .positionVos!.map((positionVo) => {
            return positionVo.positions.map((position): Position => {
              const symbol = positionVo.contractName;
              const metaData = positionMetaData.find((m) => m.symbol === symbol);
              // 1 crossed position, 2 isolated position
              const marginType =
                metaData?.marginType || (position.positionType === 1 ? MarginType.Cross : MarginType.Isolated);
              const leverage = metaData?.leverage || position.leverageLevel;
              const positionMode = metaData?.positionMode || PositionMode.OneWay;
              const side = position.side === 'BUY' ? PositionSide.Long : PositionSide.Short;
              const id = createPositionId(positionVo.contractName, side, positionMode);
              const existingPosition = this.store.positions.find((p) => p.id === id);
              const ticker = this.store.tickersBySymbol[symbol];
              const contracts = position.volume;
              const lastPrice = ticker?.last || 0;
              const entryPrice = position.openPrice;
              const unrealizedPnl = (side === PositionSide.Long ? 1 : -1) * (lastPrice - entryPrice) * position.volume;
              const parsedLiquidationPrice = [position.forceLiquidationPrice, position.reducePrice]
                .map((value) => Number(value))
                .find((value) => Number.isFinite(value) && value > 0);
              const liquidationPrice = parsedLiquidationPrice || existingPosition?.liquidationPrice || 0;
              // const unrealizedPnl = position.openRealizedAmount
              return {
                id,
                symbol,
                side,
                positionMode,
                marginType,
                entryPrice,
                notional: position.positionBalance,
                leverage,
                unrealizedPnl,
                contracts,
                liquidationPrice,
                info: position,
              };
            });
          })
          .flat();
      })
      .flat();
  };

  parseBalance = (data?: FETCH_ACCOUNT_RESPONSE) => {
    if (!data) {
      return this.store.balance;
    }

    const balanceByCoins: Record<string, Balance> = data.data.account.reduce(
      (acc, account) => {
        if (account.marginCoin === 'USDT' || account.marginCoin === 'USDC') {
          const total = Number(account.totalEquity || 0);
          const free = Number(account.accountNormal || 0);
          const accountLock = Number(account.accountLock || 0);
          const maintenance = Number(account.sumMaintenanceMargin || 0);
          const positionMargin = Number(account.totalPositionNormal || 0);
          const totalCost = Number(account.totalCost || 0);
          const used = Math.max(total - free, accountLock, maintenance, positionMargin, totalCost, 0);
          acc[account.marginCoin] = {
            total,
            free,
            used,
            upnl: account.sumOpenRealizedAmount,
          };
        }
        return acc;
      },
      {} as Record<string, Balance>,
    );

    const primary = balanceByCoins.USDT || balanceByCoins.USDC || { total: 0, free: 0, used: 0, upnl: 0 };
    const balance: Balance = {
      ...primary,
      currencies: balanceByCoins,
      info: data,
    };

    return balance;
  };

  fetchOrders = async (symbols?: string[]) => {
    if (symbols?.length) {
      const orders = await mapSeries(symbols, (symbol) => this.fetchOrdersForSymbol(symbol));
      return orders.flat();
    } else {
      console.warn('No symbols provided... fetching async');
      this.fetchAndEmitOrders();
      return [];
    }
  };

  fetchAndEmitOrders = async () => {
    const fetchId = this.inProgressOrderFetchId + 1;
    this.inProgressOrderFetchId = fetchId;
    const fetchedSymbols = new Set<string>();
    const toFetch = new Set<string>(this.store.markets.map((m) => m.symbol));
    while (toFetch.size) {
      if (this.inProgressOrderFetchId !== fetchId) {
        console.log('fetchAndEmitOrders: fetch cancelled');
        break;
      }
      // if any new priority symbol fetch that now
      let nextToFetch = Array.from(this.prioritySymbols).find((s) => toFetch.has(s));
      if (!nextToFetch) {
        nextToFetch = toFetch.values().next().value;
      }
      if (!nextToFetch) break;
      toFetch.delete(nextToFetch);
      fetchedSymbols.add(nextToFetch);
      const orders = await this.fetchOrdersForSymbol(nextToFetch);
      if (this.isDisposed) return;
      this.store.addOrUpdateOrders(orders);
      await sleep(100);
      if (this.isDisposed) return;
    }
  };

  fetchOrdersForSymbol = async (symbol: string) => {
    try {
      // Open-order reconciliation is an authenticated read. Keep it on the
      // helper-wired client so secondary read keys and the package rate limit
      // apply; `unlimitedXHR` is reserved for urgent writes and public markets.
      const { data } = await this.xhr.get<FETCH_ORDERS_RESPONSE>(ENDPOINTS.ORDERS, {
        params: {
          contractName: symbol,
        },
      });

      if (!this.isSuccessCode(data?.code)) {
        throw new Error(String(data?.msg || `Failed to fetch orders for ${symbol}`));
      }

      // conditional
      // {
      //   "orderId": 176386,
      //   "clientOrderId": "0",
      //   "price": 0,
      //   "origAmount": 24,
      //   "executedQty": 0.0024,
      //   "status": "INIT",
      //   "side": "SELL",
      //   "action": "CLOSE",
      //   "transactTime": 1737788230000,
      //   "triggerPrice": 105089.8,
      //   "triggerType": 2,
      //   "conditionOrder": true
      // }

      type NativeOrder = FETCH_ORDERS_RESPONSE['data'][number] & {
        conditionOrder?: boolean;
        childOrders?: NativeOrder[];
      };
      if (!Array.isArray(data?.data)) throw new Error('Malformed Bitrue order snapshot');
      const orders: Order[] = [];
      const seen = new Set<string>();
      const mapOrder = (order: NativeOrder, parent?: Order): Order => {
        if (order.orderId === undefined || order.orderId === null) throw new Error('Bitrue order has no identity');
        const id = String(order.orderId);
        if (!id || seen.has(id)) throw new Error('Duplicate Bitrue order identity');
        seen.add(id);
        const protective = order.conditionOrder || parent || !order.type;
        const triggerKind =
          Number(order.triggerType) === 1
            ? OrderType.StopLoss
            : Number(order.triggerType) === 2
              ? OrderType.TakeProfit
              : undefined;
        const executionKind = ['IOC', 'FOK', 'POST_ONLY'].includes(String(order.type))
          ? OrderType.Limit
          : ORDER_TYPE[order.type!];
        const type = protective ? triggerKind || executionKind : executionKind;
        const status = ORDER_STATUS[String(order.status).toUpperCase()];
        const side =
          parent && order.action !== 'CLOSE'
            ? parent.side === OrderSide.Buy
              ? OrderSide.Sell
              : OrderSide.Buy
            : ORDER_SIDE[order.side];
        if (!type || !status || !side) throw new Error('Malformed Bitrue order schema');
        const conditional = type === OrderType.StopLoss || type === OrderType.TakeProfit;
        if (parent && !conditional) throw new Error('Malformed Bitrue protective child');
        const amount =
          conditional && !parent && Number(order.executedQty) > 0 ? Number(order.executedQty) : Number(order.origQty);
        const filled = conditional ? 0 : Number(order.executedQty);
        const price = Number(conditional ? order.triggerPrice : order.price);
        if (![amount, filled, price].every(Number.isFinite) || amount < 0 || filled < 0 || filled > amount || price < 0)
          throw new Error('Malformed Bitrue order quantities');
        const previous = this.store.orders.find((row) => row.id === id);
        const reference =
          Number(order.triggerPriceType) === 1
            ? OrderStopTriggerType.LastPrice
            : Number(order.triggerPriceType) === 2
              ? OrderStopTriggerType.MarkPrice
              : undefined;
        let mapped: Order = {
          id,
          status,
          symbol,
          type,
          side,
          price,
          amount,
          filled,
          remaining: amount - filled,
          reduceOnly: Boolean(parent) || order.action === 'CLOSE',
          ...(parent && { parentId: parent.id }),
          ...(order.clientOrderId !== undefined && { clientOrderId: String(order.clientOrderId) }),
          ...(type === OrderType.StopLoss &&
            (reference || previous?.slTriggerType) && { slTriggerType: reference || previous?.slTriggerType }),
          ...(type === OrderType.TakeProfit &&
            (reference || previous?.tpTriggerType) && { tpTriggerType: reference || previous?.tpTriggerType }),
          takeProfit: previous?.takeProfit,
          stopLoss: previous?.stopLoss,
          info: order,
          timeInForce: OrderTimeInForce.GoodTillCancel,
          lastUpdate: Number(order.transactTime || 0),
        };
        const slot = orders.length;
        orders.push(mapped);
        if (order.childOrders !== undefined) {
          if (!Array.isArray(order.childOrders)) throw new Error('Malformed Bitrue child snapshot');
          mapped = { ...mapped, stopLoss: undefined, takeProfit: undefined };
          for (const child of order.childOrders) {
            const protection = mapOrder(child, mapped);
            if ([OrderStatus.Closed, OrderStatus.Canceled].includes(protection.status)) continue;
            if (protection.type === OrderType.StopLoss) mapped = { ...mapped, stopLoss: protection.price };
            if (protection.type === OrderType.TakeProfit) mapped = { ...mapped, takeProfit: protection.price };
          }
        }
        orders[slot] = mapped;
        return mapped;
      };
      for (const order of data.data) mapOrder(order as NativeOrder);
      return orders;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      // A per-symbol reconciliation must never leak another symbol's orders
      // into the result. `fetchOrders(symbols)` flattens these arrays, so the
      // old account-wide fallback duplicated unrelated orders on every failed
      // symbol request.
      return this.store.orders.filter((order) => order.symbol === symbol);
    }
  };

  /**
   * Internal only — the public fill surface is `fetchHistoricalFills`.
   *
   * Kept because this venue's own `fetchHistoricalFills` consumes it: the two methods
   * read one axis, and here the millisecond-shaped mapper is the one that does
   * the parsing. Private so nothing outside the class can treat it as a second
   * public history axis again.
   */
  private fetchHistoricalTradesInternal = async (opts: HistoricalTradeOptions): Promise<HistoricalTrade[]> => {
    if (this.isPublicDataOnly || !opts.symbol) {
      return [];
    }

    try {
      const rows = await this.fetchBitrueTradeRows(opts);
      const requestedOrderId = opts.orderId ? String(opts.orderId) : undefined;
      const fromMs = this.normalizeTimestampMs(opts.from);
      const toMs = this.normalizeTimestampMs(opts.to);
      const requestedSymbol = this.normalizeTradeSymbol(opts.symbol);
      const maxLimit = Math.min(Math.max(opts.limit || 200, 1), 1000);
      const seen = new Set<string>();

      const mapped = rows
        .map((entry): HistoricalTrade | null => {
          const symbol = this.normalizeTradeSymbol(entry.contractName || requestedSymbol);
          if (!symbol) {
            return null;
          }

          const side = ORDER_SIDE[String(entry.side || '').toUpperCase()] || OrderSide.Buy;
          const price = Number(entry.price || 0);
          const qty = this.resolveTradeQty(entry, symbol, price);
          const quoteQty = Number(entry.amount ?? 0);
          const fee = Number(entry.fee ?? 0);
          const rawTime = entry.ctime ?? entry.cTime ?? entry.time ?? 0;
          const parsedNumericTime = Number(rawTime);
          const time = Number.isFinite(parsedNumericTime)
            ? parsedNumericTime > 1_000_000_000_000
              ? parsedNumericTime
              : parsedNumericTime * 1000
            : Date.parse(String(rawTime || ''));
          const orderId = this.pickTradeOrderId(entry);

          if (!Number.isFinite(price) || price <= 0 || !Number.isFinite(qty) || qty <= 0 || !Number.isFinite(time)) {
            return null;
          }
          if (symbol !== requestedSymbol) {
            return null;
          }
          if (requestedOrderId && orderId !== requestedOrderId) {
            return null;
          }
          if (fromMs !== undefined && time < fromMs) {
            return null;
          }
          if (toMs !== undefined && time > toMs) {
            return null;
          }
          const id = String(entry.tradeId);
          if (seen.has(id)) return null;
          seen.add(id);

          return omitUndefined({
            id,
            symbol,
            orderId,
            price,
            qty,
            quoteQty: Number.isFinite(quoteQty) ? quoteQty : undefined,
            positionSide: 'both' as const,
            side,
            buyer: typeof entry.isBuyer === 'boolean' ? entry.isBuyer : undefined,
            maker: typeof entry.isMaker === 'boolean' ? entry.isMaker : undefined,
            commission: Number.isFinite(fee) ? Math.abs(fee) : undefined,
            commissionAsset: symbol.split('-')[2],
            time,
          });
        })
        .filter((entry): entry is HistoricalTrade => !!entry)
        .sort((a, b) => b.time - a.time)
        .slice(0, maxLimit);
      if (rows.length >= maxLimit && mapped.length < maxLimit) {
        throw new Error('INCOMPLETE: Bitrue native fill page is full before local filters reach the requested limit');
      }
      return mapped;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      throw err;
    }
  };

  fetchHistoricalFills = async (opts: ExecutionsOptions): Promise<Execution[]> => {
    if (this.isPublicDataOnly || !opts.symbol) {
      return [];
    }

    try {
      const trades = await this.fetchHistoricalTradesInternal({
        symbol: opts.symbol,
        orderId: opts.orderId,
        from: opts.from,
        to: opts.to,
        limit: opts.limit,
      });

      return trades.map((trade) => {
        return omitUndefined({
          // Carry the per-fill id across the re-map; dropping it here undid the whole
          // point of populating it on the HistoricalTrade above.
          id: trade.id,
          timestamp: Math.floor(trade.time / 1000),
          symbol: trade.symbol,
          orderId: trade.orderId,
          price: trade.price,
          qty: trade.qty,
          side: trade.side,
          // Carried across the re-map for the same reason as `id`: dropping a
          // field the inner mapper populated is exactly the drift that let the
          // two history methods diverge in the first place.
          realizedPnl: trade.realizedPnl,
          positionSide: trade.positionSide,
          maker: trade.maker,
          commission: trade.commission,
          commissionAsset: trade.commissionAsset,
        });
      });
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      throw err;
    }
  };

  fetchMarkets = async () => {
    const { data } = await this.unlimitedXHR.get<FETCH_CONTRACTS_RESPONSE>(ENDPOINTS.MARKETS);
    const markets: Market[] = data.map((market) => {
      const multiplier = market.multiplier;
      return {
        id: market.symbol,
        symbol: market.symbol,
        base: market.symbol.split('-')[1],
        quote: market.symbol.split('-')[2],
        active: market.status === 1,
        contractSize: multiplier,
        precision: {
          amount: multiplier,
          price: 1 / 10 ** market.pricePrecision,
        },
        limits: {
          amount: {
            min: market.minOrderVolume * multiplier,
            max: market.maxMarketVolume * multiplier,
          },
          leverage: {
            min: 1,
            // Bitrue's documented venue ceiling is 125x, but the actual cap is
            // contract-specific. Keep the conservative startup value until
            // fetchSymbolMaxLeverage() reads /fapi/v2/leverageBracket.
            max: 100,
          },
        },
        takerFee: 0.001,
        makerFee: 0.001,
        info: market,
      };
    });
    return markets;
  };

  fetchOHLCV = async (opts: OHLCVOptions) => {
    // assume we ar ein worker if there is a ws instance
    if (this.publicWebsocket.ws) {
      const topic = this.publicWebsocket.formatKlineTopic(opts.symbol, opts.interval);

      if (this.recentWsRequests[topic] && Date.now() - this.recentWsRequests[topic].time < 10_000) {
        return this.recentWsRequests[topic].data;
      }

      let i = 0;
      while (this.publicWebsocket.ws.readyState !== 1) {
        i++;
        if (i > 100) {
          throw new Error('fetchOHLCV: websocket not connected');
        }
        await sleep(100);
      }
      const rawCandles = await this.publicWebsocket.makeRequest(topic);
      const candles = this.publicWebsocket.mapKline(rawCandles, opts.symbol);
      this.recentWsRequests[topic] = {
        time: Date.now(),
        data: candles,
      };
      return candles;

      //   else need to call method in worker
    } else {
      const _opts: any = [opts];
      const candles: Candle[] = await this.callMethodInWorker('fetchOHLCV', ..._opts);
      return candles;
    }
  };

  private formatCreateOrder = (opts: PlaceOrderOpts) => {
    const market = this.store.marketsBySymbol[opts.symbol];
    if (!market) {
      throw new Error(`Market ${opts.symbol} not found`);
    }
    const isStopOrTP = opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit;

    const foundPosition = this.store.positions.find((p) => p.symbol === opts.symbol);

    const maxSize = market.limits.amount.max;
    const pPrice = market.precision.price;
    const pAmount = market.precision.amount;

    // Reduce-only downsizing is handled client-side by the shared
    // `downSizeReduceOrder` util (gated by `autoDownSizeReduceOrders` metadata),
    // consistent with every other flag-based exchange — don't re-clamp here.
    const amount = adjust(opts.amount, pAmount);
    const side = opts.side === OrderSide.Buy ? 'BUY' : 'SELL';
    const direction = isStopOrTP || opts.reduceOnly ? 'CLOSE' : 'OPEN';
    const type = inverseObj(ORDER_TYPE)[opts.type];

    // We use price only for limit orders
    // Market order should not define price
    const price = opts.price && opts.type !== OrderType.Market ? adjust(opts.price, pPrice) : undefined;

    let leverage = market.limits.leverage.max;
    let marginType = MarginType.Cross;
    if (!foundPosition) {
      console.error('Bitrue position not found when placing order. Expected to determine leverage and margin type...');
    } else {
      leverage = foundPosition.leverage;
      marginType = foundPosition.marginType || MarginType.Cross;
    }

    if (isStopOrTP) {
      if (!foundPosition) {
        console.error('Bitrue position not found when placing order. Needed for binding stop loss or take profit...');
        throw new Error('Position not found');
      }
      // if (opts.amount > foundPosition.contracts) {
      //   const msg = `Cannot place stop loss or take profit order with amount greater than open position. Position: ${foundPosition.contracts}, Order: ${opts.amount}`;
      //   this.emitAndLogError(msg);
      //   throw new Error(msg);
      // }
    }

    if (isStopOrTP && (opts.stopLoss !== undefined || opts.takeProfit !== undefined)) {
      throw new Error('Bitrue only supports attached TP/SL on limit or market parent orders');
    }

    const req = omitUndefined({
      // contractId: 1,
      contractName: opts.symbol,
      leverage,
      // ...isStopOrTP? {triggerPrice: price} : {},
      price: isStopOrTP ? 0 : price,
      triggerPrice: isStopOrTP ? price : undefined,
      volume: amount,
      // ...isStopOrTP? {type: 'MARKET'} : {type},
      type: isStopOrTP ? 'MARKET' : type,
      side,
      open: direction,
      positionType: marginType === MarginType.Cross ? 1 : 2,
      futureModel: 'normal',
      ...(isStopOrTP
        ? {
            conditionOrder: true,
            // 1 stoploss 2 take profit
            triggerType: opts.type === OrderType.StopLoss ? 1 : 2,
            // 1 last 2 mark
            triggerPriceType:
              opts.type === OrderType.StopLoss
                ? opts.slTriggerType === OrderStopTriggerType.LastPrice
                  ? 1
                  : 2
                : opts.tpTriggerType === OrderStopTriggerType.MarkPrice
                  ? 2
                  : 1,
          }
        : {}),
    });

    const lots = amount > maxSize ? Math.ceil(amount / maxSize) : 1;
    const rest = amount > maxSize ? adjust(amount % maxSize, pAmount) : 0;

    const lotSize = adjust((amount - rest) / lots, pAmount);

    let payloads: Array<Record<string, any>> = times(lots, () => ({
      ...req,
      clientOrderId: uuid(),
      volume: `${lotSize}`,
    }));

    if (rest) {
      payloads.push({ ...req, clientOrderId: uuid(), volume: `${rest}` });
    }

    const callerClientOrderId = this.resolveCallerClientOrderId(opts);
    this.assertSingleNativeClientOrderId(callerClientOrderId, payloads.length);
    if (callerClientOrderId !== undefined) payloads[0].clientOrderId = callerClientOrderId;

    //
    if (opts.type === OrderType.Market && !opts.reduceOnly) {
      const ticker = this.store.tickersBySymbol[opts.symbol];
      const referencePrice = ticker?.mark || ticker?.last || ticker?.bid || ticker?.ask || 0;
      if (!referencePrice) {
        throw new Error('Market order without reference price');
      }
      payloads = payloads.map(({ volume, ...rest }) => {
        // bitrue supports 4 decimals for amount. Add 1/3 of step size (amount precision) as we are already using exact base size, and bitrue
        // will round down if quote size is any units below. We may have a different reference price, rounding error, etc
        const amount = adjust((parseFloat(volume) + market.precision.amount / 3) * referencePrice, 0.0001);
        return { ...rest, amount: Math.max(amount, market.info.minOrderMoney) };
      });
    }

    if (opts.stopLoss !== undefined || opts.takeProfit !== undefined) {
      payloads = payloads.map((payload) => {
        const triggerOrderCreateParams = [] as Array<{
          clientOrderId: string;
          triggerType: 1 | 2;
          triggerPriceType: 1 | 2;
          triggerPrice: number;
          type: 'MARKET';
          price: 0;
        }>;
        if (opts.stopLoss !== undefined) {
          triggerOrderCreateParams.push({
            clientOrderId: uuid(),
            triggerType: 1,
            triggerPriceType: opts.slTriggerType === OrderStopTriggerType.LastPrice ? 1 : 2,
            triggerPrice: opts.stopLoss,
            type: 'MARKET',
            price: 0,
          });
        }
        if (opts.takeProfit !== undefined) {
          triggerOrderCreateParams.push({
            clientOrderId: uuid(),
            triggerType: 2,
            triggerPriceType: opts.tpTriggerType === OrderStopTriggerType.MarkPrice ? 2 : 1,
            triggerPrice: opts.takeProfit,
            type: 'MARKET',
            price: 0,
          });
        }
        return { ...payload, triggerOrderCreateParams };
      });
    }

    return payloads;
  };

  placeOrder = async (opts: PlaceOrderOpts) => {
    this.resolveCallerClientOrderId(opts);
    const payloads = this.formatCreateOrder(opts);
    const res = await this.placeOrderBatch(payloads);
    if (opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit || opts.stopLoss || opts.takeProfit) {
      await (this as any as BaseExchange).callMethodInWorker('fetchAndSetOrders', [opts.symbol]);
      // stop orders have bad ID, just remove the placeholder by returning []
      return [];
    }
    return res;
  };

  placeOrders = async (orders: PlaceOrderOpts[]) => {
    this.assertCallerClientOrderIds(orders);
    const requests = orders.flatMap((o) => this.formatCreateOrder(o));
    const res = await this.placeOrderBatch(requests);
    // set of symbols of orders that were TP or SL
    const tpSlSymbols = new Set(
      orders
        .filter((o) => o.type === OrderType.StopLoss || o.type === OrderType.TakeProfit || o.stopLoss || o.takeProfit)
        .map((o) => o.symbol),
    );

    // if set is not empty, need to fetch updated orders in worker as WS does not provide TP/SL updates
    if (tpSlSymbols.size) {
      await (this as any as BaseExchange).callMethodInWorker('fetchAndSetOrders', Array.from(tpSlSymbols));
      // stop orders have bad ID, just remove the placeholder by returning []
      return [];
    }
    return res;
  };

  private placeOrderBatch = async (payloads: any[]) => {
    const lots = chunk(payloads, 5);
    const orderIds = [] as string[];

    for (const lot of lots) {
      await Promise.all(
        lot.map(async (payload) => {
          try {
            const { data } = await this.unlimitedXHR.post(ENDPOINTS.PLACE_ORDER, payload);
            if (!this.isSuccessCode(data?.code)) {
              throw new Error(String(data?.msg || 'Failed to place order'));
            }
            orderIds.push(data?.data?.orderId);
          } catch (err: any) {
            this.emitAndLogError(err?.response?.data?.msg || err?.message);
            throw new Error(err?.response?.data?.msg || err?.message);
          }
        }),
      );
    }

    return orderIds;
  };

  fetchSymbolMaxLeverage = async (symbol: string) => {
    const fetchedMaxLeverage = await this.fetchAndSyncSymbolMaxLeverage(symbol);
    if (fetchedMaxLeverage !== null) {
      return fetchedMaxLeverage;
    }
    const existingMaxLeverage = Number(this.store.marketsBySymbol[symbol]?.limits?.leverage?.max);
    if (Number.isFinite(existingMaxLeverage) && existingMaxLeverage > 0) {
      return existingMaxLeverage;
    }
    return 1;
  };

  updateSymbolMetadata = async (symbol: string) => {
    await this.fetchAndSyncSymbolMaxLeverage(symbol);
  };

  setLeverage = async (symbol: string, leverage: number, silent = false) => {
    // const foundPosition = this.store.positions.find((p) => p.symbol === symbol);
    // if (!foundPosition) {
    //   console.log(this.store.positions);
    //   throw new Error('Position not found');
    // }
    // if (foundPosition.contracts !== 0) {
    //   this.emitAndLogError('Cannot change leverage of open position');
    //   throw new Error('Cannot change leverage of open position');
    // }
    //
    // const updatedPosition = {
    //   ...foundPosition,
    //   leverage,
    // };
    //
    // this.store.addOrUpdatePosition(updatedPosition);

    // this.xhr.post(ENDPOINTS.SET_LEVERAGE, {
    //   contractName: symbol,
    //   leverage,
    // });

    try {
      const { data } = await this.xhr.post<{
        code: '0';
        msg: 'Success';
        data: any;
      }>(ENDPOINTS.SET_LEVERAGE, {
        contractName: symbol,
        leverage,
      });
      if (!this.isSuccessCode(data?.code)) {
        throw new Error(String(data?.msg || 'Failed to set leverage'));
      }
      this.positionMetadataBySymbol.delete(symbol);
    } catch (err: any) {
      const errorMessage = err?.response?.data?.msg || err?.message || 'Failed to set leverage';
      const maxLeverage = this.parseMaxLeverageFromErrorMessage(errorMessage);
      if (maxLeverage !== null) {
        this.setMarketMaxLeverage(symbol, maxLeverage);
      }
      !silent && this.emitAndLogError(errorMessage);
      throw new Error(errorMessage);
    }
  };

  changeMarginType = async (_isolated: boolean, _symbol?: string) => {
    const msg = 'Cannot change Margin Type on Tealstreet as API does not support it. Change it on bitrue.com';
    this.emitAndLogError(msg);
    throw new Error(msg);
  };

  cancelOrders = async (orders: Order[]) => {
    try {
      const cancelPayloads = orders.map((o) => {
        const conditionOrder = o.type === OrderType.StopLoss || o.type === OrderType.TakeProfit;
        return { orderId: o.id, contractName: o.symbol, conditionOrder };
      });
      const lots = chunk(cancelPayloads, 50);
      await mapSeries(lots, async (lot) => {
        const responses = await Promise.all(
          lot.map(async (payload) => {
            const { data } = await this.xhr.post<{
              code: '0';
              msg: 'Success';
              data: {
                orderId: number;
              };
            }>(ENDPOINTS.CANCEL_ORDER, {
              ...payload,
            });
            if (!this.isSuccessCode(data?.code)) {
              throw new Error(String(data?.msg || 'Failed to cancel order'));
            }
            return data.data?.orderId;
          }),
        );
        return responses.flat();
      });

      // bitrue doesnt update orders on cancel, so we need to remove them manually
      const orderIds: Pick<Order, 'id'>[] = orders.map((o) => ({ id: o.id }));
      await (this as any as BaseExchange).callMethodInWorker('removeOrders', orderIds);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      throw new Error(err?.response?.data?.msg || err?.message);
    }
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    this.prioritySymbols.add(opts.symbol);
    return this.publicWebsocket.listenOHLCV(opts, callback);
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    return this.publicWebsocket.listenOrderBook(symbol, callback);
  };
}
