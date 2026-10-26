import { createHmac } from 'node:crypto';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { createAPI } from '../reference/apexomni/apexomni.api';
import { virtualClock } from '../../utils/virtualClock';

const now = 1_700_000_000_123;
const credentials = { key: 'api-key', secret: 'mysecret', passphrase: 'pass' };
const signature = (message: string) =>
  createHmac('sha256', Buffer.from('mysecret').toString('base64')).update(message).digest('base64');

describe('ApeX native signing contract and retained original payload', () => {
  afterEach(() => vi.restoreAllMocks());

  it.each([
    ['/v3/delete-order', { id: 'order-1' }, 'id=order-1', 'application/x-www-form-urlencoded'],
    [
      '/v3/set-initial-margin-rate',
      { symbol: 'BTC-USDT', token: 'USDT', initialMarginRate: '0.10000' },
      'initialMarginRate=0.10000&symbol=BTC-USDT&token=USDT',
      'multipart/form-data',
    ],
  ] as const)(
    'signs original fields for a single legacy resend of %s',
    async (path, fields, canonical, contentType) => {
      vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(now);
      const emitAndLogError = vi.fn();
      const api = createAPI(credentials as any, { name: 'apexomni', emitAndLogError } as any);
      const attempts: Array<{ headers: Record<string, any>; data: unknown }> = [];
      api.defaults.adapter = async (config) => {
        attempts.push({ headers: config.headers.toJSON(), data: config.data });
        return {
          status: 200,
          statusText: '200',
          headers: {},
          config,
          data:
            attempts.length === 1 ? { code: 20016, msg: 'signature rejected' } : { code: 0, data: { id: 'accepted' } },
        };
      };
      await expect(api.post(path, fields, { headers: { 'Content-Type': contentType } })).resolves.toMatchObject({
        data: { code: 0 },
      });
      expect(attempts).toHaveLength(2);
      expect(attempts[0].headers['APEX-SIGNATURE']).toBe(signature(`${now}POST/api${path}${canonical}`));
      expect(attempts[1].headers['APEX-SIGNATURE']).toBe(signature(`1700000000000POST/api${path}${canonical}`));
      expect(attempts[1].headers['APEX-TIMESTAMP']).toBe('1700000000000');
      expect(emitAndLogError).not.toHaveBeenCalled();
      if (path.includes('delete')) expect(attempts[0].data).toBe('id=order-1');
      else expect(typeof attempts[0].data).toBe('object');
    },
  );

  it('keeps raw signed order bytes stable and refuses a third signature attempt', async () => {
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(now);
    const api = createAPI(credentials as any, { name: 'apexomni', emitAndLogError: vi.fn() } as any);
    const attempts: Array<{ signature: string; body: string }> = [];
    api.defaults.adapter = async (config) => {
      attempts.push({ signature: String(config.headers.get('APEX-SIGNATURE')), body: config.data });
      return { status: 200, statusText: '200', headers: {}, config, data: { code: 20016, msg: 'signature rejected' } };
    };
    const body = 'clientId=caller-01&size=1';
    await expect(
      api.post('/v3/order', body, {
        headers: {
          'Content-Type': 'application/x-www-form-urlencoded; charset=UTF-8',
        },
      }),
    ).rejects.toThrow('20016');
    expect(attempts).toEqual([
      { signature: signature(`${now}POST/api/v3/order${body}`), body },
      { signature: signature(`1700000000000POST/api/v3/order${body}`), body },
    ]);
  });

  it('retains a numeric zero GET bound on the wire but omits it from the legacy signature', async () => {
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(now);
    const api = createAPI(credentials as any, { name: 'apexomni', emitAndLogError: vi.fn() } as any);
    const attempts: Array<{ signature: string; begin: number }> = [];
    api.defaults.adapter = async (config) => {
      attempts.push({
        signature: String(config.headers.get('APEX-SIGNATURE')),
        begin: config.params.beginTimeInclusive,
      });
      return {
        status: 200,
        statusText: '200',
        headers: {},
        config,
        data: attempts.length === 1 ? { code: 20016, msg: 'signature rejected' } : { code: 0, data: [] },
      };
    };
    await api.get('/v3/fills', { params: { beginTimeInclusive: 0, symbol: 'BTC-USDT' } });
    expect(attempts).toEqual([
      { signature: signature(`${now}GET/api/v3/fills?beginTimeInclusive=0&symbol=BTC-USDT`), begin: 0 },
      { signature: signature('1700000000000GET/api/v3/fills?symbol=BTC-USDT'), begin: 0 },
    ]);
  });
});
