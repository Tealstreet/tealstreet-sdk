import { stableHash } from '@tealstreet/utils/src/stableHash';
import { grafanaLog } from './log';
import { LogSeverity } from './LogSeverity';

// Conditional Sentry import - may not be available in all contexts
let Sentry: any;
try {
  Sentry = require('@sentry/nextjs');
} catch {
  // Sentry not available (e.g., in Web Workers)
  Sentry = undefined;
}

export enum DebugLevel {
  Local = 'Local',
  Warning = 'Warning',
  Error = 'Error',
  Info = 'Info',
}

interface DebugLoggerOptions {
  /** The prefix to use in console.log and Grafana logs */
  prefix: string;
  /** Function to check if console logging should be enabled */
  shouldLogToConsole?: (level: DebugLevel) => boolean;
  /** Whether to always enable console logging */
  enableConsoleLog?: boolean;
  /** Override for production check (for use in Web Workers) */
  isProduction?: boolean;
  /** Force external logging even in development */
  forceExternalLogging?: boolean;
  /** Whether to throttle duplicate messages (default: true) */
  throttleDuplicates?: boolean;
  /** Throttle interval in milliseconds (default: 15000) */
  throttleIntervalMs?: number;
}

/**
 * Creates a debug logger function that logs to console (conditionally) and Grafana
 *
 * @example
 * ```typescript
 * // Basic usage with custom condition
 * const debug = createDebugLogger({
 *   prefix: 'MyComponent',
 *   shouldLogToConsole: () => getDevOverrides().otherLogging
 * });
 *
 * // Always log to console
 * const debug = createDebugLogger({
 *   prefix: 'MyComponent',
 *   enableConsoleLog: true
 * });
 *
 * // Usage
 * debug(DebugLevel.Info, 'Something happened', { data: 123 });
 * debug(DebugLevel.Error, 'An error occurred', error);
 * ```
 */
export function createDebugLogger(options: DebugLoggerOptions) {
  const {
    prefix,
    shouldLogToConsole,
    enableConsoleLog = false,
    isProduction,
    forceExternalLogging,
    throttleDuplicates = true,
    throttleIntervalMs = 15000,
  } = options;

  // Map to track throttled messages
  const throttledMessages = new Map<string, number>();

  return (level: DebugLevel, ...args: any[]) => {
    // Create a unique key for the message
    if (throttleDuplicates) {
      const messageKey = `${level}:${stableHash(args)}`;
      const now = Date.now();
      const lastLogged = throttledMessages.get(messageKey);

      if (lastLogged && now - lastLogged < throttleIntervalMs) {
        // Skip this message as it was recently logged
        return;
      }

      // Update the last logged time
      throttledMessages.set(messageKey, now);

      // Clean up old entries periodically
      if (throttledMessages.size > 100) {
        const cutoff = now - throttleIntervalMs;
        for (const [key, time] of throttledMessages.entries()) {
          if (time < cutoff) {
            throttledMessages.delete(key);
          }
        }
      }
    }
    // Console logging
    if (level === DebugLevel.Error || enableConsoleLog || (shouldLogToConsole && shouldLogToConsole(level))) {
      switch (level) {
        case DebugLevel.Warning:
          console.warn(`[${prefix}]`, ...args);
          break;
        case DebugLevel.Error:
          console.error(`[${prefix}]`, ...args);
          break;
        case DebugLevel.Info:
        case DebugLevel.Local:
        default:
          console.log(`[${prefix}]`, ...args);
          break;
      }
    }

    // Skip external logging in development unless explicitly enabled
    // In Web Workers, use the passed isProduction flag
    // In main thread, check process.env
    let isDevelopment: boolean;
    let shouldForceExternal: boolean;

    if (typeof process !== 'undefined' && process.env) {
      // Main thread
      isDevelopment = process.env.NODE_ENV !== 'production';
      shouldForceExternal = forceExternalLogging || process.env.NEXT_PUBLIC_FORCE_EXTERNAL_LOGGING === '1';
    } else {
      // Web Worker
      isDevelopment = isProduction === false;
      shouldForceExternal = forceExternalLogging || false;
    }

    if (isDevelopment && !shouldForceExternal) {
      return; // Don't send to external services in development
    }

    // Determine log severity
    let logSeverity: LogSeverity = LogSeverity.Info;
    if (level === DebugLevel.Local) {
      return; // Don't send to Grafana
    } else if (level === DebugLevel.Warning) {
      logSeverity = LogSeverity.Warning;
    } else if (level === DebugLevel.Error) {
      logSeverity = LogSeverity.Error;
    }

    // Format message safely
    let errorObject: Error | null = null;

    const formatArg = (arg: any): string => {
      try {
        if (arg === null) return 'null';
        if (arg === undefined) return 'undefined';

        // Handle Error objects specially
        if (arg instanceof Error) {
          errorObject = arg; // Store for Sentry
          return `Error: ${arg.message}`; // Only log message to Grafana, not stack
        }

        // Handle primitive types
        if (typeof arg !== 'object') {
          return String(arg);
        }

        // For objects, use JSON.stringify with safety measures
        const seen = new WeakSet();
        return JSON.stringify(
          arg,
          (key, value) => {
            // Handle circular references
            if (typeof value === 'object' && value !== null) {
              if (seen.has(value)) {
                return '[Circular]';
              }
              seen.add(value);
            }

            // Redact potential sensitive fields
            if (
              key.toLowerCase().includes('key') ||
              key.toLowerCase().includes('secret') ||
              key.toLowerCase().includes('password') ||
              key.toLowerCase().includes('token')
            ) {
              return '[REDACTED]';
            }

            return value;
          },
          2,
        ).substring(0, 1000); // Limit length to prevent huge logs
      } catch {
        // If all else fails, just return a safe string
        return `[Object: ${arg?.constructor?.name || 'Unknown'}]`;
      }
    };

    const message = args.map(formatArg).join(' ');

    // Send error to Sentry if we have one
    // Only if Sentry is available (not in all Web Worker contexts)
    if (errorObject && level === DebugLevel.Error && typeof Sentry !== 'undefined') {
      // Check if we should send to Sentry
      const shouldSendToSentry =
        !isDevelopment ||
        shouldForceExternal ||
        (typeof process !== 'undefined' && process.env?.NEXT_PUBLIC_FORCE_SENTRY === '1');

      if (shouldSendToSentry) {
        Sentry.captureException(errorObject, {
          tags: {
            module: prefix,
            severity: 'error',
          },
          extra: {
            message,
            debugLevel: level,
          },
        });
      }
    }

    // Always log message to Grafana (without stack trace)
    if (message.length > 0) {
      grafanaLog(message, logSeverity, `[${prefix}] `);
    }
  };
}

// Re-export DebugLevel for convenience
export { DebugLevel as LogLevel };
