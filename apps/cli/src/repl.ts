import * as readline from 'readline';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import chalk from 'chalk';
import type { CLISession, PromptConfig } from '../../../packages/cli-core/src/index';
import type { CommandContext } from '../../../packages/cli-core/src/commands/Command';
import {
  executeCommand as cliExecuteCommand,
  connectToAccount,
  formatPrompt,
  ConnectionMonitor,
  NetworkMonitor,
  PROMPT_PRESETS,
  preprocessCommand,
  parseArgs,
  resolveDeferredCaptures,
} from '../../../packages/cli-core/src/index';
import { FilePreprocessStore } from './preprocess-store';
import { commandResultError, runCommandWithRetries } from './command-outcome';
import {
  isAuditEnabled,
  setAuditEnabled,
  readAuditTail,
  recordWriteOp,
  recordScriptWrite,
  recordFill,
  newRunId,
  runWithAuditRunId,
  AUDIT_LOG_PATH,
} from './audit-log';
// All commands now handled by cli-core
import {
  formatPositionsTable,
  formatOrdersTable,
  formatMarginTable,
  formatMultiAccountPositionsTable,
  formatMultiAccountOrdersTable,
  formatMultiAccountMarginTable,
  formatMultiAccountMonitorOutput,
  formatAccountMonitorOutput,
  type AccountExchange,
  ChartCommand,
  type ChartLine,
  renderCandleChart,
  formatChartHeader,
  formatCountdown,
  formatPositionLine,
  formatOrderLine,
  formatPositionSummary,
  TIMEFRAME_MS,
  formatAllChasersStatus,
  getActiveChasers,
  formatSpotTable,
  formatMultiAccountSpotTable,
  GREEN,
  RED,
  YELLOW,
  CYAN,
  RESET,
  DIM,
  CLEAR_SCREEN,
  CLEAR_LINE,
} from '../../../packages/cli-core/src/commands/trading';

import { WatchMode, ENTER_ALT_SCREEN, EXIT_ALT_SCREEN, CURSOR_HOME } from './utils/watch-mode';
import { registerRecordCommands, registerExportImportCommands } from './commands';

import {
  OrderSide,
  type OrderFillEvent,
  type Candle,
  type Timeframe,
  type Market,
} from '../../../packages/safe-cex/src/types';
import type { Exchange } from '../../../packages/safe-cex/src/exchanges/base';
import { loadConfig, saveConfig, getAccount } from './config';
// TWAP support for taskAction handling
import { commandRegistry } from '../../../packages/cli-core/src/commands/CommandRegistry';
import { ensureCommandsRegistered } from '../../../packages/cli-core/src/commands/CommandExecutor';
import { getAuthState, markTokenExpiryWarned } from './auth';
import { emitJson, emitJsonAndExit } from './jsonMode';
import {
  connectFailureCode,
  findKeyRejection,
  stripAnsi,
  summarizeOneShot,
  type OneShotCode,
  type OneShotLine,
} from './oneShotResult';
import { flushCliSentry } from './sentry';
import { virtualClock } from '../../../packages/safe-cex/src/utils/virtualClock';
import { disposeHOWWebSockets } from '../../../packages/http-over-websocket/src/axiosHttpOverWebsocketAdapter';
import { twapManager } from '../../../packages/safe-cex/src/webworker/twap/TwapManager';
import { TwapInstance } from '../../../packages/safe-cex/src/webworker/twap/TwapInstance';
import { createStandalonePlatformAdapter } from './platform-adapter';
import { logger, originalConsoleLog, setConsoleTap, setPromptController } from './logger';
import type { ExchangeLike, ExchangeAccountConfig } from '../../../packages/cli-core/src/types';
import { formatBaseSize } from '../../../packages/cli-core/src/utils/formatBaseSize';
import {
  login,
  logout,
  showAuthStatus,
  isLoggedIn,
  checkAndRefreshToken,
  tokenNeedsRefresh,
  getTokenExpiryDays,
} from './auth';
import { getCurrentVersion, performUpdate, forceCheckForUpdates } from './updater';
import { ExchangeManager } from './exchange-manager';
import { classifyConnectError } from './connect-error';
import { disableFooter, enableFooter, redrawFooter, setFooterRenderer } from './footer';
import {
  DEFAULT_SEGMENTS,
  FOOTER_SEGMENT_TYPES,
  renderFooter,
  type FooterConnection,
  type FooterSegment,
  type FooterSegmentType,
  type FooterState,
} from './footer-segments';
import { enableStatusLine } from './status-line';
import { classifyForCLI, criticalErrorKey } from './error-classifier';
import { startListener, stopListener } from './listener';
import { webhookBodyRefusal, webhookChainRefusal, webhookRefusal } from '@tealstreet/cli-core';
import { restartWebhookRelay, startWebhookRelay, stopWebhookRelay } from './listener/webhook-relay';
import { startAliasSync, stopAliasSync, syncAliasesIfStale } from './alias-sync';
import { cmdWebhookClaim, cmdWebhookRelease, cmdWebhookStatus } from './listener/webhook-repl';
import { cmdClientsList, cmdClientsRevoke } from './listener/clients';
import { cmdHookList, cmdHookCreate, cmdHookRevoke, cmdHookRotate, cmdHookShow } from './listener/hook-repl';
import { getAdapterContext, runWithAdapterContext } from './listener/adapter-context';
import type { Capabilities, CoreAdapter, RunScriptResult } from '@tealstreet/script-api-core';

import { createStandaloneCoreAdapter } from './script-adapter';
import { runScriptFromHost, runScriptFromHostWithAdapter } from './script-runner';
import { awaitScriptsLoaded, loadScripts, lookupScript, SCRIPTS_DIR } from './scripts-store';
import { getCliTranspile } from './bun-transpile';
import { parseScriptInvocation } from './script-dispatcher';
import { BRAND } from './brand';

const adapter = createStandalonePlatformAdapter();
const exchangeManager = new ExchangeManager(adapter);
// Set exchange manager reference on adapter for connection status queries
adapter.setExchangeManager(exchangeManager);
adapter.setPromptReadline(
  () => rlInstance,
  () => isMonitorMode,
);

// Plug the `history` command's source into readline's internal
// history buffer. readline stores entries newest-first (the order
// the HistoryCommand expects), so we just slice. rlInstance may
// be null briefly during readline takedown/recreation in watch
// modes — fall back to an empty list rather than crashing.
adapter.getCommandHistory = (limit?: number): string[] => {
  const HIST_DEFAULT = 50;
  const HIST_MAX = 200;
  const cap = Math.min(limit ?? HIST_DEFAULT, HIST_MAX);
  const rl = rlInstance as (readline.Interface & { history?: string[] }) | null;
  const hist = rl?.history ?? [];
  return hist.slice(0, cap);
};

// File-backed user vars + aliases store. Reads ~/.tealstreet/vars.json
// and ~/.tealstreet/aliases.json on construction; persists synchronously
// after every mutation so a crash mid-session doesn't lose state.
const preprocessStore = new FilePreprocessStore();

// Note: Console interception is now done early in index.ts

let session: CLISession = {
  currentExchange: null,
  currentAccountName: undefined,
  currentSymbol: 'BTCUSDT',
};

let promptConfig: PromptConfig = PROMPT_PRESETS.default;

/**
 * Per-account connection lifecycle, used so the prompt reflects honest
 * load/auth state instead of optimistically implying a selected-but-not-yet-
 * connected account is live. `getPrompt` derives `connected` from
 * `exchangeManager.isConnected` (the source of truth), and falls back to this
 * map only to distinguish `connecting` from `failed`/`auth-failed` while there
 * is no live exchange. The background-connect `.then`/`.catch` in `startRepl`
 * write the terminal states.
 */
type AccountConnectionState = 'connecting' | 'connected' | 'failed' | 'auth-failed';
const connectionStates = new Map<string, AccountConnectionState>();

let connectionMonitor: ConnectionMonitor | null = null;
let networkMonitor: NetworkMonitor | null = null;
let monitorInterval: ReturnType<typeof setInterval> | null = null;
let isMonitorMode = false;
let monitorExitTime = 0; // Timestamp when monitor mode was exited (for cooldown)
const MONITOR_EXIT_COOLDOWN_MS = 200; // Skip prompt updates for this long after exiting monitor
let rlInstance: readline.Interface | null = null;
let isRecreatingReadline = false; // Flag to prevent close handler from exiting during recreation
const chartCommand = new ChartCommand();
let stopMonitorFn: (() => void) | null = null;
let chartUnsubscribe: (() => void) | null = null;
let commandQueue: Promise<void> = Promise.resolve();

// Track critical errors that have already been shown (account:category)
const shownCriticalErrors = new Set<string>();

/**
 * Check if interactive mode is available.
 * Returns error message if blocked, null if allowed.
 */
function checkInteractiveMode(): string | null {
  if (!rlInstance) {
    return 'REPL not initialized';
  }
  return null;
}

// Watch mode message buffer - captures errors/warnings during watch modes
interface WatchMessage {
  type: 'error' | 'warning' | 'info' | 'fill';
  message: string;
  timestamp: number;
}
const watchMessageBuffer: WatchMessage[] = [];
const MAX_WATCH_MESSAGES = 5;
const WATCH_MESSAGE_TTL = 10000; // 10 seconds

function addWatchMessage(type: WatchMessage['type'], message: string): void {
  watchMessageBuffer.push({ type, message, timestamp: Date.now() });
  // Keep only recent messages
  while (watchMessageBuffer.length > MAX_WATCH_MESSAGES) {
    watchMessageBuffer.shift();
  }
}

function getWatchMessages(): WatchMessage[] {
  const now = Date.now();
  // Filter out expired messages
  const valid = watchMessageBuffer.filter((m) => now - m.timestamp < WATCH_MESSAGE_TTL);
  // Update buffer to remove expired
  watchMessageBuffer.length = 0;
  watchMessageBuffer.push(...valid);
  return valid;
}

function formatWatchMessages(): string {
  const messages = getWatchMessages();
  if (messages.length === 0) return '';

  let output = '\n' + DIM + '─── Recent Messages ───' + RESET + '\n';
  for (const msg of messages) {
    const color = msg.type === 'error' ? RED : msg.type === 'warning' ? YELLOW : msg.type === 'fill' ? GREEN : CYAN;
    const prefix = msg.type === 'error' ? '✗' : msg.type === 'warning' ? '⚠' : msg.type === 'fill' ? '✓' : 'ℹ';
    output += `${color}${prefix}${RESET} ${msg.message}\n`;
  }
  return output;
}

// Command history persistence
const HISTORY_DIR = path.join(os.homedir(), BRAND.configDirName);
const HISTORY_FILE = path.join(HISTORY_DIR, 'cli_history');
const MAX_HISTORY_SIZE = 1000;

/**
 * Load command history from file
 * Returns in reverse chronological order (most recent first) as required by readline
 */
function loadHistory(): string[] {
  try {
    if (fs.existsSync(HISTORY_FILE)) {
      const content = fs.readFileSync(HISTORY_FILE, 'utf-8');
      // Readline expects history in reverse order (most recent first)
      return content
        .split('\n')
        .filter((line) => line.trim())
        .reverse();
    }
  } catch (error) {
    // Ignore errors, start with empty history
  }
  return [];
}

/**
 * Save command history to file
 */
function saveHistory(history: string[]): void {
  try {
    if (!fs.existsSync(HISTORY_DIR)) {
      fs.mkdirSync(HISTORY_DIR, { recursive: true });
    }
    // Keep only the last MAX_HISTORY_SIZE entries
    const trimmedHistory = history.slice(-MAX_HISTORY_SIZE);
    fs.writeFileSync(HISTORY_FILE, trimmedHistory.join('\n') + '\n');
  } catch (error) {
    // Ignore errors
  }
}

/**
 * Append a single command to history file
 */
function appendToHistory(command: string): void {
  try {
    if (!fs.existsSync(HISTORY_DIR)) {
      fs.mkdirSync(HISTORY_DIR, { recursive: true });
    }
    fs.appendFileSync(HISTORY_FILE, command + '\n');
  } catch (error) {
    // Ignore errors
  }
}

// Export alternate screen codes for serve mode
export { ENTER_ALT_SCREEN, EXIT_ALT_SCREEN, CURSOR_HOME };

/**
 * Pause readline for watch/serve modes
 * This stops the readline prompt from interfering with custom output
 */
export function pauseReadline(): void {
  if (rlInstance) {
    rlInstance.pause();
  }
}

/**
 * Close readline completely for modes that need full SIGINT control (e.g., serve)
 * Use recreateReadline() to restore it afterwards
 */
export function closeReadline(): void {
  if (rlInstance) {
    isRecreatingReadline = true; // Prevent exit on close
    rlInstance.close();
    isRecreatingReadline = false;
    rlInstance = null as unknown as readline.Interface;
  }
}

/**
 * Suppress prompt updates (e.g., during pairing when we need clean output)
 */
