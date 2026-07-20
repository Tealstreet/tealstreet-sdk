// Types
export type {
  AccountConfig,
  ExchangeAccount,
  ExchangeAccountConfig,
  CLISession,
  CommandResult,
  CLIPlatformAdapter,
  ExchangeInfo,
  ExchangeLike,
  LastReceivedMessage,
  CommandExecutionContext,
  // Task types
  TaskType,
  TaskOrigin,
  TaskStatus,
  TwapTaskParams,
  ChaserTaskParams,
  DcaTaskParams,
  UnifiedTaskState,
  TaskAction,
} from './types';

// Enums
export { CommandType, CommandAvailability, ExecutionMode } from './types';

// Re-export exchange creation utilities from safe-cex
export {
  createExchangeFromAccount,
  accountToExchangeOptions,
} from '@tealstreet/safe-cex/src/createExchangeFromAccount';

// Exchange info
export { SUPPORTED_EXCHANGES, getSupportedExchangeNames, getExchangeInfo } from './exchanges';

// Legacy command processing (deprecated - use executeCommand)
export { processSessionCommand, connectToAccount } from './legacyCommands';

// New command architecture
export {
  // Core
  BaseCommand,
  CommandCategory,
  CommandRegistry,
  commandRegistry,
  parseArgs,

  // Executor
  executeCommand,
  isRegisteredCommand,
  getRegisteredCommandNames,

  // Types
  type Command,
  type CommandContext,
  type CommandResult as NewCommandResult,
  type ParsedArgs,
  type ExecuteResult,
} from './commands';

// Utilities
export {
  ConnectionMonitor,
  getConnectionStatus,
  formatConnectionStatusLine,
  formatPrompt,
  simplePrompt,
  getPromptPreset,
  listPromptPresets,
  PROMPT_PRESETS,
  NetworkMonitor,
  type ConnectionHealth,
  type ConnectionStatus,
  type MonitorCallback,
  type PromptOptions,
  type PromptInfo,
  type PromptConfig,
  type PromptToken,
  type NetworkMonitorCallback,
} from './utils';

// Table formatting
export {
  setTableRenderMode,
  getTableRenderMode,
  renderTable,
  cell,
  separatorRow,
  type TableRenderMode,
  type TableData,
  type TableColumn,
  type TableRow,
  type TableCell,
  type TextStyle,
} from './table';

// Preprocess layer (Phase 1 of the CLI v2 alignment plan — see
// docs/CLI_v2.md). Substitutes constants/vars/aliases, expands repeat
// blocks, splits chains on ; and &, flags `sleep` as a blocking step.
export {
  preprocessCommand,
  preprocessSegment,
  resolveDeferredCaptures,
  parseDuration,
  expandRepeats,
  splitChain,
  MemoryPreprocessStore,
  MAX_SLEEP_MS,
  MAX_REPEAT_COUNT,
  MAX_REPEAT_DEPTH,
  MAX_ALIAS_DEPTH,
  FULL_PASSTHROUGH_COMMANDS,
  SET_COMMAND,
  type PreprocessStore,
  type PreprocessContext,
  type PreprocessedStep,
  type PreprocessResult,
  type PreprocessWarning,
  type UITokenResolvers,
  type ChainOp,
  type ChainSegment,
  type TrackedVarEntry,
  type TrackedVarKind,
} from './preprocess';

// Terminal management and UnifiedExecutor are CLI-only (use Node.js APIs)
// Import directly from '@tealstreet/cli-core/src/terminal' or '@tealstreet/cli-core/src/executor'
// in the CLI app instead of through the main package export
