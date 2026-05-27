import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { UnifiedExecutor, UnifiedExecuteResult } from './UnifiedExecutor';
import { TerminalManager } from '../terminal/TerminalManager';
import { commandRegistry } from '../commands/CommandRegistry';
import type { CLIPlatformAdapter, CLISession, ExchangeLike } from '../types';
import { ExecutionMode, CommandType, CommandAvailability } from '../types';
import type { Command, CommandContext, CommandResult, ParsedArgs } from '../commands/Command';
import { CommandCategory } from '../commands/Command';

// Mock the command registry
vi.mock('../commands/CommandRegistry', () => ({
  commandRegistry: {
    find: vi.fn(),
  },
}));

// Mock process.exit to prevent test from exiting
vi.spyOn(process, 'exit').mockImplementation((() => {}) as never);

describe('UnifiedExecutor', () => {
  let executor: UnifiedExecutor;
  let mockTerminalManager: TerminalManager;
  let mockAdapter: CLIPlatformAdapter;
  let mockSession: CLISession;
  let mockExchange: ExchangeLike;

  beforeEach(() => {
    // Create mock terminal manager
    mockTerminalManager = {
      enterSuspendMode: vi.fn(),
      exitSuspendMode: vi.fn(),
      setupExitHandlers: vi.fn().mockReturnValue(() => {}),
      getExitMessage: vi.fn().mockReturnValue('Press Escape to exit'),
    } as unknown as TerminalManager;

    // Create mock exchange
    mockExchange = {
      name: 'test-exchange',
      emitter: {
        on: vi.fn(),
        off: vi.fn(),
        emit: vi.fn(),
      },
    };

    // Create mock adapter
    mockAdapter = {
      platformType: 'standalone',
      listAccounts: vi.fn().mockReturnValue([]),
      getAccount: vi.fn(),
      addAccount: vi.fn(),
      updateAccount: vi.fn(),
      removeAccount: vi.fn(),
      setDefaultAccount: vi.fn(),
      getDefaultSymbol: vi.fn(),
      setDefaultSymbol: vi.fn(),
      getActiveAccounts: vi.fn().mockReturnValue([]),
      createExchange: vi.fn(),
      startExchange: vi.fn(),
      stopExchange: vi.fn(),
      print: vi.fn(),
      printError: vi.fn(),
      printSuccess: vi.fn(),
      printWarning: vi.fn(),
      printInfo: vi.fn(),
    };

    // Create mock session
    mockSession = {
      currentExchange: mockExchange,
      currentAccountName: 'test-account',
      currentSymbol: 'BTCUSDT',
    };

    // Create executor
    executor = new UnifiedExecutor(mockTerminalManager, mockAdapter);
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  describe('execute', () => {
    it('should return handled: false for empty input', async () => {
      const result = await executor.execute('', ExecutionMode.REPL, mockSession);
      expect(result.handled).toBe(false);
    });

    it('should return handled: false for whitespace-only input', async () => {
      const result = await executor.execute('   ', ExecutionMode.REPL, mockSession);
      expect(result.handled).toBe(false);
    });

    it('should return handled: false for unknown command', async () => {
      vi.mocked(commandRegistry.find).mockReturnValue(undefined);

      const result = await executor.execute('unknowncmd', ExecutionMode.REPL, mockSession);
      expect(result.handled).toBe(false);
    });

    it('should handle REPL_ONLY command in CLI_ARGS mode', async () => {
      const mockCommand: Command = {
        name: 'replonly',
        aliases: [],
        description: 'REPL only command',
        examples: [],
        category: CommandCategory.Utility,
        requiresConnection: false,
        availability: CommandAvailability.REPL_ONLY,
        execute: vi.fn(),
      };

      vi.mocked(commandRegistry.find).mockReturnValue(mockCommand);

      const result = await executor.execute('replonly', ExecutionMode.CLI_ARGS, mockSession);

      expect(result.handled).toBe(true);
      expect(result.error).toContain('only available in REPL mode');
    });

    it('should handle CLI_ONLY command in REPL mode', async () => {
      const mockCommand: Command = {
        name: 'clionly',
        aliases: [],
        description: 'CLI only command',
        examples: [],
        category: CommandCategory.Utility,
        requiresConnection: false,
        availability: CommandAvailability.CLI_ONLY,
        execute: vi.fn(),
      };

      vi.mocked(commandRegistry.find).mockReturnValue(mockCommand);

      const result = await executor.execute('clionly', ExecutionMode.REPL, mockSession);

      expect(result.handled).toBe(true);
      expect(result.error).toContain('only available from command line');
    });

    it('should handle command requiring connection when not connected', async () => {
      const mockCommand: Command = {
        name: 'trade',
        aliases: [],
        description: 'Trade command',
        examples: [],
        category: CommandCategory.Trading,
        requiresConnection: true,
        execute: vi.fn(),
      };

      vi.mocked(commandRegistry.find).mockReturnValue(mockCommand);

      const sessionWithoutExchange: CLISession = {
        currentExchange: null,
        currentAccountName: undefined,
        currentSymbol: 'BTCUSDT',
      };

      const result = await executor.execute('trade', ExecutionMode.REPL, sessionWithoutExchange);

      expect(result.handled).toBe(true);
      expect(result.error).toContain('requires an exchange connection');
    });

    it('should execute instant command', async () => {
      const mockCommand: Command = {
        name: 'help',
        aliases: [],
        description: 'Help command',
        examples: [],
        category: CommandCategory.Utility,
        requiresConnection: false,
        type: CommandType.INSTANT,
        execute: vi.fn().mockResolvedValue({ success: true, message: 'Help output' }),
      };

      vi.mocked(commandRegistry.find).mockReturnValue(mockCommand);

      const result = await executor.execute('help', ExecutionMode.REPL, mockSession);

      expect(result.handled).toBe(true);
      expect(result.result?.success).toBe(true);
      expect(mockCommand.execute).toHaveBeenCalled();
    });

    it('should execute suspending command', async () => {
      const mockCommand: Command = {
        name: 'watch',
        aliases: [],
        description: 'Watch command',
        examples: [],
        category: CommandCategory.Utility,
        requiresConnection: false,
        type: CommandType.SUSPENDING,
        execute: vi.fn().mockResolvedValue({ success: true }),
      };

      vi.mocked(commandRegistry.find).mockReturnValue(mockCommand);

      const result = await executor.execute('watch', ExecutionMode.REPL, mockSession);

      expect(result.handled).toBe(true);
      expect(mockTerminalManager.enterSuspendMode).toHaveBeenCalled();
      expect(mockTerminalManager.exitSuspendMode).toHaveBeenCalled();
    });

    it('should handle command with arguments', async () => {
      const mockCommand: Command = {
        name: 'symbol',
        aliases: [],
        description: 'Symbol command',
        examples: [],
        category: CommandCategory.Session,
        requiresConnection: false,
        execute: vi.fn().mockResolvedValue({ success: true }),
      };

      vi.mocked(commandRegistry.find).mockReturnValue(mockCommand);

      await executor.execute('symbol ETHUSDT', ExecutionMode.REPL, mockSession);

      const callArgs = (mockCommand.execute as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(callArgs.args._).toContain('ETHUSDT');
    });
  });

  describe('executeInstant (via execute)', () => {
    it('should handle session updates', async () => {
      const mockCommand: Command = {
        name: 'symbol',
        aliases: [],
        description: 'Symbol command',
        examples: [],
        category: CommandCategory.Session,
        requiresConnection: false,
        type: CommandType.INSTANT,
        execute: vi.fn().mockResolvedValue({
          success: true,
          updateSession: {
            symbol: 'ETHUSDT',
          },
        }),
      };

      vi.mocked(commandRegistry.find).mockReturnValue(mockCommand);

      const result = await executor.execute('symbol ETHUSDT', ExecutionMode.REPL, mockSession);

      expect(result.sessionUpdates?.currentSymbol).toBe('ETHUSDT');
    });

    it('should handle account name updates', async () => {
      const mockCommand: Command = {
        name: 'account',
        aliases: [],
        description: 'Account command',
        examples: [],
        category: CommandCategory.Account,
        requiresConnection: false,
        type: CommandType.INSTANT,
        execute: vi.fn().mockResolvedValue({
          success: true,
          updateSession: {
            accountName: 'new-account',
          },
        }),
      };

      vi.mocked(commandRegistry.find).mockReturnValue(mockCommand);

      const result = await executor.execute('account new-account', ExecutionMode.REPL, mockSession);

      expect(result.sessionUpdates?.currentAccountName).toBe('new-account');
    });

    it('should handle exchange updates', async () => {
      const newExchange: ExchangeLike = {
        name: 'new-exchange',
        emitter: { on: vi.fn(), off: vi.fn(), emit: vi.fn() },
      };

      const mockCommand: Command = {
        name: 'connect',
        aliases: [],
        description: 'Connect command',
        examples: [],
        category: CommandCategory.Account,
        requiresConnection: false,
        type: CommandType.INSTANT,
        execute: vi.fn().mockResolvedValue({
          success: true,
          updateSession: {
            exchange: newExchange,
          },
        }),
      };

      vi.mocked(commandRegistry.find).mockReturnValue(mockCommand);

      const result = await executor.execute('connect', ExecutionMode.REPL, mockSession);

      expect(result.sessionUpdates?.currentExchange).toBe(newExchange);
    });

    it('should handle execution errors', async () => {
      const mockCommand: Command = {
        name: 'failing',
        aliases: [],
        description: 'Failing command',
        examples: [],
        category: CommandCategory.Utility,
        requiresConnection: false,
        type: CommandType.INSTANT,
        execute: vi.fn().mockRejectedValue(new Error('Execution failed')),
      };

      vi.mocked(commandRegistry.find).mockReturnValue(mockCommand);

      const result = await executor.execute('failing', ExecutionMode.REPL, mockSession);

      expect(result.handled).toBe(true);
      expect(result.error).toBe('Execution failed');
    });

    it('should handle non-Error throws', async () => {
      const mockCommand: Command = {
        name: 'failing',
        aliases: [],
        description: 'Failing command',
        examples: [],
        category: CommandCategory.Utility,
        requiresConnection: false,
        type: CommandType.INSTANT,
        execute: vi.fn().mockRejectedValue('String error'),
      };

      vi.mocked(commandRegistry.find).mockReturnValue(mockCommand);

      const result = await executor.execute('failing', ExecutionMode.REPL, mockSession);

      expect(result.handled).toBe(true);
      expect(result.error).toBe('String error');
    });

    it('should pass correct context to command', async () => {
      const mockCommand: Command = {
        name: 'test',
        aliases: [],
        description: 'Test command',
        examples: [],
        category: CommandCategory.Utility,
        requiresConnection: false,
        type: CommandType.INSTANT,
        execute: vi.fn().mockResolvedValue({ success: true }),
      };

      vi.mocked(commandRegistry.find).mockReturnValue(mockCommand);

      await executor.execute('test --flag value', ExecutionMode.REPL, mockSession);

      const ctx: CommandContext = (mockCommand.execute as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(ctx.symbol).toBe('BTCUSDT');
      expect(ctx.accountName).toBe('test-account');
      expect(ctx.exchange).toBe(mockExchange);
      expect(ctx.adapter).toBe(mockAdapter);
      expect(ctx.mode).toBe(ExecutionMode.REPL);
    });
  });

  describe('executeSuspending (via execute)', () => {
    it('should enter and exit suspend mode', async () => {
      const mockCommand: Command = {
        name: 'serve',
        aliases: [],
        description: 'Serve command',
        examples: [],
        category: CommandCategory.Utility,
        requiresConnection: false,
        type: CommandType.SUSPENDING,
        execute: vi.fn().mockResolvedValue({ success: true }),
      };

      vi.mocked(commandRegistry.find).mockReturnValue(mockCommand);

      await executor.execute('serve', ExecutionMode.REPL, mockSession);

      expect(mockTerminalManager.enterSuspendMode).toHaveBeenCalled();
      expect(mockTerminalManager.exitSuspendMode).toHaveBeenCalled();
    });

    it('should setup exit handlers', async () => {
      const mockCommand: Command = {
        name: 'serve',
        aliases: [],
        description: 'Serve command',
        examples: [],
        category: CommandCategory.Utility,
        requiresConnection: false,
        type: CommandType.SUSPENDING,
        execute: vi.fn().mockResolvedValue({ success: true }),
      };

      vi.mocked(commandRegistry.find).mockReturnValue(mockCommand);

      await executor.execute('serve', ExecutionMode.REPL, mockSession);

      expect(mockTerminalManager.setupExitHandlers).toHaveBeenCalledWith(ExecutionMode.REPL, expect.any(Function));
    });

    it('should print exit message', async () => {
      const mockCommand: Command = {
        name: 'serve',
        aliases: [],
        description: 'Serve command',
        examples: [],
        category: CommandCategory.Utility,
        requiresConnection: false,
        type: CommandType.SUSPENDING,
        execute: vi.fn().mockResolvedValue({ success: true }),
      };

      vi.mocked(commandRegistry.find).mockReturnValue(mockCommand);

      await executor.execute('serve', ExecutionMode.REPL, mockSession);

      expect(mockAdapter.printInfo).toHaveBeenCalledWith('Press Escape to exit');
    });

    it('should call cleanup on command if provided', async () => {
      const mockCleanup = vi.fn();
      const mockCommand: Command = {
        name: 'serve',
        aliases: [],
        description: 'Serve command',
        examples: [],
        category: CommandCategory.Utility,
        requiresConnection: false,
        type: CommandType.SUSPENDING,
        execute: vi.fn().mockResolvedValue({ success: true }),
        cleanup: mockCleanup,
      };

      vi.mocked(commandRegistry.find).mockReturnValue(mockCommand);

      await executor.execute('serve', ExecutionMode.REPL, mockSession);

      expect(mockCleanup).toHaveBeenCalled();
    });

    it('should handle session updates from suspending command', async () => {
      const mockCommand: Command = {
        name: 'pair',
        aliases: [],
        description: 'Pair command',
        examples: [],
        category: CommandCategory.Account,
        requiresConnection: false,
        type: CommandType.SUSPENDING,
        execute: vi.fn().mockResolvedValue({
          success: true,
          updateSession: {
            symbol: 'ETHUSDT',
            accountName: 'paired-account',
          },
        }),
      };

      vi.mocked(commandRegistry.find).mockReturnValue(mockCommand);

      const result = await executor.execute('pair', ExecutionMode.REPL, mockSession);

      expect(result.sessionUpdates?.currentSymbol).toBe('ETHUSDT');
      expect(result.sessionUpdates?.currentAccountName).toBe('paired-account');
    });

    it('should handle errors in suspending command', async () => {
      const mockCommand: Command = {
        name: 'serve',
        aliases: [],
        description: 'Serve command',
        examples: [],
        category: CommandCategory.Utility,
        requiresConnection: false,
        type: CommandType.SUSPENDING,
        execute: vi.fn().mockRejectedValue(new Error('Server failed')),
      };

      vi.mocked(commandRegistry.find).mockReturnValue(mockCommand);

      const result = await executor.execute('serve', ExecutionMode.REPL, mockSession);

      expect(result.handled).toBe(true);
      expect(result.error).toBe('Server failed');
      // Should still exit suspend mode on error
      expect(mockTerminalManager.exitSuspendMode).toHaveBeenCalled();
    });

    it('should cleanup handlers even on error', async () => {
      const mockCleanupFn = vi.fn();
      (mockTerminalManager.setupExitHandlers as ReturnType<typeof vi.fn>).mockReturnValue(mockCleanupFn);

      const mockCommand: Command = {
        name: 'serve',
        aliases: [],
        description: 'Serve command',
        examples: [],
        category: CommandCategory.Utility,
        requiresConnection: false,
        type: CommandType.SUSPENDING,
        execute: vi.fn().mockRejectedValue(new Error('Server failed')),
      };

      vi.mocked(commandRegistry.find).mockReturnValue(mockCommand);

      await executor.execute('serve', ExecutionMode.REPL, mockSession);

      expect(mockCleanupFn).toHaveBeenCalled();
    });

    it('should provide exit signal and onExit in context', async () => {
      const mockCommand: Command = {
        name: 'watch',
        aliases: [],
        description: 'Watch command',
        examples: [],
        category: CommandCategory.Utility,
        requiresConnection: false,
        type: CommandType.SUSPENDING,
        execute: vi.fn().mockResolvedValue({ success: true }),
      };

      vi.mocked(commandRegistry.find).mockReturnValue(mockCommand);

      await executor.execute('watch', ExecutionMode.REPL, mockSession);

      const ctx: CommandContext = (mockCommand.execute as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(ctx.exitSignal).toBeInstanceOf(Promise);
      expect(typeof ctx.onExit).toBe('function');
    });
  });

  describe('edge cases', () => {
    it('should handle command name case-insensitively', async () => {
      const mockCommand: Command = {
        name: 'help',
        aliases: [],
        description: 'Help command',
        examples: [],
        category: CommandCategory.Utility,
        requiresConnection: false,
        execute: vi.fn().mockResolvedValue({ success: true }),
      };

      vi.mocked(commandRegistry.find).mockReturnValue(mockCommand);

      await executor.execute('HELP', ExecutionMode.REPL, mockSession);

      expect(commandRegistry.find).toHaveBeenCalledWith('help');
    });

    it('should handle session with null exchange', async () => {
      const mockCommand: Command = {
        name: 'help',
        aliases: [],
        description: 'Help command',
        examples: [],
        category: CommandCategory.Utility,
        requiresConnection: false,
        execute: vi.fn().mockResolvedValue({ success: true }),
      };

      vi.mocked(commandRegistry.find).mockReturnValue(mockCommand);

      const sessionWithoutExchange: CLISession = {
        currentExchange: null,
        currentAccountName: undefined,
        currentSymbol: 'BTCUSDT',
      };

      const result = await executor.execute('help', ExecutionMode.REPL, sessionWithoutExchange);

      expect(result.handled).toBe(true);
      expect(result.result?.success).toBe(true);
    });

    it('should handle command with no type (defaults to instant)', async () => {
      const mockCommand: Command = {
        name: 'simple',
        aliases: [],
        description: 'Simple command',
        examples: [],
        category: CommandCategory.Utility,
        requiresConnection: false,
        // No type specified
        execute: vi.fn().mockResolvedValue({ success: true }),
      };

      vi.mocked(commandRegistry.find).mockReturnValue(mockCommand);

      const result = await executor.execute('simple', ExecutionMode.REPL, mockSession);

      expect(result.handled).toBe(true);
      // Should not enter suspend mode for instant command
      expect(mockTerminalManager.enterSuspendMode).not.toHaveBeenCalled();
    });

    it('should handle command with no availability (defaults to BOTH)', async () => {
      const mockCommand: Command = {
        name: 'versatile',
        aliases: [],
        description: 'Versatile command',
        examples: [],
        category: CommandCategory.Utility,
        requiresConnection: false,
        // No availability specified
        execute: vi.fn().mockResolvedValue({ success: true }),
      };

      vi.mocked(commandRegistry.find).mockReturnValue(mockCommand);

      // Should work in both modes
      let result = await executor.execute('versatile', ExecutionMode.REPL, mockSession);
      expect(result.handled).toBe(true);

      result = await executor.execute('versatile', ExecutionMode.CLI_ARGS, mockSession);
      expect(result.handled).toBe(true);
    });

    it('should handle promptConfig parameter', async () => {
      const mockCommand: Command = {
        name: 'test',
        aliases: [],
        description: 'Test command',
        examples: [],
        category: CommandCategory.Utility,
        requiresConnection: false,
        execute: vi.fn().mockResolvedValue({ success: true }),
      };

      vi.mocked(commandRegistry.find).mockReturnValue(mockCommand);

      const promptConfig = { format: '[{symbol}]> ', prefix: 'tealstreet' };
      await executor.execute('test', ExecutionMode.REPL, mockSession, promptConfig);

      const ctx: CommandContext = (mockCommand.execute as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(ctx.promptConfig).toBe(promptConfig);
    });
  });
});
