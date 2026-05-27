/**
 * Shared connect-failure classification for the two account-connect paths
 * (the REPL background-connect handler in `repl.ts` and the one-shot
 * `runSingleCommand`). Keeping it in one pure, exported helper means the
 * paths can't drift and the branches are unit-testable without dragging the
 * whole REPL (readline / exchange manager) into a test.
 *
 * Case-insensitive on purpose: exchange errors arrive in mixed case
 * (e.g. "Authentication failed", "Request timed out").
 */
export type ConnectErrorKind = 'auth' | 'timeout' | 'generic';

export function classifyConnectError(errorMsg: string): ConnectErrorKind {
  const m = errorMsg.toLowerCase();
  if (m.includes('authentication') || m.includes('token') || m.includes('401')) return 'auth';
  if (m.includes('timed out') || m.includes('timeout')) return 'timeout';
  return 'generic';
}
