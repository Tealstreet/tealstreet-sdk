import type { BitunixExchange } from './bitunix.exchange';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { sleep } from '@tealstreet/sleep/src/sleep';
import { ENABLE_PWS_LOG } from '../../config/config';
import {
  Order,
  OrderSide,
  OrderStatus,
  OrderStopTriggerType,
  OrderTimeInForce,
  OrderType,
  PositionSide,
  Writable,
} from '../../types';
import { v } from '../../utils/get-key';
import { subtract } from '../../utils/safe-math';
import { uuid } from '../../utils/uuid';
import { virtualClock } from '../../utils/virtualClock';
import { BaseWebSocket } from '../base.ws';
import {
  BASE_WS_URL,
  FALLBACK_WS_URL,
  ORDER_SIDE,
  ORDER_TRIGGER_TYPE,
  ORDER_TYPE,
  TIME_IN_FORCE,
} from './bitunix.types';
import { sha256Hex } from './bitunix.utils';

export class BitunixPrivateWebsocket extends BaseWebSocket<BitunixExchange> {
  pingLoopTimeoutId?: NodeJS.Timeout;
  handledPongEvent = false;
  lastPongEventTimestamp = 0;
  private messageCount = 0;
  private useFallbackUrl = this.parent.options.tealstreetWsRoute;
  private lastMessageCount = 0;
  private connectionAttempts = 0;
  private readonly MAX_ATTEMPTS = 2;

  connectAndSubscribe = async () => {
    if (!this.isDisposed) {
      // Only switch to fallback URL if we've tried the normal connection multiple times without success
      if (this.connectionAttempts >= this.MAX_ATTEMPTS && this.lastMessageCount === 0 && this.messageCount === 0) {
        this.useFallbackUrl = true;
        this.connectionAttempts = 0; // Reset attempts when switching to fallback
      } else if (!this.useFallbackUrl) {
        this.connectionAttempts++;
      }

      this.lastMessageCount = this.messageCount;
      this.messageCount = 0; // Reset message counter on new connection

      const key = this.parent.options.testnet ? 'testnet' : 'livenet';
      const url = this.useFallbackUrl ? FALLBACK_WS_URL.private[key] : BASE_WS_URL.private[key];

      if (this.useFallbackUrl) {
        try {
          this.parent.emitter.emit('grafana-info', 'Using tealstreet bitunix proxy (private)');
        } catch (_e) {}
      }

      const handleClose = () => {
        clearTimeout(this.pingLoopTimeoutId);
        this.onClose();
      };

      const handleError = () => {
        if (this.ws && this.ws.readyState !== WebSocket.CLOSED) {
          try {
            this.ws.close();
          } catch (_e) {}
        }
        handleClose();
      };

      this.ws = this.createWebsocket(url);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('close', handleClose);
      this.ws.addEventListener('error', handleError);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.ping();

      if (this.pingLoopTimeoutId) {
        clearTimeout(this.pingLoopTimeoutId);
      }

      (async () => {
        await this.sendLogin();

        const doPingLoop = () => {
          if (!this.isDisposed) {
            this.ping();
            this.pingLoopTimeoutId = setTimeout(doPingLoop, 15_000);
          }
        };

        doPingLoop();
      })();

      // This is not needed, because the login will subscribe to all topics
      // ['balance', 'order', 'position', 'tpsl'].forEach((topic) => {
      //   const payload = {
      //     op: 'subscribe',
      //     args: [topic],
      //   };
      //   this.send(JSON.stringify(payload));
      // });
    }
  };