export function setMonitorMode(value: boolean): void {
  isMonitorMode = value;
}

/**
 * Recreate readline interface after exiting watch/raw mode
 * This is necessary because raw mode corrupts readline's internal state
 * Exported for use by other modules that need to restore terminal state (e.g., pairing)
 */
/**
 * The footer + its divider live BELOW the readline prompt, so readline's
 * `_refreshLine` (which runs `clearScreenDown` on every keystroke, output line,
 * and prompt redraw) wipes them. Patch `_refreshLine` to force-repaint the
 * footer immediately after readline finishes its own redraw — no 1s flash, and
 * the divider survives. Bun implements `_refreshLine` for Node compat; if it's
 * ever absent we simply don't hook (the timer still repaints, just less
 * smoothly). Safe to call per readline instance; `redrawFooter` no-ops when the
 * footer is disabled/suspended.
 */
function hookFooterRepaint(rl: readline.Interface): void {
  const r = rl as unknown as { _refreshLine?: () => void; __footerHooked?: boolean };
  if (typeof r._refreshLine !== 'function' || r.__footerHooked) return;
  const orig = r._refreshLine;
  r._refreshLine = function patchedRefreshLine(this: unknown) {
    orig.call(this);
    redrawFooter();
  };
  r.__footerHooked = true;
}

export function recreateReadline(): void {
  // Get history from existing instance or load from file
  let history: string[] = [];
  if (rlInstance) {
    history = (rlInstance as readline.Interface & { history?: string[] }).history || [];
    isRecreatingReadline = true;
    rlInstance.close();
    isRecreatingReadline = false;
  } else {
    // Load history from file if no existing instance
    history = loadHistory();
  }

  const newRl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
    prompt: getPrompt(),
    history: [...history],
    historySize: MAX_HISTORY_SIZE,
  });

  rlInstance = newRl;
  hookFooterRepaint(newRl);

  // Re-setup event handlers
  newRl.on('line', (line: string) => {
    const trimmedLine = line.trim();
    if (trimmedLine) {
      appendToHistory(trimmedLine);
      commandQueue = commandQueue.then(() => processCommand(trimmedLine)).catch(console.error);
    }
    commandQueue.then(() => {
      newRl.setPrompt(getPrompt());
      newRl.prompt();
    });
  });

  newRl.on('close', () => {
    // Skip if we're recreating readline (not a real close)
    if (isRecreatingReadline) return;

    if (stopMonitorFn) {
      stopMonitorFn();
    }
    originalConsoleLog('\nGoodbye!');
    process.exit(0);
  });

  newRl.prompt();
}

/**
 * Start watch account mode - continuously display positions and orders
 */
function startWatchAccount(exchange: Exchange, rl: readline.Interface): void {
  if (monitorInterval) {
    clearInterval(monitorInterval);
  }

  // Enter monitor mode - this prevents connection monitor from updating prompt
  isMonitorMode = true;

  // Pause readline to prevent it from processing input
  rl.pause();

  // Create watch mode instance
  const watch = new WatchMode({
    mode: 'repl',
    useAltScreen: true,
    onBeforeCleanup: () => {
      if (monitorInterval) {
        clearInterval(monitorInterval);
        monitorInterval = null;
      }
    },
    onCleanup: () => {
      isMonitorMode = false;
      monitorExitTime = Date.now();
      stopMonitorFn = null;
      originalConsoleLog('Account watch stopped.\n');
      recreateReadline();
    },
  });

  // Render function
  const renderMonitor = () => {
    const output = formatAccountMonitorOutput(exchange) + formatWatchMessages();
    watch.render(output);
  };

  // Enter watch mode and start rendering
  watch.enter();
  renderMonitor();
  monitorInterval = setInterval(renderMonitor, 1000);

  // Store exit function for external cleanup
  stopMonitorFn = () => watch.exit();
}

/**
 * Start watch chart mode - live updating candlestick chart
 */
async function startWatchChart(
  exchange: Exchange,
  rl: readline.Interface,
  symbol: string,
  timeframe: Timeframe,
  limit: number,
): Promise<void> {
  // Enter monitor mode
  isMonitorMode = true;
  rl.pause();

  // Store candles for live updating
  let candles: Candle[] = [];
  let firstClose = 0;

  // Fetch initial candles
  try {
    candles = await exchange.fetchOHLCV({ symbol, interval: timeframe, limit });
    if (candles.length > 0) {
      firstClose = candles[0].close;
    }
  } catch (error) {
    originalConsoleLog(`Error fetching candles: ${(error as Error).message}`);
    isMonitorMode = false;
    rl.resume();
    rl.prompt();
    return;
  }

  // Get market precision
  const market = exchange.store.markets.find((m) => m.symbol === symbol);
  const pricePrecision = market?.precision?.price ?? 2;

  // Periodic re-render interval
  let chartRenderInterval: ReturnType<typeof setInterval> | null = null;

  // Create watch mode instance
  const watch = new WatchMode({
    mode: 'repl',
    useAltScreen: true,
    onBeforeCleanup: () => {
      if (chartRenderInterval) {
        clearInterval(chartRenderInterval);
        chartRenderInterval = null;
      }
      if (chartUnsubscribe) {
        chartUnsubscribe();
        chartUnsubscribe = null;
      }
    },
    onCleanup: () => {
      isMonitorMode = false;
      monitorExitTime = Date.now();
      stopMonitorFn = null;
      originalConsoleLog('Chart watch stopped.\n');
      recreateReadline();
    },
  });

  // Render function
  const renderChart = () => {
    if (!watch.isActive) return;
    if (candles.length === 0) return;

    const lastCandle = candles[candles.length - 1];
    const lastPrice = lastCandle.close;
    const header = formatChartHeader(
      symbol,
      timeframe,
      candles.length,
      lastPrice,
      firstClose,
      pricePrecision,
      lastCandle.timestamp,
    );

    const position = exchange.store.positions.find((p) => p.symbol === symbol);
    const orders = exchange.store.orders.filter((o) => o.symbol === symbol && o.status === 'open');

    const chartLines: ChartLine[] = [];
    const positionLine = position ? formatPositionLine(position, market) : null;
    if (positionLine) chartLines.push(positionLine);
    for (const order of orders) {
      const orderLine = formatOrderLine(order, pricePrecision);
      if (orderLine) chartLines.push(orderLine);
    }

    const positionSummary = formatPositionSummary(position, market);

    let countdown: string | undefined;
    const tfMs = TIMEFRAME_MS[timeframe];
    if (tfMs) {
      const candleEndTime = lastCandle.timestamp * 1000 + tfMs;
      const remaining = candleEndTime - Date.now();
      if (remaining > 0) countdown = formatCountdown(remaining);
    }

    const priceUp = lastCandle.close >= lastCandle.open;
    const chart = renderCandleChart(candles, 15, pricePrecision, lastPrice, countdown, priceUp, chartLines);

    let output = header + '\n';
    if (positionSummary) output += positionSummary + '\n';
    output += '\n' + chart + '\n\n';
    output += `${DIM}${watch.exitMessage}${RESET}\n`;
    const messages = formatWatchMessages();
    if (messages) output += messages;
    watch.render(output);
  };

  // Enter watch mode and start rendering
  watch.enter();
  renderChart();
  chartRenderInterval = setInterval(renderChart, 1000);

  // Subscribe to live updates
  chartUnsubscribe = exchange.listenOHLCV({ symbol, interval: timeframe }, (newCandle: Candle) => {
    if (candles.length > 0) {
      const lastCandle = candles[candles.length - 1];
      if (newCandle.timestamp === lastCandle.timestamp) {
        candles[candles.length - 1] = newCandle;
      } else if (newCandle.timestamp > lastCandle.timestamp) {
        candles.push(newCandle);
        if (candles.length > limit) {
          candles.shift();
          if (candles.length > 0) firstClose = candles[0].close;
        }
      }
    } else {
      candles.push(newCandle);
      firstClose = newCandle.close;
    }
    renderChart();
  });

  // Store exit function for external cleanup
  stopMonitorFn = () => watch.exit();
}

/**
 * Start watch chase mode - live updating chaser status with price charts
 */
function startWatchChase(rl: readline.Interface): void {
  // Check if there are any chasers
  const activeChasers = getActiveChasers();
  if (activeChasers.length === 0) {
    originalConsoleLog('No active chasers to watch');
    return;
  }

  // Enter monitor mode
  isMonitorMode = true;
  rl.pause();

  let chaseInterval: ReturnType<typeof setInterval> | null = null;

  // Track price history for each chaser
  const priceHistoryMap = new Map<string, number[]>();
  const MAX_PRICE_HISTORY = 120;

  // Initialize price history for each active chaser
  for (const chaser of activeChasers) {
    priceHistoryMap.set(chaser.id, []);
  }

  // Create watch mode instance
  const watch = new WatchMode({
    mode: 'repl',
    useAltScreen: true,
    onBeforeCleanup: () => {
      if (chaseInterval) {
        clearInterval(chaseInterval);
        chaseInterval = null;
      }
      priceHistoryMap.clear();
    },
    onCleanup: () => {
      isMonitorMode = false;
      monitorExitTime = Date.now();
      stopMonitorFn = null;
      originalConsoleLog('Chase watch stopped. Chasers continue in background.\n');
      recreateReadline();
    },
  });

  // Collect current prices for all chasers
  const collectPrices = () => {
    const allChasers = [...getActiveChasers()];
    for (const [id] of priceHistoryMap) {
      if (!allChasers.find((c) => c.id === id)) {
        priceHistoryMap.delete(id);
      }
    }
    for (const chaser of allChasers) {
      const ticker = chaser.exchange.store.tickers.find((t) => t.symbol === chaser.options.symbol);
      if (ticker?.last) {
        let history = priceHistoryMap.get(chaser.id);
        if (!history) {
          history = [];
          priceHistoryMap.set(chaser.id, history);
        }
        history.push(ticker.last);
        if (history.length > MAX_PRICE_HISTORY) history.shift();
      }
    }
  };

  // Render function
  const renderChasers = () => {
    if (!watch.isActive) return;
    collectPrices();
    const watchOutput = formatAllChasersStatus(priceHistoryMap) + formatWatchMessages();
    watch.render(watchOutput);
  };

  // Enter watch mode and start rendering
  watch.enter();
  renderChasers();

  chaseInterval = setInterval(() => {
    renderChasers();
    // Auto-stop when all chasers are done
    if (getActiveChasers().length === 0) {
      watch.exit();
      originalConsoleLog('\nAll chasers completed.\n');
    }
  }, 500);

  // Store exit function for external cleanup
  stopMonitorFn = () => watch.exit();
}

type WatchTableType = 'positions' | 'orders' | 'margin';

/**
 * Get all connected accounts as AccountExchange array
 */
function getAllConnectedAccounts(): AccountExchange[] {
  const connectedAccounts = exchangeManager.getConnectedAccounts();
  return connectedAccounts.map(({ name, exchange }) => ({
    accountName: name,
    exchange: exchange as Exchange,
  }));
}

/**
 * Parse --all or -a flag from command string
 * Returns { hasAllFlag: boolean, cleanedCommand: string }
 */
function parseAllFlag(command: string): { hasAllFlag: boolean; cleanedCommand: string } {
  // Check for --all or -a flag
  const hasAllFlag = /\s+--all(\s|$)/.test(command) || /\s+-a(\s|$)/.test(command);
  // Remove the flag from command
  const cleanedCommand = command
    .replace(/\s+--all(\s|$)/, ' ')
    .replace(/\s+-a(\s|$)/, ' ')
    .trim();
  return { hasAllFlag, cleanedCommand };
}

/**
 * Start a multi-account watch mode with a table formatter
 */
function startWatchTableMultiAccount(
  accounts: AccountExchange[],
  rl: readline.Interface,
  tableType: WatchTableType,
  title: string,
): void {
  // Enter monitor mode
  isMonitorMode = true;
  rl.pause();

  let watchInterval: ReturnType<typeof setInterval> | null = null;

  // Create watch mode instance
  const watch = new WatchMode({
    mode: 'repl',
    useAltScreen: true,
    onBeforeCleanup: () => {
      if (watchInterval) {
        clearInterval(watchInterval);
        watchInterval = null;
      }
    },
    onCleanup: () => {
      isMonitorMode = false;
      monitorExitTime = Date.now();
      stopMonitorFn = null;
      originalConsoleLog(`${title} watch stopped.\n`);
      recreateReadline();
    },
  });

  // Render function
  const renderTable = () => {
    if (!watch.isActive) return;

    const timestamp = new Date().toLocaleTimeString();
    let output = `═══════════════════════════════════════════════════════════════════════════════\n`;
    output += `  ${title.toUpperCase()} (ALL ACCOUNTS)                                ${timestamp}\n`;
    output += `═══════════════════════════════════════════════════════════════════════════════\n\n`;

    switch (tableType) {
      case 'positions':
        output += formatMultiAccountPositionsTable(accounts);
        break;
      case 'orders':
        output += formatMultiAccountOrdersTable(accounts);
        break;
      case 'margin':
        output += formatMultiAccountMarginTable(accounts);
        break;
    }

    output += '\n';
    output += `${DIM}${watch.exitMessage}${RESET}\n`;
    output += formatWatchMessages();
    watch.render(output);
  };

  // Enter watch mode and start rendering
  watch.enter();
  renderTable();
  watchInterval = setInterval(renderTable, 1000);

  // Store exit function for external cleanup
  stopMonitorFn = () => watch.exit();
}

