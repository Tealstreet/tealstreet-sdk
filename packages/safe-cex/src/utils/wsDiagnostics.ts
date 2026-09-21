let wsDiagnosticsEnabled = false;

const REDACTED = '[redacted]';
const HEX_SECRET_RE = /\b[0-9a-f]{40,}\b/gi;
const MAX_JSON_REDACTION_DEPTH = 12;

const SECRET_KEYS = new Set([
  'apikey',
  'api_key',
  'key',
  'secret',
  'passphrase',
  'sign',
  'signature',
  'wsauthsignature',
  'jwt',
  'token',
  'sessiontoken',
  'session',
  'authkey',
  'listenkey',
  'password',
]);

const JSON_SECRET_KEY_RE =
  /("(?:apiKey|api_key|key|secret|passphrase|sign|signature|wsAuthSignature|jwt|token|sessionToken|session|authKey|listenKey|password)"\s*:\s*)("(?:\\.|[^"])*"|[^,}\]\s]+)/gi;
const QUERY_SECRET_KEY_RE =
  /((?:^|[?&\s])(?:apiKey|api_key|key|secret|passphrase|sign|signature|wsAuthSignature|jwt|token|sessionToken|session|authKey|listenKey|password)=)([^&\s]+)/gi;
const AUTH_OPERATION_RE = /auth|login|signature|jwt/i;

export const setWsDiagnosticsEnabled = (enabled: boolean): void => {
  wsDiagnosticsEnabled = enabled === true;
};

export const isWsDiagnosticsEnabled = (): boolean => wsDiagnosticsEnabled;

export const stringifyWsDiagnosticPayload = (data: unknown): string => {
  if (typeof data === 'string') return data;

  try {
    const serialized = JSON.stringify(data);
    return serialized ?? String(data);
  } catch {
    return String(data);
  }
};

const normalizeSecretKey = (key: string): string => key.replace(/[^a-z0-9_]/gi, '').toLowerCase();

const isAuthOperationObject = (value: Record<string, unknown>): boolean =>
  ['op', 'method', 'type', 'event'].some((key) => {
    const field = value[key];
    return typeof field === 'string' && AUTH_OPERATION_RE.test(field);
  });

const redactJsonValue = (value: unknown, depth = 0, redactStrings = false): unknown => {
  if (depth > MAX_JSON_REDACTION_DEPTH) return value;
  if (typeof value === 'string') return redactStrings ? REDACTED : value;
  if (Array.isArray(value)) return value.map((item) => redactJsonValue(item, depth + 1, redactStrings));
  if (!value || typeof value !== 'object') return value;

  const result: Record<string, unknown> = {};
  const authScope = redactStrings || isAuthOperationObject(value as Record<string, unknown>);
  for (const [key, child] of Object.entries(value)) {
    const normalizedKey = normalizeSecretKey(key);
    result[key] = SECRET_KEYS.has(normalizedKey)
      ? REDACTED
      : redactJsonValue(child, depth + 1, authScope && (normalizedKey === 'args' || normalizedKey === 'params'));
  }
  return result;
};

const redactStructuredJson = (message: string): string | undefined => {
  const trimmed = message.trim();
  if (!trimmed || (trimmed[0] !== '{' && trimmed[0] !== '[')) return undefined;

  try {
    return JSON.stringify(redactJsonValue(JSON.parse(trimmed)));
  } catch {
    return undefined;
  }
};

const shortenHexSecret = (value: string): string => `${value.slice(0, 4)}...${value.slice(-4)}`;

export const redactWsDiagnosticPayload = (message: string): string => {
  const structured = redactStructuredJson(message);
  const source = structured ?? message;

  return source
    .replace(JSON_SECRET_KEY_RE, (_match, prefix) => `${prefix}"${REDACTED}"`)
    .replace(QUERY_SECRET_KEY_RE, (_match, prefix) => `${prefix}${REDACTED}`)
    .replace(HEX_SECRET_RE, shortenHexSecret);
};

