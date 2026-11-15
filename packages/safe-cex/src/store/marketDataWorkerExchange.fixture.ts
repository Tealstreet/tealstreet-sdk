import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { nanoid } from 'nanoid';
import ts from 'typescript';

import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import { BaseExchange } from '../exchanges/base';
import { FrameApplyError } from './frameApplier';
import { marketDataFrameApplier } from './marketDataFrameApplier.fixture';
import { sharedMarketDataSource } from './sharedMarketDataSource.fixture';
import { WorkerFrameRecoveryScopes } from './workerFrameRecoveryScopes';

/** Run the exact proposed shared methods without editing integrator-owned files. */
export function marketDataWorkerExchange(): typeof BaseExchange {
  const filename = 'packages/safe-cex/src/exchanges/base.ts';
  const current = readFileSync(resolve(__dirname, '../../../..', filename), 'utf8');
  if (current.includes('new WorkerFrameRecoveryScopes()')) return BaseExchange;
  const source = sharedMarketDataSource(filename);
  const parsed = ts.createSourceFile(filename, source, ts.ScriptTarget.Latest, true);
  const declaration = parsed.statements.find(ts.isClassDeclaration);
  if (!declaration) throw new Error('Missing proposed BaseExchange class');
  const names = new Set([
    'workerFrameRejectedSections',
    'applyWorkerFrameBuffer',
    'listenOrderBookWebworker',
    'listenOHLCVWebworker',
  ]);
  const members = declaration.members.filter((member) => member.name && names.has(member.name.getText(parsed)));
  if (members.length !== names.size) throw new Error('Missing proposed worker recovery methods');
  const code = ts.transpileModule(
    `exports.BaseExchange = class extends BaseExchange { ${members.map((member) => member.getText(parsed)).join('\n')} };`,
    { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS } },
  ).outputText;
  const loaded: { BaseExchange?: typeof BaseExchange } = {};
  new Function(
    'exports',
    'BaseExchange',
    'Frame',
    'FrameApplier',
    'FrameApplyError',
    'WorkerFrameRecoveryScopes',
    'nanoid',
    code,
  )(loaded, BaseExchange, Frame, marketDataFrameApplier(), FrameApplyError, WorkerFrameRecoveryScopes, nanoid);
  if (!loaded.BaseExchange) throw new Error('Missing proposed worker exchange');
  return loaded.BaseExchange;
}
