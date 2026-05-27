import type { ExchangeLike, LastReceivedMessage } from '../types';

// Thresholds (in milliseconds)
const WARNING_THRESHOLD_MS = 10_000; // 10 seconds - show warning
const ERROR_THRESHOLD_MS = 30_000; // 30 seconds - consider disconnected
const INIT_GRACE_PERIOD_MS = 60_000; // 60 seconds - grace period after connecting

export type ConnectionHealth = 'healthy' | 'warning' | 'error' | 'initializing' | 'disconnected';

export interface ConnectionStatus {
  overall: ConnectionHealth;
  public: { status: ConnectionHealth; lastMessageAgo: number | null };
  private: { status: ConnectionHealth; lastMessageAgo: number | null };
  futures?: { status: ConnectionHealth; lastMessageAgo: number | null };
  business?: { status: ConnectionHealth; lastMessageAgo: number | null };
  latency?: number;
}

export interface MonitorCallback {
  onWarning?: (message: string) => void;
  onError?: (message: string) => void;
  onRecovered?: (message: string) => void;
  /** Called periodically to allow prompt refresh */
  onTick?: () => void;
}

/**
 * ConnectionMonitor - monitors exchange connection health for CLI
 *
 * Usage:
 *   const monitor = new ConnectionMonitor(exchange, {
 *     onWarning: (msg) => console.warn(msg),
 *     onError: (msg) => console.error(msg),
 *     onRecovered: (msg) => console.log(msg),
 *   });
 *   monitor.start();
 *   // ... later
 *   monitor.stop();
 */
export class ConnectionMonitor {
  private exchange: ExchangeLike;
  private callbacks: MonitorCallback;
  private intervalId: NodeJS.Timeout | null = null;
  private connectedAt: number;
  private lastStatus: ConnectionHealth = 'initializing';
  private hasEmittedWarning = false;

  constructor(exchange: ExchangeLike, callbacks: MonitorCallback = {}) {
    this.exchange = exchange;
    this.callbacks = callbacks;
    this.connectedAt = Date.now();
  }

  /**
   * Start monitoring (checks every 5 seconds)
   */
  start(intervalMs = 5000): void {
    if (this.intervalId) {
      return;
    }

    this.connectedAt = Date.now();
    this.intervalId = setInterval(() => this.check(), intervalMs);

    // Also listen for reconnect events
    this.exchange.emitter.on('ws-reconnected', this.handleReconnected);
  }

  /**
   * Stop monitoring
   */
  stop(): void {
    if (this.intervalId) {
      clearInterval(this.intervalId);
      this.intervalId = null;
    }
    this.exchange.emitter.off('ws-reconnected', this.handleReconnected);
  }

  /**
   * Get current connection status
   */
  getStatus(): ConnectionStatus {
    const now = Date.now();

    // Get lastReceivedMessage from store, or build it from WebSockets directly
    // (standalone CLI doesn't run postprocessors, so we check WebSockets)
    const lastReceived = this.getLastReceivedTimestamps();

    const publicStatus = this.getStreamStatus('public', lastReceived, now);
    const privateStatus = this.getStreamStatus('private', lastReceived, now);

    // Optional streams
    const futuresStatus = lastReceived?.futures !== undefined
      ? this.getStreamStatus('futures', lastReceived, now)
      : undefined;
    const businessStatus = lastReceived?.business !== undefined
      ? this.getStreamStatus('business', lastReceived, now)
      : undefined;

    // Overall status is worst of public/private
    const overall = this.combineStatus(publicStatus.status, privateStatus.status);

    return {
      overall,
      public: publicStatus,
      private: privateStatus,
      futures: futuresStatus,
      business: businessStatus,
      latency: this.exchange.store?.latency,
    };
  }

  /**
   * Get lastReceived timestamps - from store if available, otherwise from WebSockets directly
   */
  private getLastReceivedTimestamps(): LastReceivedMessage | undefined {
    const storeTimestamps = this.exchange.store?.lastReceivedMessage;

    // If store has timestamps, use them (web app with postprocessors)
    if (storeTimestamps && (storeTimestamps.public || storeTimestamps.private)) {
      return storeTimestamps;
    }

    // Otherwise, try to get from WebSockets directly (standalone CLI)
    const result: LastReceivedMessage = {};

    if (this.exchange.publicWebsocket?.lastReceived) {
      result.public = this.exchange.publicWebsocket.lastReceived;
    }
    if (this.exchange.privateWebsocket?.lastReceived) {
      result.private = this.exchange.privateWebsocket.lastReceived;
    }

    // Return undefined if we have no data at all
    if (result.public === undefined && result.private === undefined) {
      return undefined;
    }

    return result;
  }

