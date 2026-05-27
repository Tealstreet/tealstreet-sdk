import type { ExchangeLike } from '../types';

// DNS module is only available in Node.js, not in browser
 
let dnsLookup: ((hostname: string, callback: (err: Error | null) => void) => void) | null = null;

// Only import dns in Node.js environment
if (typeof window === 'undefined') {
  try {
    // Dynamic import to avoid bundling issues in browser
     
    dnsLookup = require('dns').lookup;
  } catch {
    // dns module not available
  }
}

const CHECK_INTERVAL_MS = 5_000; // Check every 5 seconds
const DNS_TIMEOUT_MS = 3_000; // DNS lookup timeout

export interface NetworkMonitorCallback {
  onOffline?: (message: string) => void;
  onOnline?: (message: string) => void;
}

/**
 * NetworkMonitor - monitors system network connectivity for CLI
 *
 * Uses DNS resolution to detect online/offline status and notifies
 * exchange instances via setOnlineStatus() to prevent reconnection
 * spam when network is unavailable.
 *
 * Usage:
 *   const monitor = new NetworkMonitor(exchange, {
 *     onOffline: (msg) => console.warn(msg),
 *     onOnline: (msg) => console.log(msg),
 *   });
 *   monitor.start();
 *   // ... later
 *   monitor.stop();
 */
export class NetworkMonitor {
  private exchange: ExchangeLike | null = null;
  private callbacks: NetworkMonitorCallback;
  private intervalId: NodeJS.Timeout | null = null;
  private isOnline: boolean = true;

  constructor(exchange: ExchangeLike | null, callbacks: NetworkMonitorCallback = {}) {
    this.exchange = exchange;
    this.callbacks = callbacks;
  }

  /**
   * Update the exchange being monitored (e.g., when connecting to a new account)
   */
  setExchange(exchange: ExchangeLike | null): void {
    this.exchange = exchange;
    // Immediately update the new exchange with current status
    if (exchange) {
      exchange.setOnlineStatus?.(this.isOnline);
    }
  }

  /**
   * Start monitoring network connectivity
   */
  start(intervalMs = CHECK_INTERVAL_MS): void {
    if (this.intervalId) {
      return;
    }

    // Initial check
    this.check();

    this.intervalId = setInterval(() => this.check(), intervalMs);
  }

  /**
   * Stop monitoring
   */
  stop(): void {
    if (this.intervalId) {
      clearInterval(this.intervalId);
      this.intervalId = null;
    }
  }

  /**
   * Get current network status
   */
  getIsOnline(): boolean {
    return this.isOnline;
  }

  /**
   * Check network connectivity using DNS resolution
   */
  private async check(): Promise<void> {
    const wasOnline = this.isOnline;

    try {
      await this.checkDns();
      this.isOnline = true;
    } catch {
      this.isOnline = false;
    }

    // Notify exchange of status change
    if (wasOnline !== this.isOnline) {
      this.exchange?.setOnlineStatus?.(this.isOnline);

      if (this.isOnline) {
        this.callbacks.onOnline?.('Network connection restored');
      } else {
        this.callbacks.onOffline?.('Network connection lost - reconnects paused');
      }
    }
  }

  /**
   * Perform DNS lookup with timeout
   * We use dns.google as it's globally accessible and highly reliable
   * In browser, falls back to navigator.onLine
   */
  private checkDns(): Promise<void> {
    return new Promise((resolve, reject) => {
      // In browser environment, use navigator.onLine as fallback
      if (!dnsLookup) {
        if (typeof navigator !== 'undefined' && navigator.onLine !== undefined) {
          if (navigator.onLine) {
            resolve();
          } else {
            reject(new Error('Browser reports offline'));
          }
        } else {
          // Can't determine status, assume online
          resolve();
        }
        return;
      }

      const timeout = setTimeout(() => {
        reject(new Error('DNS timeout'));
      }, DNS_TIMEOUT_MS);

      // Try resolving a well-known hostname
      dnsLookup('dns.google', (err) => {
        clearTimeout(timeout);
        if (err) {
          reject(err);
        } else {
          resolve();
        }
      });
    });
  }
}
