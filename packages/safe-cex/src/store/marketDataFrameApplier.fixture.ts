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
import { sharedMarketDataSource } from './sharedMarketDataSource.fixture';

// Exercise the real integrated class while shared cutover remains integrator-owned.
export function marketDataFrameApplier(): typeof FrameApplier {
  const root = resolve(__dirname, '../../../..');
  const filename = 'packages/safe-cex/src/store/frameApplier.ts';
  let source = readFileSync(resolve(root, filename), 'utf8');
  if (source.includes('private readonly bookFrames = new BookFrames()')) return actual.FrameApplier;
  source = sharedMarketDataSource(filename);
  source = source.replace(
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
