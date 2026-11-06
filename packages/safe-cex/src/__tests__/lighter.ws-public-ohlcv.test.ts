import { describe, expect, it } from 'vitest';

import {
  buildLighterCandleTopic,
  buildLighterTradesTopic,
  shouldAttemptLighterOhlcvRecovery,
  shouldPromoteLighterOhlcvRecoveryToProxy,
} from './reference/lighter/lighter.ws-public-ohlcv';

describe('lighter.ws-public-ohlcv helpers', () => {
  it('builds expected candle and trades topics', () => {
    expect(buildLighterCandleTopic('BTC', '1m')).toBe('candle/BTC/1m');
    expect(buildLighterTradesTopic('BTC')).toBe('trade/BTC');
  });

  it('allows recovery when handlers exist and cooldown elapsed', () => {
    expect(
      shouldAttemptLighterOhlcvRecovery({
        hasTopicHandlers: true,
        nowMs: 50_000,
        lastRecoveryReconnectAtMs: 0,
        recoveryCooldownMs: 45_000,
      }),
    ).toBe(true);
  });

  it('blocks recovery when no handlers or cooldown not elapsed', () => {
    expect(
      shouldAttemptLighterOhlcvRecovery({
        hasTopicHandlers: false,
        nowMs: 50_000,
        lastRecoveryReconnectAtMs: 0,
        recoveryCooldownMs: 45_000,
      }),
    ).toBe(false);

    expect(
      shouldAttemptLighterOhlcvRecovery({
        hasTopicHandlers: true,
        nowMs: 20_000,
        lastRecoveryReconnectAtMs: 0,
        recoveryCooldownMs: 45_000,
      }),
    ).toBe(false);
  });

  it('promotes to proxy only when proxy route is available and not active', () => {
    expect(
      shouldPromoteLighterOhlcvRecoveryToProxy({
        isWsProxyRouteEnabled: true,
        useProxyRoute: false,
        connectedViaProxyRoute: false,
      }),
    ).toBe(true);

    expect(
      shouldPromoteLighterOhlcvRecoveryToProxy({
        isWsProxyRouteEnabled: false,
        useProxyRoute: false,
        connectedViaProxyRoute: false,
      }),
    ).toBe(false);

    expect(
      shouldPromoteLighterOhlcvRecoveryToProxy({
        isWsProxyRouteEnabled: true,
        useProxyRoute: true,
        connectedViaProxyRoute: false,
      }),
    ).toBe(false);
  });
});
