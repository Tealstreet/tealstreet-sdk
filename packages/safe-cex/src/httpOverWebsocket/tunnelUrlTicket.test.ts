import { describe, expect, it, vi } from 'vitest';

import { createTunnelUrlTicketProvider, getProxyTicketsUrl } from './tunnelUrlTicket';

function jsonResponse(body: unknown, ok = true): Response {
  return {
    ok,
    json: vi.fn().mockResolvedValue(body),
  } as unknown as Response;
}

describe('tunnelUrlTicket', () => {
  it('builds the proxy ticket URL from an API base URL', () => {
    expect(getProxyTicketsUrl('https://preview.tealstreet.io')).toBe(
      'https://preview.tealstreet.io/api/proxykey/tickets',
    );
  });

  it('fetches and caches a trade-tunnel URL ticket', async () => {
    const now = 1_700_000_000_000;
    vi.spyOn(Date, 'now').mockReturnValue(now);
    const fetchImpl = vi.fn().mockResolvedValue(
      jsonResponse({
        tickets: {
          tradeTunnelUrl: {
            token: 'ticket-1',
            expiresAt: now + 60_000,
          },
        },
      }),
    );
    const getAuthToken = vi.fn().mockReturnValue('access-token');

    const getTicket = createTunnelUrlTicketProvider({
      endpoint: 'https://preview.tealstreet.io/api/proxykey/tickets',
      getAuthToken,
      fetchImpl: fetchImpl as unknown as typeof fetch,
    });

    await expect(getTicket()).resolves.toBe('ticket-1');
    await expect(getTicket()).resolves.toBe('ticket-1');

    expect(fetchImpl).toHaveBeenCalledTimes(1);
    expect(getAuthToken).toHaveBeenCalledTimes(1);
    const [, init] = fetchImpl.mock.calls[0];
    expect(init.cache).toBe('no-store');
    expect((init.headers as Headers).get('Authorization')).toBe('Bearer access-token');
  });

  it('refreshes cached tickets inside the refresh buffer', async () => {
    const now = 1_700_000_000_000;
    vi.spyOn(Date, 'now').mockReturnValue(now);
    const fetchImpl = vi
      .fn()
      .mockResolvedValueOnce(
        jsonResponse({
          tickets: {
            tradeTunnelUrl: {
              token: 'ticket-1',
              expiresAt: now + 20_000,
            },
          },
        }),
      )
      .mockResolvedValueOnce(
        jsonResponse({
          tickets: {
            tradeTunnelUrl: {
              token: 'ticket-2',
              expiresAt: now + 60_000,
            },
          },
        }),
      );

    const getTicket = createTunnelUrlTicketProvider({
      endpoint: 'https://preview.tealstreet.io/api/proxykey/tickets',
      fetchImpl: fetchImpl as unknown as typeof fetch,
    });

    await expect(getTicket()).resolves.toBe('ticket-1');
    await expect(getTicket()).resolves.toBe('ticket-2');
    expect(fetchImpl).toHaveBeenCalledTimes(2);
  });

  it('returns an empty ticket for unavailable or malformed responses', async () => {
    const getRejectedTicket = createTunnelUrlTicketProvider({
      endpoint: 'https://preview.tealstreet.io/api/proxykey/tickets',
      fetchImpl: vi.fn().mockResolvedValue(jsonResponse({}, false)) as unknown as typeof fetch,
    });
    const getMalformedTicket = createTunnelUrlTicketProvider({
      endpoint: 'https://preview.tealstreet.io/api/proxykey/tickets',
      fetchImpl: vi.fn().mockResolvedValue(jsonResponse({ tickets: {} })) as unknown as typeof fetch,
    });

    await expect(getRejectedTicket()).resolves.toBe('');
    await expect(getMalformedTicket()).resolves.toBe('');
  });

  it('keeps a still-valid cached ticket when proactive refresh throws', async () => {
    const now = 1_700_000_000_000;
    vi.spyOn(Date, 'now').mockReturnValue(now);
    const fetchImpl = vi
      .fn()
      .mockResolvedValueOnce(
        jsonResponse({
          tickets: {
            tradeTunnelUrl: {
              token: 'ticket-1',
              expiresAt: now + 20_000,
            },
          },
        }),
      )
      .mockRejectedValueOnce(new Error('network error'));

    const getTicket = createTunnelUrlTicketProvider({
      endpoint: 'https://preview.tealstreet.io/api/proxykey/tickets',
      fetchImpl: fetchImpl as unknown as typeof fetch,
    });

    await expect(getTicket()).resolves.toBe('ticket-1');
    await expect(getTicket()).resolves.toBe('ticket-1');
    expect(fetchImpl).toHaveBeenCalledTimes(2);
  });
});
