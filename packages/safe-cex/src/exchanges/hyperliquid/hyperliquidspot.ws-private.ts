import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import {
  MarginType,
  Order,
  OrderSide,
  OrderStatus,
  OrderTimeInForce,
  OrderType,
  Position,
  PositionMode,
  PositionSide,
  Ticker,
  Writable,
} from '../../types';
import { createPositionId } from '../../utils/createPositionId';
import { virtualClock } from '../../utils/virtualClock';
import { BaseWebSocket } from '../base.ws';
import { systemSubscriptionTypes } from './hyperliquidspot.constants';
import { HyperliquidSpotExchange } from './hyperliquidspot.exchange';
import { UserFills, UserFundingEntry, UserNonFundingLedgerEntry, WebData2, WSS_URLS } from './hyperliquidspot.types';

export class HyperliquidSpotPrivateWebSocket extends BaseWebSocket<HyperliquidSpotExchange> {
  constructor(parent: HyperliquidSpotExchange) {
    super(parent);
  }

  connectAndSubscribe = (_isReconnect = false) => {
    if (!this.isDisposed) {
      this.ws = this.createWebsocket(WSS_URLS[this.parent.options.testnet ? 'TESTNET' : 'PRODUCTION']);

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      this.subscribe();
      this.ping();
      this.parent.requestSpotBalanceRefresh(true);
    }
  };

