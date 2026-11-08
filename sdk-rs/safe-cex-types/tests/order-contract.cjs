const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const { normalized } = require('./parity.cjs');
const root = path.resolve(__dirname, '../../..');
const generated = path.join(root, 'packages/safe-cex/src/generated');
const names = ['Order', 'PlaceOrderOpts', 'UpdateOrderOpts', 'OrderSide', 'OrderStatus', 'OrderType',
  'OrderTimeInForce', 'OrderStopTriggerType', 'OrderRejectReason'];

test('Order slice exports the exact Rust-generated declarations and runtime enums', () => {
  for (const name of names) {
    const output = fs.readFileSync(path.join(generated, `${name}.ts`), 'utf8');
    assert.equal(output, fs.readFileSync(path.join(__dirname, '../bindings', `${name}.ts`), 'utf8'));
    const ast = ts.createSourceFile(`${name}.ts`, output, ts.ScriptTarget.Latest, true);
    const declaration = ast.statements.find(node => node.name?.text === name);
    assert(declaration, name);
    if (name.startsWith('Order') && name !== 'Order') {
      assert(ts.isEnumDeclaration(declaration), name);
      const runtime = {};
      const output = ts.transpileModule(ast.text, {compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText;
      new Function('exports', output)(runtime);
      assert.deepEqual(runtime[name], Object.fromEntries(declaration.members.map(member => [member.name.text, member.initializer.text])));
    }
  }
});

test('Order slice keeps the maintained field shape, nested readonly and optionality', () => {
  const source = ts.createSourceFile('types.ts', fs.readFileSync(path.join(root, 'packages/safe-cex/src/types.ts'), 'utf8'), ts.ScriptTarget.Latest, true);
  for (const name of ['Order', 'PlaceOrderOpts', 'UpdateOrderOpts']) {
    const old = source.statements.find(node => ts.isTypeAliasDeclaration(node) && node.name.text === name);
    if (!old) {
      assert(source.statements.some(node => ts.isExportDeclaration(node)
        && node.moduleSpecifier?.text === `./generated/${name}`
        && node.exportClause?.elements?.some(element => element.name.text === name)), `${name}: missing public re-export`);
      continue;
    }
    const ast = ts.createSourceFile(`${name}.ts`, fs.readFileSync(path.join(generated, `${name}.ts`), 'utf8'), ts.ScriptTarget.Latest, true);
    const current = ast.statements.find(node => ts.isTypeAliasDeclaration(node) && node.name.text === name);
    assert.deepEqual(normalized(current.type), normalized(old.type), name);
  }
});

test('generated Order contracts typecheck both ways and reject strings, null and mutation', () => {
  const fixture = path.join(generated, '__order_contract_check__.ts');
  const imports = names.filter(name => name.startsWith('Order') && name !== 'Order')
    .map(name => `import { ${name} } from './${name}';`).join('\n');
  const source = `${imports}
import type { Order } from './Order';
import type { PlaceOrderOpts } from './PlaceOrderOpts';
import type { UpdateOrderOpts } from './UpdateOrderOpts';
import type { Order as RustOrder } from '${path.join(root, 'sdk-rs/safe-cex-types/bindings/Order')}';
import type { PlaceOrderOpts as RustPlace } from '${path.join(root, 'sdk-rs/safe-cex-types/bindings/PlaceOrderOpts')}';
import type { UpdateOrderOpts as RustUpdate } from '${path.join(root, 'sdk-rs/safe-cex-types/bindings/UpdateOrderOpts')}';
import type { Order as PublicOrder, PlaceOrderOpts as PublicPlace, UpdateOrderOpts as PublicUpdate } from '../types';
declare const order: Order; declare const rustOrder: RustOrder;
declare const publicOrder: PublicOrder; declare const publicPlace: PublicPlace; declare const publicUpdate: PublicUpdate;
const publicA: PublicOrder = order; const publicB: Order = publicOrder;
const publicC: PlaceOrderOpts = publicPlace; const publicD: PublicPlace = publicC;
const publicE: UpdateOrderOpts = publicUpdate; const publicF: PublicUpdate = publicE;
const a: Order = rustOrder; const b: RustOrder = order;
declare const place: PlaceOrderOpts; declare const rustPlace: RustPlace;
const c: PlaceOrderOpts = rustPlace; const d: RustPlace = place;
declare const update: UpdateOrderOpts; declare const rustUpdate: RustUpdate;
const e: UpdateOrderOpts = rustUpdate; const f: RustUpdate = update;
const minimal: Order = { id:'o', status:OrderStatus.Open, symbol:'BTC', type:OrderType.Limit,
 side:OrderSide.Buy, price:0, amount:1, filled:0, remaining:1, reduceOnly:false, lastUpdate:0 };
const zero: PlaceOrderOpts = {symbol:'BTC', type:OrderType.StopLoss, side:OrderSide.Sell, amount:1,
 triggerPrice:0, stopLoss:0, takeProfit:0, timeInForce:OrderTimeInForce.PostOnly,
 triggerType:OrderStopTriggerType.MarkPrice};
const priceUpdate: UpdateOrderOpts = {order, update:{price:0}};
const sizeUpdate: UpdateOrderOpts = {order, update:{amount:0}};
// @ts-expect-error Readonly top-level fields.
minimal.remaining = 2;
// @ts-expect-error Readonly placement fields.
zero.amount = 2;
// @ts-expect-error Readonly nested update.
if ('price' in priceUpdate.update) priceUpdate.update.price = 2;
// @ts-expect-error At least one amendment is required.
const emptyUpdate: UpdateOrderOpts = {order, update:{}};
// @ts-expect-error Bare strings cannot replace the generated enum.
const side: Order['side'] = 'buy';
// @ts-expect-error Optional prices are not nullable.
const badTrigger: PlaceOrderOpts = {...zero, triggerPrice:null};
declare const extra: NonNullable<Order['extra']>;
// @ts-expect-error Nested extras remain readonly.
extra.remainingNative = 2;
const reject: NonNullable<Order['extra']> = {rejectReason:OrderRejectReason.PostOnly};
`;
  const configPath = path.join(root, 'packages/safe-cex/tsconfig.json');
  const config = ts.readConfigFile(configPath, ts.sys.readFile);
  const parsed = ts.parseJsonConfigFileContent(config.config, ts.sys, path.dirname(configPath));
  const host = ts.createCompilerHost(parsed.options);
  const original = host.getSourceFile.bind(host);
  host.getSourceFile = (file, version, ...args) => file === fixture
    ? ts.createSourceFile(file, source, version, true) : original(file, version, ...args);
  const program = ts.createProgram([fixture], {...parsed.options, incremental:false}, host);
  assert.deepEqual(ts.getPreEmitDiagnostics(program).map(d => ts.flattenDiagnosticMessageText(d.messageText, '\n')), []);
});
