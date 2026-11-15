import type { EngineSocketChannel, EngineSocketStats } from '@tealstreet/safe-cex-wasm/host';
import type { StoreData } from '../../types';
import type { MessageHandlersStats } from '../types';

/**
 * Rust venues watch their own sockets and reconnect after up to a minute of
 * silence, so a quieter socket than the TS 20 s threshold is still healthy.
 */
export const RUST_SOCKET_STALE_MS = 60_000;

export interface RustSocketOwner {
  /** Undefined until the engine host starts. */
  readonly socketHost: RustSocketHost | undefined;
  readonly isDisposed: boolean;
  readonly store?: Pick<StoreData, 'messageHandlerStats'>;
}

export interface RustSocketHost {
  readonly dead: boolean;
  socketHealth(): Record<EngineSocketChannel, EngineSocketStats>;
  reconnectSockets(channel?: EngineSocketChannel): number;
  redialWaitingSockets(): void;
}

const emptyStats: MessageHandlersStats = {};
const emptyLog = { toArray: (): string[] => [] };

/**
 * One engine socket channel exposed through the `BaseWebSocket` members that
 * app, CLI and base-exchange callers read (health, `lastReceived`,
 * `reconnect`, `recoverAfterOffline`). Venue reconnection stays in the engine:
 * reconnecting closes the channel's open sockets and the venue reopens them.
 */
export class RustSocketHandle {
  readonly lastLog = emptyLog;
  readonly isReconnectPauseActive = false;
  get messageHandlersStats(): MessageHandlersStats {
    return this.owner.store?.messageHandlerStats?.[this.channel] ?? emptyStats;
  }
  /** Patched by `disablePublicWebsockets`; the engine's feed policy suppresses public sockets. */
  connectAndSubscribe: () => void = () => {};

  constructor(
    readonly channel: EngineSocketChannel,
    private readonly owner: RustSocketOwner,
  ) {}

  private get stats(): EngineSocketStats | undefined {
    return this.owner.socketHost?.socketHealth()[this.channel];
  }

  get isConnected() {
    return (this.stats?.open ?? 0) > 0;
  }

  get isInitialized() {
    return (this.stats?.initialized ?? 0) > 0;
  }

  get lastReceived() {
    return this.stats?.lastReceived ?? 0;
  }

  get lastMessageTimestamp() {
    return this.lastReceived > 0 ? this.lastReceived : undefined;
  }

  get isHealthy() {
    return this.isConnected && this.lastReceived > 0 && Date.now() - this.lastReceived < RUST_SOCKET_STALE_MS;
  }

  get lastCloseCode() {
    return this.stats?.lastCloseCode;
  }

  get lastCloseAt() {
    return this.stats?.lastCloseAt ?? 0;
  }

  get lifetimeCloseCount() {
    return this.stats?.closes ?? 0;
  }

  get onCloseCount() {
    return this.stats?.closeStreak ?? 0;
  }

  getWatchdogStaleThresholdMs() {
    return RUST_SOCKET_STALE_MS;
  }

  reconnect = () => {
    this.owner.socketHost?.reconnectSockets(this.channel);
  };

  /** Close open sockets, then have the engine redial everything waiting out backoff. */
  recoverAfterOffline = (): boolean => {
    const host = this.owner.socketHost;
    // A dead or disposed engine cannot recover in place; the caller reloads the account.
    if (this.owner.isDisposed || host?.dead) return false;
    host?.reconnectSockets(this.channel);
    host?.redialWaitingSockets();
    return true;
  };

  forceClose = () => this.reconnect();

  forceOpen = () => {};

  /** The engine never falls back to a read-only private stream. */
  isReadonlyFallbackActive = () => false;

  dispose = () => {};
}