/**
 * Start a multi-account watch monitor mode - full account view (margin, positions, orders)
 */
function startWatchAccountMultiAccount(accounts: AccountExchange[], rl: readline.Interface): void {
  // Enter monitor mode
  isMonitorMode = true;
  rl.pause();

  let watchInterval: ReturnType<typeof setInterval> | null = null;

  // Create watch mode instance
  const watch = new WatchMode({
    mode: 'repl',
    useAltScreen: true,
    onBeforeCleanup: () => {
      if (watchInterval) {
        clearInterval(watchInterval);
        watchInterval = null;
      }
    },
    onCleanup: () => {
      isMonitorMode = false;
      monitorExitTime = Date.now();
      stopMonitorFn = null;
      originalConsoleLog('Account watch stopped.\n');
      recreateReadline();
    },
  });

  // Render function
  const renderMonitor = () => {
    if (!watch.isActive) return;
    const output = formatMultiAccountMonitorOutput(accounts) + formatWatchMessages();
    watch.render(output);
  };

  // Enter watch mode and start rendering
  watch.enter();
  renderMonitor();
  watchInterval = setInterval(renderMonitor, 1000);

  // Store exit function for external cleanup
  stopMonitorFn = () => watch.exit();
}

/**
 * Start a generic watch mode with a table formatter
 */
function startWatchTable(exchange: Exchange, rl: readline.Interface, tableType: WatchTableType, title: string): void {
  // Enter monitor mode
  isMonitorMode = true;
  rl.pause();

  let watchInterval: ReturnType<typeof setInterval> | null = null;

  // Create watch mode instance
  const watch = new WatchMode({
    mode: 'repl',
    useAltScreen: true,
    onBeforeCleanup: () => {
      if (watchInterval) {
        clearInterval(watchInterval);
        watchInterval = null;
      }
    },
    onCleanup: () => {
      isMonitorMode = false;
      monitorExitTime = Date.now();
      stopMonitorFn = null;
      originalConsoleLog(`${title} watch stopped.\n`);
      recreateReadline();
    },
  });

  // Render function
  const renderTable = () => {
    if (!watch.isActive) return;

    const timestamp = new Date().toLocaleTimeString();
    let output = `═══════════════════════════════════════════════════════════════════════════════\n`;
    output += `  ${title.toUpperCase()}                                              ${timestamp}\n`;
    output += `═══════════════════════════════════════════════════════════════════════════════\n\n`;

    // Format the appropriate table
    switch (tableType) {
      case 'positions':
        output += formatPositionsTable(exchange);
        break;
      case 'orders':
        output += formatOrdersTable(exchange);
        break;
      case 'margin':
        output += formatMarginTable(exchange);
        break;
    }

    output += '\n';
    output += `${DIM}${watch.exitMessage}${RESET}\n`;
    output += formatWatchMessages();

    watch.render(output);
  };

  // Enter watch mode and start rendering
  watch.enter();
  renderTable();
  watchInterval = setInterval(renderTable, 1000);

  // Store exit function for external cleanup
  stopMonitorFn = () => watch.exit();
}

/**
 * Format a fill event for display
 */
function formatFill(fill: OrderFillEvent): string {
  const side = fill.side === OrderSide.Buy ? 'Bought' : 'Sold';
  const priceStr = fill.price?.toLocaleString() ?? 'market';
  return `${side} ${fill.amount} ${fill.symbol} @ $${priceStr}`;
}

/**
 * Setup event listeners on an exchange
 */
function setupExchangeListeners(exchange: ExchangeLike, accountName?: string): void {
  // Listen for command output. ExchangeLike.emitter is a generic
  // `(...args: unknown[]) => void` shape, so per-event payload types are
  // narrowed locally at each handler entry.
  exchange.emitter.on('exec-log', (...args: unknown[]) => {
    const { message } = args[0] as { message: string };
    if (isMonitorMode) {
      // Buffer messages during watch mode so they don't get wiped
      addWatchMessage('info', message);
    } else {
      console.log(message);
    }
  });

  // Listen for fills
  exchange.emitter.on('fill', (...args: unknown[]) => {
    const fill = args[0] as OrderFillEvent;
    const fillMsg = formatFill(fill);
    logger.info(`Fill: ${fillMsg}`, fill);
    // TSBUG-107 slice 4: attribute the fill back to the run/command that
    // placed the order (no-op when auditing is off or the order isn't
    // tracked). Account comes from the remembered lineage. Independent of
    // display mode.
    recordFill(fill.orderId ?? fill.id, fillMsg);
    if (isMonitorMode) {
      addWatchMessage('fill', fillMsg);
    } else {
      adapter.printSuccess(`✓ Fill: ${fillMsg}`);
    }
  });

  // Listen for errors — classify via health monitor before displaying
  exchange.emitter.on('error', (...args: unknown[]) => {
    const error = args[0] as string;
    const acctName = accountName || session.currentAccountName || 'unknown';
    const monitor = exchangeManager.getHealthMonitor(acctName);
    const category = monitor ? monitor.processMessage(error, true) : null;
    const action = classifyForCLI(category, 'error', error);

    switch (action.action) {
      case 'suppress':
        // Don't print at all
        return;
      case 'critical': {
        const key = criticalErrorKey(acctName, category!);
        if (shownCriticalErrors.has(key)) return; // Already shown
        shownCriticalErrors.add(key);
        const msg = action.message || error;
        if (isMonitorMode) {
          addWatchMessage('error', `[${category!.toUpperCase()}] ${acctName}: ${msg}`);
        } else {
          adapter.printError(
            `[${category!.toUpperCase()}] Account "${acctName}": ${msg}. Run "accounts disable ${acctName}" to stop.`,
          );
        }
        return;
      }
      case 'warn':
        if (isMonitorMode) {
          addWatchMessage('warning', error);
        } else {
          console.warn(`Exchange warning: ${error}`);
        }
        return;
      case 'error': {
        const errMsg = action.message ? `${action.message}: ${error}` : error;
        if (isMonitorMode) {
          addWatchMessage('error', errMsg);
        } else {
          console.error(`Exchange error: ${errMsg}`);
        }
        return;
      }
      case 'passthrough':
      default:
        if (isMonitorMode) {
          addWatchMessage('error', error);
        } else {
          console.error(`Exchange error: ${error}`);
        }
        return;
    }
  });

  // Listen for warnings — classify via health monitor before displaying
  exchange.emitter.on('warning', (...args: unknown[]) => {
    const warning = args[0] as string;
    const acctName = accountName || session.currentAccountName || 'unknown';
    const monitor = exchangeManager.getHealthMonitor(acctName);
    const category = monitor ? monitor.processMessage(warning, false) : null;
    const action = classifyForCLI(category, 'warning', warning);

    if (action.action === 'suppress') return;

    if (isMonitorMode) {
      addWatchMessage('warning', warning);
    } else {
      console.warn(`Exchange warning: ${warning}`);
    }
  });
}

/**
 * Generate the CLI prompt with status info
 */
function getPrompt(): string {
  // Get health status for the current account
  const healthMonitor = session.currentAccountName
    ? exchangeManager.getHealthMonitor(session.currentAccountName)
    : undefined;
  const healthStatus = healthMonitor?.getState().status;
  // Connection state: a live registered exchange is the source of truth for
  // `connected`; otherwise fall back to the tracked lifecycle so the prompt
  // shows `connecting` vs `failed`/`auth-failed` honestly.
  const acct = session.currentAccountName;
  const connectionState: AccountConnectionState | undefined = acct
    ? exchangeManager.isConnected(acct)
      ? 'connected'
      : connectionStates.get(acct)
    : undefined;
  const { prompt } = formatPrompt({
    accountName: session.currentAccountName,
    symbol: session.currentSymbol,
    exchange: session.currentExchange,
    config: promptConfig,
    healthStatus,
    connectionState,
  });
  // The CLI version moved out of the prompt prefix into the default footer
  // `version` segment (removable via `footer remove`).
  return chalk.cyan(prompt);
}

/** Active footer layout (mutated by the `footer` command; read each redraw tick). */
let footerSegments: FooterSegment[] = DEFAULT_SEGMENTS;

/**
 * Snapshot the live, in-memory state the footer segments read. All reads are
 * synchronous (streamed store + tracked connection state) — no API calls — so
 * this is safe to call on the footer's redraw tick. Mirrors getPrompt's
 * connection derivation.
 */
function buildFooterState(): FooterState {
  const acct = session.currentAccountName;
  const symbol = session.currentSymbol;

  let connection: FooterConnection = 'unknown';
  if (acct) {
    connection = exchangeManager.isConnected(acct)
      ? 'connected'
      : ((connectionStates.get(acct) as FooterConnection | undefined) ?? 'disconnected');
  }

  const state: FooterState = {
    connection,
    account: acct,
    symbol,
    version: getCurrentVersion(),
    now: new Date(),
    scriptValues: footerScriptCache.size ? Object.fromEntries(footerScriptCache) : undefined,
  };

  const store = acct ? exchangeManager.getExchange(acct)?.store : undefined;
  if (store) {
    const bal = store.balance;
    if (bal) {
      state.balanceFree = bal.free;
      state.balanceTotal = bal.total;
      if (typeof bal.upnl === 'number') state.totalUpnl = bal.upnl;
    }
    const positions = store.positions ?? [];
    if (state.totalUpnl == null) {
      state.totalUpnl = positions.reduce((sum, p) => sum + (p.unrealizedPnl ?? 0), 0);
    }
    if (symbol) {
      const market = (store.markets ?? []).find((m) => m.symbol === symbol);
      const pos = positions.find((p) => p.symbol === symbol && p.contracts !== 0);
      if (pos) {
        state.position = {
          side: pos.side as 'long' | 'short',
          size: `${formatBaseSize(pos.contracts, market)}${market?.base ? ' ' + market.base : ''}`,
          entryPrice: pos.entryPrice,
          notional: pos.notional,
          unrealizedPnl: pos.unrealizedPnl,
        };
      }
      const ticker = (store.tickers ?? []).find((t) => t.symbol === symbol);
      if (ticker) {
        state.mark = ticker.mark;
        state.last = ticker.last;
        state.fundingRate = ticker.fundingRate;
      }
    }
  }
  return state;
}

/** The renderer handed to the footer surface: layout × live snapshot. */
function footerRenderer(): { left: string; right: string } {
  return renderFooter(footerSegments, buildFooterState());
}

// ── Script-backed footer segments ───────────────────────────────────────────
// A `{ type:'script', name }` segment runs the named script off the redraw tick
// and renders its returned string. Three hard safety rules, because this auto-
// runs ~1×/sec:
//   1. READ-ONLY: capabilities forced to [] so any placeOrder/cancel/runCommand
//      throws (caught → error token). A footer can never trade.
//   2. SILENT: a no-op CoreAdapter.print so footer scripts can't spam the
//      terminal (only the return value is used).
//   3. NON-BLOCKING: timeout race + per-script in-flight guard so a slow/hung
//      script never stalls the loop or stacks runs; the last good value stays
//      cached and renders meanwhile.
const FOOTER_SCRIPT_TIMEOUT_MS = 2000;
const footerScriptCache = new Map<string, string>();
const footerScriptInflight = new Set<string>();
let footerScriptTimer: ReturnType<typeof setInterval> | null = null;

// Capabilities a footer script runs under: NO writes, NO runCommand (omitted →
// disabled), NO storage. Reads stay open (`reads` undefined). `accounts: []` =
// no account restriction (reads resolve against the active account). This is
// the hard read-only floor that makes 1Hz auto-execution safe.
const FOOTER_READONLY_CAPS: Capabilities = {
  accounts: [],
  writes: [],
  idempotent: true,
  storage: { enabled: false, quotaBytes: 0 },
};

