/**
 * Record Command - Unified command for market data recording
 *
 * Subcommands:
 *   record start <exchange>    - Start recording
 *   record stop <exchange>     - Stop recording
 *   record list                - List all recordings
 *   record stats [exchange]    - Show statistics
 *
 * Shorthand:
 *   record <exchange>          - Same as: record start <exchange>
 */

import {
  BaseCommand,
  CommandCategory,
  type CommandContext,
  // NewCommandResult is the BaseCommand-aligned shape; public `CommandResult`
  // from cli-core still points at the legacy type and is incompatible with
  // BaseCommand.execute.
  type NewCommandResult as CommandResult,
} from '@tealstreet/cli-core';

export class RecordCommand extends BaseCommand {
  readonly name = 'record';
  readonly description = 'Record market data (orderbook, trades, etc.)';
  readonly category = CommandCategory.Session;
  readonly aliases = ['rec'];
  readonly examples = [
    'record start bybit',
    'record start bybit --symbol ETHUSDT --interval 30s',
    'record stop bybit',
    'record list',
    'record stats',
    'record stats bybit',
    'record bybit  (shorthand for: record start bybit)',
  ];
  readonly requiresConnection = false;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    const subcommand = this.getArg(ctx, 1);

    if (!subcommand) {
      return this.showHelp(ctx);
    }

