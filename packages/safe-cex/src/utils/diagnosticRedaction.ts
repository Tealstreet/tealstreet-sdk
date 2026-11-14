// Diagnostic policy parity: tealstreet-next origin/master 6fde9450cf.
import { isDiagnosticSecretKey, stripDiagnosticUrlQueries } from '@tealstreet/http-over-websocket/src/diagnosticUrl';

// HTTP query redaction does not cover every exchange's headers or WS auth protocol.
const isExchangeCredentialKey = (key: string): boolean =>
  /authent|jwt|session/.test(
    stripDiagnosticUrlQueries(key)
      .replace(/[^a-z0-9]/gi, '')
      .toLowerCase(),
  );
const isCredentialKey = (key: string): boolean => isDiagnosticSecretKey(key) || isExchangeCredentialKey(key);
const isAuthOperation = (value: unknown): boolean =>
  typeof value === 'string' && /(?:^|[._:-])(?:auth(?:ent\w*)?|login)(?:$|[._:-])/i.test(value);

const isAuthOperationField = (key: string, value: unknown): boolean =>
  /^(?:op|method|event|action|type)$/i.test(key) && isAuthOperation(value);

const isCredentialHeaderTuple = (value: unknown): boolean =>
  Array.isArray(value) &&
  value.length === 2 &&
  typeof value[0] === 'string' &&
  typeof value[1] === 'string' &&
  /^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/.test(value[0]) &&
  isCredentialKey(value[0]);

/** Detached diagnostic copy. Never mutate the message still needed by trading. */
export function redactDiagnosticData<T>(input: T): T {
  const seen = new WeakMap<object, unknown>();
  const text = (value: string): string => {
    // Parse complete JSON before decoding escapes, including a log prefix.
    // Bound failed suffix probes so malformed diagnostics cannot cause quadratic work.
    for (const candidate of [value, stripDiagnosticUrlQueries(value)]) {
      let attempts = 0;
      for (const start of candidate.matchAll(/[\[{]/g)) {
        if (++attempts > 16) break;
        let parsed: unknown;
        try {
          parsed = JSON.parse(candidate.slice(start.index));
        } catch {
          continue;
        }
        if (parsed && typeof parsed === 'object') {
          const prefix = candidate.slice(0, start.index);
          const safePrefix = prefix ? text(prefix) : '';
          // A prefix can itself introduce positional auth values in the JSON suffix.
          if (safePrefix === '[REDACTED]') return safePrefix;
          return safePrefix + JSON.stringify(visit(parsed));
        }
      }
    }
    const safe = stripDiagnosticUrlQueries(value);
    const keys = /(?:^|[^A-Za-z0-9_.-])["']?([A-Za-z0-9_.-]+)["']?\s*[:=]\s*/g;
    let match: RegExpExecArray | null;
    while ((match = keys.exec(safe))) {
      // Revisit the label's final separator or space as the next key's boundary.
      keys.lastIndex--;
      const tail = safe.slice(match.index + match[0].length);
      const omitted = /^(?:"\[REDACTED\]"|'\[REDACTED\]'|\[REDACTED\])(?=$|[\s,}\]&;])/.test(tail);
      if (isExchangeCredentialKey(match[1]) || (isCredentialKey(match[1]) && !omitted)) return '[REDACTED]';
    }
    // Truncated auth frames use exactly the structured operation-key/value policy.
    if (/(?:args|params)["']?\s*[:=]/i.test(safe)) {
      // Inspect values without consuming them: a label's value may be the next key.
      const fields = /(?:^|[^A-Za-z0-9_.-])["']?([A-Za-z0-9_.-]+)["']?\s*[:=]\s*(?=["']?([A-Za-z0-9_.:-]+))/g;
      while ((match = fields.exec(safe))) {
        fields.lastIndex--;
        if (isAuthOperationField(match[1], match[2])) return '[REDACTED]';
      }
    }
    return safe;
  };
  const visit = (value: unknown): unknown => {
    if (typeof value === 'function') return '[function omitted]';
    if (typeof value === 'string') return text(value);
    if (!value || typeof value !== 'object') return value;
    if (value instanceof Date) return new Date(value.getTime());
    if (value instanceof ArrayBuffer || ArrayBuffer.isView(value)) return '[binary omitted]';
    if (seen.has(value)) return seen.get(value);
    const output: object = value instanceof Error ? new Error() : Array.isArray(value) ? [] : {};
    seen.set(value, output);
    const header = isCredentialHeaderTuple(value);
    const entries =
      value instanceof Error
        ? Object.entries({
            ...value,
            name: value.name,
            message: value.message,
            stack: value.stack,
            ...('cause' in value ? { cause: value.cause } : {}),
          })
        : Object.entries(value);
    const auth = entries.some(([key, operation]) => isAuthOperationField(key, operation));
    for (const [key, child] of entries) {
      Object.defineProperty(output, stripDiagnosticUrlQueries(key), {
        value:
          (header && key === '1') || isCredentialKey(key) || (auth && /^(?:args|params)$/i.test(key))
            ? '[REDACTED]'
            : visit(child),
        enumerable: !(value instanceof Error && /^(?:name|message|stack|cause)$/.test(key)),
        configurable: true,
        writable: true,
      });
    }
    return output;
  };
  try {
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
    return typeof safe === 'string'
      ? safe
      : (JSON.stringify(safe, (_key, child) =>
          child instanceof Error
            ? {
                ...child,
                name: child.name,
                message: child.message,
                stack: child.stack,
                ...('cause' in child ? { cause: child.cause } : {}),
              }
            : child,
        ) ?? String(safe));
  } catch {
    return '[unserializable diagnostic]';
  }
}

/** Opt-in sink wrapper; unlike replacing global console this affects only its caller. */
export const diagnosticConsole = {
  log: (...args: unknown[]) => globalThis.console.log(...args.map((arg) => redactDiagnosticData(arg))),
  info: (...args: unknown[]) => globalThis.console.info(...args.map((arg) => redactDiagnosticData(arg))),
  warn: (...args: unknown[]) => globalThis.console.warn(...args.map((arg) => redactDiagnosticData(arg))),
  error: (...args: unknown[]) => globalThis.console.error(...args.map((arg) => redactDiagnosticData(arg))),
  debug: (...args: unknown[]) => globalThis.console.log(...args.map((arg) => redactDiagnosticData(arg))),
  trace: (...args: unknown[]) => globalThis.console.trace(...args.map((arg) => redactDiagnosticData(arg))),
};
