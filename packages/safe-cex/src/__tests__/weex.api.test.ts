import { describe, expect, it } from 'vitest';

import { serializeWeexParams, signWeexMessage } from './reference/weex/weex.api';

describe('WEEX API helpers', () => {
  it('serializes query parameters in the same order used for signing', () => {
    expect(serializeWeexParams({ symbol: 'BTCUSDT', limit: 20, empty: undefined })).toBe('limit=20&symbol=BTCUSDT');
  });

  it('signs REST messages with HMAC-SHA256 and base64 encoding', () => {
    const message = '1591089508404GET/capi/v3/market/depth?limit=20&symbol=BTCUSDT';
    expect(signWeexMessage('test-secret', message)).toBe('8xqIh3nZ4Omo5R78wCTl/lN1L3l8/DzO1xBJO8Bgyc4=');
  });

  it('signs private websocket headers with timestamp plus request path', () => {
    const message = '1693208170000/v3/ws/private';
    expect(signWeexMessage('test-secret', message)).toBe('v7T5E72ymGfF5eldzI5/vs0odgid0Cj19oeQdQEb10Q=');
  });
});
