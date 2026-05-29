import { configureHOW } from '@tealstreet/http-over-websocket/src/config';
import { getProxyToken, isTokenExpired } from '../accessToken';
import { SIMULATE_HTTP_OVER_WEBSOCKET } from '../config/config';
import { getUniqueLatencyDomains, latencyDomainToExchanges } from '../exchangeMetadata';
import { virtualClock } from '../utils/virtualClock';
import { createTunnelUrlTicketProvider, PROXY_TICKETS_PATH } from './tunnelUrlTicket';

const getTunnelUrlTicket = createTunnelUrlTicketProvider({
  endpoint: () => {
    if (typeof location === 'undefined' || typeof fetch === 'undefined') {
      return null;
    }

    return new URL(PROXY_TICKETS_PATH, location.origin);
  },
  credentials: 'include',
});

/**
 * Initialize the HTTP-over-WebSocket package with safe-cex dependencies
 * This should be called early in the application startup
 */
export function initializeHOW(): void {
  configureHOW({
    getProxyToken,
    getTunnelUrlTicket,
    isTokenExpired,
    simulateHttpOverWebsocket: SIMULATE_HTTP_OVER_WEBSOCKET,
    syncServerTime: (serverTime: number) => {
      virtualClock.syncBybitClientServerTime(serverTime);
    },
    getUniqueLatencyDomains,
    latencyDomainToExchanges,
  });
}

// Auto-initialize when this module is imported
initializeHOW();
