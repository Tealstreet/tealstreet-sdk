import { configureHOW } from '@tealstreet/http-over-websocket/src/config';
import { getProxyToken, isTokenExpired } from '../accessToken';
import { SIMULATE_HTTP_OVER_WEBSOCKET } from '../config/config';
import { getUniqueLatencyDomains, latencyDomainToExchanges } from '../exchangeMetadata';
import { virtualClock } from '../utils/virtualClock';

const PROXY_TICKETS_URL = '/api/proxykey/tickets';
const TICKET_REFRESH_BUFFER_MS = 30 * 1000;

let cachedTunnelUrlTicket = '';
let cachedTunnelUrlTicketExpiresAt = 0;

async function getTunnelUrlTicket(): Promise<string> {
  if (typeof location === 'undefined' || typeof fetch === 'undefined') {
    return '';
  }

  if (cachedTunnelUrlTicket && Date.now() < cachedTunnelUrlTicketExpiresAt - TICKET_REFRESH_BUFFER_MS) {
    return cachedTunnelUrlTicket;
  }

  const response = await fetch(new URL(PROXY_TICKETS_URL, location.origin), {
    credentials: 'include',
    cache: 'no-store',
  });
  if (!response.ok) {
    return '';
  }

  const body = (await response.json()) as {
    tickets?: {
      tradeTunnelUrl?: {
        token?: unknown;
        expiresAt?: unknown;
      };
    };
  };
  const ticket = body.tickets?.tradeTunnelUrl;
  if (typeof ticket?.token !== 'string' || typeof ticket.expiresAt !== 'number') {
    return '';
  }

  cachedTunnelUrlTicket = ticket.token;
  cachedTunnelUrlTicketExpiresAt = ticket.expiresAt;
  return cachedTunnelUrlTicket;
}

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
