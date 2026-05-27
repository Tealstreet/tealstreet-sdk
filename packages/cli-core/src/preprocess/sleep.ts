/** Hard cap on a single `sleep` command — 10 minutes per spec. */
export const MAX_SLEEP_MS = 10 * 60 * 1000;

/**
 * Parse a sleep duration string. Accepts `ms`, `s`, `m`. Default unit is
 * seconds (so `sleep 5` == `sleep 5s`). Caps at 10 minutes.
 *
 * Examples: `5` → 5000, `300ms` → 300, `30s` → 30000, `2m` → 120000.
 */
export function parseDuration(input: string): number {
  const trimmed = input.trim().toLowerCase();
  if (!trimmed) {
    throw new Error('sleep: missing duration');
  }
  const match = trimmed.match(/^(\d+(?:\.\d+)?)\s*(ms|s|m)?$/);
  if (!match) {
    throw new Error(`sleep: invalid duration "${input}" (expected e.g. 250ms, 5s, 2m)`);
  }
  const n = parseFloat(match[1]);
  const unit = match[2] || 's';

  let ms: number;
  switch (unit) {
    case 'ms':
      ms = n;
      break;
    case 's':
      ms = n * 1000;
      break;
    case 'm':
      ms = n * 60_000;
      break;
    default:
      // unreachable thanks to the regex, but keeps TS happy
      throw new Error(`sleep: unknown unit "${unit}"`);
  }

  if (ms < 0 || !Number.isFinite(ms)) {
    throw new Error(`sleep: invalid duration "${input}"`);
  }
  if (ms > MAX_SLEEP_MS) {
    throw new Error(`sleep: ${input} exceeds 10 minute cap`);
  }
  return ms;
}