async function runFooterScript(name: string): Promise<void> {
  // In-flight guard: never stack runs for the same script. Held until the REAL
  // run settles (not the timeout), so a hung script can't spawn a fresh run
  // every tick — it stays in-flight and renders its provisional error token.
  if (footerScriptInflight.has(name)) return;
  const record = lookupScript(name);
  if (!record) {
    footerScriptCache.set(name, `?${name}`); // unknown script
    return;
  }
  footerScriptInflight.add(name);
  let settled = false;
  // Overrun → surface `!name` so the segment isn't blank; the run keeps going
  // and overwrites the cache if/when it lands.
  const slowTimer = setTimeout(() => {
    if (!settled) footerScriptCache.set(name, `!${name}`);
  }, FOOTER_SCRIPT_TIMEOUT_MS);
  slowTimer.unref?.();
  try {
    // Read-only: deny writes + runCommand so the footer can never trade.
    const roRecord = { ...record, capabilities: FOOTER_READONLY_CAPS };
    // Silent: footer-script prints must not reach the terminal — only the
    // return value is rendered.
    const silent: CoreAdapter = {
      ...createStandaloneCoreAdapter(exchangeManager, session, adapter),
      print: () => {},
    };
    const result: RunScriptResult = await runScriptFromHostWithAdapter(
      { adapter: silent, transpile: getCliTranspile() },
      { record: roRecord, rawArgs: '', invokedFrom: 'inline' },
    );
    if (result.ok) {
      const raw =
        typeof result.returnValue === 'string'
          ? result.returnValue
          : result.returnValue != null
            ? String(result.returnValue)
            : result.output.join(' ');
      footerScriptCache.set(name, raw.replace(/\s+/g, ' ').trim());
    } else {
      footerScriptCache.set(name, `!${name}`); // error token (incl. denied writes)
    }
  } catch {
    footerScriptCache.set(name, `!${name}`);
  } finally {
    settled = true;
    clearTimeout(slowTimer);
    footerScriptInflight.delete(name);
  }
}

function refreshFooterScripts(): void {
  for (const seg of footerSegments) {
    if (seg.type === 'script' && seg.name) void runFooterScript(seg.name);
  }
}

/** Start the off-tick script refresh loop (no-op without script segments). */
function startFooterScriptRefresh(intervalMs?: number): void {
  if (footerScriptTimer || !footerSegments.some((s) => s.type === 'script')) return;
  refreshFooterScripts();
  footerScriptTimer = setInterval(refreshFooterScripts, Math.max(500, intervalMs ?? 1000));
  footerScriptTimer.unref?.();
}

function stopFooterScriptRefresh(): void {
  if (footerScriptTimer) {
    clearInterval(footerScriptTimer);
    footerScriptTimer = null;
  }
  footerScriptCache.clear();
  footerScriptInflight.clear();
}

/**
 * Process a single command - first try CLI-specific commands,
 * then session commands from cli-core
 */
