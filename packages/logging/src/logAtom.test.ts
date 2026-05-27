import { getDefaultStore } from 'jotai';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { logAtom } from './logAtom';
import { LogSeverity } from './LogSeverity';

describe('logAtom', () => {
  let store: ReturnType<typeof getDefaultStore>;

  beforeEach(() => {
    store = getDefaultStore();
    // Reset the atom to its initial state for each test
    store.set(logAtom, []);
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  describe('atom initialization', () => {
    it('should initialize with an empty array', () => {
      const initialValue = store.get(logAtom);
      expect(initialValue).toEqual([]);
      expect(Array.isArray(initialValue)).toBe(true);
      expect(initialValue.length).toBe(0);
    });
  });

  describe('atom type definitions', () => {
    it('should accept log entries with correct structure', () => {
      const logEntry = {
        timestamp: Date.now(),
        message: 'Test log message',
        severity: LogSeverity.Info,
      };

      store.set(logAtom, [logEntry]);
      const value = store.get(logAtom);

      expect(value).toHaveLength(1);
      expect(value[0]).toEqual(logEntry);
      expect(typeof value[0].timestamp).toBe('number');
      expect(typeof value[0].message).toBe('string');
      expect(Object.values(LogSeverity)).toContain(value[0].severity);
    });

    it('should handle multiple log entries', () => {
      const logEntries = [
        {
          timestamp: Date.now(),
          message: 'Info message',
          severity: LogSeverity.Info,
        },
        {
          timestamp: Date.now() + 1000,
          message: 'Warning message',
          severity: LogSeverity.Warning,
        },
        {
          timestamp: Date.now() + 2000,
          message: 'Error message',
          severity: LogSeverity.Error,
        },
      ];

      store.set(logAtom, logEntries);
      const value = store.get(logAtom);

      expect(value).toHaveLength(3);
      expect(value).toEqual(logEntries);
    });

    it('should accept all LogSeverity enum values', () => {
      const logEntries = [
        {
          timestamp: Date.now(),
          message: 'Info test',
          severity: LogSeverity.Info,
        },
        {
          timestamp: Date.now(),
          message: 'Warning test',
          severity: LogSeverity.Warning,
        },
        {
          timestamp: Date.now(),
          message: 'Error test',
          severity: LogSeverity.Error,
        },
      ];

      store.set(logAtom, logEntries);
      const value = store.get(logAtom);

      expect(value[0].severity).toBe(LogSeverity.Info);
      expect(value[1].severity).toBe(LogSeverity.Warning);
      expect(value[2].severity).toBe(LogSeverity.Error);
    });
  });

  describe('atom behavior', () => {
    it('should maintain referential integrity when reading multiple times', () => {
      const logEntry = {
        timestamp: 1234567890,
        message: 'Test message',
        severity: LogSeverity.Info,
      };

      store.set(logAtom, [logEntry]);

      const firstRead = store.get(logAtom);
      const secondRead = store.get(logAtom);

      expect(firstRead).toBe(secondRead);
      expect(firstRead[0]).toBe(secondRead[0]);
    });

    it('should allow appending new log entries', () => {
      const firstEntry = {
        timestamp: 1000,
        message: 'First message',
        severity: LogSeverity.Info,
      };

      const secondEntry = {
        timestamp: 2000,
        message: 'Second message',
        severity: LogSeverity.Warning,
      };

      // Set initial entry
      store.set(logAtom, [firstEntry]);
      expect(store.get(logAtom)).toHaveLength(1);

      // Append second entry
      const currentLogs = store.get(logAtom);
      store.set(logAtom, [...currentLogs, secondEntry]);

      const finalLogs = store.get(logAtom);
      expect(finalLogs).toHaveLength(2);
      expect(finalLogs[0]).toEqual(firstEntry);
      expect(finalLogs[1]).toEqual(secondEntry);
    });

    it('should allow clearing all log entries', () => {
      const logEntries = [
        {
          timestamp: Date.now(),
          message: 'Message 1',
          severity: LogSeverity.Info,
        },
        {
          timestamp: Date.now(),
          message: 'Message 2',
          severity: LogSeverity.Error,
        },
      ];

      store.set(logAtom, logEntries);
      expect(store.get(logAtom)).toHaveLength(2);

      // Clear logs
      store.set(logAtom, []);
      expect(store.get(logAtom)).toHaveLength(0);
      expect(store.get(logAtom)).toEqual([]);
    });

    it('should handle updates to specific log entries', () => {
      const originalEntry = {
        timestamp: 1000,
        message: 'Original message',
        severity: LogSeverity.Info,
      };

      const updatedEntry = {
        timestamp: 1000,
        message: 'Updated message',
        severity: LogSeverity.Warning,
      };

      store.set(logAtom, [originalEntry]);

      // Update the entry
      store.set(logAtom, [updatedEntry]);

      const result = store.get(logAtom);
      expect(result).toHaveLength(1);
      expect(result[0]).toEqual(updatedEntry);
      expect(result[0].message).toBe('Updated message');
      expect(result[0].severity).toBe(LogSeverity.Warning);
    });
  });

  describe('atom structure validation', () => {
    it('should be a Jotai atom', () => {
      // Check that logAtom has the basic structure of a Jotai atom
      expect(logAtom).toBeDefined();
      expect(typeof logAtom).toBe('object');
      expect(logAtom).toHaveProperty('read');
      expect(logAtom).toHaveProperty('write');
    });

    it('should have correct initial state type', () => {
      const initialValue = store.get(logAtom);

      // Verify it's an array that can hold the expected structure
      expect(Array.isArray(initialValue)).toBe(true);

      // Test that we can add entries with the expected structure
      const testEntry = {
        timestamp: 123456789,
        message: 'Test',
        severity: LogSeverity.Info,
      };

      expect(() => {
        store.set(logAtom, [testEntry]);
      }).not.toThrow();
    });

    it('should maintain immutability when modified', () => {
      const originalArray = store.get(logAtom);

      const logEntry = {
        timestamp: Date.now(),
        message: 'Test message',
        severity: LogSeverity.Info,
      };

      store.set(logAtom, [logEntry]);
      const newArray = store.get(logAtom);

      // Original array should remain unchanged
      expect(originalArray).toEqual([]);
      expect(newArray).not.toBe(originalArray);
      expect(newArray).toEqual([logEntry]);
    });
  });

  describe('edge cases', () => {
    it('should handle empty log entries gracefully', () => {
      expect(() => {
        store.set(logAtom, []);
      }).not.toThrow();

      expect(store.get(logAtom)).toEqual([]);
    });

    it('should handle large arrays of log entries', () => {
      const largeLogArray = Array.from({ length: 1000 }, (_, index) => ({
        timestamp: Date.now() + index,
        message: `Log message ${index}`,
        severity: index % 3 === 0 ? LogSeverity.Info : index % 3 === 1 ? LogSeverity.Warning : LogSeverity.Error,
      }));

      expect(() => {
        store.set(logAtom, largeLogArray);
      }).not.toThrow();

      const result = store.get(logAtom);
      expect(result).toHaveLength(1000);
      expect(result[0].message).toBe('Log message 0');
      expect(result[999].message).toBe('Log message 999');
    });

    it('should handle log entries with edge case timestamp values', () => {
      const edgeCaseEntries = [
        {
          timestamp: 0,
          message: 'Zero timestamp',
          severity: LogSeverity.Info,
        },
        {
          timestamp: Number.MAX_SAFE_INTEGER,
          message: 'Max timestamp',
          severity: LogSeverity.Warning,
        },
        {
          timestamp: Date.now(),
          message: 'Current timestamp',
          severity: LogSeverity.Error,
        },
      ];

      expect(() => {
        store.set(logAtom, edgeCaseEntries);
      }).not.toThrow();

      const result = store.get(logAtom);
      expect(result).toHaveLength(3);
      expect(result[0].timestamp).toBe(0);
      expect(result[1].timestamp).toBe(Number.MAX_SAFE_INTEGER);
    });

    it('should handle log entries with special characters in messages', () => {
      const specialCharacterEntries = [
        {
          timestamp: Date.now(),
          message: 'Message with emoji 🚀',
          severity: LogSeverity.Info,
        },
        {
          timestamp: Date.now(),
          message: 'Message with newlines\nand tabs\t',
          severity: LogSeverity.Warning,
        },
        {
          timestamp: Date.now(),
          message: 'Message with "quotes" and \'apostrophes\'',
          severity: LogSeverity.Error,
        },
        {
          timestamp: Date.now(),
          message: '',
          severity: LogSeverity.Info,
        },
      ];

      expect(() => {
        store.set(logAtom, specialCharacterEntries);
      }).not.toThrow();

      const result = store.get(logAtom);
      expect(result).toHaveLength(4);
      expect(result[0].message).toContain('🚀');
      expect(result[1].message).toContain('\n');
      expect(result[2].message).toContain('"quotes"');
      expect(result[3].message).toBe('');
    });
  });
});
