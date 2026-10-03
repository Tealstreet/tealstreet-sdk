import type { BitmexExchange } from './bitmex.exchange';

import createHmac from 'create-hmac';
import { nanoid } from 'nanoid';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { ENABLE_RECONNECTING_WEBSOCKET } from '../../config/config';
import { resolveExchangeRouting } from '../../routing/resolveExchangeRouting';
import { OrderSide, Position } from '../../types';
import { vn } from '../../utils/get-key';
import { virtualClock } from '../../utils/virtualClock';
import { BaseWebSocket } from '../base.ws';
import { BASE_WS_URL } from './bitmex.types';
import { BITMEX_ORDER, BITMEX_POSITION, ORDER_WS_UPDATE, POSITION_WS_UPDATE } from './types';

export class BitmexPrivateWebsocket extends BaseWebSocket<BitmexExchange> {
  private orderFillCumQtyById = new Map<string, number>();
  private authRefreshTimeoutId?: NodeJS.Timeout;
  private lastSignatureErrorAt = 0;
  // Private topics are subscribed once per socket connection. The 30-min auth
  // refresh re-signs only — re-sending `subscribe` on the still-connected,
  // already-subscribed socket makes BitMEX reply "already subscribed to
  // topic:…" (suppressed from users but log-noisy, and repeated WS command
  // errors can eventually get the socket dropped). Reset to false in onOpen so
  // a fresh/reconnected socket re-subscribes.
  private privateSubscriptionsSent = false;

  private static readonly MAX_TRACKED_ORDER_CUM_QTY = 2_000;
  private static readonly WS_AUTH_REFRESH_MS = 30 * 60 * 1000;
  private static readonly SIGNATURE_ERROR_COOLDOWN_MS = 60_000;
  private orderFillCumQtyOrder: string[] = [];

  constructor(parent: BitmexExchange) {
    super(parent);
    const baseOnClose = this.onClose;
    this.onClose = (event?: any): void => {
      this.clearAuthRefreshTimeout();
      baseOnClose(event);
    };
  }