async function processSingleCommand(command: string, captureTo?: string): Promise<void> {
  // Handle login command
  if (command === 'login') {
    await login();
    // The relay authenticated with the token that was current when it
    // connected; reconnect it as whoever is signed in now.
    restartWebhookRelay();
    return;
  }

  // Handle logout command
  if (command === 'logout') {
    logout();
    // Alerts must stop with the session, not with the socket.
    restartWebhookRelay();
    return;
  }

  // Handle auth status command
  if (command === 'auth' || command === 'whoami') {
    showAuthStatus();
    return;
  }

  // Handle trusted-client management
  if (command === 'clients' || command === 'clients list') {
    cmdClientsList(adapter);
    return;
  }
  if (command.startsWith('clients revoke')) {
    const target = command.slice('clients revoke'.length).trim();
    cmdClientsRevoke(adapter, target || undefined);
    return;
  }

  // Hosted webhook relay
  if (command === 'webhook' || command === 'webhook status') {
    await cmdWebhookStatus(adapter);
    return;
  }
  if (command === 'webhook claim') {
    await cmdWebhookClaim(adapter);
    return;
  }
  if (command === 'webhook release') {
    await cmdWebhookRelease(adapter);
    return;
  }

  // Handle webhook management
  if (command === 'hook' || command === 'hook list' || command === 'hooks') {
    cmdHookList(adapter);
    return;
  }
  if (command.startsWith('hook create ')) {
    cmdHookCreate(adapter, command.slice('hook create '.length).trim());
    return;
  }
  if (command.startsWith('hook revoke ')) {
    cmdHookRevoke(adapter, command.slice('hook revoke '.length).trim() || undefined);
    return;
  }
  if (command.startsWith('hook rotate ')) {
    cmdHookRotate(adapter, command.slice('hook rotate '.length).trim() || undefined);
    return;
  }
  if (command.startsWith('hook show ')) {
    cmdHookShow(adapter, command.slice('hook show '.length).trim() || undefined);
    return;
  }

  // Handle update command
  if (command === 'update') {
    await performUpdate();
    return;
  }

  // Handle update check command
  if (command === 'update --check' || command === 'update -c') {
    await forceCheckForUpdates();
    return;
  }

  // Handle logs command locally
  if (command === 'logs' || command === 'log') {
    adapter.printInfo(`Log file: ${logger.getLogPath()}`);
    adapter.printInfo(`Tail logs in another terminal with:`);
    adapter.printSuccess(`  tail -f ${logger.getLogPath()}`);
    return;
  }

  if (command === 'logs clear' || command === 'clearlogs') {
    logger.clear();
    adapter.printSuccess('Logs cleared');
    return;
  }

  // `audit` — durable append-only write-op log (TSBUG-107). CLI-only
  // (writes to ~/.tealstreet/audit.log); off by default.
  if (command === 'audit' || command.startsWith('audit ')) {
    const sub = command.slice('audit'.length).trim().toLowerCase();
    if (sub === 'on') {
      setAuditEnabled(true);
      adapter.printSuccess(`Audit log ON — write-ops append to ${AUDIT_LOG_PATH}`);
    } else if (sub === 'off') {
      setAuditEnabled(false);
      adapter.printSuccess('Audit log OFF');
    } else if (sub === '' || sub === 'status') {
      adapter.printInfo(`Audit log: ${isAuditEnabled() ? 'ON' : 'OFF'} (${AUDIT_LOG_PATH})`);
    } else if (sub === 'tail' || sub.startsWith('tail ')) {
      const n = parseInt(sub.slice('tail'.length).trim(), 10);
      const entries = readAuditTail(Number.isFinite(n) && n > 0 ? n : 20);
      if (entries.length === 0) {
        adapter.printInfo('Audit log is empty');
      } else {
        for (const e of entries) {
          const run = e.runId ? ` run=${e.runId}` : '';
          const ids = e.orderIds?.length ? ` ids=${e.orderIds.join(',')}` : '';
          const err = e.error ? ` error="${e.error}"` : '';
          adapter.printInfo(`${e.ts}${run} [${e.account}] ${e.ok ? 'ok' : 'FAIL'} ${e.command}${ids}${err}`);
        }
      }
    } else {
      adapter.printError('Usage: audit [on|off|status|tail [N]]');
    }
    return;
  }

  // Persistent bottom status footer. CLI-only (no FS on web).
  if (command === 'footer' || command.startsWith('footer ')) {
    // Interactive-CLI only: the footer is a local terminal surface, so a
    // web-tab `cmd:exec` / webhook run must not read or persist footer config.
    const ctxSource = getAdapterContext()?.source;
    if (ctxSource === 'cmd-exec' || ctxSource === 'webhook') {
      adapter.printError('The footer command is only available in the interactive CLI.');
      return;
    }
    const sub = command.slice('footer'.length).trim();
    const subLower = sub.toLowerCase();
    const usage = 'Usage: footer [on|off|list|reset|add <type|script <name>|text <value>> [right]|remove <index>]';
    const cfg = loadConfig();
    cfg.footer = cfg.footer ?? {};
    // Nullish (not truthy-length): an explicit `[]` is a user-emptied layout and
    // must be preserved, not resurrected to DEFAULT_SEGMENTS.
    const segsOrDefault = (): FooterSegment[] =>
      cfg.footer?.segments ? [...cfg.footer.segments] : [...DEFAULT_SEGMENTS];
    // Re-apply the live surface from the (just-mutated + saved) config. On by
    // default — only an explicit `enabled: false` opts out.
    const apply = (): void => {
      footerSegments = segsOrDefault();
      if (cfg.footer?.enabled !== false) {
        setFooterRenderer(footerRenderer);
        enableFooter(cfg.footer?.intervalMs);
        startFooterScriptRefresh(cfg.footer?.intervalMs);
      } else {
        disableFooter();
        stopFooterScriptRefresh();
      }
    };

    if (subLower === 'on') {
      cfg.footer.enabled = true;
      saveConfig(cfg);
      apply();
      adapter.printSuccess('Footer ON');
    } else if (subLower === 'off') {
      cfg.footer.enabled = false;
      saveConfig(cfg);
      disableFooter();
      stopFooterScriptRefresh();
      adapter.printSuccess('Footer OFF');
    } else if (subLower === 'reset') {
      cfg.footer.segments = [...DEFAULT_SEGMENTS];
      saveConfig(cfg);
      apply();
      adapter.printSuccess('Footer segments reset to default');
    } else if (subLower === '' || subLower === 'list' || subLower === 'status') {
      const segs = segsOrDefault();
      adapter.printInfo(`Footer: ${cfg.footer.enabled !== false ? 'ON' : 'OFF'}`);
      segs.forEach((s, i) => {
        const g = s.group === 'right' ? ' [right]' : '';
        const extra = s.type === 'text' ? ` "${s.text ?? ''}"` : s.type === 'script' ? ` ${s.name ?? '?'}` : '';
        adapter.printInfo(`  ${i}. ${s.type}${extra}${g}`);
      });
      adapter.printInfo(usage);
    } else if (subLower.startsWith('add ')) {
      const parts = sub.slice('add '.length).trim().split(/\s+/);
      const type = parts[0]?.toLowerCase() as FooterSegmentType;
      if (!FOOTER_SEGMENT_TYPES.includes(type)) {
        adapter.printError(`Unknown segment type '${parts[0]}'. Valid: ${FOOTER_SEGMENT_TYPES.join(', ')}`);
        return;
      }
      // A trailing `right` token (after the type/name/text) sends it to the
      // right group. `script <name>` takes a name; `text <value...>` takes the
      // rest as literal content.
      const isRight = parts.length > 1 && parts[parts.length - 1].toLowerCase() === 'right';
      const group = isRight ? 'right' : undefined;
      const name = type === 'script' ? parts[1] : undefined;
      const text = type === 'text' ? parts.slice(1, isRight ? -1 : undefined).join(' ') : undefined;
      if (type === 'script' && !name) {
        adapter.printError('Usage: footer add script <name> [right]');
        return;
      }
      if (type === 'text' && !text) {
        adapter.printError('Usage: footer add text <value> [right]');
        return;
      }
      const seg: FooterSegment = {
        type,
        ...(name ? { name } : {}),
        ...(text ? { text } : {}),
        ...(group ? { group } : {}),
      };
      cfg.footer.segments = [...segsOrDefault(), seg];
      saveConfig(cfg);
      apply();
      const labelStr = name ? ` ${name}` : text ? ` "${text}"` : '';
      adapter.printSuccess(`Added '${type}${labelStr}'${group ? ' [right]' : ''}`);
    } else if (subLower.startsWith('remove ')) {
      const idx = parseInt(sub.slice('remove '.length).trim(), 10);
      const segs = segsOrDefault();
      if (!Number.isInteger(idx) || idx < 0 || idx >= segs.length) {
        adapter.printError(`Invalid index. Use 'footer list' to see indices (0..${segs.length - 1}).`);
        return;
      }
      const [removed] = segs.splice(idx, 1);
      cfg.footer.segments = segs;
      saveConfig(cfg);
      apply();
      adapter.printSuccess(`Removed '${removed.type}'`);
    } else {
      adapter.printError(usage);
    }
    return;
  }

  // Re-scan `~/.tealstreet/scripts/` and rebuild the in-memory registry.
  // Phase 5's CLI dispatcher will read from this registry. Edits in the
  // user's IDE don't auto-reload — they must `script reload` after
  // saving (matches the v1 throwaway shape per SPEC §5.1).
  //
  // `loadScripts` is imported statically at the top of this file so the
  // call publishes the inflight promise SYNCHRONOUSLY before this
  // handler yields on `await`. That way, a chain like
  // `script reload; script:foo` sees the FRESH promise when the
  // `script:foo` arm calls `awaitScriptsLoaded()` — the previous
  // dynamic-import shape created a yield BEFORE `loadScripts` ran, so
  // the next step picked up the stale registry.
  if (command === 'script reload' || command === 'scripts reload') {
    const loadPromise = loadScripts();
    try {
      const scripts = await loadPromise;
      adapter.printSuccess(`Loaded ${scripts.length} script${scripts.length === 1 ? '' : 's'} from ${SCRIPTS_DIR}`);
      for (const s of scripts) {
        adapter.printInfo(`  - ${s.name}`);
      }
    } catch (err) {
      adapter.printError(`Failed to reload scripts: ${err instanceof Error ? err.message : String(err)}`);
    }
    return;
  }

  // Handle watch account command - live updating positions, orders, and margin
  // Supports: wa, wa --all, wa -a, watch account, watch account --all, etc.
  if (command.match(/^(watch\s+acc(ount)?|wa)(\s|$)/)) {
    const interactiveError = checkInteractiveMode();
    if (interactiveError) {
      adapter.printError(interactiveError);
      return;
    }

    const { hasAllFlag } = parseAllFlag(command);

    if (hasAllFlag) {
      const accounts = getAllConnectedAccounts();
      if (accounts.length === 0) {
        adapter.printError('No accounts connected');
        return;
      }
      startWatchAccountMultiAccount(accounts, rlInstance!);
    } else {
      if (!session.currentExchange) {
        adapter.printError('Not connected to any exchange');
        return;
      }
      startWatchAccount(session.currentExchange as Exchange, rlInstance!);
    }
    return;
  }

  // Handle watch positions command - live updating positions table
  // Supports: wp, wp --all, wp -a, watch pos, watch position, watch positions, --all variants, etc.
  if (command.match(/^(watch\s+pos(?:ition)?s?|wp)(\s|$)/)) {
    const interactiveError = checkInteractiveMode();
    if (interactiveError) {
      adapter.printError(interactiveError);
      return;
    }

    const { hasAllFlag } = parseAllFlag(command);

    if (hasAllFlag) {
      const accounts = getAllConnectedAccounts();
      if (accounts.length === 0) {
        adapter.printError('No accounts connected');
        return;
      }
      startWatchTableMultiAccount(accounts, rlInstance!, 'positions', 'Positions');
    } else {
      if (!session.currentExchange) {
        adapter.printError('Not connected to any exchange');
        return;
      }
      startWatchTable(session.currentExchange as Exchange, rlInstance!, 'positions', 'Positions');
    }
    return;
  }

  // Handle watch orders command - live updating orders table
  // Supports: wo, wo --all, wo -a, watch ord, watch order, watch orders, --all variants, etc.
  if (command.match(/^(watch\s+ord(?:er)?s?|wo)(\s|$)/)) {
    const interactiveError = checkInteractiveMode();
    if (interactiveError) {
      adapter.printError(interactiveError);
      return;
    }

    const { hasAllFlag } = parseAllFlag(command);

    if (hasAllFlag) {
      const accounts = getAllConnectedAccounts();
      if (accounts.length === 0) {
        adapter.printError('No accounts connected');
        return;
      }
      startWatchTableMultiAccount(accounts, rlInstance!, 'orders', 'Orders');
    } else {
      if (!session.currentExchange) {
        adapter.printError('Not connected to any exchange');
        return;
      }
      startWatchTable(session.currentExchange as Exchange, rlInstance!, 'orders', 'Orders');
    }
    return;
  }

  // Handle watch margin command - live updating margin/balance table
  // Supports: wm, wm --all, wm -a, watch margin, watch bal, watch balance, --all variants, etc.
  if (command.match(/^(watch\s+(margin|balance|bal)|wm)(\s|$)/)) {
    const interactiveError = checkInteractiveMode();
    if (interactiveError) {
      adapter.printError(interactiveError);
      return;
    }

    const { hasAllFlag } = parseAllFlag(command);

    if (hasAllFlag) {
      const accounts = getAllConnectedAccounts();
      if (accounts.length === 0) {
        adapter.printError('No accounts connected');
        return;
      }
      startWatchTableMultiAccount(accounts, rlInstance!, 'margin', 'Margin');
    } else {
      if (!session.currentExchange) {
        adapter.printError('Not connected to any exchange');
        return;
      }
      startWatchTable(session.currentExchange as Exchange, rlInstance!, 'margin', 'Margin');
    }
    return;
  }

  // Handle positions command with --all flag (one-shot, non-watch mode)
  // Supports: positions --all, positions -a, ps --all, ps -a
  // Reuses the same table format as watch positions
  if (command.match(/^(positions?|ps)(\s|$)/)) {
    const { hasAllFlag } = parseAllFlag(command);

    if (hasAllFlag) {
      const accounts = getAllConnectedAccounts();
      if (accounts.length === 0) {
        adapter.printError('No accounts connected');
        return;
      }
      // Reuse same table format as watch positions
      const output = formatMultiAccountPositionsTable(accounts);
      console.log(output);
      return;
    }
    // If no --all flag, fall through to the safe-cex router
  }

  // Handle orders command with --all flag (one-shot, non-watch mode)
  // Supports: orders --all, orders -a
  // Reuses the same table format as watch orders
  if (command.match(/^orders?(\s|$)/)) {
    const { hasAllFlag } = parseAllFlag(command);

    if (hasAllFlag) {
      const accounts = getAllConnectedAccounts();
      if (accounts.length === 0) {
        adapter.printError('No accounts connected');
        return;
      }
      // Reuse same table format as watch orders
      const output = formatMultiAccountOrdersTable(accounts);
      console.log(output);
      return;
    }
    // If no --all flag, fall through to the safe-cex router
  }

  // Handle balance command with --all flag (one-shot, non-watch mode)
  // Supports: balance --all, balance -a, bal --all, bal -a
  // Reuses the same table format as watch margin
  if (command.match(/^(balance|bal)(\s|$)/)) {
    const { hasAllFlag } = parseAllFlag(command);

    if (hasAllFlag) {
      const accounts = getAllConnectedAccounts();
      if (accounts.length === 0) {
        adapter.printError('No accounts connected');
        return;
      }
      // Reuse same table format as watch margin
      const output = formatMultiAccountMarginTable(accounts);
      console.log(output);
      return;
    }
    // If no --all flag, fall through to the safe-cex router
  }

  // Handle margin command (one-shot, non-watch mode)
  // Supports: margin, margin --all, margin -a
  // Reuses the same table format as watch margin
  if (command.match(/^margin(\s|$)/)) {
    const { hasAllFlag } = parseAllFlag(command);

    if (hasAllFlag) {
      const accounts = getAllConnectedAccounts();
      if (accounts.length === 0) {
        adapter.printError('No accounts connected');
        return;
      }
      const output = formatMultiAccountMarginTable(accounts);
      console.log(output);
      return;
    } else {
      // Single account margin
      if (!session.currentExchange) {
        adapter.printError('Not connected to any exchange');
        return;
      }
      const output = formatMarginTable(session.currentExchange as Exchange);
      console.log(output);
      return;
    }
  }

  // Handle spot command (one-shot, non-watch mode)
  // Supports: spot, spot --all, spot -a
  if (command.match(/^spot(\s|$)/)) {
    const { hasAllFlag } = parseAllFlag(command);

    if (hasAllFlag) {
      const accounts = getAllConnectedAccounts();
      if (accounts.length === 0) {
        adapter.printError('No accounts connected');
        return;
      }
      const output = formatMultiAccountSpotTable(accounts);
      console.log(output);
      return;
    } else {
      // Single account spot
      if (!session.currentExchange) {
        adapter.printError('Not connected to any exchange');
        return;
      }
      const output = formatSpotTable(session.currentExchange as Exchange);
      console.log(output);
      return;
    }
  }

  // Handle watch chart command - live updating chart
  if (command.startsWith('watch chart') || command.startsWith('watch c ')) {
    const interactiveError = checkInteractiveMode();
    if (interactiveError) {
      adapter.printError(interactiveError);
      return;
    }
    if (!session.currentExchange) {
      adapter.printError('Not connected to any exchange');
      return;
    }

    // Parse chart args (remove 'watch' prefix and parse like regular chart command).
    // parseChartParams now takes a CommandContext — build a minimal one carrying
    // just the fields the parser reads (`exchange`, `symbol`, `args._`, `adapter`).
    const chartRaw = command.replace(/^watch\s+/, '');
    const chartCtx: CommandContext = {
      raw: chartRaw,
      args: parseArgs(chartRaw),
      symbol: session.currentSymbol,
      accountName: session.currentAccountName,
      exchange: session.currentExchange as ExchangeLike,
      adapter,
    };
    const params = chartCommand.parseChartParams(chartCtx, chartCtx.args._);

    if ('error' in params) {
      adapter.printError(params.error);
      return;
    }

    await startWatchChart(
      session.currentExchange as Exchange,
      rlInstance!,
      params.symbol,
      params.timeframe,
      params.limit,
    );
    return;
  }

  // Handle watch chase command - live updating chaser status
  if (command === 'watch chase' || command.startsWith('watch chase ')) {
    const interactiveError = checkInteractiveMode();
    if (interactiveError) {
      adapter.printError(interactiveError);
      return;
    }
    startWatchChase(rlInstance!);
    return;
  }

  // Log the command
  logger.info(`Command: ${command}`);

  // Per-call overrides from the listener's `cmd:exec` handler.
  // Mirror of the resolution in `processCommand` — see the comment
  // there for why we don't mutate `session.*` on the override path.
  const ctx = getAdapterContext();
  const overrideAccount = ctx?.accountOverride;
  const overrideSymbol = ctx?.symbolOverride;
  const effectiveAccount = overrideAccount ?? session.currentAccountName;
  const effectiveSymbol = overrideSymbol ?? session.currentSymbol;
  const effectiveExchange =
    overrideAccount !== undefined ? (exchangeManager.getExchange(overrideAccount) ?? null) : session.currentExchange;
  const isOverrideRun = overrideAccount !== undefined || overrideSymbol !== undefined;

  // Execute command via cli-core
  const result = await cliExecuteCommand(
    command,
    effectiveSymbol,
    effectiveAccount,
    effectiveExchange,
    adapter,
    promptConfig,
    preprocessStore,
  );

  // TSBUG-107: durable write-op audit trail (off by default). Recorded
  // BEFORE the failure throw below so failed writes are logged too.
  recordWriteOp(command, effectiveAccount ?? 'unknown', result);

  if (result.handled && result.success === false) {
    // The failure has to become a throw so `retry` / `set ec` / blocking
    // waits see it (cli-core/CLAUDE.md, "Runner failure contract"). Carry
    // whether the executor already showed it to the user, or reportStepError
    // prints the same sentence a second time under a different prefix.
    throw commandResultError(result);
  }

  // `<var> = <body>` capture (preprocess sets step.captureTo; runner
  // passes it in here). Writes the comma-joined order-ID list into
  // the static var via the existing `set` path, so `cancel @<var>`
  // substitutes the list and cancel's existing comma-list handling
  // takes care of the rest. Empty array → empty var + a warning so
  // the user knows the exchange call didn't expose IDs (e.g. Bybit
  // position TP/SL).
  if (captureTo !== undefined && result.handled) {
    const ids = result.data?.orderIds ?? [];
    preprocessStore.setVar(captureTo, ids.join(','));
    if (ids.length === 0) {
      adapter.printWarning(
        `@${captureTo}: captured 0 order IDs — the underlying call didn't return any (e.g. position TP/SL on Bybit)`,
      );
    } else {
      adapter.printInfo(`@${captureTo} = ${ids.join(',')}`);
    }
  }

  // Update session if changed. Skip on override runs — a web tab's
  // `account <name>` shouldn't hijack the local REPL's current account.
  if (result.sessionChanged && result.newSession && !isOverrideRun) {
    // Update session state
    if (result.newSession.currentSymbol !== undefined) {
      session.currentSymbol = result.newSession.currentSymbol;
    }
    if (result.newSession.currentExchange !== undefined) {
      session.currentExchange = result.newSession.currentExchange;
    }
    if (result.newSession.currentAccountName !== undefined) {
      session.currentAccountName = result.newSession.currentAccountName;
    }
    // Update prompt config
    if (result.newSession.promptConfig !== undefined) {
      promptConfig = result.newSession.promptConfig;
    }

    // Setup listeners for new exchange
    if (result.newSession.currentExchange) {
      setupExchangeListeners(result.newSession.currentExchange, result.newSession.currentAccountName);
      // Update network monitor with new exchange reference
      networkMonitor?.setExchange(result.newSession.currentExchange);

      // Register the new primary exchange with the manager
      const accountConfig = adapter.getAccount(result.newSession.currentAccountName);
      if (accountConfig) {
        exchangeManager.register(
          result.newSession.currentAccountName!,
          result.newSession.currentExchange,
          accountConfig,
        );
      }
    }
  }

  // Handle account actions (enable/disable streaming)
  if (result.accountAction) {
    const { action, accountName, accountNames } = result.accountAction;
    // Support both single accountName and multiple accountNames
    const names = accountNames || (accountName ? [accountName] : []);

    for (const name of names) {
      const accountConfig = adapter.getAccount(name);

      if (!accountConfig) {
        adapter.printError(`Account "${name}" not found`);
        continue;
      }

      if (action === 'enable') {
        // Enable the account - start streaming
        if (exchangeManager.isConnected(name)) {
          adapter.printInfo(`Account "${name}" is already enabled`);
        } else {
          try {
            adapter.printInfo(`Enabling account "${name}"...`);
            const exchange = await exchangeManager.enable(accountConfig);
            setupExchangeListeners(exchange, name);
            adapter.printSuccess(`Account "${name}" enabled`);
          } catch (error) {
            adapter.printError(`Failed to enable account "${name}": ${(error as Error).message}`);
          }
        }
      } else if (action === 'disable') {
        // Disable the account - stop streaming
        if (!exchangeManager.isConnected(name)) {
          adapter.printInfo(`Account "${name}" is already disabled`);
        } else {
          try {
            // If disabling the primary account, clear the session first
            if (name === session.currentAccountName) {
              session.currentExchange = null;
              session.currentAccountName = undefined;
            }
            await exchangeManager.disable(name);
            adapter.printSuccess(`Account "${name}" disabled`);
          } catch (error) {
            adapter.printError(`Failed to disable account "${name}": ${(error as Error).message}`);
          }
        }
      }
    }
  }

  // Handle task actions (TWAP, chaser, DCA)
  if (result.taskAction) {
    const { action, type, taskId, taskIds, params } = result.taskAction;

    if (action === 'start' && type === 'twap' && params) {
      // Start a new TWAP using the worker-based TwapManager. Use the
      // override-resolved exchange/account (a web tab's `cmd:exec` with
      // an `account` field must run the TWAP against THAT account, not
      // the long-lived REPL session) — same resolution the command ran
      // under above.
      const exchange = effectiveExchange as Exchange;
      if (!exchange) {
        adapter.printError('Not connected to exchange');
      } else {
        try {
          // Get market for the symbol
          const symbol = params.symbol as string;
          const market = exchange.store.markets.find((m: Market) => m.symbol === symbol);
          if (!market) {
            adapter.printError(`Market not found for symbol ${symbol}`);
          } else {
            // Extract TWAP params (with optional twap-chase fields)
            const twapParams = params.params as {
              totalSize: number;
              duration: number;
              slices: number;
              useChaserOrders?: boolean;
              reduceOnly?: boolean;
              timeoutPct?: number;
              timeoutAction?: 'market' | 'next';
              failAction?: 'market' | 'next';
            };

            // Create TWAP instance with proper options
            // Cast to Worker since TwapInstance only uses postMessage
            const twapContext = {
              postMessage: (_msg: unknown) => {
                // No-op context for CLI - TWAP updates go to twapManager.runningTwaps
              },
            } as unknown as Worker;

            const twapInstance = new TwapInstance(
              exchange,
              twapContext,
              {
                accountName: effectiveAccount || 'default',
                activeMarket: market,
                size: twapParams.totalSize,
                duration: twapParams.duration,
                ordersCount: twapParams.slices,
                side: params.side as OrderSide,
                chaserDelay: 1000,
                reduceOnly: twapParams.reduceOnly ?? false,
                randomnessPercentage: 0,
                useLimitOrders: false,
                useChaserOrders: twapParams.useChaserOrders || false,
                timeoutPct: twapParams.timeoutPct,
                timeoutAction: twapParams.timeoutAction,
                failAction: twapParams.failAction,
                origin: 'cli',
              },
              () => {
                // State updated callback - log progress
                const state = twapInstance.state;
                logger.info(`[TWAP] ${state.ordersExecuted}/${state.ordersCount} slices executed`);
              },
            );

            twapManager.add(twapInstance);
            twapInstance.start();
            adapter.printSuccess(`TWAP started [${twapInstance.id.slice(0, 8)}]`);
          }
        } catch (error) {
          adapter.printError(`Failed to start TWAP: ${(error as Error).message}`);
        }
      }
    } else if (action === 'stop' && type === 'twap') {
      // Stop TWAP(s)
      const idsToStop = taskIds || (taskId ? [taskId] : []);
      for (const id of idsToStop) {
        const twap = twapManager.getById(id);
        if (twap) {
          twap.terminate();
          adapter.printSuccess(`TWAP [${id.slice(0, 8)}] stopped`);
        } else {
          adapter.printError(`TWAP [${id.slice(0, 8)}] not found`);
        }
      }
    }
  }

  // If session command was handled, we're done
  if (result.handled) {
    return;
  }

  // Unknown command
  const cmdName = command.split(/\s+/)[0];
  adapter.printError(`Unknown command: ${cmdName}. Type 'help' for available commands.`);
}

