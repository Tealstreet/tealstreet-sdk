/**
 * ANSI color codes for CLI output
 * Standardized colors used across all CLI commands
 */

// Basic colors
export const GREEN = '\x1b[32m';
export const RED = '\x1b[31m';
export const YELLOW = '\x1b[33m';
export const CYAN = '\x1b[36m';
export const BLUE = '\x1b[34m';
export const MAGENTA = '\x1b[35m';
export const WHITE = '\x1b[37m';

// Modifiers
export const RESET = '\x1b[0m';
export const DIM = '\x1b[2m';
export const BOLD = '\x1b[1m';
export const UNDERLINE = '\x1b[4m';

// Screen control
export const CLEAR_SCREEN = '\x1b[2J\x1b[H'; // Clear screen and move cursor to top
export const CLEAR_LINE = '\r\x1b[K'; // Clear current line

// Semantic colors (use these for consistency)
export const COLOR_LONG = GREEN;
export const COLOR_SHORT = RED;
export const COLOR_PROFIT = GREEN;
export const COLOR_LOSS = RED;
export const COLOR_WARNING = YELLOW;
export const COLOR_INFO = CYAN;
export const COLOR_PRICE = CYAN;
export const COLOR_ERROR = RED;

// Candle colors
export const COLOR_CANDLE_UP = GREEN;
export const COLOR_CANDLE_DOWN = RED;
