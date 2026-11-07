import type { Balance } from '../../../types';
import type { PhemexExchange } from './phemex.exchange';

import createHmac from 'create-hmac';
import sumBy from 'lodash/sumBy';
import uniqBy from 'lodash/uniqBy';
import { nanoid } from 'nanoid';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { add, divide1, subtract } from '@tealstreet/safe-cex/src/utils/safe-math';
import { OrderStatus, Position, PositionMode, PositionSide } from '../../../types';
import { roundUSD } from '../../../utils/roundUSD';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { BASE_WS_URL, ORDER_STATUS } from '../../../exchanges/phemex/phemex.types';

type PositionSyncSide = PositionSide | 'merged' | 'unknown';
type PositionSyncTarget = { symbol: string; side: PositionSyncSide };

export class BinancePrivateWebsocket extends BaseWebSocket<PhemexExchange> {
  requestId: number = 1200;
  private positionsAndBalanceSyncTimeout?: NodeJS.Timeout;
  private readonly positionsAndBalanceSyncSymbols = new Map<string, Set<PositionSyncSide>>();
  private readonly positionsAndBalanceSyncDebounceMs = 1_000;
  private accountRevision = 0;

  constructor(parent: PhemexExchange) {
    super(parent);
    // Base lifecycle handlers are instance fields; retain the real handler so
    // fencing old reads also preserves the shared cleanup/reconnect loop.
    const baseOnClose = this.onClose;
    this.onClose = (event) => {
      this.accountRevision++;
      baseOnClose(event);
    };
  }

  get accountStateRevision(): number {
    return this.accountRevision;
  }

  commitAccountSnapshot(revision: number, balance: Balance, positions: Position[]): boolean {
    if (this.isDisposed || revision !== this.accountRevision) return false;
    // A successful REST read owns the state too: fence other outstanding reads
    // from replacing it even if no private push arrived between the requests.
    this.accountRevision++;
    this.store.addOrUpdatePositionsFull(positions);
    this.store.update({
      balance,
      loaded: { ...this.store.loaded, balance: true, positions: true },
    });
    return true;
  }

  dispose() {
    this.accountRevision++;
    if (this.positionsAndBalanceSyncTimeout) {
      clearTimeout(this.positionsAndBalanceSyncTimeout);
      this.positionsAndBalanceSyncTimeout = undefined;
    }
    this.positionsAndBalanceSyncSymbols.clear();
    super.dispose();
  }

  private syncPositionsAndBalance = async () => {
    this.positionsAndBalanceSyncTimeout = undefined;
    const symbols = [...this.positionsAndBalanceSyncSymbols];
    this.positionsAndBalanceSyncSymbols.clear();
    if (this.isDisposed) return;
    const revision = this.accountRevision;

    try {
      const { balance, positions } = await this.parent.fetchPositionsAndBalance();
      if (this.isDisposed || revision !== this.accountRevision) return;

      // Recover missing affected legs, while retaining nonzero full-snapshot rows
      // if a scoped response lags behind them.
      const nonzeroPositions = positions.filter(
        (position) => Number.isFinite(position.contracts) && position.contracts !== 0,
      );
      const authoritativeIds = new Set(nonzeroPositions.map((position) => position.id));
      const recoverySymbols = symbols
        .filter(
          ([symbol, sides]) =>
            ![...sides].every(
              (side) =>
                side !== 'unknown' &&
                nonzeroPositions.some(
                  (position) =>
                    position.symbol === symbol &&
                    (side === 'merged' ? position.positionMode === PositionMode.OneWay : position.side === side),
                ),
            ),
        )
        .map(([symbol]) => symbol);
      const symbolPositions = (
        await Promise.allSettled(recoverySymbols.map((symbol) => this.parent.fetchPositions([symbol])))
      )
        .flatMap((result) => (result.status === 'fulfilled' ? result.value : []))
        .filter((position) => !authoritativeIds.has(position.id));
      const symbolPositionIds = new Set(symbolPositions.map((position) => position.id));
      const mergedPositions = uniqBy(
        [...positions.filter((position) => !symbolPositionIds.has(position.id)), ...symbolPositions],
        'id',
      );

      // The full read and optional scoped recovery can both cross a newer
      // account/position push. Never replace that push with an older snapshot.
      this.commitAccountSnapshot(revision, balance, mergedPositions);
    } catch {
      // Silent fallback: periodic tick loop continues reconciliation.
    }
  };

