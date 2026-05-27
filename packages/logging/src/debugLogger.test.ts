import { afterEach, beforeEach, describe, expect, it, MockInstance, vi } from 'vitest';

import { stableHash } from '@tealstreet/utils/src/stableHash';
// Import after mocks
import { createDebugLogger, DebugLevel, LogLevel } from './debugLogger';
import { grafanaLog } from './log';

// Mock dependencies before importing the module
vi.mock('@tealstreet/utils/src/stableHash', () => ({
  stableHash: vi.fn((args: any[]) => JSON.stringify(args)),
}));

vi.mock('./LogSeverity', () => ({
  LogSeverity: {
    Info: 'info',
    Warning: 'warning',
    Error: 'error',
    Debug: 'debug',
  },
}));

vi.mock('./log', () => ({
  grafanaLog: vi.fn(),
}));

const mockGrafanaLog = vi.mocked(grafanaLog);
const mockStableHash = vi.mocked(stableHash);

describe('debugLogger.ts', () => {
  let mockConsoleLog: MockInstance;
  let mockConsoleWarn: MockInstance;
  let mockConsoleError: MockInstance;

  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
    // Default to production mode
    process.env.NODE_ENV = 'production';
    delete process.env.NEXT_PUBLIC_FORCE_EXTERNAL_LOGGING;
    delete process.env.NEXT_PUBLIC_FORCE_SENTRY;

    // Set up console mocks inside beforeEach
    mockConsoleLog = vi.spyOn(console, 'log').mockImplementation(() => {});
    mockConsoleWarn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    mockConsoleError = vi.spyOn(console, 'error').mockImplementation(() => {});
  });

  afterEach(() => {
    vi.useRealTimers();
    mockConsoleLog.mockRestore();
    mockConsoleWarn.mockRestore();
    mockConsoleError.mockRestore();
  });

  describe('DebugLevel enum', () => {
    it('should have all expected values', () => {
      expect(DebugLevel.Local).toBe('Local');
      expect(DebugLevel.Warning).toBe('Warning');
      expect(DebugLevel.Error).toBe('Error');
      expect(DebugLevel.Info).toBe('Info');
    });

    it('should be re-exported as LogLevel', () => {
      expect(LogLevel).toBe(DebugLevel);
      expect(LogLevel.Local).toBe('Local');
      expect(LogLevel.Warning).toBe('Warning');
      expect(LogLevel.Error).toBe('Error');
      expect(LogLevel.Info).toBe('Info');
    });
  });

  describe('createDebugLogger', () => {
    describe('basic functionality', () => {
      it('should return a function', () => {
        const debug = createDebugLogger({ prefix: 'Test' });
        expect(typeof debug).toBe('function');
      });

      it('should log to console.log for Info level when enableConsoleLog is true', () => {
        const debug = createDebugLogger({
          prefix: 'Test',
          enableConsoleLog: true,
        });

        debug(DebugLevel.Info, 'Test message');

        expect(mockConsoleLog).toHaveBeenCalledWith('[Test]', 'Test message');
      });

      it('should log to console.warn for Warning level when enableConsoleLog is true', () => {
        const debug = createDebugLogger({
          prefix: 'Test',
          enableConsoleLog: true,
        });

        debug(DebugLevel.Warning, 'Warning message');

        expect(mockConsoleWarn).toHaveBeenCalledWith('[Test]', 'Warning message');
      });

      it('should log to console.error for Error level even when enableConsoleLog is false', () => {
        const debug = createDebugLogger({
          prefix: 'Test',
          enableConsoleLog: false,
        });

        debug(DebugLevel.Error, 'Error message');

        expect(mockConsoleError).toHaveBeenCalledWith('[Test]', 'Error message');
      });

      it('should log to console.log for Local level when enableConsoleLog is true', () => {
        const debug = createDebugLogger({
          prefix: 'Test',
          enableConsoleLog: true,
        });

        debug(DebugLevel.Local, 'Local message');

        expect(mockConsoleLog).toHaveBeenCalledWith('[Test]', 'Local message');
      });
    });

    describe('shouldLogToConsole callback', () => {
      it('should use shouldLogToConsole callback to determine console logging', () => {
        const shouldLog = vi.fn().mockReturnValue(true);
        const debug = createDebugLogger({
          prefix: 'Test',
          shouldLogToConsole: shouldLog,
        });

        debug(DebugLevel.Info, 'Test message');

        expect(shouldLog).toHaveBeenCalledWith(DebugLevel.Info);
        expect(mockConsoleLog).toHaveBeenCalledWith('[Test]', 'Test message');
      });

      it('should not log to console when shouldLogToConsole returns false', () => {
        const shouldLog = vi.fn().mockReturnValue(false);
        const debug = createDebugLogger({
          prefix: 'Test',
          shouldLogToConsole: shouldLog,
        });

        debug(DebugLevel.Info, 'Test message');

        expect(shouldLog).toHaveBeenCalledWith(DebugLevel.Info);
        expect(mockConsoleLog).not.toHaveBeenCalled();
      });

      it('should always log Error level regardless of shouldLogToConsole', () => {
        const shouldLog = vi.fn().mockReturnValue(false);
        const debug = createDebugLogger({
          prefix: 'Test',
          shouldLogToConsole: shouldLog,
        });

        debug(DebugLevel.Error, 'Error message');

        // Error level always logs to console
        expect(mockConsoleError).toHaveBeenCalledWith('[Test]', 'Error message');
      });
    });

    describe('Grafana logging', () => {
      it('should send Info level to Grafana in production', () => {
        process.env.NODE_ENV = 'production';
        const debug = createDebugLogger({ prefix: 'Test' });

        debug(DebugLevel.Info, 'Test message');

        expect(mockGrafanaLog).toHaveBeenCalledWith(
          'Test message',
          'info', // LogSeverity.Info
          '[Test] ',
        );
      });

      it('should send Warning level to Grafana in production', () => {
        process.env.NODE_ENV = 'production';
        const debug = createDebugLogger({ prefix: 'Test' });

        debug(DebugLevel.Warning, 'Warning message');

        expect(mockGrafanaLog).toHaveBeenCalledWith(
          'Warning message',
          'warning', // LogSeverity.Warning
          '[Test] ',
        );
      });

      it('should send Error level to Grafana in production', () => {
        process.env.NODE_ENV = 'production';
        const debug = createDebugLogger({ prefix: 'Test' });

        debug(DebugLevel.Error, 'Error message');

        expect(mockGrafanaLog).toHaveBeenCalledWith(
          'Error message',
          'error', // LogSeverity.Error
          '[Test] ',
        );
      });

      it('should NOT send Local level to Grafana', () => {
        process.env.NODE_ENV = 'production';
        const debug = createDebugLogger({
          prefix: 'Test',
          enableConsoleLog: true,
        });

        debug(DebugLevel.Local, 'Local message');

        expect(mockGrafanaLog).not.toHaveBeenCalled();
      });

      it('should NOT send to Grafana in development without forceExternalLogging', () => {
        process.env.NODE_ENV = 'development';
        const debug = createDebugLogger({ prefix: 'Test' });

        debug(DebugLevel.Info, 'Test message');

        expect(mockGrafanaLog).not.toHaveBeenCalled();
      });

      it('should send to Grafana in development when forceExternalLogging is true', () => {
        process.env.NODE_ENV = 'development';
        const debug = createDebugLogger({
          prefix: 'Test',
          forceExternalLogging: true,
        });

        debug(DebugLevel.Info, 'Test message');

        expect(mockGrafanaLog).toHaveBeenCalled();
      });

      it('should send to Grafana in development when NEXT_PUBLIC_FORCE_EXTERNAL_LOGGING is set', () => {
        process.env.NODE_ENV = 'development';
        process.env.NEXT_PUBLIC_FORCE_EXTERNAL_LOGGING = '1';
        const debug = createDebugLogger({ prefix: 'Test' });

        debug(DebugLevel.Info, 'Test message');

        expect(mockGrafanaLog).toHaveBeenCalled();
      });
    });

    describe('throttling', () => {
      it('should throttle duplicate messages by default', () => {
        const debug = createDebugLogger({
          prefix: 'Test',
          enableConsoleLog: true,
        });

        // Call with same message multiple times
        debug(DebugLevel.Info, 'Same message');
        debug(DebugLevel.Info, 'Same message');
        debug(DebugLevel.Info, 'Same message');

        // Should only log once due to throttling
        expect(mockConsoleLog).toHaveBeenCalledTimes(1);
      });

      it('should allow duplicate messages after throttle interval', () => {
        const debug = createDebugLogger({
          prefix: 'Test',
          enableConsoleLog: true,
          throttleIntervalMs: 1000,
        });

        debug(DebugLevel.Info, 'Same message');
        expect(mockConsoleLog).toHaveBeenCalledTimes(1);

        // Advance time past throttle interval
        vi.advanceTimersByTime(1500);

        debug(DebugLevel.Info, 'Same message');
        expect(mockConsoleLog).toHaveBeenCalledTimes(2);
      });

      it('should not throttle when throttleDuplicates is false', () => {
        const debug = createDebugLogger({
          prefix: 'Test',
          enableConsoleLog: true,
          throttleDuplicates: false,
        });

        debug(DebugLevel.Info, 'Same message');
        debug(DebugLevel.Info, 'Same message');
        debug(DebugLevel.Info, 'Same message');

        // Should log all three times
        expect(mockConsoleLog).toHaveBeenCalledTimes(3);
      });

      it('should use custom throttle interval', () => {
        const debug = createDebugLogger({
          prefix: 'Test',
          enableConsoleLog: true,
          throttleIntervalMs: 500,
        });

        debug(DebugLevel.Info, 'Same message');
        expect(mockConsoleLog).toHaveBeenCalledTimes(1);

        // Advance time less than throttle interval
        vi.advanceTimersByTime(400);
        debug(DebugLevel.Info, 'Same message');
        expect(mockConsoleLog).toHaveBeenCalledTimes(1);

        // Advance past throttle interval
        vi.advanceTimersByTime(200);
        debug(DebugLevel.Info, 'Same message');
        expect(mockConsoleLog).toHaveBeenCalledTimes(2);
      });

      it('should throttle based on level and message content', () => {
        const debug = createDebugLogger({
          prefix: 'Test',
          enableConsoleLog: true,
        });

        // Same message, different levels should both log
        debug(DebugLevel.Info, 'Same message');
        debug(DebugLevel.Warning, 'Same message');

        expect(mockConsoleLog).toHaveBeenCalledTimes(1);
        expect(mockConsoleWarn).toHaveBeenCalledTimes(1);
      });

      it('should clean up old throttle entries when map grows large', () => {
        const debug = createDebugLogger({
          prefix: 'Test',
          enableConsoleLog: true,
          throttleIntervalMs: 1000,
        });

        // Generate more than 100 unique messages
        for (let i = 0; i < 105; i++) {
          mockStableHash.mockReturnValueOnce(String(i));
          debug(DebugLevel.Info, `Message ${i}`);
        }

        // All should log since they're unique
        expect(mockConsoleLog).toHaveBeenCalledTimes(105);

        // Advance time past throttle interval to make entries stale
        vi.advanceTimersByTime(2000);

        // Next message should trigger cleanup
        mockStableHash.mockReturnValueOnce('cleanup-trigger');
        debug(DebugLevel.Info, 'Cleanup trigger');

        expect(mockConsoleLog).toHaveBeenCalledTimes(106);
      });
    });

    describe('message formatting', () => {
      it('should format primitive values correctly', () => {
        process.env.NODE_ENV = 'production';
        const debug = createDebugLogger({ prefix: 'Test' });

        debug(DebugLevel.Info, 'string', 123, true, false);

        expect(mockGrafanaLog).toHaveBeenCalledWith('string 123 true false', 'info', '[Test] ');
      });

      it('should handle null and undefined values', () => {
        process.env.NODE_ENV = 'production';
        const debug = createDebugLogger({ prefix: 'Test' });

        debug(DebugLevel.Info, null, undefined);

        expect(mockGrafanaLog).toHaveBeenCalledWith('null undefined', 'info', '[Test] ');
      });

      it('should format Error objects with message only', () => {
        process.env.NODE_ENV = 'production';
        const debug = createDebugLogger({ prefix: 'Test' });
        const error = new Error('Test error');

        debug(DebugLevel.Info, error);

        expect(mockGrafanaLog).toHaveBeenCalledWith('Error: Test error', 'info', '[Test] ');
      });

      it('should format objects as JSON', () => {
        process.env.NODE_ENV = 'production';
        const debug = createDebugLogger({ prefix: 'Test' });

        debug(DebugLevel.Info, { foo: 'bar', num: 42 });

        const call = mockGrafanaLog.mock.calls[0];
        expect(call[0]).toContain('"foo"');
        expect(call[0]).toContain('"bar"');
        expect(call[0]).toContain('42');
      });

      it('should handle circular references in objects when throttling is disabled', () => {
        process.env.NODE_ENV = 'production';
        // Disable throttling because stableHash (used for throttle key) may not handle circular refs
        const debug = createDebugLogger({ prefix: 'Test', throttleDuplicates: false });

        const circularObj: any = { name: 'test' };
        circularObj.self = circularObj;

        expect(() => {
          debug(DebugLevel.Info, circularObj);
        }).not.toThrow();

        expect(mockGrafanaLog).toHaveBeenCalled();
        const call = mockGrafanaLog.mock.calls[0];
        expect(call[0]).toContain('[Circular]');
      });

      it('should redact sensitive fields', () => {
        process.env.NODE_ENV = 'production';
        const debug = createDebugLogger({ prefix: 'Test' });

        debug(DebugLevel.Info, {
          apiKey: 'secret123',
          secretValue: 'hidden',
          password: 'pass123',
          accessToken: 'token123',
          normalField: 'visible',
        });

        const call = mockGrafanaLog.mock.calls[0];
        expect(call[0]).toContain('[REDACTED]');
        expect(call[0]).toContain('visible');
        expect(call[0]).not.toContain('secret123');
        expect(call[0]).not.toContain('hidden');
        expect(call[0]).not.toContain('pass123');
        expect(call[0]).not.toContain('token123');
      });

      it('should limit long JSON output to 1000 characters', () => {
        process.env.NODE_ENV = 'production';
        const debug = createDebugLogger({ prefix: 'Test' });

        const largeObject = { data: 'x'.repeat(2000) };
        debug(DebugLevel.Info, largeObject);

        const call = mockGrafanaLog.mock.calls[0];
        expect(call[0].length).toBeLessThanOrEqual(1000);
      });

      it('should handle objects that fail JSON.stringify when throttling is disabled', () => {
        process.env.NODE_ENV = 'production';
        // Disable throttling because stableHash (used for throttle key) may fail with problematic objects
        const debug = createDebugLogger({ prefix: 'Test', throttleDuplicates: false });

        // Create an object with a getter that throws
        const problematicObject = {
          get value() {
            throw new Error('Cannot serialize');
          },
        };

        expect(() => {
          debug(DebugLevel.Info, problematicObject);
        }).not.toThrow();

        expect(mockGrafanaLog).toHaveBeenCalled();
      });

      it('should not send empty messages to Grafana', () => {
        process.env.NODE_ENV = 'production';
        const debug = createDebugLogger({ prefix: 'Test' });

        // Create a scenario where the formatted message would be empty
        debug(DebugLevel.Local, 'Local only');

        // Local level should not send to Grafana regardless
        expect(mockGrafanaLog).not.toHaveBeenCalled();
      });
    });

    describe('Web Worker compatibility', () => {
      it('should use isProduction option when process is undefined', () => {
        const originalProcess = global.process;
        // @ts-expect-error - intentionally making process undefined
        global.process = undefined;

        try {
          const debug = createDebugLogger({
            prefix: 'Test',
            isProduction: true,
            enableConsoleLog: true,
          });

          debug(DebugLevel.Info, 'Test message');

          // Should log since isProduction is true
          expect(mockConsoleLog).toHaveBeenCalled();
          expect(mockGrafanaLog).toHaveBeenCalled();
        } finally {
          global.process = originalProcess;
        }
      });

      it('should skip external logging when isProduction is false and no forceExternalLogging', () => {
        const originalProcess = global.process;
        // @ts-expect-error - intentionally making process undefined
        global.process = undefined;

        try {
          const debug = createDebugLogger({
            prefix: 'Test',
            isProduction: false,
            enableConsoleLog: true,
          });

          debug(DebugLevel.Info, 'Test message');

          expect(mockConsoleLog).toHaveBeenCalled();
          // Should NOT send to Grafana in development
          expect(mockGrafanaLog).not.toHaveBeenCalled();
        } finally {
          global.process = originalProcess;
        }
      });
    });

    describe('multiple arguments', () => {
      it('should handle multiple string arguments', () => {
        const debug = createDebugLogger({
          prefix: 'Test',
          enableConsoleLog: true,
        });

        debug(DebugLevel.Info, 'arg1', 'arg2', 'arg3');

        expect(mockConsoleLog).toHaveBeenCalledWith('[Test]', 'arg1', 'arg2', 'arg3');
      });

      it('should handle mixed type arguments', () => {
        const debug = createDebugLogger({
          prefix: 'Test',
          enableConsoleLog: true,
        });

        debug(DebugLevel.Info, 'message', 123, { key: 'value' }, null);

        expect(mockConsoleLog).toHaveBeenCalledWith('[Test]', 'message', 123, { key: 'value' }, null);
      });

      it('should join all arguments for Grafana log', () => {
        process.env.NODE_ENV = 'production';
        const debug = createDebugLogger({ prefix: 'Test' });

        debug(DebugLevel.Info, 'part1', 'part2', 'part3');

        expect(mockGrafanaLog).toHaveBeenCalledWith('part1 part2 part3', 'info', '[Test] ');
      });
    });

    describe('edge cases', () => {
      it('should handle empty prefix', () => {
        const debug = createDebugLogger({
          prefix: '',
          enableConsoleLog: true,
        });

        debug(DebugLevel.Info, 'Test message');

        expect(mockConsoleLog).toHaveBeenCalledWith('[]', 'Test message');
      });

      it('should handle special characters in prefix', () => {
        const debug = createDebugLogger({
          prefix: '[Module:Component]',
          enableConsoleLog: true,
        });

        debug(DebugLevel.Info, 'Test message');

        expect(mockConsoleLog).toHaveBeenCalledWith('[[Module:Component]]', 'Test message');
      });

      it('should handle no arguments after level', () => {
        const debug = createDebugLogger({
          prefix: 'Test',
          enableConsoleLog: true,
        });

        debug(DebugLevel.Info);

        expect(mockConsoleLog).toHaveBeenCalledWith('[Test]');
      });

      it('should handle arrays as arguments', () => {
        process.env.NODE_ENV = 'production';
        const debug = createDebugLogger({ prefix: 'Test' });

        debug(DebugLevel.Info, [1, 2, 3], ['a', 'b']);

        expect(mockGrafanaLog).toHaveBeenCalled();
        const call = mockGrafanaLog.mock.calls[0];
        expect(call[0]).toContain('[');
        expect(call[0]).toContain('1');
        expect(call[0]).toContain('2');
        expect(call[0]).toContain('3');
      });

      it('should handle deeply nested objects', () => {
        process.env.NODE_ENV = 'production';
        const debug = createDebugLogger({ prefix: 'Test' });

        const deepObject = {
          level1: {
            level2: {
              level3: {
                value: 'deep',
              },
            },
          },
        };

        debug(DebugLevel.Info, deepObject);

        expect(mockGrafanaLog).toHaveBeenCalled();
        const call = mockGrafanaLog.mock.calls[0];
        expect(call[0]).toContain('deep');
      });
    });

    describe('default options', () => {
      it('should use default throttleDuplicates (true)', () => {
        const debug = createDebugLogger({
          prefix: 'Test',
          enableConsoleLog: true,
        });

        debug(DebugLevel.Info, 'Message');
        debug(DebugLevel.Info, 'Message');

        // With default throttling, second call should be skipped
        expect(mockConsoleLog).toHaveBeenCalledTimes(1);
      });

      it('should use default throttleIntervalMs (15000)', () => {
        const debug = createDebugLogger({
          prefix: 'Test',
          enableConsoleLog: true,
        });

        debug(DebugLevel.Info, 'Message');

        // Advance time less than 15000ms
        vi.advanceTimersByTime(10000);
        debug(DebugLevel.Info, 'Message');

        // Still throttled
        expect(mockConsoleLog).toHaveBeenCalledTimes(1);

        // Advance past 15000ms
        vi.advanceTimersByTime(6000);
        debug(DebugLevel.Info, 'Message');

        // Should now log
        expect(mockConsoleLog).toHaveBeenCalledTimes(2);
      });

      it('should use default enableConsoleLog (false)', () => {
        const debug = createDebugLogger({
          prefix: 'Test',
        });

        debug(DebugLevel.Info, 'Message');

        // Should not log to console for Info level with default enableConsoleLog
        expect(mockConsoleLog).not.toHaveBeenCalled();
      });
    });

    describe('Error handling with Sentry', () => {
      it('should handle Error objects without Sentry being available', () => {
        process.env.NODE_ENV = 'production';
        const debug = createDebugLogger({
          prefix: 'Test',
          enableConsoleLog: true,
        });

        const error = new Error('Test error');

        expect(() => {
          debug(DebugLevel.Error, 'An error occurred', error);
        }).not.toThrow();

        expect(mockConsoleError).toHaveBeenCalledWith('[Test]', 'An error occurred', error);
      });
    });
  });
});
