// Initialize HOW with safe-cex dependencies (side effect import)
import './configureHOW';

// Import what we need for getServerLatencyForExchange
import type { LatencyEntry } from '@tealstreet/http-over-websocket/src/types';

import { getHOWWebSocketForExchange } from '@tealstreet/http-over-websocket/src/axiosHttpOverWebsocketAdapter';
import { getExchangeMetadata } from '../exchangeMetadata';
import { ExchangeName } from '../types';

// Re-export everything from the http-over-websocket package
export {
  createAxiosHttpOverWebsocketAdapter,
  axiosHttpOverWebsocketAdapter,
  bigIntAxiosHttpOverWebsocketAdapter,
  howWebSocketSG,
  howWebSocketJA,
  getHOWWebSocketForExchange,
  getAllServerLatencies,
  getTunnelLatencyForExchange,
  getAllTunnelLatencies,
} from '@tealstreet/http-over-websocket/src/axiosHttpOverWebsocketAdapter';

export type { HOWRegion, CreateAxiosHttpOverWebsocketAdapterOptions } from '@tealstreet/http-over-websocket/src/types';
export { HOWWebSocketAuthState } from '@tealstreet/http-over-websocket/src/types';
export type { LatencyEntry } from '@tealstreet/http-over-websocket/src/types';

/**
 * Get server-side latency (from tradetunnel to exchange) for a given exchange
 * Returns undefined if not available
 *
 * This wrapper uses safe-cex's exchangeMetadata to get the latency domain
 */
export const getServerLatencyForExchange = (exchangeName: ExchangeName): LatencyEntry | undefined => {
  const metadata = getExchangeMetadata(exchangeName);
  if (!metadata.latencyDomain) {
    return undefined;
  }

  const howWebSocket = getHOWWebSocketForExchange(exchangeName);
  return howWebSocket.getLatencyForDomain(metadata.latencyDomain);
};
