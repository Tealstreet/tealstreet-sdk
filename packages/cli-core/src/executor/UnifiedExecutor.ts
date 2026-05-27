import type { CLIPlatformAdapter, CLISession } from '../types';
import { ExecutionMode, CommandType, CommandAvailability } from '../types';
import type { Command, CommandContext, CommandResult, ParsedArgs } from '../commands/Command';
import { commandRegistry } from '../commands/CommandRegistry';
import { parseArgs } from '../commands/parseArgs';
import { TerminalManager } from '../terminal/TerminalManager';
import type { PromptConfig } from '../utils/PromptFormatter';

/**
 * Result from unified executor
 */
export interface UnifiedExecuteResult {
  /** Whether a command was executed */
  handled: boolean;
  /** Command result if handled */
  result?: CommandResult;
  /** Error message if execution failed */
  error?: string;
  /** Updated session state */
  sessionUpdates?: Partial<CLISession>;
}

/**
 * UnifiedExecutor - Handles command execution for both CLI args and REPL modes
 *
 * Features:
 * - Routes commands to appropriate handlers based on type (instant/suspending)
 * - Manages terminal state for suspending commands
 * - Handles exit signals (Escape, Ctrl+X, Ctrl+C)
 * - Provides clean transitions between REPL and suspending commands
 */
export class UnifiedExecutor {
  constructor(
    private terminalManager: TerminalManager,
    private adapter: CLIPlatformAdapter,
  ) {}

  /**
   * Execute a command
   *
   * @param input - Raw command string (e.g., "serve --local-only")
   * @param mode - Execution mode (CLI_ARGS or REPL)
   * @param session - Current session state
   * @param promptConfig - Optional prompt configuration
   */
  async execute(
    input: string,
    mode: ExecutionMode,
    session: CLISession,
    promptConfig?: PromptConfig,
  ): Promise<UnifiedExecuteResult> {
    const trimmed = input.trim();
    if (!trimmed) {
      return { handled: false };
    }

    // Parse the command
    const args = parseArgs(trimmed);
    const commandName = args._[0]?.toLowerCase();

    if (!commandName) {
      return { handled: false };
    }

    // Find the command in registry
    const command = commandRegistry.find(commandName);
    if (!command) {
      // Not a registered command - return unhandled so caller can try other handlers
      return { handled: false };
    }

    // Check command availability
    if (command.availability === CommandAvailability.REPL_ONLY && mode === ExecutionMode.CLI_ARGS) {
      return {
        handled: true,
        error: `Command '${commandName}' is only available in REPL mode`,
      };
    }
    if (command.availability === CommandAvailability.CLI_ONLY && mode === ExecutionMode.REPL) {
      return {
        handled: true,
        error: `Command '${commandName}' is only available from command line`,
      };
    }

    // Check connection requirement
    if (command.requiresConnection && !session.currentExchange) {
      return {
        handled: true,
        error: `Command '${commandName}' requires an exchange connection`,
      };
    }

    // Execute based on command type
    if (command.type === CommandType.SUSPENDING) {
      return this.executeSuspending(command, args, mode, session, promptConfig);
    } else {
      return this.executeInstant(command, args, mode, session, promptConfig);
    }
  }

  /**
   * Execute an instant (non-suspending) command
   */
  private async executeInstant(
    command: Command,
    args: ParsedArgs,
    mode: ExecutionMode,
    session: CLISession,
    promptConfig?: PromptConfig,
  ): Promise<UnifiedExecuteResult> {
    // Build context
    const ctx: CommandContext = {
      raw: args._.join(' '),
      args,
      symbol: session.currentSymbol,
      accountName: session.currentAccountName,
      exchange: session.currentExchange,
      adapter: this.adapter,
      promptConfig,
      mode,
    };

    try {
      const result = await command.execute(ctx);

      // Map updateSession fields to CLISession fields
      let sessionUpdates: Partial<CLISession> | undefined;
      if (result.updateSession) {
        sessionUpdates = {};
        if (result.updateSession.symbol !== undefined) {
          sessionUpdates.currentSymbol = result.updateSession.symbol;
        }
        if (result.updateSession.accountName !== undefined) {
          sessionUpdates.currentAccountName = result.updateSession.accountName;
        }
        if (result.updateSession.exchange !== undefined) {
          sessionUpdates.currentExchange = result.updateSession.exchange;
        }
      }

      return {
        handled: true,
        result,
        sessionUpdates,
      };
    } catch (error) {
      return {
        handled: true,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  /**
   * Execute a suspending command (takes over terminal until exit)
   */
  private async executeSuspending(
    command: Command,
    args: ParsedArgs,
    mode: ExecutionMode,
    session: CLISession,
    promptConfig?: PromptConfig,
  ): Promise<UnifiedExecuteResult> {
    // Create exit signal
    let exitResolve: () => void;
    const exitSignal = new Promise<void>((resolve) => {
      exitResolve = resolve;
    });

    // Build context with exit handling
    const ctx: CommandContext = {
      raw: args._.join(' '),
      args,
      symbol: session.currentSymbol,
      accountName: session.currentAccountName,
      exchange: session.currentExchange,
      adapter: this.adapter,
      promptConfig,
      mode,
      onExit: () => exitResolve!(),
      exitSignal,
    };

    // Enter suspend mode (closes readline, prepares terminal)
    this.terminalManager.enterSuspendMode();

    // Set up exit handlers (Escape, Ctrl+X, and Ctrl+C for CLI args)
    const cleanupHandlers = this.terminalManager.setupExitHandlers(mode, () => {
      exitResolve!();
    });

    try {
      // Show exit instructions
      this.adapter.printInfo(this.terminalManager.getExitMessage(mode));

      // Execute the command
      const result = await command.execute(ctx);

      // Run cleanup if provided
      if (command.cleanup) {
        await command.cleanup();
      }

      // Map updateSession fields to CLISession fields
      let sessionUpdates: Partial<CLISession> | undefined;
      if (result.updateSession) {
        sessionUpdates = {};
        if (result.updateSession.symbol !== undefined) {
          sessionUpdates.currentSymbol = result.updateSession.symbol;
        }
        if (result.updateSession.accountName !== undefined) {
          sessionUpdates.currentAccountName = result.updateSession.accountName;
        }
        if (result.updateSession.exchange !== undefined) {
          sessionUpdates.currentExchange = result.updateSession.exchange;
        }
      }

      return {
        handled: true,
        result,
        sessionUpdates,
      };
    } catch (error) {
      return {
        handled: true,
        error: error instanceof Error ? error.message : String(error),
      };
    } finally {
      // Clean up handlers
      cleanupHandlers();

      // Exit suspend mode (restores readline)
      this.terminalManager.exitSuspendMode();

      // Handle post-exit based on mode
      if (mode === ExecutionMode.CLI_ARGS) {
        process.exit(0);
      }
      // REPL mode: control returns to REPL naturally
    }
  }
}
