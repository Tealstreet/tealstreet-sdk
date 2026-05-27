import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import * as readline from 'node:readline';
import { TerminalManager, getTerminalManager, ReadlineOptions, LineHandler, CloseHandler } from './TerminalManager';
import { ExecutionMode } from '../types';

// Mock readline module
vi.mock('node:readline', () => ({
  createInterface: vi.fn(),
}));

describe('TerminalManager', () => {
  let terminalManager: TerminalManager;
  let mockReadlineInterface: {
    on: ReturnType<typeof vi.fn>;
    setPrompt: ReturnType<typeof vi.fn>;
    prompt: ReturnType<typeof vi.fn>;
    pause: ReturnType<typeof vi.fn>;
    close: ReturnType<typeof vi.fn>;
    history: string[];
  };
  let readlineOptions: ReadlineOptions;

  beforeEach(() => {
    // Create mock readline interface
    mockReadlineInterface = {
      on: vi.fn().mockReturnThis(),
      setPrompt: vi.fn(),
      prompt: vi.fn(),
      pause: vi.fn(),
      close: vi.fn(),
      history: ['cmd1', 'cmd2'],
    };

    vi.mocked(readline.createInterface).mockReturnValue(mockReadlineInterface as unknown as readline.Interface);

    // Create readline options
    readlineOptions = {
      getPrompt: () => '> ',
      onLine: vi.fn(),
      onClose: vi.fn(),
      history: ['initial'],
      historySize: 500,
    };

    // Create fresh instance
    terminalManager = new TerminalManager();
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  describe('initReadline', () => {
    it('should create readline interface with provided options', () => {
      const rl = terminalManager.initReadline(readlineOptions);

      expect(readline.createInterface).toHaveBeenCalledWith({
        input: process.stdin,
        output: process.stdout,
        prompt: '> ',
        history: ['initial'],
        historySize: 500,
      });

      expect(rl).toBe(mockReadlineInterface);
    });

    it('should register line and close handlers', () => {
      terminalManager.initReadline(readlineOptions);

      expect(mockReadlineInterface.on).toHaveBeenCalledWith('line', readlineOptions.onLine);
      expect(mockReadlineInterface.on).toHaveBeenCalledWith('close', readlineOptions.onClose);
    });

    it('should use default history if not provided', () => {
      const optionsWithoutHistory: ReadlineOptions = {
        getPrompt: () => '$ ',
        onLine: vi.fn(),
        onClose: vi.fn(),
      };

      terminalManager.initReadline(optionsWithoutHistory);

      expect(readline.createInterface).toHaveBeenCalledWith(
        expect.objectContaining({
          history: [],
          historySize: 1000,
        }),
      );
    });

    it('should use default historySize if not provided', () => {
      const optionsWithoutHistorySize: ReadlineOptions = {
        getPrompt: () => '$ ',
        onLine: vi.fn(),
        onClose: vi.fn(),
        history: ['test'],
      };

      terminalManager.initReadline(optionsWithoutHistorySize);

      expect(readline.createInterface).toHaveBeenCalledWith(
        expect.objectContaining({
          historySize: 1000,
        }),
      );
    });
  });

  describe('getReadline', () => {
    it('should return null before initialization', () => {
      expect(terminalManager.getReadline()).toBeNull();
    });

    it('should return readline instance after initialization', () => {
      terminalManager.initReadline(readlineOptions);
      expect(terminalManager.getReadline()).toBe(mockReadlineInterface);
    });
  });

  describe('isSuspended', () => {
    it('should return false by default', () => {
      expect(terminalManager.isSuspended).toBe(false);
    });

    it('should return true after entering suspend mode', () => {
      terminalManager.initReadline(readlineOptions);
      terminalManager.enterSuspendMode();

      expect(terminalManager.isSuspended).toBe(true);
    });

    it('should return false after exiting suspend mode', () => {
      terminalManager.initReadline(readlineOptions);
      terminalManager.enterSuspendMode();
      terminalManager.exitSuspendMode();

      expect(terminalManager.isSuspended).toBe(false);
    });
  });

  describe('enterSuspendMode', () => {
    it('should set suspend mode to true', () => {
      terminalManager.initReadline(readlineOptions);
      terminalManager.enterSuspendMode();

      expect(terminalManager.isSuspended).toBe(true);
    });

    it('should pause and close readline', () => {
      terminalManager.initReadline(readlineOptions);
      terminalManager.enterSuspendMode();

      expect(mockReadlineInterface.pause).toHaveBeenCalled();
      expect(mockReadlineInterface.close).toHaveBeenCalled();
    });

    it('should set readline to null after closing', () => {
      terminalManager.initReadline(readlineOptions);
      terminalManager.enterSuspendMode();

      expect(terminalManager.getReadline()).toBeNull();
    });

    it('should not do anything if already in suspend mode', () => {
      terminalManager.initReadline(readlineOptions);
      terminalManager.enterSuspendMode();

      // Reset mocks
      vi.clearAllMocks();

      // Call again
      terminalManager.enterSuspendMode();

      // Should not call pause or close again
      expect(mockReadlineInterface.pause).not.toHaveBeenCalled();
      expect(mockReadlineInterface.close).not.toHaveBeenCalled();
    });

    it('should handle case when readline is not initialized', () => {
      // Should not throw
      expect(() => terminalManager.enterSuspendMode()).not.toThrow();
      expect(terminalManager.isSuspended).toBe(true);
    });
  });

  describe('exitSuspendMode', () => {
    it('should set suspend mode to false', () => {
      terminalManager.initReadline(readlineOptions);
      terminalManager.enterSuspendMode();
      terminalManager.exitSuspendMode();

      expect(terminalManager.isSuspended).toBe(false);
    });

    it('should recreate readline interface with saved history', () => {
      terminalManager.initReadline(readlineOptions);
      terminalManager.enterSuspendMode();

      // Clear mocks to check new createInterface call
      vi.clearAllMocks();

      // Setup new mock for recreated interface
      const newMockInterface = {
        on: vi.fn().mockReturnThis(),
        setPrompt: vi.fn(),
        prompt: vi.fn(),
        pause: vi.fn(),
        close: vi.fn(),
        history: [],
      };
      vi.mocked(readline.createInterface).mockReturnValue(newMockInterface as unknown as readline.Interface);

      terminalManager.exitSuspendMode();

      expect(readline.createInterface).toHaveBeenCalledWith(
        expect.objectContaining({
          history: ['cmd1', 'cmd2'], // Saved history from mock
        }),
      );
    });

    it('should re-register event handlers', () => {
      terminalManager.initReadline(readlineOptions);
      terminalManager.enterSuspendMode();

      // Setup new mock for recreated interface
      const newMockInterface = {
        on: vi.fn().mockReturnThis(),
        setPrompt: vi.fn(),
        prompt: vi.fn(),
        pause: vi.fn(),
        close: vi.fn(),
        history: [],
      };
      vi.mocked(readline.createInterface).mockReturnValue(newMockInterface as unknown as readline.Interface);

      terminalManager.exitSuspendMode();

      expect(newMockInterface.on).toHaveBeenCalledWith('line', readlineOptions.onLine);
      expect(newMockInterface.on).toHaveBeenCalledWith('close', readlineOptions.onClose);
    });

    it('should show prompt after restoring readline', () => {
      terminalManager.initReadline(readlineOptions);
      terminalManager.enterSuspendMode();

      // Setup new mock for recreated interface
      const newMockInterface = {
        on: vi.fn().mockReturnThis(),
        setPrompt: vi.fn(),
        prompt: vi.fn(),
        pause: vi.fn(),
        close: vi.fn(),
        history: [],
      };
      vi.mocked(readline.createInterface).mockReturnValue(newMockInterface as unknown as readline.Interface);

      terminalManager.exitSuspendMode();

      expect(newMockInterface.prompt).toHaveBeenCalled();
    });

    it('should not do anything if not in suspend mode', () => {
      vi.clearAllMocks();

      terminalManager.exitSuspendMode();

      expect(readline.createInterface).not.toHaveBeenCalled();
    });

    it('should not recreate readline if options were never set', () => {
      // Create new instance without ever calling initReadline
      const freshManager = new TerminalManager();

      // Force into suspend mode directly
      (freshManager as any).isInSuspendMode = true;

      vi.clearAllMocks();
      freshManager.exitSuspendMode();

      // Should not create interface because readlineOptions is null
      expect(readline.createInterface).not.toHaveBeenCalled();
    });
  });

  describe('setupExitHandlers', () => {
    let originalStdinDescriptor: PropertyDescriptor | undefined;
    let mockStdin: {
      isTTY: boolean;
      setRawMode: ReturnType<typeof vi.fn>;
      resume: ReturnType<typeof vi.fn>;
      on: ReturnType<typeof vi.fn>;
      off: ReturnType<typeof vi.fn>;
    };

    beforeEach(() => {
      originalStdinDescriptor = Object.getOwnPropertyDescriptor(process, 'stdin');

      mockStdin = {
        isTTY: true,
        setRawMode: vi.fn(),
        resume: vi.fn(),
        on: vi.fn(),
        off: vi.fn(),
      };

      // Replace stdin with mock
      Object.defineProperty(process, 'stdin', {
        configurable: true,
        value: mockStdin,
      });
    });

    afterEach(() => {
      if (originalStdinDescriptor) {
        Object.defineProperty(process, 'stdin', originalStdinDescriptor);
      }
    });

    it('should setup handlers for REPL mode', () => {
      const callback = vi.fn();
      terminalManager.setupExitHandlers(ExecutionMode.REPL, callback);

      expect(mockStdin.setRawMode).toHaveBeenCalledWith(true);
      expect(mockStdin.resume).toHaveBeenCalled();
      expect(mockStdin.on).toHaveBeenCalledWith('data', expect.any(Function));
    });

    it('should setup SIGINT handler for CLI_ARGS mode', () => {
      const callback = vi.fn();
      const processOnSpy = vi.spyOn(process, 'on');

      terminalManager.setupExitHandlers(ExecutionMode.CLI_ARGS, callback);

      expect(processOnSpy).toHaveBeenCalledWith('SIGINT', expect.any(Function));

      processOnSpy.mockRestore();
    });

    it('should return cleanup function', () => {
      const callback = vi.fn();
      const cleanup = terminalManager.setupExitHandlers(ExecutionMode.REPL, callback);

      expect(typeof cleanup).toBe('function');
    });

    it('should cleanup handlers when cleanup function is called', () => {
      const callback = vi.fn();
      const cleanup = terminalManager.setupExitHandlers(ExecutionMode.REPL, callback);

      cleanup();

      expect(mockStdin.off).toHaveBeenCalledWith('data', expect.any(Function));
      expect(mockStdin.setRawMode).toHaveBeenCalledWith(false);
    });

    it('should call callback on Escape key (27)', () => {
      const callback = vi.fn();
      terminalManager.setupExitHandlers(ExecutionMode.REPL, callback);

      // Get the data handler - use type assertion to avoid complex typing
      const calls = mockStdin.on.mock.calls as Array<[string, (data: Buffer) => void]>;
      const dataHandler = calls.find((call) => call[0] === 'data')?.[1];

      // Simulate Escape key press
      dataHandler?.(Buffer.from([27]));

      expect(callback).toHaveBeenCalled();
    });

    it('should call callback on Ctrl+X (24)', () => {
      const callback = vi.fn();
      terminalManager.setupExitHandlers(ExecutionMode.REPL, callback);

      // Get the data handler - use type assertion to avoid complex typing
      const calls = mockStdin.on.mock.calls as Array<[string, (data: Buffer) => void]>;
      const dataHandler = calls.find((call) => call[0] === 'data')?.[1];

      // Simulate Ctrl+X key press
      dataHandler?.(Buffer.from([24]));

      expect(callback).toHaveBeenCalled();
    });

    it('should not call callback on other keys', () => {
      const callback = vi.fn();
      terminalManager.setupExitHandlers(ExecutionMode.REPL, callback);

      // Get the data handler - use type assertion to avoid complex typing
      const calls = mockStdin.on.mock.calls as Array<[string, (data: Buffer) => void]>;
      const dataHandler = calls.find((call) => call[0] === 'data')?.[1];

      // Simulate regular key press (letter 'a')
      dataHandler?.(Buffer.from([97]));

      expect(callback).not.toHaveBeenCalled();
    });

    it('should handle non-TTY stdin', () => {
      mockStdin.isTTY = false;

      const callback = vi.fn();
      // Should not throw
      expect(() => terminalManager.setupExitHandlers(ExecutionMode.REPL, callback)).not.toThrow();
    });
  });

  describe('getExitMessage', () => {
    it('should return CLI message for CLI_ARGS mode', () => {
      const message = terminalManager.getExitMessage(ExecutionMode.CLI_ARGS);
      expect(message).toBe('Press Ctrl+C or Escape to exit');
    });

    it('should return REPL message for REPL mode', () => {
      const message = terminalManager.getExitMessage(ExecutionMode.REPL);
      expect(message).toBe('Press Escape or Ctrl+X to exit');
    });
  });

  describe('setPrompt', () => {
    it('should set prompt on readline interface', () => {
      terminalManager.initReadline(readlineOptions);
      terminalManager.setPrompt('new> ');

      expect(mockReadlineInterface.setPrompt).toHaveBeenCalledWith('new> ');
    });

    it('should not throw if readline is not initialized', () => {
      expect(() => terminalManager.setPrompt('test> ')).not.toThrow();
    });
  });

  describe('prompt', () => {
    it('should show prompt on readline interface', () => {
      terminalManager.initReadline(readlineOptions);
      terminalManager.prompt();

      expect(mockReadlineInterface.prompt).toHaveBeenCalled();
    });

    it('should not show prompt if in suspend mode', () => {
      terminalManager.initReadline(readlineOptions);
      terminalManager.enterSuspendMode();

      // Clear mocks
      vi.clearAllMocks();

      terminalManager.prompt();

      expect(mockReadlineInterface.prompt).not.toHaveBeenCalled();
    });

    it('should not throw if readline is not initialized', () => {
      expect(() => terminalManager.prompt()).not.toThrow();
    });
  });

  describe('dispose', () => {
    it('should close readline interface', () => {
      terminalManager.initReadline(readlineOptions);
      terminalManager.dispose();

      expect(mockReadlineInterface.close).toHaveBeenCalled();
    });

    it('should set readline to null', () => {
      terminalManager.initReadline(readlineOptions);
      terminalManager.dispose();

      expect(terminalManager.getReadline()).toBeNull();
    });

    it('should not throw if readline is not initialized', () => {
      expect(() => terminalManager.dispose()).not.toThrow();
    });
  });

  describe('getTerminalManager', () => {
    it('should return singleton instance', () => {
      const instance1 = getTerminalManager();
      const instance2 = getTerminalManager();

      expect(instance1).toBe(instance2);
    });

    it('should return TerminalManager instance', () => {
      const instance = getTerminalManager();
      expect(instance).toBeInstanceOf(TerminalManager);
    });
  });
});
