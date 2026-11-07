import type { BingxExchange } from './bingx.exchange';

import { bigIntAsStringJsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { LogSeverity, MarginType, OrderStatus, OrderType, Position, PositionMode, PositionSide } from '../../../types';
import { createPositionId } from '../../../utils/createPositionId';
import { readBlob } from '../../../utils/readBlob';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { BASE_WS_URL, ENDPOINTS, ORDER_SIDE, ORDER_TRADE_UPDATE, POSITION_SIDE } from './bingx.types';

export class BingxPrivateWebsocket extends BaseWebSocket<BingxExchange> {
  private static readonly LISTEN_KEY_REFRESH_MS = 30 * 60 * 1000;
  private static readonly LISTEN_KEY_RETRY_MS = 30 * 1000;

  private lastPositionFetchTime = 0;
  private positionFetchThrottleMs = 750;
  private positionFetchScheduled = false;
  private hasPendingPositionEvents = false;
  private emittedFillQtyByOrderId = new Map<string, number>();
  private listenKey: string | undefined;
  private listenKeyRefreshTimer?: ReturnType<typeof setTimeout>;
  private rebuildingListenKey = false;

  constructor(parent: BingxExchange) {
    super(parent);

    const baseOnClose = this.onClose;
    this.onClose = (event?: any) => {
      this.clearListenKeyRefreshTimer();
      baseOnClose(event);
    };
  }

  /** One connect at a time: a recovery and a reconnect landing together would leak a socket. */
  private connecting = false;

  connectAndSubscribe = async () => {
    if (!this.isDisposed && !this.connecting) {
      this.clearListenKeyRefreshTimer();
      this.connecting = true;
      let listenKey: string;
      try {
        listenKey = await this.fetchListenKey();
      } catch (error: any) {
        // Commonly the browser reported "online" before the link worked. Through the base
        // backoff, so an online recovery can run the pending retry at once.
        this.retryFailedConnect(
          `Failed to fetch BingX listen key: ${error?.response?.data?.msg || error?.message || String(error)}`,
        );
        return;
      } finally {
        this.connecting = false;
      }
      if (this.isDisposed) return;

      const key = this.parent.options.testnet ? 'testnet' : 'livenet';
      const base = BASE_WS_URL.private[key];

      const url = `${base}?listenKey=${listenKey}`;

      this.ws = this.createWebsocket(url);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
      this.ws.addEventListener('open', this.onOpen);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.scheduleListenKeyRefresh();
      this.ping();
    }
  };

  onMessage = async ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      let strData = data;
      if (data instanceof Blob) {
        strData = await readBlob(data);
        if (this.isDisposed) return;
      }

      this.emitWsDiagnosticReceive(strData);

      // Bingx uses big ints (orderId in particular). We need to replace them with strings to avoid JS rounding errors
      const json: any = bigIntAsStringJsonParse(strData);

      if (strData === 'Ping' || json?.ping) {
        this.sendWithLog('Pong');
        return;
      }

      if (strData === 'Pong' || json?.pong) {
        this.handlePongEvent();
        return;
      }

      if (json?.e === 'listenKeyExpired') {
        this.recordMessageHandlerStats('listenKey');
        await this.rebuildWithFreshListenKey();
        return;
      }

      if (json?.e === 'ACCOUNT_UPDATE') {
        await this.handleAccountEvents([json]);
        this.recordMessageHandlerStats('position');
      }
      if (json?.e === 'ORDER_TRADE_UPDATE' || json?.e === 'TRADE_UPDATE') {
        this.handleOrderEvents([json]);
        this.recordMessageHandlerStats('order');
      }
    }
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.sendWithLog('Ping');
    }
  };

  handleOrderEvents = (events: ORDER_TRADE_UPDATE[]) => {
    events.forEach(({ o: data }) => {
      const status = data.X;
      if (status === 'PARTIALLY_FILLED' || status === 'FILLED') {
        const orderId = String(data.i);
        const prevFilled = this.emittedFillQtyByOrderId.get(orderId) ?? 0;
        const cumulativeFilled = parseFloat(String(data.z ?? 0));
        const fillFromEvent = parseFloat(String((data as any).l ?? 0));
        const deltaFromCumulative = Number.isFinite(cumulativeFilled) ? Math.max(cumulativeFilled - prevFilled, 0) : 0;
        const hasDirectFill = Number.isFinite(fillFromEvent) && fillFromEvent > 0;
        const incrementalFill = Number.isFinite(cumulativeFilled)
          ? deltaFromCumulative
          : hasDirectFill
            ? fillFromEvent
            : 0;

        if (incrementalFill > 0) {
          this.schedulePositionRefresh();

          // BingX can emit multiple partial trade updates for one market order;
          // emit fill only on terminal state to avoid duplicate toasts/optimistic over-application.
          if (status === 'FILLED') {
            const fillPrice = parseFloat(String(data.ap || data.p || 0));
            const normalizedFillPrice = Number.isFinite(fillPrice) ? fillPrice : 0;
            const emittedAmount =
              Number.isFinite(cumulativeFilled) && cumulativeFilled > 0 ? cumulativeFilled : incrementalFill;
            this.parent.emitter.emit('fill', {
              id: data.i,
              side: ORDER_SIDE[data.S],
              symbol: data.s.replace(/-/g, ''),
              price: normalizedFillPrice,
              amount: emittedAmount,
            });
          }
        }

        if (Number.isFinite(cumulativeFilled) && cumulativeFilled >= prevFilled) {
          this.emittedFillQtyByOrderId.set(orderId, cumulativeFilled);
          if (this.emittedFillQtyByOrderId.size > 5000) {
            this.emittedFillQtyByOrderId.clear();
          }
        }
      }

      const order = this.parent.mapOrder(data);
      if (order?.status === OrderStatus.Open && order.type !== OrderType.Market) {
        this.store.addOrUpdateOrder(order);
      }

      if (status === 'CANCELED' || status === 'EXPIRED') {
        this.store.updateOrderStatus({ id: data.i }, OrderStatus.Canceled);
        this.emittedFillQtyByOrderId.delete(String(data.i));
      }
      if (status === 'FILLED') {
        this.store.updateOrderStatus({ id: data.i }, OrderStatus.Closed);
        this.emittedFillQtyByOrderId.delete(String(data.i));
      }
    });
  };

  handleAccountEvents = async (events: Array<Record<string, any>>) => {
    const accountPositionMode = await this.parent.getOrFetchPositionMode().catch(() => null);

    events.forEach((event) => {
      const accountPositions = this.extractAccountPositions(event);
      if (accountPositions.length > 0) {
        this.schedulePositionRefresh();
      }

      accountPositions.forEach((positionUpdate) => {
        const symbol = this.normalizeSymbol(positionUpdate.s);
        if (!symbol) return;

        const rawContracts = parseFloat(String(positionUpdate.pa ?? positionUpdate.positionAmt ?? 0));
        if (!Number.isFinite(rawContracts)) return;

        const wsPositionSide = typeof positionUpdate.ps === 'string' ? positionUpdate.ps : '';
        const fallbackPositionMode: PositionMode =
          wsPositionSide === 'BOTH'
            ? PositionMode.OneWay
            : wsPositionSide === 'LONG' || wsPositionSide === 'SHORT'
              ? PositionMode.Hedged
              : PositionMode.OneWay;
        const positionMode = accountPositionMode ?? fallbackPositionMode;

        const inferredSideFromAmount =
          rawContracts > 0 ? PositionSide.Long : rawContracts < 0 ? PositionSide.Short : undefined;
        const positionsForSymbol = this.store.positions.filter((p) => p.symbol === symbol);
        const mappedWsSide = POSITION_SIDE[wsPositionSide];
        let side =
          positionMode === PositionMode.OneWay
            ? (inferredSideFromAmount ?? positionsForSymbol[0]?.side)
            : (mappedWsSide ?? inferredSideFromAmount);
        if (!side) return;

        const modeAwarePositionId = createPositionId(symbol, side, positionMode);
        const existingPosition =
          this.store.positions.find((p) => p.id === modeAwarePositionId) ??
          this.store.positions.find((p) => p.symbol === symbol && p.side === side) ??
          (positionMode === PositionMode.OneWay ? positionsForSymbol[0] : undefined);

        if (!side && existingPosition) {
          side = existingPosition.side;
        }
        if (!side) return;

        const contracts = Math.abs(rawContracts);
        if (!(contracts > 0) && !existingPosition) return;

        const entryPrice = parseFloat(
          String(positionUpdate.ep ?? positionUpdate.avgPrice ?? existingPosition?.entryPrice ?? 0),
        );
        const upnl = parseFloat(
          String(positionUpdate.up ?? positionUpdate.unrealizedProfit ?? existingPosition?.unrealizedPnl ?? 0),
        );
        const liquidationPrice = parseFloat(
          String(positionUpdate.liqp ?? positionUpdate.liquidationPrice ?? existingPosition?.liquidationPrice ?? 0),
        );

        const normalizedEntryPrice = Number.isFinite(entryPrice) ? entryPrice : 0;
        const normalizedUpnl = Number.isFinite(upnl) ? upnl : 0;
        const normalizedLiqPrice = Number.isFinite(liquidationPrice) ? liquidationPrice : 0;
        const normalizedMarginType =
          typeof positionUpdate.mt === 'string'
            ? positionUpdate.mt.toLowerCase() === 'isolated'
              ? MarginType.Isolated
              : MarginType.Cross
            : (existingPosition?.marginType ?? MarginType.Cross);

        const positionId = existingPosition?.id ?? modeAwarePositionId;
        const nextPositionMode = existingPosition?.positionMode ?? positionMode;

        const nextPosition: Position = {
          id: positionId,
          symbol,
          side,
          entryPrice: normalizedEntryPrice,
          notional: contracts * normalizedEntryPrice + normalizedUpnl,
          leverage: existingPosition?.leverage ?? 1,
          marginType: normalizedMarginType,
          positionMode: nextPositionMode,
          unrealizedPnl: normalizedUpnl,
          contracts,
          liquidationPrice: normalizedLiqPrice,
          info: positionUpdate,
        };

        this.store.addOrUpdatePosition(nextPosition);
      });
    });
  };

  private normalizeSymbol = (symbol: string) => {
    return String(symbol || '').replace(/-/g, '');
  };

  private extractAccountPositions = (event: Record<string, any>): Array<Record<string, any>> => {
    const candidates = [event?.a?.P, event?.a?.B, event?.a?.p];
    for (const candidate of candidates) {
      if (!Array.isArray(candidate)) continue;

      const positionLike = candidate.filter((row: Record<string, any>) => {
        if (!row || typeof row !== 'object') return false;
        if (typeof row.s !== 'string' || !row.s) return false;
        return (
          row.pa !== undefined ||
          row.positionAmt !== undefined ||
          row.ep !== undefined ||
          row.avgPrice !== undefined ||
          row.up !== undefined ||
          row.unrealizedProfit !== undefined
        );
      });

      if (positionLike.length > 0) {
        return positionLike;
      }
    }

    return [];
  };

  private schedulePositionRefresh = () => {
    this.hasPendingPositionEvents = true;
    const now = Date.now();

    if (this.isDisposed) return;
    if (this.positionFetchScheduled) return;

    const timeSinceLastFetch = now - this.lastPositionFetchTime;
    if (timeSinceLastFetch >= this.positionFetchThrottleMs) {
      this.executePositionRefresh();
      return;
    }

    this.positionFetchScheduled = true;
    setTimeout(() => {
      this.positionFetchScheduled = false;
      if (!this.isDisposed && this.hasPendingPositionEvents) {
        this.executePositionRefresh();
      }
    }, this.positionFetchThrottleMs - timeSinceLastFetch);
  };

  private executePositionRefresh = async () => {
    try {
      this.lastPositionFetchTime = Date.now();
      this.hasPendingPositionEvents = false;
      const positions = await this.parent.fetchPositions();
      if (this.isDisposed) return;

      this.store.addOrUpdatePositionsFull(positions);
      this.store.update({
        loaded: {
          ...this.store.loaded,
          positions: true,
        },
      });
    } catch {
      // Will retry on the next fill/account event.
    }
  };

  private fetchListenKey = async () => {
    const { data } = await this.parent.xhr.post(ENDPOINTS.LISTEN_KEY);
    const listenKey = data?.listenKey || data?.data?.listenKey;
    if (!listenKey) {
      throw new Error(`BingX listen key missing in response: ${JSON.stringify(data)}`);
    }
    this.listenKey = listenKey;
    return listenKey;
  };

  private updateListenKey = async () => {
    if (this.isDisposed) return;
    if (!this.listenKey) {
      await this.rebuildWithFreshListenKey();
      return;
    }

    try {
      await this.parent.xhr.put(ENDPOINTS.LISTEN_KEY, undefined, {
        params: {
          listenKey: this.listenKey,
        },
      });
      this.scheduleListenKeyRefresh();
    } catch (error: any) {
      const message = error?.response?.data?.msg || error?.message || String(error);
      this.parent.log(
        `Failed to refresh BingX listen key: ${message}. Retrying in ${BingxPrivateWebsocket.LISTEN_KEY_RETRY_MS / 1000}s`,
        LogSeverity.Warning,
      );

      if (error?.response?.status === 404 || /listen.?key.*(expired|does not exist|invalid)/i.test(message)) {
        await this.rebuildWithFreshListenKey();
      } else {
        this.scheduleListenKeyRefresh(BingxPrivateWebsocket.LISTEN_KEY_RETRY_MS);
      }
    }
  };

  private rebuildWithFreshListenKey = async () => {
    if (this.isDisposed || this.rebuildingListenKey) return;

    this.rebuildingListenKey = true;
    this.clearListenKeyRefreshTimer();
    this.listenKey = undefined;
    const staleSocket = this.ws;
    staleSocket?.removeEventListener('message', this.onMessage);
    staleSocket?.removeEventListener('close', this.onClose);
    staleSocket?.removeEventListener('open', this.onOpen);
    this.ws = undefined;
    staleSocket?.close();

    try {
      await this.connectAndSubscribe();
    } catch (error: any) {
      const message = error?.response?.data?.msg || error?.message || String(error);
      this.parent.log(
        `Failed to rebuild BingX private WebSocket with a fresh listen key: ${message}. Retrying in ${BingxPrivateWebsocket.LISTEN_KEY_RETRY_MS / 1000}s`,
        LogSeverity.Warning,
      );
      this.scheduleListenKeyRecovery();
    } finally {
      this.rebuildingListenKey = false;
    }
  };

  private scheduleListenKeyRefresh = (delayMs = BingxPrivateWebsocket.LISTEN_KEY_REFRESH_MS) => {
    this.clearListenKeyRefreshTimer();
    this.listenKeyRefreshTimer = setTimeout(() => void this.updateListenKey(), delayMs);
  };

  private scheduleListenKeyRecovery = () => {
    this.clearListenKeyRefreshTimer();
    this.listenKeyRefreshTimer = setTimeout(
      () => void this.rebuildWithFreshListenKey(),
      BingxPrivateWebsocket.LISTEN_KEY_RETRY_MS,
    );
  };

  private clearListenKeyRefreshTimer = () => {
    if (this.listenKeyRefreshTimer) {
      clearTimeout(this.listenKeyRefreshTimer);
      this.listenKeyRefreshTimer = undefined;
    }
  };

  dispose = () => {
    this.clearListenKeyRefreshTimer();
    this.listenKey = undefined;
    super.dispose();
  };
}
