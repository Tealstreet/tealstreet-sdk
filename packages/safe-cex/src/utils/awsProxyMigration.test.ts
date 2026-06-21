import { afterEach, describe, expect, it } from 'vitest';

import { getPublicCorsProxy, isAwsProxyMigrationEnabled, setAwsProxyMigrationEnabled } from './transportResolution';

const READ_FLEET_JP = 'https://read-fleet-jp.tealstreet.io/api/exchange_proxy';
const LEGACY_AWS = 'https://exchange-proxy.jp.tealstreet.io/api/exchange_proxy';

describe('ENABLE_AWS_PROXY_MIGRATION gate (getPublicCorsProxy)', () => {
  afterEach(() => setAwsProxyMigrationEnabled(false));

  it('defaults OFF -> legacy AWS public CORS proxy (zero-impact pre-migration baseline)', () => {
    expect(isAwsProxyMigrationEnabled()).toBe(false);
    expect(getPublicCorsProxy()).toBe(LEGACY_AWS);
  });

  it('ON -> Vultr read-fleet (jp)', () => {
    setAwsProxyMigrationEnabled(true);
    expect(isAwsProxyMigrationEnabled()).toBe(true);
    expect(getPublicCorsProxy()).toBe(READ_FLEET_JP);
  });

  it('is resolved at call time, so a flip is reflected immediately', () => {
    expect(getPublicCorsProxy()).toBe(LEGACY_AWS);
    setAwsProxyMigrationEnabled(true);
    expect(getPublicCorsProxy()).toBe(READ_FLEET_JP);
    setAwsProxyMigrationEnabled(false);
    expect(getPublicCorsProxy()).toBe(LEGACY_AWS);
  });
});
