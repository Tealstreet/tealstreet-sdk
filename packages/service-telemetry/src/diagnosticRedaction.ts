import {
  isDiagnosticSecretKey,
  redactDiagnosticSecrets,
  stripDiagnosticUrlQueries,
} from '@tealstreet/http-over-websocket/src/diagnosticUrl';

// Credential roles apply regardless of venue, event name or field spelling.
const isCredentialKey = (key: string): boolean => {
  const normalized = stripDiagnosticUrlQueries(key)
    .replace(/[^a-z0-9]/gi, '')
    .toLowerCase();
  return (
    /sign|hmac|proof|auth|jwt|token|key|passphrase|session|challenge|cookie|secret|password|seed|mnemonic/.test(normalized) ||
    /^(?:sig$|kcapi|okaccess|xbapi|xmbx|accesssign)/.test(normalized)
  );
};

const entropy = (value: string): number => {
  const counts = new Map<string, number>();
  for (const char of value) counts.set(char, (counts.get(char) ?? 0) + 1);
  return [...counts.values()].reduce((sum, count) => {
    const p = count / value.length;
    return sum - p * Math.log2(p);
  }, 0);
};

// Value roles catch compact/unknown field names. UUIDs, short IDs and numeric
// prices remain context; long proof-shaped runs are never useful log payloads.
const redactValueShapes = (value: string): string =>
  value
    .replace(/-----BEGIN [^-]+-----[\s\S]*?-----END [^-]+-----/g, '[REDACTED]')
    .replace(/[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}/g, '[REDACTED]')
    .replace(/0x[a-f0-9]{40,}/gi, '[REDACTED]')
    .replace(/[a-f0-9]{40,}/gi, (run) => (entropy(run) >= 1 ? '[REDACTED]' : run))
    .replace(/[A-Za-z0-9+/_-]{32,}={0,2}/g, (run) =>
      /^[a-f0-9]{8}-(?:[a-f0-9]{4}-){3}[a-f0-9]{12}$/i.test(run) || entropy(run) < 3 ? run : '[REDACTED]',
    );

const isAuthOperation = (value: unknown): boolean =>
  typeof value === 'string' && /(?:^|[._:/-])(?:auth(?:ent\w*)?|login)(?:$|[._:/-])/i.test(value);

