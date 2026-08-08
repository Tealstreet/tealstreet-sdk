#!/usr/bin/env bun
// Shim WebSocket for tealstreet.io proxy connections (must be first import)
import './ws-shim';
import { Command } from 'commander';
import chalk from 'chalk';
import type { ExchangeName } from '@tealstreet/safe-cex/src/types';
import {
  loadConfig,
  addAccount,
  removeAccount,
  listAccounts,
  setDefaultAccount,
  setDefaultSymbol,
  getConfigDir,
  type AccountConfig,
} from './config';
import { interceptConsole, logger } from './logger';
import { isCliExitKey, getCliExitMessage } from './utils/exit-keys';
import { fireCheckin } from './checkin';
import { installStdoutGuard, isMcpInvocation } from './mcp/stdoutGuard';

// MCP stdio frames JSON-RPC over stdout, so nothing else may touch it. Detect
// the mode from argv here — the noisy writers below run at module top level,
// long before commander has parsed anything.
const RUNNING_AS_MCP = isMcpInvocation();

// Start console interception early to catch all logs
interceptConsole();

if (RUNNING_AS_MCP) installStdoutGuard();

// Announce this process to the web app so the web's CliListenerWorker
// gate knows there's a CLI on this platform. Fire-and-forget — no
// network awaited, all errors swallowed; safe to run unconditionally.
// See apps/cli/src/checkin.ts for the gating + dedupe contract.
fireCheckin();

// Global error handlers to prevent crashes and verbose stack traces
process.on('uncaughtException', (error: Error) => {
  logger.error(`Uncaught exception: ${error.message}`, error.stack);
  console.error(error.message);
  process.exit(1);
});

process.on('unhandledRejection', (reason: unknown) => {
  const message = reason instanceof Error ? reason.message : String(reason);
  const stack = reason instanceof Error ? reason.stack : undefined;
  logger.error(`Unhandled rejection: ${message}`, stack);
  // Don't exit on unhandled rejection - let the app continue
  // The error will be logged and a clean message shown to user
});
// Exchange info inlined to avoid circular dependency issues
type ExchangeInfo = {
  displayName: string;
  requiredFields: string[];
  optionalFields: string[];
};

const SUPPORTED_EXCHANGES: Record<string, ExchangeInfo> = {
  bybit: { displayName: 'Bybit', requiredFields: ['apiKey', 'apiSecret'], optionalFields: ['testnet'] },
  binance: { displayName: 'Binance Futures', requiredFields: ['apiKey', 'apiSecret'], optionalFields: ['testnet'] },
  hyperliquid: {
    displayName: 'Hyperliquid',
    requiredFields: ['privateKey', 'walletAddress'],
    optionalFields: ['testnet'],
  },
  blofin: { displayName: 'BloFin', requiredFields: ['apiKey', 'apiSecret', 'passphrase'], optionalFields: [] },
  bitget: { displayName: 'Bitget', requiredFields: ['apiKey', 'apiSecret', 'passphrase'], optionalFields: [] },
  woo: { displayName: 'WOO X', requiredFields: ['apiKey', 'apiSecret'], optionalFields: [] },
  okx: { displayName: 'OKX', requiredFields: ['apiKey', 'apiSecret', 'passphrase'], optionalFields: ['testnet'] },
  okxeu: { displayName: 'OKX EU', requiredFields: ['apiKey', 'apiSecret', 'passphrase'], optionalFields: ['testnet'] },
  bitmex: { displayName: 'BitMEX', requiredFields: ['apiKey', 'apiSecret'], optionalFields: ['testnet'] },
  coinbase: { displayName: 'Coinbase', requiredFields: ['apiKey', 'apiSecret'], optionalFields: [] },
  kraken: { displayName: 'Kraken', requiredFields: ['apiKey', 'apiSecret'], optionalFields: [] },
  kucoin: { displayName: 'KuCoin', requiredFields: ['apiKey', 'apiSecret', 'passphrase'], optionalFields: [] },
  phemex: { displayName: 'Phemex', requiredFields: ['apiKey', 'apiSecret'], optionalFields: ['testnet'] },
  paradex: { displayName: 'Paradex', requiredFields: ['privateKey', 'walletAddress'], optionalFields: ['testnet'] },
};

