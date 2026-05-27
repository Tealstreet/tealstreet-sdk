import { describe, it, expect, beforeEach, vi } from 'vitest';
import { CommandRegistry } from './CommandRegistry';
import type { Command, CommandContext, CommandResult } from './Command';
import { CommandCategory } from './Command';

// Helper to create mock commands
const createMockCommand = (name: string, options: Partial<Command> = {}): Command => ({
  name,
  aliases: [],
  description: `Mock ${name} command`,
  examples: [],
  category: CommandCategory.Utility,
  requiresConnection: false,
  execute: vi.fn().mockResolvedValue({ success: true }),
  ...options,
});

describe('CommandRegistry', () => {
  let registry: CommandRegistry;

  beforeEach(() => {
    registry = new CommandRegistry();
  });

  describe('register', () => {
    it('should register a command by name', () => {
      const command = createMockCommand('test');
      registry.register(command);
      expect(registry.has('test')).toBe(true);
    });

    it('should register command with lowercase name', () => {
      const command = createMockCommand('TEST');
      registry.register(command);
      expect(registry.has('test')).toBe(true);
      expect(registry.has('TEST')).toBe(true);
    });

    it('should register aliases', () => {
      const command = createMockCommand('position', {
        aliases: ['pos', 'p'],
      });
      registry.register(command);
      expect(registry.has('position')).toBe(true);
      expect(registry.has('pos')).toBe(true);
      expect(registry.has('p')).toBe(true);
    });

    it('should handle empty aliases array', () => {
      const command = createMockCommand('solo', { aliases: [] });
      registry.register(command);
      expect(registry.has('solo')).toBe(true);
    });

    it('should overwrite existing command with same name', () => {
      const command1 = createMockCommand('test', { description: 'First' });
      const command2 = createMockCommand('test', { description: 'Second' });
      registry.register(command1);
      registry.register(command2);
      expect(registry.find('test')?.description).toBe('Second');
    });
  });

  describe('registerAll', () => {
    it('should register multiple commands at once', () => {
      const commands = [createMockCommand('cmd1'), createMockCommand('cmd2'), createMockCommand('cmd3')];
      registry.registerAll(commands);
      expect(registry.has('cmd1')).toBe(true);
      expect(registry.has('cmd2')).toBe(true);
      expect(registry.has('cmd3')).toBe(true);
    });

    it('should handle empty array', () => {
      registry.registerAll([]);
      expect(registry.getAll()).toHaveLength(0);
    });

    it('should register all commands with their aliases', () => {
      const commands = [
        createMockCommand('buy', { aliases: ['b', 'long'] }),
        createMockCommand('sell', { aliases: ['s', 'short'] }),
      ];
      registry.registerAll(commands);
      expect(registry.has('buy')).toBe(true);
      expect(registry.has('b')).toBe(true);
      expect(registry.has('long')).toBe(true);
      expect(registry.has('sell')).toBe(true);
      expect(registry.has('s')).toBe(true);
      expect(registry.has('short')).toBe(true);
    });
  });

  describe('find', () => {
    beforeEach(() => {
      registry.register(createMockCommand('order', { aliases: ['o', 'ord'] }));
    });

    it('should find command by primary name', () => {
      const found = registry.find('order');
      expect(found).toBeDefined();
      expect(found?.name).toBe('order');
    });

    it('should find command by alias', () => {
      const found = registry.find('o');
      expect(found).toBeDefined();
      expect(found?.name).toBe('order');
    });

    it('should find command by another alias', () => {
      const found = registry.find('ord');
      expect(found).toBeDefined();
      expect(found?.name).toBe('order');
    });

    it('should be case-insensitive', () => {
      expect(registry.find('ORDER')?.name).toBe('order');
      expect(registry.find('Order')?.name).toBe('order');
      expect(registry.find('O')?.name).toBe('order');
    });

    it('should return undefined for unknown command', () => {
      expect(registry.find('unknown')).toBeUndefined();
    });

    it('should return undefined for empty string', () => {
      expect(registry.find('')).toBeUndefined();
    });
  });

  describe('getAll', () => {
    it('should return empty array when no commands registered', () => {
      expect(registry.getAll()).toEqual([]);
    });

    it('should return all registered commands', () => {
      registry.register(createMockCommand('cmd1'));
      registry.register(createMockCommand('cmd2'));
      registry.register(createMockCommand('cmd3'));
      const all = registry.getAll();
      expect(all).toHaveLength(3);
      expect(all.map((c) => c.name)).toContain('cmd1');
      expect(all.map((c) => c.name)).toContain('cmd2');
      expect(all.map((c) => c.name)).toContain('cmd3');
    });

    it('should not include duplicates for aliases', () => {
      registry.register(createMockCommand('command', { aliases: ['cmd', 'c'] }));
      const all = registry.getAll();
      expect(all).toHaveLength(1);
      expect(all[0].name).toBe('command');
    });
  });

  describe('getByCategory', () => {
    beforeEach(() => {
      registry.register(createMockCommand('buy', { category: CommandCategory.Trading }));
      registry.register(createMockCommand('sell', { category: CommandCategory.Trading }));
      registry.register(createMockCommand('help', { category: CommandCategory.Utility }));
      registry.register(createMockCommand('account', { category: CommandCategory.Account }));
    });

    it('should return commands in specified category', () => {
      const trading = registry.getByCategory(CommandCategory.Trading);
      expect(trading).toHaveLength(2);
      expect(trading.map((c) => c.name)).toContain('buy');
      expect(trading.map((c) => c.name)).toContain('sell');
    });

    it('should return single command in category', () => {
      const utility = registry.getByCategory(CommandCategory.Utility);
      expect(utility).toHaveLength(1);
      expect(utility[0].name).toBe('help');
    });

    it('should return empty array for category with no commands', () => {
      const session = registry.getByCategory(CommandCategory.Session);
      expect(session).toEqual([]);
    });

    it('should return empty array for unknown category', () => {
      const unknown = registry.getByCategory('nonexistent' as CommandCategory);
      expect(unknown).toEqual([]);
    });
  });

  describe('has', () => {
    beforeEach(() => {
      registry.register(createMockCommand('exists', { aliases: ['e'] }));
    });

    it('should return true for existing command', () => {
      expect(registry.has('exists')).toBe(true);
    });

    it('should return true for alias', () => {
      expect(registry.has('e')).toBe(true);
    });

    it('should be case-insensitive', () => {
      expect(registry.has('EXISTS')).toBe(true);
      expect(registry.has('E')).toBe(true);
    });

    it('should return false for unknown command', () => {
      expect(registry.has('notexists')).toBe(false);
    });

    it('should return false for empty string', () => {
      expect(registry.has('')).toBe(false);
    });
  });

  describe('execute', () => {
    it('should execute command and return result', async () => {
      const mockResult: CommandResult = {
        success: true,
        message: 'Command executed',
      };
      const command = createMockCommand('run', {
        execute: vi.fn().mockResolvedValue(mockResult),
      });
      registry.register(command);

      const ctx = {
        raw: 'run',
        args: { _: ['run'] },
        symbol: 'BTCUSDT',
        accountName: 'test',
        exchange: null,
        adapter: {} as any,
      } as CommandContext;

      const result = await registry.execute('run', ctx);
      expect(result).toEqual(mockResult);
      expect(command.execute).toHaveBeenCalledWith(ctx);
    });

    it('should return null for unknown command', async () => {
      const ctx = {
        raw: 'unknown',
        args: { _: ['unknown'] },
        symbol: 'BTCUSDT',
        accountName: undefined,
        exchange: null,
        adapter: {} as any,
      } as CommandContext;

      const result = await registry.execute('unknown', ctx);
      expect(result).toBeNull();
    });

    it('should return error when command requires connection but not connected', async () => {
      const command = createMockCommand('order', {
        requiresConnection: true,
      });
      registry.register(command);

      const ctx = {
        raw: 'order',
        args: { _: ['order'] },
        symbol: 'BTCUSDT',
        accountName: undefined,
        exchange: null,
        adapter: {} as any,
      } as CommandContext;

      const result = await registry.execute('order', ctx);
      expect(result?.success).toBe(false);
      expect(result?.error).toContain('Not connected');
    });

    it('should execute command when connection required and exchange provided', async () => {
      const mockResult: CommandResult = { success: true };
      const command = createMockCommand('order', {
        requiresConnection: true,
        execute: vi.fn().mockResolvedValue(mockResult),
      });
      registry.register(command);

      const mockExchange = { name: 'binance' };
      const ctx = {
        raw: 'order',
        args: { _: ['order'] },
        symbol: 'BTCUSDT',
        accountName: 'test',
        exchange: mockExchange,
        adapter: {} as any,
      } as CommandContext;

      const result = await registry.execute('order', ctx);
      expect(result?.success).toBe(true);
      expect(command.execute).toHaveBeenCalledWith(ctx);
    });

    it('should find command by alias and execute', async () => {
      const mockResult: CommandResult = { success: true };
      const command = createMockCommand('position', {
        aliases: ['pos'],
        execute: vi.fn().mockResolvedValue(mockResult),
      });
      registry.register(command);

      const ctx = {
        raw: 'pos',
        args: { _: ['pos'] },
        symbol: 'BTCUSDT',
        accountName: undefined,
        exchange: null,
        adapter: {} as any,
      } as CommandContext;

      const result = await registry.execute('pos', ctx);
      expect(result?.success).toBe(true);
      expect(command.execute).toHaveBeenCalled();
    });
  });
});
