let wsDiagnosticsEnabled = false;
let wsDiagnosticsGeneration = 0;

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
  'sig',
  'apisecret',
  'api_secret',
  'secretkey',
  'secret_key',
  'privatekey',
  'private_key',
  'accesskey',
  'access_key',
  'authorization',
  'proxytoken',
  'proxy_token',
]);

/** Any key naming one of these is a secret, so a venue's new spelling (`jwt_token`, `refreshToken`) is covered. */
const SECRET_KEY_PARTS = [
  'secret',
  'token',
  'jwt',
  'signature',
  'passphrase',
  'password',
  'apikey',
  'privatekey',
  'accesskey',
  'listenkey',
  'authkey',
  'authorization',
  'credential',
  'cookie',
  'challenge',
  'bearer',
];

const isSecretKey = (key: string): boolean => {
  const normalized = key.replace(/[^a-z0-9_]/gi, '').toLowerCase();
  if (SECRET_KEYS.has(normalized) || normalized.startsWith('auth')) return true;
  const compact = normalized.replace(/_/g, '');
  return (
    compact.endsWith('sign') || compact.endsWith('auth') || SECRET_KEY_PARTS.some((part) => compact.includes(part))
  );
};

const JSON_KEY_VALUE_RE = /("((?:\\.|[^"\\])*)"\s*:\s*)("(?:\\.|[^"])*"|[^,}\]\s]+)/g;
const QUERY_KEY_VALUE_RE = /((?:^|[?&\s])([A-Za-z0-9_.-]+)=)([^&\s]+)/g;
/** A JWT, whatever key or position carries it. */
const JWT_RE = /eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+/g;
const AUTH_OPERATION_RE = /auth|login|logon|signature|jwt|challenge/i;

export const setWsDiagnosticsEnabled = (enabled: boolean): void => {
  const next = enabled === true;
  if (wsDiagnosticsEnabled && !next) wsDiagnosticsGeneration += 1;
  wsDiagnosticsEnabled = next;
};

export const isWsDiagnosticsEnabled = (): boolean => wsDiagnosticsEnabled;

/** Internal cancellation generation; re-enabling cannot revive queued diagnostics. */
export const getWsDiagnosticsGeneration = (): number => wsDiagnosticsGeneration;

/** Literal or JSON ping/pong frames, which the Requests Log leaves out. */
export function isHeartbeatFrame(message: string): boolean {
  const trimmed = message.trim();
  if (/^(ping|pong)$/i.test(trimmed)) return true;

  try {
    const payload = JSON.parse(trimmed) as Record<string, unknown>;
    if (!payload || Array.isArray(payload) || typeof payload !== 'object') return false;

    const heartbeatOperation = [payload.op, payload.type, payload.event, payload.method].some(
      (value) => typeof value === 'string' && /^(?:server\.)?(?:ping|pong)$/i.test(value),
    );
    return heartbeatOperation || Object.hasOwn(payload, 'ping') || Object.hasOwn(payload, 'pong');
  } catch {
    return false;
  }
}

const WS_DIAGNOSTIC_RATE_WINDOW_MS = 1000;
const WS_DIAGNOSTIC_MAX_FRAMES_PER_WINDOW = 200;
const WS_DIAGNOSTIC_MAX_PAYLOAD_CHARS = 4096;

/** The Requests Log's per-socket sampling: a frame-rate cap with a suppressed-count notice, and a payload cap. */
export class WsDiagnosticSampler {
  private windowStartedAt = 0;
  private frames = 0;
  private suppressed = 0;

  /** Whether this frame is over the window's cap (and so counted as suppressed). */
  drop(): boolean {
    const now = Date.now();
    if (now - this.windowStartedAt >= WS_DIAGNOSTIC_RATE_WINDOW_MS) {
      this.windowStartedAt = now;
      this.frames = 0;
    }
    this.frames += 1;
    if (this.frames <= WS_DIAGNOSTIC_MAX_FRAMES_PER_WINDOW) return false;
    this.suppressed += 1;
    return true;
  }

  /** Emit `message` (capped), preceded by a notice for frames dropped since the last one. */
  emit(emit: (message: string) => void, label: string, message: string): void {
    if (this.suppressed > 0) {
      emit(`[${label}] suppressed ${this.suppressed} websocket diagnostic frame(s) due to rate limit`);
      this.suppressed = 0;
    }
    emit(
      message.length <= WS_DIAGNOSTIC_MAX_PAYLOAD_CHARS
        ? message
        : `${message.slice(0, WS_DIAGNOSTIC_MAX_PAYLOAD_CHARS)}...truncated (${message.length} chars)`,
    );
  }
}

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

const isAuthOperationObject = (value: object): boolean =>
  Object.keys(value).some((key) => normalizeSecretKey(key).includes('challenge')) ||
  Object.entries(value).some(([key, field]) =>
    ['op', 'method', 'type', 'event', 'action'].includes(key)
      && typeof field === 'string' && AUTH_OPERATION_RE.test(field));