  private schedulePositionsAndBalanceSync = (targets: PositionSyncTarget[] = []) => {
    targets.forEach(({ symbol, side }) => {
      const sides = this.positionsAndBalanceSyncSymbols.get(symbol) ?? new Set<PositionSyncSide>();
      sides.add(side);
      this.positionsAndBalanceSyncSymbols.set(symbol, sides);
    });

    if (this.positionsAndBalanceSyncTimeout) {
      clearTimeout(this.positionsAndBalanceSyncTimeout);
    }

    this.positionsAndBalanceSyncTimeout = setTimeout(() => {
      void this.syncPositionsAndBalance();
    }, this.positionsAndBalanceSyncDebounceMs);
  };

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      this.accountRevision++;
      const key = this.parent.options.testnet ? 'testnet' : 'livenet';
      const base = BASE_WS_URL.private[key];
      this.ws = this.createWebsocket(base);
      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.accountRevision++;
      this.auth();
      this.ping();
    }
  };

  auth = async () => {
    const timestamp = +virtualClock.getCurrentTime();
    const expiryString = String(Math.round(divide1(add(timestamp, 60 * 1000), 1000)));

    if (this.parent.options.remoteSigner) {
      const result = await this.parent.options.remoteSigner.sign({
        id: nanoid(),
        type: 'ws-auth',
        exchangeName: 'phemex',
        accountName: this.parent.options.remoteSigner.accountName,
        wsAuth: {
          toSign: `${this.parent.options.key}${expiryString}`,
          outputFormat: 'hex',
          extraData: { expiry: +expiryString },
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

      this.sendWithLog(
        JSON.stringify({
          method: 'user.auth',
          params: ['API', this.parent.options.key, result.wsAuthSignature, +expiryString],
          id: 999,
        }),
      );
      return;
    }

    const toSign = [this.parent.options.key, expiryString];
    const signature = createHmac('sha256', this.parent.options.secret).update(toSign.join('')).digest('hex');

    this.sendWithLog(
      JSON.stringify({
        method: 'user.auth',
        params: ['API', this.parent.options.key, signature, +expiryString],
        id: 999,
      }),
    );
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      this.emitWsDiagnosticReceive(data);

      const json = jsonParse(data);

      if (json?.id === 999) {
        if (!json?.error) {
          this.sendWithLog(
            JSON.stringify({
              method: 'aop_p.subscribe',
              params: [],
              id: this.requestId++,
            }),
          );
        } else {
          this.parent.emitter.emit('error', 'Phemex websocket error - wrong api key or secret');
        }
      }

      if (json?.method === 'server.ping' || json?.result === 'pong') {
        this.handlePongEvent();
        return;
      }

      if (json?.accounts_p || json?.orders_p || json?.positions_p) {
        if (Array.isArray(json.accounts_p) || Array.isArray(json.positions_p)) this.accountRevision++;
        if (json?.orders_p) {
          // Order status includes New, PartiallyFilled, Filled, Canceled, Rejected, Triggered, Untriggered;
          // Open order status includes New, PartiallyFilled, Untriggered;

          // console.log('phemex order update', json);

          // this.parent.emitter.emit('fill', {
          //   id: data.orderId,
          //   side: updatedOrder.side,
          //   symbol: updatedOrder.symbol,
          //   price,
          //   amount,
          // });

          // "accountID":9328670003,
          // "action":"New",
          // "actionBy":"ByUser",
          // "actionTimeNs":1666858780876924611,
          // "addedSeq":77751555,
          // "apRp":"0",
          // "bonusChangedAmountRv":"0",
          // "bpRp":"0",
          // "clOrdID":"c0327a7d-9064-62a9-28f6-2db9aaaa04e0",
          // "closedPnlRv":"0",
          // "closedSize":"0",
          // "code":0,
          // "cumFeeRv":"0",
          // "cumQty":"0",
          // "cumValueRv":"0",
          // "curAccBalanceRv":"1508.489893982237",
          // "curAssignedPosBalanceRv":"24.62786650928",
          // "curBonusBalanceRv":"0",
          // "curLeverageRr":"-10",
          // "curPosSide":"Buy",
          // "curPosSize":"0.043",
          // "curPosTerm":1,
          // "curPosValueRv":"894.0689",
          // "curRiskLimitRv":"1000000",
          // "currency":"USDT",
          // "cxlRejReason":0,
          // "displayQty":"0.003",
          // "execFeeRv":"0",
          // "execID":"00000000-0000-0000-0000-000000000000",
          // "execPriceRp":"20723.7",
          // "execQty":"0",
          // "execSeq":77751555,
          // "execStatus":"New",
          // "execValueRv":"0",
          // "feeRateRr":"0",
          // "leavesQty":"0.003",
          // "leavesValueRv":"63.4503",
          // "message":"No error",
          // "ordStatus":"New",
          // "ordType":"Market",
          // "orderID":"fa64c6f2-47a4-4929-aab4-b7fa9bbc4323",
          // "orderQty":"0.003",
          // "pegOffsetValueRp":"0",
          // "posSide":"Long",
          // "priceRp":"21150.1",
          // "relatedPosTerm":1,
          // "relatedReqNum":11,
          // "side":"Buy",
          // "slTrigger":"ByMarkPrice",
          // "stopLossRp":"0",
          // "stopPxRp":"0",
          // "symbol":"BTCUSDT",
          // "takeProfitRp":"0",
          // "timeInForce":"ImmediateOrCancel",
          // "tpTrigger":"ByLastPrice",
          // "tradeType":"Amend",
          // "transactTimeNs":1666858780881545305,
          // "userID":932867

          const orders = json?.orders_p;
          const terminalOrderTargets: PositionSyncTarget[] =
            json?.type !== 'snapshot'
              ? orders
                  .filter((order: any) => {
                    const rawOrderStatus = `${order?.ordStatus || ''}`.toLowerCase();
                    return ['filled', 'triggered', 'canceled', 'rejected'].includes(rawOrderStatus);
                  })
                  .map((order: any) => ({
                    symbol: this.parent.formatSymbolIn(order.symbol) as string,
                    side:
                      order.posSide === 'Long'
                        ? PositionSide.Long
                        : order.posSide === 'Short'
                          ? PositionSide.Short
                          : order.posSide === 'Merged'
                            ? 'merged'
                            : 'unknown',
                  }))
                  .filter((target: PositionSyncTarget) => Boolean(target.symbol))
              : [];
          const shouldSyncPositionsAfterOrderEvent = json?.type !== 'snapshot' && terminalOrderTargets.length > 0;
          this.recordMessageHandlerStats('order');

          // mapOrder = (o: Record<string, any>) => {
          //   const order: Writable<Order> = {
          //     id: o.orderId || o.ordId || o.id,
          //     status: ORDER_STATUS[o.state || o.status],
          //     symbol: (o.symbol || o.instId).replace(`_${this.apiProductType.toUpperCase()}`, ''),
          //     type: ORDER_TYPE[o.orderType || o.ordType],
          //     side: ORDER_SIDE[o.tradeSide || o.tS],
          //     price: o.price || parseFloat(o.px),
          //     amount: o.size || parseFloat(o.sz) || 0,
          //     filled: o.filledQty || parseFloat(o.accFillSz) || 0,
          //     reduceOnly: o.reduceOnly || o.low,
          //     remaining: subtract(o.size || parseFloat(o.sz) || 0, o.filledQty || parseFloat(o.accFillSz) || 0),
          //   };
          //
          //   if (o.planType) {
          //     order.type = ORDER_TYPE[o.planType];
          //     order.price = parseFloat(o.triggerPrice) || parseFloat(o.triggerPx);
          //     order.reduceOnly = true;
          //   }
          //
          //   return order;
          // };

          // if (sl > 0) {
          //   orders.push({
          //     ...orders[0],
          //     id: `${o.orderId}__stop_loss`,
          //     type: OrderType.StopLoss,
          //     side: inverseSide,
          //     price: sl,
          //     filled: 0,
          //     remaining: orders[0].amount,
          //     lastUpdate: Date.now(),
          //     slTriggerType: ORDER_TRIGGER_TYPE[v(o, 'slTriggerBy')],
          //   });
          // }

          // if (tp > 0) {
          //   orders.push({
          //     ...orders[0],
          //     id: `${o.orderId}__take_profit`,
          //     type: OrderType.TakeProfit,
          //     side: inverseSide,
          //     price: tp,
          //     filled: 0,
          //     remaining: orders[0].amount,
          //     lastUpdate: Date.now(),
          //     tpTriggerType: ORDER_TRIGGER_TYPE[v(o, 'tpTriggerBy')],
          //   });
          // }

          // "info": {
          //   "accountID": 21498120003,
          //     "action": "New",
          //     "actionBy": "ByUser",
          //     "actionTimeNs": 1738057549702063400,
          //     "addedSeq": 12286508664,
          //     "bonusChangedAmountRv": "0",
          //     "clOrdID": "Tealstreet_MV01i5jGqGUYdnDLgolP",
          //     "cl_req_code": 0,
          //     "closedPnlRv": "-0.019264965491",
          //     "closedSize": "80",
          //     "code": 0,
          //     "cumFeeRv": "0.0252864",
          //     "cumPtFeeRv": "0",
          //     "cumQty": "160",
          //     "cumValueRv": "42.144",
          //     "curAccBalanceRv": "27.539169627789",
          //     "curAssignedPosBalanceRv": "9.029603023378",
          //     "curBonusBalanceRv": "0",
          //     "curLeverageRr": "-50",
          //     "curPosSide": "Buy",
          //     "curPosSize": "1381",
          //     "curPosTerm": 1,
          //     "curPosValueRv": "364.087961466796",
          //     "curRiskLimitRv": "25000",
          //     "currency": "USDT",
          //     "cxlRejReason": 0,
          //     "displayQty": "0",
          //     "execFeeRv": "0.0126432",
          //     "execID": "a88834f5-0c19-5c56-b66e-da7f07d82b5b",
          //     "execPriceRp": "0.2634",
          //     "execQty": "80",
          //     "execSeq": 12286508664,
          //     "execStatus": "TakerFill",
          //     "execValueRv": "21.072",
          //     "feeRateRr": "0.0006",
          //     "lastLiquidityInd": "RemovedLiquidity",
          //     "leavesQty": "240",
          //     "leavesValueRv": "63.216",
          //     "message": "No error",
          //     "ordStatus": "PartiallyFilled",
          //     "ordType": "Limit",
          //     "orderID": "026a6849-adea-44ff-9bc0-7d5000b1d561",
          //     "orderQty": "400",
          //     "pegOffsetProportionRr": "0",
          //     "pegOffsetValueRp": "0",
          //     "posSide": "Merged",
          //     "priceRp": "0.2634",
          //     "ptFeeRv": "0",
          //     "ptPriceRp": "0",
          //     "relatedPosTerm": 1,
          //     "relatedReqNum": 38,
          //     "side": "Sell",
          //     "slPxRp": "0",
          //     "slTimeInForce": "ImmediateOrCancel",
          //     "slTrigger": "ByMarkPrice",
          //     "stopLossRp": "0",
          //     "stopPxRp": "0",
          //     "symbol": "LISTAUSDT",
          //     "takeProfitRp": "0",
          //     "timeInForce": "GoodTillCancel",
          //     "tpPxRp": "0",
          //     "tpTimeInForce": "ImmediateOrCancel",
          //     "tpTrigger": "ByLastPrice",
          //     "tradeType": "Trade",
          //     "transactTimeNs": 1738057549707282700,
          //     "userID": 2149812
          // }

          // {
          //   "accountID": 21498120003,
          //   "action": "New",
          //   "actionBy": "ByUser",
          //   "actionTimeNs": 1738057549702063400,
          //   "addedSeq": 12286508664,
          //   "bonusChangedAmountRv": "0",
          //   "clOrdID": "Tealstreet_MV01i5jGqGUYdnDLgolP",
          //   "cl_req_code": 0,
          //   "closedPnlRv": "-0.019264965491",
          //   "closedSize": "80",
          //   "code": 0,
          //   "cumFeeRv": "0.0126432",
          //   "cumPtFeeRv": "0",
          //   "cumQty": "80",
          //   "cumValueRv": "21.072",
          //   "curAccBalanceRv": "27.57107779328",
          //   "curAssignedPosBalanceRv": "9.029603023378",
          //   "curBonusBalanceRv": "0",
          //   "curLeverageRr": "-50",
          //   "curPosSide": "Buy",
          //   "curPosSize": "1461",
          //   "curPosTerm": 1,
          //   "curPosValueRv": "385.179226432287",
          //   "curRiskLimitRv": "25000",
          //   "currency": "USDT",
          //   "cxlRejReason": 0,
          //   "displayQty": "0",
          //   "execFeeRv": "0.0126432",
          //   "execID": "9051c75f-1639-50b8-a724-994efa62432a",
          //   "execPriceRp": "0.2634",
          //   "execQty": "80",
          //   "execSeq": 12286508664,
          //   "execStatus": "TakerFill",
          //   "execValueRv": "21.072",
          //   "feeRateRr": "0.0006",
          //   "lastLiquidityInd": "RemovedLiquidity",
          //   "leavesQty": "320",
          //   "leavesValueRv": "84.288",
          //   "message": "No error",
          //   "ordStatus": "PartiallyFilled",
          //   "ordType": "Limit",
          //   "orderID": "026a6849-adea-44ff-9bc0-7d5000b1d561",
          //   "orderQty": "400",
          //   "pegOffsetProportionRr": "0",
          //   "pegOffsetValueRp": "0",
          //   "posSide": "Merged",
          //   "priceRp": "0.2634",
          //   "ptFeeRv": "0",
          //   "ptPriceRp": "0",
          //   "relatedPosTerm": 1,
          //   "relatedReqNum": 38,
          //   "side": "Sell",
          //   "slPxRp": "0",
          //   "slTimeInForce": "ImmediateOrCancel",
          //   "slTrigger": "ByMarkPrice",
          //   "stopLossRp": "0",
          //   "stopPxRp": "0",
          //   "symbol": "LISTAUSDT",
          //   "takeProfitRp": "0",
          //   "timeInForce": "GoodTillCancel",
          //   "tpPxRp": "0",
          //   "tpTimeInForce": "ImmediateOrCancel",
          //   "tpTrigger": "ByLastPrice",
          //   "tradeType": "Trade",
          //   "transactTimeNs": 1738057549707282700,
          //   "userID": 2149812
          // }

          const partiallyFilledOrderIds = new Set(
            orders
              .filter((order: any) => {
                const rawOrderStatus = `${order?.ordStatus || ''}`.toLowerCase();
                return rawOrderStatus === 'partiallyfilled' || rawOrderStatus === 'partially_filled';
              })
              .map((order: any) => order.orderID)
              .filter(Boolean),
          );

          const formattedOrders = orders.flatMap((order: any) => {
            const rawOrderStatus = order.ordStatus.toLowerCase();
            let status = ORDER_STATUS[rawOrderStatus] || OrderStatus.Canceled;
            if (json?.type === 'snapshot' && status !== OrderStatus.Open) return [];

            // possible we have another update in same msg which says order is closed, but doesnt have tpsl
            let orderForMapping = order;
            if (status === OrderStatus.Open) {
              const foundClosedOrder = orders.find(
                (o: any) =>
                  o.orderID === order.orderID &&
                  (ORDER_STATUS[o.ordStatus.toLowerCase()] || OrderStatus.Canceled) !== OrderStatus.Open,
              );
              if (foundClosedOrder) {
                status = ORDER_STATUS[foundClosedOrder.ordStatus.toLowerCase()] || OrderStatus.Canceled;
                orderForMapping = { ...order, ordStatus: foundClosedOrder.ordStatus };
              }
            }

            const formattedOrders = this.parent.mapOrderUpdate(orderForMapping);

            if (json?.type !== 'snapshot') {
              const fills = uniqBy(
                formattedOrders.filter(
                  (o) => o.filled > 0 && (o.status === OrderStatus.Closed || partiallyFilledOrderIds.has(o.id)),
                ),
                'id',
              );
              fills.forEach((order) => {
                this.parent.emitter.emit('fill', {
                  id: order.id,
                  side: order.side,
                  symbol: this.parent.formatSymbolIn(order.symbol),
                  price: parseFloat(order.info?.execPriceRp || order.price),
                  amount: order.filled,
                });
              });
            }

            return formattedOrders;
          });
          if (json?.type === 'snapshot') {
            this.store.update({ orders: formattedOrders, loaded: { ...this.store.loaded, orders: true } });
          } else {
            this.store.addOrUpdateOrders(formattedOrders);

            // Terminal order events (filled/canceled/etc.) often imply position changes.
            // Debounce REST sync to avoid waiting for slow tick intervals.
            if (shouldSyncPositionsAfterOrderEvent) {
              this.schedulePositionsAndBalanceSync(terminalOrderTargets);
            }
          }
        }

        if (json?.positions_p) {
          const positions = json?.positions_p;
          this.recordMessageHandlerStats('position');

          const supportedPositions = positions.filter((p: any) =>
            this.store.markets.some((m) => m.symbol === this.parent.formatSymbolIn(p.symbol)),
          );

          const parsedPositions: Position[] = supportedPositions
            .map(this.parent.mapPosition)
            .filter((position: Position | undefined) => Boolean(position));

          const upsertablePositions = parsedPositions.filter(
            (position) =>
              position.contracts > 0 || this.store.positions.some((existing) => existing.id === position.id),
          );

          if (upsertablePositions.length > 0) {
            this.store.addOrUpdatePositions(upsertablePositions);
          }
        }

        if (json?.accounts_p) {
          this.recordMessageHandlerStats('account');
          if (!Array.isArray(json.accounts_p)) return;
          const currencies: NonNullable<Balance['currencies']> = { ...(this.store.balance.currencies || {}) };
          const accounts =
            this.store.balance.info && !Array.isArray(this.store.balance.info) ? { ...this.store.balance.info } : {};

          for (const account of json.accounts_p) {
            const currency = account?.currency;
            if (!currency) continue;

            const currencyPositions = this.store.positions.filter((position) => {
              const market = this.store.marketsBySymbol[position.symbol];
              return (
                market?.quote === currency ||
                market?.info?.settleCurrency === currency ||
                position.info?.currency === currency
              );
            });
            const total = parseFloat(account.accountBalanceRv);
            const used = parseFloat(account.totalUsedBalanceRv);
            const upnl = roundUSD(sumBy(currencyPositions, (p) => parseFloat(String(+p.unrealizedPnl))));
            currencies[currency] = {
              used,
              free: subtract(total, used),
              total: subtract(total, upnl),
              upnl,
              info: account,
            };
            accounts[currency] = account;
          }

          if (Object.keys(currencies).length === 0) return;

          const balance: Balance = {
            used: sumBy(Object.values(currencies), 'used'),
            free: sumBy(Object.values(currencies), 'free'),
            total: sumBy(Object.values(currencies), 'total'),
            upnl: roundUSD(sumBy(Object.values(currencies), 'upnl')),
            info: accounts,
            currencies,
          };

          this.store.update({ balance });
        }
      }
    }
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.sendWithLog(`{"id":${this.requestId++},"method":"server.ping","params":[]}`);
    }
  };
}
