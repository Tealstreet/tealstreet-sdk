#!/usr/bin/env bun
// Shim WebSocket for tealstreet.io proxy connections (must be first import)
import './ws-shim';
import './rust-engine';
import '@tealstreet/safe-cex/src/transport/registerTealstreetTransport';
import { Command, Option } from 'commander';
import chalk from 'chalk';
import type { ExchangeName } from '@tealstreet/safe-cex/src/types';
import {
  buildExchangeAccountConfig,
  ExchangeAccountConfigError,
  getExchangeAccountCredentialSchema,
  getSupportedExchangeAccountNames,
  getSupportedExchangeAccountCredentialSchemas,
} from '@tealstreet/safe-cex/src/accounts/buildExchangeAccountConfig';
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
import { cleanupStaleUpdate } from './windows-update';
import { fireCheckin } from './checkin';
import { getAuthState, login, logout, showAuthStatus } from './auth';
import { emitJsonAndExit, enterJsonMode, isJsonMode } from './jsonMode';
import { initCliSentry, captureCliError, flushCliSentry } from './sentry';

// Start console interception early to catch all logs
interceptConsole();

// cli-core prints the product name and docs URL in REPL `help`; a whitelabel
// binary must never print Tealstreet's. See src/brand.ts.
setCliBrand(BRAND.cliCore);

const JSON_MODE = isJsonMode();
if (JSON_MODE) enterJsonMode();

// A Windows self-update parks the previous binary as `.old` beside the new one
// — it cannot be deleted while it is the running image. This is the next
// launch, so clear it. No-op everywhere else, and never throws.
cleanupStaleUpdate();

// Released binaries only, and opt-out via TEALSTREET_TELEMETRY=0. Must precede
// the process-level handlers below so an early crash still has a client.
initCliSentry();

// Announce this process to the web app so the web's CliListenerWorker
// gate knows there's a CLI on this platform. Fire-and-forget — no
// network awaited, all errors swallowed; safe to run unconditionally.
// See apps/cli/src/checkin.ts for the gating + dedupe contract.
fireCheckin();

// Global error handlers to prevent crashes and verbose stack traces
/** Builds and emits a --json document, answering INTERNAL_ERROR if building it throws. */
const emitJsonOrCrash = async (build: () => unknown) => {
  let value: unknown;
  try {
    value = build();
  } catch (error) {
    return emitJsonAndExit({ ok: false, code: 'INTERNAL_ERROR', error: (error as Error)?.message ?? String(error) }, 1);
  }
  return emitJsonAndExit(value);
};

// Under --json a crash must still answer with a document, once, and a non-zero exit.
// Unhandled rejections keep their old non-fatal handling: exchange code leaves some
// unhandled during ordinary runs, and failing the run on one would be a false failure.
let jsonCrashReported = false;
const reportJsonCrash = (reason: unknown) => {
  if (!JSON_MODE || jsonCrashReported) return false;
  jsonCrashReported = true;
  void emitJsonAndExit(
    { ok: false, code: 'INTERNAL_ERROR', error: reason instanceof Error ? reason.message : String(reason) },
    1,
  );
  return true;
};

process.on('uncaughtException', (error: Error) => {
  if (reportJsonCrash(error)) return;
  logger.error(`Uncaught exception: ${error.message}`, error.stack);
  console.error(error.message);
  captureCliError(error, { handler: 'uncaughtException' });
  // Exit is deferred until the event is on the wire, else the crash we most
  // want to see is the one that never gets reported.
  void flushCliSentry().finally(() => process.exit(1));
});

