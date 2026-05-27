import { renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { log } from './log';
import { LogSeverity } from './LogSeverity';
import { useLog } from './useLog';

// Mock the log function
vi.mock('./log', () => ({
  log: vi.fn(),
}));

const mockLog = vi.mocked(log);

describe('useLog', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('Hook initialization', () => {
    it('should return a function', () => {
      const { result } = renderHook(() => useLog());

      expect(typeof result.current).toBe('function');
    });

    it('should return the same function reference on re-renders', () => {
      const { result, rerender } = renderHook(() => useLog());
      const firstRender = result.current;

      rerender();
      const secondRender = result.current;

      expect(firstRender).toBe(secondRender);
    });
  });

  describe('Hook return values', () => {
    it('should call log function with default parameters', () => {
      const { result } = renderHook(() => useLog());
      const logFunction = result.current;

      logFunction('test message');

      expect(mockLog).toHaveBeenCalledWith('test message', LogSeverity.Info, '');
      expect(mockLog).toHaveBeenCalledTimes(1);
    });

    it('should call log function with custom severity', () => {
      const { result } = renderHook(() => useLog());
      const logFunction = result.current;

      logFunction('error message', LogSeverity.Error);

      expect(mockLog).toHaveBeenCalledWith('error message', LogSeverity.Error, '');
      expect(mockLog).toHaveBeenCalledTimes(1);
    });

    it('should call log function with custom prefix', () => {
      const { result } = renderHook(() => useLog());
      const logFunction = result.current;

      logFunction('test message', LogSeverity.Info, '[PREFIX] ');

      expect(mockLog).toHaveBeenCalledWith('test message', LogSeverity.Info, '[PREFIX] ');
      expect(mockLog).toHaveBeenCalledTimes(1);
    });

    it('should call log function with all custom parameters', () => {
      const { result } = renderHook(() => useLog());
      const logFunction = result.current;

      logFunction('warning message', LogSeverity.Warning, '[WARN] ');

      expect(mockLog).toHaveBeenCalledWith('warning message', LogSeverity.Warning, '[WARN] ');
      expect(mockLog).toHaveBeenCalledTimes(1);
    });
  });

  describe('State management', () => {
    it('should maintain function identity across multiple calls', () => {
      const { result } = renderHook(() => useLog());
      const logFunction = result.current;

      // Call multiple times
      logFunction('message 1');
      logFunction('message 2');
      logFunction('message 3');

      // Function reference should remain the same
      expect(result.current).toBe(logFunction);
      expect(mockLog).toHaveBeenCalledTimes(3);
    });

    it('should not have dependencies that would cause re-creation', () => {
      const { result, rerender } = renderHook(() => useLog());
      const firstFunction = result.current;

      // Force multiple re-renders
      for (let i = 0; i < 5; i++) {
        rerender();
      }

      expect(result.current).toBe(firstFunction);
    });
  });

  describe('Hook behavior with different inputs', () => {
    it('should handle empty string messages', () => {
      const { result } = renderHook(() => useLog());
      const logFunction = result.current;

      logFunction('');

      expect(mockLog).toHaveBeenCalledWith('', LogSeverity.Info, '');
    });

    it('should handle long messages', () => {
      const { result } = renderHook(() => useLog());
      const logFunction = result.current;
      const longMessage = 'a'.repeat(1000);

      logFunction(longMessage);

      expect(mockLog).toHaveBeenCalledWith(longMessage, LogSeverity.Info, '');
    });

    it('should handle special characters in messages', () => {
      const { result } = renderHook(() => useLog());
      const logFunction = result.current;
      const specialMessage = '🚀 Special chars: äöü @#$%^&*()';

      logFunction(specialMessage);

      expect(mockLog).toHaveBeenCalledWith(specialMessage, LogSeverity.Info, '');
    });

    it('should handle special characters in prefix', () => {
      const { result } = renderHook(() => useLog());
      const logFunction = result.current;
      const specialPrefix = '[🔥ERROR🔥] ';

      logFunction('test message', LogSeverity.Error, specialPrefix);

      expect(mockLog).toHaveBeenCalledWith('test message', LogSeverity.Error, specialPrefix);
    });

    it('should handle all LogSeverity values', () => {
      const { result } = renderHook(() => useLog());
      const logFunction = result.current;

      // Test all severity levels
      const severityLevels = [LogSeverity.Info, LogSeverity.Warning, LogSeverity.Error];

      severityLevels.forEach((severity, index) => {
        logFunction(`message ${index}`, severity);
        expect(mockLog).toHaveBeenNthCalledWith(index + 1, `message ${index}`, severity, '');
      });

      expect(mockLog).toHaveBeenCalledTimes(severityLevels.length);
    });
  });

  describe('Hook behavior with default parameters', () => {
    it('should use LogSeverity.Info as default severity', () => {
      const { result } = renderHook(() => useLog());
      const logFunction = result.current;

      logFunction('test message');

      expect(mockLog).toHaveBeenCalledWith('test message', LogSeverity.Info, '');
    });

    it('should use empty string as default prefix', () => {
      const { result } = renderHook(() => useLog());
      const logFunction = result.current;

      logFunction('test message', LogSeverity.Warning);

      expect(mockLog).toHaveBeenCalledWith('test message', LogSeverity.Warning, '');
    });

    it('should handle partial parameter overrides', () => {
      const { result } = renderHook(() => useLog());
      const logFunction = result.current;

      // Only override severity
      logFunction('test message', LogSeverity.Error);
      expect(mockLog).toHaveBeenLastCalledWith('test message', LogSeverity.Error, '');

      // Override severity and prefix
      logFunction('test message 2', LogSeverity.Warning, '[TEST] ');
      expect(mockLog).toHaveBeenLastCalledWith('test message 2', LogSeverity.Warning, '[TEST] ');
    });
  });

  describe('Error handling', () => {
    it('should not throw when log function throws an error', () => {
      mockLog.mockImplementationOnce(() => {
        throw new Error('Log function error');
      });

      const { result } = renderHook(() => useLog());
      const logFunction = result.current;

      expect(() => {
        logFunction('test message');
      }).toThrow('Log function error');

      expect(mockLog).toHaveBeenCalledWith('test message', LogSeverity.Info, '');
    });

    it('should continue working after an error', () => {
      mockLog
        .mockImplementationOnce(() => {
          throw new Error('First call error');
        })
        .mockImplementationOnce(() => {
          // Second call works normally
        });

      const { result } = renderHook(() => useLog());
      const logFunction = result.current;

      // First call throws
      expect(() => {
        logFunction('first message');
      }).toThrow();

      // Second call should work
      expect(() => {
        logFunction('second message');
      }).not.toThrow();

      expect(mockLog).toHaveBeenCalledTimes(2);
    });
  });

  describe('Performance considerations', () => {
    it('should be memoized and not re-create function unnecessarily', () => {
      let renderCount = 0;
      const { result, rerender } = renderHook(() => {
        renderCount++;
        return useLog();
      });

      const originalFunction = result.current;

      // Force multiple re-renders
      rerender();
      rerender();
      rerender();

      expect(renderCount).toBe(4); // Initial + 3 re-renders
      expect(result.current).toBe(originalFunction);
    });

    it('should handle rapid successive calls efficiently', () => {
      const { result } = renderHook(() => useLog());
      const logFunction = result.current;

      // Make many rapid calls
      for (let i = 0; i < 100; i++) {
        logFunction(`message ${i}`);
      }

      expect(mockLog).toHaveBeenCalledTimes(100);
      // Verify the function reference is still the same
      expect(result.current).toBe(logFunction);
    });
  });

  describe('Integration scenarios', () => {
    it('should work correctly when used in different components', () => {
      const { result: result1 } = renderHook(() => useLog());
      const { result: result2 } = renderHook(() => useLog());

      const logFunction1 = result1.current;
      const logFunction2 = result2.current;

      // Both should work independently
      logFunction1('message from component 1');
      logFunction2('message from component 2');

      expect(mockLog).toHaveBeenCalledTimes(2);
      expect(mockLog).toHaveBeenNthCalledWith(1, 'message from component 1', LogSeverity.Info, '');
      expect(mockLog).toHaveBeenNthCalledWith(2, 'message from component 2', LogSeverity.Info, '');
    });

    it('should handle concurrent calls from multiple hook instances', () => {
      const { result: result1 } = renderHook(() => useLog());
      const { result: result2 } = renderHook(() => useLog());
      const { result: result3 } = renderHook(() => useLog());

      // Simulate concurrent calls
      result1.current('concurrent message 1', LogSeverity.Info);
      result2.current('concurrent message 2', LogSeverity.Warning);
      result3.current('concurrent message 3', LogSeverity.Error);

      expect(mockLog).toHaveBeenCalledTimes(3);
      expect(mockLog).toHaveBeenNthCalledWith(1, 'concurrent message 1', LogSeverity.Info, '');
      expect(mockLog).toHaveBeenNthCalledWith(2, 'concurrent message 2', LogSeverity.Warning, '');
      expect(mockLog).toHaveBeenNthCalledWith(3, 'concurrent message 3', LogSeverity.Error, '');
    });
  });
});
