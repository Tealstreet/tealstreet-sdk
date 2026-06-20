import { describe, expect, it } from 'vitest';

import { getReadFleetProxyUrl } from './getReadFleetProxy';

describe('getReadFleetProxyUrl', () => {
  it('returns undefined until fleet endpoints are wired (default-off)', () => {
    expect(getReadFleetProxyUrl('bybit')).toBeUndefined();
    expect(getReadFleetProxyUrl('binance', 'JA')).toBeUndefined();
  });
});
