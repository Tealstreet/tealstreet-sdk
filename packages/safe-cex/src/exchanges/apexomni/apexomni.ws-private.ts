import type { ApexOmniExchange } from './apexomni.exchange';

import createHmac from 'create-hmac';
import { nanoid } from 'nanoid';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { ENABLE_RECONNECTING_WEBSOCKET } from '../../config/config';
import { resolveTransportRouting } from '../../transport/registry';
import { LogSeverity, OrderStatus, Position } from '../../types';
import { v } from '../../utils/get-key';
import { virtualClock } from '../../utils/virtualClock';
import { BaseWebSocket } from '../base.ws';
import { BASE_WS_URL, ORDER_CANCEL_REASON, ORDER_STATUS, ORDER_TIME_IN_FORCE, ORDER_TYPE } from './apexomni.types';

type WsZkAccountsV3Message = {
  contents: {
    accounts: any[];
    deleverages: any[];
    fills: ReadonlyArray<{
      symbol: string; // 'ETH-USDC';
      side: 'BUY' | 'SELL';
      matchFillId: string; // 'effc5f02-6f3c-5a09-ad5a-45893af2d362';
      liquidateFee: string; // '0.000000';
      orderId: string; // '612263822920516405';
      clientOrderId: string; // '18093915151169226';
      fee: string; // '0.012873';
      isDeleverage: boolean; // false;
      createdAt: number; //  1723811870706;
      isOpen: boolean; //  false;
      size: string; // '0.01';
      price: string; // '2574.70';
      quoteAmount: string; // '25.7470';
      isLiquidate: boolean; // false;
      id: string; // '612263822962459445';
      direction: 'TAKER' | 'MAKER';
      status: string; // 'SUCCESS';
      updatedAt: number; // 1723811870715;
    }>;
    orders: ReadonlyArray<{
      cumSuccessLiquidateFee: string; // "0.000000",
      symbol: string; // "ETH-USDC",
      openSlParams: {
        triggerSize: string; // "",
        triggerPrice: string; // "",
        triggerPriceType: 'UNKNOWN_PRICE_TYPE';
      };
      cumSuccessFillFee: string; // "0.012873",
      type: keyof typeof ORDER_TYPE;
      isPositionTpsl: boolean; // false,
      isDeleverage: boolean; // false,
      createdAt: number; // 1723811870696,
      isSetOpenTp: boolean; // false,
      price: string; // "0.05",
      cumSuccessFillValue: string; // "25.7470",
      id: string; // "612263822920516405",
      cancelReason: keyof typeof ORDER_CANCEL_REASON; //  'UNKNOWN_ORDER_CANCEL_REASON';
      timeInForce: keyof typeof ORDER_TIME_IN_FORCE; // "IMMEDIATE_OR_CANCEL",
      updatedAt: number; //  1723811870715,
      limitFee: string; // "0.000001",
      side: 'SELL' | 'BUY';
      clientId: string; // "18093915151169226",
      triggerPrice: string; // "0.00",
      triggerPriceType: 'MARKET';
      expiresAt: number; // 1726405200000,
      openTpParams: {
        triggerSize: string; // "",
        triggerPrice: string; // "",
        triggerPriceType: 'UNKNOWN_PRICE_TYPE';
      };
      cumSuccessFillSize: string; // "0.01",
      accountId: string; // "611779989544632629",
      size: string; // "0.01",
      reduceOnly: boolean; // true,
      isSetOpenSl: boolean; //  false,
      isLiquidate: boolean; // false,
      remainingSize: string; // "0.00",
      status: keyof typeof ORDER_STATUS; // 'FILLED';
    }>;
    positionClosedTransactions: ReadonlyArray<{
      symbol: string; // 'ETH-USDC';
      side: 'LONG' | 'SHORT'; // 'LONG';
      liquidateFee: string; // '0.000000';
      orderId: string; // '612263822920516405';
      fee: string; // '0.012873';
      closeSharedFundingFee: string; // '0';
      type: 'CLOSE_POSITION';
      isDeleverage: boolean; // false;
      createdAt: number; //  1723811870715;
      size: string; // '0.01';
      price: string; // '2574.70';
      closeSharedOpenFee: string; // '0.012865';
      closeSharedOpenValue: string; // '25.731500';
      fundingValue: string; // '0';
      isLiquidate: boolean; // false;
      id: string; // '612263823000209205';
    }>;
    positions: ReadonlyArray<{
      symbol: string; // 'ETH-USDC';
      exitPrice: string; // '2574.700000000000000000';
      side: 'LONG' | 'SHORT'; // 'LONG';
      openValue: string; // '0';
      maxSize: string; // '1300';
      sumOpen: string; // '0.01';
      fundingFee: string; // '0';
      sumClose: string; // '0.01';
      entryPrice: string; // '0';
      accountId: string; // '611779989544632629';
      customImr: string; // '0';
      size: string; // '0.00';
      realizedPnl: string; // '-0.01023800000000000000';
      updatedAt: number; // 1723811870715;
    }>;
    transfers: any[];
    wallets: ReadonlyArray<{
      accountId: string; //  "611779989544632629",
      pendingDepositAmount: string; // "0.000000",
      balance: string; // "-15.908058",
      pendingWithdrawAmount: string; // "0.000000",
      pendingTransferInAmount: string; // "0.000000",
      pendingTransferOutAmount: string; // "0.000000",
      token: 'USDC' | 'USDT';
    }>;
  };
  topic: string;
  id: string;
  timestamp: number;
  type: 'delta' | 'snapshot';
};

