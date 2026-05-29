export const PROXY_TICKETS_PATH = '/api/proxykey/tickets';

const TICKET_REFRESH_BUFFER_MS = 30 * 1000;

type MaybePromise<T> = T | Promise<T>;

interface ProxyTicketResponse {
  tickets?: {
    tradeTunnelUrl?: {
      token?: unknown;
      expiresAt?: unknown;
    };
  };
}

export interface TunnelUrlTicketProviderOptions {
  endpoint: string | URL | (() => MaybePromise<string | URL | null | undefined>);
  getAuthToken?: () => MaybePromise<string | null | undefined>;
  credentials?: RequestCredentials;
  fetchImpl?: typeof fetch;
}

export function getProxyTicketsUrl(apiBaseUrl: string): string {
  return new URL(PROXY_TICKETS_PATH, apiBaseUrl).toString();
}

export function createTunnelUrlTicketProvider(options: TunnelUrlTicketProviderOptions): () => Promise<string> {
  let cachedToken = '';
  let cachedExpiresAt = 0;

  return async () => {
    const hasValidCachedTicket = () => cachedToken !== '' && Date.now() < cachedExpiresAt;

    if (cachedToken && Date.now() < cachedExpiresAt - TICKET_REFRESH_BUFFER_MS) {
      return cachedToken;
    }

    try {
      const fetchImpl = options.fetchImpl ?? globalThis.fetch;
      if (typeof fetchImpl !== 'function') {
        return hasValidCachedTicket() ? cachedToken : '';
      }

      const endpoint = typeof options.endpoint === 'function' ? await options.endpoint() : options.endpoint;
      if (!endpoint) {
        return hasValidCachedTicket() ? cachedToken : '';
      }

      let headers: Headers | undefined;
      const authToken = await options.getAuthToken?.();
      if (authToken) {
        headers = new Headers();
        headers.set('Authorization', `Bearer ${authToken}`);
      }

      const response = await fetchImpl(endpoint, {
        credentials: options.credentials,
        cache: 'no-store',
        headers,
      });
      if (!response.ok) {
        return hasValidCachedTicket() ? cachedToken : '';
      }

      const body = (await response.json()) as ProxyTicketResponse;
      const ticket = body.tickets?.tradeTunnelUrl;
      if (typeof ticket?.token !== 'string' || typeof ticket.expiresAt !== 'number') {
        return hasValidCachedTicket() ? cachedToken : '';
      }

      cachedToken = ticket.token;
      cachedExpiresAt = ticket.expiresAt;
      return cachedToken;
    } catch {
      return hasValidCachedTicket() ? cachedToken : '';
    }
  };
}
