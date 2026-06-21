import { describe, expect, it } from 'vitest';

import { getReadFleetProxyUrl } from './getReadFleetProxy';

describe('getReadFleetProxyUrl', () => {
  it('routes JA and unset region to the Tokyo read-fleet pool', () => {
    expect(getReadFleetProxyUrl('bybit')).toBe('https://read-fleet-jp.tealstreet.io/api/exchange_proxy');
    expect(getReadFleetProxyUrl('binance', 'JA')).toBe('https://read-fleet-jp.tealstreet.io/api/exchange_proxy');
  });

  it('routes SG region to the Singapore read-fleet pool', () => {
    expect(getReadFleetProxyUrl('bybit', 'SG')).toBe('https://read-fleet-sg.tealstreet.io/api/exchange_proxy');
  });
});
