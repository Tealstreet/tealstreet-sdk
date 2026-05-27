/**
 * Centralized exit key handling for CLI suspending commands
 *
 * REPL mode: Escape or Ctrl+X (Ctrl+C stays with readline for line cancel)
 * CLI args mode: Ctrl+C, Escape, or Ctrl+X
 */

// Key codes
export const KEY_ESCAPE = 27;
export const KEY_CTRL_X = 24;
export const KEY_CTRL_C = 3;

/**
 * Check if key is an exit key for REPL mode (Escape or Ctrl+X)
 */
export function isReplExitKey(keyCode: number): boolean {
  return keyCode === KEY_ESCAPE || keyCode === KEY_CTRL_X;
}

/**
 * Check if key is an exit key for CLI args mode (Ctrl+C, Escape, or Ctrl+X)
 */
export function isCliExitKey(keyCode: number): boolean {
  return keyCode === KEY_CTRL_C || keyCode === KEY_ESCAPE || keyCode === KEY_CTRL_X;
}

/**
 * Get exit message for REPL mode
 */
export function getReplExitMessage(): string {
  return 'Press Escape or Ctrl+X to exit';
}

/**
 * Get cancel message for REPL mode (lowercase, for use in spinners)
 */
export function getReplCancelMessage(): string {
  return 'press Escape to cancel';
}

/**
 * Get cancel message for REPL mode (capitalized, for standalone prompts)
 */
export function getReplCancelPrompt(): string {
  return 'Press Escape to cancel';
}

/**
 * Get exit message for CLI args mode
 */
export function getCliExitMessage(): string {
  return 'Press Ctrl+C or Escape to exit';
}

/**
 * Get cancel message for CLI args mode
 */
export function getCliCancelMessage(): string {
  return 'press Ctrl+C to cancel';
}

/**
 * Get exit message based on execution mode
 */
export function getExitMessage(isReplMode: boolean): string {
  return isReplMode ? getReplExitMessage() : getCliExitMessage();
}

/**
 * Get cancel message based on execution mode (lowercase, for use in spinners)
 */
export function getCancelMessage(isReplMode: boolean): string {
  return isReplMode ? getReplCancelMessage() : getCliCancelMessage();
}

/**
 * Check if key is an exit key based on execution mode
 */
export function isExitKey(keyCode: number, isReplMode: boolean): boolean {
  return isReplMode ? isReplExitKey(keyCode) : isCliExitKey(keyCode);
}
