const DEFAULT_BACKOFF_MULTIPLIER = 1.5;
const DEFAULT_BACKOFF_MAX_EXPONENT = 8;
const DEFAULT_BACKOFF_MAX_DELAY_MS = 30_000;

export const LIGHTER_POLICY_REASON_RE = /(geo|region|jurisdiction|restricted|policy|forbidden|\bip\b)/i;
export const LIGHTER_AUTH_REASON_RE = /(auth|token|signature|permission|api[-_\s]?key|credential)/i;

export type LighterPublicCloseClassification = {
  isRateLimited: boolean;
  isPolicyRestricted: boolean;
};

export type LighterPrivateCloseClassification = {
  isHandshakeFailure: boolean;
  isRateLimited: boolean;
  isPolicyRestricted: boolean;
  isAuthIssue: boolean;
  isReadonlyFallbackSignal: boolean;
  shouldBackoff: boolean;
};

export function calculateExponentialBackoffDelay(
  baseDelayMs: number,
  failureCount: number,
  opts: {
    multiplier?: number;
    maxExponent?: number;
    maxDelayMs?: number;
  } = {},
): number {
  const multiplier = opts.multiplier ?? DEFAULT_BACKOFF_MULTIPLIER;
  const maxExponent = opts.maxExponent ?? DEFAULT_BACKOFF_MAX_EXPONENT;
  const maxDelayMs = opts.maxDelayMs ?? DEFAULT_BACKOFF_MAX_DELAY_MS;
  return Math.min(baseDelayMs * Math.pow(multiplier, Math.min(failureCount, maxExponent)), maxDelayMs);
}

export function classifyLighterPublicClose(
  closeCode: number | undefined,
  closeReason: string,
): LighterPublicCloseClassification {
  const normalizedReason = String(closeReason || '').toLowerCase();
  const isRateLimited =
    closeCode === 1013 || normalizedReason.includes('too many websocket messages') || normalizedReason.includes('rate limit');
  const isPolicyRestricted = closeCode === 1008 && LIGHTER_POLICY_REASON_RE.test(closeReason);
  return {
    isRateLimited,
    isPolicyRestricted,
  };
}

export function calculateLighterPublicReconnectDelay(params: {
  baseDelayMs: number;
  failureCount: number;
  isRateLimited: boolean;
  rateLimitReconnectDelayMs: number;
}): number {
  const backoffDelayMs = calculateExponentialBackoffDelay(params.baseDelayMs, params.failureCount);
  return params.isRateLimited ? Math.max(backoffDelayMs, params.rateLimitReconnectDelayMs) : backoffDelayMs;
}

export function classifyLighterPrivateClose(params: {
  closeCode: number | undefined;
  closeReason: string;
  failedBeforeOpen: boolean;
  earlyPostOpenNoStream: boolean;
}): LighterPrivateCloseClassification {
  const normalizedReason = String(params.closeReason || '');
  const lowerReason = normalizedReason.toLowerCase();
  const isAuthIssue = params.closeCode === 1008 && LIGHTER_AUTH_REASON_RE.test(normalizedReason);
  const hasPolicyHint = LIGHTER_POLICY_REASON_RE.test(normalizedReason);
  const isPolicyRestricted = params.closeCode === 1008 && !isAuthIssue && (hasPolicyHint || normalizedReason.length === 0);
  const isRateLimited =
    params.closeCode === 1013 ||
    lowerReason.includes('too many websocket messages') ||
    lowerReason.includes('rate limit');
  const isEarlyLikeFailure = params.failedBeforeOpen || params.earlyPostOpenNoStream;
  const isReadonlyFallbackSignal = params.closeCode === 1006 || isPolicyRestricted;
  const isHandshakeFailure =
    isEarlyLikeFailure &&
    (params.closeCode === 1006 ||
      params.closeCode === 1013 ||
      params.closeCode === 1015 ||
      isPolicyRestricted ||
      isRateLimited);

  return {
    isHandshakeFailure,
    isRateLimited,
    isPolicyRestricted,
    isAuthIssue,
    isReadonlyFallbackSignal,
    shouldBackoff: isHandshakeFailure || isRateLimited,
  };
}

export function calculateLighterPrivateReconnectDelay(params: {
  baseDelayMs: number;
  failedHandshakes: number;
  shouldBackoff: boolean;
  isRateLimited: boolean;
  isWsRateLimited: boolean;
  rateLimitReconnectDelayMs: number;
}): number {
  const backoffDelayMs = calculateExponentialBackoffDelay(params.baseDelayMs, params.failedHandshakes);
  const delayBeforeRateLimitMs = params.shouldBackoff ? backoffDelayMs : params.baseDelayMs;
  return params.isRateLimited || params.isWsRateLimited
    ? Math.max(delayBeforeRateLimitMs, params.rateLimitReconnectDelayMs)
    : delayBeforeRateLimitMs;
}