  // {"op":"connect","data":{"result":true}}
  // {"op":"login","data":{"nonce":"nrDPV14zLsxI4HOSYCB","result":true}}
  // {"ch":"balance","ts":1743869139375,"data":{"coin":"USDT","available":"78.5074555080940018","frozen":"4.132555","isolationFrozen":"0","crossFrozen":"4.132555","margin":"0","isolationMargin":"0","crossMargin":"0","expMoney":"0"}}
  // limit
  // {"ch":"order","ts":1743869139445,"data":{"event":"CREATE","symbol":"BTCUSDT","orderId":"1908551617210011648","positionType":"CROSS","positionMode":"HEDGE","side":"Buy","effect":"SHORT","type":"Limit","qty":"0.001","price":"82651.1","ctime":"2025-04-05T16:05:39.369077000Z","mtime":"2025-04-05T16:05:39.369077000Z","leverage":"20","orderStatus":"NEW","fee":"0","averagePrice":"0","dealAmount":"0","clientId":"1908551617106288642"}}
  // market
  // {"ch":"order","ts":1743869177060,"data":{"event":"CREATE","symbol":"BTCUSDT","orderId":"1908551774961979392","positionType":"CROSS","positionMode":"HEDGE","side":"Buy","effect":"SHORT","type":"Market","qty":"0.001","price":"86820.3","ctime":"2025-04-05T16:06:16.979434000Z","mtime":"2025-04-05T16:06:16.979434000Z","leverage":"20","orderStatus":"NEW","fee":"0","averagePrice":"0","dealAmount":"0","clientId":"1908551774887620609"}}
  // {"ch":"order","ts":1743869177066,"data":{"event":"CLOSE","symbol":"BTCUSDT","orderId":"1908551774961979392","positionType":"CROSS","positionMode":"HEDGE","side":"Buy","effect":"SHORT","type":"Market","qty":"0.001","price":"86820.3","ctime":"2025-04-05T16:06:16.979434000Z","mtime":"2025-04-05T16:06:16.979434000Z","leverage":"20","orderStatus":"FILLED","fee":"0.0496092","averagePrice":"82682","dealAmount":"0.001","clientId":"1908551774887620609"}}
  // {"ch":"position","ts":1743869177067,"data":{"event":"OPEN","marginMode":"CROSS","positionId":"93365513147542397","positionMode":"HEDGE","side":"LONG","leverage":"20","margin":"4.1341","ctime":"2025-04-05T16:06:17.047000000Z","qty":"0.001","entryValue":"82.682","symbol":"BTCUSDT","realizedPNL":"-0.0496092","funding":"0","fee":"-0.0496092","unrealizedPNL":"0"}}
  // {"ch":"order","ts":1743869196854,"data":{"event":"CLOSE","symbol":"BTCUSDT","orderId":"1908551617210011648","positionType":"CROSS","positionMode":"HEDGE","side":"Buy","effect":"SHORT","type":"Limit","qty":"0.001","price":"82651.1","ctime":"2025-04-05T16:05:39.369077000Z","mtime":"2025-04-05T16:05:39.369077000Z","leverage":"20","orderStatus":"FILLED","fee":"0.01653022","averagePrice":"82651.1","dealAmount":"0.001","clientId":"1908551617106288642"}}
  // full tp sl
  // {"ch":"tpsl","ts":1743869224321,"data":{"event":"CREATE","symbol":"BTCUSDT","orderId":"1676572590500514944","positionId":"93365513147542397","leverage":"20","side":"Sell","positionMode":"HEDGE","status":"NEW","ctime":"2025-04-05T16:07:04.310660000Z","type":"POSITION_TPSL","tpStopType":"Last","tpPrice":"88866.6","tpOrderType":"Market","slStopType":"Last","slPrice":"79566.6"}}
  // {"ch":"tpsl","ts":1743869234458,"data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"1676572590500514944","positionId":"93365513147542397","leverage":"20","side":"Sell","positionMode":"HEDGE","status":"NEW","ctime":"2025-04-05T16:07:04.310660000Z","type":"POSITION_TPSL","tpQty":"0","slQty":"0","tpStopType":"Last","tpPrice":"86979.5","tpOrderType":"Market","tpOrderPrice":"0","slStopType":"Last","slPrice":"79566.6","slOrderPrice":"0"}}
  // {"ch":"tpsl","ts":1743869244692,"data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"1676572590500514944","positionId":"93365513147542397","leverage":"20","side":"Sell","positionMode":"HEDGE","status":"NEW","ctime":"2025-04-05T16:07:04.310660000Z","type":"POSITION_TPSL","slQty":"0","tpStopType":"Last","tpOrderType":"Market","slStopType":"Last","slPrice":"79566.6","slOrderPrice":"0"}}
  // {"ch":"position","ts":1743869244693,"data":{"event":"UPDATE","marginMode":"CROSS","positionId":"93365513147542397","positionMode":"HEDGE","side":"LONG","leverage":"20","margin":"8.266655","ctime":"2025-04-05T16:06:17.057167000Z","qty":"0.002","entryValue":"165.3331","symbol":"BTCUSDT","realizedPNL":"-0.06613942","funding":"0","fee":"-0.06613942","unrealizedPNL":"0"}}
  // partial tpsl
  // {"ch":"tpsl","ts":1743869263978,"data":{"event":"CREATE","symbol":"BTCUSDT","orderId":"3390840160823603227","positionId":"93365513147542397","leverage":"20","side":"Sell","positionMode":"HEDGE","status":"NEW","ctime":"2025-04-05T16:07:43.959002000Z","type":"TPSL","tpQty":"0.001","tpStopType":"Last","tpPrice":"88804.6","tpOrderType":"Market"}}
  // position reverse
  // {"ch":"position","ts":1743869383554,"data":{"event":"CLOSE","marginMode":"CROSS","positionId":"93365513147542397","positionMode":"HEDGE","side":"LONG","leverage":"20","margin":"0","ctime":"2025-04-05T16:06:17.057167000Z","qty":"0","entryValue":"0","symbol":"BTCUSDT","realizedPNL":"-0.13825554","funding":"0","fee":"-0.16535554","unrealizedPNL":"0.0271"}}
  // {"ch":"position","ts":1743869383727,"data":{"event":"OPEN","marginMode":"CROSS","positionId":"7178479509927876382","positionMode":"HEDGE","side":"SHORT","leverage":"20","margin":"8.26801","ctime":"2025-04-05T16:09:43.708000000Z","qty":"0.002","entryValue":"165.3602","symbol":"BTCUSDT","realizedPNL":"-0.09921612","funding":"0","fee":"-0.09921612","unrealizedPNL":"0"}}
  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      this.messageCount++; // Increment message counter

