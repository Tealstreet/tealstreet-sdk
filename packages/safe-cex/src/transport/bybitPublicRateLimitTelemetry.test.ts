import { describe, expect, it } from 'vitest';

import {
  bybitPublicRateLimitNotice as notice,
  observeBybitPublicRateLimit as observe,
  bybitPublicRateLimitReport as report,
} from './bybitPublicRateLimitTelemetry';

const request = {
  method: 'GET',
  url: 'https://api.bybit.com/v5/market/kline?category=inverse&symbol=BTCUSD&interval=1',
  headers: {},
};
const limited = '{"retCode":10006,"retMsg":"secret body excluded"}';

describe('Bybit public rate-limit telemetry', () => {
  it('works with old tunnels and reports bounded response metadata only', () => {
    expect(notice(request, limited, {})).toEqual({ retCode: 10006, egress: 'unknown', target: 'unknown' });
    const result = notice(request, limited, {
      'X-Tealstreet-Egress': 'proxy-02',
      'X-Tealstreet-Egress-Target': 'tokyo-02',
      'x-bapi-limit': '120',
      'x-bapi-limit-status': '0',
      'x-bapi-limit-reset-timestamp': '1800000000000',
      'Retry-After': '10',
      Authorization: 'must-not-leak',
    })!;
    expect(result).toEqual({
      retCode: 10006,
      egress: 'proxy-02',
      target: 'tokyo-02',
      limit: '120',
      remaining: '0',
      resetTimestamp: '1800000000000',
      retryAfter: '10',
    });
    const serialized = JSON.stringify(report('bybitinverse', result));
    expect(serialized).toContain('bybitinverse');
    for (const excluded of ['must-not-leak', 'secret body', 'BTCUSD', 'https:', 'retMsg'])
      expect(serialized).not.toContain(excluded);
  });
  it('refuses private, signed, ambiguous and non-rate reports', () => {
    for (const name of ['X-BAPI-SIGN', 'Authorization', 'cookie', 'X-BAPI-API-KEY'])
      expect(notice({ ...request, headers: { [name]: 'x' } }, limited, {})).toBeUndefined();
    for (const suffix of ['&%73ign=x', '&apiKey=x', '&interval=2', '&unknown=x'])
      expect(notice({ ...request, url: request.url + suffix }, limited, {})).toBeUndefined();
    for (const url of [
      'https://api.bybit.com/v5/order/history',
      'https://api.bybit.com.evil/v5/market/kline',
      'http://api.bybit.com/v5/market/kline',
    ])
      expect(notice({ ...request, url }, limited, {})).toBeUndefined();
    expect(notice({ ...request, method: 'POST' }, limited, {})).toBeUndefined();
    expect(notice({ ...request, body: 'x' }, limited, {})).toBeUndefined();
    for (const body of ['{}', 'malformed', '{"retCode":"10006"}', '{"retCode":0}', 'x'.repeat(1024 * 1024 + 1)])
      expect(notice(request, body, {})).toBeUndefined();
  });
  it.each([10006, 10018, 429])('recognizes documented native rate code %s', (retCode) => {
    expect(notice(request, JSON.stringify({ retCode }), {})).toEqual({ retCode, egress: 'unknown', target: 'unknown' });
  });
  it('uses status independently of the body while retaining public request validation', () => {
    for (const body of ['', 'malformed', '{"retCode":10001}', 'x'.repeat(1024 * 1024 + 1)]) {
      expect(notice(request, body, {}, 429)).toEqual({ httpStatus: 429, egress: 'unknown', target: 'unknown' });
      expect(notice({ ...request, headers: { 'X-BAPI-SIGN': 'x' } }, body, {}, 429)).toBeUndefined();
      expect(notice({ ...request, url: 'https://api.bybit.com/v5/order/history' }, body, {}, 429)).toBeUndefined();
    }
  });
  it('drops unbounded, malformed and non-string header values', () => {
    expect(
      notice(request, limited, {
        'x-tealstreet-egress': 'secret?signature=x',
        'x-tealstreet-egress-target': 'x'.repeat(65),
        'x-bapi-limit': '120-secret',
        'x-bapi-limit-status': 0,
        'retry-after': '9'.repeat(21),
      }),
    ).toEqual({ retCode: 10006, egress: 'unknown', target: 'unknown' });
  });
});