export class ApexOmniPrivateWebsocket extends BaseWebSocket<ApexOmniExchange> {
  // Apex's private WS doesn't always echo our `pong` back — relying solely on
  // the pong-miss timer fires spurious reconnects even while account data and
  // server-initiated pings are flowing. Any inbound traffic here is sufficient
  // proof of liveness.
  protected override treatAnyMessageAsPong = true;

  lastConnectTime = 0;
  ordersThrottleTimeout: NodeJS.Timeout | undefined;
  positionsThrottleTimeout: NodeJS.Timeout | undefined;
  // Per-orderId cumulative-fill / cumulative-fee trackers. Apex order updates
  // carry CUMULATIVE totals (cumSuccessFillSize / cumSuccessFillFee), not deltas.
  // Differencing against the last total seen per order derives a positive
  // delta and avoids double-counting against contents.fills rows.
  private lastCumFilledByOrderId: Map<string, number> = new Map();
  private lastCumFeeByOrderId: Map<string, number> = new Map();

  throttleFetchOrders = () => {
    if (this.ordersThrottleTimeout) {
      clearTimeout(this.ordersThrottleTimeout);
    }
    this.ordersThrottleTimeout = setTimeout(async () => {
      this.ordersThrottleTimeout = undefined;
      if (!this.isDisposed) {
        try {
          const orders = await this.parent.fetchOrders();
          if (this.isDisposed || this.parent.isDisposed) return;
          this.parent.store.addOrUpdateOrdersFull(orders);
        } catch (err: any) {
          // Route through the structured log pipeline at Warning severity so a
          // reconnect storm doesn't paint DevTools red with raw console.error
          // entries that are hard to filter or suppress.
          this.parent.log(`Error fetching orders: ${err?.message || err}`, LogSeverity.Warning);
        }
      }
    }, 1000);
  };

