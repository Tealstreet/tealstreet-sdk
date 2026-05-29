import chalk from 'chalk';
import type { CLIPlatformAdapter, ExchangeLike, ExchangeAccountConfig } from '@tealstreet/cli-core';
import {
  getAccount as getAccountFromConfig,
  addAccount as addAccountToConfig,
  updateAccount as updateAccountInConfig,
  removeAccount as removeAccountFromConfig,
  setDefaultAccount as setDefaultAccountInConfig,
  setDefaultSymbol as setDefaultSymbolInConfig,
  listAccounts as listAccountsFromConfig,
  getActiveAccounts as getActiveAccountsFromConfig,
  loadConfig,
} from './config';
import { isLoggedIn, checkAndRefreshToken, getProxyTokenFromConfig, getTealstreetApiBaseUrl } from './auth';
// Import original console functions to bypass rate limiting for CLI command output
import { originalConsoleLog, originalConsoleError, originalConsoleWarn, writeAbovePrompt } from './logger';
import { classifyLog } from './log-classify';
import { setStatus } from './status-line';
import type { ExchangeManager } from './exchange-manager';
import { getAdapterContext } from './listener/adapter-context';
// Static import for token management - must match what safe-cex uses internally
import { setProxyToken, setTokenRefreshFunction } from '@tealstreet/safe-cex/src/accessToken';
import { getExchangeMetadata } from '@tealstreet/safe-cex/src/exchangeMetadata';
import {
  createTunnelUrlTicketProvider,
  getProxyTicketsUrl,
} from '@tealstreet/safe-cex/src/httpOverWebsocket/tunnelUrlTicket';
import { initializeHOW } from '@tealstreet/safe-cex/src/httpOverWebsocket/configureHOW';
import type { ExchangeName } from '@tealstreet/safe-cex/src/types';
import { configureHOW } from '@tealstreet/http-over-websocket/src/config';

// Default timeout for exchange operations (30 seconds)
const EXCHANGE_OPERATION_TIMEOUT = 30000;

/**
 * Extended adapter interface that supports setting the exchange manager
 */
export interface StandalonePlatformAdapter extends CLIPlatformAdapter {
  /** Set the exchange manager reference (called after manager is created) */
  setExchangeManager(manager: ExchangeManager): void;

  /**
   * Plumb a getter so the adapter can ask repl.ts for the live readline
   * + monitor-mode state at prompt-time. Both can change while the
   * adapter is alive (readline is recreated after watch-mode raw-mode
   * takedown), so we resolve via callback rather than caching.
   */
  setPromptReadline(getReadline: () => import('readline').Interface | null, isInMonitorMode: () => boolean): void;

  /**
   * Start capturing output instead of printing to console
   * Returns a function to stop capturing and get the captured output
   */
  startCapture(): () => string;
}

// Dynamic import to avoid circular dependency issues
const getCreateExchangeFromAccount = async () => {
  const { createExchangeFromAccount } = await import('@tealstreet/safe-cex/src/createExchangeFromAccount');
  return createExchangeFromAccount;
};

/**
 * Standalone CLI platform adapter
 * Implements CLIPlatformAdapter interface for the standalone CLI
 */
