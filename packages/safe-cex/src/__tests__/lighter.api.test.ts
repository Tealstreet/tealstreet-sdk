import { beforeEach, describe, expect, it, vi } from 'vitest';

import '../transport/registerTealstreetTransport';

import { createLighterSDKClient, stripUnsupportedLighterRestHeaders } from './reference/lighter/lighter.api';
import { getPublicCorsProxy } from '../transport/tealstreet/transportResolution';

const axiosMocks = vi.hoisted(() => {
  return {
    create: vi.fn(),
    requestUse: vi.fn(),
    responseUse: vi.fn(),
  };
});

vi.mock('axios', () => ({
  default: {
    create: axiosMocks.create,
  },
}));

describe('Lighter API client', () => {
  beforeEach(() => {
    axiosMocks.create.mockReset();
    axiosMocks.requestUse.mockReset();
    axiosMocks.responseUse.mockReset();
    axiosMocks.create.mockReturnValue({
      get: vi.fn(),
      post: vi.fn(),
      put: vi.fn(),
      delete: vi.fn(),
      interceptors: {
        request: { use: axiosMocks.requestUse },
        response: { use: axiosMocks.responseUse },
      },
    });
  });

  it('strips unsupported legacy X headers without removing explicit auth', () => {
    const headers = {
      'X-Timestamp': '123',
      'x-account-index': '7',
      authorization: 'auth-token',
    };

    stripUnsupportedLighterRestHeaders(headers);

    expect(headers).toEqual({
      authorization: 'auth-token',
    });
  });

  it('does not inject CORS-blocking timestamp headers through the request interceptor', () => {
    const exchange = { emitAndLogError: vi.fn(), name: 'lighter' };
    createLighterSDKClient(
      {
        apiKeyIndex: '7',
        testnet: false,
      } as any,
      exchange as any,
    );

    expect(axiosMocks.create).toHaveBeenCalledWith(
      expect.objectContaining({
        baseURL: expect.stringContaining('https://mainnet.zklighter.elliot.ai'),
      }),
    );
    expect(axiosMocks.requestUse).toHaveBeenCalledTimes(1);

    const requestInterceptor = axiosMocks.requestUse.mock.calls[0][0];
    const config = requestInterceptor({
      url: '/api/v1/accountActiveOrders',
      headers: {
        'X-Timestamp': '123',
        'X-Account-Index': '7',
        authorization: 'auth-token',
      },
    });

    expect(config.headers).toEqual({
      authorization: 'auth-token',
    });
  });

  it('uses the proxied Robinhood API host for the Robinhood exchange variant', () => {
    const exchange = { emitAndLogError: vi.fn(), name: 'lighter-robinhood' };
    createLighterSDKClient(
      {
        apiKeyIndex: '7',
        testnet: false,
      } as any,
      exchange as any,
    );

    expect(axiosMocks.create).toHaveBeenCalledWith(
      expect.objectContaining({
        baseURL: `${getPublicCorsProxy()}/https://api.rh.lighter.xyz`,
      }),
    );
  });
});
