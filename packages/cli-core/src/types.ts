import type {
  ExchangeName,
  Market,
  Order,
  Position,
  Ticker,
  Balance,
  StoreDataLoaded,
  UpdateOrderOpts,
  ExchangeAccountConfig,
  PlaceOrderOpts,
  Candle,
  OHLCVOptions,
  OrderBook,
  PositionSide,
  NukeResult,
} from '@tealstreet/safe-cex/src/types';

// Re-export types from safe-cex
export type { ExchangeName, UpdateOrderOpts, ExchangeAccountConfig, PlaceOrderOpts, Candle, OHLCVOptions, OrderBook };
// Alias for backwards compatibility
export type { ExchangeAccountConfig as ExchangeAccount } from '@tealstreet/safe-cex/src/types';

/**
 * Types of message handlers for connection monitoring
 */
export type MessageHandlerStatsType = 'public' | 'private' | 'futures' | 'business';

/**
 * Last received message timestamps for connection monitoring
 */
export type LastReceivedMessage = Partial<Record<MessageHandlerStatsType, number>>;

/**
 * WebSocket-like interface with lastReceived timestamp
 */
export interface WebSocketLike {
  lastReceived?: number;
  reconnect?: () => void;
}

/**
 * Minimal exchange interface - used to allow both web and standalone exchanges
 * to work with the same code. This is a subset of BaseExchange.
 */
export interface ExchangeLike {
  name: string;
  /** Synchronize native private state before making a store-based decision. */
  syncAccountState?: () => void;
  store?: {
    markets?: Market[];
    orders?: Order[];
    positions?: Position[];
    tickers?: Ticker[];
    balance?: Balance;
    loaded?: StoreDataLoaded;
    lastReceivedMessage?: LastReceivedMessage;
    latency?: number;
    /** WS health, set by BaseExchange. Read by the trading health gate
     *  to block exposure-increasing writes when the private stream is
     *  stale (TSBUG-106). */
    socketHealth?: {
      publicHealthy: boolean;
      privateHealthy: boolean;
      publicInitialized: boolean;
      privateInitialized: boolean;
      publicLastMessage?: number;
      privateLastMessage?: number;
    };
  };
  /** Non-zero once the venue has rejected the credentials. The one-shot path
   *  reads it to make a rejected key terminal instead of printing a zeroed
   *  table that reads as an empty account. */
  authFailureStreak?: number;
  emitter: {
    on: (event: string, handler: (...args: unknown[]) => void) => void;
    off: (event: string, handler: (...args: unknown[]) => void) => void;
    emit: (event: string, ...args: unknown[]) => void;
  };
  // Order management methods (optional, for trading commands)
  placeOrder?: (opts: PlaceOrderOpts) => Promise<string[]>;
  placeOrders?: (opts: PlaceOrderOpts[]) => Promise<string[]>;
  updateOrder?: (opts: UpdateOrderOpts) => Promise<string[]>;
  cancelOrders?: (orders: Order[]) => Promise<void>;
  cancelSymbolOrders?: (symbol: string) => Promise<void>;
  // Nuke methods
  nuke?: (retries?: number) => Promise<void | NukeResult>;
  nukePositions?: (retries?: number) => Promise<void | NukeResult>;
  nukeOrders?: () => Promise<void | NukeResult>;
  // Leverage
  /** Third argument names one leg of a hedge-mode position; safe-cex accepts
   *  it and ignores it where the venue has no per-side leverage. */
  setLeverage?: (symbol: string, leverage: number, side?: PositionSide) => Promise<void>;
  setAllLeverage?: (leverage: number) => Promise<void>;
  // OHLCV data
  fetchOHLCV?: (opts: OHLCVOptions) => Promise<Candle[]>;
  // Public orderbook stream
  listenOrderBook?: (symbol: string, callback: (orderbook: OrderBook) => void) => () => void;
  // Connection management (optional)
  getAllSockets?: () => Array<WebSocketLike | undefined>;
  // Direct WebSocket access (for standalone CLI without postprocessors)
  publicWebsocket?: WebSocketLike;
  privateWebsocket?: WebSocketLike;
  // Lifecycle methods
  start?: () => Promise<void>;
  dispose?: () => void;
  // Network status (for offline detection)
  setOnlineStatus?: (isOnline: boolean) => void;
}

