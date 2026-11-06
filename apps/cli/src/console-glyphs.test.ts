import { readdirSync, readFileSync, statSync } from 'fs';
import { join } from 'path';
import { describe, expect, it, vi } from 'vitest';
import { TRANSLITERATION, asciiText, glyphMode, glyphsFor, installAsciiConsole } from './console-glyphs';

const UTF8 = 65001;
const OEM_US = 437;

describe('glyphMode', () => {
  const mode = (platform: NodeJS.Platform, env: NodeJS.ProcessEnv, codePage?: number) =>
    glyphMode({ platform, env, codePage: () => codePage });

  it.each<[NodeJS.Platform, NodeJS.ProcessEnv, number | undefined, 'unicode' | 'ascii']>([
    ['darwin', {}, undefined, 'unicode'],
    ['linux', {}, undefined, 'unicode'],
    ['linux', { WT_SESSION: 'x' }, OEM_US, 'unicode'],
    // cmd/conhost: ASCII whatever the code page, its fonts lack the markers.
    ['win32', {}, OEM_US, 'ascii'],
    ['win32', {}, UTF8, 'ascii'],
    // Windows Terminal renders Unicode only on a UTF-8 code page.
    ['win32', { WT_SESSION: 'x' }, UTF8, 'unicode'],
    ['win32', { WT_SESSION: 'x' }, OEM_US, 'ascii'],
    ['win32', { WT_SESSION: 'x' }, undefined, 'ascii'],
    // The override wins in both directions.
    ['darwin', { TEALSTREET_ASCII: '1' }, undefined, 'ascii'],
    ['win32', { WT_SESSION: 'x', TEALSTREET_ASCII: '1' }, UTF8, 'ascii'],
    ['win32', { TEALSTREET_ASCII: '0' }, OEM_US, 'unicode'],
    // Anything else is no override.
    ['win32', { TEALSTREET_ASCII: 'yes' }, OEM_US, 'ascii'],
    ['darwin', { TEALSTREET_ASCII: '' }, undefined, 'unicode'],
  ])('%s %j code page %s -> %s', (platform, env, codePage, expected) => {
    expect(mode(platform, env, codePage)).toBe(expected);
  });

  it('reads the code page only for Windows Terminal on Windows', () => {
    const codePage = vi.fn(() => UTF8);
    glyphMode({ platform: 'darwin', env: { WT_SESSION: 'x' }, codePage });
    glyphMode({ platform: 'win32', env: {}, codePage });
    glyphMode({ platform: 'win32', env: { TEALSTREET_ASCII: '0', WT_SESSION: 'x' }, codePage });
    expect(codePage).not.toHaveBeenCalled();
    glyphMode({ platform: 'win32', env: { WT_SESSION: 'x' }, codePage });
    expect(codePage).toHaveBeenCalledTimes(1);
  });
});

describe('glyph tables', () => {
  it('ASCII markers are pure ASCII and cover every Unicode glyph', () => {
    const unicode = glyphsFor('unicode');
    const ascii = glyphsFor('ascii');
    expect(Object.keys(ascii).sort()).toEqual(Object.keys(unicode).sort());
    for (const [name, value] of Object.entries(ascii)) expect(value, name).toMatch(/^[\x20-\x7E]+$/);
    expect([ascii.error, ascii.warn, ascii.ok, ascii.info]).toEqual(['[x]', '[!]', '[ok]', '[i]']);
  });

  it('Unicode keeps the glyphs the CLI has always printed', () => {
    const unicode = glyphsFor('unicode');
    expect([unicode.error, unicode.warn, unicode.ok, unicode.info]).toEqual(['✗', '⚠', '✓', 'ℹ']);
    expect(`${unicode.paused}  Paused`).toBe('⏸️  Paused');
    expect(`${unicode.warnEmoji}  x`).toBe('⚠️  x');
  });
});

describe('asciiText', () => {
  it('keeps box-drawn table columns aligned', () => {
    const table = ['┌──────┬─────┐', '│ BTC  │ 1.5 │', '├──────┼─────┤', '└──────┴─────┘'];
    const ascii = table.map(asciiText);
    expect(ascii).toEqual(['+------+-----+', '| BTC  | 1.5 |', '+------+-----+', '+------+-----+']);
    ascii.forEach((line, i) => expect(line.length).toBe(table[i].length));
  });

  it('transliterates prose punctuation and leaves ASCII alone', () => {
    expect(asciiText('stopped — retry… a → b, n ≥ 8')).toBe('stopped - retry. a > b, n >= 8');
    expect(asciiText('plain \x1b[31mred\x1b[0m text')).toBe('plain \x1b[31mred\x1b[0m text');
    expect(asciiText('⚠️ x')).toBe('! x');
    expect(asciiText('🚀')).toBe('?');
  });
});

describe('installAsciiConsole', () => {
  it('transliterates string arguments and string writes only', () => {
    const calls: unknown[][] = [];
    const record = (...args: unknown[]) => void calls.push(args);
    const target = { log: record, info: record, warn: record, error: record, debug: record, trace: record };
    const written: unknown[] = [];
    const stream = {
      write: function (chunk: unknown) {
        written.push(chunk);
        return true;
      } as NodeJS.WriteStream['write'],
    };
    installAsciiConsole(target, [stream]);
    const bytes = Buffer.from('✓');
    target.log('✓ done', 3, { a: '✓' });
    target.error('✗ failed');
    stream.write('─ rule');
    stream.write(bytes);
    expect(calls).toEqual([['v done', 3, { a: '✓' }], ['x failed']]);
    expect(written).toEqual(['- rule', bytes]);
  });
});

/** Source that prints a glyph must go through `G` or the transliteration table. */
describe('printed glyph guard', () => {
  const MARKERS = new Set(['✓', '✔', '✗', '✘', '⚠', 'ℹ', '⛔', '️']);
  const sources = (dir: string): string[] =>
    readdirSync(dir).flatMap((name) => {
      const path = join(dir, name);
      if (statSync(path).isDirectory()) return name === '__tests__' ? [] : sources(path);
      return /\.ts$/.test(name) && !/\.(test|d)\.ts$/.test(name) && name !== 'console-glyphs.ts' ? [path] : [];
    });
  const code = (text: string) =>
    text
      .replace(/\/\*[\s\S]*?\*\//g, '')
      .split('\n')
      .filter((line) => !/^\s*\/\//.test(line))
      .map((line) => line.replace(/\s\/\/\s.*$/, ''))
      .join('\n');

  it('no source prints a marker, emoji or unmapped glyph directly', () => {
    const offenders: string[] = [];
    for (const path of sources(__dirname)) {
      for (const glyph of code(readFileSync(path, 'utf8')).match(/[^\x00-\x7F]/gu) ?? []) {
        if (MARKERS.has(glyph) || !(glyph in TRANSLITERATION)) offenders.push(`${path}: ${glyph}`);
      }
    }
    expect(offenders).toEqual([]);
  });
});
