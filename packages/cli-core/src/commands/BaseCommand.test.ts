import { describe, expect, it, vi } from 'vitest';

import { BaseCommand } from './BaseCommand';
import type { CommandCategory, CommandContext, CommandResult } from './Command';
import { CommandAvailability, CommandType } from './Command';

// Concrete implementation for testing
class TestCommand extends BaseCommand {
  readonly name = 'test';
  readonly description = 'A test command';
  readonly category = 'utility' as CommandCategory;

  async execute(_ctx: CommandContext): Promise<CommandResult> {
    return this.success('Executed successfully');
  }

  // Expose protected methods for testing
  public testSuccess(message?: string) {
    return this.success(message);
  }

  public testError(error: string) {
    return this.error(error);
  }

  public testPrint(ctx: CommandContext, lines: string[]) {
    this.print(ctx, lines);
  }

  public testPrintInfo(ctx: CommandContext, message: string) {
    this.printInfo(ctx, message);
  }

  public testPrintError(ctx: CommandContext, message: string) {
    this.printError(ctx, message);
  }

  public testPrintSuccess(ctx: CommandContext, message: string) {
    this.printSuccess(ctx, message);
  }

  public testPrintWarning(ctx: CommandContext, message: string) {
    this.printWarning(ctx, message);
  }

  public testGetArg(ctx: CommandContext, index: number) {
    return this.getArg(ctx, index);
  }

  public testGetArgs(ctx: CommandContext) {
    return this.getArgs(ctx);
  }

  public testHasFlag(ctx: CommandContext, ...names: string[]) {
    return this.hasFlag(ctx, ...names);
  }

  public testGetOption(ctx: CommandContext, ...names: string[]) {
    return this.getOption(ctx, ...names);
  }
}

// Command with aliases for testing
class TestCommandWithAliases extends BaseCommand {
  readonly name = 'help';
  readonly description = 'Show help';
  readonly category = 'utility' as CommandCategory;
  readonly aliases = ['h', '?'];

  async execute(): Promise<CommandResult> {
    return this.success();
  }
}

// Helper to create mock context
function createMockContext(overrides: Partial<CommandContext> = {}): CommandContext {
  return {
    raw: 'test command',
    args: { _: ['test'] },
    symbol: 'BTCUSDT',
    accountName: 'test-account',
    exchange: null,
    adapter: {
      print: vi.fn(),
      printInfo: vi.fn(),
      printError: vi.fn(),
      printSuccess: vi.fn(),
      printWarning: vi.fn(),
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
    },
    ...overrides,
  };
}

