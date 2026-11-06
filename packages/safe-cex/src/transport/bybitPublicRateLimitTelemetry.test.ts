import { describe, expect, it } from 'vitest';

import {
  bybitPublicRateLimitNotice as notice,
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
    expect(notice(request, limited, {})).toEqual({ egress: 'unknown', target: 'unknown' });
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
  it('refuses private, signed, ambiguous and non-10006 reports', () => {
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
  it('drops unbounded, malformed and non-string header values', () => {
    expect(
      notice(request, limited, {
        'x-tealstreet-egress': 'secret?signature=x',
        'x-tealstreet-egress-target': 'x'.repeat(65),
        'x-bapi-limit': '120-secret',
        'x-bapi-limit-status': 0,
        'retry-after': '9'.repeat(21),
      }),
    ).toEqual({ egress: 'unknown', target: 'unknown' });
  });
});