/**
 * Account configuration - stored in config file (standalone) or settings (web)
 * @deprecated Use ExchangeAccountConfig from @tealstreet/safe-cex instead
 */
export type AccountConfig = ExchangeAccountConfig;

/**
 * CLI session state
 */
export interface CLISession {
  currentExchange: ExchangeLike | null;
  currentAccountName: string | undefined;
  currentSymbol: string;
}

/**
 * Result of processing a command
 */
export interface CommandResult {
  handled: boolean;
  output?: string[];
  error?: string;
  /** If true, the session state was modified */
  sessionChanged?: boolean;
  /** New session state if changed */
  newSession?: Partial<CLISession>;
}

/**
 * Platform type - determines available features
 */
export type PlatformType = 'standalone' | 'embedded';

/**
 * Platform adapter - each platform (web/standalone) implements this
 */
export interface CLIPlatformAdapter {
  /** Platform type - 'standalone' for terminal CLI, 'embedded' for web widget */
  platformType?: PlatformType;

  // Account management
  listAccounts(): ExchangeAccountConfig[];
  getAccount(name?: string): ExchangeAccountConfig | undefined;
  addAccount(account: ExchangeAccountConfig): void;
  updateAccount(name: string, updates: Partial<ExchangeAccountConfig>): boolean;
  removeAccount(name: string): boolean;
  setDefaultAccount(name: string): boolean;
  getDefaultSymbol(): string | undefined;
  setDefaultSymbol(symbol: string): void;
  getActiveAccounts(): ExchangeAccountConfig[];

  // Exchange management
  createExchange(account: ExchangeAccountConfig): Promise<ExchangeLike>;
  startExchange(exchange: ExchangeLike): Promise<void>;
  stopExchange(exchange: ExchangeLike): Promise<void>;
  /** Get exchange instance by account name (returns undefined if not connected) */
  getExchange?(accountName: string): ExchangeLike | undefined;
  /** Check if an account is connected */
  isAccountConnected?(accountName: string): boolean;

  // Output
  print(lines: string[]): void;
  printError(message: string): void;
  printSuccess(message: string): void;
  printWarning(message: string): void;
  printInfo(message: string): void;

  /** Yes/no confirmation prompt. Standalone CLI implements via readline. Web omits.
   *  Headless adapters may auto-deny or auto-approve from a preconfigured allowlist. */
  promptYesNo?(message: string, options?: { timeoutMs?: number }): Promise<boolean>;

  /**
   * Run a command chain the way the user typing it would — preprocessing,
   * alias expansion, `;` / `&` operators, the runner's failure contract and
   * the host's write-op audit trail.
   *
   * `when` fires through this rather than through `executeCommand`, which does
   * none of that: a trigger's order must be audited and substituted exactly
   * like a typed one, and `taskAction` results (a fired `chase` or `twap`)
   * only reach a task manager on the host's path.
   *
   * `scope` pins the run to the symbol and account the trigger was ARMED on.
   * Without it a fired command reads the session's CURRENT symbol, so arming on
   * BTCUSDT and then switching to ETHUSDT would move the wrong position's stop.
   *
   * OPTIONAL, and its absence is meaningful — `when` refuses to arm rather
   * than arming something that can never fire. A one-shot invocation exits as
   * soon as its command returns, so it must not supply this.
   */
  runChain?(input: string, scope?: { symbol?: string; account?: string }): Promise<void>;

  /**
   * Get recent commands the user has typed, newest first. Used by
   * the `history` command. Sources differ per runtime:
   *   - standalone CLI: a small in-memory ring buffer maintained by
   *     repl.ts (pushed on each input).
   *   - web terminal: the per-tab `cliHistoryAtom`.
   * Returns up to `limit` entries (newest first). When the adapter
   * doesn't implement this, the command reports "history not
   * available".
   */
  getCommandHistory?(limit?: number): string[];
}

/**
 * Exchange info for config
 */
