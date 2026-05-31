/**
 * Log classification — the single taxonomy for "where does this terminal-bound
 * line go." Pure + dependency-free so it's unit-testable and shared across the
 * console patch (logger.ts) and the surfacing layer (writeAbovePrompt / the
 * status-line toaster).
 *
 * Categories:
 *   - `operational` — routine WS/keepalive diagnostics. File only; never on the
 *     terminal. This is the noise that used to corrupt the prompt
 *     (`[trackLastReceived] … sending ping`).
 *   - `info`        — general informational console.log. Shown normally today.
 *   - `warn`        — warnings. Toasted (Phase 3).
 *   - `error`       — real, user-facing errors. Surfaced persistently.
 *   - `critical`    — errors a trader must not miss (auth failure, repeated
 *                     disconnect, liquidation). Surfaced loud + persistent.
 *
 * Severity comes from which console method was called (`log` ≈ info,
 * `warn`, `error`); content patterns refine it: operational prefixes demote
 * info→operational, critical patterns promote warn/error→critical.
 */
export type LogCategory = 'operational' | 'info' | 'warn' | 'error' | 'critical';
export type ConsoleMethod = 'log' | 'warn' | 'error';

/**
 * Routine, high-frequency diagnostics that belong in `cli.log`, not the
 * terminal. Matched only against `console.log` (info-level) output — a genuine
 * `console.error` is never silenced as "operational" even if it mentions a
 * reconnect. Add new noisy prefixes here, not in logger.ts.
 */
export const OPERATIONAL_PATTERNS: readonly RegExp[] = [
  /^\[trackLastReceived\]/,
  /^\[BaseReconnectingWebSocket\]/,
  /^onClose\(\) called/,
  /^onClose handler called/,
  /^setting reconnecting/,
  /^\[HOW\] Latency/, // HTTP-over-WebSocket latency logs (noisy)
  /^\[TradeTunnel\]/, // verbose HoW request/response summaries
  /sending ping/i,
  /\bkeepalive\b/i,
  // safe-cex bitunix dumps the full balance WS message via console.info before
  // refetching from REST — verbose, file-only.
  /received balance message, but we will fetch/i,
];

/**
 * Content that escalates a warn/error to `critical` — things a trader must not
 * miss in a scrolling terminal. Kept conservative; expand as real incidents
 * surface rather than guessing.
 */
export const CRITICAL_PATTERNS: readonly RegExp[] = [
  /authentication failed/i,
  /\btoken (?:expired|invalid|may have expired)/i,
  /\b401\b/,
  /\bunauthorized\b/i,
  /liquidat/i, // liquidation / liquidated
  /max(?:imum)?\s+consecutive\s+failures/i,
];

export function classifyLog(message: string, method: ConsoleMethod): LogCategory {
  if (method === 'log') {
    if (OPERATIONAL_PATTERNS.some((p) => p.test(message))) return 'operational';
    return 'info';
  }
  const isCritical = CRITICAL_PATTERNS.some((p) => p.test(message));
  if (method === 'error') return isCritical ? 'critical' : 'error';
  // method === 'warn'
  return isCritical ? 'critical' : 'warn';
}

/** True when the line is routine noise that should stay out of the terminal. */
export function isOperational(message: string): boolean {
  return OPERATIONAL_PATTERNS.some((p) => p.test(message));
}
