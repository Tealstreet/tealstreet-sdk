// Several adapters (OKX, Bybit, Bitget, Phemex) rethrow `new Error(message)`, dropping status and code,
// so the message is usually all there is.
const UNKNOWN_OUTCOME_MESSAGE =
  /timeout|timed out|socket hang up|network error|network (connection|request) failed|tradetunnel|connection (lost|closed|reset)|failed to fetch|send status unknown|econnreset|econnaborted|etimedout|status code 5\d\d|bad gateway|gateway time-?out|service (temporarily )?unavailable|upstream/i;
const NETWORK_CODES = new Set(['ECONNRESET', 'ECONNABORTED', 'ETIMEDOUT', 'ERR_NETWORK']);
// Binance -1007: "Timeout waiting for response from backend server. Send status unknown".
const UNKNOWN_OUTCOME_VENUE_CODES = new Set([-1007]);
// Hosted mode signs remotely: these fail BEFORE the send, so nothing can have landed.
const NOT_SENT_MESSAGE =
  /(Signing request \S+ timed out|Remote signer unavailable|Failed to send signing request|signing aborted)/;

/** A hosted request that failed while getting its signature, so it never reached the venue. */
export const isUnsentSigningFailure = (message: string | undefined): boolean => NOT_SENT_MESSAGE.test(message ?? '');

/** Whether a failed send may still have placed the order: the venue never answered, or said it does not know. */
export const isUnknownOutcome = (error: unknown): boolean => {
  if (typeof error !== 'object' || error === null) return false;
  // Rust venues classify the outcome themselves; text matching is the TS fallback.
  const { outcome } = error as { outcome?: unknown };
  if (outcome === 'unknown') return true;
  if (outcome === 'notSent' || outcome === 'rejected') return false;
  const { code, message, name, response } = error as {
    code?: string;
    message?: string;
    name?: string;
    response?: { status?: number; data?: { code?: number } };
  };
  if (isUnsentSigningFailure(message)) return false;
  if (response) {
    return (response.status ?? 0) >= 500 || UNKNOWN_OUTCOME_VENUE_CODES.has(Number(response.data?.code));
  }
  return (
    (code !== undefined && NETWORK_CODES.has(code)) ||
    UNKNOWN_OUTCOME_MESSAGE.test(message ?? '') ||
    /tradetunnel/i.test(name ?? '')
  );
};