/**
 * Process a command string. Runs the cli-core preprocess pipeline
 * (repeat expansion, chain split on `;` / `&`, alias + var + $constant
 * substitution) and dispatches each step.
 *
 * Chain semantics: `;` is sequential — the runner drains all in-flight
 * fire-and-forget steps before dispatching a `;`-separated step (its
 * `awaitPrior` flag), so `close all; buy $1000` waits for the close.
 * `&` is parallel — no barrier. `sleep`/`wait` (block:true), retry, and
 * capture steps are awaited inline regardless.
 *
 * The legacy `@cmd` parallel prefix is recognised by the preprocessor
 * (stripped + one warning per invocation) for backwards compatibility;
 * users should migrate to `&`.
 */
export async function processCommand(
  commandString: string,
  options: { drainFinalInFlight?: boolean } = {},
): Promise<void> {
  // Per-call overrides from the listener's `cmd:exec` handler. When a
  // web tab dispatches a command with `account`/`symbol` set, scope the
  // run to those values instead of the REPL's long-lived session — so
  // `buy 100` from a web tab on bybit-main always hits bybit-main even
  // if the local REPL is currently switched to hl-main. Overrides do
  // NOT mutate `session.*`; the REPL stays on whatever it was.
  const ctx = getAdapterContext();
  const effectiveAccount = ctx?.accountOverride ?? session.currentAccountName;
  const effectiveSymbol = ctx?.symbolOverride ?? session.currentSymbol;
  const effectiveExchange =
    ctx?.accountOverride !== undefined
      ? (exchangeManager.getExchange(ctx.accountOverride) ?? null)
      : session.currentExchange;

  if (ctx?.untrustedBody) {
    const bodyRefusal = webhookBodyRefusal(commandString);
    if (bodyRefusal) {
      adapter.printError(bodyRefusal);
      return;
    }
  }

  let result;
  try {
    result = preprocessCommand(commandString, {
      exchange: effectiveExchange,
      symbol: effectiveSymbol,
      accountName: effectiveAccount,
      store: preprocessStore,
    });
  } catch (e) {
    adapter.printError((e as Error).message);
    return;
  }

  for (const w of result.warnings) {
    adapter.printWarning(w.message);
  }

  if (ctx?.untrustedBody) {
    const chainRefusal = webhookChainRefusal(result.steps);
    if (chainRefusal) {
      adapter.printError(chainRefusal);
      return;
    }
  }

  // Chain execution with `;` / `&` semantics. Fire-and-forget steps
  // (`&`, and the default) are tracked in `inFlight`. A `;`-separated
  // step (awaitPrior) is a barrier: drain all in-flight steps before
  // dispatching it, so `close all; buy $1000` waits for the close to
  // finish before buying. `&` steps don't drain. block/retry/capture
  // steps are awaited inline as before (pacing / capture ordering).
  //
  // The whole chain runs under one audit correlation id (TSBUG-107
  // slice 3) so every write — including a `repeat`'s expanded steps and
  // a script's internal orders — shares a `runId`. The id flows via
  // AsyncLocalStorage, so even `&`/default steps that settle after this
  // function returns still carry it (they were spawned inside the scope).
  await runWithAuditRunId(newRunId(), async () => {
    const inFlight: Array<Promise<unknown>> = [];
    for (const step of result.steps) {
      if (step.awaitPrior && inFlight.length > 0) {
        await Promise.allSettled(inFlight.splice(0));
      }
      const p = runStep(step);
      if (step.block || step.retry !== undefined || step.captureTo !== undefined) {
        await p;
      } else {
        inFlight.push(p);
      }
    }
    if (options.drainFinalInFlight && inFlight.length > 0) {
      await Promise.allSettled(inFlight.splice(0));
    }
  });
}

async function runStep(step: import('@tealstreet/cli-core').PreprocessedStep): Promise<void> {
  // Finalize forward-reference `@<name>` capture tokens against the
  // store now that the capturing step's writeback has run. Capture vars
  // hold an order-ID list; a name that resolved to "" means the capture
  // produced ZERO IDs — fail the step closed instead of letting the
  // token vanish, so `cancel @o1` with an empty @o1 can't widen into
  // "cancel every order on the focused symbol" (TSBUG-103).
  let resolvedCommand = step.command;
  if (step.deferredSubstitution) {
    const { command, emptyCaptures } = resolveDeferredCaptures(step.command, step.deferredSubstitution, (name) =>
      preprocessStore.getVar(name),
    );
    if (emptyCaptures.length > 0) {
      reportStepError(
        step,
        new Error(
          `${emptyCaptures.map((n) => `@${n}`).join(', ')} captured 0 order IDs — skipping "${step.command}" rather than running it with an empty target list.`,
        ),
      );
      return;
    }
    resolvedCommand = command;
  }

  // A hosted webhook body is someone else's text; it may trade, not reconfigure the CLI.
  if (getAdapterContext()?.untrustedBody) {
    // A capture writes ~/.tealstreet/vars.json, which a later typed command reads.
    const refusal =
      webhookRefusal(resolvedCommand) ??
      (step.captureTo !== undefined ? `"@${step.captureTo} = …" captures cannot be used in a webhook.` : null);
    if (refusal) {
      reportStepError(step, new Error(refusal));
      return;
    }
  }

  // Phase 5 — `script:<name>` prefix dispatch. See SPEC §6.1.
  // Routes loaded scripts via `runScriptFromHost`, honoring the same
  // retry / capture / error-mode semantics as regular commands. The
  // arm lives in `runStep` (not `processSingleCommand`) so the retry
  // loop can preflight idempotency before user code runs.
  if (resolvedCommand.startsWith('script:')) {
    await runScriptStep(resolvedCommand, step);
    return;
  }

  const fire = () => processSingleCommand(resolvedCommand, step.captureTo);

  // Sleep — block the chain.
  if (step.block) {
    try {
      await fire();
    } catch (e) {
      reportStepError(step, e as Error);
    }
    return;
  }

  // `retry N <body>` — await + retry up to N times. The first attempt
  // is "try 0", so retry: 3 means up to 4 total attempts.
  if (step.retry !== undefined) {
    try {
      await runCommandWithRetries(fire, step.retry);
    } catch (error) {
      reportStepError(step, error as Error);
    }
    return;
  }

  // Capture — `@<var> = <body>` MUST be awaited so subsequent steps
  // can reference @<var>. processSingleCommand does the writeback
  // internally when its captureTo arg is set.
  if (step.captureTo !== undefined) {
    try {
      await fire();
    } catch (e) {
      reportStepError(step, e as Error);
    }
    return;
  }

  // Default: fire-and-don't-await. Errors print but don't block the
  // chain. Return the promise so the chain runner can track it in
  // `inFlight` and drain it at the next `;` barrier.
  return fire().catch((e) => reportStepError(step, e as Error));
}

function reportStepError(step: import('@tealstreet/cli-core').PreprocessedStep, err: Error): void {
  if (step.errorMode === 'continue') {
    adapter.printWarning(`(continue-on-error) ${err.message}`);
  } else if (!(err as Error & { alreadyReported?: boolean }).alreadyReported) {
    adapter.printError(`Error in command: ${err.message}`);
  }
}

/**
 * Phase 5 dispatcher for `script:<name>` steps. Sibling to the regular
 * command path in `runStep` — uses the same block/retry/capture/
 * errorMode semantics as cli-core commands, but routes through
 * `runScriptFromHost` instead of `cliExecuteCommand`.
 *
 * - Awaits the initial-load promise so cold REPL doesn't race the
 *   fire-and-forget `loadScripts()` in `startRepl()`.
 * - On retry, rejects non-idempotent scripts before the first attempt;
 *   the lower-level `runScriptFromHost` gate still protects direct
 *   host calls that pass retry context.
 * - On capture, passes `{ captureTo, preprocessStore }` so the §7.4
 *   writeback inside the runner sets the user var.
 */
