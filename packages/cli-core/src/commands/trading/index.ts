// Display commands
export { OrdersCommand } from './OrdersCommand';
export { PositionsCommand } from './PositionsCommand';
export { PositionCommand } from './PositionCommand';
export { MarginCommand } from './MarginCommand';
export { SpotCommand, formatSpotTable, formatMultiAccountSpotTable } from './SpotCommand';

// Order modification commands
export { BumpCommand } from './BumpCommand';
export { MoveCommand } from './MoveCommand';

// Trading commands
export { BuyCommand } from './BuyCommand';
export { SellCommand } from './SellCommand';
export { CancelCommand } from './CancelCommand';
export { CloseCommand } from './CloseCommand';
export { NukeCommand } from './NukeCommand';
export { ScaleCommand } from './ScaleCommand';
export { StopCommand } from './StopCommand';
export { LeverageCommand } from './LeverageCommand';
export { MaxCommand } from './MaxCommand';
export { SwarmCommand } from './SwarmCommand';

// Price commands
export { LastPriceCommand, MarkPriceCommand, IndexPriceCommand } from './PriceCommand';

// Watch mode formatters (wa, wp, wo, wm commands)
export {
  formatPositionsTable,
  formatOrdersTable,
  formatMarginTable,
  formatMultiAccountPositionsTable,
  formatMultiAccountOrdersTable,
  formatMultiAccountMarginTable,
  formatMultiAccountMonitorOutput,
  formatAccountMonitorOutput,
  type AccountExchange,
} from './MonitorCommand';

// Chart command with helpers for watch mode
export {
  ChartCommand,
  renderCandleChart,
  formatChartHeader,
  formatPositionLine,
  formatOrderLine,
  formatPositionSummary,
  formatCountdown,
  VALID_TIMEFRAMES,
  TIMEFRAME_MS,
  type ChartLine,
  type ChartWatchParams,
} from './ChartCommand';

// Chase commands with helpers
export {
  ChaseCommand,
  getChaserInstance,
  getActiveChasers,
  renderMiniTickChart,
  formatChaserStatus,
  formatAllChasersStatus,
  type ChaseWatchParams,
} from './ChaseCommand';
export { ChasersCommand } from './ChasersCommand';

// TWAP command
export { TwapCommand } from './TwapCommand';

// Utility commands
export { EchoCommand } from './EchoCommand';

// Base classes
export { TradingBaseCommand } from './TradingBaseCommand';
export { BaseBuySellCommand } from './BaseBuySellCommand';

// Colors
export * from './colors';
