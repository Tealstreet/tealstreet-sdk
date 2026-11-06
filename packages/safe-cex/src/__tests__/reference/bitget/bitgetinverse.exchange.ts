import type { Store } from '../../../store/store.interface';
import type {
  Balance,
  CoinBalance,
  ExchangeName,
  ExchangeOptions,
  Market,
  Order,
  PlaceOrderOpts,
  Position,
  UpdateOrderOpts,
  Writable,
} from '../../../types';

import { MarginType, OrderSide, OrderTimeInForce, OrderType, PositionMode, PositionSide } from '../../../types';
import { createPositionId } from '../../../utils/createPositionId';
import { adjust, multiply, subtract } from '../../../utils/safe-math';
import { virtualClock } from '../../../utils/virtualClock';
import { BITGET_UTA_VARIANT_UNSUPPORTED_MESSAGE, BitgetExchange } from './bitget.exchange';
import {
  ENDPOINTS,
  FETCH_POSITIONS_V2_RESPONSE,
  ORDER_SIDE,
  ORDER_STATUS,
  ORDER_TYPE,
  POSITION_SIDE,
  TIME_IN_FORCE,
} from '../../../exchanges/bitget/bitget.types';

/**
 * BitGet Inverse Exchange
 *
 * Extends BitgetExchange to support coin-margined (inverse) futures contracts.
 * Uses coin-futures (or scoin-futures for testnet) as the product type.
 *
 * Key differences from USDT/USDC futures:
 * - Product type: 'coin-futures' (vs 'usdt-futures')
 * - Symbol format: BTCUSD, ETHUSD (quote is always USD)
 * - Margin coin: The base asset (BTC, ETH, etc.) - varies per symbol
 * - Profits/losses settled in the base currency (not stablecoin)
 *
 * API Documentation: https://www.bitget.com/api-doc/contract/intro
 */
