import type { RemoteSignerRequest } from '../../types';

import wooHostedVectors from '../../../../../sdk-rs/venues/woo/tests/fixtures/hosted-signing.json';

export type WooHostedVector = { label: string; method: string; path: string; params: [string, string][]; body: string | null; payload: string; signature: string };
export const vectors = wooHostedVectors as WooHostedVector[];
export const request = (v: WooHostedVector): RemoteSignerRequest => ({ id: v.label, type: 'http', exchangeName: 'woo', config: {
  method: v.method, baseURL: 'https://api.woox.io', url: v.path,
  ...(v.path.includes('/v1/') && v.method !== 'GET' ? { data: Object.fromEntries(v.params) }
    : { ...(v.params.length ? { params: Object.fromEntries(v.params) } : {}), ...(v.body ? { data: JSON.parse(v.body) } : {}) }),
  headers: { 'X-Transport': 'keep', 'x-api-key': 'stale', 'x-api-signature': 'stale', 'x-api-timestamp': 'stale' },
} });
