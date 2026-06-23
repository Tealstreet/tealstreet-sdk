import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { getReadFleetProxyUrl } from './getReadFleetProxy';
import {
  READ_FLEET_JP as JP,
  LEGACY_AWS_PUBLIC_CORS as LEGACY_AWS,
  setAwsProxyMigrationEnabled,
  READ_FLEET_SG as SG,
} from './transportResolution';

// Host literals come from the canonical constants (no drift); awsProxyMigration.test.ts
// guards their actual values. This file asserts the per-exchange REGION logic.

// The read fleet egresses from the SAME region the primary REST pool uses for the
// exchange (restEgressRegion): binance/okx/bitget declare the jp (Tokyo) pool, grvt the
// 'tiny' (Tokyo) pool, every other exchange the 'asia' (Singapore) pool; proxyRegion
// 'JA' forces Tokyo, otherwise the exchange's declared pool wins. This is true parity
// with getPrivateProxy — NOT a coarse "SG -> Singapore, else Tokyo" split.
describe('getReadFleetProxyUrl', () => {
  afterEach(() => setAwsProxyMigrationEnabled(false));

  describe('migration on — egress region matches the exchange primary REST pool', () => {
    beforeEach(() => setAwsProxyMigrationEnabled(true));

    // bybit declares the 'asia' (Singapore) pool.
    it('asia-pool exchange with no region -> Singapore (its declared pool), not Tokyo', () => {
      expect(getReadFleetProxyUrl('bybit')).toBe(SG);
    });
    it('asia-pool exchange, SG -> Singapore', () => {
      expect(getReadFleetProxyUrl('bybit', 'SG')).toBe(SG);
    });
    it('asia-pool exchange, JA -> Tokyo (JA forces jp)', () => {
      expect(getReadFleetProxyUrl('bybit', 'JA')).toBe(JP);
    });

    // binance/okx/bitget declare the 'jp' (Tokyo) pool.
    it('jp-pool exchange with no region -> Tokyo', () => {
      expect(getReadFleetProxyUrl('binance')).toBe(JP);
      expect(getReadFleetProxyUrl('okx')).toBe(JP);
      expect(getReadFleetProxyUrl('bitget')).toBe(JP);
    });
    it('jp-pool exchange, JA -> Tokyo', () => {
      expect(getReadFleetProxyUrl('binance', 'JA')).toBe(JP);
    });
    // The fix: SG must NOT pull a jp-pool exchange to Singapore — AWS keeps it on its
    // declared jp pool (only JA overrides), so the read fleet must too.
    it('jp-pool exchange, SG stays Tokyo (declared pool wins; only JA overrides)', () => {
      expect(getReadFleetProxyUrl('binance', 'SG')).toBe(JP);
    });

    // grvt uses the 'tiny' pool, which egresses Tokyo regardless of region.
    it('grvt (tiny pool) -> Tokyo for any region', () => {
      expect(getReadFleetProxyUrl('grvt')).toBe(JP);
      expect(getReadFleetProxyUrl('grvt', 'SG')).toBe(JP);
      expect(getReadFleetProxyUrl('grvt', 'JA')).toBe(JP);
    });
  });

  describe('migration off (default) -> legacy AWS public CORS', () => {
    it('routes every exchange/region to the legacy AWS public CORS proxy', () => {
      expect(getReadFleetProxyUrl('bybit')).toBe(LEGACY_AWS);
      expect(getReadFleetProxyUrl('bybit', 'SG')).toBe(LEGACY_AWS);
      expect(getReadFleetProxyUrl('binance', 'SG')).toBe(LEGACY_AWS);
      expect(getReadFleetProxyUrl('grvt')).toBe(LEGACY_AWS);
    });
  });
});
