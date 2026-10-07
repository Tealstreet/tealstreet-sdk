/**
 * The throttle key for a process-level alert.
 *
 * The key IS the throttle bucket, so anything unique-per-occurrence inside it
 * disables throttling silently. exchange-server learned this the hard way:
 * `Signing request Uj1n1kabvdORzqSyYHyV8 timed out` minted a fresh bucket every
 * few seconds and posted every one of them to a channel whose whole point was
 * that it only carried things worth waking up for.
 *
 * Shared rather than copied because the next service to add process handlers
 * would otherwise re-derive it, and re-derive it slightly differently.
 */

/**
 * Deliberately narrow in both directions. Over-collapsing is the worse failure:
 * it silences a real alert for a whole window because an unrelated condition
 * shares its bucket. So bare numbers are NOT collapsed — `status code 401` and
 * `status code 503` are different conditions — and the id rule needs 16+ chars
 * with a letter AND a digit, or it eats `ECONNREFUSED`, `disconnected` and
 * `shared-public-restart`.
 */
const VOLATILE_TOKENS: [RegExp, string][] = [
  [/\b\d{1,3}(?:\.\d{1,3}){3}\b/g, '<ip>'],
  [/\b\d{10,}\b/g, '<ts>'], // epoch ms — "banned until 1788849295904"
  [/\b(?=[\w-]*\d)(?=[\w-]*[A-Za-z])[A-Za-z0-9_-]{16,}\b/g, '<id>'], // nanoid request ids
];

/**
 * `JSON.stringify` returns `undefined` for `undefined` and THROWS on a circular
 * value or a BigInt. Both reach here: `Promise.reject()` with no argument is a
 * real rejection shape. Throwing inside an `unhandledRejection` handler
 * re-enters as an uncaughtException — which is the key that pages — so a
 * malformed rejection would alert the critical channel for nothing.
 */
function describe(err: unknown): string {
  if (err instanceof Error) return `${err.name}:${err.message}`;
  if (typeof err === 'string') return err;
  try {
    return JSON.stringify(err) ?? String(err);
  } catch {
    return String(err);
  }
}

export function alertKey(prefix: string, err: unknown): string {
  const signature = VOLATILE_TOKENS.reduce(
    (acc, [re, sub]) => acc.replace(re, sub),
    describe(err).replace(/\s+/g, ' '),
  );
  return `${prefix}:${signature.slice(0, 160)}`;
}
