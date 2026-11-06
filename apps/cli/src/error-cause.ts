/**
 * The terminal text for a console.error/warn call. A prefix such as
 * "bybitdemo error" is never printed alone: every non-string argument
 * contributes its cause, falling back to a bounded serialization.
 */

const MESSAGE_FIELDS = ['message', 'retMsg', 'ret_msg', 'msg', 'error_description', 'error', 'reason'] as const;
const CODE_FIELDS = ['code', 'retCode', 'ret_code', 'kind', 'status', 'statusCode'] as const;
const NESTED_FIELDS = ['raw_response', 'rawResponse', 'cause', 'data', 'response'] as const;
const MAX_SERIALIZED = 300;
const MAX_DEPTH = 3;

/** Strings redundant with the CLI's own framing. */
const REDUNDANT_PREFIXES = new Set(['exchange error', 'Exchange error:']);

const text = (value: unknown): string | undefined =>
  typeof value === 'string' ? (value.trim() ? value.trim() : undefined) : undefined;

const scalar = (value: unknown): string | undefined =>
  typeof value === 'number' || typeof value === 'boolean' ? String(value) : text(value);

function parsed(value: unknown): unknown {
  const raw = text(value);
  if (!raw || !/^[[{]/.test(raw)) return value;
  try {
    return JSON.parse(raw);
  } catch {
    return value;
  }
}

function serialize(value: unknown): string {
  let json: string | undefined;
  try {
    json = JSON.stringify(value);
  } catch {
    json = undefined;
  }
  const body = json === undefined ? '(unserializable object)' : json === '{}' ? '(empty object)' : json;
  return body.length > MAX_SERIALIZED ? `${body.slice(0, MAX_SERIALIZED - 3)}...` : body;
}

/** The message and code an object carries, searching common envelope nesting. */
function fields(value: unknown, depth: number): { message?: string; code?: string } {
  const object = parsed(value);
  if (!object || typeof object !== 'object') return { message: text(object) };
  const record = object as Record<string, unknown>;
  let message = MESSAGE_FIELDS.map((key) => text(record[key])).find(Boolean);
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
  if (typeof value !== 'object') return text(value) ?? scalar(value) ?? `(empty ${typeof value})`;
  const { message, code } = fields(value, 0);
  const name = value instanceof Error && value.name && value.name !== 'Error' ? value.name : undefined;
  if (message) return message;
  if (code) return `${name ?? 'error'} code ${code}`;
  return name ?? (value instanceof Error ? 'Error (no message)' : serialize(value));
}

/** The terminal text for console.error/warn arguments; null only for blank or redundant strings. */
export function terminalErrorText(args: unknown[]): string | null {
  if (args.length === 0) return null;
  const parts: string[] = [];
  for (const arg of args) {
    if (typeof arg === 'string') {
      if (!REDUNDANT_PREFIXES.has(arg) && arg.trim()) parts.push(arg);
    } else {
      parts.push(describeCause(arg));
    }
  }
  return parts.length > 0 ? parts.join(' ') : null;
}
