/**
 * Every non-ASCII glyph the CLI prints, in one table.
 *
 * The legacy Windows console (cmd/conhost) renders glyphs outside its code page
 * and font as '?' and shifts the font on fallback, so a terminal there gets ASCII
 * markers instead. Windows Terminal on a UTF-8 code page, and every other
 * platform, keeps Unicode. `TEALSTREET_ASCII=1` forces ASCII, `=0` forces Unicode.
 *
 * ASCII is a human-terminal rendering only: it needs stdout to be a TTY, and it
 * is applied by terminal renderers and terminal sinks, never to JSON, pipes or
 * capture/listener payloads, which always carry the canonical Unicode text.
 */

export type GlyphMode = 'unicode' | 'ascii';

const UTF8_CODE_PAGE = 65001;

export interface ConsoleEnvironment {
  platform: NodeJS.Platform;
  env: NodeJS.ProcessEnv;
  /** Whether stdout is a terminal; redirected output stays canonical. */
  isTTY: boolean;
  /** The console output code page, or undefined when it cannot be read. */
  codePage: () => number | undefined;
}

export function glyphMode({ platform, env, isTTY, codePage }: ConsoleEnvironment): GlyphMode {
  if (!isTTY) return 'unicode';
  if (env.TEALSTREET_ASCII === '1') return 'ascii';
  if (env.TEALSTREET_ASCII === '0') return 'unicode';
  if (platform !== 'win32') return 'unicode';
  if (!env.WT_SESSION) return 'ascii';
  return codePage() === UTF8_CODE_PAGE ? 'unicode' : 'ascii';
}

/** One kernel32 call through Bun FFI, read once at startup: no `chcp` spawn. */
function windowsConsoleCodePage(): number | undefined {
  if (typeof Bun === 'undefined') return undefined;
  try {
    const { dlopen, FFIType } = require('bun:ffi') as typeof import('bun:ffi');
    const kernel32 = dlopen('kernel32.dll', { GetConsoleOutputCP: { args: [], returns: FFIType.u32 } });
    try {
      return kernel32.symbols.GetConsoleOutputCP() || undefined;
    } finally {
      kernel32.close();
    }
  } catch {
    return undefined;
  }
}

/** The canonical glyphs: what shared content and machine output always carry. */
export const U = {
  error: '✗',
  warn: '⚠',
  ok: '✓',
  info: 'ℹ',
  critical: '⛔',
  warnEmoji: '⚠️',
  errorEmoji: '❌',
  okEmoji: '✅',
  online: '🌐',
  running: '🟢',
  paused: '⏸️',
  inactive: '⭕',
  dash: '—',
  ellipsis: '…',
  arrow: '→',
  backArrow: '←',
  bullet: '•',
  middot: '·',
  dot: '●',
  sum: 'Σ',
  atLeast: '≥',
  atMost: '≤',
  rule: '─',
  doubleRule: '═',
  bar: '│',
  topLeft: '┌',
  topRight: '┐',
  bottomLeft: '└',
  bottomRight: '┘',
} as const;

export type Glyphs = { readonly [K in keyof typeof U]: string };

const ASCII: Glyphs = {
  error: '[x]',
  warn: '[!]',
  ok: '[ok]',
  info: '[i]',
  critical: '[!!]',
  warnEmoji: '[!]',
  errorEmoji: '[x]',
  okEmoji: '[ok]',
  online: '[net]',
  running: '[on]',
  paused: '[||]',
  inactive: '[off]',
  dash: '-',
  ellipsis: '...',
  arrow: '->',
  backArrow: '<-',
  bullet: '*',
  middot: '-',
  dot: '*',
  sum: 'sum',
  atLeast: '>=',
  atMost: '<=',
  rule: '-',
  doubleRule: '=',
  bar: '|',
  topLeft: '+',
  topRight: '+',
  bottomLeft: '+',
  bottomRight: '+',
};

export const glyphsFor = (mode: GlyphMode): Glyphs => (mode === 'ascii' ? ASCII : U);

/**
 * Canonical text bound for an ASCII terminal (shared-package tables, adapter
 * output): each glyph keeps its JS string length so `padEnd` columns stay
 * aligned. Anything unmapped, such as user data, passes through unchanged.
 */
export const TRANSLITERATION: Readonly<Record<string, string>> = {
  '─': '-',
  '━': '-',
  '═': '=',
  '│': '|',
  '┃': '|',
  '║': '|',
  '┌': '+',
  '┐': '+',
  '└': '+',
  '┘': '+',
  '├': '+',
  '┤': '+',
  '┬': '+',
  '┴': '+',
  '┼': '+',
  '╔': '+',
  '╗': '+',
  '╚': '+',
  '╝': '+',
  '╠': '+',
  '╣': '+',
  '╦': '+',
  '╩': '+',
  '╬': '+',
  '█': '#',
  '▓': '#',
  '▒': ':',
  '░': '.',
  '●': '*',
  '•': '*',
  '·': '.',
  '—': '-',
  '–': '-',
  '…': '.',
  '→': '>',
  '←': '<',
  '↑': '^',
  '↓': 'v',
  '▸': '>',
  '▶': '>',
  '►': '>',
  '◂': '<',
  '◀': '<',
  '◄': '<',
  '≥': '>=',
  '≤': '<=',
  '✓': '+',
  '✔': '+',
  '✗': 'x',
  '✘': 'x',
  '⚠': '!',
  ℹ: 'i',
  '⛔': '!',
  '❌': 'x',
  '✅': '+',
  '⭕': 'o',
  '⏸': '|',
  '⏸️': '||',
  '⚠️': '!!',
  '\uFE0F': '',
  '🟢': 'on',
  '🌐': '@@',
  Σ: 'S',
};
// A glyph with its emoji variation selector maps as one unit (⏸️ is two code units).
const NON_ASCII = /[^\x00-\x7F]\uFE0F?/gu;

export function asciiText(text: string): string {
  return text.replace(NON_ASCII, (glyph) => TRANSLITERATION[glyph] ?? TRANSLITERATION[glyph[0]] ?? glyph);
}

export const GLYPH_MODE: GlyphMode = glyphMode({
  platform: process.platform,
  env: process.env,
  isTTY: Boolean(process.stdout.isTTY),
  codePage: windowsConsoleCodePage,
});

/** The glyphs for terminal renderers (status bar, footer, spinner, terminal log lines). */
export const G: Glyphs = glyphsFor(GLYPH_MODE);

/** Canonical text at a human-terminal sink; identity unless the terminal is ASCII. */
export const terminalText = (text: string): string => (GLYPH_MODE === 'ascii' ? asciiText(text) : text);