function getSupportedExchangeNames(): string[] {
  return Object.keys(SUPPORTED_EXCHANGES);
}

function getExchangeInfo(name: string): ExchangeInfo | undefined {
  return SUPPORTED_EXCHANGES[name.toLowerCase()];
}
import { checkForUpdatesBackground, performUpdate, forceCheckForUpdates, getCurrentVersion } from './updater';
import { registerCrossTradeCommand } from './commands/cross-trade';

// Dynamic import for repl to avoid circular dependency issues with safe-cex
const getRepl = async () => import('./repl');

// Check for updates in background (non-blocking).
// Skipped under MCP: it is setImmediate-based and prints a chalk box via
// console.log, which would land on stdout mid-session and corrupt the
// JSON-RPC stream.
if (!RUNNING_AS_MCP) checkForUpdatesBackground();

const program = new Command();

program
  .name('tealstreet')
  .description('Trade crypto from your terminal')
  .version(getCurrentVersion(), '-v, --version', 'Display version number')
  .option('--update', 'Update CLI to the latest version')
  .option('--check-update', 'Check for updates without installing')
  .option('--disable-all', 'Start with all exchanges disabled (skip auto-connecting)');

// ============ Account Commands ============
const accountCmd = program.command('account').alias('accounts').description('Manage accounts');

// List accounts
accountCmd
  .command('list')
  .alias('ls')
  .description('List all configured accounts')
  .action(() => {
    const config = loadConfig();
    const accounts = listAccounts();

    if (accounts.length === 0) {
      console.log(chalk.yellow('No accounts configured.'));
      console.log(chalk.gray('Run: tealstreet account add'));
      return;
    }

    console.log(chalk.cyan('\nConfigured Accounts:\n'));
    accounts.forEach((account) => {
      const isDefault = account.name === config.defaultAccount;
      const prefix = isDefault ? chalk.green('* ') : '  ';
      const suffix = isDefault ? chalk.gray(' (default)') : '';
      console.log(`${prefix}${chalk.bold(account.name)} - ${account.exchange}${suffix}`);
      console.log(chalk.gray(`    API Key: ${maskSecret(account.apiKey || '')}`));
      if (account.testnet) {
        console.log(chalk.yellow('    [testnet]'));
      }
    });

    if (config.defaultSymbol) {
      console.log(chalk.gray(`\nDefault Symbol: ${config.defaultSymbol}`));
    }
    console.log(chalk.gray(`\nConfig location: ${getConfigDir()}`));
  });

