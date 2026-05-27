/**
 * Exchange Health Monitoring System
 *
 * Provides fuzzy error detection for exchange issues.
 * Monitors error/log emits and categorizes them.
 */

export {
  ExchangeHealthMonitor,
  type HealthStatus,
  type HealthState,
  type ErrorEntry,
  type ExchangeHealthMonitorOptions,
} from './ExchangeHealthMonitor';

export {
  DEFAULT_ERROR_PATTERNS,
  EXCHANGE_SPECIFIC_PATTERNS,
  mergePatterns,
  type ErrorCategory,
  type ErrorPattern,
  type ErrorSeverity,
} from './errorPatterns';
