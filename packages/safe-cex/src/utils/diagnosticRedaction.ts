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

/** Detached diagnostic copy. Never mutate the message still needed by trading. */
export function redactDiagnosticData<T>(input: T): T {
  const seen = new WeakMap<object, unknown>();
  const text = (value: string): string => {
    // Parse before decoding escapes so nested serialized frames keep their boundaries.
    for (const candidate of [value, stripDiagnosticUrlQueries(value)]) {
      if (!/^[\s]*[\[{]/.test(candidate)) continue;
      try {
        const parsed: unknown = JSON.parse(candidate);
        if (parsed && typeof parsed === 'object') return JSON.stringify(visit(parsed));
      } catch {
        // Non-JSON console text still crosses the conservative text boundary below.
      }
    }
    const safe = stripDiagnosticUrlQueries(value);
    const keys = safe.matchAll(/(?:^|[^A-Za-z0-9_.-])["']?([A-Za-z0-9_.-]+)["']?\s*[:=]\s*/g);
    for (const match of keys) {
      const tail = safe.slice((match.index ?? 0) + match[0].length);
      const omitted = /^(?:"\[REDACTED\]"|'\[REDACTED\]'|\[REDACTED\])(?=$|[\s,}\]&;])/.test(tail);
      if (isExchangeCredentialKey(match[1]) || (isCredentialKey(match[1]) && !omitted)) return '[REDACTED]';
    }
    // A prefixed/truncated auth frame cannot be parsed safely. Do not retain its positional values.
    if (
      /(?:op|method|event|action)["']?\s*[:=]\s*["']?(?:user\.)?(?:auth\w*|login)\b/i.test(safe) &&
      /(?:args|params)["']?\s*[:=]/i.test(safe)
    )
      return '[REDACTED]';
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
    const header = Array.isArray(value) && typeof value[0] === 'string' && isCredentialKey(value[0]);
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
    const auth = entries.some(
      ([key, operation]) => /^(?:op|method|event|action|type)$/i.test(key) && isAuthOperation(operation),
    );
    for (const [key, child] of entries) {
      Object.defineProperty(output, stripDiagnosticUrlQueries(key), {
        value:
          (header && key === '1') || isCredentialKey(key) || (auth && /^(?:args|params)$/i.test(key))
            ? '[REDACTED]'
            : visit(child),
        enumerable: true,
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
    return typeof safe === 'string' ? safe : (JSON.stringify(safe) ?? String(safe));
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