// Fallback strings and decoded tokens must use the same exchange policy as objects.
const containsCredentialRoles = (value: string): boolean => {
  for (const [, key] of value.matchAll(/\[\s*["']([^"']+)["']\s*,/g)) {
    if (isCredentialKey(key)) return true;
  }
  for (const match of value.matchAll(/(?<![A-Za-z0-9_.-])["']?([A-Za-z0-9_.-]+)["']?\s*[:=]/g)) {
    if (match.index === undefined) continue;
    if (isCredentialKey(match[1]) && !/^[\s"']*\[REDACTED\]/i.test(value.slice(match.index + match[0].length)))
      return true;
  }
  if (/(?:args|params)["']?\s*[:=]/i.test(value)) {
    for (const [, operation] of value.matchAll(
      /(?:^|[^A-Za-z0-9_.-])["']?(?:op|method|event|action|type)["']?\s*[:=]\s*["']?([A-Za-z0-9_./:-]+)/gi,
    )) {
      if (isAuthOperation(operation)) return true;
    }
  }
  return false;
};

// Bound work across nested base64/base64url tokens, including tokens inside JSON.
const redactEncodedCredentials = (value: string, depth = 0, budget = { remaining: 64 }): string =>
  value.replace(/[A-Za-z0-9+/_-]{14,}={0,2}/g, (token) => {
    if (token.length < 16) return token;
    if (token.length > 4096 || budget.remaining-- <= 0 || depth >= 4 || typeof atob !== 'function') return '[REDACTED]';
    let decoded: string;
    try {
      decoded = atob(token.replace(/-/g, '+').replace(/_/g, '/'));
    } catch {
      return token;
    }
    try {
      decoded = new TextDecoder('utf-8', { fatal: true }).decode(
        Uint8Array.from(decoded, (char) => char.charCodeAt(0)),
      );
    } catch {
      // Latin-1 can still contain recognizable credential field names.
    }
    const normalized = stripDiagnosticUrlQueries(scrubUrlUserinfo(decoded));
    if (normalized !== decoded || containsCredentialRoles(normalized) || redactValueShapes(decoded) !== decoded)
      return '[REDACTED]';
    return redactEncodedCredentials(normalized, depth + 1, budget) !== normalized ? '[REDACTED]' : token;
  });

const scrubUrlUserinfo = (value: string): string =>
  redactDiagnosticSecrets(value).replace(/(?:(?:https?|wss?):)?\/\/[^\s<>"'`]+/gi, (url) => {
    try {
      const relative = url.startsWith('//');
      const parsed = new URL(url, 'https://diagnostic.invalid');
      if (!parsed.username && !parsed.password) return url;
      parsed.username = '';
      parsed.password = '';
      return relative ? `//${parsed.host}${parsed.pathname}${parsed.search}${parsed.hash}` : parsed.href;
    } catch {
      return '[invalid URL]';
    }
  });

const serializedFragmentEnd = (value: string, start: number): number | undefined => {
  let depth = 0;
  let inString = false;
  let escaped = false;
  for (let index = start; index < value.length; index++) {
    const char = value[index];
    if (inString) {
      if (escaped) escaped = false;
      else if (char === '\\') escaped = true;
      else if (char === '"') inString = false;
    } else if (char === '"') inString = true;
    else if (char === '{' || char === '[') depth++;
    else if ((char === '}' || char === ']') && --depth === 0) return index + 1;
  }
  return undefined;
};

// ebf3138bff's closed Sentry shape tags stay in the common policy after cutover.
const safeSentryShapeTag = (key: string, value: unknown): boolean => {
  if (typeof value !== 'string') return false;
  switch (key) {
    case 'key_check_path':
      return value === 'add' || value === 'engine' || value === 'hosted';
    case 'signing_mode':
      return value === 'local' || value === 'hosted_cosign' || value === 'hosted_delegated';
    case 'passphrase_len':
      return /^(?:0|[1-9][0-9]{0,5})$/.test(value);
    case 'passphrase_trim_differs':
    case 'passphrase_non_ascii':
      return value === 'true' || value === 'false';
    case 'passphrase_autofilled':
      return value === 'yes' || value === 'no' || value === 'unknown';
    case 'passphrase_entry':
      return value === 'pasted' || value === 'typed' || value === 'unknown';
    default:
      return false;
  }
};

/** Detached diagnostic copy. Never mutate the message still needed by trading. */
export function redactDiagnosticData<T>(input: T): T {
  const seen = new WeakMap<object, unknown>();
  const parserMessages = new Set<string>();
  const inspected = new WeakSet<object>();
  const inspectedStrings = new Set<string>();
  let parserStringBudget = 64;
  const isParserProjection = (value: string): boolean => {
    for (const message of parserMessages) if (value.includes(message)) return true;
    return false;
  };
  // Process handlers and Sentry project parser messages into sibling fields.
  // Collect their class context before visiting any projected text.
  const inspectParsers = (value: unknown, serializedDepth = 0): void => {
    if (typeof value === 'string') {
      if (inspectedStrings.has(value)) return;
      inspectedStrings.add(value);
      for (const candidate of new Set([value, scrubUrlUserinfo(value)])) {
        let nextStart = 0;
        // Logger prefixes and suffixes can surround several complete JSON records.
        for (const match of candidate.matchAll(/[\[{]/g)) {
          const start = match.index;
          if (start === undefined || start < nextStart) continue;
          if (serializedDepth >= 4 || candidate.length > 65_536 || parserStringBudget-- <= 0)
            throw new Error('Diagnostic parser inspection limit exceeded');
          const end = serializedFragmentEnd(candidate, start);
          if (end === undefined) continue;
          let parsed: unknown;
          try { parsed = JSON.parse(candidate.slice(start, end)); } catch { continue; }
          inspectParsers(parsed, serializedDepth + 1);
          nextStart = end;
        }
      }
      return;
    }
    if (!value || typeof value !== 'object' || inspected.has(value)) return;
    if (value instanceof Date || value instanceof ArrayBuffer || ArrayBuffer.isView(value)) return;
    inspected.add(value);
    if (
      value instanceof SyntaxError ||
      ('name' in value && value.name === 'SyntaxError') ||
      ('type' in value && value.type === 'SyntaxError')
    ) {
      for (const message of [
        'message' in value ? value.message : undefined,
        'value' in value ? value.value : undefined,
      ]) {
        if (typeof message === 'string' && message && message !== 'Invalid diagnostic syntax') parserMessages.add(message);
      }
    }
    // Native Error causes and AggregateError members are non-enumerable.
    if ('cause' in value) inspectParsers(value.cause, serializedDepth);
    if ('errors' in value) inspectParsers(value.errors, serializedDepth);
    for (const child of Object.values(value)) inspectParsers(child, serializedDepth);
  };
  const sanitizeText = (value: string): string =>
    scrubUrlUserinfo(value).replace(/\beyJ[A-Za-z0-9_-]*(?:\.[A-Za-z0-9_-]*)*/g, '[REDACTED]');
  const text = (value: string): string => {
    if (isParserProjection(value)) return '[REDACTED]';
    // Parse before decoding escapes so nested serialized frames keep their boundaries.
    for (const candidate of [value, sanitizeText(value)]) {
      const start = candidate.search(/[\[{]/);
      if (start < 0) continue;
      try {
        const parsed: unknown = JSON.parse(candidate.slice(start));
        if (parsed && typeof parsed === 'object') {
          inspectParsers(parsed);
          const prefix = sanitizeText(candidate.slice(0, start));
          if (isParserProjection(value) || isParserProjection(prefix)) return '[REDACTED]';
          if (containsCredentialRoles(prefix)) return '[REDACTED]';
          return redactValueShapes(redactEncodedCredentials(prefix)) + JSON.stringify(visit(parsed));
        }
      } catch {
        // Non-JSON console text still crosses the conservative text boundary below.
      }
    }
    const safe = sanitizeText(value);
    if (/\bSyntaxError\b/.test(safe) || containsCredentialRoles(safe)) return '[REDACTED]';
    return redactValueShapes(redactEncodedCredentials(safe));
  };
  const visit = (value: unknown, path: string[] = [], authTiming = false): unknown => {
    if (typeof value === 'function') return '[function omitted]';
    if (typeof value === 'string') {
      const sentryQueryRole = path.includes('breadcrumbs') || path[0] === 'exception' ||
        (path[0] === 'contexts' && (path[1] === 'request' || path[1] === 'axios'));
      // An existing redaction marker is query data, not trailing URL punctuation.
      if (sentryQueryRole)
        return text(stripDiagnosticUrlQueries(value.replaceAll('[REDACTED]', '%5BREDACTED%5D')));
      if (path.at(-1) === 'stack') {
        return value.split('\n').map((line) =>
          // A validated source coordinate is a code-location role, not a field label.
          !isParserProjection(line) &&
          /^\s*at (?:[A-Za-z0-9_$.[\]<>]+\s+\()?(?:file:\/\/)?[A-Za-z0-9_./@-]+\.[cm]?[jt]sx?:\d+(?::\d+)?\)?$/.test(line)
            ? redactValueShapes(redactEncodedCredentials(sanitizeText(line)))
            : text(line),
        ).join('\n');
      }
      return text(value);
    }
    if (!value || typeof value !== 'object') return value;
    if (value instanceof Date) return new Date(value.getTime());
    if (value instanceof ArrayBuffer || ArrayBuffer.isView(value)) return '[binary omitted]';
    if (seen.has(value)) return seen.get(value);
    const output: object = value instanceof Error ? new Error() : Array.isArray(value) ? [] : {};
    seen.set(value, output);
    const header =
      Array.isArray(value) &&
      typeof value[0] === 'string' &&
      (isCredentialKey(value[0]) || isDiagnosticSecretKey(value[0], true));
    // Worker serialization loses instanceof while preserving the parser class.
    const parserError =
      value instanceof SyntaxError ||
      ('name' in value && value.name === 'SyntaxError') ||
      ('type' in value && value.type === 'SyntaxError');
    if (parserError) {
      Object.assign(output, { name: 'SyntaxError', message: 'Invalid diagnostic syntax' });
      if (output instanceof Error) output.stack = undefined;
      if ('type' in value && value.type === 'SyntaxError')
        Object.assign(output, { type: 'SyntaxError', value: 'Invalid diagnostic syntax' });
      return output;
    }
    const entries = Object.entries(
      value instanceof Error ? { ...value, name: value.name, message: value.message, stack: value.stack } : value,
    );
    const auth = entries.some(
      ([key, operation]) => /^(?:op|method|event|action|type)$/i.test(key) && isAuthOperation(operation),
    );
    const breadcrumb = !Array.isArray(value) && ('category' in value || path.includes('breadcrumbs'));
    for (const [key, child] of entries) {
      const countRole =
        path.slice(-2).join('.') === 'contexts.client_cpu_pressure' &&
        /^(?:reportsThisSession|maxReportsPerSession)$/.test(key) &&
        typeof child === 'number' &&
        Number.isSafeInteger(child) &&
        child >= 0;
      const correlationRole = path.at(-1) === 'stream' && path.includes('streams') && key === 'sessionKey';
      const shapeRole = path.length === 1 && path[0] === 'tags' && safeSentryShapeTag(key, child);
      // Diagnostic argument vectors may prepend context before a header/value pair.
      const previous = Array.isArray(value) ? value[Number(key) - 1] : undefined;
      const positionalCredential = typeof previous === 'string' && isCredentialKey(previous);
      const credential =
        isCredentialKey(key) || ((authTiming || path.at(-1) === 'headers') && isDiagnosticSecretKey(key, true));
      // Omit changed credential-shaped keys entirely: renaming several keys to
      // the same marker would collide and could retain misleading map entries.
      const safeKey = text(key);
      if (safeKey !== key && (safeKey.includes('[REDACTED]') || isCredentialKey(safeKey))) continue;
      Object.defineProperty(output, safeKey, {
        value:
          !countRole &&
          !correlationRole &&
          !shapeRole &&
          ((header && key === '1') || positionalCredential || credential || (auth && /^(?:args|params)$/i.test(key)))
            ? '[REDACTED]'
            : visit(child, [...path, key], authTiming || (breadcrumb && key === 'data')),
        enumerable: true,
        configurable: true,
        writable: true,
      });
    }
    return output;
  };
  try {
    inspectParsers(input);
    return visit(input) as T;
  } catch {
    // Logging a lazy/host object must not throw from an existing error handler.
    return '[unserializable diagnostic]' as T;
  }
}

/** Console formatting must happen after redaction, including Error.config/data. */
export function diagnosticText(value: unknown): string {
  try {
    const safe = redactDiagnosticData(value);
    return typeof safe === 'string' ? safe : (JSON.stringify(safe) ?? String(safe));
  } catch {
    return '[unserializable diagnostic]';
  }
}

/** Opt-in sink wrapper; unlike replacing global console this affects only its caller. */
export const diagnosticConsole = {
  log: (...args: unknown[]) => globalThis.console.log(...redactDiagnosticData(args)),
  info: (...args: unknown[]) => globalThis.console.info(...redactDiagnosticData(args)),
  warn: (...args: unknown[]) => globalThis.console.warn(...redactDiagnosticData(args)),
  error: (...args: unknown[]) => globalThis.console.error(...redactDiagnosticData(args)),
  debug: (...args: unknown[]) => globalThis.console.log(...redactDiagnosticData(args)),
};