// Add account
accountCmd
  .command('add')
  .description('Add a new account')
  .requiredOption('-n, --name <name>', 'Account name (unique identifier)')
  .requiredOption(
    '-e, --exchange <exchange>',
    `Exchange name (${getSupportedExchangeNames().slice(0, 5).join(', ')}...)`,
  )
  .option('-k, --api-key <key>', 'API key')
  .option('-s, --api-secret <secret>', 'API secret')
  .option('-p, --passphrase <passphrase>', 'API passphrase (for exchanges that require it)')
  .option('--private-key <key>', 'Private key (for DEX exchanges)')
  .option('--wallet-address <address>', 'Wallet address (for DEX exchanges)')
  .option('-t, --testnet', 'Use testnet')
  .option('--tealstreet-proxy', 'Route REST through the Tealstreet proxy (required for IP-whitelisted keys)')
  .option('--tealstreet-tunnel', 'Route REST through the Tealstreet tradetunnel')
  .option('--proxy-region <region>', 'Pin the proxy egress region')
  .option('--default', 'Set as default account')
  .action((options) => {
    const exchangeInfo = getExchangeInfo(options.exchange);
    if (!exchangeInfo) {
      console.log(chalk.red(`Unknown exchange: ${options.exchange}`));
      console.log(chalk.gray(`Supported exchanges: ${getSupportedExchangeNames().join(', ')}`));
      return;
    }

    // Validate required fields
    const missingFields: string[] = [];
    for (const field of exchangeInfo.requiredFields) {
      if (field === 'apiKey' && !options.apiKey) missingFields.push('--api-key');
      if (field === 'apiSecret' && !options.apiSecret) missingFields.push('--api-secret');
      if (field === 'passphrase' && !options.passphrase) missingFields.push('--passphrase');
      if (field === 'privateKey' && !options.privateKey) missingFields.push('--private-key');
      if (field === 'walletAddress' && !options.walletAddress) missingFields.push('--wallet-address');
    }

    if (missingFields.length > 0) {
      console.log(chalk.red(`Missing required fields for ${exchangeInfo.displayName}:`));
      console.log(chalk.yellow(`  ${missingFields.join(', ')}`));
      return;
    }

    // wallet-based exchanges store wallet → apiKey, private key → apiSecret (canonical safe-cex shape)
    const isWalletBased = exchangeInfo.requiredFields.includes('privateKey');
    const account: AccountConfig = {
      name: options.name,
      exchange: options.exchange.toLowerCase() as AccountConfig['exchange'],
      apiKey: isWalletBased ? options.walletAddress || '' : options.apiKey || '',
      apiSecret: isWalletBased ? options.privateKey || '' : options.apiSecret || '',
      passphrase: options.passphrase,
      testnet: options.testnet || false,
      // Routing was already honored by platform-adapter but unsettable here, so a
      // CLI-added account always egressed direct — which an IP-whitelisted key rejects.
      ...(options.tealstreetProxy ? { tealstreetProxy: true } : {}),
      ...(options.tealstreetTunnel ? { tealstreetTunnel: true } : {}),
      ...(options.proxyRegion ? { proxyRegion: options.proxyRegion } : {}),
    };

    addAccount(account);
    console.log(chalk.green(`Account "${options.name}" added successfully`));

    if (options.default) {
      setDefaultAccount(options.name);
      console.log(chalk.gray(`Set as default account`));
    }
  });

// Remove account
accountCmd
  .command('remove <name>')
  .alias('rm')
  .description('Remove an account')
  .action((name) => {
    if (removeAccount(name)) {
      console.log(chalk.green(`Account "${name}" removed`));
    } else {
      console.log(chalk.red(`Account "${name}" not found`));
    }
  });

// Set default account
accountCmd
  .command('default <name>')
  .description('Set the default account')
  .action((name) => {
    if (setDefaultAccount(name)) {
      console.log(chalk.green(`Default account set to "${name}"`));
    } else {
      console.log(chalk.red(`Account "${name}" not found`));
    }
  });

// Set default symbol
accountCmd
  .command('symbol <symbol>')
  .description('Set the default trading symbol')
  .action((symbol) => {
    setDefaultSymbol(symbol.toUpperCase());
    console.log(chalk.green(`Default symbol set to "${symbol.toUpperCase()}"`));
  });

// Show supported exchanges
accountCmd
  .command('exchanges')
  .description('List supported exchanges')
  .action(() => {
    console.log(chalk.cyan('\nSupported Exchanges:\n'));
    for (const [name, info] of Object.entries(SUPPORTED_EXCHANGES)) {
      console.log(chalk.bold(`  ${name.padEnd(15)} ${info.displayName}`));
      console.log(chalk.gray(`                 Required: ${info.requiredFields.join(', ')}`));
      if (info.optionalFields.length > 0) {
        console.log(chalk.gray(`                 Optional: ${info.optionalFields.join(', ')}`));
      }
    }
  });