describe('Bybit host response observation', () => {
  const encode = (text: string) => new TextEncoder().encode(text);
  const headers = 'x-tealstreet-egress: proxy-01\r\nx-bapi-limit-status: 0\r\nretry-after: 10';

  it('reports a 429 even when its buffered body exceeds the native-code inspection bound', () => {
    const response = { status: 429, headers, body: encode('x'.repeat(1024 * 1024 + 1)) };
    const notices: unknown[] = [];
    expect(observe(request, response, (value) => notices.push(value))).toBe(response);
    expect(notices).toEqual([
      { httpStatus: 429, egress: 'proxy-01', target: 'unknown', remaining: '0', retryAfter: '10' },
    ]);
  });

  it('reports a 429 before a stream is consumed, preserving the response and isolating a throwing sink', async () => {
    let pulls = 0;
    const failure = new Error('body transport failed');
    const source = (async function* () {
      pulls++;
      yield encode('malformed');
      throw failure;
    })();
    const response = { status: 429, headers, body: source };
    let calls = 0;
    expect(
      observe(request, response, () => {
        calls++;
        throw new Error('sink unavailable');
      }),
    ).toBe(response);
    expect(pulls).toBe(0);
    expect(calls).toBe(1);
    const consume = async () => {
      for await (const _chunk of response.body) {
        /* consume */
      }
    };
    await expect(consume()).rejects.toBe(failure);
    expect(calls).toBe(1);
  });

  it('observes buffered bodies and isolates throwing telemetry consumers', () => {
    const response = { status: 200, headers, body: encode(limited) };
    let calls = 0;
    expect(
      observe(request, response, () => {
        calls++;
        throw new Error('sink unavailable');
      }),
    ).toBe(response);
    expect(calls).toBe(1);
    expect(
      observe({ ...request, headers: { 'X-BAPI-SIGN': 'secret' } }, response, () => {
        calls++;
      }),
    ).toBe(response);
    expect(calls).toBe(1);
  });

  it('preserves lazy chunk delivery and observes only complete bounded bodies, including split UTF-8', async () => {
    const bytes = encode('{"retCode":10006,"retMsg":"é"}');
    const split = bytes.indexOf(0xc3) + 1;
    const chunks = [bytes.subarray(0, split), bytes.subarray(split)];
    const notices: unknown[] = [];
    let pulls = 0;
    const response = observe(
      request,
      {
        status: 200,
        headers,
        body: (async function* () {
          for (const chunk of chunks) {
            pulls++;
            yield chunk;
          }
        })(),
      },
      (value) => notices.push(value),
    );
    expect(pulls).toBe(0);
    const received = [];
    for await (const chunk of response.body as AsyncIterable<Uint8Array>) received.push(chunk);
    expect(received[0]).toBe(chunks[0]);
    expect(received[1]).toBe(chunks[1]);
    expect(notices).toEqual([
      { retCode: 10006, egress: 'proxy-01', target: 'unknown', remaining: '0', retryAfter: '10' },
    ]);
  });

  it.each(['oversized', 'malformed', 'successful', 'failed', 'cancelled'] as const)(
    'does not report a %s stream',
    async (kind) => {
      const calls: unknown[] = [];
      const failure = new Error('body transport failed');
      let closed = false;
      const response = observe(
        request,
        {
          status: 200,
          headers,
          body: (async function* () {
            try {
              yield encode(kind === 'malformed' ? '{"retCode":' : kind === 'successful' ? '{"retCode":0}' : limited);
              if (kind === 'oversized') yield encode(' '.repeat(1024 * 1024));
              if (kind === 'failed') throw failure;
            } finally {
              closed = true;
            }
          })(),
        },
        (value) => calls.push(value),
      );
      const consume = async () => {
        for await (const _chunk of response.body as AsyncIterable<Uint8Array>) {
          if (kind === 'cancelled') break;
        }
      };
      if (kind === 'failed') await expect(consume()).rejects.toBe(failure);
      else await consume();
      expect(closed).toBe(true);
      expect(calls).toEqual([]);
    },
  );

  it('rejects ambiguous and unsafe host header values', () => {
    const notices: unknown[] = [];
    observe(
      request,
      {
        status: 200,
        body: encode(limited),
        headers: [
          'x-tealstreet-egress: secret?signature=x',
          'x-tealstreet-egress-target: ' + 'x'.repeat(65),
          'x-bapi-limit: 120',
          'X-Bapi-Limit: 121',
          'x-bapi-limit-status: secret',
          'retry-after: ' + '9'.repeat(21),
        ].join('\n'),
      },
      (value) => notices.push(value),
    );
    expect(notices).toEqual([{ retCode: 10006, egress: 'unknown', target: 'unknown' }]);
  });
});