async function runScriptStep(command: string, step: import('@tealstreet/cli-core').PreprocessedStep): Promise<void> {
  const parsed = parseScriptInvocation(command);
  if (!parsed) {
    reportStepError(
      step,
      new Error(
        `script: prefix must be followed immediately by a script name (no space). Example: \`script:myStrategy 1 BTC\`.`,
      ),
    );
    return;
  }

  const inflight = awaitScriptsLoaded();
  if (inflight) {
    try {
      await inflight;
    } catch {
      // Initial load failure already logged via logger.warn in
      // `doLoadScripts`. Continue — `lookupScript` will just return
      // undefined and the error below kicks in.
    }
  }

  const record = lookupScript(parsed.name);
  if (!record) {
    reportStepError(
      step,
      new Error(
        `script '${parsed.name}' not loaded — drop a file at ~/${BRAND.configDirName}/scripts/${parsed.name}.ts and run \`script reload\`.`,
      ),
    );
    return;
  }

  const transpile = getCliTranspile();
  // ALS context discriminator (set by `apps/cli/src/listener/handlers/cmd.ts`
  // and `hook.ts`): `'cmd-exec'` = web-tab inline command, `'webhook'`
  // = HTTP hook POST. No context = direct REPL input.
  const alsCtx = getAdapterContext();
  const invokedFrom: 'cli' | 'webhook' | 'inline' =
    alsCtx?.source === 'webhook' ? 'webhook' : alsCtx?.source === 'cmd-exec' ? 'inline' : 'cli';
  const capture = step.captureTo !== undefined ? { captureTo: step.captureTo, preprocessStore } : undefined;

  const runOnce = async (attempt: number) => {
    return runScriptFromHost(
      {
        exchangeManager,
        session,
        platformAdapter: adapter,
        transpile,
      },
      {
        record,
        rawArgs: parsed.rawArgs,
        invokedFrom,
        retry: attempt > 0 ? { isRetry: true, scriptName: record.name } : undefined,
        capture,
      },
    );
  };

  const surfaceFailure = (errorMsg: string) => {
    reportStepError(step, new Error(errorMsg));
  };

  // TSBUG-107: record the script INVOCATION in the audit trail (the
  // per-order internal writes are recorded by the script-adapter). The
  // script name lives here; individual orders are bracketed by time.
  const recordInvocation = (ok: boolean, error?: string) =>
    recordScriptWrite(
      // Attribute to the override-resolved account (listener/webhook runs
      // carry an accountOverride) — not the long-lived REPL session.
      alsCtx?.accountOverride ?? session.currentAccountName ?? 'unknown',
      `run ${parsed.name}${parsed.rawArgs ? ' ' + parsed.rawArgs : ''}`,
      { ok, ...(error ? { error } : {}) },
    );

  // Mirror the cli-core retry/block/capture/default arms below — the
  // script path needs its own loop because attempt index threads
  // through to `isRetry`.
  if (step.retry !== undefined) {
    if (record.capabilities.idempotent !== true) {
      surfaceFailure(`script '${record.name}' is not marked idempotent; cannot retry`);
      return;
    }
    let lastErr: string | undefined;
    for (let attempt = 0; attempt <= step.retry; attempt++) {
      const result = await runOnce(attempt);
      if (result.ok) {
        recordInvocation(true);
        return;
      }
      lastErr = result.error ?? 'unknown error';
    }
    if (lastErr) {
      recordInvocation(false, `(after ${step.retry} retries) ${lastErr}`);
      surfaceFailure(`(after ${step.retry} retries) ${lastErr}`);
    }
    return;
  }

  // Block / capture / default — all share the same shape now that
  // `runScriptFromHost` already awaits its IIFE. We always await the
  // script so its prints + capture writeback land in order.
  const result = await runOnce(0);
  recordInvocation(result.ok, result.ok ? undefined : result.error);
  if (!result.ok && result.error) {
    surfaceFailure(result.error);
  }
}

interface StartReplOptions {
  disableAll?: boolean;
}

export async function startRepl(accountName?: string, options: StartReplOptions = {}) {
  const { disableAll = false } = options;
  const config = loadConfig();

  // `when` arms a command to run later, so it needs a way to run one the way
  // the user typing it would. Assigned HERE rather than at module scope on
  // purpose: `runSingleCommand` shares this adapter and exits as soon as its
  // command returns, so a trigger armed there could never fire. `when` reads
  // the absence of this hook and refuses to arm instead of pretending.
  adapter.runChain = async (input: string, scope?: { symbol?: string; account?: string }): Promise<void> => {
    const run = () =>
      scope?.symbol || scope?.account
        ? // Same per-call override the listener's `cmd:exec` uses, and for the
          // same reason: run against a specific account/symbol WITHOUT mutating
          // the long-lived REPL session the user is still typing into. No
          // `printSink`, so output lands on the terminal as normal.
          runWithAdapterContext({ accountOverride: scope.account, symbolOverride: scope.symbol }, () =>
            processCommand(input),
          )
        : processCommand(input);

    // Onto the SAME queue typed input uses. A trigger fires from a timer, so
    // without this it would interleave with a chain the user is midway
    // through — two order-placing chains reading the same store at once.
    const queued = commandQueue.then(run);
    // The queue must survive a failing trigger; the caller still sees the
    // rejection so the trigger can record it.
    commandQueue = queued.then(
      () => undefined,
      () => undefined,
    );
    await queued;
  };

  // Pin the top status-line region BEFORE any banner/connect output so that
  // output flows into rows 2..N below it (no after-the-fact cursor fight).
  // No-op on non-TTY; torn down via process 'exit' (see status-line.ts).
  enableStatusLine();

  // Pin the bottom footer. On by default — `footer off` sets enabled:false to
  // opt out. Reserves the bottom two rows (divider + content) via the
  // scroll-region coordinator; no-op on non-TTY (pipes / CI).
  if (config.footer?.enabled !== false) {
    footerSegments = config.footer?.segments ? [...config.footer.segments] : [...DEFAULT_SEGMENTS];
    setFooterRenderer(footerRenderer);
    enableFooter(config.footer?.intervalMs);
    startFooterScriptRefresh(config.footer?.intervalMs);
  }

  // Register CLI-specific commands (record, export/import, etc.)
  registerRecordCommands();
  registerExportImportCommands();

  // Seed ~/.tealstreet/scripts/ (lazily — first run creates the dir +
  // the example/tsconfig/types files) and load the initial registry.
  // Per SPEC §5.1: `types.d.ts` is refreshed every launch to match the
  // running CLI's API surface, so the user's editor doesn't go stale
  // after an upgrade.
  //
  // Static import (rather than `await import(...)`) so the call
  // publishes the inflight promise SYNCHRONOUSLY before this function
  // yields — the dispatcher's `awaitScriptsLoaded()` then picks it up
  // immediately on cold-REPL `script:foo`.
  loadScripts().catch((err) => {
    logger.warn(`scripts: initial load failed — ${err instanceof Error ? err.message : String(err)}`);
  });

  // Check auth token on startup and warn if expiring soon
  if (isLoggedIn() && tokenNeedsRefresh()) {
    const daysLeft = getTokenExpiryDays();
    adapter.printWarning(`Your authentication token expires in ${daysLeft} days. Run "login" to refresh.`);
    markTokenExpiryWarned();
  }

  // Set symbol from config or default
  if (config.defaultSymbol) {
    session.currentSymbol = config.defaultSymbol;
  }

  // Skip all exchange connections if --disable-all flag is set
  if (disableAll) {
    // Persist the disabled state so it sticks across restarts
    for (const acc of config.accounts) {
      if (acc.active) {
        adapter.updateAccount(acc.name, { active: false });
      }
    }
    adapter.printWarning('Started with --disable-all flag. All accounts disabled and saved.');
    adapter.printInfo('Use "accounts enable <name>" to connect to an account manually.\n');
  } else {
    // Try to connect if accounts exist
    if (config.accounts.length > 0) {
      const account = getAccount(accountName);
      // Only auto-connect if the account is explicitly active (or was specified by name)
      if (account && (account.active || accountName)) {
        // Set current account name immediately so prompt shows it
        session.currentAccountName = account.name;
        adapter.printInfo(`Connecting to ${account.exchange} in background...`);
      } else if (accountName) {
        adapter.printError(`Account "${accountName}" not found`);
        adapter.printInfo('Available accounts:');
        config.accounts.forEach((a) => {
          adapter.printInfo(`  - ${a.name} (${a.exchange})`);
        });
      }
    } else {
      adapter.printWarning('\nNo accounts configured.');
      adapter.printInfo('Use "account add" to add an account, or "account exchanges" to see supported exchanges.\n');
    }

    // Start all accounts (including primary) in the background without blocking the CLI prompt
    const activeAccounts = adapter.getActiveAccounts();
    // Build list of accounts to enable - include primary account even if not in active list
    const primaryAccount = session.currentAccountName ? getAccount(session.currentAccountName) : null;
    const accountsToEnable: ExchangeAccountConfig[] = [];

    // Add primary account first if it exists and is active
    if (primaryAccount && primaryAccount.active && !exchangeManager.isConnected(primaryAccount.name)) {
      accountsToEnable.push(primaryAccount);
    }

    // Add other active accounts (skip primary since it's already added)
    for (const acc of activeAccounts) {
      if (acc.name === session.currentAccountName) continue;
      if (exchangeManager.isConnected(acc.name)) continue;
      accountsToEnable.push(acc);
    }

    if (accountsToEnable.length > 0) {
      // Fire off all enables in parallel, don't block
      for (const activeAccount of accountsToEnable) {
        connectionStates.set(activeAccount.name, 'connecting');
        exchangeManager
          .enable(activeAccount)
          .then((exchange) => {
            connectionStates.set(activeAccount.name, 'connected');
            setupExchangeListeners(exchange, activeAccount.name);
            // If this is the primary account, update session
            if (activeAccount.name === session.currentAccountName) {
              session.currentExchange = exchange;
              adapter.printSuccess(`Connected to ${activeAccount.exchange}`);
              adapter.printInfo(`Symbol: ${session.currentSymbol} | Account: ${session.currentAccountName}`);
            } else {
              adapter.printSuccess(`Account "${activeAccount.name}" enabled`);
            }
            // Refresh prompt to show updated status
            if (rlInstance && !isMonitorMode) {
              rlInstance.setPrompt(getPrompt());
              process.stdout.write(CLEAR_LINE);
              rlInstance.prompt(true);
            }
          })
          .catch((error) => {
            const errorMsg = (error as Error).message;
            const kind = classifyConnectError(errorMsg);
            connectionStates.set(activeAccount.name, kind === 'auth' ? 'auth-failed' : 'failed');
            if (activeAccount.name === session.currentAccountName) {
              // Primary account failed - show detailed error
              if (kind === 'auth') {
                adapter.printError('Authentication failed. Your token may have expired.');
                adapter.printInfo('Run "login" to re-authenticate.');
              } else if (kind === 'timeout') {
                adapter.printError(errorMsg);
                adapter.printInfo('Check your network connection and try again.');
              } else {
                adapter.printError(`Connection failed: ${errorMsg}`);
              }
            } else {
              adapter.printWarning(`Failed to enable "${activeAccount.name}": ${errorMsg}`);
            }
            // Refresh the prompt so it reflects the failed state instead of
            // continuing to render the account as if it were live.
            if (rlInstance && !isMonitorMode) {
              rlInstance.setPrompt(getPrompt());
              process.stdout.write(CLEAR_LINE);
              rlInstance.prompt(true);
            }
          });
      }
    }
  }

  adapter.printInfo('Type "help" for available commands, "exit" to quit');
  adapter.printInfo(`Full logs → yarn logs (or tail -f ~/${BRAND.configDirName}/cli.log)\n`);

  // Load command history
  const history = loadHistory();

  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
    prompt: getPrompt(),
    history,
    historySize: MAX_HISTORY_SIZE,
  });

  // Store reference for monitor mode
  rlInstance = rl;
  hookFooterRepaint(rl);

  // Route async terminal output (console.log/warn/error) above the prompt so it
  // doesn't shred in-progress input. The getter returns the *current*
  // rlInstance, so watch-mode readline recreation needs no re-registration; the
  // monitor-mode gate skips the redraw while a raw-mode watch screen owns the
  // terminal.
  setPromptController({ getReadline: () => rlInstance, isMonitorMode: () => isMonitorMode });

  // Setup connection monitor for background health checks and real-time prompt updates
  if (session.currentExchange) {
    connectionMonitor = new ConnectionMonitor(session.currentExchange, {
      onWarning: (msg) => {
        if (isMonitorMode || !rlInstance) return;
        process.stdout.write(CLEAR_LINE);
        console.log(chalk.yellow(`⚠️  ${msg}`));
        rlInstance.prompt(true);
      },
      onError: (msg) => {
        if (isMonitorMode || !rlInstance) return;
        process.stdout.write(CLEAR_LINE);
        console.log(chalk.red(`❌ ${msg}`));
        rlInstance.prompt(true);
      },
      onRecovered: (msg) => {
        if (isMonitorMode || !rlInstance) return;
        process.stdout.write(CLEAR_LINE);
        console.log(chalk.green(`✅ ${msg}`));
        rlInstance.prompt(true);
      },
      onTick: () => {
        // Skip prompt updates during monitor mode or cooldown
        if (isMonitorMode) return;
        if (Date.now() - monitorExitTime < MONITOR_EXIT_COOLDOWN_MS) return;
        if (!rlInstance) return;

        // Update prompt with latest status (price, position, connection)
        const newPrompt = getPrompt();
        if (rlInstance.getPrompt() !== newPrompt) {
          rlInstance.setPrompt(newPrompt);
          process.stdout.write(CLEAR_LINE);
          rlInstance.prompt(true);
        }
      },
    });
    connectionMonitor.start(2000); // Check every 2 seconds for real-time updates
  }

  // Setup network monitor to detect online/offline status and notify exchange
  networkMonitor = new NetworkMonitor(session.currentExchange, {
    onOffline: (msg) => {
      if (isMonitorMode || !rlInstance) return;
      process.stdout.write(CLEAR_LINE);
      console.log(chalk.yellow(`🌐 ${msg}`));
      rlInstance.prompt(true);
    },
    onOnline: (msg) => {
      if (isMonitorMode || !rlInstance) return;
      process.stdout.write(CLEAR_LINE);
      console.log(chalk.green(`🌐 ${msg}`));
      rlInstance.prompt(true);
    },
  });
  networkMonitor.start(5000); // Check every 5 seconds

  rl.prompt();

  await startListener({ adapter, processCommand });

  startAliasSync(preprocessStore, (message) => adapter.printWarning(message));

  // Hosted webhooks (docs/plans/WEBHOOK_RELAY_v1.md). Each delivered body goes
  // onto the SAME queue typed input uses, so an alert never interleaves with a
  // chain the user is midway through.
  startWebhookRelay({
    runBody: (body, ctx) => {
      // Drain the chain's last fire-and-forget step, or a one-step `buy` would
      // ack `done` before the order is even placed.
      // An alias in the body may have been defined on another listener moments ago.
      const queued = commandQueue.then(async () => {
        // It waited behind typed input; an alert past its expiry is stale.
        if (ctx.expiresAt !== undefined && Date.now() > ctx.expiresAt) {
          throw new Error('Not run: it waited behind your own commands past its expiry.');
        }
        await syncAliasesIfStale();
        return runWithAdapterContext({ ...ctx, untrustedBody: true }, () =>
          processCommand(body, { drainFinalInFlight: true }),
        );
      });
      commandQueue = queued.then(
        () => undefined,
        () => undefined,
      );
      return queued;
    },
    notice: {
      info: (message) => adapter.printInfo(message),
      warn: (message) => adapter.printWarning(message),
    },
  });

  rl.on('line', (line) => {
    const command = line.trim();

    if (!command) {
      rl.prompt();
      return;
    }

    // Save command to history file (append immediately for persistence)
    appendToHistory(command);

    // Chain commands to ensure sequential processing
    commandQueue = commandQueue.then(async () => {
      if (command === 'exit' || command === 'quit') {
        adapter.printInfo('Goodbye!');
        if (connectionMonitor) {
          connectionMonitor.stop();
          connectionMonitor = null;
        }
        if (networkMonitor) {
          networkMonitor.stop();
          networkMonitor = null;
        }
        stopListener();
        stopWebhookRelay();
        stopAliasSync();
        // Stop all exchanges (primary and enabled)
        await exchangeManager.stopAll();
        process.exit(0);
      }

      // Process command
      await processCommand(command);

      // Update prompt with latest status (symbol, price, position, connection)
      // But NOT if we're in monitor/serve mode - those handle their own display
      if (!isMonitorMode) {
        rl.setPrompt(getPrompt());
        rl.prompt();
      }
    });
    // A command that throws must not leave the queue rejected: every later
    // typed command and webhook would then fail without running.
    commandQueue = commandQueue.catch((err) => adapter.printError(`Error: ${(err as Error).message}`));
  });

  rl.on('close', async () => {
    // Skip if we're recreating readline (not a real close)
    if (isRecreatingReadline) return;

    // Stop connection monitor
    if (connectionMonitor) {
      connectionMonitor.stop();
      connectionMonitor = null;
    }

    // Stop network monitor
    if (networkMonitor) {
      networkMonitor.stop();
      networkMonitor = null;
    }

    stopListener();
    stopWebhookRelay();
    stopAliasSync();

    // Stop all exchanges (primary and enabled)
    await exchangeManager.stopAll();
    process.exit(0);
  });
}