// Import accounts from file
accountCmd
  .command('import <file>')
  .description('Import accounts from JSON file (same format as web app export)')
  .action(async (file) => {
    const { parseImportAccountData } = await import('@tealstreet/safe-cex/src/accounts/parseImportAccountData');
    const { readFileSync, existsSync } = await import('fs');
    const { resolve } = await import('path');

    const filePath = resolve(file);
    if (!existsSync(filePath)) {
      console.log(chalk.red(`File not found: ${filePath}`));
      return;
    }

    let content: string;
    try {
      content = readFileSync(filePath, 'utf-8');
    } catch (error) {
      console.log(chalk.red(`Failed to read file: ${(error as Error).message}`));
      return;
    }

    const result = parseImportAccountData(content);
    if (!result.isValid) {
      console.log(chalk.red(`Invalid import file: ${result.error}`));
      return;
    }

    const accounts = Array.isArray(result.data) ? result.data : [result.data!];
    const config = loadConfig();

    let imported = 0;
    let skipped = 0;

    for (const account of accounts) {
      // Check if account with same name already exists
      const existing = config.accounts.find((a) => a.name === account.name);
      if (existing) {
        console.log(chalk.yellow(`  Skipping "${account.name}" - already exists`));
        skipped++;
        continue;
      }

      // Normalize exchange name to lowercase — input came from a JSON import,
      // so narrowing through `string` then back to `ExchangeName` is honest.
      account.exchange = (account.exchange as string).toLowerCase() as ExchangeName;

      // Validate exchange is supported
      if (!getExchangeInfo(account.exchange)) {
        console.log(chalk.yellow(`  Skipping "${account.name}" - unsupported exchange: ${account.exchange}`));
        skipped++;
        continue;
      }

      addAccount(account);
      console.log(chalk.green(`  Imported "${account.name}" (${account.exchange})`));
      imported++;
    }

    console.log('');
    if (imported > 0) {
      console.log(chalk.green(`Successfully imported ${imported} account(s)`));
    }
    if (skipped > 0) {
      console.log(chalk.yellow(`Skipped ${skipped} account(s)`));
    }
    if (imported === 0 && skipped === 0) {
      console.log(chalk.yellow('No accounts to import'));
    }
  });

// Export accounts to file
accountCmd
  .command('export [file]')
  .description('Export accounts to JSON file')
  .action((file) => {
    const { writeFileSync } = require('fs');
    const { resolve } = require('path');

    const accounts = listAccounts();
    if (accounts.length === 0) {
      console.log(chalk.yellow('No accounts to export'));
      return;
    }

    const outputFile = file ? resolve(file) : resolve('tealstreet-accounts.json');
    try {
      writeFileSync(outputFile, JSON.stringify(accounts, null, 2));
      console.log(chalk.green(`Exported ${accounts.length} account(s) to ${outputFile}`));
    } catch (error) {
      console.log(chalk.red(`Failed to write file: ${(error as Error).message}`));
    }
  });

// Note: 'serve' command has been moved to services/exchange-server.
// Use the standalone exchange-server binary instead of `tealstreet serve`.

// ============ Cross-Trade Commands ============
registerCrossTradeCommand(program);

// ============ Tasks Commands ============
const tasksCmd = program.command('tasks').description('Manage running tasks');

tasksCmd
  .command('list')
  .alias('ls')
  .description('List all tasks')
  .action(async () => {
    const { loadTasks } = await import('./tasks-store');
    const tasks = loadTasks();

    if (tasks.length === 0) {
      console.log(chalk.yellow('No tasks'));
      return;
    }

    console.log(chalk.cyan('\nTasks:\n'));
    for (const task of tasks) {
      const statusColor =
        task.status === 'running'
          ? chalk.green
          : task.status === 'paused'
            ? chalk.yellow
            : task.status === 'completed'
              ? chalk.blue
              : chalk.red;

      console.log(`  ${chalk.bold(task.id.slice(0, 12))}...`);
      console.log(`    Type: ${task.type}`);
      console.log(`    Status: ${statusColor(task.status)}`);
      console.log(`    Created: ${new Date(task.createdAt).toLocaleString()}`);
      if (task.progress && Object.keys(task.progress).length > 0) {
        console.log(`    Progress: ${JSON.stringify(task.progress)}`);
      }
      console.log('');
    }
  });

