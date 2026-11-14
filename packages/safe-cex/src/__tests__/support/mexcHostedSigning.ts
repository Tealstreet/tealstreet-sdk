import type { RemoteSignerRequest } from '../../types';
import fixtures from '../../../../../sdk-rs/venues/mexc/tests/fixtures/hosted-signing.json';

export const vectors = fixtures;
export const request = (v: typeof vectors[number]): RemoteSignerRequest => ({
  id: v.label, type: 'http', exchangeName: 'mexc', config: {
    method: v.method, baseURL: 'https://api.mexc.com', url: v.path,
    ...(v.params.length ? { params: Object.fromEntries(v.params) } : {}),
    ...(v.body === null ? {} : { data: v.body }),
    headers: { 'X-Transport': 'keep', ApiKey: 'stale', Signature: 'stale', 'Request-Time': 'stale' },
  },
});