  get endpoint() {
    return BASE_WS_URL.private[this.parent.options.testnet ? 'testnet' : 'livenet'];
  }

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      this.clearAuthRefreshTimeout();
      const route = resolveExchangeRouting(this.parent.name, this.parent.options).wsPrivate;
      this.ws = this.createWebsocket(route.tunnelled ? route.host! : this.endpoint);
      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    }
  };

  override dispose() {
    this.clearAuthRefreshTimeout();
    super.dispose();
  }

  private clearAuthRefreshTimeout = () => {
    if (this.authRefreshTimeoutId) {
      clearTimeout(this.authRefreshTimeoutId);
      this.authRefreshTimeoutId = undefined;
    }
  };

  private scheduleNextAuthRefresh = () => {
    this.clearAuthRefreshTimeout();
    if (this.isDisposed) return;
    this.authRefreshTimeoutId = setTimeout(() => {
      // Refresh re-auths only — do NOT re-subscribe (already subscribed on this socket).
      this.sendAuth();
    }, BitmexPrivateWebsocket.WS_AUTH_REFRESH_MS);
  };

  private sendPrivateSubscriptions = () => {
    this.sendWithLog(
      JSON.stringify({
        op: 'subscribe',
        args: ['execution', 'margin', 'csastate', 'order', 'position'],
      }),
    );
    this.privateSubscriptionsSent = true;
  };

  onOpen = () => {
    if (!this.isDisposed) {
      // Fresh socket — clear the flag so the initial auth re-subscribes.
      this.privateSubscriptionsSent = false;
      this.sendAuth({ subscribe: true });
      this.ping();
      if (ENABLE_RECONNECTING_WEBSOCKET) {
        this.parent.fetchAndSetPrivateSnapshot();
      }
    }
  };

  handleOrder = (_json: any) => {
    this.recordMessageHandlerStats('order');
    const json = _json as any as ORDER_WS_UPDATE;
    const ordersMap = new Map<string, BITMEX_ORDER>();
    for (const order of json.data) {
      ordersMap.set(order.orderID, order);
      if (json.action !== 'partial') {
        // {
        //   "orderID": "5c436c6b-9045-4f39-b074-152887b60612",
        //   "account": 2232361,
        //   "symbol": "XRPUSDT",
        //   "side": "Buy",
        //   "orderQty": 191000,
        //   "price": 0.5223,
        //   "currency": "USDT",
        //   "settlCurrency": "USDt",
        //   "ordType": "Limit",
        //   "timeInForce": "GoodTillCancel",
        //   "ordStatus": "PartiallyFilled",
        //   "workingIndicator": true,
        //   "leavesQty": 90000,
        //   "cumQty": 101000,
        //   "avgPx": 0.5223,
        //   "text": "Tealstreet",
        //   "transactTime": "2024-05-18T07:06:44.811Z",
        //   "timestamp": "2024-05-18T07:06:44.811Z"
        // }
        if (order.ordStatus === 'Filled' || order.ordStatus === 'PartiallyFilled') {
          this.emitIncrementalFill(order);
        } else if (
          order.ordStatus === 'Canceled' &&
          order.text.includes('Canceled: Order had execInst of ParticipateDoNotInitiate')
        ) {
          this.parent.emitter.emit('warning', 'Canceled: Order had execInst of ParticipateDoNotInitiate');
        } else if (
          order.ordStatus === 'Canceled' &&
          order.text.includes('Canceled: Order had execInst of Close or ReduceOnly but current position is 0')
        ) {
          this.parent.emitter.emit(
            'warning',
            'Canceled: Order had execInst of Close or ReduceOnly but current position is 0',
          );
        }
      } else if (order.orderID) {
        const cumQty = parseFloat(`${vn(order, 'cumQty') ?? 0}`);
        if (Number.isFinite(cumQty) && cumQty >= 0) {
          this.setTrackedCumQty(this.orderFillCumQtyById, this.orderFillCumQtyOrder, order.orderID, cumQty);
        }
      }
    }

    const prevBitmexOrders = this.store.orders.map((o) => o.info);

    for (let i = 0; i < prevBitmexOrders.length; i++) {
      if (ordersMap.has(prevBitmexOrders[i].orderID)) {
        ordersMap.set(prevBitmexOrders[i].orderID, {
          ...prevBitmexOrders[i],
          ...ordersMap.get(prevBitmexOrders[i].orderID)!,
        });
      }
    }

    if (ordersMap.size > 0) {
      this.parent.store.addOrUpdateOrders([...ordersMap.values()].map(this.parent.mapOrder));
    }
  };

  handlePosition = (_json: any) => {
    const json = _json as any as POSITION_WS_UPDATE;
    const positionsMap = new Map<string, BITMEX_POSITION>();
    const rawPositions = json.data.filter(this.parent.filterBitmexPosition);
    for (const position of rawPositions) {
      positionsMap.set(position.symbol, position);
    }

    const prevBitmexPositions = this.store.positions.map((p) => p.info);
    for (let i = 0; i < prevBitmexPositions.length; i++) {
      if (positionsMap.has(prevBitmexPositions[i]?.symbol)) {
        const prevBitmexPosition = prevBitmexPositions[i];
        const update = positionsMap.get(prevBitmexPosition.symbol);
        const combinedUpdate = {
          ...prevBitmexPosition,
          ...update,
        };
        positionsMap.set(prevBitmexPosition.symbol, combinedUpdate);
      }
    }

    const parsedPositions = [...positionsMap.values()]
      .map(this.parent.mapPosition)
      .filter<Position>((position): position is Position => !!position);

    if (json.action === 'partial') {
      this.parent.store.addOrUpdatePositionsFull(parsedPositions);
    } else {
      this.parent.store.addOrUpdatePositions(parsedPositions);
    }
    this.parent.store.update({
      loaded: { ...this.store.loaded, positions: true },
    });
  };

  private setTrackedCumQty = (map: Map<string, number>, order: string[], orderId: string, value: number) => {
    if (!map.has(orderId)) {
      order.push(orderId);
      if (order.length > BitmexPrivateWebsocket.MAX_TRACKED_ORDER_CUM_QTY) {
        const oldest = order.shift();
        if (oldest) {
          map.delete(oldest);
        }
      }
    }
    map.set(orderId, value);
  };

  private emitIncrementalFill = (order: BITMEX_ORDER) => {
    if (!order.orderID || !order.symbol) return;

    const currentCumQtyRaw = parseFloat(`${vn(order, 'cumQty') ?? 0}`);
    if (!Number.isFinite(currentCumQtyRaw) || currentCumQtyRaw <= 0) return;

    const previousCumQtyRaw = this.orderFillCumQtyById.get(order.orderID) ?? 0;
    if (currentCumQtyRaw < previousCumQtyRaw) {
      this.setTrackedCumQty(this.orderFillCumQtyById, this.orderFillCumQtyOrder, order.orderID, currentCumQtyRaw);
      return;
    }
    if (currentCumQtyRaw <= previousCumQtyRaw) {
      return;
    }

    const deltaCumQtyRaw = currentCumQtyRaw - previousCumQtyRaw;
    if (!Number.isFinite(deltaCumQtyRaw) || deltaCumQtyRaw <= 0) {
      return;
    }

    const market = this.store.markets.find((m) => m.symbol === order.symbol);
    const amount =
      this.parent.convertOutputsFromMicro && market
        ? Math.abs(this.parent.convertFromMicroValue(deltaCumQtyRaw, market))
        : Math.abs(deltaCumQtyRaw);
    if (!Number.isFinite(amount) || amount <= 0) return;

    const price = parseFloat(`${vn(order, 'avgPx') ?? vn(order, 'price') ?? 0}`);
    if (!Number.isFinite(price) || price <= 0) return;

    const side = order.side === 'Buy' ? OrderSide.Buy : order.side === 'Sell' ? OrderSide.Sell : undefined;
    if (!side) return;

    this.setTrackedCumQty(this.orderFillCumQtyById, this.orderFillCumQtyOrder, order.orderID, currentCumQtyRaw);

    this.parent.emitter.emit('fill', {
      id: `${order.orderID}:${currentCumQtyRaw}`,
      orderId: order.orderID,
      side,
      symbol: order.symbol,
      price,
      amount,
    });
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.sendWithLog('ping');
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      this.emitWsDiagnosticReceive(data);

      if (data === 'pong') {
        this.handlePongEvent();
        return;
      }

      const json = jsonParse(data);

      if (json?.error) {
        const errorMessage = String(json.error);
        const errorMessageLower = errorMessage.toLowerCase();
        const isAlreadySubscribedError = errorMessageLower.includes('already');
        const isSignatureError = errorMessageLower.includes('signature not valid');

        if (!isAlreadySubscribedError) {
          if (isSignatureError) {
            const now = Date.now();
            if (now - this.lastSignatureErrorAt >= BitmexPrivateWebsocket.SIGNATURE_ERROR_COOLDOWN_MS) {
              this.lastSignatureErrorAt = now;
              this.parent.emitter.emit('error', errorMessage);
            }
          } else {
            this.parent.emitter.emit('error', errorMessage);
          }
        }

        if (!isSignatureError || Date.now() - this.lastSignatureErrorAt <= 1000) {
          console.warn('BitmexPrivateWebsocket error:', errorMessage);
        }
      }

      switch (json?.table) {
        // case 'position':
        // {
        //   table: 'position',
        //   action: 'update',
        //   data: [
        //     {
        //       account: 1515719,
        //       symbol: 'ETHUSD',
        //       currency: 'XBt',
        //       currentQty: 560,
        //       markPrice: 3404.56,
        //       markValue: 190655360,
        //       riskValue: 190655360,
        //       foreignNotional: -117615.89142504627,
        //       maintMargin: 2961435,
        //       unrealisedPnl: 920360,
        //       unrealisedPnlPcnt: 0.0049,
        //       unrealisedRoePcnt: 0.4851,
        //       liquidationPrice: 2532.25,
        //       timestamp: '2024-03-03T10:18:05.714Z',
        //     },
        //   ],
        // };
        // {
        //   table: 'position',
        //   action: 'partial',
        //   keys: ['account', 'symbol'],
        //   types: {
        //     account: 'long',
        //     symbol: 'symbol',
        //     currency: 'symbol',
        //     underlying: 'symbol',
        //     quoteCurrency: 'symbol',
        //     commission: 'float',
        //     initMarginReq: 'float',
        //     maintMarginReq: 'float',
        //     riskLimit: 'long',
        //     leverage: 'float',
        //     crossMargin: 'boolean',
        //     deleveragePercentile: 'float',
        //     rebalancedPnl: 'long',
        //     prevRealisedPnl: 'long',
        //     prevUnrealisedPnl: 'long',
        //     openingQty: 'long',
        //     openOrderBuyQty: 'long',
        //     openOrderBuyCost: 'long',
        //     openOrderBuyPremium: 'long',
        //     openOrderSellQty: 'long',
        //     openOrderSellCost: 'long',
        //     openOrderSellPremium: 'long',
        //     currentQty: 'long',
        //     currentCost: 'long',
        //     currentComm: 'long',
        //     realisedCost: 'long',
        //     unrealisedCost: 'long',
        //     grossOpenPremium: 'long',
        //     isOpen: 'boolean',
        //     markPrice: 'float',
        //     markValue: 'long',
        //     riskValue: 'long',
        //     homeNotional: 'float',
        //     foreignNotional: 'float',
        //     posState: 'symbol',
        //     posCost: 'long',
        //     posCross: 'long',
        //     posComm: 'long',
        //     posLoss: 'long',
        //     posMargin: 'long',
        //     posMaint: 'long',
        //     initMargin: 'long',
        //     maintMargin: 'long',
        //     realisedPnl: 'long',
        //     unrealisedPnl: 'long',
        //     unrealisedPnlPcnt: 'float',
        //     unrealisedRoePcnt: 'float',
        //     avgCostPrice: 'float',
        //     avgEntryPrice: 'float',
        //     breakEvenPrice: 'float',
        //     marginCallPrice: 'float',
        //     liquidationPrice: 'float',
        //     bankruptPrice: 'float',
        //     timestamp: 'timestamp',
        //   },
        //   filter: { account: 2169568 },
        //   data: [
        //     {
        //       account: 2169568,
        //       symbol: 'XBTUSDT',
        //       currency: 'USDt',
        //       underlying: 'XBT',
        //       quoteCurrency: 'USDT',
        //       commission: 7.5e-4,
        //       initMarginReq: 0.01,
        //       maintMarginReq: 0.005,
        //       riskLimit: 1000000000000,
        //       leverage: 100.0,
        //       crossMargin: true,
        //       deleveragePercentile: 1.0,
        //       rebalancedPnl: 1684118,
        //       prevRealisedPnl: 0,
        //       prevUnrealisedPnl: 0,
        //       openingQty: 1000,
        //       openOrderBuyQty: 0,
        //       openOrderBuyCost: 0,
        //       openOrderBuyPremium: 0,
        //       openOrderSellQty: 0,
        //       openOrderSellCost: 0,
        //       openOrderSellPremium: 0,
        //       currentQty: 1000,
        //       currentCost: 63460990,
        //       currentComm: 128560,
        //       realisedCost: 0,
        //       unrealisedCost: 63460990,
        //       grossOpenPremium: 0,
        //       isOpen: true,
        //       markPrice: 61721.97,
        //       markValue: 61721970,
        //       riskValue: 61721970,
        //       homeNotional: 0.001,
        //       foreignNotional: -61.72197,
        //       posCost: 61346500,
        //       posCross: 2193152,
        //       posComm: 48116,
        //       posLoss: 454132,
        //       posMargin: 2400601,
        //       posMaint: 397608,
        //       initMargin: 0,
        //       maintMargin: 661581,
        //       realisedPnl: -128560,
        //       unrealisedPnl: -1739020,
        //       unrealisedPnlPcnt: 0.0061,
        //       unrealisedRoePcnt: 0.612,
        //       avgCostPrice: 61346.5,
        //       avgEntryPrice: 63460.99,
        //       breakEvenPrice: 61905.5,
        //       marginCallPrice: 41596.0,
        //       liquidationPrice: 41596.0,
        //       bankruptPrice: 41246.5,
        //       timestamp: '2024-03-03T10:24:20.723Z',
        //     }
        //   ],
        // };
        // const positionsMap = new Map<string, Record<string, any>>();
        // for (const position of json.data) {
        //   positionsMap.set(position.symbol, position);
        // }
        // for (let i = 0; i < this.parent.positionsSnapshot.length; i++) {
        //   if (positionsMap.has(this.parent.positionsSnapshot[i].symbol)) {
        //     this.parent.positionsSnapshot[i] = {
        //       ...this.parent.positionsSnapshot[i],
        //       ...positionsMap.get(this.parent.positionsSnapshot[i].symbol)!,
        //     };
        //   }
        // }
        // for (const position of this.parent.positionsSnapshot.map(this.parent.mapPosition)) {
        //   if (position) {
        //     this.parent.store.updatePosition({ symbol: position.symbol, side: position.side }, position);
        //   }
        // }
        // break;
        case 'execution':
          if (Array.isArray(json.data)) {
            for (const execution of json.data) {
              this.parent.trackBitmexMaxSlippagePct(execution?.maxSlippagePct);
            }
          }
          break;
        case 'margin':
          // {
          //   table: 'margin',
          //   action: 'update',
          //   data: [
          //     {
          //       account: 2169568,
          //       currency: 'USDt',
          //       grossMarkValue: 129814880,
          //       riskValue: 129814880,
          //       maintMargin: 2058630,
          //       unrealisedPnl: -3990370,
          //       marginBalance: 21890400,
          //       marginLeverage: 5.930219639659394,
          //       marginUsedPcnt: 0.094,
          //       excessMargin: 19831770,
          //       availableMargin: 19831770,
          //       withdrawableMargin: 19831770,
          //       timestamp: '2024-03-03T10:18:15.703Z',
          //     },
          //     {
          //       account: 2169568,
          //       currency: 'XBt',
          //       grossMarkValue: 971982,
          //       riskValue: 1144396,
          //       unrealisedPnl: -26370,
          //       marginBalance: 121394,
          //       marginLeverage: 8.0068372407203,
          //       marginUsedPcnt: 0.1026,
          //       excessMargin: 108934,
          //       availableMargin: 108934,
          //       withdrawableMargin: 108934,
          //       timestamp: '2024-03-03T10:18:15.703Z',
          //     },
          //   ],
          // };

          const combinedUpdates = [...(this.store.balance?.info || [])];

          for (const update of json.data) {
            const updateCurrency = String(update?.currency || '').toUpperCase();
            const prevBalanceIndex = combinedUpdates.findIndex(
              (b: any) => String(b?.currency || '').toUpperCase() === updateCurrency,
            );

            if (prevBalanceIndex > -1) {
              combinedUpdates[prevBalanceIndex] = {
                ...combinedUpdates[prevBalanceIndex],
                ...update,
              };
            } else {
              combinedUpdates.push(update);
            }
          }

          this.recordMessageHandlerStats('balance');
          this.parent.store.update({
            balance: this.parent.parseBalanceSnapshot(combinedUpdates),
            loaded: { ...this.store.loaded, balance: true },
          });
          break;
        case 'order':
          this.handleOrder(json);
          break;
        case 'position':
          this.handlePosition(json);
          break;
      }
    }
  };

  private sendAuth = async ({ subscribe = false }: { subscribe?: boolean } = {}) => {
    if (this.isDisposed || !this.isConnected) return;

    const timestamp = virtualClock.getCurrentTimeMs();

    if (this.parent.options.remoteSigner) {
      const result = await this.parent.options.remoteSigner.sign({
        id: nanoid(),
        type: 'ws-auth',
        exchangeName: 'bitmex',
        accountName: this.parent.options.remoteSigner.accountName,
        wsAuth: {
          toSign: `GET/realtime${timestamp}`,
          outputFormat: 'hex',
          extraData: { timestamp },
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
        return;
      }

      if (this.isDisposed || !this.isConnected) return;
      this.sendWithLog(
        JSON.stringify({
          op: 'authKeyExpires',
          args: [this.parent.options.key, timestamp, result.wsAuthSignature],
        }),
      );
      if (subscribe && !this.privateSubscriptionsSent) {
        this.sendPrivateSubscriptions();
      }
      this.scheduleNextAuthRefresh();
      return;
    }

    const signature = createHmac('sha256', this.parent.options.secret)
      .update(['GET', '/realtime', timestamp].join(''))
      .digest('hex');
    this.sendWithLog(
      JSON.stringify({
        op: 'authKeyExpires',
        args: [this.parent.options.key, timestamp, signature],
      }),
    );
    if (subscribe && !this.privateSubscriptionsSent) {
      this.sendPrivateSubscriptions();
    }
    this.scheduleNextAuthRefresh();
  };
}