tasksCmd
  .command('clear')
  .description('Clear all completed/failed tasks')
  .action(async () => {
    const { loadTasks, saveTasks } = await import('./tasks-store');
    const tasks = loadTasks();
    const active = tasks.filter((t) => t.status === 'running' || t.status === 'paused');
    const removed = tasks.length - active.length;
    saveTasks(active);
    console.log(chalk.green(`Cleared ${removed} task(s)`));
  });

// Make tasks list the default action
tasksCmd.action(async () => {
  const { loadTasks } = await import('./tasks-store');
  const tasks = loadTasks();

  if (tasks.length === 0) {
    console.log(chalk.yellow('No tasks'));
    return;
  }

  console.log(chalk.cyan('\nTasks:\n'));
  for (const task of tasks) {
    const statusColor =
      task.status === 'running'
        ? chalk.green
        : task.status === 'paused'
          ? chalk.yellow
          : task.status === 'completed'
            ? chalk.blue
            : chalk.red;

    console.log(`  ${chalk.bold(task.id.slice(0, 12))}...  ${task.type}  ${statusColor(task.status)}`);
  }
  console.log('');
});

// ============ Record Commands ============
const recordCmd = program.command('record').description('Record market data (orderbook, trades, etc.)');

recordCmd
  .command('start')
  .alias('s')
  .argument('<exchange>', 'Exchange to record from (e.g., bybit, binance)')
  .option('--type <type>', 'Data type to record', 'orderbook')
  .option('--symbol <symbol>', 'Symbol to record', 'BTCUSDT')
  .option('--interval <interval>', 'Recording interval (e.g., 30s, 1m, 5m)', '1m')
  .option('--retention <retention>', 'Data retention period (e.g., 7d, 14d, 30d)', '7d')
  .option('--levels <levels>', 'Orderbook levels to store', '25')
  .description('Start recording market data')
  .action(async (exchange, options) => {
    const { initDatabase, startRecorder, parseInterval, parseRetention } = await import('./record');

    // Initialize database
    initDatabase();

    const dataType = options.type as 'orderbook' | 'trades' | 'funding';
    const intervalMs = parseInterval(options.interval);
    const retentionDays = parseRetention(options.retention);
    const levels = parseInt(options.levels, 10);

    console.log(chalk.cyan(`\nStarting ${dataType} recorder for ${exchange}:${options.symbol}`));
    console.log(chalk.gray(`  Interval: ${options.interval} (${intervalMs}ms)`));
    console.log(chalk.gray(`  Retention: ${options.retention} (${retentionDays} days)`));
    if (dataType === 'orderbook') {
      console.log(chalk.gray(`  Levels: ${levels}`));
    }
    console.log('');

    try {
      const { recorder, replaced } = await startRecorder({
        dataType,
        exchange: exchange.toLowerCase(),
        symbol: options.symbol,
        intervalMs,
        retentionDays,
        levels,
      });

      if (replaced) {
        console.log(chalk.yellow(`Replaced existing ${dataType}:${exchange}:${options.symbol} recorder`));
      }
      console.log(chalk.green('✓ Recorder started successfully'));
      console.log(chalk.gray('  Press Ctrl+C to stop recording\n'));

      // Keep running until interrupted
      await new Promise<void>((resolve) => {
        const shutdown = async () => {
          console.log(chalk.yellow('\nStopping recorder...'));
          await recorder.stop();
          console.log(chalk.green('✓ Recorder stopped'));
          resolve();
        };

        process.on('SIGINT', shutdown);
        process.on('SIGTERM', shutdown);
      });
    } catch (error) {
      console.error(chalk.red(`\n✗ Failed to start recorder: ${(error as Error).message}`));
      process.exit(1);
    }
  });

