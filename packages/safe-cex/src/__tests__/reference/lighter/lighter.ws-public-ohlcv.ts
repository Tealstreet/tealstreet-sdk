export function buildLighterCandleTopic(marketId: string | number, resolution: string): string {
  return `candle/${marketId}/${resolution}`;
}

export function buildLighterTradesTopic(marketId: string | number): string {
  return `trade/${marketId}`;
}

export function shouldAttemptLighterOhlcvRecovery(params: {
  hasTopicHandlers: boolean;
  nowMs: number;
  lastRecoveryReconnectAtMs: number;
  recoveryCooldownMs: number;
}): boolean {
  if (!params.hasTopicHandlers) {
    return false;
  }

  return params.nowMs - params.lastRecoveryReconnectAtMs >= params.recoveryCooldownMs;
}

export function shouldPromoteLighterOhlcvRecoveryToProxy(params: {
  isWsProxyRouteEnabled: boolean;
  useProxyRoute: boolean;
  connectedViaProxyRoute: boolean;
}): boolean {
  return params.isWsProxyRouteEnabled && !params.useProxyRoute && !params.connectedViaProxyRoute;
}