  /**
   * Get a short status string suitable for prompt display
   * Examples: "[OK]", "[SLOW]", "[!CONN]"
   */
  getPromptStatus(): string {
    const status = this.getStatus();

    switch (status.overall) {
      case 'healthy':
        return '';
      case 'warning':
        return '[SLOW] ';
      case 'error':
        return '[!CONN] ';
      case 'initializing':
        return '[...] ';
      case 'disconnected':
        return '[DISCONNECTED] ';
    }
  }

  /**
   * Check connection and emit callbacks if needed
   */
  private check(): void {
    const status = this.getStatus();

    // Always call onTick for prompt refresh
    this.callbacks.onTick?.();

    // Don't alert during init grace period
    const timeSinceConnect = Date.now() - this.connectedAt;
    if (timeSinceConnect < INIT_GRACE_PERIOD_MS && status.overall === 'initializing') {
      return;
    }

    // Check for status changes
    if (status.overall !== this.lastStatus) {
      const prevStatus = this.lastStatus;
      this.lastStatus = status.overall;

      // Transitioned to warning/error
      if (status.overall === 'warning' && !this.hasEmittedWarning) {
        this.hasEmittedWarning = true;
        this.callbacks.onWarning?.(
          `Connection slow - no data for ${this.formatAge(status.public.lastMessageAgo || status.private.lastMessageAgo)}`,
        );
      } else if (status.overall === 'error') {
        this.callbacks.onError?.(
          `Connection issues - no data for ${this.formatAge(status.public.lastMessageAgo || status.private.lastMessageAgo)}. Run "conn reconnect" to retry.`,
        );
      }
      // Recovered
      else if (
        (prevStatus === 'warning' || prevStatus === 'error') &&
        status.overall === 'healthy'
      ) {
        this.hasEmittedWarning = false;
        this.callbacks.onRecovered?.('Connection recovered');
      }
    }
  }

  private handleReconnected = (): void => {
    // Reset warning state on reconnect
    this.hasEmittedWarning = false;
    this.callbacks.onRecovered?.('WebSocket reconnected');
  };

  private getStreamStatus(
    stream: keyof LastReceivedMessage,
    lastReceived: LastReceivedMessage | undefined,
    now: number,
  ): { status: ConnectionHealth; lastMessageAgo: number | null } {
    const timestamp = lastReceived?.[stream];

    if (timestamp === undefined) {
      return { status: 'initializing', lastMessageAgo: null };
    }

    const age = now - timestamp;

    if (age > ERROR_THRESHOLD_MS) {
      return { status: 'error', lastMessageAgo: age };
    }
    if (age > WARNING_THRESHOLD_MS) {
      return { status: 'warning', lastMessageAgo: age };
    }
    return { status: 'healthy', lastMessageAgo: age };
  }

  private combineStatus(a: ConnectionHealth, b: ConnectionHealth): ConnectionHealth {
    const priority: ConnectionHealth[] = ['disconnected', 'error', 'warning', 'initializing', 'healthy'];
    const aIdx = priority.indexOf(a);
    const bIdx = priority.indexOf(b);
    return aIdx < bIdx ? a : b;
  }

  private formatAge(ageMs: number | null): string {
    if (ageMs === null) return 'unknown time';
    if (ageMs < 1000) return `${ageMs}ms`;
    if (ageMs < 60_000) return `${(ageMs / 1000).toFixed(0)}s`;
    const minutes = Math.floor(ageMs / 60_000);
    return `${minutes}m`;
  }
}

/**
 * Get a simple one-shot connection status check
 */
export function getConnectionStatus(exchange: ExchangeLike): ConnectionStatus {
  const monitor = new ConnectionMonitor(exchange);
  return monitor.getStatus();
}

/**
 * Format connection status as a single line for display
 */
export function formatConnectionStatusLine(status: ConnectionStatus): string {
  const parts: string[] = [];

  const publicIcon = getStatusIcon(status.public.status);
  const privateIcon = getStatusIcon(status.private.status);

  parts.push(`Public:${publicIcon}`);
  parts.push(`Private:${privateIcon}`);

  if (status.latency !== undefined) {
    parts.push(`${status.latency}ms`);
  }

  return parts.join(' | ');
}

function getStatusIcon(status: ConnectionHealth): string {
  switch (status) {
    case 'healthy':
      return 'OK';
    case 'warning':
      return 'SLOW';
    case 'error':
      return 'ERR';
    case 'initializing':
      return '...';
    case 'disconnected':
      return 'X';
  }
}
