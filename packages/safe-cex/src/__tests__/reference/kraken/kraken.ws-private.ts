import type { KrakenExchange } from './kraken.exchange';

import createHash from 'create-hash';
import createHmac from 'create-hmac';
import { nanoid } from 'nanoid';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import {
  LogSeverity,
  MarginType,
  Order,
  OrderSide,
  OrderStatus,
  OrderTimeInForce,
  OrderType,
  Position,
  PositionMode,
  PositionSide,
} from '../../../types';
import { createPositionId } from '../../../utils/createPositionId';
import { v } from '../../../utils/get-key';
import { BaseWebSocket } from '../../../exchanges/base.ws';

export class KrakenPrivateWebsocket extends BaseWebSocket<KrakenExchange> {
  authenticated = false;
  private waitingChallenge = false;
  private pendingChallenge: string | undefined;
  private originalChallenge: string | undefined;
  private signedChallenge: string | undefined;
  private ordersThrottleTimeout?: NodeJS.Timeout;

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      const key = this.parent.options.testnet ? 'testnet' : 'livenet';
      const url = `wss://${key === 'testnet' ? 'demo-' : ''}futures.kraken.com/ws/v1`;
      this.ws = this.createWebsocket(url);

      const handleClose = () => this.onClose();
      const handleError = () => {
        try {
          if (this.ws && this.ws.readyState !== WebSocket.CLOSED) this.ws.close();
        } catch (_e) {}
        handleClose();
      };

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage as any);
      this.ws.addEventListener('close', handleClose);
      this.ws.addEventListener('error', handleError);
    }
  };

  onOpen = async () => {
    if (!this.isDisposed) {
      try {
        await this.authenticate();
      } catch (e: any) {
        this.parent.log(`Kraken private WS auth failed: ${e?.message || e}`, LogSeverity.Error);
      }
    }
  };

  private authenticate = async () => {
    // Only the challenge request is sent here (api_key alone). The secret is
    // needed later in performAuth(), and hosted mode signs there via
    // remoteSigner instead — so requiring a secret up front would make the
    // remote-signer path unreachable and silently kill the private feed.
    if (!this.parent.options.key || (!this.parent.options.secret && !this.parent.options.remoteSigner)) {
      this.parent.log('Missing API key or secret/remoteSigner for authentication', LogSeverity.Warning);
      return;
    }
    try {
      const payload = { event: 'challenge', api_key: this.parent.options.key } as const;
      this.waitingChallenge = true;
      this.send(JSON.stringify(payload));
    } catch (e: any) {
      this.parent.log(`Failed to send challenge: ${e?.message || e}`, LogSeverity.Error);
      this.waitingChallenge = false;
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      const msg = typeof data === 'string' ? jsonParse(data) : jsonParse((data as any).toString?.() || '');
      const feed = msg?.feed || msg?.channel || msg?.event;
      if (!feed) return;

      if (feed === 'challenge' && msg?.message) {
        this.pendingChallenge = String(msg.message);
        this.waitingChallenge = false;
        this.performAuth();
        return;
      }

      // Handle authentication errors
      if (feed === 'error' || (feed === 'info' && msg?.message?.includes('authentication'))) {
        const errorMsg = msg?.message || JSON.stringify(msg);
        this.parent.log(`WebSocket authentication error: ${errorMsg}`, LogSeverity.Error);
        this.authenticated = false;
        return;
      }

      if (feed === 'subscribed' && msg?.feed) {
        return;
      }

      if (feed === 'fills') {
        this.handleFills(msg);
      } else if (feed === 'open_orders' || feed === 'open_orders_snapshot') {
        this.handleOpenOrders(msg);
      } else if (feed === 'open_positions' || feed === 'open_positions_snapshot') {
        this.handleOpenPositions(msg);
      } else if (feed === 'account_balances' || feed === 'account_balances_snapshot' || feed === 'balances') {
        this.handleBalances(msg);
      }
    }
  };

  private performAuth = async () => {
    if (!this.pendingChallenge) return;
    try {
      if (this.parent.options.remoteSigner) {
        const result = await this.parent.options.remoteSigner.sign({
          id: nanoid(),
          type: 'ws-auth',
          exchangeName: 'kraken',
          accountName: this.parent.options.remoteSigner.accountName,
          wsAuth: {
            toSign: this.pendingChallenge,
            outputFormat: 'kraken-challenge',
            extraData: { challenge: this.pendingChallenge },
          },
        });

        if (result.error) {
          // NOT "auth failed": `health/errorPatterns.ts` matches that literal under
          // `auth_invalid`, which red-toasts "API key rejected" and makes
          // `noteTerminalExchangeFailure` suppress automatic account reloads for five
          // minutes — the very reload that would re-pair the browser signer. A
          // backgrounded tab or a 15s signer timeout must not look like a bad key.
          // Mirrors the bybit fix in `bybit/bybit.ws-private.ts` → auth.
          this.emitAndLogError(`Remote WS signing unavailable: ${result.error}`);
          this.authenticated = false;
          return;
        }

        this.originalChallenge = this.pendingChallenge;
        this.signedChallenge = result.wsAuthSignature;
        this.pendingChallenge = undefined;
        this.authenticated = true;
        this.subscribePrivateFeeds();
        return;
      }

      // Kraken Futures WebSocket Authentication:
      // 1. SHA-256 hash the challenge string
      // 2. HMAC-SHA512 sign the hash with base64-decoded secret
      // 3. Base64-encode the signature

      // Step 1: SHA-256 hash the challenge
      const challengeHash = createHash('sha256').update(this.pendingChallenge, 'utf8').digest();

      // Step 2: Base64-decode the secret
      const secret = Buffer.from(this.parent.options.secret, 'base64');

      // Step 3: HMAC-SHA512 sign the hash with the secret
      const signature = createHmac('sha512', secret).update(challengeHash).digest('base64');

      // Store the challenge and signature for use in subscriptions
      this.originalChallenge = this.pendingChallenge;
      this.signedChallenge = signature;

      // Don't send authenticate event - auth happens when subscribing to private feeds
      this.pendingChallenge = undefined;
      this.authenticated = true;
      this.subscribePrivateFeeds();
    } catch (e: any) {
      this.parent.log(`Failed to perform auth: ${e?.message || e}`, LogSeverity.Error);
      this.authenticated = false;
    }
  };

  subscribePrivateFeeds = () => {
    try {
      // Private subscriptions require authentication credentials
      const baseSubscription = {
        event: 'subscribe',
        api_key: this.parent.options.key,
        original_challenge: this.originalChallenge,
        signed_challenge: this.signedChallenge,
      };

      // Subscribe to each private feed with authentication
      this.send(JSON.stringify({ ...baseSubscription, feed: 'fills' }));
      this.send(JSON.stringify({ ...baseSubscription, feed: 'open_orders' }));
      this.send(JSON.stringify({ ...baseSubscription, feed: 'open_positions' }));
      // NOTE: Kraken rejects this feed name ("Couldn't subscribe to invalid
      // feed") so balance arrives via REST polling instead. Do not "fix" it to
      // `balances` without reworking handleBalances — that feed's payload shape
      // parses to a zero balance, which is worse than the harmless alert.
      this.send(JSON.stringify({ ...baseSubscription, feed: 'account_balances' }));
    } catch (_e) {}
  };

  private handleFills = (msg: any) => {
    const fills = msg?.fills || msg?.executions || (Array.isArray(msg) ? msg : []);
    if (!Array.isArray(fills)) return;
    for (const f of fills) {
      const orderId = String(v(f, 'order_id') || v(f, 'orderId') || v(f, 'uid') || '');
      const qty = parseFloat(v(f, 'qty') || v(f, 'quantity') || v(f, 'size') || v(f, 'filled') || 0);
      const price = parseFloat(v(f, 'price') || v(f, 'fillPrice') || 0);
      const symbol = v(f, 'instrument') || v(f, 'symbol');

      // Kraken uses boolean "buy" field, other formats use "side"/"direction" string
      let side: OrderSide;
      if (f.buy !== undefined) {
        side = f.buy ? OrderSide.Buy : OrderSide.Sell;
      } else {
        const sideStr = String(v(f, 'side') || v(f, 'direction') || 'buy').toLowerCase();
        side = sideStr === 'buy' ? OrderSide.Buy : OrderSide.Sell;
      }
      if (orderId) {
        const existing = this.store.orders.find((o) => o.id === orderId);
        if (existing) {
          const newFilled = Math.min(existing.amount, existing.filled + qty);
          const remaining = Math.max(0, existing.amount - newFilled);
          this.store.updateOrder({ id: orderId }, { filled: newFilled, remaining });
          if (remaining === 0) {
            this.store.updateOrderStatus({ id: orderId }, OrderStatus.Closed);
          }
        }
      }
      this.parent.emitter.emit('fill', { id: orderId, amount: qty, price, side, symbol });
    }
    // Throttled fetch to ensure order state is synchronized after fills
    this.throttledFetchOrders();
  };

  private handleOpenOrders = (msg: any) => {
    // Handle direct order cancellation (just order_id at root level)
    if (msg?.is_cancel && msg?.order_id && !msg?.order) {
      // Cancel order by ID
      this.store.updateOrderStatus({ id: msg.order_id }, OrderStatus.Canceled);
      // Throttled fetch to sync after cancellation
      this.throttledFetchOrders();
      return;
    }

    // Handle both snapshot (array) and single order updates
    let ordersRaw = [];
    let isSnapshot = false;

    if (msg?.orders && Array.isArray(msg.orders)) {
      // Snapshot message with array of orders
      ordersRaw = msg.orders;
      isSnapshot = true;
    } else if (msg?.order) {
      // Single order update
      ordersRaw = [msg.order];
      isSnapshot = false;
    } else {
      return;
    }

    const orders: Order[] = ordersRaw.map((o: any) => {
      const typ = String(o.type || 'limit').toLowerCase();
      let tif = OrderTimeInForce.GoodTillCancel;
      if (typ === 'post') tif = OrderTimeInForce.PostOnly;
      else if (typ === 'ioc') tif = OrderTimeInForce.ImmediateOrCancel;
      else if (typ === 'fok') tif = OrderTimeInForce.FillOrKill;

      // Determine order type
      let orderType: OrderType;
      if (typ === 'stop' || typ === 'stp') {
        orderType = OrderType.StopLoss;
      } else if (typ === 'take_profit') {
        orderType = OrderType.TakeProfit;
      } else if (typ === 'trailing_stop') {
        orderType = OrderType.TrailingStopLoss;
      } else if (typ.includes('limit') || typ === 'lmt') {
        orderType = OrderType.Limit;
      } else {
        orderType = OrderType.Market;
      }

      // direction: 0 = buy, 1 = sell
      const side = o.direction === 0 || o.direction === 'buy' ? OrderSide.Buy : OrderSide.Sell;

      // Calculate remaining from qty and filled
      const amount = parseFloat(o.qty || o.quantity || o.size || 0);
      const filled = parseFloat(o.filled || 0);
      const remaining = Math.max(0, amount - filled);

      // Use stop_price for stop/take profit orders, otherwise use limit_price
      const price =
        orderType === OrderType.StopLoss ||
        orderType === OrderType.TakeProfit ||
        orderType === OrderType.TrailingStopLoss
          ? parseFloat(o.stop_price || o.stopPrice || 0)
          : parseFloat(o.limit_price || o.limitPrice || o.price || 0);

      return {
        id: String(o.order_id || o.orderId || o.uid || ''),
        clientOrderId: o.cli_ord_id || o.cliOrdId,
        status: OrderStatus.Open,
        symbol: o.instrument || o.symbol,
        type: orderType,
        side,
        price,
        amount,
        reduceOnly: !!o.reduce_only || !!o.reduceOnly,
        filled,
        lastUpdate: o.last_update_time || o.time || Date.now(),
        remaining,
        timeInForce: tif,
        info: o,
      };
    });

    // Handle order cancellation
    if (msg?.is_cancel) {
      orders.forEach((order) => {
        this.store.updateOrderStatus({ id: order.id }, OrderStatus.Canceled);
      });
      // Throttled fetch to sync after cancellation
      this.throttledFetchOrders();
    } else if (isSnapshot || msg?.feed === 'open_orders_snapshot') {
      // For snapshots, replace all orders
      this.store.addOrUpdateOrdersFull(orders);
    } else {
      // For single order updates, update individually to preserve other orders
      orders.forEach((order) => {
        this.store.addOrUpdateOrder(order);
      });
      // Throttled fetch to ensure synchronization after order events
      this.throttledFetchOrders();
    }
  };

  private handleOpenPositions = (msg: any) => {
    const positionsRaw = msg?.positions || msg?.openPositions || [];
    if (!Array.isArray(positionsRaw)) return;

    const positions: Position[] = positionsRaw.map((p: any) => {
      // Kraken uses 'instrument' not 'symbol'
      const symbol = p.instrument || p.symbol;

      // Balance indicates position size and side (positive = long, negative = short)
      const balance = parseFloat(p.balance || 0);
      const side = balance >= 0 ? PositionSide.Long : PositionSide.Short;
      const contracts = Math.abs(balance);

      const id = createPositionId(symbol, side);

      // Calculate notional value using mark price
      const markPrice = parseFloat(p.mark_price || 0);
      const notional = contracts * markPrice;

      // Detect margin mode based on cached preference first, then API response
      // Priority: 1) User's cached preference, 2) API maxFixedLeverage field
      const cachedMarginType = (this.parent as any).marginTypePreferences?.get(symbol);
      const maxFixedLeverage = p.maxFixedLeverage !== undefined ? parseFloat(p.maxFixedLeverage) : undefined;

      // Determine margin type:
      // - If user has set a preference, use it
      // - Otherwise detect from API: maxFixedLeverage exists -> isolated, undefined -> cross
      const marginType = cachedMarginType ?? (maxFixedLeverage !== undefined ? MarginType.Isolated : MarginType.Cross);

      // Update cache if we detected from API (for consistency)
      if (!cachedMarginType && maxFixedLeverage !== undefined) {
        (this.parent as any).marginTypePreferences?.set(symbol, MarginType.Isolated);
      } else if (!cachedMarginType && maxFixedLeverage === undefined) {
        (this.parent as any).marginTypePreferences?.set(symbol, MarginType.Cross);
      }

      // Determine leverage:
      // - For isolated: use maxFixedLeverage from position
      // - For cross: use cached setting or get max from market
      // Don't use effective_leverage as it's a calculated dynamic value (Position Value / Portfolio Value)
      let leverage: number;
      if (marginType === MarginType.Isolated && maxFixedLeverage !== undefined) {
        leverage = maxFixedLeverage;
      } else {
        // Cross margin: use cached setting or get from market limits
        const cachedLeverage = this.parent.leverageSettings?.get(symbol);
        if (cachedLeverage !== undefined) {
          leverage = cachedLeverage;
        } else {
          // Get max leverage from market
          // Use O(1) hashmap lookup instead of O(n) find
          const market = this.store.marketsBySymbol[symbol];
          leverage = market?.limits.leverage.max ?? 5;
        }
      }

      return {
        id,
        symbol,
        side,
        entryPrice: parseFloat(p.entry_price || p.price || 0),
        notional,
        leverage,
        unrealizedPnl: parseFloat(p.pnl || p.unrealized_funding || 0),
        contracts,
        liquidationPrice: parseFloat(p.liquidation_threshold || 0),
        marginType,
        positionMode: PositionMode.OneWay, // Kraken only supports one-way mode
        info: p,
      } as Position;
    });

    // Only include positions with non-zero contracts
    const activePositions = positions.filter((p) => p.contracts > 0);
    this.store.addOrUpdatePositionsFull(activePositions);
  };

  private handleBalances = (msg: any) => {
    const accounts = msg?.accounts || msg?.balances || msg;
    if (!accounts || typeof accounts !== 'object') return;

    // WebSocket balance updates use similar structure to REST API
    // but may come in different formats depending on the feed type
    let totalBalance = 0;
    let freeBalance = 0;
    let usedBalance = 0;
    let totalUpnl = 0;
    const currencies: Record<string, any> = {};

    // Check if this is a structured account update with cash/flex accounts
    const cashAccount = accounts.cash || accounts.cashAccount;
    const flexAccount = accounts.flex || accounts.multiCollateral;

    // Process cash account if present
    if (cashAccount?.balances) {
      for (const [currency, balance] of Object.entries(cashAccount.balances)) {
        const amount = parseFloat(String(balance || 0));
        if (amount > 0) {
          currencies[currency.toUpperCase()] = {
            free: amount,
            used: 0,
            total: amount,
            upnl: 0,
            info: { balance: amount },
          };

          const upperCurrency = currency.toUpperCase();
          if (upperCurrency === 'USD' || upperCurrency === 'USDT' || upperCurrency === 'USDC') {
            totalBalance += amount;
            freeBalance += amount;
          }
        }
      }
    }

    // Process flex account if present
    if (flexAccount && (flexAccount.type === 'multiCollateralMarginAccount' || flexAccount.portfolioValue)) {
      const portfolioValue = parseFloat(flexAccount.portfolioValue || 0);
      const availableMargin = parseFloat(flexAccount.availableMargin || 0);
      const balanceValue = parseFloat(flexAccount.balanceValue || 0);
      const pnl = parseFloat(flexAccount.pnl || 0);
      const initialMargin = parseFloat(flexAccount.initialMargin || 0);
      const maintenanceMargin = parseFloat(flexAccount.maintenanceMargin || 0);

      // Parse individual currencies in flex account
      if (flexAccount.currencies) {
        for (const [currency, details] of Object.entries(flexAccount.currencies)) {
          if (typeof details === 'object' && details !== null) {
            const currDetails = details as any;
            const quantity = parseFloat(currDetails.quantity || 0);
            const value = parseFloat(currDetails.value || 0);
            const available = parseFloat(currDetails.available || 0);
            const collateral = parseFloat(currDetails.collateral || 0);

            if (quantity > 0 || value > 0) {
              currencies[currency.toUpperCase()] = {
                free: available,
                used: quantity - available,
                total: quantity,
                upnl: 0,
                value: value,
                collateral: collateral,
                info: currDetails,
              };
            }
          }
        }
      }

      // Set overall balance based on portfolio value
      if (portfolioValue > 0) {
        const used = initialMargin + maintenanceMargin;
        const free = Math.max(0, availableMargin);

        totalBalance = portfolioValue;
        freeBalance = free;
        usedBalance = used;
        totalUpnl = pnl;
      } else if (balanceValue > 0) {
        totalBalance = balanceValue;
        freeBalance = availableMargin;
        usedBalance = initialMargin + maintenanceMargin;
        totalUpnl = pnl;
      }
    }

    // If no structured accounts, try to parse as flat balance update
    if (!cashAccount && !flexAccount) {
      for (const [_accountName, account] of Object.entries(accounts)) {
        if (typeof account === 'object' && account !== null) {
          // Check for auxiliary data (margin account format)
          const aux = (account as any).auxiliary;
          if (aux) {
            const pv = parseFloat(aux.pv || aux.portfolioValue || 0);
            const af = parseFloat(aux.af || aux.availableMargin || 0);
            const pnl = parseFloat(aux.pnl || 0);

            if (pv > 0) {
              const currency = aux.currency || 'USD';
              if (!currencies[currency]) {
                currencies[currency] = {
                  free: 0,
                  used: 0,
                  total: 0,
                  upnl: 0,
                  info: {},
                };
              }

              currencies[currency].total += pv;
              currencies[currency].free += af;
              currencies[currency].used += pv - af;
              currencies[currency].upnl += pnl;

              if (currency === 'USD') {
                totalBalance += pv;
                freeBalance += af;
                usedBalance += pv - af;
                totalUpnl += pnl;
              }
            }
          } else if ((account as any).balances) {
            // Handle nested balances structure
            for (const [curr, bal] of Object.entries((account as any).balances)) {
              const amount = parseFloat(String(bal || 0));
              if (amount > 0) {
                currencies[curr.toUpperCase()] = {
                  free: amount,
                  used: 0,
                  total: amount,
                  upnl: 0,
                  info: { balance: amount },
                };
              }
            }
          }
        }
      }
    }

    // Create balance object
    const balance = {
      free: freeBalance,
      used: usedBalance,
      total: totalBalance,
      upnl: totalUpnl,
      currencies,
      info: accounts,
    };

    this.store.update({ balance });
  };

  private throttledFetchOrders = () => {
    if (this.ordersThrottleTimeout) {
      clearTimeout(this.ordersThrottleTimeout);
    }
    this.ordersThrottleTimeout = setTimeout(this.fetchThrottledOrders, 1000);
  };

  private fetchThrottledOrders = async () => {
    this.ordersThrottleTimeout = undefined;
    if (!this.isDisposed) {
      try {
        const orders = await this.parent.fetchOrders();
        this.parent.store.update({
          orders,
          loaded: {
            ...this.parent.store.loaded,
            orders: true,
          },
        });
      } catch {
        // Silent failure, will retry on next event
      }
    }
  };
}
