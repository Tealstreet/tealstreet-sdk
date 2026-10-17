import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import * as readline from 'readline';
import { stableHash } from '@tealstreet/utils/src/stableHash';

import { classifyLog, isOperational } from './log-classify';
import { setStatus } from './status-line';
import { BRAND } from './brand';

const LOG_DIR = path.join(os.homedir(), BRAND.configDirName);
const LOG_FILE = path.join(LOG_DIR, 'cli.log');

// Ensure log directory exists
if (!fs.existsSync(LOG_DIR)) {
  fs.mkdirSync(LOG_DIR, { recursive: true });
}

// Clear log file on startup
fs.writeFileSync(LOG_FILE, '');

// Write startup marker
const startTime = new Date().toISOString();
fs.appendFileSync(LOG_FILE, `\n${'='.repeat(60)}\n`);
fs.appendFileSync(LOG_FILE, `[${startTime}] CLI session started\n`);
fs.appendFileSync(LOG_FILE, `${'='.repeat(60)}\n\n`);

export type LogLevel = 'debug' | 'info' | 'warn' | 'error';

// Log level priority (higher = more severe)
const LOG_LEVEL_PRIORITY: Record<LogLevel, number> = {
  debug: 0,
  info: 1,
  warn: 2,
  error: 3,
};

// Minimum log level for file output (controlled by DEBUG env var)
// Default is 'info' - set DEBUG=1 or DEBUG=true to include debug logs
const MIN_LOG_LEVEL: LogLevel = process.env.DEBUG === '1' || process.env.DEBUG === 'true' ? 'debug' : 'info';

function shouldLog(level: LogLevel): boolean {
  return LOG_LEVEL_PRIORITY[level] >= LOG_LEVEL_PRIORITY[MIN_LOG_LEVEL];
}

// =============================================================================
// Rate Limiting & Deduplication
// =============================================================================

// Rate limiting: max messages per second to terminal
const TERMINAL_RATE_LIMIT = 5; // max 5 messages per second
const RATE_LIMIT_WINDOW_MS = 1000;
let terminalMessageCount = 0;
let terminalWindowStart = Date.now();
let rateLimitWarningShown = false;

// Deduplication: track recent message hashes (10 second window like web app)
const DEDUP_WINDOW_MS = 10_000;
const recentMessageHashes: Map<string, number> = new Map();

// Eviction rides on isDuplicateMessage below rather than a timer: src/index.ts
// imports this module on every invocation, so any module-level timer held the
// event loop open and left every subcommand hanging until Ctrl+C.
let lastSweepAt = 0;

/**
 * Check if a message is a duplicate (seen within DEDUP_WINDOW_MS)
 */
function isDuplicateMessage(message: string): boolean {
  const hash = stableHash(message);
  const now = Date.now();

  if (now - lastSweepAt >= DEDUP_WINDOW_MS) {
    lastSweepAt = now;
    for (const [seenHash, timestamp] of recentMessageHashes) {
      if (now - timestamp > DEDUP_WINDOW_MS) recentMessageHashes.delete(seenHash);
    }
  }

  const lastSeen = recentMessageHashes.get(hash);

  if (lastSeen && now - lastSeen < DEDUP_WINDOW_MS) {
    return true;
  }

  recentMessageHashes.set(hash, now);
  return false;
}

/**
 * Check if we should rate limit terminal output
 * Returns true if the message should be suppressed
 */
function shouldRateLimit(): boolean {
  const now = Date.now();

  // Reset window if it's expired
  if (now - terminalWindowStart >= RATE_LIMIT_WINDOW_MS) {
    terminalWindowStart = now;
    terminalMessageCount = 0;
    rateLimitWarningShown = false;
  }

  terminalMessageCount++;

  if (terminalMessageCount > TERMINAL_RATE_LIMIT) {
    // Show warning once per window
    if (!rateLimitWarningShown) {
      rateLimitWarningShown = true;
      // Use original console to avoid recursion
      writeAbovePrompt(() =>
        originalConsoleWarn('\x1b[33m⚠\x1b[0m', 'Some output hidden (too many messages). Check log file for details.'),
      );
    }
    return true;
  }

  return false;
}

function formatTimestamp(): string {
  return new Date().toISOString();
}

/**
 * Format a stack trace with nice indentation for the log file
 */
function formatStackTrace(stack: string): string {
  if (!stack) return '';

  // Indent each line of the stack trace
  const lines = stack.split('\n');
  return lines.map((line) => `    ${line}`).join('\n');
}