process.on('unhandledRejection', (reason: unknown) => {
  const message = reason instanceof Error ? reason.message : String(reason);
  const stack = reason instanceof Error ? reason.stack : undefined;
  logger.error(`Unhandled rejection: ${message}`, stack);
  captureCliError(reason, { handler: 'unhandledRejection' });
  // Don't exit on unhandled rejection - let the app continue
  // The error will be logged and a clean message shown to user
});
import { checkForUpdatesBackground, performUpdate, forceCheckForUpdates, getCurrentVersion } from './updater';
import { registerCrossTradeCommand } from './commands/cross-trade';
import { BRAND } from './brand';
import { setCliBrand } from '@tealstreet/cli-core';

// Dynamic import for repl to avoid circular dependency issues with safe-cex
const getRepl = async () => import('./repl');

// Check for updates in background (non-blocking). Its banner would corrupt --json output.
if (!JSON_MODE) checkForUpdatesBackground();

const CLI_DOCS_URL = BRAND.docsUrl;

const program = new Command();

program
  .name(BRAND.binName)
  .description('Trade crypto from your terminal')
  .version(getCurrentVersion(), '-v, --version', 'Display version number')
  .option('--update', 'Update CLI to the latest version')
  .option('--check-update', 'Check for updates without installing')
  .option('--disable-all', 'Start with all exchanges disabled (skip auto-connecting)')
  .addHelpText('after', `\nDocs: ${CLI_DOCS_URL}`);

// ============ Auth Commands ============
// Same names the REPL uses (repl.ts processSingleCommand), so the instruction
// "Run \"login\" first" — which the proxy/tunnel path prints on failure — works
// whether the reader types it in the REPL or the shell. Without these,
// `tealstreet login` exits with "unknown command 'login'".
program
  .command('login')
  .description(`Authenticate with ${BRAND.displayName} (required for proxy/tunnel routing)`)
  .action(async () => {
    const ok = await login();
    process.exit(ok ? 0 : 1);
  });

program
  .command('logout')
  .description(`Clear the stored ${BRAND.displayName} auth token`)
  .action(() => logout());

program
  .command('auth')
  .alias('whoami')
  .description('Show authentication status')
  .option('--json', 'Print { state, userId, expiresInDays } as JSON')
  .action(async (options) => {
    if (options.json) await emitJsonOrCrash(() => getAuthState());
    showAuthStatus();
  });

// ============ Account Commands ============
const accountCmd = program.command('account').alias('accounts').description('Manage accounts');