      if (ENABLE_PWS_LOG) {
        this.parent.emitter.emit('grafana-pws', `${data}`);
      }

      const json = jsonParse(data);

      const channel = json?.op || json?.ch;

      switch (channel) {
        case 'connect':
          break;
        case 'balance':
          console.info('bitunix: received balance message, but we will fetch it from the exchange', json);
          // balance message doesn't have upnl, so we need to fetch it from the exchange
          this.throttledFetchBalance();
          break;
        case 'order':
          switch (json?.data?.event) {
            case 'CREATE':
              // Handle newly created order
              const orderData = json?.data;

              // Create a new order object based on the websocket data
              const newOrder: Writable<Order> = {
                id: v(orderData, 'orderId'),
                status: OrderStatus.Open,
                symbol: orderData.symbol,
                type: ORDER_TYPE[orderData.type?.toUpperCase()],
                side: ORDER_SIDE[orderData.side?.toUpperCase()],
                price: parseFloat(orderData.price || '0'),
                amount: parseFloat(v(orderData, 'qty') || '0'),
                reduceOnly: v(orderData, 'reductionOnly') || false,
                filled: parseFloat(v(orderData, 'dealAmount') || '0'),
                lastUpdate: new Date(v(orderData, 'mtime') || v(orderData, 'ctime')).getTime(),
                remaining: subtract(v(orderData, 'qty') || '0', v(orderData, 'dealAmount') || '0'),
                timeInForce: TIME_IN_FORCE[orderData.effect?.toUpperCase()] || OrderTimeInForce.GoodTillCancel,
                info: orderData,
                ...(v(orderData, 'tpStopType') && { tpTriggerType: ORDER_TRIGGER_TYPE[v(orderData, 'tpStopType')] }),
                ...(v(orderData, 'slStopType') && { slTriggerType: ORDER_TRIGGER_TYPE[v(orderData, 'slStopType')] }),
                ...(v(orderData, 'tpPrice') && { takeProfitPrice: parseFloat(v(orderData, 'tpPrice')) }),
                ...(v(orderData, 'slPrice') && { stopLossPrice: parseFloat(v(orderData, 'slPrice')) }),
              };

              // Cache TIF from WS (REST API returns incorrect effect for POST_ONLY orders)
              if (orderData.clientId && orderData.effect) {
                const wsTif = TIME_IN_FORCE[orderData.effect?.toUpperCase()];
                if (wsTif) {
                  this.parent.orderTifCache.set(orderData.clientId, wsTif);
                }
              }

              // Add the new order to the store
              this.parent.store.addOrUpdateOrders([newOrder]);
              break;

            case 'UPDATE':
              // Handle order update
              const updatedOrderData = json?.data;

              // Find and update existing order
              const existingOrder = this.parent.store.orders.find((o) => o.id === updatedOrderData.orderId);

              if (existingOrder) {
                const updatedOrder: Writable<Order> = {
                  ...existingOrder,
                  price: parseFloat(updatedOrderData.price || existingOrder.price.toString()),
                  amount: parseFloat(v(updatedOrderData, 'qty') || existingOrder.amount.toString()),
                  filled: parseFloat(v(updatedOrderData, 'dealAmount') || existingOrder.filled.toString()),
                  remaining: subtract(
                    v(updatedOrderData, 'qty') || existingOrder.amount.toString(),
                    v(updatedOrderData, 'dealAmount') || existingOrder.filled.toString(),
                  ),
                  lastUpdate:
                    new Date(v(updatedOrderData, 'mtime') || v(updatedOrderData, 'ctime')).getTime() ||
                    existingOrder.lastUpdate,
                  info: updatedOrderData,
                };

                // Update the order in the store
                this.parent.store.addOrUpdateOrders([updatedOrder]);
              }
              break;

            case 'CLOSE':
              // Handle closed order (filled or canceled)
              const closedOrderData = json?.data;

              // Determine the final status of the order
              let orderStatus = OrderStatus.Canceled;
              if (closedOrderData.orderStatus === 'FILLED') {
                orderStatus = OrderStatus.Closed;
              }

              // Find and update the order
              const orderToClose = this.parent.store.orders.find((o) => o.id === closedOrderData.orderId);

              if (orderToClose) {
                const finalOrder: Writable<Order> = {
                  ...orderToClose,
                  status: orderStatus,
                  price: parseFloat(closedOrderData.price || orderToClose.price.toString()),
                  filled: parseFloat(v(closedOrderData, 'dealAmount') || orderToClose.filled.toString()),
                  remaining: 0,
                  lastUpdate:
                    new Date(v(closedOrderData, 'mtime') || v(closedOrderData, 'ctime')).getTime() ||
                    orderToClose.lastUpdate,
                  info: closedOrderData,
                };

                // Update the order in the store
                this.parent.store.addOrUpdateOrders([finalOrder]);
              }
              break;
          }

          // Refresh orders after any order event processing
          this.throttledFetchOrders();
          break;
        case 'position':
          // Cache positionId from websocket events for reliable order placement
          if (json?.data?.positionId && json?.data?.symbol && json?.data?.side) {
            const wsPositionSide = json.data.side.toUpperCase();
            const side = wsPositionSide === 'LONG' || wsPositionSide === 'BUY' ? 'long' : 'short';
            if (json.data.event !== 'CLOSE') {
              this.parent.positionIdCache.set(`${json.data.symbol}:${side}`, json.data.positionId);
            } else {
              this.parent.positionIdCache.delete(`${json.data.symbol}:${side}`);
            }
          }

          switch (json?.data?.event) {
            case 'OPEN':
            case 'UPDATE': {
              // Update position info in store so positionId stays fresh
              const wsData = json.data;
              const wsSide =
                wsData.side?.toUpperCase() === 'LONG' || wsData.side?.toUpperCase() === 'BUY'
                  ? PositionSide.Long
                  : PositionSide.Short;
              const existingPos = this.parent.store.positions.find(
                (p) => p.symbol === wsData.symbol && p.side === wsSide,
              );
              if (existingPos && wsData.positionId) {
                (existingPos as Writable<typeof existingPos>).info = {
                  ...(existingPos.info || {}),
                  positionId: wsData.positionId,
                };
              }
              break;
            }
            case 'CLOSE':
              break;
          }
          this.throttledFetchPositions();
          break;
        case 'tpsl':
          switch (json?.data?.event) {
            case 'CREATE':
              // Handle newly created TP/SL order
              const tpslOrder = json?.data;

              // Find the associated position if any
              const positionId = tpslOrder.positionId;
              const position = this.parent.store.positions.find((p) => p.info?.positionId === positionId);
              const rawOrderId = v(tpslOrder, 'orderId');
              const hasTp = tpslOrder.tpPrice && !isNaN(parseFloat(tpslOrder.tpPrice));
              const hasSl = tpslOrder.slPrice && !isNaN(parseFloat(tpslOrder.slPrice));
              const useLegOrderIds = !!hasTp && !!hasSl;
              const orderInfo = {
                ...tpslOrder,
                id: rawOrderId,
                orderId: rawOrderId,
              };

              const newOrders: Array<Writable<Order>> = [];

              // Process take profit order if it exists
              if (hasTp) {
                const tpQty = parseFloat(tpslOrder.tpQty || '0');
                const isFullTp = position && tpQty === +position.contracts;

                newOrders.push({
                  id: this.parent.getTpslLegOrderId(rawOrderId, tpslOrder.positionId, 'tp', useLegOrderIds),
                  status: OrderStatus.Open,
                  symbol: tpslOrder.symbol,
                  type: OrderType.TakeProfit,
                  side: position
                    ? position.side === PositionSide.Long
                      ? OrderSide.Sell
                      : OrderSide.Buy
                    : ORDER_SIDE[tpslOrder.side?.toUpperCase()],
                  price: parseFloat(tpslOrder.tpPrice),
                  amount: this.parent.enableFakeFullTpSl && isFullTp ? 0 : tpQty,
                  reduceOnly: true,
                  filled: 0,
                  lastUpdate: virtualClock.getCurrentTimeMs(),
                  remaining: tpQty,
                  info: orderInfo,
                  // Bitunix omits tpStopType when it's MarkPrice (the default)
                  tpTriggerType: ORDER_TRIGGER_TYPE[tpslOrder.tpStopType] || OrderStopTriggerType.MarkPrice,
                });
              }

              // Process stop loss order if it exists
              if (hasSl) {
                const slQty = parseFloat(tpslOrder.slQty || '0');
                const isFullSl = position && slQty === +position.contracts;

                newOrders.push({
                  id: this.parent.getTpslLegOrderId(rawOrderId, tpslOrder.positionId, 'sl', useLegOrderIds),
                  status: OrderStatus.Open,
                  symbol: tpslOrder.symbol,
                  type: OrderType.StopLoss,
                  side: position
                    ? position.side === PositionSide.Long
                      ? OrderSide.Sell
                      : OrderSide.Buy
                    : ORDER_SIDE[tpslOrder.side?.toUpperCase()],
                  price: parseFloat(tpslOrder.slPrice),
                  amount: this.parent.enableFakeFullTpSl && isFullSl ? 0 : slQty,
                  reduceOnly: true,
                  filled: 0,
                  lastUpdate: virtualClock.getCurrentTimeMs(),
                  remaining: slQty,
                  info: orderInfo,
                  // Bitunix omits slStopType when it's MarkPrice (the default)
                  slTriggerType: ORDER_TRIGGER_TYPE[tpslOrder.slStopType] || OrderStopTriggerType.MarkPrice,
                });
              }

              // Add the new orders to the store if any were created
              if (newOrders.length > 0) {
                this.parent.store.addOrUpdateOrders(newOrders);
              } else {
                // If we couldn't create orders based on the data, fetch all orders to be safe
                this.throttledFetchOrders();
              }
              break;
            case 'UPDATE':
              // Handle updated TP/SL order

              // Find the order in the store by orderId
              const orderId = json?.data?.orderId;
              const existingOrders = this.parent.store.orders.filter((o) => `${o.id}`.includes(`${orderId}`));

              if (existingOrders.length > 0) {
                const updatedOrders = existingOrders.map((o) => {
                  // Update take profit price if available
                  const tpPrice = json?.data?.tpPrice ? parseFloat(json.data.tpPrice) : undefined;
                  // Update stop loss price if available
                  const slPrice = json?.data?.slPrice ? parseFloat(json.data.slPrice) : undefined;
                  const triggerPrice = o.type === OrderType.TakeProfit ? tpPrice : slPrice;

                  return {
                    ...o,
                    ...(triggerPrice && { price: triggerPrice }),
                    ...(json?.data?.tpStopType && { tpTriggerType: ORDER_TRIGGER_TYPE[json.data.tpStopType] }),
                    ...(json?.data?.slStopType && { slTriggerType: ORDER_TRIGGER_TYPE[json.data.slStopType] }),
                  };
                });

                // Update orders in the store
                this.parent.store.addOrUpdateOrders(updatedOrders);
              }
              break;
            case 'CLOSE':
              // Handle closed TP/SL orders (either filled or canceled)

              // Update order status based on order status from the event
              const status = json?.data?.status === 'FILLED' ? OrderStatus.Closed : OrderStatus.Canceled;

              this.parent.store.addOrUpdateOrders(
                this.parent.store.orders
                  .filter((o) => `${o.id}`.includes(`${json?.data?.orderId}`))
                  .map((o) => {
                    return {
                      ...o,
                      status,
                    };
                  }),
              );
              break;
          }
          // Always fetch updated orders after any TP/SL event
          this.throttledFetchOrders();
          break;
        case 'login':
          if (json?.data?.result) {
            this.parent.emitter.emit('grafana-pws', `[bitunix login] success`);
          } else {
            this.parent.emitter.emit('grafana-pws', `[bitunix login] failed ${JSON.stringify(json)}`);
            this.parent.emitAndLogError('Bitunix private ws login failed, double check your key and secret');
            (async () => {
              await virtualClock.syncBybitClientServerTime();
              await sleep(2_000);
              this.sendLogin();
            })();
          }
        default:
      }

