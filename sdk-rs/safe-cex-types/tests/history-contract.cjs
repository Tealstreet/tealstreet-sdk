const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const ts = require('typescript');
const root = path.resolve(__dirname, '../../..');
const names = [
  'HistoricalOrder',
  'HistoricalTrade',
  'ClosedPnlEntry',
  'IncomeRecord',
  'DataProvenance',
  'IncomeCategory',
];
const generated = path.join(root, 'packages/safe-cex/src/generated');

test('history contracts come from the isolated Rust slice generator', () => {
  assert.ok(fs.existsSync(path.join(__dirname, '../src/history.rs')), 'canonical Rust history module is required');
  const { generateHistory } = require('../history-generate.cjs');
  assert.equal(generateHistory({ check: true }), true);
});

test('generated history types preserve readonly, optionality, enums, and mutable journal records', () => {
  const fixture = path.join(generated, '__history_contract_check__.ts');
  const imports = names.map((name) => `import type { ${name} as Generated${name} } from './${name}';`).join('\n');
  const equality = names
    .map(
      (name) => `type Keys${name} = Assert<Equal<keyof Generated${name}, keyof Public${name}>>;
type Shape${name} = Assert<Equal<Generated${name}, Public${name}>>;
declare const rust${name}: Generated${name}; const public${name}: Public${name} = rust${name};
declare const ts${name}: Public${name}; const generated${name}: Generated${name} = ts${name};`,
    )
    .join('\n');
  const source = `${imports}
import type { ${names.map((name) => `${name} as Public${name}`).join(', ')} } from '../types';
import { OrderSide, OrderStatus, OrderType, PositionSide } from '../types';
type Equal<A, B> = (<T>() => T extends A ? 1 : 2) extends (<T>() => T extends B ? 1 : 2) ? true : false;
type Assert<T extends true> = T;
${equality}
const order: GeneratedHistoricalOrder = { id: 'order', symbol: 'BTC', side: OrderSide.Buy,
 type: OrderType.Limit, status: OrderStatus.Closed, amount: 2, filled: 0, createdTime: 1,
 price: 0, fees: 0, realizedPnl: 0, positionSide: PositionSide.Long };
// @ts-expect-error HistoricalOrder fields are readonly.
order.filled = 1;
// @ts-expect-error Bare side literals must not replace runtime enum values.
const bareSide: GeneratedHistoricalOrder['side'] = 'buy';
// @ts-expect-error Optional prices allow absence, not null.
const nullablePrice: GeneratedHistoricalOrder = { ...order, price: null };
const trade: GeneratedHistoricalTrade = { symbol: 'BTC', orderId: 'order', price: 1,
 qty: 0, positionSide: 'both', side: OrderSide.Sell, time: 2 };
// @ts-expect-error HistoricalTrade fields are readonly.
trade.qty = 1;
// @ts-expect-error Trade identity is optional, not nullable.
const nullableId: GeneratedHistoricalTrade = { ...trade, id: null };
const income: GeneratedIncomeRecord = { symbol: '', incomeType: 'VENUE_FEE', income: 0,
 asset: 'USD', timestamp: 0, tranId: 'bill', tradeId: 'execution', category: 'commission' };
income.income = -1;
income.category = 'funding';
// @ts-expect-error No inferred category or widened strings.
income.category = 'gross';
declare let closed: GeneratedClosedPnlEntry;
closed.fees = '0'; closed.leverage = 0; closed.provenance = 'derived';
closed.notional = null; closed.contracts = null; closed.entryPrice = null;
closed.exitPrice = null; closed.unrealizedPnl = null; closed.fees = null;
closed.leverage = null; closed.time = null; closed.closedTime = null;
// @ts-expect-error Nullable values remain required, not undefined.
closed.fees = undefined;
// @ts-expect-error Unknown PnL cannot be added without checking for null.
const uncheckedPnl = closed.unrealizedPnl + 1;
// @ts-expect-error Closed-PnL provenance is required.
const missingProvenance: GeneratedClosedPnlEntry = { symbol: 'BTC' };
`;
  const configFile = path.join(root, 'packages/safe-cex/tsconfig.json');
  const config = ts.readConfigFile(configFile, ts.sys.readFile);
  const parsed = ts.parseJsonConfigFileContent(config.config, ts.sys, path.dirname(configFile));
  const host = ts.createCompilerHost(parsed.options);
  const original = host.getSourceFile.bind(host);
  host.getSourceFile = (file, version, ...args) =>
    file === fixture ? ts.createSourceFile(file, source, version, true) : original(file, version, ...args);
  const verify = () => {
    const program = ts.createProgram([fixture], { ...parsed.options, incremental: false }, host);
    const diagnostics = ts.getPreEmitDiagnostics(program);
    assert.deepEqual(
      diagnostics.map((d) => ts.flattenDiagnosticMessageText(d.messageText, '\n')),
      [],
    );
  };
  verify();
  // Exercise the integrator's deletion/re-export patch in memory; shared files stay untouched.
  const publicPath = path.join(root, 'packages/safe-cex/src/types.ts');
  let integrated = fs.readFileSync(publicPath, 'utf8');
  const ast = ts.createSourceFile(publicPath, integrated, ts.ScriptTarget.Latest, true);
  const declarations = ast.statements.filter(
    (node) => ts.isTypeAliasDeclaration(node) && names.includes(node.name.text),
  );
  for (const node of declarations.sort((a, b) => b.getFullStart() - a.getFullStart())) {
    integrated = integrated.slice(0, node.getFullStart()) + integrated.slice(node.end);
  }
  const reference = '/// <reference path="./env.d.ts" />';
  const referenceStart = integrated.indexOf(reference);
  assert.notEqual(referenceStart, -1, 'public environment reference must exist');
  const referenceEnd = referenceStart + reference.length;
  const replacementExports = declarations
    .map((node) => node.name.text)
    .map((name) => `export type { ${name} } from './generated/${name}';`)
    .join('\n');
  integrated = integrated.slice(0, referenceEnd) + '\n' + replacementExports + integrated.slice(referenceEnd);
  const integratedAst = ts.createSourceFile(publicPath, integrated, ts.ScriptTarget.Latest, true);
  assert.deepEqual(
    integratedAst.referencedFiles.map((file) => file.fileName),
    ['./env.d.ts'],
  );
  const beforeIntegration = host.getSourceFile.bind(host);
  host.getSourceFile = (file, version, ...args) =>
    file === publicPath
      ? ts.createSourceFile(file, integrated, version, true)
      : beforeIntegration(file, version, ...args);
  verify();
});
