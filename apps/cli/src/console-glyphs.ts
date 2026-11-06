/**
 * Every non-ASCII glyph the CLI prints, in one table.
 *
 * The legacy Windows console (cmd/conhost) renders glyphs outside its code page
 * and font as '?' and shifts the font on fallback, so there the CLI prints ASCII
 * markers instead. Windows Terminal on a UTF-8 code page, and every other
 * platform, keeps Unicode. `TEALSTREET_ASCII=1` forces ASCII, `=0` forces Unicode.
 */

export type GlyphMode = 'unicode' | 'ascii';

const UTF8_CODE_PAGE = 65001;

export interface ConsoleEnvironment {
  platform: NodeJS.Platform;
  env: NodeJS.ProcessEnv;
  /** The console output code page, or undefined when it cannot be read. */
  codePage: () => number | undefined;
}

export function glyphMode({ platform, env, codePage }: ConsoleEnvironment): GlyphMode {
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

const UNICODE = {
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

export type Glyphs = { readonly [K in keyof typeof UNICODE]: string };

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

export const glyphsFor = (mode: GlyphMode): Glyphs => (mode === 'ascii' ? ASCII : UNICODE);

/** The catch-all for text that bypasses `G` (shared-package tables, dependencies):
 * one cell per glyph so columns stay aligned. */
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
  '✓': 'v',
  '✔': 'v',
  '✗': 'x',
  '✘': 'x',
  '⚠': '!',
  ℹ: 'i',
  '\uFE0F': '',
};
const NON_ASCII = /[^\x00-\x7F]/gu;

export function asciiText(text: string): string {
  return text.replace(NON_ASCII, (glyph) => TRANSLITERATION[glyph] ?? '?');
}

export const GLYPH_MODE: GlyphMode = glyphMode({
  platform: process.platform,
  env: process.env,
  codePage: windowsConsoleCodePage,
});

/** The glyphs for this process's console. */
export const G: Glyphs = glyphsFor(GLYPH_MODE);

type Write = typeof process.stdout.write;
const asciiWrite = (write: Write): Write =>
  function (this: NodeJS.WriteStream, chunk: unknown, ...rest: unknown[]) {
    return (write as (...args: unknown[]) => boolean).call(
      this,
      typeof chunk === 'string' ? asciiText(chunk) : chunk,
      ...rest,
    );
  } as Write;

/**
 * Transliterate everything bound for an ASCII console. Runs when this module
 * loads, so logger.ts (which imports it first) captures the wrapped console.
 * Bun's console bypasses `process.stdout.write`, so both are wrapped.
 */
export function installAsciiConsole(
  target: Pick<Console, 'log' | 'info' | 'warn' | 'error' | 'debug' | 'trace'> = console,
  streams: Pick<NodeJS.WriteStream, 'write'>[] = [process.stdout, process.stderr],
): void {
  for (const method of ['log', 'info', 'warn', 'error', 'debug', 'trace'] as const) {
    const original = target[method];
    target[method] = (...args: unknown[]) =>
      original(...args.map((arg) => (typeof arg === 'string' ? asciiText(arg) : arg)));
  }
  for (const stream of streams) stream.write = asciiWrite(stream.write);
}

if (GLYPH_MODE === 'ascii') installAsciiConsole();
