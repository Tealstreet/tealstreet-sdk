import { readFileSync, readdirSync } from 'node:fs';
import { extname, relative, resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

const SOURCE_EXTENSIONS = new Set(['.cjs', '.js', '.mjs', '.ts', '.tsx']);

const walkFiles = (directory: string): string[] =>
  readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = resolve(directory, entry.name);
    return entry.isDirectory() ? walkFiles(path) : [path];
  });

describe('exchange runtime-tree boundary', () => {
  // Walks the source tree; under CI load this outruns vitest's 5s default.
  it('contains no tests, manual utilities, or credential-reading executables', () => {
    const exchangesDirectory = resolve(process.cwd(), 'src/exchanges');
    const files = walkFiles(exchangesDirectory);
    const forbiddenPaths = files
      .map((file) => relative(exchangesDirectory, file).replaceAll('\\', '/'))
      .filter((file) =>
        /(^|\/)(?:__tests__|tests?|manual|debug|scripts?)(?:\/|$)|\.(?:test|spec)\.[^.]+$/i.test(
          file,
        ),
      );
    const executableMarkers: Array<[string, RegExp]> = [
      ['credential file access', /\.apikey\.json/],
      ['executable shebang', /^#!.*\b(?:node|tsx|ts-node)\b/m],
      ['manual run instruction', /\bRun with:/i],
      ['process exit', /\bprocess\.exit\s*\(/],
      ['script marker', /\b(?:manual|debug|cleanup|helper) script\b/i],
    ];
    const forbiddenSources = files
      .filter((file) => SOURCE_EXTENSIONS.has(extname(file)))
      .flatMap((file) => {
        const source = readFileSync(file, 'utf8');
        const fileName = relative(exchangesDirectory, file).replaceAll('\\', '/');
        return executableMarkers
          .filter(([, pattern]) => pattern.test(source))
          .map(([marker]) => `${fileName}: ${marker}`);
      });

    expect(forbiddenPaths).toEqual([]);
    expect(forbiddenSources).toEqual([]);
  }, 30_000);
});
