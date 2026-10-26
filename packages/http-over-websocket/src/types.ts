export interface LatencyEntry {
  latencyMs: number;
  source: string;
  cached: boolean;
  sampleCount: number;
  measuredAt: number;
  updatedAt: number;
}

export enum HOWWebSocketAuthState {
  Unknown = 'Unknown',
  Good = 'Good',
  Bad = 'Bad',
}

/**
 * Configuration for HTTP-over-WebSocket functionality
 * This allows the package to be used without direct dependencies on safe-cex internals
 */
export interface HOWConfig {
  /** Function to get the proxy token for authentication */
  getProxyToken: () => Promise<string>;
  /**
   * Binds the calling context's token lookup, for reads after awaits or in socket
   * callbacks. Multi-tenant hosts resolve tokens per account context.
   */
  bindProxyToken?: () => () => Promise<string>;
  /** Function to check if a token is expired */
  isTokenExpired: (token: string) => boolean;
  /** Whether to simulate HTTP over WebSocket (bypass tunnel, use direct HTTP) */
  simulateHttpOverWebsocket?: boolean;
  /** Function to sync the virtual clock with server time */
  syncServerTime?: (serverTime: number) => void;
  /** Function to get unique latency domains to monitor */
  getUniqueLatencyDomains?: () => string[];
  /** Map of latency domain to exchange names (for logging) */
  latencyDomainToExchanges?: Map<string, string[]>;
}

export type HOWRegion = 'JA' | 'SG';

export interface CreateAxiosHttpOverWebsocketAdapterOptions {
  customParser?: (str: string) => any;
  region?: HOWRegion;
  /** Host-provided, header-capable socket (for example a Workers fetch upgrade). */
  WebSocket?: typeof WebSocket;
}