recordCmd
  .command('list')
  .alias('ls')
  .description('List recording configurations')
  .action(async () => {
    const { initDatabase, getRecordings, getActiveRecorders } = await import('./record');

    initDatabase();
    const configs = getRecordings();
    const activeRecorders = getActiveRecorders();

    if (configs.length === 0 && activeRecorders.length === 0) {
      console.log(chalk.yellow('No recordings configured'));
      return;
    }

    // Print table header
    console.log(chalk.cyan('\nRecordings:\n'));
    const headers = ['Status', 'Type', 'Exchange', 'Symbol', 'Interval', 'Retention', 'Snapshots'];
    const colWidths = [12, 10, 10, 10, 10, 10, 12];

    const formatRow = (cells: string[]): string => {
      return cells.map((cell, i) => cell.padEnd(colWidths[i])).join('  ');
    };

    console.log(formatRow(headers));
    console.log(colWidths.map((w) => '-'.repeat(w)).join('  '));

    // Active recorders
    for (const recorder of activeRecorders) {
      const status = recorder.getStatus();
      console.log(
        formatRow([
          '🟢 Running',
          status.dataType,
          status.exchange,
          status.symbol,
          `${status.intervalMs / 1000}s`,
          `${status.retentionDays}d`,
          status.snapshotCount.toLocaleString(),
        ]),
      );
    }

    // Inactive configs
    for (const config of configs) {
      const isRunning = activeRecorders.some(
        (r) =>
          r.getStatus().dataType === config.dataType &&
          r.getStatus().exchange === config.exchange &&
          r.getStatus().symbol === config.symbol,
      );

      if (!isRunning) {
        console.log(
          formatRow([
            config.active ? '⏸️  Paused' : '⭕ Inactive',
            config.dataType,
            config.exchange,
            config.symbol,
            `${config.intervalMs / 1000}s`,
            `${config.retentionDays}d`,
            '-',
          ]),
        );
      }
    }
    console.log('');
  });

recordCmd
  .command('stop')
  .argument('<exchange>', 'Exchange to stop recording')
  .option('--type <type>', 'Data type to stop', 'orderbook')
  .option('--symbol <symbol>', 'Symbol to stop', 'BTCUSDT')
  .description('Stop a recording')
  .action(async (exchange, options) => {
    const { initDatabase, stopRecorder } = await import('./record');

    initDatabase();
    const stopped = await stopRecorder(
      options.type as 'orderbook' | 'trades' | 'funding',
      exchange.toLowerCase(),
      options.symbol,
    );

    if (stopped) {
      console.log(chalk.green(`✓ Stopped recording ${options.type}:${exchange}:${options.symbol}`));
    } else {
      console.log(chalk.yellow(`No active recorder found for ${options.type}:${exchange}:${options.symbol}`));
    }
  });

recordCmd
  .command('stats')
  .argument('[exchange]', 'Filter statistics by exchange')
  .description('Show database statistics')
  .action(async (exchangeFilter) => {
    const { initDatabase, getDatabaseStats, getDatabasePath, getActiveRecorders } = await import('./record');

    initDatabase();
    const stats = getDatabaseStats();
    const activeRecorders = getActiveRecorders();

    console.log(chalk.cyan('\nRecording Statistics:\n'));
    console.log(`  Database: ${getDatabasePath()}`);
    console.log(`  Size: ${stats.databaseSize}`);
    console.log(`  Active Recorders: ${activeRecorders.length}`);
    console.log(`  Total Snapshots: ${stats.totalSnapshots.toLocaleString()}`);
    console.log('');

    for (const [type, typeStats] of Object.entries(stats.byType)) {
      // Filter by exchange if specified
      const exchanges = exchangeFilter
        ? typeStats.exchanges.filter((e) => e.toLowerCase() === exchangeFilter.toLowerCase())
        : typeStats.exchanges;

      if (exchangeFilter && exchanges.length === 0) {
        continue;
      }

      console.log(`  ${chalk.bold(type)}:`);
      console.log(`    Count: ${typeStats.count.toLocaleString()}`);
      console.log(`    Exchanges: ${exchanges.join(', ') || 'none'}`);
    }
    console.log('');
  });

