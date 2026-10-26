import type { OrderlyExchange } from './orderly.exchange';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { ENABLE_RECONNECTING_WEBSOCKET } from '../../../config/config';
import { OrderStatus } from '../../../types';
import { v } from '../../../utils/get-key';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { orderlySign } from './orderly.encryption';
import { BASE_WS_URL, ORDER_SIDE } from './orderly.types';
import { normalizeSymbol } from './orderly.utils';

export class OrderlyPrivateWebSocket extends BaseWebSocket<OrderlyExchange> {
  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      const baseURL = BASE_WS_URL.private[this.parent.options.testnet ? 'testnet' : 'livenet'];

      this.ws = this.createWebsocket(`${baseURL}/${this.parent.options.applicationId}`);

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      const timestamp = virtualClock.getCurrentTimeMs();
      //const signature = createHmac('sha256', this.parent.options.secret).update(`|${timestamp}`).digest('hex');

      this.sendWithLog(
        JSON.stringify({
          event: 'auth',
          params: {
            orderly_key: this.parent.options.key,
            sign: orderlySign(`${timestamp}`, this.parent.options.secret),
            timestamp,
          },
        }),
      );

      if (ENABLE_RECONNECTING_WEBSOCKET) {
        this.parent.fetchAndSetPrivateSnapshot();
      }
    }
  };

  subscribe = () => {
    if (!this.isDisposed) {
      this.sendWithLog(JSON.stringify({ topic: 'executionreport', event: 'subscribe' }));
      this.sendWithLog(JSON.stringify({ topic: 'algoexecutionreportv2', event: 'subscribe' }));
      this.sendWithLog(JSON.stringify({ topic: 'position', event: 'subscribe' }));
      this.ping();
    }
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.sendWithLog(JSON.stringify({ event: 'ping' }));
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      this.emitWsDiagnosticReceive(data);

      const json = jsonParse(data);

      if (json?.event === 'ping') {
        this.sendWithLog(JSON.stringify({ event: 'pong' }));
        return;
      }

      if (json?.event === 'pong') {
        this.handlePongEvent();
        return;
      }

      if (json?.event === 'auth' && json?.success) {
        this.subscribe();
        return;
      }

      if (json?.topic === 'executionreport') {
        this.handleExecutionReport(json?.data);
        return;
      }

      if (json?.topic === 'algoexecutionreportv2') {
        this.handleAlgoExecutionReport(json?.data);
        return;
      }

      if (json?.topic === 'position') {
        this.handlePositionReport(json?.data);
      }
    }
  };

  handlePositionReport = (data: Record<string, any>) => {
    const positions = Array.isArray(data?.positions) ? data.positions : [];
    for (const rawPosition of positions) {
      const position = this.parent.mapPosition(rawPosition);
      if (position) this.store.addOrUpdatePosition(position);
    }
  };

  handleExecutionReport = (data: Record<string, any>) => {
    const updatedOrder = this.parent.mapLimitOrder(data);
    const status = data.status;
    const isPartiallyFilled = status === 'PARTIAL_FILLED' || status === 'PARTIALLY_FILLED';

    const price = v(data, 'executedPrice');
    const amount = v(data, 'executedQuantity');

    if (status === 'REPLACED' || status === 'NEW' || isPartiallyFilled) {
      if (updatedOrder) {
        this.store.addOrUpdateOrder(updatedOrder);
      }
    }

    if (status === 'CANCELLED' || status === 'FILLED') {
      this.store.updateOrderStatus(
        { id: `${data.orderId}` },
        status === 'CANCELLED' ? OrderStatus.Canceled : OrderStatus.Closed,
      );
    }

    if (updatedOrder) {
      if (status === 'FILLED' || isPartiallyFilled) {
        this.parent.emitter.emit('fill', {
          id: data.orderId,
          side: updatedOrder.side,
          symbol: updatedOrder.symbol,
          price,
          amount,
        });
      }
    }
  };

  // {
  //   "topic": "algoexecutionreportv2",
  //   "ts": 1722526413463,
  //   "data": {
  //     "algoOrderId": 19005669,
  //     "rootAlgoOrderId": 19005669,
  //     "parentAlgoOrderId": 0,
  //     "symbol": "PERP_BTC_USDC",
  //     "algoType": "POSITIONAL_TP_SL",
  //     "side": "SELL",
  //     "quantity": 0,
  //     "isTriggered": false,
  //     "triggerStatus": "USELESS",
  //     "rootAlgoStatus": "NEW",
  //     "algoStatus": "NEW",
  //     "triggerPriceType": "USELESS",
  //     "price": 0,
  //     "triggerTime": 0,
  //     "totalExecutedQuantity": 0,
  //     "visibleQuantity": 0,
  //     "averageExecutedPrice": 0,
  //     "totalFee": 0,
  //     "feeAsset": "USDC",
  //     "reduceOnly": true,
  //     "createdTime": 1722526413443,
  //     "timestamp": 1722526413460,
  //     "isActivated": true,
  //     "childOrders": [
  //       {
  //         "algoOrderId": 19005670,
  //         "rootAlgoOrderId": 19005669,
  //         "parentAlgoOrderId": 19005669,
  //         "symbol": "PERP_BTC_USDC",
  //         "algoType": "TAKE_PROFIT",
  //         "side": "SELL",
  //         "quantity": 0,
  //         "isTriggered": false,
  //         "triggerPrice": 71803,
  //         "triggerStatus": "USELESS",
  //         "type": "CLOSE_POSITION",
  //         "rootAlgoStatus": "NEW",
  //         "algoStatus": "NEW",
  //         "triggerPriceType": "MARK_PRICE",
  //         "price": 0,
  //         "triggerTime": 0,
  //         "totalExecutedQuantity": 0,
  //         "visibleQuantity": 0,
  //         "averageExecutedPrice": 0,
  //         "totalFee": 0,
  //         "feeAsset": "USDC",
  //         "reduceOnly": true,
  //         "createdTime": 1722526413443,
  //         "timestamp": 1722526413460,
  //         "isActivated": true
  //       },
  //       {
  //         "algoOrderId": 19005671,
  //         "rootAlgoOrderId": 19005669,
  //         "parentAlgoOrderId": 19005669,
  //         "symbol": "PERP_BTC_USDC",
  //         "algoType": "STOP_LOSS",
  //         "side": "SELL",
  //         "quantity": 0,
  //         "isTriggered": false,
  //         "triggerPrice": 61961.6,
  //         "triggerStatus": "USELESS",
  //         "type": "CLOSE_POSITION",
  //         "rootAlgoStatus": "NEW",
  //         "algoStatus": "NEW",
  //         "triggerPriceType": "MARK_PRICE",
  //         "price": 0,
  //         "triggerTime": 0,
  //         "totalExecutedQuantity": 0,
  //         "visibleQuantity": 0,
  //         "averageExecutedPrice": 0,
  //         "totalFee": 0,
  //         "feeAsset": "USDC",
  //         "reduceOnly": true,
  //         "createdTime": 1722526413443,
  //         "timestamp": 1722526413460,
  //         "isActivated": true
  //       }
  //     ]
  //   }
  // }
  handleAlgoExecutionReport = (data: Array<Record<string, any>>) => {
    const finalData = Array.isArray(data) ? data : [data];
    (finalData as any).forEach((row: any) => {
      const status = v(row, 'algoStatus');
      const isPartiallyFilled = status === 'PARTIAL_FILLED' || status === 'PARTIALLY_FILLED';

      if (status === 'NEW' || status === 'REPLACED' || isPartiallyFilled) {
        const updatedOrders = this.parent.mapAlgoOrder(row);

        for (const order of updatedOrders) {
          this.store.addOrUpdateOrder(order);
        }
      }

      if (status === 'FILLED') {
        this.parent.emitter.emit('fill', {
          id: v(row, 'orderId') || v(row, 'algoOrderId') || v(row, 'id'),
          side: ORDER_SIDE[row.side],
          symbol: normalizeSymbol(row.symbol),
          price: row.averageExecutedPrice,
          amount: row.totalExecutedQuantity,
        });
      }

      if (status === 'CANCELLED' || status === 'FILLED') {
        this.store.updateOrderStatus(
          { id: `${v(row, 'algoOrderId')}` },
          status === 'CANCELLED' ? OrderStatus.Canceled : OrderStatus.Closed,
        );
        for (const childOrder of row.childOrders || []) {
          this.store.updateOrderStatus(
            { id: `${v(childOrder, 'algoOrderId')}` },
            status === 'CANCELLED' ? OrderStatus.Canceled : OrderStatus.Closed,
          );
        }
      }
    });
  };
}
