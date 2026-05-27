export function buildLighterCandleTopic(resolvedInterval: string, symbol: string): string {
  return `candle_${resolvedInterval}_${symbol}`;
}

export function buildLighterTradesTopic(symbol: string): string {
  return `trades_${symbol}`;
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
