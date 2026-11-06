/**
 * Polymarket Exchange Implementation
 *
 * Polymarket is a prediction market platform on Polygon.
 * This integration enables trading YES/NO outcome tokens.
 *
 * Key differences from perpetual exchanges:
 * - No leverage (always 1x)
 * - Positions are token holdings (not leveraged positions)
 * - Prices are probabilities (0.00 - 1.00)
 * - No liquidation (markets resolve to 0 or 1)
 *
 * Reference: https://docs.polymarket.com/
 */

import type { Store } from '../../../store/store.interface';
import type {
  GammaMarketInfo,
  PolymarketCredentials,
  PolymarketExchangeOptions,
  PolymarketOrderResponse,
} from './polymarket.types';
import type { TickSize } from '@polymarket/clob-client-v2';

import { ethers } from 'ethers';
import sumBy from 'lodash/sumBy';

import {
  Balance,
  Candle,
  ExchangeName,
  ExchangeOptions,
  LogSeverity,
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
  Writable,
} from '../../../types';
import { createPositionId } from '../../../utils/createPositionId';
import { loop } from '../../../utils/loop';
import { adjust } from '../../../utils/safe-math';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseExchange } from '../../../exchanges/base';
import { PolymarketAPI } from './polymarket.api';
import {
  ORDER_SIDE_MAP,
  ORDER_STATUS_MAP,
  ORDER_TYPE_MAP,
  SignatureType,
} from './polymarket.constants';
import { createSignedOrder, createWallet, signApiKeyDerivationMessage } from './polymarket.signing';
import { PolymarketPrivateWebSocket } from './polymarket.ws-private';
import { PolymarketPublicWebSocket } from './polymarket.ws-public';

const SUPPORTED_TICK_SIZES = new Set(['0.1', '0.01', '0.005', '0.0025', '0.001', '0.0001']);

export function parseGammaMarketTokens(market: GammaMarketInfo): Array<{ tokenId: string; outcome: string }> {
  if (!market.clobTokenIds || !market.outcomes) return [];

  let tokenIds: unknown;
  let outcomes: unknown;
  try {
    tokenIds = JSON.parse(market.clobTokenIds);
    outcomes = JSON.parse(market.outcomes);
  } catch {
    return [];
  }
  if (!Array.isArray(tokenIds) || !Array.isArray(outcomes) || tokenIds.length !== outcomes.length) {
    return [];
  }

  return tokenIds.flatMap((tokenId, index) => {
    const outcome = outcomes[index];
    return typeof tokenId === 'string' && typeof outcome === 'string' ? [{ tokenId, outcome }] : [];
  });
}

export function precisionFromTickSize(value: number | string | null): number {
  const tick = Number(value);
  if (!Number.isFinite(tick) || tick <= 0) return 2;
  const text = tick.toFixed(8).replace(/0+$/, '');
  return Math.max(0, text.length - text.indexOf('.') - 1);
}

function normalizeTickSize(value: number | string | null): TickSize {
  const tickSize = String(value ?? '');
  if (!SUPPORTED_TICK_SIZES.has(tickSize)) {
    throw new Error(`Unsupported Polymarket tick size: ${tickSize || 'missing'}`);
  }
  return tickSize as TickSize;
}

/**
 * Polymarket Exchange Class
 *
 * Handles all interactions with the Polymarket CLOB API.
 */
export class PolymarketExchange extends BaseExchange {
  private api: PolymarketAPI;
  private wallet: ethers.Wallet | null = null;
  private credentials: PolymarketCredentials | null = null;
  private signerType: SignatureType = SignatureType.EOA;
  private funderAddress: string | null = null;
  publicWebsocket: PolymarketPublicWebSocket;
  privateWebsocket: PolymarketPrivateWebSocket;

  // Market metadata cache (condition_id -> GammaMarketInfo)
  private marketMetadata: Map<string, GammaMarketInfo> = new Map();

