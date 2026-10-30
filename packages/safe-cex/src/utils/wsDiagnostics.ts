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

const JSON_SECRET_KEY_RE =
  /("(?:apiKey|api_key|key|secret|passphrase|sign|signature|wsAuthSignature|jwt|token|sessionToken|session|authKey|listenKey|password|sig|apiSecret|api_secret|secretKey|secret_key|privateKey|private_key|accessKey|access_key|authorization|proxyToken|proxy_token)"\s*:\s*)("(?:\\.|[^"])*"|[^,}\]\s]+)/gi;
const QUERY_SECRET_KEY_RE =
  /((?:^|[?&\s])(?:apiKey|api_key|key|secret|passphrase|sign|signature|wsAuthSignature|jwt|token|sessionToken|session|authKey|listenKey|password|sig|apiSecret|api_secret|secretKey|secret_key|privateKey|private_key|accessKey|access_key|authorization|proxyToken|proxy_token)=)([^&\s]+)/gi;
const AUTH_OPERATION_RE = /auth|login|signature|jwt/i;

export const setWsDiagnosticsEnabled = (enabled: boolean): void => {
  wsDiagnosticsEnabled = enabled === true;
};

export const isWsDiagnosticsEnabled = (): boolean => wsDiagnosticsEnabled;

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

