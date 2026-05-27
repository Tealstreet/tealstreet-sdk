import type { Command, CommandContext, CommandResult } from './Command';

/**
 * Registry for commands - handles registration and lookup
 */
export class CommandRegistry {
  private commands: Map<string, Command> = new Map();
  private aliases: Map<string, string> = new Map();

  /**
   * Register a command
   */
  register(command: Command): void {
    // Register by primary name
    this.commands.set(command.name.toLowerCase(), command);

    // Register aliases
    for (const alias of command.aliases) {
      this.aliases.set(alias.toLowerCase(), command.name.toLowerCase());
    }
  }

  /**
   * Register multiple commands at once
   */
  registerAll(commands: Command[]): void {
    for (const command of commands) {
      this.register(command);
    }
  }

  /**
   * Find a command by name or alias
   */
  find(name: string): Command | undefined {
    const lower = name.toLowerCase();

    // Try direct lookup
    const direct = this.commands.get(lower);
    if (direct) return direct;

    // Try alias lookup
    const primaryName = this.aliases.get(lower);
    if (primaryName) {
      return this.commands.get(primaryName);
    }

    return undefined;
  }

  /**
   * Get all registered commands
   */
  getAll(): Command[] {
    return Array.from(this.commands.values());
  }

  /**
   * Get commands by category
   */
  getByCategory(category: string): Command[] {
    return this.getAll().filter((cmd) => cmd.category === category);
  }

  /**
   * Check if a command exists
   */
  has(name: string): boolean {
    return this.find(name) !== undefined;
  }

  /**
   * Execute a command by name
   */
  async execute(name: string, ctx: CommandContext): Promise<CommandResult | null> {
    const command = this.find(name);
    if (!command) {
      return null;
    }

    // Check if command requires connection
    if (command.requiresConnection && !ctx.exchange) {
      return {
        success: false,
        error: 'Not connected to any exchange. Use "account <name>" to connect.',
      };
    }

    return command.execute(ctx);
  }
}

// Singleton instance for the main registry
export const commandRegistry = new CommandRegistry();
