/**
 * Exchange Health Monitor
 *
 * Monitors exchange error/log emits and detects common issues
 * using stable engine codes (legacy TS messages use patterns). Stores recent error context
 * and exposes health state.
 *
 * Usage:
 * - Attach to exchange emitter in BaseExchange
 * - Check health state after startup to detect auth issues
 * - Monitor ongoing health in CLI recorders and web app
 */

import type { EngineEvent, EngineHealth } from '@tealstreet/safe-cex-wasm/host';
import type { ClassifiedError } from '../commandOutcome';

import { exchangeErrorCategory } from '../commandOutcome';
import {
  DEFAULT_ERROR_PATTERNS,
  EXCHANGE_SPECIFIC_PATTERNS,
  mergePatterns,
  type ErrorCategory,
  type ErrorPattern,
  type ErrorSeverity,
} from './errorPatterns';

export type HealthStatus = 'healthy' | 'degraded' | 'error';

/** Kinds precise enough to override text classification. */
const SPECIFIC_KINDS: ReadonlySet<string> = new Set([
  'authInvalid',
  'authExpired',
  'authPermission',
  'rateLimited',
  'venueUnavailable',
  'network',
  'timeout',
]);

export interface ErrorEntry {
  category: ErrorCategory;
  code?: string;
  message: string;
  timestamp: number;
  severity: ErrorSeverity;
}

export interface HealthState {
  status: HealthStatus;
  lastError?: ErrorEntry;
  recentErrors: ErrorEntry[];
  errorCounts: Partial<Record<ErrorCategory, number>>;
  lastUpdated: number;
  /** Authoritative engine counters, including events hidden by the minimum log level. */
  engineHealth?: EngineHealth;
}

export interface ExchangeHealthMonitorOptions {
  /** Maximum number of recent errors to store (default: 10) */
  maxRecentErrors?: number;
  /** Custom error patterns to add/override */
  customPatterns?: ErrorPattern[];
  /** Time window for "degraded" status to recover to "healthy" (ms, default: 60000) */
  recoveryWindowMs?: number;
}

export class ExchangeHealthMonitor {
  private state: HealthState;
  private patterns: ErrorPattern[];
  private maxRecentErrors: number;
  private recoveryWindowMs: number;
  private recoveryTimer?: ReturnType<typeof setTimeout>;

  constructor(exchangeName: string, options: ExchangeHealthMonitorOptions = {}) {
    this.maxRecentErrors = options.maxRecentErrors ?? 10;
    this.recoveryWindowMs = options.recoveryWindowMs ?? 60_000;

    // Merge default patterns with exchange-specific and custom patterns
    const exchangePatterns = EXCHANGE_SPECIFIC_PATTERNS[exchangeName.toLowerCase()] || [];
    this.patterns = mergePatterns(
      DEFAULT_ERROR_PATTERNS,
      exchangePatterns,
      options.customPatterns || []
    );

    // Initialize state
    this.state = {
      status: 'healthy',
      recentErrors: [],
      errorCounts: {},
      lastUpdated: Date.now(),
    };
  }

  /**
   * Process an error or log message and update health state
   * @param message - The message to analyze
   * @param isError - Whether this came from an 'error' event (vs 'log')
   * @param classified - The 'error' event's typed classification; its kind wins over text
   * @returns The detected category, or null if no pattern matched
   */
  processMessage(message: string, isError: boolean = false, classified?: ClassifiedError): ErrorCategory | null {
    if (!message || typeof message !== 'string') {
      return null;
    }

    // Specific kinds win; catch-all kinds (rejected, invalidInput, internal, unsupported)
    // defer to text patterns, which know venue auth/permission codes the engine leaves generic.
    const typed = exchangeErrorCategory(classified);
    const specific = typed && classified?.kind && SPECIFIC_KINDS.has(classified.kind);
    if (typed && specific) return this.recordTyped(typed, message);

    const lowerMessage = message.toLowerCase();

    for (const pattern of this.patterns) {
      for (const p of pattern.patterns) {
        const matches = typeof p === 'string'
          ? lowerMessage.includes(p.toLowerCase())
          : p.test(message);

        if (matches) {
          this.recordError(pattern.category, message, pattern.severity);
          return pattern.category;
        }
      }
    }

    if (typed) return this.recordTyped(typed, message);

    // If it's explicitly an error but no pattern matched, record as unknown
    if (isError) {
      this.recordError('unknown', message, 'warning');
      return 'unknown';
    }

    return null;
  }

  /** Classify native events without inspecting message wording. */
  processEvent(event: EngineEvent, classified?: ClassifiedError): ErrorCategory | null {
    const typed = exchangeErrorCategory(classified);
    let category: ErrorCategory;
    switch (event.code) {
      case 'auth.rejected':
        category = typed === 'auth_expired' || typed === 'auth_permissions' ? typed : 'auth_invalid';
        break;
      case 'rate_limited':
      case 'request.paused':
        category = 'rate_limit';
        break;
      case 'ws.closed':
      case 'ws.reconnect':
      case 'ws.disconnected':
      case 'stream.unavailable':
      case 'http.failed':
        category = 'network';
        break;
      case 'order.rejected':
      case 'child_leg.failed':
        category = typed ?? 'order_rejected';
        break;
      default:
        if (event.level !== 'warn' && event.level !== 'error') return null;
        category = typed ?? 'unknown';
    }
    const severity = category.startsWith('auth_') ? 'critical'
      : event.level === 'warn' || event.level === 'error' ? 'warning' : 'info';
    this.recordError(category, event.message, severity, event.code);
    return category;
  }