describe('BaseCommand', () => {
  describe('default properties', () => {
    it('should have empty aliases by default', () => {
      const command = new TestCommand();
      expect(command.aliases).toEqual([]);
    });

    it('should have empty examples by default', () => {
      const command = new TestCommand();
      expect(command.examples).toEqual([]);
    });

    it('should require connection by default', () => {
      const command = new TestCommand();
      expect(command.requiresConnection).toBe(true);
    });

    it('should be INSTANT type by default', () => {
      const command = new TestCommand();
      expect(command.type).toBe(CommandType.INSTANT);
    });

    it('should be available in BOTH modes by default', () => {
      const command = new TestCommand();
      expect(command.availability).toBe(CommandAvailability.BOTH);
    });
  });

  describe('matches', () => {
    it('should match the command name', () => {
      const command = new TestCommand();
      expect(command.matches('test')).toBe(true);
    });

    it('should match case-insensitively', () => {
      const command = new TestCommand();
      expect(command.matches('TEST')).toBe(true);
      expect(command.matches('Test')).toBe(true);
    });

    it('should not match different names', () => {
      const command = new TestCommand();
      expect(command.matches('other')).toBe(false);
    });

    it('should match aliases', () => {
      const command = new TestCommandWithAliases();
      expect(command.matches('h')).toBe(true);
      expect(command.matches('?')).toBe(true);
    });

    it('should match aliases case-insensitively', () => {
      const command = new TestCommandWithAliases();
      expect(command.matches('H')).toBe(true);
    });

    it('should not match partial names', () => {
      const command = new TestCommand();
      expect(command.matches('tes')).toBe(false);
      expect(command.matches('testing')).toBe(false);
    });
  });

  describe('success', () => {
    it('should create success result without message', () => {
      const command = new TestCommand();
      const result = command.testSuccess();

      expect(result).toEqual({ success: true, message: undefined });
    });

    it('should create success result with message', () => {
      const command = new TestCommand();
      const result = command.testSuccess('Operation completed');

      expect(result).toEqual({ success: true, message: 'Operation completed' });
    });
  });

  describe('error', () => {
    it('should create error result with error message', () => {
      const command = new TestCommand();
      const result = command.testError('Something went wrong');

      expect(result).toEqual({ success: false, error: 'Something went wrong' });
    });

    it('should create error result with empty string', () => {
      const command = new TestCommand();
      const result = command.testError('');

      expect(result).toEqual({ success: false, error: '' });
    });
  });

  describe('print helpers', () => {
    it('should call adapter.print with lines', () => {
      const command = new TestCommand();
      const ctx = createMockContext();
      const lines = ['Line 1', 'Line 2'];

      command.testPrint(ctx, lines);

      expect(ctx.adapter.print).toHaveBeenCalledWith(lines);
    });

    it('should call adapter.printInfo with message', () => {
      const command = new TestCommand();
      const ctx = createMockContext();

      command.testPrintInfo(ctx, 'Info message');

      expect(ctx.adapter.printInfo).toHaveBeenCalledWith('Info message');
    });

    it('should call adapter.printError with message', () => {
      const command = new TestCommand();
      const ctx = createMockContext();

      command.testPrintError(ctx, 'Error message');

      expect(ctx.adapter.printError).toHaveBeenCalledWith('Error message');
    });

    it('should call adapter.printSuccess with message', () => {
      const command = new TestCommand();
      const ctx = createMockContext();

      command.testPrintSuccess(ctx, 'Success message');

      expect(ctx.adapter.printSuccess).toHaveBeenCalledWith('Success message');
    });

    it('should call adapter.printWarning with message', () => {
      const command = new TestCommand();
      const ctx = createMockContext();

      command.testPrintWarning(ctx, 'Warning message');

      expect(ctx.adapter.printWarning).toHaveBeenCalledWith('Warning message');
    });
  });

  describe('getArg', () => {
    it('should return argument at given index', () => {
      const command = new TestCommand();
      const ctx = createMockContext({
        args: { _: ['test', 'arg1', 'arg2'] },
      });

      expect(command.testGetArg(ctx, 0)).toBe('test');
      expect(command.testGetArg(ctx, 1)).toBe('arg1');
      expect(command.testGetArg(ctx, 2)).toBe('arg2');
    });

    it('should return undefined for out of bounds index', () => {
      const command = new TestCommand();
      const ctx = createMockContext({
        args: { _: ['test'] },
      });

      expect(command.testGetArg(ctx, 1)).toBeUndefined();
      expect(command.testGetArg(ctx, 10)).toBeUndefined();
    });

    it('should return undefined for negative index', () => {
      const command = new TestCommand();
      const ctx = createMockContext({
        args: { _: ['test', 'arg1'] },
      });

      expect(command.testGetArg(ctx, -1)).toBeUndefined();
    });
  });

  describe('getArgs', () => {
    it('should return all arguments after command name', () => {
      const command = new TestCommand();
      const ctx = createMockContext({
        args: { _: ['test', 'arg1', 'arg2', 'arg3'] },
      });

      expect(command.testGetArgs(ctx)).toEqual(['arg1', 'arg2', 'arg3']);
    });

    it('should return empty array when only command name present', () => {
      const command = new TestCommand();
      const ctx = createMockContext({
        args: { _: ['test'] },
      });

      expect(command.testGetArgs(ctx)).toEqual([]);
    });

    it('should return empty array when no arguments', () => {
      const command = new TestCommand();
      const ctx = createMockContext({
        args: { _: [] },
      });

      expect(command.testGetArgs(ctx)).toEqual([]);
    });
  });

  describe('hasFlag', () => {
    it('should return true when flag is present and true', () => {
      const command = new TestCommand();
      const ctx = createMockContext({
        args: { _: ['test'], verbose: true },
      });

      expect(command.testHasFlag(ctx, 'verbose')).toBe(true);
    });

    it('should return false when flag is not present', () => {
      const command = new TestCommand();
      const ctx = createMockContext({
        args: { _: ['test'] },
      });

      expect(command.testHasFlag(ctx, 'verbose')).toBe(false);
    });

    it('should return false when flag is false', () => {
      const command = new TestCommand();
      const ctx = createMockContext({
        args: { _: ['test'], verbose: false },
      });

      expect(command.testHasFlag(ctx, 'verbose')).toBe(false);
    });

    it('should return false when flag is a string value', () => {
      const command = new TestCommand();
      const ctx = createMockContext({
        args: { _: ['test'], verbose: 'yes' },
      });

      expect(command.testHasFlag(ctx, 'verbose')).toBe(false);
    });

    it('should check multiple flag names', () => {
      const command = new TestCommand();
      const ctx = createMockContext({
        args: { _: ['test'], v: true },
      });

      expect(command.testHasFlag(ctx, 'verbose', 'v')).toBe(true);
    });

    it('should return true if any of multiple flags is present', () => {
      const command = new TestCommand();
      const ctx = createMockContext({
        args: { _: ['test'], h: true },
      });

      expect(command.testHasFlag(ctx, 'help', 'h', '?')).toBe(true);
    });

    it('should return false if none of multiple flags present', () => {
      const command = new TestCommand();
      const ctx = createMockContext({
        args: { _: ['test'] },
      });

      expect(command.testHasFlag(ctx, 'help', 'h', '?')).toBe(false);
    });
  });

  describe('getOption', () => {
    it('should return option value when present', () => {
      const command = new TestCommand();
      const ctx = createMockContext({
        args: { _: ['test'], output: 'json' },
      });

      expect(command.testGetOption(ctx, 'output')).toBe('json');
    });

    it('should return undefined when option not present', () => {
      const command = new TestCommand();
      const ctx = createMockContext({
        args: { _: ['test'] },
      });

      expect(command.testGetOption(ctx, 'output')).toBeUndefined();
    });

    it('should return undefined when option is boolean', () => {
      const command = new TestCommand();
      const ctx = createMockContext({
        args: { _: ['test'], output: true },
      });

      expect(command.testGetOption(ctx, 'output')).toBeUndefined();
    });

    it('should check multiple option names and return first string', () => {
      const command = new TestCommand();
      const ctx = createMockContext({
        args: { _: ['test'], o: 'json' },
      });

      expect(command.testGetOption(ctx, 'output', 'o')).toBe('json');
    });

    it('should return first found option when multiple present', () => {
      const command = new TestCommand();
      const ctx = createMockContext({
        args: { _: ['test'], output: 'json', o: 'yaml' },
      });

      expect(command.testGetOption(ctx, 'output', 'o')).toBe('json');
    });

    it('should return undefined when all options are non-string', () => {
      const command = new TestCommand();
      const ctx = createMockContext({
        args: { _: ['test'], output: true, o: false },
      });

      expect(command.testGetOption(ctx, 'output', 'o')).toBeUndefined();
    });

    it('should handle empty string option value', () => {
      const command = new TestCommand();
      const ctx = createMockContext({
        args: { _: ['test'], output: '' },
      });

      expect(command.testGetOption(ctx, 'output')).toBe('');
    });
  });

  describe('execute', () => {
    it('should be implemented by concrete command', async () => {
      const command = new TestCommand();
      const ctx = createMockContext();

      const result = await command.execute(ctx);

      expect(result).toEqual({ success: true, message: 'Executed successfully' });
    });
  });
});
