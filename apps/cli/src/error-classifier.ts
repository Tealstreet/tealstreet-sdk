/**
 * CLI Error Classifier
 *
 * Maps ErrorCategory (from ExchangeHealthMonitor) to CLI-specific actions.
 * Mirrors the web app's categoryToToastAction pattern but with CLI-appropriate
 * actions: suppress, critical (show once), warn, error, passthrough.
 */

import { classifiedError, type ClassifiedError } from '../../../packages/safe-cex/src/commandOutcome';
import type { ErrorCategory } from '../../../packages/safe-cex/src/health/errorPatterns';

export type CLIAction =
  | { action: 'suppress' }
  | { action: 'critical'; message?: string }
  | { action: 'warn' }
  | { action: 'error'; message?: string }
  | { action: 'passthrough' };

const BYBIT_ENV_MISMATCH_MESSAGE_PREFIX = 'Bybit API key environment mismatch';

/**
 * Classify an error category into a CLI action.
 * Exhaustive switch ensures compile-time safety when new categories are added.
 */
export function classifyForCLI(
  category: ErrorCategory | null,
  _msgType: 'error' | 'warning' = 'error',
  rawMessage?: string,
): CLIAction {
  switch (category) {
    // Suppressed — don't print at all
    case 'suppressed':
      return { action: 'suppress' };

    // Auth errors — critical, show once prominently
    case 'auth_invalid':
      if (rawMessage?.includes(BYBIT_ENV_MISMATCH_MESSAGE_PREFIX)) {
        return { action: 'critical' };
      }
      return { action: 'critical', message: 'Invalid API key or secret' };
    case 'auth_expired':
      return { action: 'critical', message: 'API key has expired' };
    case 'auth_permissions':
      return { action: 'critical', message: 'Insufficient API key permissions' };

    // Transient issues — warn (auto-recovers)
    case 'rate_limit':
      return { action: 'warn' };
    case 'network':
      return { action: 'warn' };
    case 'exchange_down':
      return { action: 'warn' };
    case 'time_sync':
      return { action: 'warn' };

    // Order-related
    case 'order_warning':
      return { action: 'warn' };
    case 'order_rejected':
    case 'insufficient_balance':
      return { action: 'error' };

    // Unsupported backend — error with context
    case 'unsupported_backend':
      return { action: 'error', message: 'Exchange backend not configured — contact support' };

    // Unknown or unclassified — pass through based on original event type
    case 'unknown':
    case null:
      return { action: 'passthrough' };

    // Exhaustive check — TypeScript compile error if new category added
    default: {
      const _exhaustive: never = category;
      return { action: 'passthrough' };
    }
  }
}

/**
 * Build a dedup key for critical errors (account + category).
 * Used to show critical errors only once per account+category combo.
 */
/** The typed classification an exchange `'error'` event carries in its options, if any. */
export const errorEventClassification = (opts: unknown): ClassifiedError | undefined =>
  opts !== null && typeof opts === 'object'
    ? classifiedError((opts as { classified?: unknown }).classified)
    : undefined;

export function criticalErrorKey(accountName: string, category: ErrorCategory): string {
  return `${accountName}:${category}`;
}