// Default action: if exchange provided, start recording; otherwise show help
recordCmd
  .argument('[exchange]', 'Exchange to record from (shorthand for "record start")')
  .option('--type <type>', 'Data type to record', 'orderbook')
  .option('--symbol <symbol>', 'Symbol to record', 'BTCUSDT')
  .option('--interval <interval>', 'Recording interval (e.g., 30s, 1m, 5m)', '1m')
  .option('--retention <retention>', 'Data retention period (e.g., 7d, 14d, 30d)', '7d')
  .option('--levels <levels>', 'Orderbook levels to store', '25')
  .action(async (exchange, options) => {
    if (!exchange) {
      recordCmd.help();
      return;
    }

    // Same logic as "record start"
    const { initDatabase, startRecorder, parseInterval, parseRetention } = await import('./record');

    initDatabase();

    const dataType = options.type as 'orderbook' | 'trades' | 'funding';
    const intervalMs = parseInterval(options.interval);
    const retentionDays = parseRetention(options.retention);
    const levels = parseInt(options.levels, 10);

    console.log(chalk.cyan(`\nStarting ${dataType} recorder for ${exchange}:${options.symbol}`));
    console.log(chalk.gray(`  Interval: ${options.interval} (${intervalMs}ms)`));
    console.log(chalk.gray(`  Retention: ${options.retention} (${retentionDays} days)`));
    if (dataType === 'orderbook') {
      console.log(chalk.gray(`  Levels: ${levels}`));
    }
    console.log('');

    try {
      const { recorder, replaced } = await startRecorder({
        dataType,
        exchange: exchange.toLowerCase(),
        symbol: options.symbol,
        intervalMs,
        retentionDays,
        levels,
      });

      if (replaced) {
        console.log(chalk.yellow(`Replaced existing ${dataType}:${exchange}:${options.symbol} recorder`));
      }
      console.log(chalk.green('✓ Recorder started successfully'));
      console.log(chalk.gray('  Press Ctrl+C to stop recording\n'));

      await new Promise<void>((resolve) => {
        const shutdown = async () => {
          console.log(chalk.yellow('\nStopping recorder...'));
          await recorder.stop();
          console.log(chalk.green('✓ Recorder stopped'));
          resolve();
        };

        process.on('SIGINT', shutdown);
        process.on('SIGTERM', shutdown);
      });
    } catch (error) {
      console.error(chalk.red(`\n✗ Failed to start recorder: ${(error as Error).message}`));
      process.exit(1);
    }
  });

// ============ Update Commands ============
program
  .command('update')
  .description('Update Tealstreet CLI to the latest version')
  .option('-c, --check', 'Only check for updates, do not install')
  .action(async (options) => {
    if (options.check) {
      await forceCheckForUpdates();
    } else {
      await performUpdate();
    }
  });

// ============ Logs Command ============
program
  .command('logs')
  .alias('log')
  .description('View CLI logs (tail -f ~/.tealstreet/cli.log)')
  .option('-c, --clear', 'Clear log file')
  .option('-n, --lines <number>', 'Number of lines to show', '50')
  .action(async (options) => {
    const { spawn } = await import('child_process');
    const { existsSync, writeFileSync } = await import('fs');
    const { join } = await import('path');
    const { homedir } = await import('os');

    const logFile = join(homedir(), '.tealstreet', 'cli.log');

    if (options.clear) {
      if (existsSync(logFile)) {
        writeFileSync(logFile, '');
        console.log(chalk.green('Logs cleared'));
      } else {
        console.log(chalk.yellow('Log file does not exist'));
      }
      return;
    }

    if (!existsSync(logFile)) {
      console.log(chalk.yellow('Log file does not exist yet'));
      console.log(chalk.gray(`Expected at: ${logFile}`));
      return;
    }

    console.log(chalk.cyan(`Tailing logs from: ${logFile}`));
    console.log(chalk.gray('Press Ctrl+C to exit\n'));

    // Use tail -f to follow the log file
    const tail = spawn('tail', ['-n', options.lines, '-f', logFile], {
      stdio: 'inherit',
    });

    tail.on('error', (err) => {
      console.log(chalk.red(`Failed to tail logs: ${err.message}`));
    });
  });

