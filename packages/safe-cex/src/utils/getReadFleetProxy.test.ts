import { afterEach, describe, expect, it } from 'vitest';

import { getReadFleetProxyUrl } from './getReadFleetProxy';
import { setAwsProxyMigrationEnabled } from './transportResolution';

const READ_FLEET_JP = 'https://read-fleet-jp.tealstreet.io/api/exchange_proxy';
const READ_FLEET_SG = 'https://read-fleet-sg.tealstreet.io/api/exchange_proxy';
const LEGACY_AWS = 'https://exchange-proxy.jp.tealstreet.io/api/exchange_proxy';

describe('getReadFleetProxyUrl', () => {
  // setAwsProxyMigrationEnabled flips a module global; always reset so state can't
  // leak between tests in this file.
  afterEach(() => setAwsProxyMigrationEnabled(false));

  describe('ENABLE_AWS_PROXY_MIGRATION on -> read fleet', () => {
    it('routes JA and unset region to the Tokyo read-fleet pool', () => {
      setAwsProxyMigrationEnabled(true);
      expect(getReadFleetProxyUrl('bybit')).toBe(READ_FLEET_JP);
      expect(getReadFleetProxyUrl('binance', 'JA')).toBe(READ_FLEET_JP);
    });

    it('routes SG region to the Singapore read-fleet pool', () => {
      setAwsProxyMigrationEnabled(true);
      expect(getReadFleetProxyUrl('bybit', 'SG')).toBe(READ_FLEET_SG);
    });
  });

  describe('ENABLE_AWS_PROXY_MIGRATION off (default) -> legacy AWS', () => {
    it('routes every region to the legacy AWS public CORS proxy', () => {
      expect(getReadFleetProxyUrl('bybit')).toBe(LEGACY_AWS);
      expect(getReadFleetProxyUrl('binance', 'JA')).toBe(LEGACY_AWS);
      expect(getReadFleetProxyUrl('bybit', 'SG')).toBe(LEGACY_AWS);
    });
  });
});
