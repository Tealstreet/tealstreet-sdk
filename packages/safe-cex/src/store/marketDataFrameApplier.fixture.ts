import type { FrameApplier } from './frameApplier';

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import ts from 'typescript';

import * as wire from '@tealstreet/safe-cex-wasm/wire';
import * as types from '../types';
import * as math from '../utils/safe-math';
import * as books from './bookFrames';
import * as candles from './candleFrames';
import * as actual from './frameApplier';
import * as positions from './positionFrames';

// Exercise the real integrated class while shared cutover remains integrator-owned.
export function marketDataFrameApplier(): typeof FrameApplier {
  const root = resolve(__dirname, '../../../..');
  const filename = 'packages/safe-cex/src/store/frameApplier.ts';
  let source = readFileSync(resolve(root, filename), 'utf8');
  if (source.includes('private readonly bookFrames = new BookFrames()')) return actual.FrameApplier;
  const patch = readFileSync(resolve(root, 'sdk-rs/safe-cex-types/tests/market-data-shared.patch'), 'utf8');
  const section = patch.split(`diff --git a/${filename} b/${filename}\n`)[1].split('diff --git ')[0];
  const original = source.split('\n');
  const lines = section.split('\n');
  const output: string[] = [];
  let cursor = 0;
  for (let i = 0; i < lines.length; i++) {
    const hunk = /^@@ -(\d+)(?:,\d+)? \+\d+(?:,\d+)? @@/.exec(lines[i]);
    if (!hunk) continue;
    const start = Number(hunk[1]) - 1;
    output.push(...original.slice(cursor, start));
    cursor = start;
    while (++i < lines.length && !lines[i].startsWith('@@')) {
      const line = lines[i];
      if (!line) {
        if (original[cursor] === '') {
          output.push('');
          cursor++;
        }
        continue;
      }
      if (line.startsWith(' ') || line.startsWith('-')) {
        if (original[cursor++] !== line.slice(1)) throw new Error('Shared FrameApplier patch context drift');
      }
      if (line.startsWith(' ') || line.startsWith('+')) output.push(line.slice(1));
    }
    i--;
  }
  output.push(...original.slice(cursor));
  source = output
    .join('\n')
    .replace(
      /^export class FrameApplyError extends Error \{[\s\S]*?^\}/m,
      "import { FrameApplyError } from '__frameError';",
    );
  const code = ts.transpileModule(source, {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  }).outputText;
  const dependencies: Record<string, unknown> = {
    '@tealstreet/safe-cex-wasm/wire': wire,
    '../types': types,
    '../utils/safe-math': math,
    './bookFrames': books,
    './candleFrames': candles,
    './positionFrames': positions,
    __frameError: actual,
  };
  const loaded: { FrameApplier?: typeof FrameApplier } = {};
  new Function('require', 'exports', code)((name: string) => {
    if (!(name in dependencies)) throw new Error(`Unexpected FrameApplier dependency ${name}`);
    return dependencies[name];
  }, loaded);
  if (!loaded.FrameApplier) throw new Error('Missing integrated FrameApplier');
  return loaded.FrameApplier;
}