  // Token ID to symbol mapping
  private tokenToSymbol: Map<string, string> = new Map();
  private symbolToToken: Map<string, string> = new Map();

  // Position cost basis tracking (token_id -> avg_price)
  private costBasis: Map<string, number> = new Map();

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    // Initialize wallet if private key provided
    if (opts.key) {
      try {
        this.wallet = createWallet(opts.key);
        this.log(`Initialized wallet: ${this.wallet.address}`, LogSeverity.Info);
      } catch (err) {
        this.log(`Failed to create wallet: ${err}`, LogSeverity.Error);
      }
    }

    // Parse Polymarket-specific options
    const polyOpts = (opts as any).polymarket as PolymarketExchangeOptions | undefined;
    if (polyOpts) {
      if (polyOpts.apiKey && polyOpts.apiSecret && polyOpts.apiPassphrase) {
        this.credentials = {
          apiKey: polyOpts.apiKey,
          secret: polyOpts.apiSecret,
          passphrase: polyOpts.apiPassphrase,
        };
      }
      if (polyOpts.signerType !== undefined) {
        this.signerType = polyOpts.signerType;
      }
      if (polyOpts.funderAddress) {
        this.funderAddress = polyOpts.funderAddress;
      }
    }

    // Create API client
    this.api = new PolymarketAPI(
      opts,
      this,
      () => this.credentials,
      () => this.wallet?.address ?? null,
    );
    this.publicWebsocket = new PolymarketPublicWebSocket(this);
    this.privateWebsocket = new PolymarketPrivateWebSocket(this, () => this.credentials);
  }

  get name(): ExchangeName {
    return 'polymarket';
  }

  dispose = () => {
    super.dispose();
  };

  /**
   * Get current position mode (always OneWay for Polymarket)
   */
  async getPositionMode(): Promise<PositionMode> {
    return PositionMode.OneWay;
  }

  /**
   * Get account information
   */
  getAccount = async () => {
    const address = this.wallet ? await this.wallet.getAddress() : 'unknown';
    return {
      userId: address,
      affiliateId: null,
    };
  };

  /**
   * Validate account credentials
   */
  validateAccount = async (): Promise<ValidateAccountResult> => {
    try {
      // Try to derive API key if we have a wallet but no credentials
      if (this.wallet && !this.credentials) {
        await this.deriveApiCredentials();
      }

      // Test by fetching balance
      if (this.credentials) {
        await this.api.getBalanceAllowance(this.signerType);
        return { error: '' };
      }

      return { error: 'No wallet configured' };
    } catch (err: any) {
      const errorMsg = err?.message || 'Failed to validate account';
      this.log(`Validation error: ${errorMsg}`, LogSeverity.Error);
      return { error: errorMsg };
    }
  };

  /**
   * Derive API credentials from wallet signature
   */
  private async deriveApiCredentials(): Promise<void> {
    if (!this.wallet) {
      throw new Error('No wallet configured');
    }

    try {
      const nonce = 0;
      const serverTimestamp = await this.api.getServerTime();
      const { address, signature, timestamp } = await signApiKeyDerivationMessage(
        this.wallet,
        nonce,
        serverTimestamp,
      );

      this.credentials = await this.api.deriveApiKey(address, nonce, signature, timestamp);
      this.log('Successfully derived API credentials', LogSeverity.Info);
    } catch (err) {
      this.log(`Failed to derive API credentials: ${err}`, LogSeverity.Error);
      throw err;
    }
  }

  /**
   * Start the exchange - fetch markets, tickers, positions
   */
  start = async () => {
    const useExternalPublicData = await this.initExternalPublicData();

    if (!useExternalPublicData) {
      // Fetch markets first
      const markets = await this.fetchMarkets();
      if (this.isDisposed) return;

      this.store.update({
        markets,
        loaded: { ...this.store.loaded, markets: true },
      });

      // Fetch tickers
      const tickers = await this.fetchTickers();
      if (this.isDisposed) return;

      this.log(`Loaded ${Math.min(tickers.length, markets.length)} Polymarket markets`);

      this.store.update({
        tickers,
        loaded: { ...this.store.loaded, tickers: true },
      });
    }

    if (this.isPublicDataOnly) {
      this.store.update({
        loaded: { ...this.store.loaded, balance: true, positions: true, orders: true },
      });
      return;
    }

    if (this.credentials) {
      this.privateWebsocket.connectAndSubscribe();
    }

    // Start the periodic tick
    await this.tick();
    if (this.isDisposed) return;

    this.log('Ready to trade on Polymarket');

    // Fetch initial orders if authenticated
    if (this.credentials) {
      const orders = await this.fetchOrders();
      if (this.isDisposed) return;

      this.store.update({
        orders,
        loaded: { ...this.store.loaded, orders: true },
      });
    }
  };

  /**
   * Periodic tick - update positions and balance
   */
  private tick = async () => {
    if (this.isDisposed) return;

    try {
      // Fetch positions if authenticated
      if (this.credentials) {
        const positions = await this.fetchAndSetPositions();
        if (this.isDisposed) return;

        // Fetch balance
        const balance = await this.fetchBalance();
        if (this.isDisposed) return;

        const upnl = positions.length > 0 ? sumBy(positions, 'unrealizedPnl') : 0;
        (balance as Writable<Balance>).upnl = upnl;

        this.store.update({
          balance,
          loaded: { ...this.store.loaded, balance: true },
        });

        // Fetch orders
        const orders = await this.fetchOrders();
        if (this.isDisposed) return;

        this.store.update({
          orders,
          loaded: { ...this.store.loaded, orders: true },
        });
      }
    } catch (err: any) {
      this.log(`Error in tick: ${err?.message}`, LogSeverity.Error);
    }

    // Schedule next tick
    if (!this.isDisposed) {
      loop(() => this.tick(), 10_000);
    }
  };

  /**
   * Fetch balance from Polymarket
   */
  private fetchBalance = async (): Promise<Balance> => {
    try {
      const balanceData = await this.api.getBalanceAllowance(this.signerType);

      const total = parseFloat(balanceData.balance) / 1e6; // pUSD has 6 decimals
      const free = total; // All balance is available for trading

      return {
        total,
        free,
        used: 0,
        upnl: 0,
      };
    } catch (err) {
      this.log(`Error fetching balance: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  /**
   * Fetch markets from Gamma API
   *
   * Polymarket markets are prediction markets with YES/NO outcomes.
   * Each market has 2 tokens (YES and NO) that can be traded.
   */
  fetchMarkets = async (): Promise<Market[]> => {
    const slugs = this.options.polymarket?.marketSlugs;
    if (slugs !== undefined && (!Array.isArray(slugs) || slugs.length === 0 || slugs.length > 100 ||
        slugs.some((slug) => typeof slug !== 'string' || !slug.trim()) || new Set(slugs).size !== slugs.length)) {
      throw new Error('Polymarket marketSlugs must contain 1-100 unique nonempty strings');
    }
    try {
      const gammaMarkets = await this.api.getGammaMarkets({ active: true, closed: false, ...(slugs !== undefined ? { slug: slugs } : {}) });

      const markets: Market[] = [];

      for (const gm of gammaMarkets) {
        // Cache market metadata
        this.marketMetadata.set(gm.conditionId, gm);

        // Gamma encodes token IDs and outcomes as parallel JSON arrays.
        for (const token of parseGammaMarketTokens(gm)) {
          const symbol = `${gm.slug}-${token.outcome}`;
          const tokenId = token.tokenId;

          // Cache token mappings
          this.tokenToSymbol.set(tokenId, symbol);
          this.symbolToToken.set(symbol, tokenId);

          markets.push({
            id: tokenId,
            symbol,
            base: gm.slug,
            quote: 'pUSD',
            active:
              gm.active &&
              !gm.closed &&
              gm.enableOrderBook !== false &&
              gm.acceptingOrders !== false,
            precision: {
              amount: 2,
              price: precisionFromTickSize(gm.orderPriceMinTickSize),
            },
            limits: {
              amount: {
                min: Number(gm.orderMinSize) || 0,
                max: 1000000,
              },
              leverage: {
                min: 1,
                max: 1, // No leverage in prediction markets
              },
            },
            contractSize: 1,
            info: {
              conditionId: gm.conditionId,
              tokenId,
              outcome: token.outcome,
              tickSize: gm.orderPriceMinTickSize,
              negRisk: gm.negRisk ?? false,
              question: gm.question,
              description: gm.description,
              endDate: gm.endDate,
              slug: gm.slug,
            },
          });
        }
      }

      this.log(`Loaded ${markets.length} markets from ${gammaMarkets.length} prediction markets`);
      return markets;
    } catch (err) {
      this.log(`Error fetching markets: ${err}`, LogSeverity.Error);
      return [];
    }
  };

  /**
   * Fetch tickers (current prices) for all markets
   */
  fetchTickers = async (): Promise<Ticker[]> => {
    try {
      const tokenIds = Array.from(this.tokenToSymbol.keys());
      if (tokenIds.length === 0) return [];

      // Fetch prices in batches
      const batchSize = 100;
      const allPrices: Record<string, number> = {};

      for (let i = 0; i < tokenIds.length; i += batchSize) {
        const batch = tokenIds.slice(i, i + batchSize);
        const prices = await this.api.getMidpoints(batch);
        Object.assign(allPrices, prices);
      }

      const tickers: Ticker[] = [];

      for (const [tokenId, price] of Object.entries(allPrices)) {
        const symbol = this.tokenToSymbol.get(tokenId);
        if (!symbol) continue;

        tickers.push({
          id: tokenId,
          symbol,
          bid: price,
          ask: price,
          last: price,
          mark: price,
          index: price,
          percentage: 0,
          fundingRate: 0,
          fundingTime: 0,
          volume: 0,
          quoteVolume: 0,
          openInterest: 0,
          info: { tokenId },
        });
      }

      return tickers;
    } catch (err) {
      this.log(`Error fetching tickers: ${err}`, LogSeverity.Error);
      return [];
    }
  };

  /**
   * Fetch positions (token holdings)
   *
   * In Polymarket, positions are token holdings.
   * We map these to Position objects with calculated cost basis.
   */
  fetchPositions = async (_symbols?: string[]): Promise<Position[]> => {
    try {
      const user = this.funderAddress || this.wallet?.address;
      if (!user) return [];
      const holdings = await this.api.getPositions(user);

      // Convert to positions
      const positions: Position[] = [];

      for (const holding of holdings) {
        const tokenId = holding.asset;
        if (!Number.isFinite(holding.size) || holding.size <= 0) continue;

        const symbol = this.tokenToSymbol.get(tokenId);
        if (!symbol) continue;

        const avgPrice = holding.avgPrice;
        this.costBasis.set(tokenId, avgPrice);

        const ticker = this.store.tickersBySymbol[symbol];
        const currentPrice = holding.curPrice || ticker?.last || avgPrice;
        const unrealizedPnl = Number.isFinite(holding.cashPnl)
          ? holding.cashPnl
          : (currentPrice - avgPrice) * holding.size;

        positions.push({
          id: createPositionId(symbol, PositionSide.Long, PositionMode.OneWay),
          symbol,
          side: PositionSide.Long, // Holding tokens is always "long"
          entryPrice: avgPrice,
          notional: holding.size * currentPrice,
          leverage: 1, // No leverage
          unrealizedPnl,
          contracts: holding.size,
          liquidationPrice: 0, // No liquidation in prediction markets
          positionMode: PositionMode.OneWay,
          info: {
            tokenId,
            conditionId: holding.conditionId,
            redeemable: holding.redeemable,
            mergeable: holding.mergeable,
          },
        });
      }

      return positions;
    } catch (err) {
      this.log(`Error fetching positions: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  /**
   * Refresh the authoritative position snapshot and commit it only after a
   * successful fetch. Private WebSocket trade events use the same path as the
   * periodic poll so a transient read failure cannot erase known positions.
   */
  fetchAndSetPositions = async (): Promise<Position[]> => {
    const positions = await this.fetchPositions();
    if (!this.isDisposed) {
      this.store.update({
        positions,
        loaded: { ...this.store.loaded, positions: true },
      });
    }
    return positions;
  };

  /**
   * Fetch open orders
   */
  fetchOrders = async (_symbols?: string[]): Promise<Order[]> => {
    try {
      const apiOrders = await this.api.getOpenOrders();

      return apiOrders.map((o) => this.mapOrder(o));
    } catch (err) {
      this.log(`Error fetching orders: ${err}`, LogSeverity.Error);
      throw err;
    }
  };

  /**
   * Map Polymarket API order to internal Order type
   */
  private mapOrder(apiOrder: any): Order {
    const symbol = this.tokenToSymbol.get(apiOrder.asset_id) || apiOrder.asset_id;

    return {
      id: apiOrder.id,
      status: (ORDER_STATUS_MAP[apiOrder.status] || apiOrder.status) as OrderStatus,
      symbol,
      type: ORDER_TYPE_MAP[apiOrder.order_type] || OrderType.Limit,
      side: ORDER_SIDE_MAP[apiOrder.side] || OrderSide.Buy,
      price: parseFloat(apiOrder.price),
      amount: parseFloat(apiOrder.original_size),
      filled: parseFloat(apiOrder.size_matched),
      remaining: parseFloat(apiOrder.original_size) - parseFloat(apiOrder.size_matched),
      reduceOnly: false,
      info: apiOrder,
      lastUpdate: virtualClock.getCurrentTimeMs(),
    };
  }

  /**
   * Place an order
   */
  placeOrder = async (opts: PlaceOrderOpts): Promise<string[]> => {
    this.resolveCallerClientOrderId(opts);
    const rejectOrder = (message: string): never => {
      this.emitAndLogError(message);
      throw new Error(message);
    };

    if (opts.type !== OrderType.Limit) {
      return rejectOrder(`Polymarket does not support ${opts.type} orders in safe-cex`);
    }
    if (
      opts.triggerPrice !== undefined ||
      opts.stopLoss !== undefined ||
      opts.takeProfit !== undefined ||
      opts.reduceOnly === true ||
      opts.close === true
    ) {
      return rejectOrder(
        'Polymarket limit orders do not support trigger, bracket, or close-only options',
      );
    }
    if (opts.timeInForce !== undefined && opts.timeInForce !== OrderTimeInForce.GoodTillCancel) {
      return rejectOrder(`Polymarket does not support ${opts.timeInForce} in safe-cex`);
    }
    if (opts.side !== OrderSide.Buy && opts.side !== OrderSide.Sell) {
      return rejectOrder(`Unsupported Polymarket order side: ${opts.side}`);
    }
    if (opts.price === undefined || !Number.isFinite(opts.price)) {
      return rejectOrder('Polymarket limit orders require a finite price');
    }
    if (!Number.isFinite(opts.amount) || opts.amount <= 0) {
      return rejectOrder('Polymarket limit orders require a positive finite amount');
    }

    if (!this.wallet || !this.credentials) {
      return rejectOrder('Wallet and credentials required for placing orders');
    }

    const tokenId = this.symbolToToken.get(opts.symbol);
    if (!tokenId) {
      return rejectOrder(`Unknown symbol: ${opts.symbol}`);
    }

    const market = this.store.marketsBySymbol[opts.symbol];
    const pAmount = market?.precision?.amount ?? 2;
    let tickSize: TickSize;
    try {
      tickSize = normalizeTickSize((market?.info as any)?.tickSize ?? null);
    } catch (err) {
      return rejectOrder(err instanceof Error ? err.message : String(err));
    }
    const price = adjust(opts.price, tickSize);
    const size = Number(opts.amount.toFixed(pAmount));
    const side = opts.side === OrderSide.Buy ? 'BUY' : 'SELL';
    const negRisk = (market?.info as any)?.negRisk === true;
    const tick = Number(tickSize);
    const minimumAmount = market?.limits?.amount?.min;

    if (price < tick || price > 1 - tick) {
      return rejectOrder(`Polymarket limit price must be between ${tick} and ${1 - tick}`);
    }
    if (size <= 0 || (minimumAmount !== undefined && size < minimumAmount)) {
      return rejectOrder(
        minimumAmount !== undefined
          ? `Polymarket order amount must be at least ${minimumAmount}`
          : 'Polymarket order amount is too small for market precision',
      );
    }

    let response: PolymarketOrderResponse;
    try {
      // Create and sign the order
      const signedOrder = await createSignedOrder(this.wallet, {
        tokenId,
        price,
        size,
        side,
        tickSize,
        negRisk,
        signatureType: this.signerType,
        funderAddress: this.funderAddress ?? undefined,
      });

      // Post the order
      response = await this.api.postOrder(signedOrder, 'GTC');
    } catch (err) {
      this.emitAndLogError(err as Error, 'Polymarket place order failed');
      throw err;
    }

    if (!response.success) {
      return rejectOrder(response.errorMsg || 'Order placement failed');
    }

    this.log(`Placed ${side} order for ${size} ${opts.symbol} @ ${price}`, LogSeverity.Info);

    return response.orderID ? [response.orderID] : [];
  };

  /**
   * Update an order (cancel and replace)
   */
  updateOrder = async (opts: UpdateOrderOpts): Promise<string[]> => {
    const { order, update } = opts;

    // Cancel the old order
    await this.api.cancelOrder(order.id);

    // Determine updated values
    const amount = 'amount' in update ? update.amount : order.amount;
    const price = 'price' in update ? update.price : order.price;

    // Place a new order with updated parameters
    return this.placeOrder({
      symbol: order.symbol,
      type: order.type,
      side: order.side,
      amount,
      price,
    });
  };

  /**
   * Cancel orders
   */
  cancelOrders = async (orders: Order[]): Promise<void> => {
    if (orders.length === 0) return;

    const orderIds = orders.map((o) => o.id);
    await this.api.cancelOrders(orderIds);

    this.log(`Cancelled ${orders.length} orders`, LogSeverity.Info);
  };

  /**
   * Cancel all orders
   */
  cancelAllOrders = async (): Promise<void> => {
    await this.api.cancelAllOrders();
    this.log('Cancelled all orders', LogSeverity.Info);
  };

  /**
   * Fetch order book for a symbol
   */
  fetchOrderBook = async (symbol: string): Promise<OrderBook> => {
    const tokenId = this.symbolToToken.get(symbol);
    if (!tokenId) {
      throw new Error(`Unknown symbol: ${symbol}`);
    }

    const book = await this.api.getOrderBook(tokenId);

    // Convert to OrderBookOrders format
    let bidTotal = 0;
    const bids = book.bids.map((b) => {
      const price = parseFloat(b.price);
      const amount = parseFloat(b.size);
      bidTotal += amount;
      return { price, amount, total: bidTotal };
    });

    let askTotal = 0;
    const asks = book.asks.map((a) => {
      const price = parseFloat(a.price);
      const amount = parseFloat(a.size);
      askTotal += amount;
      return { price, amount, total: askTotal };
    });

    return {
      bids,
      asks,
    };
  };

  /** Listen to order book updates over the official market channel. */
  listenOrderBook = (symbol: string, callback: (ob: OrderBook) => void): (() => void) => {
    const tokenId = this.symbolToToken.get(symbol);
    if (!tokenId) throw new Error(`Unknown symbol: ${symbol}`);

    const unsubscribe = this.publicWebsocket.subscribeToOrderBook(tokenId, callback);
    this.publicWebsocket.connectAndSubscribe();
    return unsubscribe;
  };

  /**
   * Fetch OHLCV data (price history)
   */
  fetchOHLCV = async (opts: OHLCVOptions): Promise<Candle[]> => {
    const tokenId = this.symbolToToken.get(opts.symbol);
    if (!tokenId) {
      return [];
    }

    const fidelityByInterval: Record<string, number> = { '1m': 1, '5m': 5, '15m': 15, '1h': 60, '4h': 240, '1d': 1440 };
    const fidelity = fidelityByInterval[opts.interval];
    if (!fidelity || opts.limit === 0 ||
        [opts.from, opts.to].some((v) => v !== undefined && (!Number.isFinite(v) || v < 0)) ||
        (opts.from !== undefined && opts.to !== undefined && opts.from > opts.to)) {
      throw new Error('Invalid Polymarket candle bounds or interval');
    }
    if (opts.from === undefined && opts.to !== undefined) {
      throw new Error('Polymarket upper-only candle bounds require from');
    }
    if (opts.from !== undefined && opts.from < 1000) {
      throw new Error('Polymarket candle from must be at least 1000 milliseconds');
    }
    const history = await this.api.getPriceHistory(
      tokenId,
      opts.from === undefined ? undefined : Math.floor(opts.from / 1000),
      opts.to === undefined ? undefined : Math.floor(opts.to / 1000),
      opts.from === undefined ? 'max' : undefined,
      fidelity,
    );
    const candles: Candle[] = history.map((h) => {
      const timestamp = Number(h.t) * 1000;
      const price = Number(h.p);
      if (!Number.isFinite(timestamp) || timestamp < 0 || !Number.isFinite(price) || price < 0 || price > 1) {
        throw new Error('Invalid Polymarket price history');
      }
      return { timestamp, open: price, high: price, low: price, close: price, volume: 0 };
    }).filter((c) => (opts.from === undefined || c.timestamp >= opts.from) && (opts.to === undefined || c.timestamp <= opts.to));
    const unique = [...new Map(candles.map((c) => [c.timestamp, c])).values()].sort((a, b) => a.timestamp - b.timestamp);
    return opts.limit === undefined ? unique : unique.slice(-opts.limit);
  };

  /**
   * Listen to OHLCV updates (polling fallback)
   */
  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void): (() => void) => {
    let isActive = true;
    let lastTimestamp = 0;

    const poll = async () => {
      if (!isActive || this.isDisposed) return;

      try {
        const candles = await this.fetchOHLCV({
          ...opts,
          from: Date.now() - 60000,
          to: Date.now(),
        });

        for (const candle of candles) {
          if (candle.timestamp > lastTimestamp) {
            lastTimestamp = candle.timestamp;
            callback(candle);
          }
        }
      } catch (err) {
        this.log(`Error in OHLCV polling: ${err}`, LogSeverity.Warning);
      }

      if (isActive && !this.isDisposed) {
        setTimeout(poll, 5000);
      }
    };

    poll();

    return () => {
      isActive = false;
    };
  };

  // ==================== No-op methods for unsupported features ====================

  /**
   * Set leverage - no-op for Polymarket (no leverage)
   */
  setLeverage = async (_symbol: string, _leverage: number): Promise<void> => {
    this.log('Leverage is not supported on Polymarket (always 1x)', LogSeverity.Warning);
  };

  /**
   * Change position mode - no-op (always OneWay)
   */
  changePositionMode = async (_hedged: boolean): Promise<void> => {
    this.log('Position mode changes not supported on Polymarket', LogSeverity.Warning);
  };

  /**
   * Change margin type - no-op (no margin in prediction markets)
   */
  changeMarginType = async (_isolated: boolean): Promise<void> => {
    this.log('Margin type changes not supported on Polymarket', LogSeverity.Warning);
  };

  /**
   * Fetch symbol max leverage - always 1
   */
  fetchSymbolMaxLeverage = async (_symbol: string): Promise<number> => {
    return 1;
  };
}