  /**
   * Refresh from exchange.getHealth(). Never derive lifetime counters from
   * emitted events: minimum-level filtering and engine auth dedupe hide some.
   * Event history/category counts describe reports observed by this monitor.
   */
  processHealth(health: EngineHealth | undefined): void {
    if (!health) return;
    this.state.engineHealth = { ...health };
    this.state.lastUpdated = Date.now();
  }

  private recordTyped(category: ErrorCategory, message: string): ErrorCategory {
    const severity = this.patterns.find((pattern) => pattern.category === category)?.severity ?? 'warning';
    this.recordError(category, message, severity);
    return category;
  }

  /**
   * Record an error and update health state
   */
  private recordError(category: ErrorCategory, message: string, severity: ErrorSeverity, code?: string): void {
    const entry: ErrorEntry = {
      category,
      ...(code === undefined ? {} : { code }),
      message,
      timestamp: Date.now(),
      severity,
    };

    // Add to recent errors
    this.state.recentErrors.push(entry);
    if (this.state.recentErrors.length > this.maxRecentErrors) {
      this.state.recentErrors.shift();
    }

    // Update last error
    this.state.lastError = entry;
    this.state.lastUpdated = Date.now();

    // Update error counts
    this.state.errorCounts[category] = (this.state.errorCounts[category] || 0) + 1;

    // Update status based on severity
    if (severity === 'critical') {
      this.state.status = 'error';
      // Critical errors don't auto-recover
      this.clearRecoveryTimer();
    } else if (severity === 'warning' && this.state.status === 'healthy') {
      this.state.status = 'degraded';
      // Start recovery timer for degraded state
      this.scheduleRecovery();
    }
  }

  /**
   * Schedule auto-recovery from degraded state
   */
  private scheduleRecovery(): void {
    this.clearRecoveryTimer();
    this.recoveryTimer = setTimeout(() => {
      if (this.state.status === 'degraded') {
        this.state.status = 'healthy';
        this.state.lastUpdated = Date.now();
      }
    }, this.recoveryWindowMs);
  }

  private clearRecoveryTimer(): void {
    if (this.recoveryTimer) {
      clearTimeout(this.recoveryTimer);
      this.recoveryTimer = undefined;
    }
  }

  /**
   * Get the current health state (returns a copy)
   */
  getState(): HealthState {
    return {
      ...this.state,
      ...(this.state.engineHealth ? { engineHealth: { ...this.state.engineHealth } } : {}),
      recentErrors: [...this.state.recentErrors],
      errorCounts: { ...this.state.errorCounts },
    };
  }

  /**
   * Check if exchange is currently healthy
   */
  isHealthy(): boolean {
    return this.state.status === 'healthy';
  }

  /**
   * Check if exchange has any authentication errors
   */
  hasAuthError(): boolean {
    const authCategories: ErrorCategory[] = ['auth_invalid', 'auth_expired', 'auth_permissions'];
    return authCategories.some(cat => (this.state.errorCounts[cat] || 0) > 0);
  }

  /**
   * Check if a specific error category has occurred
   */
  hasError(category: ErrorCategory): boolean {
    return (this.state.errorCounts[category] || 0) > 0;
  }

  /**
   * Get count for a specific error category
   */
  getErrorCount(category: ErrorCategory): number {
    return this.state.errorCounts[category] || 0;
  }

  /**
   * Get the most recent error, if any
   */
  getLastError(): ErrorEntry | undefined {
    return this.state.lastError ? { ...this.state.lastError } : undefined;
  }

  /**
   * Get recent errors filtered by category
   */
  getErrorsByCategory(category: ErrorCategory): ErrorEntry[] {
    return this.state.recentErrors.filter(e => e.category === category);
  }

  /**
   * Reset health state to healthy
   */
  reset(): void {
    this.clearRecoveryTimer();
    this.state = {
      status: 'healthy',
      recentErrors: [],
      errorCounts: {},
      lastUpdated: Date.now(),
    };
  }

  /**
   * Force set status (useful for external health signals)
   */
  setStatus(status: HealthStatus): void {
    this.state.status = status;
    this.state.lastUpdated = Date.now();
    if (status === 'degraded') {
      this.scheduleRecovery();
    }
  }

  /**
   * Dispose of the monitor (cleanup timers)
   */
  dispose(): void {
    this.clearRecoveryTimer();
  }

  /**
   * Get a summary string for display
   */
  getSummary(): string {
    const { status, lastError, errorCounts } = this.state;

    if (status === 'healthy') {
      return 'OK';
    }

    const counts = Object.entries(errorCounts)
      .filter(([, count]) => count > 0)
      .map(([cat, count]) => `${cat}: ${count}`)
      .join(', ');

    if (lastError) {
      return `${status.toUpperCase()} - ${lastError.category}: ${lastError.message.slice(0, 50)}${lastError.message.length > 50 ? '...' : ''}`;
    }

    return `${status.toUpperCase()} - ${counts}`;
  }
}
