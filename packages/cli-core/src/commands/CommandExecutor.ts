import { serializeCommandError, type SerializedCommandError } from '@tealstreet/safe-cex/src/commandOutcome';
import type { CLIPlatformAdapter, ExchangeLike, TaskAction } from '../types';
import type { CommandContext, CommandResult as CmdResult } from './Command';
import type { PreprocessStore } from '../preprocess/types';
import { commandRegistry } from './CommandRegistry';
import { parseArgs, refusedOrderToken } from './parseArgs';

// Import all commands to register them
import { HelpCommand } from './session/HelpCommand';
import { StatusCommand } from './session/StatusCommand';
import { SymbolCommand } from './session/SymbolCommand';
import { MarketsCommand } from './session/MarketsCommand';
import { ConnectionCommand } from './session/ConnectionCommand';
import { PromptCommand } from './session/PromptCommand';
import { CheckCommand } from './session/CheckCommand';
import { AccountCommand } from './account/AccountCommand';
import { AccountsCommand } from './account/AccountsCommand';
import { ExchangesCommand } from './account/ExchangesCommand';
import {
  OrdersCommand,
  PositionsCommand,
  PositionCommand,
  MarginCommand,
  SpotCommand,
  BumpCommand,
  MoveCommand,
  BuyCommand,
  SellCommand,
  CancelCommand,
  CloseCommand,
  NukeCommand,
  ScaleCommand,
  StopCommand,
  BreakevenCommand,
  TrailCommand,
  LastPriceCommand,
  MarkPriceCommand,
  IndexPriceCommand,
  ChartCommand,
  ChaseCommand,
  ChasersCommand,
  TwapCommand,
  EchoCommand,
  LeverageCommand,
  MaxCommand,
  SwarmCommand,
} from './trading';
import {
  SetCommand,
  SetsCommand,
  TrackCommand,
  UntrackCommand,
  UnsetCommand,
  HistoryCommand,
  PrintCommand,
  AliasCommand,
  UnaliasCommand,
  SleepCommand,
  ClearCommand,
  TasksCommand,
  KillCommand,
  WhitelistCommand,
  UnwhitelistCommand,
  FatfingerCommand,
  UnfatfingerCommand,
  BudgetCommand,
  SimulationCommand,
  WaitCommand,
  WhenCommand,
} from './utility';
import type { PromptConfig } from '../utils/PromptFormatter';

// Register all built-in commands
let commandsRegistered = false;

/** Exported so a caller can ask the registry about a command BEFORE running
 *  one — the CLI's one-shot path needs `requiresConnection` to decide whether a
 *  rejected API key should be terminal, and registration is lazy. */
export function ensureCommandsRegistered(): void {
  if (commandsRegistered) return;

  commandRegistry.registerAll([
    // Session commands
    new HelpCommand(),
    new StatusCommand(),
    new SymbolCommand(),
    new MarketsCommand(),
    new ConnectionCommand(),
    new PromptCommand(),
    new CheckCommand(),

    // Account commands
    new AccountCommand(),
    new AccountsCommand(),
    new ExchangesCommand(),

    // Trading display commands
    new OrdersCommand(),
    new PositionsCommand(),
    new PositionCommand(),
    new MarginCommand(),
    new SpotCommand(),

    // Order modification commands
    new BumpCommand(),
    new MoveCommand(),

    // Trading execution commands
    new BuyCommand(),
    new SellCommand(),
    new CancelCommand(),
    new CloseCommand(),
    new NukeCommand(),
    new ScaleCommand(),
    new StopCommand(),
    new BreakevenCommand(),
    new TrailCommand(),
    new LeverageCommand(),
    new MaxCommand(),
    new SwarmCommand(),

    // Price commands
    new LastPriceCommand(),
    new MarkPriceCommand(),
    new IndexPriceCommand(),

    // Watch commands (chart, chase - wa/wp/wo/wm are handled in repl.ts)
    new ChartCommand(),
    new ChaseCommand(),
    new ChasersCommand(),
    new TwapCommand(),

    // Utility commands
    new EchoCommand(),
    new SetCommand(),
    new SetsCommand(),
    new TrackCommand(),
    new UntrackCommand(),
    new UnsetCommand(),
    new PrintCommand(),
    new AliasCommand(),
    new UnaliasCommand(),
    new SleepCommand(),
    new ClearCommand(),
    new TasksCommand(),
    new KillCommand(),
    new WhitelistCommand(),
    new UnwhitelistCommand(),
    new FatfingerCommand(),
    new UnfatfingerCommand(),
    new BudgetCommand(),
    new SimulationCommand(),
    new WaitCommand(),
    new WhenCommand(),
    new HistoryCommand(),
  ]);

  commandsRegistered = true;
}

