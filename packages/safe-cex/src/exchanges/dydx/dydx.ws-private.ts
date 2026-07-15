import { LogSeverity } from '../../types';
import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { BaseWebSocket } from '../base.ws';
import { DydxExchange } from './dydx.exchange';
import {
  DydxWsChannelData,
  DydxWsSubaccountUpdate,
  DydxWsSubscribeMessage,
  WS_CHANNELS,
  WSS_URLS,
} from './dydx.types';

/**
 * dYdX v4 Private WebSocket client
 * Handles real-time private account updates: orders, positions, fills
 */
export class DydxPrivateWebSocket extends BaseWebSocket<DydxExchange> {
  private subscriptions = new Set<string>();
  private address: string = '';
  private subaccountNumber: number = 0;

  constructor(parent: DydxExchange) {
    super(parent);
  }

  /**
   * Set the address and subaccount number for private subscriptions
   */
  setCredentials(address: string, subaccountNumber: number) {
    this.address = address;
    this.subaccountNumber = subaccountNumber;
  }

  connectAndSubscribe = (_isReconnect = false) => {
    if (!this.isDisposed) {
      const wsUrl = WSS_URLS[this.parent.options.testnet ? 'TESTNET' : 'PRODUCTION'];
      this.ws = this.createWebsocket(wsUrl);

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', () => {
        this.onClose();
      });
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      this.subscriptions.clear();

      // Subscribe to subaccount if credentials are set
      if (this.address) {
        this.subscribeToSubaccount(this.address, this.subaccountNumber);
      }

      // Fetch fresh private data on reconnect
      this.parent.fetchAndSetPrivateSnapshot(10);
    }
  };

  /**
   * Subscribe to private subaccount updates
   * Receives orders, fills, and positions updates
   */
  subscribeToSubaccount = (address: string, subaccountNumber: number) => {
    const channel = WS_CHANNELS.SUBACCOUNT;
    const id = `${address}/${subaccountNumber}`;
    const subscriptionKey = `${channel}:${id}`;

    if (this.subscriptions.has(subscriptionKey)) {
      return;
    }

    const message: DydxWsSubscribeMessage = {
      type: 'subscribe',
      channel,
      id,
    };

    this.send(JSON.stringify(message));
    this.subscriptions.add(subscriptionKey);
    this.parent.log(`Subscribed to subaccount ${id}`, LogSeverity.Info);
  };

  /**
   * Unsubscribe from subaccount updates
   */
  unsubscribeFromSubaccount = (address: string, subaccountNumber: number) => {
    const channel = WS_CHANNELS.SUBACCOUNT;
    const id = `${address}/${subaccountNumber}`;
    const subscriptionKey = `${channel}:${id}`;

    if (!this.subscriptions.has(subscriptionKey)) {
      return;
    }

    const message: DydxWsSubscribeMessage = {
      type: 'unsubscribe',
      channel,
      id,
    };

    this.send(JSON.stringify(message));
    this.subscriptions.delete(subscriptionKey);
    this.parent.log(`Unsubscribed from subaccount ${id}`, LogSeverity.Info);
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();

      try {
        const json = jsonParse(data);
        if (!json) return;

        const messageType = json.type;
        const channel = json.channel;

        // Record stats
        this.recordMessageHandlerStats(channel);

        // Handle subscription responses
        if (messageType === 'subscribed') {
          this.parent.log(`Private subscription confirmed for ${channel}`, LogSeverity.Info);
          if (channel === WS_CHANNELS.SUBACCOUNT && json.contents) {
            this.handleSubaccountUpdate(json.contents as DydxWsSubaccountUpdate);
          }
          return;
        }

        if (messageType === 'unsubscribed') {
          this.parent.log(`Private unsubscription confirmed for ${channel}`, LogSeverity.Info);
          return;
        }

        if (messageType === 'error') {
          this.parent.log(`Private WebSocket error: ${json.message || 'Unknown error'}`, LogSeverity.Error);
          return;
        }

        // Handle channel data (subaccount uses channel_data, not batch)
        if (messageType === 'channel_data') {
          this.handleChannelData(json as DydxWsChannelData);
        }
      } catch (e) {
        this.parent.log(`Error parsing private WebSocket message: ${e}`, LogSeverity.Error);
      }
    }
  };

  /**
   * Handle individual channel data messages
   */
  private handleChannelData = (message: DydxWsChannelData) => {
    const { channel, contents } = message;

    if (channel === WS_CHANNELS.SUBACCOUNT) {
      this.handleSubaccountUpdate(contents as DydxWsSubaccountUpdate);
    }
  };

  /**
   * Handle subaccount updates
   * Updates orders, positions, and fills
   */
  private handleSubaccountUpdate = (data: DydxWsSubaccountUpdate) => {
    this.parent.handleSubaccountUpdate(data);

    // Emit the raw subaccount update for the exchange to process
    this.parent.emitter.emit('subaccount-update', data);

    // Process orders if present
    if (data.orders && Array.isArray(data.orders)) {
      for (const order of data.orders) {
        this.parent.emitter.emit('order-update', order);
      }
    }

    // Process fills if present
    if (data.fills && Array.isArray(data.fills)) {
      for (const fill of data.fills) {
        this.parent.emitter.emit('fill-update', fill);
      }
    }

    // Process positions if present
    if (data.positions && Array.isArray(data.positions)) {
      for (const position of data.positions) {
        this.parent.emitter.emit('position-update', position);
      }
    }

    // Process subaccount data (contains balance info) if present
    if (data.subaccount) {
      this.parent.emitter.emit('balance-update', data.subaccount);
    }
  };

  /**
   * dYdX doesn't require explicit ping, connection is maintained by server
   */
  ping = (): void => {
    // No-op: dYdX maintains the connection automatically
  };
}