// ============ Interactive Mode ============
program
  .command('interactive')
  .alias('i')
  .description('Start interactive REPL mode')
  .option('-a, --account <name>', 'Account to use')
  .option('--simple', 'Use simple REPL mode (no TUI)')
  .option('--disable-all', 'Start with all exchanges disabled')
  .action(async (options) => {
    const { startRepl } = await getRepl();
    startRepl(options.account, { disableAll: options.disableAll });
  });

// ============ Direct Command Execution ============
program
  .command('exec <command...>')
  .description('Execute a trading command')
  .option('-a, --account <account>', 'Account to use')
  .option('-s, --symbol <symbol>', 'Trading symbol')
  .action(async (cmdArgs, options) => {
    const { runSingleCommand } = await getRepl();
    const commandString = cmdArgs.join(' ');
    runSingleCommand(commandString, options.account, options.symbol);
  });

// ============ Shorthand Trading Commands ============
const tradingCommands = [
  { name: 'buy', desc: 'Buy market/limit order' },
  { name: 'sell', desc: 'Sell market/limit order' },
  { name: 'close', desc: 'Close position' },
  { name: 'cancel', desc: 'Cancel orders' },
  { name: 'scale', desc: 'Scale into position' },
  { name: 'chase', desc: 'Chase order' },
  { name: 'nuke', desc: 'Cancel all & close all' },
  { name: 'balance', desc: 'Show balance' },
  { name: 'stop', desc: 'Place stop order' },
];

tradingCommands.forEach(({ name, desc }) => {
  program
    .command(`${name} [args...]`)
    .description(desc)
    .option('-a, --account <account>', 'Account to use')
    .option('-s, --symbol <symbol>', 'Trading symbol')
    .action(async (args, options) => {
      const { runSingleCommand } = await getRepl();
      const commandString = `${name} ${args.join(' ')}`;
      runSingleCommand(commandString, options.account, options.symbol);
    });
});

// ============ MCP Server ============
program
  .command('mcp')
  .description('Run as an MCP server over stdio (for Claude Code and other MCP clients)')
  .action(async () => {
    // Lazy import: pulls in the TypeScript compiler via
    // @tealstreet/script-authoring, which no other CLI path needs at runtime.
    const { startMcpServer } = await import('./mcp/server');
    await startMcpServer();
  });

// ============ Default Action ============
// Handle top-level flags before parsing
const args = process.argv.slice(2);

if (args.includes('--update')) {
  performUpdate().then((success) => {
    process.exit(success ? 0 : 1);
  });
} else if (args.includes('--check-update')) {
  forceCheckForUpdates().then(() => {
    process.exit(0);
  });
} else if (args.includes('--disable-all') && args.length === 1) {
  // Only --disable-all flag provided, start REPL with all exchanges disabled
  getRepl().then(({ startRepl }) => startRepl(undefined, { disableAll: true }));
} else if (process.argv.length === 2) {
  // If no command is provided, start interactive REPL mode
  getRepl().then(({ startRepl }) => startRepl());
} else {
  program.parse();
}

// ============ Helpers ============
function maskSecret(secret: string): string {
  if (!secret) return '(not set)';
  if (secret.length <= 8) return '****';
  return secret.slice(0, 4) + '****' + secret.slice(-4);
}