    switch (subcommand.toLowerCase()) {
      case 'start':
      case 's':
        return this.handleStart(ctx);
      case 'stop':
        return this.handleStop(ctx);
      case 'list':
      case 'ls':
        return this.handleList(ctx);
      case 'stats':
        return this.handleStats(ctx);
      case '--help':
      case '-h':
        return this.showHelp(ctx);
      default:
        // Assume it's an exchange name (shorthand for start)
        return this.handleStart(ctx, subcommand);
    }
  }

  /**
   * Show help for record command
   */
  private showHelp(ctx: CommandContext): CommandResult {
    this.printInfo(ctx, '\nUsage: record <subcommand> [options]\n');
    this.printInfo(ctx, 'Subcommands:');
    this.printInfo(ctx, '  start <exchange>    Start recording market data');
    this.printInfo(ctx, '  stop <exchange>     Stop recording');
    this.printInfo(ctx, '  list                List all recordings');
    this.printInfo(ctx, '  stats [exchange]    Show recording statistics');
    this.printInfo(ctx, '');
    this.printInfo(ctx, 'Shorthand:');
    this.printInfo(ctx, '  record <exchange>   Same as: record start <exchange>');
    this.printInfo(ctx, '');
    this.printInfo(ctx, 'Examples:');
    this.printInfo(ctx, '  record start bybit');
    this.printInfo(ctx, '  record start bybit --symbol ETHUSDT --interval 30s');
    this.printInfo(ctx, '  record stop bybit');
    this.printInfo(ctx, '  record list');
    this.printInfo(ctx, '');
    return this.success();
  }

  /**
   * Handle: record start <exchange> [options]
   */
  private async handleStart(ctx: CommandContext, exchangeArg?: string): Promise<CommandResult> {
    // If exchangeArg provided, it means we're in shorthand mode (record bybit)
    // Otherwise, exchange is at position 2 (record start bybit)
    const exchange = exchangeArg || this.getArg(ctx, 2);

    if (!exchange) {
      this.printError(ctx, 'Exchange required. Usage: record start <exchange>');
      this.printInfo(ctx, 'Example: record start bybit');
      return this.error('Missing exchange argument');
    }

    // Parse options
    const symbol = this.getOption(ctx, 'symbol', 's') || 'BTCUSDT';
    const intervalStr = this.getOption(ctx, 'interval', 'i') || '1m';
    const retentionStr = this.getOption(ctx, 'retention', 'r') || '7d';
    const levelsStr = this.getOption(ctx, 'levels', 'l') || '25';
    const dataType = (this.getOption(ctx, 'type', 't') || 'orderbook') as 'orderbook' | 'trades' | 'funding';

    try {
      const { initDatabase, startRecorder, parseInterval, parseRetention } = await import('../../record');

      const intervalMs = parseInterval(intervalStr);
      const retentionDays = parseRetention(retentionStr);
      const levels = parseInt(levelsStr, 10);

      initDatabase();

      this.printInfo(ctx, `Starting ${dataType} recorder for ${exchange}:${symbol}`);
      this.printInfo(ctx, `  Interval: ${intervalStr} (${intervalMs}ms)`);
      this.printInfo(ctx, `  Retention: ${retentionStr} (${retentionDays} days)`);
      if (dataType === 'orderbook') {
        this.printInfo(ctx, `  Levels: ${levels}`);
      }

      const { recorder, replaced } = await startRecorder({
        dataType,
        exchange: exchange.toLowerCase(),
        symbol,
        intervalMs,
        retentionDays,
        levels,
      });

      if (replaced) {
        this.printWarning(ctx, `Replaced existing ${dataType}:${exchange}:${symbol} recorder`);
      }

      this.printSuccess(ctx, `Recorder started for ${exchange}:${symbol}`);
      this.printInfo(ctx, 'Use "record list" to see active recordings.');

      return this.success(`Started recording ${dataType} for ${exchange}:${symbol}`);
    } catch (error) {
      this.printError(ctx, `Failed to start recorder: ${(error as Error).message}`);
      return this.error((error as Error).message);
    }
  }

  /**
   * Handle: record stop <exchange> [options]
   */
  private async handleStop(ctx: CommandContext): Promise<CommandResult> {
    const exchange = this.getArg(ctx, 2);

    if (!exchange) {
      this.printError(ctx, 'Exchange required. Usage: record stop <exchange>');
      return this.error('Missing exchange argument');
    }

    const symbol = this.getOption(ctx, 'symbol', 's') || 'BTCUSDT';
    const dataType = (this.getOption(ctx, 'type', 't') || 'orderbook') as 'orderbook' | 'trades' | 'funding';

    try {
      const { initDatabase, stopRecorder } = await import('../../record');

      initDatabase();

      const stopped = await stopRecorder(dataType, exchange.toLowerCase(), symbol);

      if (stopped) {
        this.printSuccess(ctx, `Stopped recording ${dataType}:${exchange}:${symbol}`);
        return this.success('Stopped recording');
      } else {
        this.printWarning(ctx, `No active recorder found for ${dataType}:${exchange}:${symbol}`);
        return this.success('No recorder to stop');
      }
    } catch (error) {
      this.printError(ctx, `Failed to stop recorder: ${(error as Error).message}`);
      return this.error((error as Error).message);
    }
  }

  /**
   * Handle: record list
   */
  private async handleList(ctx: CommandContext): Promise<CommandResult> {
    try {
      const { initDatabase, getRecordings, getActiveRecorders } = await import('../../record');

      initDatabase();

      const configs = getRecordings();
      const activeRecorders = getActiveRecorders();

      if (configs.length === 0 && activeRecorders.length === 0) {
        this.printInfo(ctx, 'No recordings configured');
        return this.success();
      }

      // Build table data
      const rows: string[][] = [];
      const headers = ['Status', 'Type', 'Exchange', 'Symbol', 'Interval', 'Retention', 'Snapshots'];

      // Add active recorders
      for (const recorder of activeRecorders) {
        const status = recorder.getStatus();
        rows.push([
          '🟢 Running',
          status.dataType,
          status.exchange,
          status.symbol,
          `${status.intervalMs / 1000}s`,
          `${status.retentionDays}d`,
          status.snapshotCount.toLocaleString(),
        ]);
      }

      // Add inactive configs (not currently running). `exchange` and `symbol`
      // are `protected` on BaseRecorder, so reach them via the public
      // `getStatus()` snapshot rather than the protected fields directly.
      for (const config of configs) {
        const isRunning = activeRecorders.some((r) => {
          const status = r.getStatus();
          return (
            status.dataType === config.dataType &&
            status.exchange === config.exchange &&
            status.symbol === config.symbol
          );
        });

        if (!isRunning) {
          rows.push([
            config.active ? '⏸️  Paused' : '⭕ Inactive',
            config.dataType,
            config.exchange,
            config.symbol,
            `${config.intervalMs / 1000}s`,
            `${config.retentionDays}d`,
            '-',
          ]);
        }
      }

      // Print table
      this.printInfo(ctx, '\nRecordings:\n');
      this.printTable(ctx, headers, rows);
      this.printInfo(ctx, '');

      return this.success();
    } catch (error) {
      this.printError(ctx, `Failed to list recordings: ${(error as Error).message}`);
      return this.error((error as Error).message);
    }
  }

  /**
   * Handle: record stats [exchange]
   */
  private async handleStats(ctx: CommandContext): Promise<CommandResult> {
    const exchangeFilter = this.getArg(ctx, 2); // Optional exchange filter

    try {
      const { initDatabase, getDatabaseStats, getDatabasePath, getActiveRecorders } = await import('../../record');

      initDatabase();

      const stats = getDatabaseStats();
      const activeRecorders = getActiveRecorders();

      this.printInfo(ctx, '\nRecording Statistics:\n');
      this.printInfo(ctx, `  Database: ${getDatabasePath()}`);
      this.printInfo(ctx, `  Size: ${stats.databaseSize}`);
      this.printInfo(ctx, `  Active Recorders: ${activeRecorders.length}`);
      this.printInfo(ctx, `  Total Snapshots: ${stats.totalSnapshots.toLocaleString()}`);
      this.printInfo(ctx, '');

      for (const [type, typeStats] of Object.entries(stats.byType)) {
        // Filter by exchange if specified
        const exchanges = exchangeFilter
          ? typeStats.exchanges.filter((e) => e.toLowerCase() === exchangeFilter.toLowerCase())
          : typeStats.exchanges;

        if (exchangeFilter && exchanges.length === 0) {
          continue; // Skip this type if exchange filter doesn't match
        }

        this.printInfo(ctx, `  ${type}:`);
        this.printInfo(ctx, `    Count: ${typeStats.count.toLocaleString()}`);
        this.printInfo(ctx, `    Exchanges: ${exchanges.join(', ') || 'none'}`);
      }
      this.printInfo(ctx, '');

      return this.success();
    } catch (error) {
      this.printError(ctx, `Failed to get stats: ${(error as Error).message}`);
      return this.error((error as Error).message);
    }
  }

  /**
   * Print a simple ASCII table
   */
  private printTable(ctx: CommandContext, headers: string[], rows: string[][]): void {
    // Calculate column widths
    const colWidths = headers.map((h, i) => {
      const maxDataWidth = Math.max(...rows.map((r) => (r[i] || '').length));
      return Math.max(h.length, maxDataWidth);
    });

    // Format row
    const formatRow = (cells: string[]): string => {
      return cells.map((cell, i) => (cell || '').padEnd(colWidths[i])).join('  ');
    };

    // Print header
    this.printInfo(ctx, formatRow(headers));
    this.printInfo(ctx, colWidths.map((w) => '-'.repeat(w)).join('  '));

    // Print rows
    for (const row of rows) {
      this.printInfo(ctx, formatRow(row));
    }
  }
}