  subscribe = () => {
    this.subscribeSystemTopics();
    this.subscribeCoins();
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.send(JSON.stringify({ method: 'ping' }));
    }
  };

  subscribeSystemTopics = () => {
    for (const type of systemSubscriptionTypes) {
      this.send(
        JSON.stringify({
          method: 'subscribe',
          subscription: {
            type,
            user: this.parent.options.vault || this.parent.options.applicationId,
          },
        }),
      );
    }
  };

  subscribeCoins = () => {
    for (const market of this.parent.store.markets) {
      this.send(
        JSON.stringify({
          method: 'subscribe',
          subscription: {
            type: 'activeAssetData',
            user: this.parent.options.vault || this.parent.options.applicationId,
            coin: market.id,
          },
        }),
      );
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();

      try {
        const json = jsonParse(data);
        this.recordMessageHandlerStats(json?.channel);
        // Deliberate hack: bump 'pong' stats on every frame, not just real
        // pongs. Hyperliquid's pong cadence is unreliable; without this the
        // staleness watchdog flags the socket as dead and reconnects even
        // when tickers/orderbooks are flowing normally.
        this.recordMessageHandlerStats('pong');

        switch (json?.channel) {
          case 'activeAssetData':
            if (json?.data?.coin) {
              this.parent.activeAssetData.set(json?.data?.coin, json?.data || {});
              // Use O(1) hashmap lookup instead of O(n) find
              const market = this.store.marketsById[json?.data?.coin];
              if (market) {
                // Indexed scan with `continue` instead of
                // `positions.filter(...)`. And drop the `{ ...position }`
                // spread — `updatePosition` writes only the fields in
                // `changes` onto the live position; spreading every field
                // back was wasted work.
                const allPositions = this.store.positions;
                const symbolConfiguration = this.parent._getSymbolConfigurationFromAssetData(json?.data?.coin);
                for (let i = 0; i < allPositions.length; i++) {
                  const position = allPositions[i];
                  if (position.symbol !== market.symbol) continue;
                  const isOpenPosition = position.contracts > 0;
                  this.parent.store.updatePosition(position, {
                    positionMode: symbolConfiguration.positionMode,
                    marginType: isOpenPosition ? position.marginType : symbolConfiguration.marginType,
                    leverage: isOpenPosition ? position.leverage : symbolConfiguration.leverage,
                  });
                }
              }
            }
            break;
          case 'spotClearinghouseState':
            this.handleSpotClearinghouseState(json?.data || {});
            break;
          case 'spotState':
            this.handleSpotClearinghouseState(json?.data?.spotState || json?.data || {});
            break;
          case 'pong':
            this.handlePongEvent();
            break;
          case 'webData2':
            this.handleWebData2(json?.data || {});
            break;
          case 'userFills':
            this.handleFills(json?.data || {});
            break;
          case 'userFundings':
            this.handleFundings(json?.data || {});
            break;
          case 'userNonFundingLedgerUpdates':
            this.handleUserNonFundingLedgerUpdates(json?.data || {});
            break;
        }
      } catch (_e) {}
    }
  };

  // Reusable update buffer — same pattern as the perp WS's
  // `_webData2UpdateBuf`. Avoids per-ticker `{ ... }` allocation on the
  // webData2 hot path.
  private _webData2UpdateBuf: Partial<Writable<Ticker>> = {
    mark: 0,
    last: 0,
    midPrice: 0,
    index: 0,
    bid: 0,
    ask: 0,
    openInterest: 0,
    fundingRate: 0,
    volume: 0,
  };

  handleWebData2 = (data: WebData2) => {
    if (data?.meta?.universe && Array.isArray(data?.meta?.universe)) {
      try {
        // handle market data changes
        // Use O(1) hashmap lookup instead of O(n) find in loop
        const tickersById = this.parent.store.tickersById;
        const universe = data.meta.universe;
        const assetCtxs = data?.assetCtxs;
        const updateBuf = this._webData2UpdateBuf;
        for (let i = 0; i < universe.length; i++) {
          const asset = assetCtxs?.[i];
          if (!asset) continue;
          const ticker = tickersById[universe[i].name];
          if (!ticker) continue;

          const midPx = parseFloat(asset.midPx);
          updateBuf.mark = parseFloat(asset.markPx);
          updateBuf.last = midPx;
          updateBuf.midPrice = midPx;
          updateBuf.index = parseFloat(asset.oraclePx);
          updateBuf.bid = parseFloat(asset.impactPxs?.[0] || asset.midPx);
          updateBuf.ask = parseFloat(asset.impactPxs?.[1] || asset.midPx);
          updateBuf.openInterest = parseFloat(asset.openInterest);
          updateBuf.fundingRate = parseFloat(asset.funding);
          updateBuf.volume = parseFloat(asset.dayNtlVlm);

          this.store.updateTicker(ticker, updateBuf);
        }
      } catch (e) {
        console.error(e);
      }
    }

    if (data?.clearinghouseState?.assetPositions) {
      // Use O(1) hashmap lookup instead of O(n) find in map.
      // Indexed for-loop with `continue` instead of `.map(...).filter(...)`,
      // and Map-based merge with fakes instead of `[...spread] + uniqBy`.
      // Same pattern as the perp WS.
      const marketsById = this.parent.store.marketsById;
      const fakes = this.parent.getFakePositions();
      const merged = new Map<string, Position>();
      // Insert fakes first (lower priority); incoming overwrites on conflict.
      for (let i = 0; i < fakes.length; i++) merged.set(fakes[i].id, fakes[i]);

      const rawAssetPositions = data.clearinghouseState.assetPositions;
      for (let i = 0; i < rawAssetPositions.length; i++) {
        const assetPosition = rawAssetPositions[i];
        const market = marketsById[assetPosition.position.coin];
        if (!market) continue;
        const side = +assetPosition.position.szi > 0 ? PositionSide.Long : PositionSide.Short;
        const incoming: Position = {
          id: createPositionId(
            market.symbol,
            side,
            assetPosition.type === 'oneWay' ? PositionMode.OneWay : PositionMode.Hedged,
          ),
          symbol: market.symbol,
          side,
          entryPrice: +assetPosition.position.entryPx,
          notional: +assetPosition.position.positionValue,
          leverage: +assetPosition.position.leverage.value,
          unrealizedPnl: +assetPosition.position.unrealizedPnl,
          contracts: Math.abs(+assetPosition.position.szi),
          liquidationPrice: +assetPosition.position.liquidationPx,
          positionMode: PositionMode.OneWay,
          marginType: assetPosition.position.leverage.type === 'cross' ? MarginType.Cross : MarginType.Isolated,
        };
        merged.set(incoming.id, incoming);
      }

      this.parent.store.addOrUpdatePositionsFull(Array.from(merged.values()));
    }

    if (data?.openOrders) {
      // Use O(1) hashmap lookup instead of O(n) find in map.
      // Single indexed for-loop replaces filter().map().filter() — was
      // allocating three intermediate arrays + two closure pools.
      const marketsById2 = this.parent.store.marketsById;
      const rawOpenOrders = data.openOrders;
      const orders: Order[] = [];
      for (let i = 0; i < rawOpenOrders.length; i++) {
        const openOrder = rawOpenOrders[i];
        const oid = `${openOrder.oid}`;
        if (this.parent._deadOrders.has(oid)) continue;
        const market = marketsById2[openOrder.coin];
        if (!market) continue;
        orders.push({
          id: oid,
          status: OrderStatus.Open,
          symbol: market.symbol,
          type:
            openOrder.orderType === 'Limit'
              ? OrderType.Limit
              : openOrder.orderType === 'Take Profit Market'
                ? OrderType.TakeProfit
                : openOrder.orderType === 'Stop Market'
                  ? OrderType.StopLoss
                  : OrderType.Market,
          side: openOrder.side === 'B' ? OrderSide.Buy : OrderSide.Sell,
          price:
            openOrder.orderType === 'Limit' && openOrder.limitPx
              ? parseFloat(openOrder.limitPx)
              : openOrder.triggerPx
                ? parseFloat(openOrder.triggerPx)
                : 0,
          amount: +openOrder.origSz,
          filled: +openOrder.origSz - +openOrder.sz,
          remaining: +openOrder.sz,
          reduceOnly: Boolean(openOrder.reduceOnly),
          lastUpdate: virtualClock.getCurrentTimeMs(),
          timeInForce:
            openOrder.tif === 'Gtc'
              ? OrderTimeInForce.GoodTillCancel
              : openOrder.tif === 'Alo'
                ? OrderTimeInForce.PostOnly
                : undefined,
          info: openOrder,
        });
      }
      this.parent.store.addOrUpdateOrdersFull(orders);
    }
  };

  private handleSpotClearinghouseState = (data: {
    balances?: Array<{ coin: string; hold: string; total: string; token?: number }>;
  }) => {
    this.parent.applySpotBalances(data?.balances || []);
  };

  handleFills = (data: { fills: UserFills[]; isSnapshot: boolean; user: string }) => {
    if (data.isSnapshot) return;

    const fills = data.fills || [];
    // Use O(1) hashmap lookup instead of O(n) find fallback
    const marketsById = this.parent.store.marketsById;

    // Indexed for-loop over forEach — drops iterator + closure overhead
    // when many fills arrive in one frame.
    for (let i = 0; i < fills.length; i++) {
      const fill = fills[i] as any;
      this.parent.emitter.emit('fill', {
        id: `${fill.tid}`,
        side: fill.side === 'B' ? OrderSide.Buy : OrderSide.Sell,
        symbol: marketsById[fill.coin]?.symbol || fill.coin,
        price: parseFloat(fill.px),
        amount: parseFloat(fill.sz),
      });
    }

    if (fills.length > 0) {
      this.parent.requestSpotBalanceRefresh(true);
    }
  };

  handleFundings = (_data: { fundings: UserFundingEntry[]; isSnapshot: boolean; user: string }) => {};

  handleUserNonFundingLedgerUpdates = (_data: {
    nonFundingLedgerUpdates: UserNonFundingLedgerEntry[];
    isSnapshot: boolean;
    user: string;
  }) => {
    this.parent.requestSpotBalanceRefresh();
  };
}