  throttleFetchPositions = () => {
    if (this.positionsThrottleTimeout) {
      clearTimeout(this.positionsThrottleTimeout);
    }
    this.positionsThrottleTimeout = setTimeout(async () => {
      this.positionsThrottleTimeout = undefined;
      if (!this.isDisposed) {
        try {
          const positions = await this.parent.fetchPositions();
          this.parent.store.addOrUpdatePositionsFull(positions);
        } catch (err: any) {
          this.parent.log(`Error fetching positions: ${err?.message || err}`, LogSeverity.Warning);
        }
      }
    }, 1000);
  };

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      const defaultEndpoint = BASE_WS_URL.private[this.parent.options.testnet ? 'testnet' : 'livenet'];
      const liveTimestamp = `${virtualClock.getCurrentTimeMs()}`;
      const endpointWithLiveTimestamp = defaultEndpoint.replace(/timestamp=\d+/, `timestamp=${liveTimestamp}`);
      const joinChar = endpointWithLiveTimestamp.includes('?') ? '&' : '?';
      const requestIdEndpoint = `${endpointWithLiveTimestamp}${joinChar}requestId=${nanoid()}`;
      const route = resolveTransportRouting(this.parent.name, this.parent.options).wsPrivate;
      this.ws = this.createWebsocket(route.tunnelled ? route.host! : requestIdEndpoint);

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    }
  };

  onOpen = async () => {
    if (!this.isDisposed) {
      // Keep the pre-PR connection-setup order: auth → subscribe immediately → ping.
      // The PR's auth-watchdog + subscribe-after-ack gating caused the upstream proxy
      // to close the socket with code 1005 ("no status code received") shortly after
      // each connect. Pre-PR Apex called subscribe() unconditionally in onOpen and
      // worked reliably; whatever proxy expectation that satisfies is undocumented,
      // so we mirror the legacy sequence verbatim and rely on the WS heartbeat plus
      // periodic REST reconcile to catch missing live updates.
      //
      // `await` is NOT the ack-gating that `d1184f3d8e` reverted, and it does not
      // reintroduce it. It waits on OUR signer, never on Apex. The local branch of
      // `auth` computes an HMAC with no `await` before its send, so awaiting costs
      // the direct path a single microtask and the login still precedes the
      // subscribe in the same macrotask — whereas the reverted gating waited on a
      // server ack for up to 5s. What it fixes is hosted mode, where the signature
      // is a round trip to the paired browser: un-awaited, `subscribe()` reached the
      // wire before the login and the private channels were never established.
      let authed = false;
      try {
        authed = await this.auth();
      } catch (error) {
        // `remoteSigner.sign` THROWS (unavailable, sign timeout, failed send) rather
        // than resolving `{ error }`. Pass the error ALONE: for an Error input a
        // second argument REPLACES the message (`base.ts`), which would destroy the
        // only description of the cause.
        this.emitAndLogError(error instanceof Error ? error : new Error(String(error)));
      }

      if (!authed) {
        // Send NOTHING on a failed login. A subscribe would be refused, and either
        // frame makes `trackLastReceived` set `isInitialized` and keep `lastReceived`
        // fresh — so the staleness threshold can never trip and a socket holding zero
        // channels reports healthy forever. Reconnect instead: a BOUNDED automatic
        // retry, paced by `processClose`'s close-code backoff. It does NOT make
        // health honest — a private socket that never initialized is filtered out of
        // `base.ts`'s `initializedPrivateSockets` and `privateHealthy` short-circuits
        // true on an empty list — but it is the only path that recovers on its own.
        void this.reconnect();
        return;
      }

      this.lastConnectTime = virtualClock.getCurrentTimeMs();
      this.subscribe();
      this.ping();

      if (ENABLE_RECONNECTING_WEBSOCKET) {
        this.parent.fetchAndSetPrivateSnapshot();
      }
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      this.emitWsDiagnosticReceive(data);

      const json = jsonParse<any>(data);

      if (json?.success === false && json?.ret_msg) {
        this.parent.log(`Private WS request rejected: ${json.ret_msg}`, LogSeverity.Warning);
      }

      if (json?.topic === 'ws_zk_accounts_v3') {
        this.recordMessageHandlerStats('ws_zk_accounts_v3');
        this.handleWsAccountsV2Topic(json);
      }

      if (json?.op === 'ping' || json?.ping !== undefined) {
        const incomingArgs = Array.isArray(json?.args) && json.args.length > 0 ? json.args : undefined;
        this.send(
          JSON.stringify(
            incomingArgs
              ? { op: 'pong', args: incomingArgs }
              : { op: 'pong', args: [`${virtualClock.getCurrentTimeMs()}`] },
          ),
        );
      }

      if (json?.op === 'pong' || json?.ret_msg === 'pong' || json?.pong !== undefined) {
        // 5s ping cadence matches pre-PR keepalive frequency. Going to 15s caused
        // upstream proxy idle-timeouts to fire (close code 1005, no status received)
        // for users whose tunnel path has a ~30s idle window.
        this.handlePongEvent(5_000);
      }
    }
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.sendWithLog(JSON.stringify({ op: 'ping' }));
      if (this.pingTimeoutId) {
        clearTimeout(this.pingTimeoutId);
      }
      // Self-heal ping if no pong arrives — schedule the next ping at the same
      // 5s cadence as handlePongEvent so the keepalive stays consistent whether
      // pongs land or not.
      this.pingTimeoutId = setTimeout(() => this.ping(), 5_000);
    }
  };

  handleWsAccountsV2Topic = (data: WsZkAccountsV3Message) => {
    if (data.contents.fills) {
      this.handleFillTopic(data.contents.fills);
    }
    if (data.contents.orders) {
      this.handleOrderTopic(data.contents.orders);
    }
    if (data.contents.positions) {
      this.handlePositionTopic(data.contents.positions);
    }
  };

  /*
    PENDING: OrderStatus.Open,
    OPEN: OrderStatus.Open,
    FILLED: OrderStatus.Closed,
    CANCELED: OrderStatus.Canceled,
    EXPIRED: OrderStatus.Closed,
    UNTRIGGERED: OrderStatus.Open,
  */

  handleFillTopic = (fills: WsZkAccountsV3Message['contents']['fills']) => {
    // IMPORTANT: do NOT emit 'fill' events from this handler.
    //
    // The pre-PR Apex private WS only emitted 'fill' from handleOrderTopic
    // (gated by `shouldNotify` = within 5s of order update AND >5s after
    // connect). Our PR added handleFillTopic to capture per-fill rows for
    // the new fetchHistoricalFills / chart fill-arrows cache via
    // recordWsExecution — that part is correct and needed.
    //
    // However, this handler initially also emitted 'fill' events, which
    // caused two regressions:
    //
    //   1. Live-fill duplication: contents.fills[] and contents.orders[]
    //      both arrive in the same aop_p batch for a single fill, so
    //      handleFillTopic + handleOrderTopic each fired emit('fill') →
    //      2 fill toasters per real fill.
    //
    //   2. Cold-start spam: Apex sends type:'snapshot' on connect with
    //      historical fills in contents.fills[]. handleFillTopic had no
    //      shouldNotify-equivalent guard, so it fired emit('fill') for
    //      each historical fill → 3-5 stale toasters on every cold start.
    //
    // Restoring pre-PR toaster behavior: handleOrderTopic remains the
    // sole 'fill' emitter (with its existing guards). recordWsExecution
    // continues to populate the chart fill-arrow cache for both live and
    // snapshot rows, which is the desired behavior for that cache.
    fills.forEach((fill) => {
      this.parent.recordWsExecution(fill as Record<string, any>);
    });
  };

  handleOrderTopic = (data: WsZkAccountsV3Message['contents']['orders']) => {
    // Check if any orders are CANCELED before deciding to throttle fetch
    const hasCanceledOrders = data.some((order) => order.status === 'CANCELED');

    // Only fetch orders if we're not dealing with cancellations
    // Cancellations should be handled by the blacklist mechanism, not by re-fetching
    if (!hasCanceledOrders) {
      this.throttleFetchOrders();
    }

    data.forEach((order) => {
      // Skip recently cancelled orders
      if (this.parent.recentlyCancelledOrders.has(order.id)) {
        return;
      }

      const orders = this.parent.mapOrder(order);
      if (orders.length === 0) return;

      const price = parseFloat(v(order, 'price'));
      const amount = parseFloat(v(order, 'size'));

      const isFreshOrder = Date.now() - order.updatedAt <= 5_000;
      const isEnoughTimePassedAfterConnect = virtualClock.getCurrentTimeMs() - this.lastConnectTime > 5_000;

      const shouldNotify = isEnoughTimePassedAfterConnect && isFreshOrder;

      if (order.status === 'FILLED') {
        // False positive when order is replaced
        // it emits a partially filled with 0 amount & price
        if (price <= 0 && amount <= 0) return;

        // Apex order updates carry CUMULATIVE fill totals, not per-event deltas. Recording
        // the cumulative qty as a single execution would double-count alongside the per-fill
        // rows from contents.fills (handled in handleFillTopic). Mirror Blofin's pattern:
        // track last seen cumulative per orderId and only push a synthetic execution when
        // there's a positive delta — and only as a safety net for deltas not already
        // accounted for by the fills topic.
        const cumulativeFilledSize = Number.parseFloat(v(order, 'cumSuccessFillSize') || '0');
        if (Number.isFinite(cumulativeFilledSize) && cumulativeFilledSize > 0) {
          const orderKey = String(order.id);
          const previousCumulative = this.lastCumFilledByOrderId.get(orderKey) || 0;
          const fillDelta = cumulativeFilledSize - previousCumulative;
          if (fillDelta > 0) {
            this.lastCumFilledByOrderId.set(orderKey, cumulativeFilledSize);
            const cumulativeFee = Number.parseFloat(v(order, 'cumSuccessFillFee') || '0');
            const previousCumulativeFee = this.lastCumFeeByOrderId.get(orderKey) || 0;
            const feeDelta = Number.isFinite(cumulativeFee) ? Math.max(cumulativeFee - previousCumulativeFee, 0) : 0;
            if (Number.isFinite(cumulativeFee)) {
              this.lastCumFeeByOrderId.set(orderKey, cumulativeFee);
            }
            this.parent.recordWsExecution({
              symbol: order.symbol,
              side: order.side,
              orderId: order.id,
              price: v(order, 'latestMatchFillPrice') || v(order, 'averagePrice') || v(order, 'price'),
              size: `${fillDelta}`,
              fee: `${feeDelta}`,
              token: v(order, 'token'),
              feeCurrency: v(order, 'feeCurrency'),
              createdAt: v(order, 'updatedAt') || v(order, 'createdAt'),
            });
          } else if (fillDelta === 0 && previousCumulative === 0) {
            // First time we see a non-zero cumulative without prior tracking — record the
            // baseline so subsequent deltas compute correctly without duplicating this row
            // (handleFillTopic already emitted the per-fill executions if Apex sent them).
            this.lastCumFilledByOrderId.set(orderKey, cumulativeFilledSize);
            const cumulativeFeeBaseline = Number.parseFloat(v(order, 'cumSuccessFillFee') || '0');
            if (Number.isFinite(cumulativeFeeBaseline)) {
              this.lastCumFeeByOrderId.set(orderKey, cumulativeFeeBaseline);
            }
          }
        }
      }

      if (order.status === 'FILLED' && shouldNotify) {
        this.emitter.emit('fill', {
          id: orders[0].id,
          side: orders[0].side,
          symbol: orders[0].symbol,
          price,
          amount,
        });
      }

      if (order.status === 'FILLED' || order.status === 'CANCELED') {
        // Order is terminal — drop our cumulative-fill trackers so they don't leak memory.
        const orderKey = String(order.id);
        this.lastCumFilledByOrderId.delete(orderKey);
        this.lastCumFeeByOrderId.delete(orderKey);

        // We remove the order and its stop loss and take profit
        // if they exists, because they will be replaced with correct IDs
        if (
          order.status !== 'FILLED' &&
          shouldNotify &&
          order.cancelReason &&
          // exclude EC_PerCancelRequest from error as thats simply user canceling the order
          order.cancelReason !== 'UNKNOWN_ORDER_CANCEL_REASON' &&
          order.cancelReason !== 'INTERNAL_FAILED' &&
          order.cancelReason !== 'USER_CANCELED'
        ) {
          this.emitAndLogError(order.cancelReason);
        }

        if (order.status === 'CANCELED') {
          // Remove cancelled orders entirely instead of adding them back with Canceled status
          const idsToRemove = [
            { id: orders[0].id },
            { id: `${orders[0].id}__stop_loss` },
            { id: `${orders[0].id}__take_profit` },
          ];
          this.store.removeOrders(idsToRemove);
        } else {
          // For FILLED orders, update their status
          this.store.addOrUpdateOrders(
            orders.map((safeCexOrder) => ({
              ...safeCexOrder,
              status: OrderStatus.Closed,
            })),
          );
        }
      } else if (order.status === 'OPEN' || order.status === 'UNTRIGGERED') {
        // Only add open/untriggered orders - don't re-add filled/cancelled ones
        this.store.addOrUpdateOrders(orders);
      }
    });
  };

  handlePositionTopic = (data: WsZkAccountsV3Message['contents']['positions']) => {
    const positions: Position[] = data.map(this.parent.mapPosition.bind(this)).filter((p): p is Position => p !== null);
    const updates: Array<[Position, Position]> = positions.map((p) => [p, p]);
    this.store.updatePositions(updates);
    this.parent.lastWebsocketPositionsUpdate = virtualClock.getCurrentTimeMs();
    // Throttled refetch to reconcile any discrepancies between WebSocket updates and API state
    this.throttleFetchPositions();
  };

  private auth = async (): Promise<boolean> => {
    // NOTE: keep the auth payload shape EXACTLY matching pre-PR Apex. We previously
    // added a `topics` field as a hint to the upstream proxy, which empirically
    // correlated with code-1005 server closes shortly after connect. Stripping it
    // restored stable connections.
    const timestamp = virtualClock.getCurrentTimeMs();
    const messageToSign = [timestamp, 'GET', '/ws/accounts'].join('');

    if (this.parent.options.remoteSigner) {
      const result = await this.parent.options.remoteSigner.sign({
        id: nanoid(),
        type: 'ws-auth',
        exchangeName: 'apexomni',
        accountName: this.parent.options.remoteSigner.accountName,
        wsAuth: {
          toSign: messageToSign,
          outputFormat: 'apexomni-base64',
          extraData: { timestamp },
        },
      });

      if (result.error) {
        // NOT "auth failed" — `health/errorPatterns.ts` matches that literal as
        // `auth_invalid`, which claims the user's key is rejected and suppresses
        // automatic account reloads for five minutes.
        this.emitAndLogError(`Remote WS signing unavailable: ${result.error}`);
        return false;
      }

      const payload = {
        op: 'login',
        args: [
          JSON.stringify({
            type: 'login',
            httpMethod: 'GET',
            requestPath: '/ws/accounts',
            apiKey: this.parent.options.key,
            passphrase: this.parent.options.passphrase,
            timestamp,
            signature: result.wsAuthSignature,
          }),
        ],
      };
      this.sendWithLog(JSON.stringify(payload));
      return true;
    }

    const signature = createHmac('sha256', Buffer.from(this.parent.options.secret).toString('base64'))
      .update(messageToSign)
      .digest('base64');
    const payload = {
      op: 'login',
      args: [
        JSON.stringify({
          type: 'login',
          httpMethod: 'GET',
          requestPath: '/ws/accounts',
          apiKey: this.parent.options.key,
          passphrase: this.parent.options.passphrase,
          timestamp,
          signature,
        }),
      ],
    };
    this.sendWithLog(JSON.stringify(payload));
    return true;
  };

  private subscribe = () => {
    // Restore the pre-PR three-channel subscribe list. Even though ws_user_v1 and
    // ws_notify_v1 have no message handlers in our code, dropping them from the
    // subscribe args correlated with the 1005 server-close churn — the upstream
    // proxy / Apex server may have a stricter expectation about subscribe shape
    // than the API docs imply. Subscribing to channels we don't process is cheap
    // and matches the production-tested pre-PR behavior.
    const payload = { op: 'subscribe', args: ['ws_zk_accounts_v3', 'ws_user_v1', 'ws_notify_v1'] };
    this.sendWithLog(JSON.stringify(payload));
  };

  dispose() {
    if (this.ordersThrottleTimeout) {
      clearTimeout(this.ordersThrottleTimeout);
      this.ordersThrottleTimeout = undefined;
    }
    if (this.positionsThrottleTimeout) {
      clearTimeout(this.positionsThrottleTimeout);
      this.positionsThrottleTimeout = undefined;
    }
    super.dispose();
  }
}