function writeLog(level: LogLevel, message: string, data?: unknown): void {
  // Skip if below minimum log level
  if (!shouldLog(level)) {
    return;
  }

  const timestamp = formatTimestamp();
  const levelStr = level.toUpperCase().padEnd(5);
  const prefix = `[${timestamp}] [${levelStr}]`;

  let logLine = `${prefix} ${message}`;

  if (data !== undefined) {
    // Check if data is a stack trace string
    if (typeof data === 'string' && data.includes('\n') && data.includes('    at ')) {
      // It's a stack trace - format nicely
      logLine += '\n' + formatStackTrace(data);
    } else {
      try {
        const formatted = JSON.stringify(data, null, 2);
        // If it's multi-line JSON, put on new line with indent
        if (formatted.includes('\n')) {
          logLine +=
            '\n' +
            formatted
              .split('\n')
              .map((l) => `    ${l}`)
              .join('\n');
        } else {
          logLine += ` ${formatted}`;
        }
      } catch {
        logLine += ` [unserializable data]`;
      }
    }
  }

  // Use appendFileSync for immediate flush (so tail -f works)
  fs.appendFileSync(LOG_FILE, logLine + '\n');
}

export const logger = {
  debug(message: string, data?: unknown): void {
    writeLog('debug', message, data);
  },

  info(message: string, data?: unknown): void {
    writeLog('info', message, data);
  },

  warn(message: string, data?: unknown): void {
    writeLog('warn', message, data);
  },

  error(message: string, data?: unknown): void {
    writeLog('error', message, data);
  },

  /** Get the log file path for tailing */
  getLogPath(): string {
    return LOG_FILE;
  },

  /** Clear the log file */
  clear(): void {
    fs.truncateSync(LOG_FILE, 0);
  },
};

// Intercept console methods to also log to file
// Export originals so platform-adapter can use them directly (bypassing rate limiting)
export const originalConsoleLog = console.log;
export const originalConsoleError = console.error;

// Under `--json`, stdout carries exactly one JSON document, so console output
// goes to stderr instead. Bun's console bypasses a patched process.stdout.write,
// which is why the switch lives here rather than on the stream.
let stdoutReservedForJson = false;
export const reserveStdoutForJson = () => {
  stdoutReservedForJson = true;
};
export const isStdoutReservedForJson = () => stdoutReservedForJson;

// Exchange code reports through console, not the adapter, so a `--json` run taps
// it to notice what the result must not hide (a key the venue rejected).
type ConsoleTap = (kind: 'info' | 'warn' | 'error', message: string) => void;
let consoleTap: ConsoleTap | undefined;
export const setConsoleTap = (tap: ConsoleTap | undefined) => {
  consoleTap = tap;
};
export const originalConsoleWarn = console.warn;
const originalConsoleTrace = console.trace;
const originalConsoleInfo = console.info;
const originalConsoleDebug = console.debug;

// =============================================================================
// Readline-aware terminal output
// =============================================================================

/**
 * Bridge to the REPL's readline instance so async terminal output prints ABOVE
 * the prompt instead of shredding the user's in-progress input. Registered once
 * by repl.ts (startRepl); `getReadline` returns the *current* rlInstance, so
 * readline recreation (watch-mode exit) needs no re-registration.
 */
interface PromptController {
  getReadline: () => readline.Interface | null;
  isMonitorMode: () => boolean;
}
let promptController: PromptController | null = null;
export function setPromptController(controller: PromptController | null): void {
  promptController = controller;
}

/**
 * Run `writeFn` (which prints one line to the terminal) without corrupting an
 * active readline prompt: clear the current input line, print, then redraw the
 * prompt WITH the user's typed buffer + cursor restored. Falls through to a
 * plain write when there's no active prompt or we're in a raw-mode watch screen
 * (where readline isn't driving the line).
 */
export function writeAbovePrompt(writeFn: () => void): void {
  const rl = promptController?.getReadline();
  if (!rl || promptController?.isMonitorMode()) {
    writeFn();
    return;
  }
  readline.cursorTo(process.stdout, 0);
  readline.clearLine(process.stdout, 0);
  writeFn();
  // Redraw prompt + current line buffer + cursor. `_refreshLine` is what
  // readline runs on every keystroke; Bun implements it for Node compat. Fall
  // back to prompt(true) (buffer reappears on next keypress) if ever absent.
  const refresh = (rl as unknown as { _refreshLine?: () => void })._refreshLine;
  if (typeof refresh === 'function') {
    refresh.call(rl);
  } else {
    rl.prompt(true);
  }
}

// Track if we've already intercepted
let intercepted = false;

/**
 * Format an argument for logging (full details) vs terminal (clean message)
 */
function formatForLog(arg: unknown): string {
  if (arg instanceof Error) {
    // Format error with nice stack trace
    const errorName = arg.name || 'Error';
    const message = arg.message || 'Unknown error';
    let result = `${errorName}: ${message}`;

    if (arg.stack) {
      // Extract just the stack frames (skip the first line which is the message)
      const stackLines = arg.stack.split('\n').slice(1);
      if (stackLines.length > 0) {
        result += '\n' + stackLines.join('\n');
      }
    }
    return result;
  }
  if (typeof arg === 'string') {
    return arg;
  }
  try {
    return JSON.stringify(arg, null, 2);
  } catch {
    return String(arg);
  }
}

