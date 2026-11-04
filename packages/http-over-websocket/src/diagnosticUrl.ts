/** Diagnostic context only: the original signed URL must remain unchanged on the wire. */
export const diagnosticUrl = (url: string): string => {
  if (typeof url !== 'string') return '[invalid URL]';
  try {
    const parsed = new URL(url);
    return `${parsed.origin}${parsed.pathname}`;
  } catch {
    return url.startsWith('/') && !url.startsWith('//') ? url.split(/[?#]/, 1)[0] : '[invalid URL]';
  }
};

const stripLiteralUrlQueries = (message: string): string =>
  message.replace(/(?:https?|wss?):\/\/[^\s<>"'`]+|\/[^\s<>"'`?#]*\?[^\s<>"'`]+/gi, (url) => {
    const suffix = url.match(/[)\]},;.!]+$/)?.[0] ?? '';
    return diagnosticUrl(url.slice(0, url.length - suffix.length)) + suffix;
  });

/** Covers relative, escaped and encoded upstream diagnostics as well as literal URLs. */
export const stripDiagnosticUrlQueries = (message: string): string =>
  stripLiteralUrlQueries(redactDiagnosticSecrets(stripLiteralUrlQueries(message)));

const REDACTED = '[REDACTED]';
const MAX_DECODE_DEPTH = 4;

const decodeDiagnosticLayer = (text: string): string =>
  text
    .replace(/&(?:amp|#0*38|#x0*26);/gi, '&')
    .replace(
      /\\(?:u([0-9a-f]{4})|x([0-9a-f]{2})|(["'\\/]))/gi,
      (_, unicode, hex, escaped) => escaped ?? String.fromCharCode(parseInt(unicode ?? hex, 16)),
    )
    .replace(/(?:%[0-9a-f]{2})+/gi, (encoded) => {
      try {
        return decodeURIComponent(encoded);
      } catch {
        return encoded.replace(/%([0-9a-f]{2})/gi, (_, hex) => String.fromCharCode(parseInt(hex, 16)));
      }
    });

export const isDiagnosticSecretKey = (key: string, includeAuthTiming = false): boolean => {
  let decoded = key;
  for (let depth = 0; depth < MAX_DECODE_DEPTH; depth++) {
    const next = decodeDiagnosticLayer(decoded);
    if (next === decoded) break;
    decoded = next;
  }
  if (decodeDiagnosticLayer(decoded) !== decoded || /[%\\]/.test(decoded)) return true;
  const normalized = decoded.replace(/[^a-z0-9]/gi, '').toLowerCase();
  return (
    /^(?:key|sign|sig|auth|session|.*(?:api|access)sign)$/.test(normalized) ||
    /apikey|secret|password|passphrase|privatekey|accesskey|token|signature|authorization|listenkey|cookie/.test(
      normalized,
    ) ||
    (includeAuthTiming &&
      /^(?:timestamp|recvwindow|nonce|expires|expiry|expiration|validuntil|requesttime|reqtime)$|timestamp$/.test(
        normalized,
      ))
  );
};

const redactAssignments = (message: string): string => {
  // Serialized header tuples and compound auth values have ambiguous value boundaries.
  for (const header of message.matchAll(/\[\s*["']([^"']+)["']\s*,/g)) {
    if (isDiagnosticSecretKey(header[1], true)) return REDACTED;
  }
  const keys = /(?:^|[^A-Za-z0-9_.-])["']?([A-Za-z0-9_.-]+)["']?\s*[:=]\s*/g;
  let output = '';
  let cursor = 0;
  let match: RegExpExecArray | null;
  while ((match = keys.exec(message))) {
    if (!isDiagnosticSecretKey(match[1], true)) continue;
    const start = keys.lastIndex;
    const tail = message.slice(start);
    if (/^[{\[]/.test(tail) && !tail.startsWith(REDACTED)) return REDACTED;
    const value = message.slice(start).match(/^("(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|[^\s,"'}&]+)/)?.[0];
    if (!value) continue;
    const quote = value[0] === '"' || value[0] === "'" ? value[0] : '';
    if (!quote && !value.startsWith(REDACTED)) return REDACTED;
    output += message.slice(cursor, start) + quote + REDACTED + quote;
    cursor = keys.lastIndex = start + value.length;
  }
  return output + message.slice(cursor);
};

const redactParameterValues = (message: string): string =>
  redactAssignments(
    message
      .replace(/\bBearer\s+[A-Za-z0-9._~+\/-]+=*/gi, 'Bearer [REDACTED]')
      .replace(/([?&]([^?&#\s="'<>]+)=)([^&#\s"'<>]*)/g, (match, prefix, key) =>
        isDiagnosticSecretKey(key, true) ? `${prefix}${REDACTED}` : match,
      ),
  );

/** Newly exposed encoded secrets lose the entire string, avoiding ambiguous decoded value boundaries. */
export const redactDiagnosticSecrets = (message: string): string => {
  let current = message;
  for (let depth = 0; depth < MAX_DECODE_DEPTH; depth++) {
    const safe = redactParameterValues(current);
    const decoded = decodeDiagnosticLayer(safe);
    if (decoded === safe) return safe;
    if (redactParameterValues(decoded) !== decoded) return REDACTED;
    current = decoded;
  }
  return decodeDiagnosticLayer(current) === current ? redactParameterValues(current) : REDACTED;
};
