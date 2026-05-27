import type { CLIPlatformAdapter, ExchangeLike, TaskAction } from '../types';
import { CommandType, CommandAvailability, ExecutionMode } from '../types';
import type { PromptConfig } from '../utils/PromptFormatter';
import type { PreprocessStore } from '../preprocess/types';

// Re-export TaskAction for convenience
export type { TaskAction };

// Re-export for convenience
export { CommandType, CommandAvailability, ExecutionMode };

/**
 * Command categories for organization and help display
 */
export enum CommandCategory {
  Session = 'session',
  Account = 'account',
  Trading = 'trading',
  Position = 'position',
  Utility = 'utility',
}

/**
 * Parsed command arguments
 */
export interface ParsedArgs {
  /** Positional arguments (command name is _[0]) */
  _: string[];
  /** Named flags and options */
  [key: string]: string | boolean | string[] | undefined;
}

/**
 * Context passed to command execution
 */
export interface CommandContext {
  /** Raw command string */
  raw: string;
  /** Parsed arguments */
  args: ParsedArgs;
  /** Current symbol */
  symbol: string;
  /** Current account name */
  accountName: string | undefined;
  /** Exchange instance (may be null if not connected) */
  exchange: ExchangeLike | null;
  /** Platform adapter for I/O and account management */
  adapter: CLIPlatformAdapter;
  /** Prompt configuration */
  promptConfig?: PromptConfig;
  /** User vars and aliases store, populated by the preprocess layer.
   *  Required by `set` / `unset` / `print` / `alias` / `unalias`. */
  preprocessStore?: PreprocessStore;

  // Execution context (for suspending commands)
  /** Execution mode - CLI args or REPL */
  mode?: ExecutionMode;
  /** Call to signal command exit (suspending commands) */
  onExit?: () => void;
  /** Resolves when exit requested (suspending commands await this) */
  exitSignal?: Promise<void>;
}

/**
 * Result returned from command execution
 */
export interface CommandResult {
  /** Whether the command was handled */
  success: boolean;
  /** Output message (if any) */
  message?: string;
  /** Error message (if failed) */
  error?: string;
  /** Whether to update session state */
  updateSession?: {
    symbol?: string;
    accountName?: string;
    exchange?: ExchangeLike | null;
    promptConfig?: PromptConfig;
  };
  /** Action to perform on accounts (enable/disable streaming) */
  accountAction?: {
    action: 'enable' | 'disable';
    accountName?: string; // Single account (deprecated, use accountNames)
    accountNames?: string[]; // Multiple accounts
  };
  /** Action to perform on tasks (TWAP, chaser, DCA) */
  taskAction?: TaskAction;
  /**
   * Structured payload for the capture syntax (`o1 = buy …`). Today
   * the runner only reads `orderIds` — it's joined with `,` and
   * written into the named user var so `cancel @o1` expands to a
   * comma-list of IDs via the existing var-substitution path.
   *
   * Multi-ID is the default: bracket orders (entry + SL + TP),
   * scale, swarm, and any other primitive that places more than one
   * order all surface every ID they placed. An empty array means
   * the underlying exchange call doesn't expose IDs (e.g. Bybit's
   * `SET_TRADING_STOP` for position-level stops) — the runner
   * warns the user in that case rather than silently binding an
   * empty string.
   */
  data?: {
    orderIds?: string[];
  };
}

/**
 * Command interface - all commands must implement this
 */
export interface Command {
  /** Primary command name */
  readonly name: string;

  /** Alternative names for this command */
  readonly aliases: string[];

  /** Short description for help */
  readonly description: string;

  /** Usage examples */
  readonly examples: string[];

  /** Command category */
  readonly category: CommandCategory;

  /** Whether this command requires an exchange connection */
  readonly requiresConnection: boolean;

  /** Command type - instant (run and return) or suspending (takes over terminal)
   * @default CommandType.INSTANT
   */
  readonly type?: CommandType;

  /** Where the command can be executed from
   * @default CommandAvailability.BOTH
   */
  readonly availability?: CommandAvailability;

  /**
   * Execute the command
   */
  execute(ctx: CommandContext): Promise<CommandResult>;

  /**
   * Optional cleanup when suspending command exits
   */
  cleanup?(): Promise<void>;
}