export function createStandalonePlatformAdapter(): StandalonePlatformAdapter {
  let exchangeManager: ExchangeManager | null = null;
  let captureBuffer: string[] | null = null;
  let getReadlineRef: (() => import('readline').Interface | null) | null = null;
  let isInMonitorRef: (() => boolean) | null = null;

  // Wire the proxy-token refresh function once, regardless of account flags.
  // Many exchanges (BitMEX, Bitget, etc.) force the HOW tunnel via metadata
  // even when the user hasn't set tealstreetTunnel on their account, and the
  // HOW adapter polls getProxyToken() globally. Setting this once here means
  // HOW always sees the freshest token from config; login() also primes the
  // in-memory cache so any in-flight retries pick up the new token immediately.
  initializeHOW();
  setTokenRefreshFunction(async () => getProxyTokenFromConfig() || '');
  configureHOW({
    getTunnelUrlTicket: createTunnelUrlTicketProvider({
      endpoint: getProxyTicketsUrl(getTealstreetApiBaseUrl()),
      getAuthToken: getProxyTokenFromConfig,
    }),
  });
  const initialToken = getProxyTokenFromConfig();
  if (initialToken) {
    setProxyToken(initialToken);
  }

  // Helper to either capture or print. Terminal prints go through
  // writeAbovePrompt so async status lines (connect/listener/errors) don't
  // shred the user's in-progress input — same readline-aware path as the
  // console patches. The capture branch (cmd:exec / `export`) is untouched.
  const output = (text: string) => {
    if (captureBuffer !== null) {
      captureBuffer.push(text);
    } else {
      writeAbovePrompt(() => originalConsoleLog(text));
    }
  };

  const outputError = (text: string) => {
    if (captureBuffer !== null) {
      captureBuffer.push(text);
    } else {
      writeAbovePrompt(() => originalConsoleError(text));
    }
  };

  const outputWarn = (text: string) => {
    if (captureBuffer !== null) {
      captureBuffer.push(text);
    } else {
      writeAbovePrompt(() => originalConsoleWarn(text));
    }
  };

  return {
    // Set exchange manager reference (called after manager is created)
    setExchangeManager(manager: ExchangeManager): void {
      exchangeManager = manager;
    },

    setPromptReadline(getReadline: () => import('readline').Interface | null, isInMonitorMode: () => boolean): void {
      getReadlineRef = getReadline;
      isInMonitorRef = isInMonitorMode;
    },

    async promptYesNo(message: string, options?: { timeoutMs?: number }): Promise<boolean> {
      // Watch-mode collision: if the REPL is in alt-screen / raw mode,
      // stdin is consumed by keypress handlers. Auto-reject so the caller
      // doesn't hang waiting on a paused readline.
      if (isInMonitorRef?.()) return false;
      const rl = getReadlineRef?.();
      if (!rl) return false;
      const timeoutMs = options?.timeoutMs ?? 30000;
      return new Promise<boolean>((resolve) => {
        let settled = false;
        const finish = (value: boolean) => {
          if (settled) return;
          settled = true;
          clearTimeout(timer);
          resolve(value);
        };
        const timer = setTimeout(() => {
          // Settle the caller first, then cancel the pending rl.question
          // so the next REPL line the user types doesn't get absorbed by
          // the (now-discarded) question callback. node:readline doesn't
          // have a native cancel; writing a newline to stdin resolves the
          // pending question with an empty answer, which our callback
          // below treats as a no-op since `settled` is already true.
          finish(false);
          try {
            process.stdin.write('\n');
          } catch {
            /* stdin may have closed; nothing useful to do */
          }
        }, timeoutMs);
        rl.question(message, (answer) => {
          const a = (answer || '').trim().toLowerCase();
          finish(a === 'y' || a === 'yes');
        });
      });
    },

    // Account management
    listAccounts(): ExchangeAccountConfig[] {
      return listAccountsFromConfig();
    },

    getAccount(name?: string): ExchangeAccountConfig | undefined {
      return getAccountFromConfig(name);
    },

    addAccount(account: ExchangeAccountConfig): void {
      addAccountToConfig(account);
    },

    updateAccount(name: string, updates: Partial<ExchangeAccountConfig>): boolean {
      return updateAccountInConfig(name, updates);
    },

    removeAccount(name: string): boolean {
      return removeAccountFromConfig(name);
    },

    setDefaultAccount(name: string): boolean {
      return setDefaultAccountInConfig(name);
    },

    getDefaultSymbol(): string | undefined {
      const config = loadConfig();
      return config.defaultSymbol;
    },

    setDefaultSymbol(symbol: string): void {
      setDefaultSymbolInConfig(symbol);
    },

    getActiveAccounts(): ExchangeAccountConfig[] {
      return getActiveAccountsFromConfig();
    },

    // Exchange management
    async createExchange(account: ExchangeAccountConfig): Promise<ExchangeLike> {
      // Mirror safe-cex's accountToExchangeOptions: tunnel/proxy may be forced
      // on by metadata (enforceWsTunnel / requiresCors) even when the user
      // didn't explicitly enable it on the account.
      const metadata = getExchangeMetadata(account.exchange as ExchangeName);
      const requiresAuth =
        account.tealstreetTunnel || account.tealstreetProxy || metadata.requiresCors || metadata.enforceWsTunnel;

      if (requiresAuth) {
        if (!isLoggedIn()) {
          throw new Error('Tealstreet proxy/tunnel requires authentication. Run "login" first.');
        }
        await checkAndRefreshToken();

        const proxyToken = getProxyTokenFromConfig();
        if (!proxyToken) {
          throw new Error('No proxy token found. Please run "login" to authenticate.');
        }
        // Refresh the in-memory token in case it changed since startup.
        setProxyToken(proxyToken);
      }

      // Use the shared createExchangeFromAccount which handles all account fields
      // including tealstreetProxy, tealstreetTunnel, proxyRegion, etc.
      const createExchangeFromAccount = await getCreateExchangeFromAccount();
      const exchange = await createExchangeFromAccount(account);
      return exchange as ExchangeLike;
    },

    async startExchange(exchange: ExchangeLike): Promise<void> {
      if (exchange.start) {
        // Add timeout to prevent hanging on network issues
        const startPromise = exchange.start();
        const timeoutPromise = new Promise<never>((_, reject) => {
          setTimeout(() => {
            reject(new Error(`Exchange connection timed out after ${EXCHANGE_OPERATION_TIMEOUT / 1000} seconds`));
          }, EXCHANGE_OPERATION_TIMEOUT);
        });

        await Promise.race([startPromise, timeoutPromise]);
      }
    },

    async stopExchange(exchange: ExchangeLike): Promise<void> {
      if (exchange.dispose) {
        exchange.dispose();
      }
    },

    // Get exchange by account name
    getExchange(accountName: string): ExchangeLike | undefined {
      return exchangeManager?.getExchange(accountName);
    },

    // Check if account is connected
    isAccountConnected(accountName: string): boolean {
      return exchangeManager?.isConnected(accountName) ?? false;
    },

    // Output
    // Use original console functions to bypass rate limiting for CLI command output
    // When capturing, output goes to buffer instead.
    // When an AsyncLocalStorage adapter context is active (set by the loopback
    // listener's cmd:exec handler), print* methods route into the context's
    // sink instead so output streams as WS frames.
    print(lines: string[]): void {
      const ctx = getAdapterContext();
      if (ctx) {
        for (const line of lines) ctx.printSink('print', line);
        return;
      }
      lines.forEach((line) => output(line));
    },

    printError(message: string): void {
      const ctx = getAdapterContext();
      if (ctx) return ctx.printSink('error', message);
      outputError(chalk.red(message));
      // Mirror to the top status line (raw, uncolored message). No-op when the
      // status line is disabled/suspended/non-TTY.
      setStatus(message, classifyLog(message, 'error'));
    },

    printSuccess(message: string): void {
      const ctx = getAdapterContext();
      if (ctx) return ctx.printSink('success', message);
      output(chalk.green(message));
    },

    printWarning(message: string): void {
      const ctx = getAdapterContext();
      if (ctx) return ctx.printSink('warn', message);
      outputWarn(chalk.yellow(message));
      setStatus(message, classifyLog(message, 'warn'));
    },

    printInfo(message: string): void {
      const ctx = getAdapterContext();
      if (ctx) return ctx.printSink('info', message);
      output(chalk.gray(message));
    },

    /**
     * Start capturing output - returns function to stop and get captured output
     */
    startCapture(): () => string {
      captureBuffer = [];
      return () => {
        const result = (captureBuffer || []).join('\n');
        captureBuffer = null;
        return result;
      };
    },
  };
}