// Operational/keepalive diagnostics are suppressed from the terminal (still
// written to the file). The pattern list is the shared taxonomy in
// `log-classify.ts` — add new noisy prefixes there, not here.
function shouldSuppressLog(args: unknown[]): boolean {
  if (args.length === 0) return false;
  return isOperational(String(args[0]));
}

export function interceptConsole(): void {
  // Prevent double interception
  if (intercepted) return;
  intercepted = true;

  // Shared handler for info-level console methods (log/info). Always logs to
  // the file, then for the terminal applies operational suppression, dedup, and
  // rate limiting, printing above the prompt. `console.info` routes here too —
  // exchange code uses it for verbose dumps (e.g. bitunix balance messages),
  // and untouched it would bypass the file log + suppression + readline entirely.
  const handleInfoLog = (...args: unknown[]) => {
    // Log at `info` (not `debug`) so cli.log ALWAYS captures console.log/info —
    // including the operational noise we suppress from the terminal below.
    // `logger.debug` is gated by MIN_LOG_LEVEL and would be dropped unless
    // DEBUG=1, which would hide the very lines "Full logs → yarn logs" promises.
    logger.info(args.map(formatForLog).join(' '));
    consoleTap?.('info', args.map(formatForLog).join(' '));

    if (shouldSuppressLog(args)) return;

    const message = args.map(formatForLog).join(' ');
    if (isDuplicateMessage(message)) return;
    if (shouldRateLimit()) return;

    writeAbovePrompt(() => (stdoutReservedForJson ? originalConsoleError : originalConsoleLog)(...args));
  };
  console.log = handleInfoLog;
  console.info = handleInfoLog;

  // console.debug is the noisiest tier — never terminal, and file only when
  // DEBUG=1 (logger.debug is gated by MIN_LOG_LEVEL). Matches the repo
  // convention to avoid console.debug for user-facing output.
  console.debug = (...args: unknown[]) => {
    logger.debug(args.map(formatForLog).join(' '));
  };

  console.error = (...args: unknown[]) => {
    // Always log full details (including stack traces) to file
    logger.error(args.map(formatForLog).join(' '));

    // For terminal: extract just the error message, skip noise
    const cleanMessage = extractErrorMessage(args);
    if (!cleanMessage) return;
    consoleTap?.('error', cleanMessage);

    // Apply deduplication (10 second window)
    if (isDuplicateMessage(cleanMessage)) return;

    // Apply rate limiting
    if (shouldRateLimit()) return;

    // Format with red X like watch mode
    writeAbovePrompt(() => originalConsoleError('\x1b[31m✗\x1b[0m', cleanMessage));
    setStatus(cleanMessage, classifyLog(cleanMessage, 'error'));
  };

  console.warn = (...args: unknown[]) => {
    // Always log to file
    logger.warn(args.map(formatForLog).join(' '));

    // Show clean message in terminal
    const cleanMessage = extractErrorMessage(args);
    if (!cleanMessage) return;
    consoleTap?.('warn', cleanMessage);

    // Apply deduplication
    if (isDuplicateMessage(cleanMessage)) return;

    // Apply rate limiting
    if (shouldRateLimit()) return;

    writeAbovePrompt(() => originalConsoleWarn('\x1b[33m⚠\x1b[0m', cleanMessage));
    setStatus(cleanMessage, classifyLog(cleanMessage, 'warn'));
  };

  // Suppress console.trace in terminal but log to file
  console.trace = (...args: unknown[]) => {
    // Create a stack trace and log it to file
    const stack = new Error().stack || '';
    logger.error('TRACE: ' + args.map(formatForLog).join(' ') + '\n' + stack);
    // Don't output to terminal - full trace is in log file
  };
}

/**
 * Extract a clean error message from console args
 */
function extractErrorMessage(args: unknown[]): string | null {
  if (args.length === 0) return null;

  const parts: string[] = [];
  for (const arg of args) {
    if (arg instanceof Error) {
      parts.push(arg.message);
    } else if (typeof arg === 'string') {
      // Skip common prefixes that are redundant
      if (arg === 'exchange error' || arg === 'Exchange error:') continue;
      parts.push(arg);
    } else if (typeof arg === 'object' && arg !== null) {
      // For objects like API responses, extract message if present
      if ('message' in arg) {
        parts.push(String((arg as { message: unknown }).message));
      } else if ('retMsg' in arg) {
        // Bybit-style error
        parts.push(String((arg as { retMsg: unknown }).retMsg));
      } else if ('msg' in arg) {
        parts.push(String((arg as { msg: unknown }).msg));
      }
    }
  }

  return parts.length > 0 ? parts.join(' ') : null;
}

export function restoreConsole(): void {
  console.log = originalConsoleLog;
  console.error = originalConsoleError;
  console.trace = originalConsoleTrace;
  console.warn = originalConsoleWarn;
  console.info = originalConsoleInfo;
  console.debug = originalConsoleDebug;
  intercepted = false;
}
