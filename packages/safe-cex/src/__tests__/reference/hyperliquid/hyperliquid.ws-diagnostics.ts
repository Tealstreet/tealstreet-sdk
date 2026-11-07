type HyperliquidWsDiagnosticScope = 'public' | 'private';

const HYPERLIQUID_ADDRESS_RE = /\b0x[a-fA-F0-9]{40}\b/g;
const NOISY_SEND_SUBSCRIPTION_TYPES = new Set(['activeAssetData']);
const PUBLIC_RECEIVE_DIAGNOSTIC_CHANNELS = new Set([
  'error',
  'subscribed',
  'subscriptionResponse',
  'SubscriptionResponse',
]);
const PRIVATE_RECEIVE_DIAGNOSTIC_CHANNELS = new Set([
  'error',
  'notification',
  'orderUpdates',
  'subscribed',
  'subscriptionResponse',
  'SubscriptionResponse',
  'userEvents',
  'userFills',
  'userFundings',
  'userNonFundingLedgerUpdates',
]);

const parseJsonObject = (data: unknown): Record<string, any> | undefined => {
  if (data && typeof data === 'object' && !Array.isArray(data)) return data as Record<string, any>;
  if (typeof data !== 'string') return undefined;

  try {
    const parsed = JSON.parse(data);
    return parsed && typeof parsed === 'object' && !Array.isArray(parsed) ? parsed : undefined;
  } catch {
    return undefined;
  }
};

const stringifyDiagnosticPayload = (data: unknown): string => {
  if (typeof data === 'string') return data;
  try {
    const serialized = JSON.stringify(data);
    return serialized ?? String(data);
  } catch {
    return String(data);
  }
};

export const formatHyperliquidWsDiagnostic = (scope: HyperliquidWsDiagnosticScope, data: unknown): string =>
  `[hyperliquid ${scope}] ${stringifyDiagnosticPayload(data).replace(HYPERLIQUID_ADDRESS_RE, '[redacted]')}`;

export const shouldEmitHyperliquidWsSendDiagnostic = (data: unknown): boolean => {
  const parsed = parseJsonObject(data);
  if (parsed?.method === 'ping') return false;

  const subscriptionType = parsed?.subscription?.type;
  if (typeof subscriptionType === 'string' && NOISY_SEND_SUBSCRIPTION_TYPES.has(subscriptionType)) return false;

  return true;
};

export const shouldEmitHyperliquidWsReceiveDiagnostic = (
  scope: HyperliquidWsDiagnosticScope,
  data: unknown,
): boolean => {
  const channel = parseJsonObject(data)?.channel;
  if (typeof channel !== 'string') return false;

  return scope === 'private'
    ? PRIVATE_RECEIVE_DIAGNOSTIC_CHANNELS.has(channel)
    : PUBLIC_RECEIVE_DIAGNOSTIC_CHANNELS.has(channel);
};
