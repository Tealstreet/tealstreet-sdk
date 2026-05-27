import { getDefaultStore } from 'jotai';
import slice from 'lodash/slice';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// Import the functions to test after mocking
import { grafanaLog, log } from './log';
import { logAtom } from './logAtom';
import { LogSeverity } from './LogSeverity';
import { logError, logInfo, logWarn } from './utils';

// Mock jotai
vi.mock('jotai', async (importOriginal) => {
  const actual = await importOriginal<typeof import('jotai')>();
  return {
    ...actual,
    atom: vi.fn(() => ({ toString: () => 'mockAtom' })),
    getDefaultStore: vi.fn(() => ({
      get: vi.fn(),
      set: vi.fn(),
    })),
  };
});

// Mock lodash/slice
vi.mock('lodash/slice', () => ({
  default: vi.fn(),
}));

// Mock logAtom
vi.mock('./logAtom', () => ({
  logAtom: { key: 'logAtom' },
}));

// Mock utils
vi.mock('./utils', () => ({
  logError: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
}));

describe('log.ts', () => {
  let mockStore: any;
  const mockGetDefaultStore = vi.mocked(getDefaultStore);
  const mockSlice = vi.mocked(slice);
  const mockLogError = vi.mocked(logError);
  const mockLogInfo = vi.mocked(logInfo);
  const mockLogWarn = vi.mocked(logWarn);

  beforeEach(() => {
    // Reset all mocks
    vi.clearAllMocks();

    // Setup mock store
    mockStore = {
      get: vi.fn(),
      set: vi.fn(),
    };

    mockGetDefaultStore.mockReturnValue(mockStore);

    // Setup default mock returns
    mockStore.get.mockReturnValue([]);
    mockSlice.mockImplementation((arr: any, start?: number) => (arr || []).slice(start));
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  describe('log function', () => {
    it('should add log entry to atom with default severity (Info)', () => {
      const message = 'Test message';
      const mockTimestamp = 1234567890;

      vi.spyOn(Date, 'now').mockReturnValue(mockTimestamp);

      log(message);

      // Verify store.get was called
      expect(mockStore.get).toHaveBeenCalledWith(logAtom);

      // Verify slice was called with -200 to keep last 200 entries
      expect(mockSlice).toHaveBeenCalledWith(
        expect.arrayContaining([
          expect.objectContaining({
            timestamp: mockTimestamp,
            message: message,
            severity: LogSeverity.Info,
          }),
        ]),
        -200,
      );

      // Verify store.set was called
      expect(mockStore.set).toHaveBeenCalledWith(logAtom, expect.any(Array));

      // Verify logInfo was called
      expect(mockLogInfo).toHaveBeenCalledWith(message);
    });

    it('should add log entry with custom severity (Error)', () => {
      const message = 'Error message';
      const mockTimestamp = 1234567890;

      vi.spyOn(Date, 'now').mockReturnValue(mockTimestamp);

      log(message, LogSeverity.Error);

      // Verify the log entry has Error severity
      expect(mockSlice).toHaveBeenCalledWith(
        expect.arrayContaining([
          expect.objectContaining({
            timestamp: mockTimestamp,
            message: message,
            severity: LogSeverity.Error,
          }),
        ]),
        -200,
      );

      // Verify logError was called
      expect(mockLogError).toHaveBeenCalledWith(message);
    });

    it('should add log entry with custom severity (Warning)', () => {
      const message = 'Warning message';
      const mockTimestamp = 1234567890;

      vi.spyOn(Date, 'now').mockReturnValue(mockTimestamp);

      log(message, LogSeverity.Warning);

      // Verify the log entry has Warning severity
      expect(mockSlice).toHaveBeenCalledWith(
        expect.arrayContaining([
          expect.objectContaining({
            timestamp: mockTimestamp,
            message: message,
            severity: LogSeverity.Warning,
          }),
        ]),
        -200,
      );

      // Verify logWarn was called
      expect(mockLogWarn).toHaveBeenCalledWith(message);
    });

    it('should handle prefix correctly', () => {
      const message = 'Test message';
      const prefix = '[PREFIX] ';
      const expectedMessage = `${prefix}${message}`;
      const mockTimestamp = 1234567890;

      vi.spyOn(Date, 'now').mockReturnValue(mockTimestamp);

      log(message, LogSeverity.Info, prefix);

      // Verify the log entry has prefixed message
      expect(mockSlice).toHaveBeenCalledWith(
        expect.arrayContaining([
          expect.objectContaining({
            timestamp: mockTimestamp,
            message: expectedMessage,
            severity: LogSeverity.Info,
          }),
        ]),
        -200,
      );

      // Verify logInfo was called with prefixed message
      expect(mockLogInfo).toHaveBeenCalledWith(expectedMessage);
    });

    it('should maintain log history with slice keeping last 200 entries', () => {
      const existingLogs = Array.from({ length: 200 }, (_, i) => ({
        timestamp: i,
        message: `Message ${i}`,
        severity: LogSeverity.Info,
      }));

      mockStore.get.mockReturnValue(existingLogs);
      mockSlice.mockReturnValue(existingLogs.slice(-200));

      log('New message');

      // Verify slice was called with the concatenated array
      expect(mockSlice).toHaveBeenCalledWith(
        expect.arrayContaining([
          ...existingLogs,
          expect.objectContaining({
            message: 'New message',
            severity: LogSeverity.Info,
          }),
        ]),
        -200,
      );
    });

    it('should handle errors in utils functions gracefully', () => {
      const message = 'Test message';

      // Mock logInfo to throw an error
      mockLogInfo.mockImplementation(() => {
        throw new Error('Utils function error');
      });

      // Should not throw
      expect(() => log(message, LogSeverity.Info)).not.toThrow();

      // Verify the log entry was still added to the atom
      expect(mockStore.set).toHaveBeenCalledWith(logAtom, expect.any(Array));
    });

    it('should handle errors in utils functions for Error severity', () => {
      const message = 'Error message';

      // Mock logError to throw an error
      mockLogError.mockImplementation(() => {
        throw new Error('Utils function error');
      });

      // Should not throw
      expect(() => log(message, LogSeverity.Error)).not.toThrow();

      // Verify the log entry was still added to the atom
      expect(mockStore.set).toHaveBeenCalledWith(logAtom, expect.any(Array));
    });

    it('should handle errors in utils functions for Warning severity', () => {
      const message = 'Warning message';

      // Mock logWarn to throw an error
      mockLogWarn.mockImplementation(() => {
        throw new Error('Utils function error');
      });

      // Should not throw
      expect(() => log(message, LogSeverity.Warning)).not.toThrow();

      // Verify the log entry was still added to the atom
      expect(mockStore.set).toHaveBeenCalledWith(logAtom, expect.any(Array));
    });

    it('should not call utils functions for unsupported severity levels', () => {
      const message = 'Debug message';
      // Cast to LogSeverity to test edge case
      const debugSeverity = 'debug' as LogSeverity;

      log(message, debugSeverity);

      // Verify none of the utils functions were called
      expect(mockLogError).not.toHaveBeenCalled();
      expect(mockLogWarn).not.toHaveBeenCalled();
      expect(mockLogInfo).not.toHaveBeenCalled();

      // But the log entry should still be added to the atom
      expect(mockStore.set).toHaveBeenCalledWith(logAtom, expect.any(Array));
    });
  });

  describe('grafanaLog function', () => {
    it('should call logInfo for Info severity', () => {
      const message = 'Info message';

      grafanaLog(message, LogSeverity.Info);

      expect(mockLogInfo).toHaveBeenCalledWith(message);
      expect(mockLogError).not.toHaveBeenCalled();
      expect(mockLogWarn).not.toHaveBeenCalled();
    });

    it('should call logError for Error severity', () => {
      const message = 'Error message';

      grafanaLog(message, LogSeverity.Error);

      expect(mockLogError).toHaveBeenCalledWith(message);
      expect(mockLogInfo).not.toHaveBeenCalled();
      expect(mockLogWarn).not.toHaveBeenCalled();
    });

    it('should call logWarn for Warning severity', () => {
      const message = 'Warning message';

      grafanaLog(message, LogSeverity.Warning);

      expect(mockLogWarn).toHaveBeenCalledWith(message);
      expect(mockLogError).not.toHaveBeenCalled();
      expect(mockLogInfo).not.toHaveBeenCalled();
    });

    it('should use default Info severity when no severity provided', () => {
      const message = 'Default severity message';

      grafanaLog(message);

      expect(mockLogInfo).toHaveBeenCalledWith(message);
    });

    it('should handle prefix correctly', () => {
      const message = 'Test message';
      const prefix = '[GRAFANA] ';
      const expectedMessage = `${prefix}${message}`;

      grafanaLog(message, LogSeverity.Info, prefix);

      expect(mockLogInfo).toHaveBeenCalledWith(expectedMessage);
    });

    it('should handle errors in utils functions gracefully', () => {
      const message = 'Test message';

      // Mock logInfo to throw an error
      mockLogInfo.mockImplementation(() => {
        throw new Error('Utils function error');
      });

      // Should not throw
      expect(() => grafanaLog(message, LogSeverity.Info)).not.toThrow();
    });

    it('should handle errors in logError gracefully', () => {
      const message = 'Error message';

      // Mock logError to throw an error
      mockLogError.mockImplementation(() => {
        throw new Error('Utils function error');
      });

      // Should not throw
      expect(() => grafanaLog(message, LogSeverity.Error)).not.toThrow();
    });

    it('should handle errors in logWarn gracefully', () => {
      const message = 'Warning message';

      // Mock logWarn to throw an error
      mockLogWarn.mockImplementation(() => {
        throw new Error('Utils function error');
      });

      // Should not throw
      expect(() => grafanaLog(message, LogSeverity.Warning)).not.toThrow();
    });

    it('should not call utils functions for unsupported severity levels', () => {
      const message = 'Debug message';
      // Cast to LogSeverity to test edge case
      const debugSeverity = 'debug' as LogSeverity;

      grafanaLog(message, debugSeverity);

      // Verify none of the utils functions were called
      expect(mockLogError).not.toHaveBeenCalled();
      expect(mockLogWarn).not.toHaveBeenCalled();
      expect(mockLogInfo).not.toHaveBeenCalled();
    });

    it('should not affect atom state (only calls utils functions)', () => {
      const message = 'Grafana only message';

      grafanaLog(message, LogSeverity.Info);

      // Verify atom operations were not called
      expect(mockStore.get).not.toHaveBeenCalled();
      expect(mockStore.set).not.toHaveBeenCalled();
      expect(mockSlice).not.toHaveBeenCalled();

      // But utils function should be called
      expect(mockLogInfo).toHaveBeenCalledWith(message);
    });
  });

  describe('Integration scenarios', () => {
    it('should handle complex log entry with existing history', () => {
      const existingLogs = [
        { timestamp: 1000, message: 'Old log 1', severity: LogSeverity.Info },
        { timestamp: 2000, message: 'Old log 2', severity: LogSeverity.Error },
      ];
      const newMessage = 'New complex log';
      const prefix = '[COMPLEX] ';
      const mockTimestamp = 3000;

      mockStore.get.mockReturnValue(existingLogs);
      vi.spyOn(Date, 'now').mockReturnValue(mockTimestamp);

      log(newMessage, LogSeverity.Warning, prefix);

      // Verify the new log was added to existing logs
      expect(mockSlice).toHaveBeenCalledWith(
        [
          ...existingLogs,
          {
            timestamp: mockTimestamp,
            message: `${prefix}${newMessage}`,
            severity: LogSeverity.Warning,
          },
        ],
        -200,
      );

      // Verify proper utils function was called
      expect(mockLogWarn).toHaveBeenCalledWith(`${prefix}${newMessage}`);
    });

    it('should handle empty prefix correctly', () => {
      const message = 'No prefix message';
      const emptyPrefix = '';

      log(message, LogSeverity.Info, emptyPrefix);

      expect(mockLogInfo).toHaveBeenCalledWith(message);
      expect(mockSlice).toHaveBeenCalledWith(
        expect.arrayContaining([
          expect.objectContaining({
            message: message,
          }),
        ]),
        -200,
      );
    });

    it('should maintain function isolation between log and grafanaLog', () => {
      const message = 'Isolation test';

      // Call log function
      log(message, LogSeverity.Info);

      // Verify atom operations were called
      expect(mockStore.get).toHaveBeenCalledTimes(1);
      expect(mockStore.set).toHaveBeenCalledTimes(1);

      // Reset mocks
      vi.clearAllMocks();

      // Call grafanaLog function
      grafanaLog(message, LogSeverity.Info);

      // Verify atom operations were NOT called for grafanaLog
      expect(mockStore.get).not.toHaveBeenCalled();
      expect(mockStore.set).not.toHaveBeenCalled();

      // But utils function should still be called
      expect(mockLogInfo).toHaveBeenCalledWith(message);
    });
  });

  describe('Edge cases and error handling', () => {
    it('should handle store.get returning undefined', () => {
      mockStore.get.mockReturnValue(undefined);

      const message = 'Test with undefined logs';

      // Should throw because the actual implementation doesn't handle undefined
      // The log function calls .concat() directly on the result of store.get()
      expect(() => log(message)).toThrow("Cannot read properties of undefined (reading 'concat')");
    });

    it('should handle store operations throwing errors', () => {
      mockStore.get.mockImplementation(() => {
        throw new Error('Store get error');
      });

      const message = 'Test with store error';

      // Should throw since store operations are not wrapped in try-catch
      expect(() => log(message)).toThrow('Store get error');
    });

    it('should handle slice function throwing errors', () => {
      mockSlice.mockImplementation(() => {
        throw new Error('Slice error');
      });

      const message = 'Test with slice error';

      // Should throw since slice is not wrapped in try-catch
      expect(() => log(message)).toThrow('Slice error');
    });

    it('should verify Date.now() is called for timestamp', () => {
      const mockTimestamp = 9876543210;
      const dateSpy = vi.spyOn(Date, 'now').mockReturnValue(mockTimestamp);

      log('Timestamp test');

      expect(dateSpy).toHaveBeenCalled();
      expect(mockSlice).toHaveBeenCalledWith(
        expect.arrayContaining([
          expect.objectContaining({
            timestamp: mockTimestamp,
          }),
        ]),
        -200,
      );

      dateSpy.mockRestore();
    });
  });
});