/**
 * Result from executing a command
 */
export interface ExecuteResult {
  /** Whether the command was handled (false means pass to trading router) */
  handled: boolean;
  /** Whether the handled command succeeded. Undefined when handled=false. */
  success?: boolean;
  /** Error message from a handled command failure. */
  error?: string;
  /** Known native rejection or interrupted-command attribution, when available. */
  errorDetails?: SerializedCommandError;
  /**
   * The executor already printed this error to the user.
   *
   * A host is contractually required to treat `success === false` as a failure
   * (see cli-core/CLAUDE.md, "Runner failure contract") and apps/cli converts
   * it to a throw so `retry` / `set ec` work — which then printed it a SECOND
   * time from its own catch. One failed order announced itself as
   * "Command failed: X" followed by "Error in command: X". Hosts should skip
   * their own reporting when this is set.
   */
  errorReported?: boolean;
  /** Whether session state changed */
  sessionChanged?: boolean;
  /** New session values if changed */
  newSession?: {
    currentSymbol?: string;
    currentExchange?: ExchangeLike | null;
    currentAccountName?: string;
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
   * Structured payload from the inner CommandResult. Used by the
   * runner to fulfil `@<var> = <body>` capture: when `step.captureTo`
   * is set, the runner reads `data.orderIds` and writes the
   * comma-joined list into the named user var.
   */
  data?: CmdResult['data'];
}

/**
 * Execute a CLI command
 *
 * @param input - The raw command string
 * @param symbol - Current trading symbol
 * @param accountName - Current account name
 * @param exchange - Current exchange instance
 * @param adapter - Platform adapter for I/O
 * @param promptConfig - Current prompt configuration
 * @param preprocessStore - User vars + aliases store; required for
 *   `set`/`unset`/`print`/`alias`/`unalias` commands. Undefined means
 *   those commands return an error.
 * @returns Result indicating if command was handled and any session changes
 */
export async function executeCommand(
  input: string,
  symbol: string,
  accountName: string | undefined,
  exchange: ExchangeLike | null,
  adapter: CLIPlatformAdapter,
  promptConfig?: PromptConfig,
  preprocessStore?: PreprocessStore,
): Promise<ExecuteResult> {
  // Ensure commands are registered
  ensureCommandsRegistered();

  const trimmed = input.trim();
  if (!trimmed) {
    return { handled: true };
  }

  // Parse the command
  const args = parseArgs(trimmed);
  const positionalArgs = args._;

  if (!positionalArgs[0]) {
    return { handled: true };
  }

  // Try to find command - check multi-word aliases first (e.g., "tealstreet pair")
  // then fall back to single word
  let command = null;
  let commandWordCount = 0;

  // Try matching first two words as command (for aliases like "tealstreet pair")
  if (positionalArgs.length >= 2) {
    const twoWordName = `${positionalArgs[0]} ${positionalArgs[1]}`;
    command = commandRegistry.find(twoWordName);
    if (command) {
      commandWordCount = 2;
    }
  }

  // Fall back to single word command
  if (!command) {
    command = commandRegistry.find(positionalArgs[0]);
    if (command) {
      commandWordCount = 1;
    }
  }

  // Bare-instrument shorthand: typing `BTCUSDT` (or any other known
  // market on the active exchange) acts as `switch BTCUSDT`. Only kicks
  // in when (a) the input is a single positional token, (b) we're
  // connected, and (c) the token matches a market — so a typo like
  // `buyy 100$` still falls through to "unknown command" rather than
  // getting absorbed silently.
  let bareInstrumentSymbol: string | null = null;
  if (!command && positionalArgs.length === 1 && exchange) {
    const raw = positionalArgs[0];
    const upper = raw.toUpperCase();
    const markets = exchange.store?.markets ?? [];
    const hit = markets.find((m) => m.symbol === upper || m.symbol === raw);
    if (hit) {
      command = commandRegistry.find('symbol');
      bareInstrumentSymbol = hit.symbol;
    }
  }

  if (!command) {
    // Not a registered command - pass to trading router
    return { handled: false };
  }

  const refusal = refusedOrderToken(command.name, trimmed);
  if (refusal) {
    adapter.printError(refusal);
    return { handled: true, success: false, error: refusal, errorReported: true };
  }

  // Adjust args: keep command name at index 0, followed by remaining args
  // This matches BaseCommand.getArg() contract where 0 = command name, 1 = first arg
  const adjustedArgs = bareInstrumentSymbol
    ? { ...args, _: ['symbol', bareInstrumentSymbol] }
    : {
        ...args,
        _: [command.name, ...positionalArgs.slice(commandWordCount)],
      };

  // Build context
  const ctx: CommandContext = {
    raw: trimmed,
    args: adjustedArgs,
    symbol,
    accountName,
    exchange,
    adapter,
    promptConfig,
    preprocessStore,
  };

  // Check connection requirement
  if (command.requiresConnection && !exchange) {
    const error = 'Not connected to any exchange. Use "account <name>" to connect.';
    adapter.printError(error);
    return { handled: true, success: false, error, errorReported: true };
  }

  // Execute the command
  try {
    if (command.requiresConnection) exchange?.syncAccountState?.();
    const result = await command.execute(ctx);
    return convertResult(result);
  } catch (error) {
    const message = (error as Error).message;
    // The exchange 'error' channel is filtered and deduplicated for account health,
    // so it cannot stand in for the command's own result line.
    adapter.printError(`Command failed: ${message}`);
    return {
      handled: true,
      success: false,
      error: message,
      errorReported: true,
      ...(error instanceof Error && { errorDetails: serializeCommandError(error) }),
    };
  }
}

/**
 * Convert command result to execute result
 */
function convertResult(result: CmdResult): ExecuteResult {
  const execResult: ExecuteResult = {
    handled: true,
    success: result.success,
  };
  if (!result.success && result.error) {
    execResult.error = result.error;
  }

  if (result.updateSession) {
    // Only include fields that are actually defined to avoid overwriting
    // existing session values with undefined
    const newSession: ExecuteResult['newSession'] = {};
    if (result.updateSession.symbol !== undefined) {
      newSession.currentSymbol = result.updateSession.symbol;
    }
    if (result.updateSession.exchange !== undefined) {
      newSession.currentExchange = result.updateSession.exchange;
    }
    if (result.updateSession.accountName !== undefined) {
      newSession.currentAccountName = result.updateSession.accountName;
    }
    if (result.updateSession.promptConfig !== undefined) {
      newSession.promptConfig = result.updateSession.promptConfig;
    }

    execResult.sessionChanged = true;
    execResult.newSession = newSession;
  }

  // Pass through account action if present
  if (result.accountAction) {
    execResult.accountAction = result.accountAction;
  }

  // Pass through task action if present
  if (result.taskAction) {
    execResult.taskAction = result.taskAction;
  }

  // Pass through structured data (used by the runner to fulfil
  // `@<var> = <body>` capture).
  if (result.data) {
    execResult.data = result.data;
  }

  return execResult;
}

/**
 * Check if a command is registered (for testing/introspection)
 */
export function isRegisteredCommand(name: string): boolean {
  ensureCommandsRegistered();
  return commandRegistry.has(name);
}

/**
 * Get all registered command names
 */
export function getRegisteredCommandNames(): string[] {
  ensureCommandsRegistered();
  return commandRegistry.getAll().map((cmd) => cmd.name);
}
