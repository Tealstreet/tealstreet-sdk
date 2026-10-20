import type { HOWConfig } from './types';

// Default configuration with no-op functions
// Must be configured before use via configureHOW()
let config: HOWConfig = {
  getProxyToken: async () => {
    throw new Error('HOW not configured: getProxyToken not set. Call configureHOW() first.');
  },
  isTokenExpired: () => {
    throw new Error('HOW not configured: isTokenExpired not set. Call configureHOW() first.');
  },
  simulateHttpOverWebsocket: false,
  syncServerTime: undefined,
  getUniqueLatencyDomains: () => [],
  latencyDomainToExchanges: new Map(),
};

/**
 * Configure the HTTP-over-WebSocket module with necessary dependencies
 * This must be called before using HOW functionality
 */
export function configureHOW(newConfig: Partial<HOWConfig>): void {
  config = { ...config, ...newConfig };
}

/**
 * Get the current HOW configuration
 */
export function getHOWConfig(): HOWConfig {
  return config;
}

/**
 * Get proxy token (calls the configured function)
 */
export async function getProxyToken(): Promise<string> {
  return config.getProxyToken();
}

/** A token lookup bound to the calling context; call synchronously at request or socket start. */
export function bindProxyToken(): () => Promise<string> {
  return config.bindProxyToken?.() ?? getProxyToken;
}

/**
 * Check if token is expired (calls the configured function)
 */
export function isTokenExpired(token: string): boolean {
  return config.isTokenExpired(token);
}

/**
 * Get whether to simulate HTTP over WebSocket
 */
export function shouldSimulateHttpOverWebsocket(): boolean {
  return config.simulateHttpOverWebsocket ?? false;
}

/**
 * Sync server time if configured
 */
export function syncServerTime(serverTime: number): void {
  if (config.syncServerTime) {
    config.syncServerTime(serverTime);
  }
}

/**
 * Get unique latency domains to monitor
 */
export function getUniqueLatencyDomains(): string[] {
  return config.getUniqueLatencyDomains?.() ?? [];
}

/**
 * Get the mapping of latency domain to exchange names
 */
export function getLatencyDomainToExchanges(): Map<string, string[]> {
  return config.latencyDomainToExchanges ?? new Map();
}
