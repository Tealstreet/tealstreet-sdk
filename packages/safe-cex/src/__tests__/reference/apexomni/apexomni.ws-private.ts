import type { ApexOmniExchange } from './apexomni.exchange';

import createHmac from 'create-hmac';
import { nanoid } from 'nanoid';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { ENABLE_RECONNECTING_WEBSOCKET } from '../../../config/config';
import { resolveTransportRouting } from '../../../transport/registry';
import { LogSeverity, OrderStatus, Position } from '../../../types';
import { v } from '../../../utils/get-key';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseWebSocket } from '../../../exchanges/base.ws';
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
  private executionProgress = new Map<string, { reported: number; topic: number; synthetic: number; fee: number }>();
  private terminalOrderIds = new Set<string>();

  private progressFor = (id: string) => {
    const progress = this.executionProgress.get(id) || { reported: 0, topic: 0, synthetic: 0, fee: 0 };
    this.executionProgress.delete(id);
    this.executionProgress.set(id, progress);
    if (this.executionProgress.size > 4096) this.executionProgress.delete(this.executionProgress.keys().next().value!);
    return progress;
  };

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
      this.handleFillTopic(data.contents.fills, data.type === 'snapshot');
    }
    if (data.contents.orders) {
      this.handleOrderTopic(data.contents.orders, data.type === 'snapshot');
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

  handleFillTopic = (fills: WsZkAccountsV3Message['contents']['fills'], snapshot = false) => {
    fills.forEach((row) => {
      const execution = this.parent.recordWsExecution(row as Record<string, any>);
      if (!execution) return;
      const orderId = execution.orderId || '';
      const progress = this.progressFor(orderId);
      const covered = Math.min(progress.synthetic, execution.qty);
      progress.synthetic -= covered;
      progress.topic += execution.qty;
      if (covered > 0) this.parent.consumeSyntheticExecutions(orderId, covered);
      if (!snapshot) {
        this.parent.noteAmendFillProgress(execution, execution.qty);
        this.emitter.emit('fill', {
          id: orderId,
          symbol: execution.symbol,
          side: execution.side,
          price: execution.price,
          amount: execution.qty,
        });
      }
    });
  };

  handleOrderTopic = (data: WsZkAccountsV3Message['contents']['orders'], snapshot = false) => {
    if (!data.some((order) => order.status === 'CANCELED')) this.throttleFetchOrders();
    data.forEach((order) => {
      let orders;
      try {
        orders = this.parent.mapOrder(order);
      } catch (error) {
        this.parent.log(`Invalid private order update: ${String(error)}`, LogSeverity.Warning);
        return;
      }
      if (orders.length === 0) return;
      const price = parseFloat(v(order, 'price'));
      const amount = parseFloat(v(order, 'size'));
      if (order.status === 'FILLED' && price <= 0 && amount <= 0) return;
      const cumulative = Number.parseFloat(v(order, 'cumSuccessFillSize') || '0');
      if (Number.isFinite(cumulative) && cumulative >= 0 && cumulative <= amount) {
        this.parent.noteAmendOrderProgress(
          String(order.id),
          cumulative,
          order.status === 'FILLED' || order.status === 'EXPIRED',
          v(order, 'updatedAt') || v(order, 'updatedTime'),
        );
      }
      if (this.parent.recentlyCancelledOrders.has(order.id) || this.terminalOrderIds.has(order.id)) return;

      const progress = this.progressFor(String(order.id));
      const delta = Math.max(cumulative - Math.max(progress.reported, progress.topic), 0);
      const fee = Number.parseFloat(v(order, 'cumSuccessFillFee') || '0');
      const feeDelta = Number.isFinite(fee) ? Math.max(fee - progress.fee, 0) : 0;
      progress.reported = Math.max(progress.reported, cumulative);
      if (Number.isFinite(fee)) progress.fee = fee;
      if (snapshot) progress.topic = Math.max(progress.topic, cumulative);
      else if (delta > 0) {
        progress.synthetic += delta;
        const executionPrice = [v(order, 'latestMatchFillPrice'), v(order, 'averagePrice'), v(order, 'price')]
          .map(Number)
          .find((candidate) => Number.isFinite(candidate) && candidate > 0);
        if (executionPrice) {
          // Order totals can repair history, but cannot identify individual executions.
          this.parent.recordWsExecution(
            {
              symbol: order.symbol,
              side: order.side,
              orderId: order.id,
              price: executionPrice,
              size: `${delta}`,
              cumulative,
              fee: `${feeDelta}`,
              token: v(order, 'token'),
              feeCurrency: v(order, 'feeCurrency'),
              createdAt: v(order, 'updatedAt') || v(order, 'createdAt'),
            },
            true,
          );
        }
      }

      if (order.status === 'FILLED' || order.status === 'CANCELED' || order.status === 'EXPIRED') {
        this.terminalOrderIds.add(order.id);
        if (this.terminalOrderIds.size > 8192)
          this.terminalOrderIds.delete(this.terminalOrderIds.values().next().value!);
        const terminal = order.status === 'CANCELED' ? OrderStatus.Canceled : OrderStatus.Closed;
        const ids = [order.id, `${order.id}__stop_loss`, `${order.id}__take_profit`].map((id) => ({ id }));
        this.store.markOrdersTerminal(ids, terminal);
        if (order.status === 'CANCELED') {
          this.store.removeOrders(ids);
          const fresh = Date.now() - order.updatedAt <= 5000;
          const connected = virtualClock.getCurrentTimeMs() - this.lastConnectTime > 5000;
          if (
            !snapshot &&
            fresh &&
            connected &&
            order.cancelReason &&
            !['UNKNOWN_ORDER_CANCEL_REASON', 'INTERNAL_FAILED', 'USER_CANCELED'].includes(order.cancelReason)
          ) {
            this.emitAndLogError(order.cancelReason);
          }
        } else {
          this.store.addOrUpdateOrders(orders.map((mapped) => ({ ...mapped, status: OrderStatus.Closed })));
        }
      } else if (order.status === 'OPEN' || order.status === 'UNTRIGGERED' || order.status === 'PENDING') {
        this.store.addOrUpdateOrders(orders);
      }
    });
  };

  handlePositionTopic = (data: WsZkAccountsV3Message['contents']['positions']) => {
    let positions: Position[];
    try {
      positions = data.map(this.parent.mapPosition.bind(this)).filter((p): p is Position => p !== null);
    } catch (error) {
      this.parent.log(`Invalid private positions update: ${String(error)}`, LogSeverity.Warning);
      return;
    }
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