// Execute a single command and exit
/**
 * Execute a command and capture its output as a string.
 * Used by serve mode to relay command results back to web app.
 * Uses the exact same command processing path as local CLI.
 */
export async function executeCommandWithCapture(command: string): Promise<string> {
  // Ensure CLI-specific commands are registered (idempotent)
  registerRecordCommands();
  registerExportImportCommands();

  // Start capturing adapter output
  const stopCapture = adapter.startCapture();

  // Also capture direct console.log calls (some commands use console.log directly)
  const capturedConsole: string[] = [];
  const originalLog = console.log;
  console.log = (...args: unknown[]) => {
    capturedConsole.push(args.map((a) => String(a)).join(' '));
  };

  try {
    await processSingleCommand(command);
  } catch (error) {
    adapter.printError(`Error: ${(error as Error).message}`);
  } finally {
    // Restore console.log
    console.log = originalLog;
  }

  // Stop capturing and get adapter output
  const adapterOutput = stopCapture();

  // Combine both outputs
  const allOutput = [...capturedConsole];
  if (adapterOutput) {
    allOutput.push(adapterOutput);
  }

  return allOutput.join('\n').trim();
}

/**
 * Get current session info for serve mode
 */
export function getSessionInfo(): {
  currentSymbol: string;
  currentAccountName?: string;
  connected: boolean;
} {
  return {
    currentSymbol: session.currentSymbol,
    currentAccountName: session.currentAccountName,
    connected: !!session.currentExchange,
  };
}

export async function runSingleCommand(
  commandString: string,
  accountName?: string,
  symbol?: string,
  options: { json?: boolean } = {},
): Promise<void> {
  if (options.json) return runSingleCommandJson(commandString, accountName, symbol);
  const config = loadConfig();

  // Check auth token on startup and warn if expiring soon
  if (isLoggedIn() && tokenNeedsRefresh()) {
    const daysLeft = getTokenExpiryDays();
    adapter.printWarning(`Your authentication token expires in ${daysLeft} days. Run "login" to refresh.`);
    markTokenExpiryWarned();
  }

  if (config.accounts.length === 0) {
    adapter.printWarning('No accounts configured.');
    adapter.printInfo(`Run: ${BRAND.binName} account add --help`);
    process.exit(1);
  }

  const account = getAccount(accountName);
  if (!account) {
    adapter.printError(`Account "${accountName || 'default'}" not found`);
    process.exit(1);
  }

  session.currentSymbol = symbol || config.defaultSymbol || 'BTCUSDT';

  try {
    session = await connectToAccount(account, session, adapter);
  } catch (error) {
    const errorMsg = (error as Error).message;
    const kind = classifyConnectError(errorMsg);
    if (kind === 'auth') {
      adapter.printError('Authentication failed. Your token may have expired.');
      adapter.printInfo('Run "login" to re-authenticate.');
    } else if (kind === 'timeout') {
      adapter.printError(errorMsg);
      adapter.printInfo('Check your network connection and try again.');
    } else {
      adapter.printError(`Connection failed: ${errorMsg}`);
    }
    process.exit(1);
  }

  if (!session.currentExchange) {
    process.exit(1);
  }

  // Setup event listeners
  setupExchangeListeners(session.currentExchange, session.currentAccountName);

  // A rejected key is terminal for anything that needs the venue — running it
  // anyway is what printed a zeroed balance table that read as an empty
  // account. Local commands (alias, simulation, …) still work with a bad key.
  ensureCommandsRegistered(); // registration is lazy; the lookup below is empty without it
  const requestedCommand = commandRegistry.find(commandString.trim().split(/\s+/)[0]?.toLowerCase() ?? '');
  if ((session.currentExchange.authFailureStreak ?? 0) > 0 && requestedCommand?.requiresConnection !== false) {
    // `connectToAccount` already said the key was rejected; add the actionable half.
    adapter.printInfo('Check the key and secret, and that they were created on the same environment as the account.');
    await teardownOneShot(session.currentExchange, 1);
  }

  const twapsBeforeCommand = new Set(twapManager.runningTwaps.keys());
  await processCommand(commandString, { drainFinalInFlight: true });
  await waitForTwapsStartedByCommand(twapsBeforeCommand);
  await teardownOneShot(session.currentExchange);
}

/**
 * `runSingleCommand` for agents: the same steps, but every outcome is one JSON
 * document with a stable `code`, and every line the run prints is captured into
 * it instead of reaching stdout. Keep the steps in step with the human path above.
 */
async function runSingleCommandJson(commandString: string, accountName?: string, symbol?: string): Promise<void> {
  const output: OneShotLine[] = [];
  let resolvedSymbol: string | undefined = symbol;
  const result = (ok: boolean, code: OneShotCode, error?: string) => ({
    ok,
    code,
    command: commandString.trim(),
    account: session.currentAccountName ?? accountName,
    ...(resolvedSymbol ? { symbol: resolvedSymbol } : {}),
    ...(error ? { error } : {}),
    output,
  });
  const fail = (code: OneShotCode, error: string) => emitJsonAndExit(result(false, code, error), 1);
  // Exchange code reports through console, not the adapter: a venue that rejects
  // an order logs it and returns no order, so these lines are part of the result.
  const exchangeLines: OneShotLine[] = [];
  setConsoleTap((kind, message) => {
    if (kind !== 'info' || /warning|error/i.test(message)) exchangeLines.push({ kind, text: stripAnsi(message) });
  });

  try {
    await runWithAdapterContext(
      { printSink: (kind, text) => output.push({ kind, text: stripAnsi(text) }) },
      async () => {
        const config = loadConfig();
        if (isLoggedIn() && tokenNeedsRefresh()) {
          adapter.printWarning(
            `Your authentication token expires in ${getTokenExpiryDays()} days. Run "login" to refresh.`,
          );
        }
        if (!config.accounts?.length) {
          return fail(
            'NO_ACCOUNTS',
            `No accounts configured. Add one with \`${BRAND.binName} account add\`, or sync from the web app.`,
          );
        }
        const account = getAccount(accountName);
        if (!account) return fail('ACCOUNT_NOT_FOUND', `Account "${accountName || 'default'}" not found`);

        resolvedSymbol = session.currentSymbol = symbol || config.defaultSymbol || 'BTCUSDT';
        try {
          session = await connectToAccount(account, session, adapter);
        } catch (error) {
          adapter.printError(`Failed to connect: ${(error as Error).message}`);
        }
        if (!session.currentExchange) {
          const errorLine = [...output].reverse().find((line) => line.kind === 'error')?.text;
          return fail(connectFailureCode(errorLine, getAuthState()), errorLine ?? 'Could not connect to the account.');
        }

        setupExchangeListeners(session.currentExchange, session.currentAccountName);
        ensureCommandsRegistered();
        const requestedCommand = commandRegistry.find(commandString.trim().split(/\s+/)[0]?.toLowerCase() ?? '');
        const needsKey = requestedCommand?.requiresConnection !== false;
        const rejection = findKeyRejection(exchangeLines.map((line) => line.text));
        if (((session.currentExchange.authFailureStreak ?? 0) > 0 || rejection) && needsKey) {
          await emitJson(result(false, 'KEY_REJECTED', rejection ?? 'The exchange rejected this API key.'));
          return teardownOneShot(session.currentExchange, 1);
        }

        let threw: unknown;
        // Connect-time lines (a key warning a local command does not need) are not the command's result.
        const commandStart = output.length;
        const exchangeStart = exchangeLines.length;
        const twapsBeforeCommand = new Set(twapManager.runningTwaps.keys());
        try {
          await processCommand(commandString, { drainFinalInFlight: true });
          await waitForTwapsStartedByCommand(twapsBeforeCommand);
        } catch (error) {
          threw = error;
        }
        const exchangeDuring = exchangeLines.slice(exchangeStart);
        const lateRejection = needsKey ? findKeyRejection(exchangeDuring.map((line) => line.text)) : undefined;
        const summary = lateRejection
          ? { ok: false, code: 'KEY_REJECTED' as const, error: lateRejection }
          : summarizeOneShot(output.slice(commandStart), threw, exchangeDuring);
        output.push(...exchangeDuring);
        await emitJson(result(summary.ok, summary.code, summary.error));
        await teardownOneShot(session.currentExchange, summary.ok ? 0 : 1);
      },
    );
  } catch (error) {
    await emitJsonAndExit(result(false, 'INTERNAL_ERROR', error instanceof Error ? error.message : String(error)), 1);
  }
}

/**
 * One-shot teardown. `stopExchange` alone leaves the process alive: the
 * virtual clock re-arms a 5-minute sync forever and the HoW tunnel sockets hold
 * a ping timer, so `tealstreet balance` ran until the user pressed Ctrl-C. The
 * REPL never hit this because every one of its exits calls `process.exit`.
 */
async function teardownOneShot(exchange: NonNullable<typeof session.currentExchange>, exitCode = 0): Promise<void> {
  try {
    await adapter.stopExchange(exchange);
  } finally {
    virtualClock.stop();
    disposeHOWWebSockets();
    await flushCliSentry();
  }
  process.exit(exitCode);
}

async function waitForTwapsStartedByCommand(twapsBeforeCommand: Set<string>): Promise<void> {
  const startedTwapIds = Array.from(twapManager.runningTwaps.keys()).filter((id) => !twapsBeforeCommand.has(id));
  if (startedTwapIds.length === 0) return;

  await new Promise<void>((resolve) => {
    const interval = setInterval(() => {
      const stillRunning = startedTwapIds.some((id) => twapManager.getById(id));
      if (!stillRunning) {
        clearInterval(interval);
        resolve();
      }
    }, 250);
  });
}
