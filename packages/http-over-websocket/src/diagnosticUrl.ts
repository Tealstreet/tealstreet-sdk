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
    .replace(/&#([0-9]+);?|&#x([0-9a-f]+);?|&(quot|apos|lt|gt|amp);/gi, (_, decimal, hex, named) => {
      if (named)
        return ({ quot: '"', apos: "'", lt: '<', gt: '>', amp: '&' } as Record<string, string>)[named.toLowerCase()];
      const point = parseInt(decimal ?? hex, decimal ? 10 : 16);
      return point > 0 && point <= 0x10ffff ? String.fromCodePoint(point) : '\uFFFD';
    })
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
    if (!isDiagnosticSecretKey(match[1], true)) {
      // A label's final separator or space may also introduce the next key.
      keys.lastIndex--;
      continue;
    }
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
const redactDiagnosticParameters = (message: string): string => {
  let current = message;
  for (let depth = 0; depth < MAX_DECODE_DEPTH; depth++) {
    // Entity ampersands can be part of a value rather than true query delimiters.
    if (/&(?:#|quot;|apos;|lt;|gt;|amp;)/i.test(current)) {
      const decodedEntities = decodeDiagnosticLayer(current);
      if (redactParameterValues(decodedEntities) !== decodedEntities) return REDACTED;
    }
    const safe = redactParameterValues(current);
    const decoded = decodeDiagnosticLayer(safe);
    if (decoded === safe) return safe;
    if (redactParameterValues(decoded) !== decoded) return REDACTED;
    current = decoded;
  }
  return decodeDiagnosticLayer(current) === current ? redactParameterValues(current) : REDACTED;
};

const MAX_BASE64_TOKEN_LENGTH = 4096;
const MAX_BASE64_TOKENS = 64;

const redactBase64Tokens = (message: string, depth = 0, budget = { remaining: MAX_BASE64_TOKENS }): string =>
  message.replace(/[A-Za-z0-9+/_-]{14,}={0,2}/g, (token) => {
    if (token.length < 16) return token;
    if (
      typeof atob !== 'function' ||
      token.length > MAX_BASE64_TOKEN_LENGTH ||
      budget.remaining-- <= 0 ||
      depth >= MAX_DECODE_DEPTH
    )
      return REDACTED;
    let decoded: string;
    try {
      decoded = atob(token.replace(/-/g, '+').replace(/_/g, '/'));
    } catch {
      return token;
    }
    // UTF-8 where valid; otherwise atob's Latin-1 text can still name auth parameters.
    try {
      decoded = new TextDecoder('utf-8', { fatal: true }).decode(
        Uint8Array.from(decoded, (char) => char.charCodeAt(0)),
      );
    } catch {
      /* Keep Latin-1. */
    }
    if (/[\u0000-\u0008\u000B\u000C\u000E-\u001F\u007F-\u009F]/.test(decoded)) return token;
    for (let layer = 0; layer < MAX_DECODE_DEPTH; layer++) {
      if ([...decoded.matchAll(/[A-Za-z0-9_.%+-]+/g)].some(([key]) => isDiagnosticSecretKey(key, true)))
        return REDACTED;
      const next = decodeDiagnosticLayer(decoded);
      if (next === decoded) break;
      decoded = next;
    }
    if (decodeDiagnosticLayer(decoded) !== decoded) return REDACTED;
    return redactBase64Tokens(decoded, depth + 1, budget) !== decoded ? REDACTED : token;
  });

export const redactDiagnosticSecrets = (message: string): string =>
  redactBase64Tokens(redactDiagnosticParameters(message));