      if (!this.handledPongEvent && +virtualClock.getCurrentTimeMs() - this.lastPongEventTimestamp > 5_000) {
        this.lastPongEventTimestamp = +virtualClock.getCurrentTimeMs();
        this.handledPongEvent = true;
        this.handlePongEvent();
      }
    }
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.sendWithLog(JSON.stringify({ op: 'ping' }));
      // BitUnix doesn't send pong events, so we handle it immediately
      this.handlePongEvent();
      // Update lastReceived timestamp to prevent reconnection banner from appearing
      // since BitUnix private WS doesn't respond to pings
      this.trackLastReceived();
    }
  };

  sendLogin = async () => {
    const timestamp = +virtualClock.getCurrentTime();
    const nonce = uuid();

    await virtualClock.syncBybitClientServerTime();
    await sleep(25);

    this.send(
      JSON.stringify({
        op: 'login',
        args: [
          {
            apiKey: this.parent.options.key.trim(),
            timestamp,
            nonce,
            sign: sha256Hex(
              sha256Hex(nonce + timestamp + this.parent.options.key.trim()) + this.parent.options.secret.trim(),
            ),
          },
        ],
      }),
    );
  };

  balanceThrottleTimeout?: NodeJS.Timeout;
  ordersThrottleTimeout?: NodeJS.Timeout;
  positionsThrottleTimeout?: NodeJS.Timeout;

  private throttledFetchBalance = () => {
    if (this.balanceThrottleTimeout) {
      clearTimeout(this.balanceThrottleTimeout);
    }
    this.balanceThrottleTimeout = setTimeout(this.fetchThrottledBalance, 1000);
  };

  private fetchThrottledBalance = async () => {
    this.balanceThrottleTimeout = undefined;
    if (!this.isDisposed) {
      const { balance, positions } = await this.parent.fetchBalance();
      this.parent.store.update({
        balance,
        positions,
        loaded: {
          ...this.parent.store.loaded,
          balance: true,
          positions: true,
        },
      });
    }
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
      const orders = await this.parent.fetchOrders();
      this.parent.store.update({
        orders,
        loaded: {
          ...this.parent.store.loaded,
          orders: true,
        },
      });
    }
  };

  private throttledFetchPositions = () => {
    if (this.positionsThrottleTimeout) {
      clearTimeout(this.positionsThrottleTimeout);
    }
    this.positionsThrottleTimeout = setTimeout(this.fetchThrottledPositions, 1000);
  };

  private fetchThrottledPositions = async () => {
    this.positionsThrottleTimeout = undefined;
    if (!this.isDisposed) {
      const positions = await this.parent.fetchPositions();
      this.parent.store.update({
        positions,
        loaded: {
          ...this.parent.store.loaded,
          positions: true,
        },
      });
    }
  };
}