export class BitgetInverseExchange extends BitgetExchange {
  protected supportsUtaRouting = false;

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);
  }

  /**
   * Product type for coin-margined futures
   * - 'coin-futures' for mainnet
   * - 'scoin-futures' for testnet (simulated)
   */
  get apiProductType(): 'susdt-futures' | 'usdt-futures' {
    return (this.options.testnet ? 'scoin-futures' : 'coin-futures') as any;
  }

  /**
   * Default margin coin for inverse contracts.
   * This is used as a fallback - actual margin coin is determined per-symbol.
   */
  get apiMarginCoin(): 'USDT' | 'SUSDT' {
    return 'BTC' as any;
  }

  /**
   * Get the margin coin for a specific symbol.
   * For inverse contracts, the margin coin is the base asset.
   */
  getMarginCoinForSymbol(symbol: string): string {
    const market = this.store.marketsBySymbol[symbol];
    if (market) {
      return market.base; // e.g., 'BTC' for BTCUSD, 'ETH' for ETHUSD
    }
    // Fallback: extract from symbol (e.g., BTCUSD -> BTC)
    return symbol.replace(/USD$/, '');
  }

  /**
   * Override fetchMarkets to filter by quoteCoin === 'USD' for inverse contracts
   */
  fetchMarkets = async (): Promise<Market[]> => {
    const {
      data: { data },
    } = await this.xhr.get<{ data: Array<Record<string, any>> }>(ENDPOINTS.MARKETS, {
      params: { productType: this.apiProductType },
    });

    // For inverse contracts, quoteCoin is 'USD' not 'USDT'
    const markets: Market[] = data
      .filter((m) => m.quoteCoin === 'USD')
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
          isInverse: true,
          info: m,
        };
      });

    return markets;
  };

  /**
   * Override fetchBalance to handle multi-currency balances for inverse contracts
   */
  fetchBalance = async (): Promise<Balance> => {
    const {
      data: response,
    } = await this.xhr.get<{ data: Array<Record<string, any>> }>(ENDPOINTS.BALANCE, {
      params: { productType: this.apiProductType },
    });

    if (parseInt(`${(response as any).code ?? 0}`, 10) !== 0) {
      const message = `${(response as any).code ?? ''}` === '40085'
        ? BITGET_UTA_VARIANT_UNSUPPORTED_MESSAGE
        : (response as any).msg || 'Bitget inverse account balance request failed';
      throw new Error(message);
    }

    const data = response.data;
    if (!data || data.length === 0) return this.store.balance;

    // For inverse, we may have multiple margin coins (BTC, ETH, etc.)
    // Aggregate all balances and convert to USD equivalent
    let totalUsd = 0;
    let upnlUsd = 0;
    let usedUsd = 0;
    let freeUsd = 0;
    const coins: { [coin: string]: CoinBalance } = {};

    for (const balance of data) {
      const marginCoin = balance.marginCoin;
      const equity = parseFloat(balance.unionTotalMargin || balance.accountEquity || '0');
      const unrealizedPL = parseFloat(balance.unrealizedPL || '0');
      const available = parseFloat(balance.unionAvailable || balance.available || '0');
      const used = subtract(equity, available);
      const total = subtract(equity, unrealizedPL);

      // Get USD value using btcEquity field or ticker
      let usdValue = parseFloat(balance.usdtEquity || '0');
      if (usdValue === 0) {
        // Try to get price from tickers
        const ticker = this.store.tickers.find(
          (t) => t.symbol === `${marginCoin}USD` || t.symbol === `${marginCoin}USDT`,
        );
        if (ticker) {
          usdValue = equity * ticker.mark;
        }
      }

      totalUsd += usdValue > 0 ? subtract(usdValue, unrealizedPL * (usdValue / equity)) : total;
      upnlUsd += unrealizedPL * (usdValue > 0 ? usdValue / equity : 1);
      usedUsd += used * (usdValue > 0 ? usdValue / equity : 1);
      freeUsd += available * (usdValue > 0 ? usdValue / equity : 1);

      coins[marginCoin] = {
        base: marginCoin,
        amount: equity,
        notional: usdValue || equity,
      };
    }

    const balanceResult: Balance = {
      total: totalUsd,
      upnl: upnlUsd,
      used: usedUsd,
      free: freeUsd,
      coins,
      info: data,
    };

    return balanceResult;
  };

  /**
   * Override fetchOpenPositions to use per-symbol marginCoin
   * For inverse contracts, converts USD contracts to crypto amounts for display
   */
  fetchOpenPositions = async (): Promise<Position[]> => {
    // For inverse, we need to fetch positions for each margin coin
    // Get unique base coins from markets
    const marginCoins = [...new Set(this.store.markets.map((m) => m.base))];

    const allPositions: Position[] = [];

    for (const marginCoin of marginCoins) {
      try {
        const { data } = await this.xhr.get<FETCH_POSITIONS_V2_RESPONSE>(ENDPOINTS.POSITIONS_V2, {
          params: {
            productType: this.apiProductType,
            marginCoin,
          },
        });

        if (parseInt(data.code || '0') !== 0) {
          this.emitAndLogError(data.msg);
          continue;
        }

        const positions: Position[] = data.data.map((p) => {
          const contractsNative = parseFloat(p.total); // USD contracts from API
          let contracts = contractsNative; // Will be converted to crypto for inverse
          const entryPrice = parseFloat(p.openPriceAvg);

          const symbol = p.symbol.replace(`_${this.apiProductType.toUpperCase()}`, '');
          const positionMode = p.posMode === 'hedge_mode' ? PositionMode.Hedged : PositionMode.OneWay;
          const marginMode = p.marginMode === 'isolated' ? MarginType.Isolated : MarginType.Cross;
          const side = POSITION_SIDE[p.holdSide];
          const positionId = createPositionId(symbol, side, positionMode);

          const unrealizedPnl = parseFloat(p.unrealizedPL);
          const unrealizedPnlNative = unrealizedPnl; // PnL in native currency (crypto for inverse)

          // For inverse contracts, convert USD contracts to crypto amount
          const market = this.store.marketsBySymbol[symbol];
          let notional = multiply(contractsNative, parseFloat(p.markPrice));

          if (market?.isInverse) {
            // Get reference price from ticker or entry price
            const ticker = this.store.tickersBySymbol[symbol];
            const referencePrice = ticker?.mark || ticker?.last || entryPrice;
            if (referencePrice && referencePrice > 0) {
              // Convert USD contracts to crypto amount
              // e.g., 1000 USD / 50000 USD/BTC = 0.02 BTC
              const cryptoAmount = contractsNative / referencePrice;
              // Round to 8 decimal places for crypto precision
              contracts = Math.round(cryptoAmount * 1e8) / 1e8;

              if (contractsNative > 0) {
                this.log(
                  `BitgetInverse position: ${symbol} - USD contracts: ${contractsNative}, price: ${referencePrice}, crypto amount: ${contracts}`,
                );
              }
            }
            // For inverse, notional is the USD value (which is the native contract size)
            notional = contractsNative;
          }

          const marginSize = p.marginSize != null ? parseFloat(p.marginSize) : undefined;
          const position: Position = {
            id: positionId,
            symbol,
            side,
            entryPrice,
            notional,
            leverage: parseFloat(p.leverage),
            unrealizedPnl,
            contracts,
            realizedPnl:
              (p.achievedProfits ? parseFloat(p.achievedProfits) : 0) +
              (p.totalFee ? parseFloat(p.totalFee) : 0) -
              (p.deductedFee ? parseFloat(p.deductedFee) : 0),
            liquidationPrice: this.resolveLiquidationPrice(positionId, p.liquidationPrice),
            marginSize: marginSize && marginSize > 0 ? marginSize : undefined,
            positionMode,
            marginType: marginMode,
            extra: {
              contractsNative, // Native contracts (USD for inverse)
              unrealizedPnlNative, // PnL in native currency (crypto for inverse)
            },
            info: p,
            lastUpdate: virtualClock.getCurrentTimeMs(),
          };

          return position;
        });

        allPositions.push(...positions);
      } catch (err: any) {
        this.emitAndLogError(err?.response?.data?.msg || err?.message);
      }
    }

    return allPositions;
  };

  /**
   * Override cancelAllOrders - for inverse, we don't use marginCoin
   */
  cancelAllOrders = async () => {
    try {
      // For inverse, cancel orders for each margin coin
      const marginCoins = [...new Set(this.store.markets.map((m) => m.base))];

      for (const marginCoin of marginCoins) {
        await this.unlimitedXHR.post(ENDPOINTS.CANCEL_ALL_ORDERS, {
          productType: this.apiProductType,
          marginCoin,
        });
      }
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
    }
  };

  /**
   * Override cancelNormalOrders to use per-symbol marginCoin
   */
  cancelNormalOrders = async (orders: Order[]) => {
    // Group orders by symbol
    const ordersBySymbol = orders.reduce(
      (acc, order) => {
        const key = order.symbol;
        if (!acc[key]) acc[key] = [];
        acc[key].push({ ...order, id: order.id.split('__')[0] });
        return acc;
      },
      {} as Record<string, Order[]>,
    );

    for (const [symbol, symbolOrders] of Object.entries(ordersBySymbol)) {
      const marginCoin = this.getMarginCoinForSymbol(symbol);
      const uniqueOrders = symbolOrders.filter(
        (order, index, self) => self.findIndex((o) => o.id === order.id) === index,
      );

      // Chunk into batches of 50
      const lots = [];
      for (let i = 0; i < uniqueOrders.length; i += 50) {
        lots.push(uniqueOrders.slice(i, i + 50));
      }

      for (const lot of lots) {
        const orderIdList = lot.map((o) => ({ orderId: o.id }));

        try {
          await this.unlimitedXHR.post(ENDPOINTS.CANCEL_ORDERS, {
            symbol,
            orderIdList,
            marginCoin,
            productType: this.apiProductType,
          });
        } catch (err: any) {
          this.emitAndLogError(err?.response?.data?.msg || err?.message);
        }
      }
    }
  };

  /**
   * Override cancelAlgoOrders to use per-symbol marginCoin
   */
  cancelAlgoOrders = async (orders: Order[]) => {
    for (const order of orders) {
      try {
        const marginCoin = this.getMarginCoinForSymbol(order.symbol);
        const [planType] = this.selectPlanTypeAndAmount(order, this.store.positions);
        await this.unlimitedXHR.post(ENDPOINTS.CANCEL_ALGO_ORDER, {
          orderId: order.id,
          symbol: `${order.symbol}`,
          marginCoin,
          productType: this.apiProductType,
          planType,
        });
      } catch (err: any) {
        this.emitAndLogError(err?.response?.data?.msg || err?.message);
      }
    }
  };

  /**
   * Override formatAlgoOrder to use per-symbol marginCoin
   */
  formatAlgoOrder = async (opts: PlaceOrderOpts) => {
    const market = this.store.marketsBySymbol[opts.symbol];
    if (!market) throw new Error(`Market ${opts.symbol} not found`);

    const marginCoin = this.getMarginCoinForSymbol(opts.symbol);

    const payload: Record<string, any> = {
      marginCoin,
      symbol: market.id,
      clientOid: this.generateUuid(),
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
      payload.holdSide = this.getInverseAlgoOrderSide(opts, positionMode);
      const [planType, amount] = await this.getOrderPlanTypeAndAmount(opts);
      payload.planType = planType;
      if (amount) payload.size = `${adjust(amount, market.precision.amount)}`;
    }

    if (opts.type === OrderType.StopLoss && opts.price) {
      const price = adjust(opts.price, market.precision.price);
      payload.triggerPrice = `${price}`;
      payload.holdSide = this.getInverseAlgoOrderSide(opts, positionMode);
      const [planType, amount] = await this.getOrderPlanTypeAndAmount(opts);
      payload.planType = planType;
      if (amount) payload.size = `${adjust(amount, market.precision.amount)}`;
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
      payload.side = this.getInverseAlgoOrderSide(opts, positionMode);
      payload.rangeRate = `${distancePercentage}`;
    }

    return payload;
  };

  /**
   * Override placeOrderBatch to use per-symbol marginCoin
   */
  placeOrderBatch = async (payloads: Array<Record<string, any>>) => {
    const newOrderIds: string[] = [];

    // Group by symbol
    const grouped = payloads.reduce(
      (acc, payload) => {
        const key = payload.symbol;
        if (!acc[key]) acc[key] = [];
        acc[key].push(payload);
        return acc;
      },
      {} as Record<string, Array<Record<string, any>>>,
    );

    for (const [symbol, orders] of Object.entries(grouped)) {
      const marginCoin = this.getMarginCoinForSymbol(symbol);
      const marginType = await this.getOrFetchMarginType(symbol);

      // Chunk into batches of 50
      const lots = [];
      for (let i = 0; i < orders.length; i += 50) {
        lots.push(orders.slice(i, i + 50));
      }

      for (const lot of lots) {
        try {
          const {
            data: { data },
          } = await this.unlimitedXHR.post(ENDPOINTS.BATCH_ORDERS, {
            symbol,
            productType: this.apiProductType,
            marginCoin,
            marginMode: marginType,
            orderList: lot.map((o: Record<string, any>) => {
              const { symbol: _, ...rest } = o;
              return rest;
            }),
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

  /**
   * Override updateAlgoOrder to use per-symbol marginCoin
   */
  updateAlgoOrder = async ({ order, update }: UpdateOrderOpts) => {
    const market = this.store.marketsBySymbol[order.symbol];
    if (!market) throw new Error(`Market ${order.symbol} not found`);

    const marginCoin = this.getMarginCoinForSymbol(order.symbol);

    const payload: Record<string, any> = {
      orderId: order.id,
      marginCoin,
      symbol: market.id,
      productType: this.apiProductType,
    };

    if ('price' in update) {
      const price = adjust(update.price, market.precision.price);
      payload.triggerPrice = `${price}`;
    }

    if ('amount' in update && Boolean(update.amount) && +update.amount > 0) {
      const amount = adjust(update.amount, market.precision.amount);
      payload.size = `${amount}`;
    }

    if (!payload.newSize) {
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

  /**
   * Override setLeverage to use per-symbol marginCoin
   */
  setLeverage = async (symbol: string, inputLeverage: number, side?: PositionSide) => {
    const market = this.store.marketsBySymbol[symbol];
    const position =
      (side ? this.store.positions.find((p) => p.symbol === symbol && p.side === side) : undefined) ??
      this.store.positions.find((p) => p.symbol === symbol);

    if (!market) throw new Error(`Market ${symbol} not found`);
    if (!position) throw new Error(`Position ${symbol} not found`);

    const marginCoin = this.getMarginCoinForSymbol(symbol);
    const leverage = Math.min(Math.max(inputLeverage, market.limits.leverage.min), market.limits.leverage.max);

    if (position.leverage !== leverage) {
      try {
        const { data } = await this.xhr.post(ENDPOINTS.SET_LEVERAGE, {
          symbol: market.id,
          marginCoin,
          productType: this.apiProductType,
          leverage,
          ...(position.positionMode === PositionMode.Hedged
            ? { holdSide: side === PositionSide.Short ? 'short' : 'long' }
            : {}),
        });

        if (parseInt(data.code || '0') !== 0) {
          throw new Error(data.msg);
        }

        const positions = await this.fetchPositions([symbol]);
        this.store.addOrUpdatePositions(positions);
      } catch (err: any) {
        this.emitAndLogError(err, err?.response?.data?.msg || err?.message);
        throw err;
      }
    }
  };

  /**
   * Override changeMarginType to use per-symbol marginCoin
   */
  changeMarginType = async (isolated: boolean, symbol?: string) => {
    if (!symbol) {
      this.emitAndLogError('Symbol is required');
      throw new Error('Symbol is required');
    }

    const marginCoin = this.getMarginCoinForSymbol(symbol);

    try {
      const { data } = await this.xhr.post(ENDPOINTS.SET_MARGIN_MODE, {
        productType: this.apiProductType,
        marginCoin,
        marginMode: isolated ? 'isolated' : 'crossed',
        symbol,
      });
      if (parseInt(data.code || '0') !== 0) {
        throw new Error(data.msg);
      }
      const positions = await this.fetchPositions([symbol]);
      this.store.addOrUpdatePositions(positions);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.msg || err?.message);
      throw err;
    }
  };

  /**
   * Override mapOrder to convert USD quantities to crypto for inverse contracts
   * For inverse contracts:
   * - API returns quantities in USD (e.g., 1000 = $1000 worth)
   * - We convert to crypto amount for display (e.g., 0.02 BTC)
   * - Store native USD value in extra.remainingNative
   */
  mapOrder = (o: Record<string, any>): Order[] => {
    const symbol = (o.symbol || o.instId).replace(`_${this.apiProductType.toUpperCase()}`, '');
    const price = parseFloat(o.price || o.px || '0');

    // Calculate remaining in native (USD) units
    const remainingNative = subtract(
      parseFloat(o.size || o.sz || '0'),
      parseFloat(o.baseVolume || o.filledQty || o.accFillSz || '0'),
    );

    // For inverse contracts, convert USD remaining to crypto
    const market = this.store.marketsBySymbol[symbol];
    let remaining = remainingNative;

    if (market?.isInverse && price > 0) {
      // Convert USD remaining to crypto amount
      // e.g., 1000 USD / 50000 USD/BTC = 0.02 BTC
      const cryptoRemaining = remainingNative / price;
      // Round to 8 decimal places for crypto precision
      remaining = Math.round(cryptoRemaining * 1e8) / 1e8;
    }

    const order: Writable<Order> = {
      id: o.orderId || o.ordId || o.id,
      status: ORDER_STATUS[o.status || o.state || o.planStatus],
      symbol,
      type: ORDER_TYPE[o.orderType || o.ordType || o.planType],
      side: ORDER_SIDE[o.tradeSide || o.tS || o.side] || o.side,
      price,
      amount: parseFloat(o.size || o.sz || '0'),
      filled: parseFloat(o.baseVolume || o.filledQty || o.accFillSz || '0'),
      lastUpdate: virtualClock.getCurrentTimeMs(),
      reduceOnly: o.reduceOnly?.toUpperCase?.() === 'YES',
      remaining,
      timeInForce: TIME_IN_FORCE[o.force || o.timeInForce || o.tiF] || OrderTimeInForce.GoodTillCancel,
      extra: {
        remainingNative, // Native remaining (USD for inverse)
      },
      info: o,
    };

    if (o.planType) {
      order.type = ORDER_TYPE[o.planType];
      order.price = parseFloat(o.triggerPrice) || parseFloat(o.triggerPx);
      order.reduceOnly = true;

      if (order.info?.posMode === 'hedge_mode') {
        order.side = o.side === 'buy' ? OrderSide.Sell : OrderSide.Buy;
      }
    } else if (order.info?.reduceOnly === 'YES' && order.info?.posMode === 'hedge_mode') {
      order.side = o.side === 'buy' ? OrderSide.Sell : OrderSide.Buy;
    }

    const orders = [order];
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
      if (o.presetStopSurplusPrice || o.stopSurplusTriggerPrice) {
        orders.push({
          ...order,
          id: `${order.id}__take_profit`,
          side: order.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy,
          type: OrderType.TakeProfit,
          price: parseFloat(o.presetStopSurplusPrice || o.stopSurplusTriggerPrice),
        });
      }
    }
    return orders;
  };

  /**
   * Helper method to get algo order side
   */
  private getInverseAlgoOrderSide = (opts: PlaceOrderOpts, positionMode: PositionMode): string => {
    // For two-way (hedged) position mode
    if (positionMode === PositionMode.Hedged) {
      if (
        opts.type === OrderType.TakeProfit ||
        opts.type === OrderType.StopLoss ||
        opts.type === OrderType.TrailingStopLoss
      ) {
        if (opts.side === OrderSide.Buy) return 'short';
        if (opts.side === OrderSide.Sell) return 'long';
      }
    }
    // For one-way position mode
    else {
      if (
        opts.type === OrderType.TakeProfit ||
        opts.type === OrderType.StopLoss ||
        opts.type === OrderType.TrailingStopLoss
      ) {
        if (opts.side === OrderSide.Buy) return 'sell';
        if (opts.side === OrderSide.Sell) return 'buy';
      }
    }

    throw new Error(`Unknown algo order side: ${opts.type} - ${opts.side}`);
  };

  /**
   * Helper to generate UUID
   */
  private generateUuid(): string {
    return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => {
      const r = (Math.random() * 16) | 0;
      const v = c === 'x' ? r : (r & 0x3) | 0x8;
      return v.toString(16);
    });
  }
}