// Field names and values are independently allowlisted. Even an id or topic
// can contain a venue's authentication echo; no free-form string is safe.
const DIAGNOSTIC_OPERATIONS = new Set([
  'auth', 'login', 'challenge', 'error', 'echo', 'subscribe', 'subscribed', 'unsubscribe', 'unsubscribed',
  'SUBSCRIBE', 'UNSUBSCRIBE', 'snapshot', 'delta', 'update',
]);
const DIAGNOSTIC_ENUMS: Readonly<Record<string, ReadonlySet<string> | undefined>> = {
  op: DIAGNOSTIC_OPERATIONS, method: DIAGNOSTIC_OPERATIONS, type: DIAGNOSTIC_OPERATIONS,
  event: DIAGNOSTIC_OPERATIONS, action: DIAGNOSTIC_OPERATIONS,
  e: new Set(['ORDER_TRADE_UPDATE', 'ACCOUNT_UPDATE', 'executionReport', 'outboundAccountPosition']),
  channel: new Set(['public', 'private', 'business']),
  code: new Set(['AUTH_FAILED', 'INVALID_SIGNATURE', 'INVALID_API_KEY', 'RATE_LIMITED', 'UNAUTHORIZED',
    'FORBIDDEN', 'INVALID_REQUEST', 'INTERNAL_ERROR']),
  side: new Set(['Buy', 'Sell', 'buy', 'sell', 'BUY', 'SELL']),
  orderType: new Set(['Market', 'Limit', 'market', 'limit', 'MARKET', 'LIMIT']),
  orderStatus: new Set(['New', 'Filled', 'Canceled', 'Cancelled', 'PartiallyFilled', 'Rejected', 'Untriggered', 'Triggered']),
};
const DIAGNOSTIC_NUMBERS = new Set([
  'code', 'count', 'length', 'i', 'id', 'price', 'qty', 'size', 'amount', 'lastPrice', 'bidPrice', 'askPrice',
  'timestamp', 'ts',
]);
const DIAGNOSTIC_FIELDS = new Set([
  'op', 'method', 'type', 'event', 'action', 'e', 'topic', 'channel', 'feed',
  'args', 'params', 'data', 'result', 'o', 'i', 'id', 'code', 'success', 'count', 'length',
  'symbol', 'orderId', 'orderLinkId', 'clientOrderId', 'orderStatus', 'side', 'orderType',
  'price', 'qty', 'size', 'amount', 'lastPrice', 'bidPrice', 'askPrice', 'timestamp', 'ts',
]);

const diagnosticJsonValue = (value: unknown, depth = 0, field = ''): unknown => {
  if (depth > MAX_JSON_REDACTION_DEPTH) return REDACTED;
  if (typeof value === 'string') return DIAGNOSTIC_ENUMS[field]?.has(value) ? value : REDACTED;
  if (typeof value === 'number') return DIAGNOSTIC_NUMBERS.has(field)
    && Number.isSafeInteger(value) && Math.abs(value) <= 2_147_483_647
    && (!(field === 'count' || field === 'length') || value >= 0) ? value : REDACTED;
  if (typeof value === 'boolean') return field === 'success' ? value : REDACTED;
  if (Array.isArray(value)) {
    const entries = value.slice(0, 32).map((child) => diagnosticJsonValue(child, depth + 1));
    if (value.length > 32) entries.push(REDACTED);
    return entries;
  }
  if (!value || typeof value !== 'object') return REDACTED;
  const result: Record<string, unknown> = {};
  for (const [key, child] of Object.entries(value)) {
    if (DIAGNOSTIC_FIELDS.has(key)) result[key] = diagnosticJsonValue(child, depth + 1, key);
    else result.redacted = REDACTED;
  }
  return result;
};

const redactJsonValue = (value: unknown, depth = 0, redactStrings = false): unknown => {
  if (depth > MAX_JSON_REDACTION_DEPTH) return REDACTED;
  if (typeof value === 'string') {
    return redactStrings ? REDACTED : value;
  }
  if (Array.isArray(value)) return value.map((item) => redactJsonValue(item, depth + 1, redactStrings));
  if (!value || typeof value !== 'object') return value;

  const result: Record<string, unknown> = {};
  const authScope = redactStrings || isAuthOperationObject(value);
  for (const [key, child] of Object.entries(value)) {
    const normalizedKey = normalizeSecretKey(key);
    result[key] = isSecretKey(key)
      ? REDACTED
      : redactJsonValue(
          child,
          depth + 1,
          authScope && (normalizedKey === 'args' || normalizedKey === 'params'),
        );
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

export const redactWsDiagnosticPayload = (message: string): string => {
  const structured = redactStructuredJson(message);
  const source = structured ?? message;

  return source
    .replace(JSON_KEY_VALUE_RE, (match, prefix, key) => (isSecretKey(key) ? `${prefix}"${REDACTED}"` : match))
    .replace(QUERY_KEY_VALUE_RE, (match, prefix, key) => (isSecretKey(key) ? `${prefix}${REDACTED}` : match))
    .replace(JWT_RE, REDACTED)
    .replace(HEX_SECRET_RE, REDACTED);
};

/** Opaque or incomplete frames cannot be safely redacted; never fall back to their wire text. */
export const sanitizeWsDiagnosticFrame = (message: string): string => {
  if (message.length > WS_DIAGNOSTIC_MAX_PAYLOAD_CHARS * 4) return '[redacted oversized frame]';
  try {
    const parsed: unknown = JSON.parse(message);
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return '[redacted opaque frame]';
    return redactWsDiagnosticPayload(JSON.stringify(diagnosticJsonValue(parsed)));
  } catch {
    return '[redacted malformed frame]';
  }
};
