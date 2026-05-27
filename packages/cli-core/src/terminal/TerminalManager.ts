import * as readline from 'node:readline';
import { ExecutionMode } from '../types';

/**
 * Callback type for readline line events
 */
export type LineHandler = (line: string) => void;

/**
 * Callback type for readline close events
 */
export type CloseHandler = () => void;

/**
 * Options for initializing readline
 */
export interface ReadlineOptions {
  /** Initial history entries */
  history?: string[];
  /** Maximum history size */
  historySize?: number;
  /** Prompt string generator */
  getPrompt: () => string;
  /** Handler for line input */
  onLine: LineHandler;
  /** Handler for close (Ctrl+D, etc.) */
  onClose: CloseHandler;
}

/**
 * TerminalManager - Centralized terminal state management for CLI
 *
 * Handles:
 * - Readline interface lifecycle
 * - Suspend/resume for commands that take over terminal
 * - Exit key detection (Escape, Ctrl+X, Ctrl+C)
 * - Clean terminal state transitions
 */
export class TerminalManager {
  private readline: readline.Interface | null = null;
  private isInSuspendMode = false;
  private savedHistory: string[] = [];
  private readlineOptions: ReadlineOptions | null = null;

  /**
   * Initialize readline interface for REPL mode
   */
  initReadline(options: ReadlineOptions): readline.Interface {
    this.readlineOptions = options;

    const rl = readline.createInterface({
      input: process.stdin,
      output: process.stdout,
      prompt: options.getPrompt(),
      history: options.history || [],
      historySize: options.historySize || 1000,
    });

    rl.on('line', options.onLine);
    rl.on('close', options.onClose);

    this.readline = rl;
    return rl;
  }

  /**
   * Get the current readline instance
   */
  getReadline(): readline.Interface | null {
    return this.readline;
  }

  /**
   * Check if terminal is in suspend mode
   */
  get isSuspended(): boolean {
    return this.isInSuspendMode;
  }

  /**
   * Enter suspend mode - closes readline, prepares for suspending command
   * Called when a suspending command (serve, watch, pair) starts
   */
  enterSuspendMode(): void {
    if (this.isInSuspendMode) return;
    this.isInSuspendMode = true;

    // Save history before closing
    if (this.readline) {
      this.savedHistory = [...((this.readline as any).history || [])];
      this.readline.pause();
      this.readline.close();
      this.readline = null;
    }

    // Clear any pending output on current line
    process.stdout.write('\x1B[2K\r');
  }

  /**
   * Exit suspend mode - restores readline with saved state
   * Called when a suspending command exits
   */
  exitSuspendMode(): void {
    if (!this.isInSuspendMode) return;
    this.isInSuspendMode = false;

    // Ensure clean terminal state
    if (process.stdin.isTTY && (process.stdin as any).isRaw) {
      process.stdin.setRawMode(false);
    }

    // Restore readline if we have options
    if (this.readlineOptions) {
      const rl = readline.createInterface({
        input: process.stdin,
        output: process.stdout,
        prompt: this.readlineOptions.getPrompt(),
        history: this.savedHistory,
        historySize: this.readlineOptions.historySize || 1000,
      });

      rl.on('line', this.readlineOptions.onLine);
      rl.on('close', this.readlineOptions.onClose);

      this.readline = rl;
      rl.prompt();
    }
  }

  /**
   * Set up exit key handlers based on execution mode
   *
   * @param mode - CLI_ARGS or REPL mode
   * @param callback - Called when exit key is pressed
   * @returns Cleanup function to remove handlers
   */
  setupExitHandlers(mode: ExecutionMode, callback: () => void): () => void {
    const cleanups: (() => void)[] = [];

    // Enter raw mode for key detection (Escape = 27, Ctrl+X = 24)
    if (process.stdin.isTTY) {
      process.stdin.setRawMode(true);
    }
    process.stdin.resume();

    const rawHandler = (data: Buffer) => {
      const key = data[0];
      // Escape (27) or Ctrl+X (24)
      if (key === 27 || key === 24) {
        callback();
      }
    };

    process.stdin.on('data', rawHandler);
    cleanups.push(() => {
      process.stdin.off('data', rawHandler);
      if (process.stdin.isTTY) {
        process.stdin.setRawMode(false);
      }
    });

    // SIGINT handler for CLI args mode only (Ctrl+C works reliably there)
    if (mode === ExecutionMode.CLI_ARGS) {
      const sigintHandler = () => callback();
      process.on('SIGINT', sigintHandler);
      cleanups.push(() => process.off('SIGINT', sigintHandler));
    }

    return () => cleanups.forEach((fn) => fn());
  }

  /**
   * Get the appropriate exit message based on execution mode
   */
  getExitMessage(mode: ExecutionMode): string {
    return mode === ExecutionMode.CLI_ARGS
      ? 'Press Ctrl+C or Escape to exit'
      : 'Press Escape or Ctrl+X to exit';
  }

  /**
   * Update the prompt string
   */
  setPrompt(prompt: string): void {
    if (this.readline) {
      this.readline.setPrompt(prompt);
    }
  }

  /**
   * Show the prompt
   */
  prompt(): void {
    if (this.readline && !this.isInSuspendMode) {
      this.readline.prompt();
    }
  }

  /**
   * Clean up resources
   */
  dispose(): void {
    if (this.readline) {
      this.readline.close();
      this.readline = null;
    }
  }
}

// Singleton instance for CLI use
let terminalManagerInstance: TerminalManager | null = null;

/**
 * Get or create the singleton TerminalManager instance
 */
export function getTerminalManager(): TerminalManager {
  if (!terminalManagerInstance) {
    terminalManagerInstance = new TerminalManager();
  }
  return terminalManagerInstance;
}