// List accounts
accountCmd
  .command('list')
  .alias('ls')
  .description('List all configured accounts')
  .option('--json', 'Print the accounts (never their keys) as JSON')
  .action(async (options) => {
    if (options.json) {
      // An allowlist: account configs also hold secondary keys, passphrases and bot tokens.
      await emitJsonOrCrash(() => {
        const config = loadConfig();
        return {
          accounts: listAccounts().map((account) => ({
            name: account.name,
            exchange: account.exchange,
            default: account.name === config.defaultAccount,
            testnet: Boolean(account.testnet),
          })),
          defaultSymbol: config.defaultSymbol ?? null,
        };
      });
    }

    const config = loadConfig();
    const accounts = listAccounts();

    if (accounts.length === 0) {
      console.log(chalk.yellow('No accounts configured.'));
      console.log(chalk.gray(`Run: ${BRAND.binName} account add`));
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
    `Exchange name (${getSupportedExchangeAccountNames().slice(0, 5).join(', ')}...)`,
  )
  .option('-k, --api-key <key>', 'API key')
  .option('-s, --api-secret <secret>', 'API secret')
  .option('-p, --passphrase <passphrase>', 'API passphrase (for exchanges that require it)')
  .option('--private-key <key>', 'Alias for wallet/private-key exchanges')
  .option('--wallet-address <address>', 'Alias for wallet/private-key exchanges')
  .option('--application-id <id>', 'Application ID / main wallet / account index')
  .option('--api-key-index <index>', 'API key index')
  .option('--vault <vault>', 'Optional vault address/name')
  .option('--omni-key-seed <seed>', 'Omni key seed')
  .option('--paradex-address <address>', 'Paradex account address')
  .option('--subkey-address <address>', 'Paradex subkey public address')
  .option('--subaccount <subaccount>', 'Exchange subaccount/account ID')
  .option('--grvt-signer-address <address>', 'GRVT signer address')
  .option('--grvt-trading-account-id <id>', 'GRVT trading account ID')
  .option('--enable-spot-markets', 'Enable spot markets for exchanges with a spot toggle')
  .option('-t, --testnet', 'Use testnet')
  // Routing flags: both spellings on every brand, help shows the brand's own.
  .option('--proxy', `Route REST through the ${BRAND.displayName} proxy (required for IP-whitelisted keys)`)
  .option('--tunnel', `Route REST through the ${BRAND.displayName} tradetunnel`)
  .option('--ws-route', `Route private WebSockets through ${BRAND.displayName}`)
  .addOption(new Option('--tealstreet-proxy', 'Alias of --proxy').hideHelp())
  .addOption(new Option('--tealstreet-tunnel', 'Alias of --tunnel').hideHelp())
  .addOption(new Option('--tealstreet-ws-route', 'Alias of --ws-route').hideHelp())
  .option('--proxy-region <region>', 'Pin the proxy egress region')
  .option('--default', 'Set as default account')
  .action((options) => {
    const exchangeInfo = getExchangeAccountCredentialSchema(options.exchange);
    if (!exchangeInfo) {
      console.log(chalk.red(`Unknown exchange: ${options.exchange}`));
      console.log(chalk.gray(`Supported exchanges: ${getSupportedExchangeAccountNames().join(', ')}`));
      return;
    }

    let account: AccountConfig;
    try {
      account = buildExchangeAccountConfig({
        name: options.name,
        exchange: options.exchange.toLowerCase() as ExchangeName,
        apiKey: options.apiKey,
        apiSecret: options.apiSecret,
        passphrase: options.passphrase,
        privateKey: options.privateKey,
        walletAddress: options.walletAddress,
        applicationId: options.applicationId,
        apiKeyIndex: options.apiKeyIndex,
        vault: options.vault,
        omniKeySeed: options.omniKeySeed,
        paradexAddress: options.paradexAddress,
        subkeyAddress: options.subkeyAddress,
        subaccount: options.subaccount,
        grvtSignerAddress: options.grvtSignerAddress,
        grvtTradingAccountId: options.grvtTradingAccountId,
        testnet: options.testnet || false,
        tealstreetProxy: options.proxy || options.tealstreetProxy,
        tealstreetTunnel: options.tunnel || options.tealstreetTunnel,
        tealstreetWsRoute: options.wsRoute || options.tealstreetWsRoute,
        proxyRegion: options.proxyRegion,
        enableSpotMarkets: options.enableSpotMarkets,
      });
    } catch (error) {
      if (error instanceof ExchangeAccountConfigError) {
        console.log(chalk.red(error.message));
      } else {
        console.log(chalk.red(error instanceof Error ? error.message : String(error)));
      }
      return;
    }

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
    for (const info of getSupportedExchangeAccountCredentialSchemas()) {
      console.log(chalk.bold(`  ${info.exchange.padEnd(15)} ${info.displayName}`));
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

      // Validate exchange is supported
      if (!getExchangeAccountCredentialSchema(account.exchange)) {
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
    const { chmodSync, writeFileSync } = require('fs');
    const { resolve } = require('path');

    const accounts = listAccounts();
    if (accounts.length === 0) {
      console.log(chalk.yellow('No accounts to export'));
      return;
    }

    const outputFile = file ? resolve(file) : resolve(`${BRAND.binName}-accounts.json`);
    try {
      // The export carries API keys; chmod covers overwriting an older export.
      writeFileSync(outputFile, JSON.stringify(accounts, null, 2), { mode: 0o600 });
      chmodSync(outputFile, 0o600);
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
  .description(`Update ${BRAND.displayName} CLI to the latest version`)
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
  .description(`View CLI logs (tail -f ~/${BRAND.configDirName}/cli.log)`)
  .option('-c, --clear', 'Clear log file')
  .option('-n, --lines <number>', 'Number of lines to show', '50')
  .action(async (options) => {
    const { spawn } = await import('child_process');
    const { existsSync, writeFileSync } = await import('fs');
    const { join } = await import('path');
    const { homedir } = await import('os');

    const logFile = join(homedir(), BRAND.configDirName, 'cli.log');

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
  .option('--json', 'Print one JSON result { ok, code, output } for scripts and agents')
  .action(async (cmdArgs, options) => {
    const { runSingleCommand } = await getRepl();
    const commandString = cmdArgs.join(' ');
    await runSingleCommand(commandString, options.account, options.symbol, { json: options.json });
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
    .option('--json', 'Print one JSON result { ok, code, output } for scripts and agents')
    .action(async (args, options) => {
      const { runSingleCommand } = await getRepl();
      const commandString = `${name} ${args.join(' ')}`;
      await runSingleCommand(commandString, options.account, options.symbol, { json: options.json });
    });
});

// ============ Command Catalog ============
program
  .command('commands')
  .description('List every command, for people and for AI agents (--json)')
  .option('--json', 'Print the full catalog as JSON')
  .action(async (options) => {
    const { buildCommandCatalog } = await import('../../../packages/cli-core/src/commands/catalog');
    type ShellEntry = {
      name: string;
      aliases: string[];
      description: string;
      options: { flags: string; description: string }[];
      subcommands: ShellEntry[];
    };
    const describe = (command: Command): ShellEntry => ({
      name: command.name(),
      aliases: command.aliases(),
      description: command.description(),
      options: command.options.map((option) => ({ flags: option.flags, description: option.description })),
      subcommands: command.commands.map(describe),
    });
    const shell = program.commands.map(describe);
    const repl = buildCommandCatalog();
    if (options.json) await emitJsonOrCrash(() => ({ version: getCurrentVersion(), shell, repl }));
    console.log(chalk.cyan(`\nShell commands (${BRAND.binName} <command>):\n`));
    for (const command of shell) console.log(`  ${chalk.bold(command.name.padEnd(14))} ${command.description}`);
    console.log(chalk.cyan(`\nTrading commands (at the prompt, or ${BRAND.binName} exec '<command>'):\n`));
    for (const command of repl) console.log(`  ${chalk.bold(command.name.padEnd(14))} ${command.description}`);
    console.log(chalk.gray(`\nDetails: ${BRAND.docsUrl}/reference/commands`));
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
} else if (JSON_MODE) {
  // Commander would print a usage error as text and exit; under --json the caller expects one document.
  const quietOnError = (command: Command) => {
    command.exitOverride();
    command.configureOutput({ writeErr: () => {} });
    command.commands.forEach(quietOnError);
  };
  quietOnError(program);
  program.parseAsync().catch((error: { code?: string; message?: string; exitCode?: number }) => {
    const askedForHelp = error?.code === 'commander.help' && error.exitCode === 0;
    if (askedForHelp || error?.code === 'commander.helpDisplayed' || error?.code === 'commander.version')
      process.exit(0);
    if (error?.code === 'commander.help')
      error = { ...error, message: `missing subcommand; see \`${BRAND.binName} commands --json\`` };
    const fromCommander = String(error?.code ?? '').startsWith('commander.');
    void emitJsonAndExit(
      {
        ok: false,
        code: fromCommander ? 'USAGE_ERROR' : 'INTERNAL_ERROR',
        error: (error?.message ?? String(error)).replace(/^error: /, ''),
      },
      1,
    );
  });
} else {
  program.parse();
}

// ============ Helpers ============
function maskSecret(secret: string): string {
  if (!secret) return '(not set)';
  if (secret.length <= 8) return '****';
  return secret.slice(0, 4) + '****' + secret.slice(-4);
}