export type { ExchangeAccountCredentialSchema as ExchangeInfo } from '@tealstreet/safe-cex/src/accounts/buildExchangeAccountConfig';

// ============================================================================
// Command Execution Types
// ============================================================================

/**
 * Command type - determines how the command interacts with the terminal
 */
export enum CommandType {
  /** Non-suspending: runs and returns immediately (accounts, help, trading) */
  INSTANT = 'instant',
  /** Suspending: takes over terminal until user exits (watch, serve, pair) */
  SUSPENDING = 'suspending',
}

/**
 * Command availability - where the command can be executed from
 */
export enum CommandAvailability {
  /** Only available in REPL mode (e.g., some watch modes) */
  REPL_ONLY = 'repl_only',
  /** Only available as CLI args (currently none) */
  CLI_ONLY = 'cli_only',
  /** Available in both modes (serve, pair, accounts, etc.) */
  BOTH = 'both',
}

/**
 * Execution mode - context in which a command is being executed
 */
export enum ExecutionMode {
  /** Running from command line: `tealstreet serve` */
  CLI_ARGS = 'cli_args',
  /** Running from REPL prompt: `serve` */
  REPL = 'repl',
}

/**
 * Extended command context with execution mode and exit handling
 */
export interface CommandExecutionContext {
  /** The execution mode (CLI args or REPL) */
  mode: ExecutionMode;
  /** Call this to signal the command should exit (for suspending commands) */
  onExit: () => void;
  /** Resolves when exit is requested (for suspending commands to await) */
  exitSignal: Promise<void>;
}

// ============================================================================
// Unified Task Types (TWAP, Chaser, DCA)
// ============================================================================

/**
 * Task type - type of long-running job
 */
export type TaskType = 'twap' | 'chaser' | 'dca';

/**
 * Task origin - where the task was initiated from
 */
export type TaskOrigin = 'web' | 'cli' | 'webhook' | 'automation';

/**
 * Task status
 */
export type TaskStatus = 'pending' | 'running' | 'paused' | 'completed' | 'cancelled' | 'failed';

/**
 * TWAP-specific parameters
 */
export interface TwapTaskParams {
  totalSize: number;
  duration: number; // ms
  slices: number;
  ordersExecuted: number;
  nextOrderAt?: number;
  useChaserOrders?: boolean;
  reduceOnly?: boolean;
  /** `twap chase` only: % of slice interval before per-slice timeout. */
  timeoutPct?: number;
  /** What to do when a slice's chaser times out. */
  timeoutAction?: 'market' | 'next';
  /** What to do when a slice's chaser fails (cancels/maxes out). */
  failAction?: 'market' | 'next';
}

/**
 * Chaser-specific parameters
 */
export interface ChaserTaskParams {
  size: number;
  reduceOnly: boolean;
  attempts: number;
  maxAttempts: number;
  currentOrderIds: string[];
  delay: number; // ms between attempts
}

/**
 * DCA-specific parameters
 */
export interface DcaTaskParams {
  amountPerPeriod: number;
  interval: number; // ms
  maxExecutions?: number;
  executions: number;
  totalFilled: number;
}

/**
 * Unified task state - same schema across all execution contexts (web worker, CLI REPL, webhooks)
 */
export interface UnifiedTaskState {
  // Identity
  id: string;
  type: TaskType;
  origin: TaskOrigin;

  // Common fields
  symbol: string;
  side: 'buy' | 'sell';
  account?: string;
  status: TaskStatus;
  createdAt: number;
  startedAt?: number;
  completedAt?: number;

  // Progress
  progress: {
    filled: number;
    total: number;
    percentage: number;
  };

  // Type-specific params (discriminated union based on type)
  params: TwapTaskParams | ChaserTaskParams | DcaTaskParams;

  // Optional: error info
  error?: string;
}

/**
 * Task action - returned by commands to trigger task operations
 */
export interface TaskAction {
  action: 'start' | 'stop' | 'pause' | 'resume' | 'list';
  type?: TaskType;
  taskId?: string; // For stop/pause/resume
  taskIds?: string[]; // For batch operations
  params?: Partial<UnifiedTaskState>; // For start action
}
