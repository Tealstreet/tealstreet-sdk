/**
 * WatchMode - Centralized terminal state management for suspending commands
 *
 * Handles:
 * - Exit key detection (Escape/Ctrl+X for REPL, also Ctrl+C for CLI args)
 * - Exit key messages
 * - Raw mode stdin management
 * - Alternate screen buffer
 * - Cleanup and readline restoration
 */
import { resumeFooter, suspendFooter } from '../footer';
import { suspendStatusLine, resumeStatusLine } from '../status-line';

// Key codes
const KEY_ESCAPE = 27;
const KEY_CTRL_X = 24;
const KEY_CTRL_C = 3;

// Alternate screen buffer escape codes
export const ENTER_ALT_SCREEN = '\x1b[?1049h';
export const EXIT_ALT_SCREEN = '\x1b[?1049l';
export const CURSOR_HOME = '\x1b[H';
export const CLEAR_SCREEN = '\x1b[2J';

/**
 * Execution mode determines which exit keys are active
 */
export type ExecutionMode = 'repl' | 'cli';

export interface WatchModeConfig {
  /**
   * Execution mode:
   * - 'repl': Exit with Escape or Ctrl+X (Ctrl+C stays with readline)
   * - 'cli': Exit with Ctrl+C, Escape, or Ctrl+X
   */
  mode: ExecutionMode;

  /** Use alternate screen buffer (prevents scrollback pollution). Default: true */
  useAltScreen?: boolean;

  /** Called after terminal state is restored (typically recreateReadline) */
  onCleanup?: () => void;

  /** Called before cleanup starts (for custom cleanup like clearing intervals) */
  onBeforeCleanup?: () => void;
}

/**
 * WatchMode class - manages terminal state for suspending commands
 *
 * @example
 * ```typescript
 * const watch = new WatchMode({
 *   mode: 'repl',
 *   onCleanup: () => recreateReadline(),
 * });
 *
 * // Pause readline before entering
 * rl.pause();
 * watch.enter();
 *
 * // Set up render loop
 * const interval = setInterval(() => watch.render(getOutput()), 1000);
 *
 * // Store cleanup for interval
 * watch.onBeforeCleanup = () => clearInterval(interval);
 *
 * // User presses Escape/Ctrl+X -> automatic cleanup
 * // Or call watch.exit() programmatically
 * ```
 */
export class WatchMode {
  private config: Required<Omit<WatchModeConfig, 'onBeforeCleanup'>> & { onBeforeCleanup?: () => void };
  private _isActive = false;
  private onRawKey: ((data: Buffer) => void) | null = null;
  private exitResolvers: Array<() => void> = [];

  constructor(config: WatchModeConfig) {
    this.config = {
      useAltScreen: true,
      onCleanup: () => {},
      ...config,
    };
  }

  /**
   * Check if a key code is an exit key for the current mode
   */
  private isExitKey(keyCode: number): boolean {
    if (this.config.mode === 'repl') {
      // REPL mode: Escape or Ctrl+X
      return keyCode === KEY_ESCAPE || keyCode === KEY_CTRL_X;
    } else {
      // CLI mode: Ctrl+C, Escape, or Ctrl+X
      return keyCode === KEY_CTRL_C || keyCode === KEY_ESCAPE || keyCode === KEY_CTRL_X;
    }
  }

  /**
   * Get the exit message for the current mode
   */
  get exitMessage(): string {
    if (this.config.mode === 'repl') {
      return 'Press Escape or Ctrl+X to exit';
    } else {
      return 'Press Ctrl+C or Escape to exit';
    }
  }

  /**
   * Get a dimmed exit message (for embedding in output)
   */
  get exitMessageDim(): string {
    return `\x1b[2m${this.exitMessage}\x1b[0m`;
  }

  /**
   * Check if watch mode is currently active
   */
  get isActive(): boolean {
    return this._isActive;
  }

  /**
   * Set the before-cleanup callback (useful for clearing intervals)
   */
  set onBeforeCleanup(fn: () => void) {
    this.config.onBeforeCleanup = fn;
  }

  /**
   * Enter watch mode - sets up raw stdin and optional alt screen
   */
  enter(): void {
    if (this._isActive) {
      throw new Error('WatchMode already active');
    }

    this._isActive = true;

    // Enter alternate screen buffer
    if (this.config.useAltScreen) {
      // Reset the top status-line + bottom footer scroll regions before the alt
      // buffer takes over; resumed on exit below.
      suspendStatusLine();
      suspendFooter();
      process.stdout.write(ENTER_ALT_SCREEN);
    }

    // Set up raw key handler
    this.onRawKey = (data: Buffer) => {
      const key = data[0];
      if (this.isExitKey(key)) {
        this.exit();
      }
    };

    // Enter raw mode and start listening
    if (process.stdin.isTTY) {
      process.stdin.setRawMode(true);
    }
    process.stdin.resume();
    process.stdin.on('data', this.onRawKey);
  }

  /**
   * Exit watch mode and restore terminal state
   */
  exit(): void {
    if (!this._isActive) return;
    this._isActive = false;

    // Call before-cleanup (e.g., clear intervals)
    if (this.config.onBeforeCleanup) {
      this.config.onBeforeCleanup();
    }

    // Remove raw key handler
    if (this.onRawKey) {
      process.stdin.removeListener('data', this.onRawKey);
      this.onRawKey = null;
    }

    // Exit raw mode and resume stdin (ensure it's flowing for readline)
    if (process.stdin.isTTY) {
      process.stdin.setRawMode(false);
    }
    process.stdin.resume();

    // Exit alternate screen buffer
    if (this.config.useAltScreen) {
      process.stdout.write(EXIT_ALT_SCREEN);
      // Re-pin the top bar + bottom footer after returning to the main buffer.
      resumeStatusLine();
      resumeFooter();
    }

    // Resolve any waiting promises
    for (const resolve of this.exitResolvers) {
      resolve();
    }
    this.exitResolvers = [];

    // Call cleanup callback (typically recreateReadline)
    this.config.onCleanup();
  }

  /**
   * Returns a promise that resolves when watch mode exits
   */
  untilExit(): Promise<void> {
    if (!this._isActive) {
      return Promise.resolve();
    }
    return new Promise((resolve) => {
      this.exitResolvers.push(resolve);
    });
  }

  /**
   * Render content to the screen (handles alt screen cursor positioning)
   */
  render(content: string): void {
    if (!this._isActive) return;
    if (this.config.useAltScreen) {
      process.stdout.write(CURSOR_HOME + CLEAR_SCREEN + content);
    } else {
      process.stdout.write(content);
    }
  }

  /**
   * Write raw content without clearing (for incremental updates)
   */
  write(content: string): void {
    if (!this._isActive) return;
    process.stdout.write(content);
  }
}

/**
 * Helper to create exit key checker function for the given mode
 */
export function createExitKeyChecker(mode: ExecutionMode): (keyCode: number) => boolean {
  if (mode === 'repl') {
    return (keyCode: number) => keyCode === KEY_ESCAPE || keyCode === KEY_CTRL_X;
  } else {
    return (keyCode: number) => keyCode === KEY_CTRL_C || keyCode === KEY_ESCAPE || keyCode === KEY_CTRL_X;
  }
}

/**
 * Get exit message for the given mode
 */
export function getExitMessage(mode: ExecutionMode): string {
  if (mode === 'repl') {
    return 'Press Escape or Ctrl+X to exit';
  } else {
    return 'Press Ctrl+C or Escape to exit';
  }
}
