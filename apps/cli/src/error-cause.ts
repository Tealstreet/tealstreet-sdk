/**
 * The terminal text for a console.error/warn call. A prefix such as
 * "bybitdemo error" is never printed alone: an argument that used to add
 * nothing now contributes its cause, falling back to a bounded serialization.
 * Arguments that already printed something print exactly what they did before.
 */

const MESSAGE_FIELDS = ['message', 'retMsg', 'ret_msg', 'msg', 'error_description', 'error', 'reason'] as const;
const CODE_FIELDS = ['code', 'retCode', 'ret_code', 'kind', 'status', 'statusCode'] as const;
const NESTED_FIELDS = ['raw_response', 'rawResponse', 'cause', 'data', 'response'] as const;
/** The fields the terminal always printed, verbatim, in this order. */
const LEGACY_FIELDS = ['message', 'retMsg', 'msg'] as const;
const MAX_SERIALIZED = 300;
const MAX_DEPTH = 3;
/** Never print values under these keys: request headers and configs carry credentials. */
const SECRET_KEY = /key|secret|sign|pass|token|auth|cookie|session|private|mnemonic|seed|jwt|credential/i;

/** Strings redundant with the CLI's own framing. */
const REDUNDANT_PREFIXES = new Set(['exchange error', 'Exchange error:']);

const nonBlank = (value: unknown): string | undefined =>
  typeof value === 'string' && value.trim() ? value : undefined;

const scalar = (value: unknown): string | undefined =>
  typeof value === 'number' || typeof value === 'boolean' ? String(value) : nonBlank(value)?.trim();

function parsed(value: unknown): unknown {
  const raw = nonBlank(value)?.trim();
  if (!raw || !/^[[{]/.test(raw)) return value;
  try {
    return JSON.parse(raw);
  } catch {
    return value;
  }
}

/** A JSON string value (a proxied raw_response) is parsed so its inner keys are masked too. */
function maskSecrets(key: string, value: unknown): unknown {
  if (key && SECRET_KEY.test(key)) return '[redacted]';
  if (typeof value !== 'string') return value;
  const inner = parsed(value);
  return inner !== null && typeof inner === 'object' ? inner : value;
}

function serialize(value: unknown): string {
  let json: string | undefined;
  try {
    json = JSON.stringify(value, maskSecrets);
  } catch {
    json = undefined;
  }
  const body = json === undefined ? '(unserializable object)' : json === '{}' ? '(empty object)' : json;
  return body.length > MAX_SERIALIZED ? `${body.slice(0, MAX_SERIALIZED - 3)}...` : body;
}

/** The message and code an object carries, searching common envelope nesting. */
function fields(value: unknown, depth: number): { message?: string; code?: string } {
  const object = parsed(value);
  if (!object || typeof object !== 'object') return { message: nonBlank(object)?.trim() };
  const record = object as Record<string, unknown>;
  let message = MESSAGE_FIELDS.map((key) => nonBlank(record[key])?.trim()).find(Boolean);
  let code = CODE_FIELDS.map((key) => scalar(record[key])).find(Boolean);
  if (depth < MAX_DEPTH && (!message || !code)) {
    for (const key of NESTED_FIELDS) {
      if (record[key] === undefined || record[key] === object) continue;
      const nested = fields(record[key], depth + 1);
      message ??= nested.message;
      code ??= nested.code;
      if (message && code) break;
    }
  }
  return { message, code };
}

/** One argument's cause: never empty. */
export function describeCause(value: unknown): string {
  if (value === undefined || value === null) return `(no details: ${String(value)})`;
  if (typeof value !== 'object') return nonBlank(value) ?? scalar(value) ?? `(empty ${typeof value})`;
  const { message, code } = fields(value, 0);
  const name = value instanceof Error && value.name && value.name !== 'Error' ? value.name : undefined;
  if (message) return message;
  if (code) return `${name ?? 'error'} code ${code}`;
  return name ?? (value instanceof Error ? 'Error (no message)' : serialize(value));
}

/** What the terminal printed for an object before: its message, retMsg or msg, verbatim. */
function legacyText(arg: object): string | undefined {
  if (arg instanceof Error) return arg.message;
  const field = LEGACY_FIELDS.find((key) => key in arg);
  return field === undefined ? undefined : String((arg as Record<string, unknown>)[field]);
}

/** The terminal text for console.error/warn arguments; null only when nothing was given. */
export function terminalErrorText(args: unknown[]): string | null {
  if (args.length === 0) return null;
  const parts: string[] = [];
  const open = () => parts.length === 0 || /:\s*$/.test(parts[parts.length - 1]);
  for (const arg of args) {
    if (typeof arg === 'string') {
      if (!REDUNDANT_PREFIXES.has(arg)) parts.push(arg);
    } else if (arg !== null && typeof arg === 'object' && !Array.isArray(arg)) {
      const legacy = legacyText(arg);
      parts.push(legacy !== undefined && legacy.trim() ? legacy : describeCause(arg));
    } else if (open()) {
      // A trailing primitive used to print nothing; it is the cause only of a bare prefix.
      parts.push(describeCause(arg));
    }
  }
  return parts.join(' ');
}
