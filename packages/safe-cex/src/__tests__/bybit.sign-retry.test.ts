import type { InternalAxiosRequestConfig } from 'axios';
import type { RemoteSignerRequest } from '../types';

import axios from 'axios';
import createHmac from 'create-hmac';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { createAPI, isBybitSignRejection, shouldRetryBybitRequest } from './reference/bybit/bybit.api';

const NEW_SIGN_MESSAGE =
  'Error sign, please check your signature generation algorithm: origin_string[1791862469309keyx20000category=linear]';

const stubExchange = () => ({
  name: 'bybit',
  grafanaEndpointsEvery: [],
  grafanaEndpointsPeriodic: [],
  emitter: { emit: vi.fn() },
  emitAndLogError: vi.fn(),
  recordAuthSuccess: vi.fn(),
  recordAuthFailure: vi.fn(),
});

const ok = { retCode: 0, retMsg: 'OK', result: { list: [] } };
const signRejected = { retCode: 10004, retMsg: NEW_SIGN_MESSAGE };

/** Answers each request with the next body in turn and records what went out. */
const scriptedVenue = (bodies: unknown[]) => {
  const sent: InternalAxiosRequestConfig[] = [];
  const adapter = (config: InternalAxiosRequestConfig) => {
    sent.push(config);
    const data = bodies[Math.min(sent.length - 1, bodies.length - 1)];
    return Promise.resolve({ data, status: 200, statusText: 'OK', headers: {}, config });
  };
  return { sent, adapter };
};

const localKeys = { testnet: false, key: 'api-key', secret: 'api-secret' } as any;

describe('bybit signature rejection', () => {
  it('recognises both wordings Bybit has used, and the code', () => {
    expect(isBybitSignRejection({ ret_msg: 'error sign!' })).toBe(true);
    expect(isBybitSignRejection({ retMsg: NEW_SIGN_MESSAGE })).toBe(true);
    expect(isBybitSignRejection({ retCode: 10004 })).toBe(true);
    expect(isBybitSignRejection({ retCode: 10003, retMsg: 'API key is invalid.' })).toBe(false);
  });

  it('lets axios-retry resend a rejected response carrying the new wording, once', () => {
    const error = { config: { url: '/v5/position/list' }, response: { data: { retMsg: NEW_SIGN_MESSAGE } } };
    expect(shouldRetryBybitRequest(error)).toBe(true);
    expect(shouldRetryBybitRequest(error)).toBe(false);
  });

  it('re-signs and resends once, and a key that then works is never counted as bad', async () => {
    const exchange = stubExchange();
    const xhr = createAPI(localKeys, exchange as any);
    const venue = scriptedVenue([signRejected, ok]);
    xhr.defaults.adapter = venue.adapter as any;

    const response = await xhr.get('/v5/position/list', { params: { category: 'linear' } });

    expect(response.data).toEqual(ok);
    expect(venue.sent).toHaveLength(2);
    expect(venue.sent[1].params.requestId).not.toBe(venue.sent[0].params.requestId);
    expect(venue.sent[1].headers['X-BAPI-SIGN']).not.toBe(venue.sent[0].headers['X-BAPI-SIGN']);
    expect(exchange.recordAuthFailure).not.toHaveBeenCalled();
    expect(exchange.recordAuthSuccess).toHaveBeenCalledTimes(1);
  });

  it('surfaces a second rejection after exactly one resend, counted once', async () => {
    const exchange = stubExchange();
    const xhr = createAPI(localKeys, exchange as any);
    const venue = scriptedVenue([signRejected]);
    xhr.defaults.adapter = venue.adapter as any;

    const response = await xhr.post('/v5/order/create', { symbol: 'BTCUSDT' });

    expect(response.data).toEqual(signRejected);
    expect(venue.sent).toHaveLength(2);
    expect(exchange.recordAuthFailure).toHaveBeenCalledTimes(1);
  });

  it('signs a resent write over the body that is actually sent, not a re-quoted copy', async () => {
    const xhr = createAPI(localKeys, stubExchange() as any);
    const venue = scriptedVenue([signRejected, ok]);
    xhr.defaults.adapter = venue.adapter as any;

    await xhr.post('/v5/order/create', { symbol: 'BTCUSDT', qty: '0.001' });

    expect(venue.sent).toHaveLength(2);
    for (const config of venue.sent) {
      const { headers } = config;
      const expected = createHmac('sha256', 'api-secret')
        .update(`${headers['X-BAPI-TIMESTAMP']}api-key${headers['X-BAPI-RECV-WINDOW'] ?? ''}${config.data}`)
        .digest('hex');
      expect(headers['X-BAPI-SIGN']).toBe(expected);
    }
  });

  describe('hosted, signed remotely', () => {
    const originalAdapter = axios.defaults.adapter;
    afterEach(() => {
      axios.defaults.adapter = originalAdapter;
    });

    it('asks the remote signer again rather than resending the rejected signature', async () => {
      const exchange = stubExchange();
      const signed: RemoteSignerRequest[] = [];
      const remoteSigner = {
        accountName: 'acct',
        sign: async (request: RemoteSignerRequest) => {
          signed.push(request);
          return {
            id: request.id,
            config: {
              headers: { ...request.config!.headers, 'X-BAPI-SIGN': `remote-sig-${signed.length}` },
              params: request.config!.params,
            },
          };
        },
      };
      const xhr = createAPI({ testnet: false, remoteSigner } as any, exchange as any);
      // No transport adapter: the remote signer executes through axios' default
      // adapter LIST, which is the shape that leaves it on the resent config.
      const venue = scriptedVenue([signRejected, ok]);
      axios.defaults.adapter = [venue.adapter] as any;

      const response = await xhr.get('/v5/position/list', { params: { category: 'linear' } });

      expect(response.data).toEqual(ok);
      expect(signed).toHaveLength(2);
      expect(venue.sent.map((config) => config.headers['X-BAPI-SIGN'])).toEqual(['remote-sig-1', 'remote-sig-2']);
    });
  });
});
