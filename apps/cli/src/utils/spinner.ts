/**
 * Simple spinner replacement for ora
 *
 * ora breaks raw mode stdin handling, so we use simple print-based
 * status messages instead of animated spinners.
 */

import chalk from 'chalk';
import { originalConsoleLog } from '../logger';

const print = originalConsoleLog;

export interface Spinner {
  start: (text?: string) => Spinner;
  succeed: (text: string) => Spinner;
  fail: (text: string) => Spinner;
  warn: (text: string) => Spinner;
  stop: () => Spinner;
  text: string;
}

/**
 * Create a simple spinner that prints status messages
 * API compatible with ora for easy migration
 */
export function createSpinner(initialText: string): Spinner {
  let currentText = initialText;

  const instance: Spinner = {
    start: (text?: string) => {
      if (text) currentText = text;
      print(chalk.gray(`  ${currentText}`));
      return instance;
    },
    succeed: (text: string) => {
      print(chalk.green(`  ✓ ${text}`));
      return instance;
    },
    fail: (text: string) => {
      print(chalk.red(`  ✗ ${text}`));
      return instance;
    },
    warn: (text: string) => {
      print(chalk.yellow(`  ⚠ ${text}`));
      return instance;
    },
    stop: () => instance,
    get text() {
      return currentText;
    },
    set text(val: string) {
      currentText = val;
      print(chalk.gray(`  ${val}`));
    },
  };

  return instance;
}
