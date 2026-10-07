/**
 * The one scrubber, shared by every service that reports to Sentry.
 *
 * These services hold users' exchange credentials and their errors quote venue
 * responses verbatim, so what leaves the process has to be filtered rather than
 * trusted. It lives in a package because the alternative — a copy per service —
 * drifts, and a scrubber that has drifted is worse than none: it still looks
 * like protection.
 *
 * Lifted from `apps/cli/src/sentry.ts`, which now imports it.
 */

/** Key-shaped values that must never leave the machine. */
export const SENSITIVE_KEY_PATTERN =
  /(authorization|cookie|secret|passphrase|password|privatekey|private_key|api[-_]?key|apisecret|proxykey|proxytoken|token|mnemonic|seed)/i;

// 32+ chars of unbroken base64/hex is an API key or JWT segment, not prose.
//
// `/` is NOT in this class, and that is deliberate. It used to be, and the
// pattern therefore ate FILE PATHS: a stack frame at
// `/app/services/exchange-server/src/remote-signer.ts:62` arrived in Sentry as
// `/[redacted].ts:62`, which is the one piece of information a stack trace
// exists to carry. Excluding `/` also spares URLs, while still catching the
// part that matters — a signature or key in a query string has no slash in it.
const KEY_SHAPED_VALUE = /\b[A-Za-z0-9_\-+]{32,}={0,2}\b/g;

/**
 * The base64 that DOES contain `/`, which the rule above now walks past.
 *
 * Distinguished from a path by entropy rather than by length: a real key mixes
 * cases and digits, and `services/exchange-server/src/remote-signer` does not.
 * Requiring all three is what keeps a path readable while an encoded secret
 * still goes.
 */
const SLASHED_KEY_SHAPED_VALUE = /(?=[A-Za-z0-9_\-+/]*[a-z])(?=[A-Za-z0-9_\-+/]*[A-Z])(?=[A-Za-z0-9_\-+/]*\d)[A-Za-z0-9_\-+/]{32,}={0,2}/g;

export function scrubString(value: string): string {
  // Slashed first, or the narrower rule bites a chunk out of the middle of a
  // slashed secret and leaves its first segment in place.
  return value.replace(SLASHED_KEY_SHAPED_VALUE, '[redacted]').replace(KEY_SHAPED_VALUE, '[redacted]');
}

export function scrub(value: unknown, depth = 0): unknown {
  if (depth > 4) return '[truncated]';
  if (typeof value === 'string') return scrubString(value);
  if (Array.isArray(value)) return value.slice(0, 16).map((item) => scrub(item, depth + 1));

  if (value && typeof value === 'object') {
    const out: Record<string, unknown> = {};
    for (const [key, child] of Object.entries(value as Record<string, unknown>).slice(0, 32)) {
      out[key] = SENSITIVE_KEY_PATTERN.test(key) ? '[redacted]' : scrub(child, depth + 1);
    }
    return out;
  }

  return value;
}
