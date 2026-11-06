import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { OrderSide } from '../../../types';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { systemSubscriptionTypes } from './hyperliquidspot.constants';
import { HyperliquidSpotExchange } from './hyperliquidspot.exchange';
import { UserFills, UserFundingEntry, UserNonFundingLedgerEntry, WSS_URLS } from './hyperliquidspot.types';

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
        orderId: `${fill.oid}`,
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
