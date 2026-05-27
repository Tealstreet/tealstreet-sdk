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

import type { Store } from '../../store/store.interface';
import type { GammaMarketInfo, PolymarketCredentials, PolymarketExchangeOptions } from './polymarket.types';

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
import { loop } from '../../utils/loop';
import { adjust } from '../../utils/safe-math';
import { virtualClock } from '../../utils/virtualClock';
import { BaseExchange } from '../base';
import { PolymarketAPI } from './polymarket.api';
import {
  ORDER_SIDE_MAP,
  ORDER_STATUS_MAP,
  ORDER_TYPE_MAP,
  ORDER_TYPE_REVERSE,
  SignatureType,
} from './polymarket.constants';
import { createSignedOrder, createWallet, signApiKeyDerivationMessage } from './polymarket.signing';

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
    this.api = new PolymarketAPI(opts, this, () => this.credentials);
  }

  get name(): ExchangeName {
    return 'polymarket';
  }

  dispose = () => {
    this.isDisposed = true;
    // WebSockets will be disposed here when implemented
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
  validateAccount = async (): Promise<string> => {
    try {
      // Try to derive API key if we have a wallet but no credentials
      if (this.wallet && !this.credentials) {
        await this.deriveApiCredentials();
      }

      // Test by fetching balance
      if (this.credentials) {
        await this.api.getBalanceAllowance();
        return '';
      }

      return 'No wallet configured';
    } catch (err: any) {
      const errorMsg = err?.message || 'Failed to validate account';
      this.log(`Validation error: ${errorMsg}`, LogSeverity.Error);
      return errorMsg;
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
      const nonce = Math.floor(Math.random() * 1000000);
      const { address, signature, timestamp } = await signApiKeyDerivationMessage(this.wallet, nonce);

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
        const positions = await this.fetchPositions();
        if (this.isDisposed) return;

        this.store.update({
          positions,
          loaded: { ...this.store.loaded, positions: true },
        });

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
      const balanceData = await this.api.getBalanceAllowance();

      const total = parseFloat(balanceData.balance) / 1e6; // USDC has 6 decimals
      const free = total; // All balance is available for trading

      return {
        total,
        free,
        used: 0,
        upnl: 0,
      };
    } catch (err) {
      this.log(`Error fetching balance: ${err}`, LogSeverity.Error);
      return { total: 0, free: 0, used: 0, upnl: 0 };
    }
  };

  /**
   * Fetch markets from Gamma API
   *
   * Polymarket markets are prediction markets with YES/NO outcomes.
   * Each market has 2 tokens (YES and NO) that can be traded.
   */
  fetchMarkets = async (): Promise<Market[]> => {
    try {
      const gammaMarkets = await this.api.getGammaMarkets({ active: true });

      const markets: Market[] = [];

      for (const gm of gammaMarkets) {
        // Cache market metadata
        this.marketMetadata.set(gm.conditionId, gm);

        // Create a market for each token (YES/NO)
        for (const token of gm.tokens || []) {
          const symbol = `${gm.slug}-${token.outcome}`;
          const tokenId = token.token_id;

          // Cache token mappings
          this.tokenToSymbol.set(tokenId, symbol);
          this.symbolToToken.set(symbol, tokenId);

          markets.push({
            id: tokenId,
            symbol,
            base: gm.slug,
            quote: 'USDC',
            active: gm.active && !gm.closed,
            precision: {
              amount: 2, // Shares precision
              price: 4, // Price precision (0.0001)
            },
            limits: {
              amount: {
                min: parseFloat(gm.tokens?.[0]?.price || '0.01') || 0.01,
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
      // Get trades to calculate cost basis
      const trades = await this.api.getTrades();

      // Calculate cost basis from trades
      const holdingsMap = new Map<string, { size: number; cost: number }>();

      for (const trade of trades) {
        const tokenId = trade.asset_id;
        const size = parseFloat(trade.size);
        const price = parseFloat(trade.price);
        const isBuy = trade.side === 'BUY';

        const existing = holdingsMap.get(tokenId) || { size: 0, cost: 0 };

        if (isBuy) {
          existing.cost += size * price;
          existing.size += size;
        } else {
          // For sells, reduce position and cost basis proportionally
          if (existing.size > 0) {
            const avgPrice = existing.cost / existing.size;
            existing.cost -= size * avgPrice;
            existing.size -= size;
          }
        }

        holdingsMap.set(tokenId, existing);
      }

      // Convert to positions
      const positions: Position[] = [];

      for (const [tokenId, holding] of holdingsMap) {
        if (holding.size <= 0) continue;

        const symbol = this.tokenToSymbol.get(tokenId);
        if (!symbol) continue;

        const avgPrice = holding.cost / holding.size;
        this.costBasis.set(tokenId, avgPrice);

        // Get current price
        const ticker = this.store.tickersBySymbol[symbol];
        const currentPrice = ticker?.last || avgPrice;

        // Calculate unrealized PnL
        const unrealizedPnl = (currentPrice - avgPrice) * holding.size;

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
            costBasis: holding.cost,
          },
        });
      }

      return positions;
    } catch (err) {
      this.log(`Error fetching positions: ${err}`, LogSeverity.Error);
      return [];
    }
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
      return [];
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
    if (!this.wallet || !this.credentials) {
      throw new Error('Wallet and credentials required for placing orders');
    }

    const tokenId = this.symbolToToken.get(opts.symbol);
    if (!tokenId) {
      throw new Error(`Unknown symbol: ${opts.symbol}`);
    }

    const market = this.store.marketsBySymbol[opts.symbol];
    const pPrice = market?.precision?.price || 4;
    const pAmount = market?.precision?.amount || 2;

    const price = opts.price ? adjust(opts.price, pPrice) : 0.5;
    const size = adjust(opts.amount, pAmount);
    const side = opts.side === OrderSide.Buy ? 'BUY' : 'SELL';

    // Create and sign the order
    const signedOrder = await createSignedOrder(this.wallet, {
      tokenId,
      price,
      size,
      side,
      signatureType: this.signerType,
    });

    // Determine order type
    const orderType = ORDER_TYPE_REVERSE[opts.type] || 'GTC';

    // Post the order
    const response = await this.api.postOrder(signedOrder, orderType as any);

    if (!response.success) {
      throw new Error(response.errorMsg || 'Order placement failed');
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

  /**
   * Listen to order book updates (placeholder for WebSocket implementation)
   */
  listenOrderBook = (symbol: string, callback: (ob: OrderBook) => void): (() => void) => {
    // Polling fallback until WebSocket is implemented
    let isActive = true;

    const poll = async () => {
      if (!isActive || this.isDisposed) return;

      try {
        const orderBook = await this.fetchOrderBook(symbol);
        callback(orderBook);
      } catch (err) {
        this.log(`Error polling order book: ${err}`, LogSeverity.Warning);
      }

      if (isActive && !this.isDisposed) {
        setTimeout(poll, 2000);
      }
    };

    poll();

    return () => {
      isActive = false;
    };
  };

  /**
   * Fetch OHLCV data (price history)
   */
  fetchOHLCV = async (opts: OHLCVOptions): Promise<Candle[]> => {
    const tokenId = this.symbolToToken.get(opts.symbol);
    if (!tokenId) {
      return [];
    }

    try {
      const history = await this.api.getPriceHistory(
        tokenId,
        opts.from,
        opts.to,
        undefined,
        60, // 1 minute fidelity
      );

      return history.map((h) => ({
        timestamp: h.t * 1000,
        open: parseFloat(h.p),
        high: parseFloat(h.p),
        low: parseFloat(h.p),
        close: parseFloat(h.p),
        volume: 0,
      }));
    } catch (err) {
      this.log(`Error fetching OHLCV: ${err}`, LogSeverity.Warning);
      return [];
    }
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
          from: Math.floor((Date.now() - 60000) / 1000),
          to: Math.floor(Date.now() / 1000),
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
