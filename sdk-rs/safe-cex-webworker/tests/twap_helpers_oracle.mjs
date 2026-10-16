// Live bit capture: validated Node 26.7.0 on darwin/arm64; CI captures on its own host.
import fs from 'node:fs';
import vm from 'node:vm';
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = new URL('../../../packages/safe-cex/src/', import.meta.url);
function source(path) {
  const text = fs.readFileSync(new URL(path, root), 'utf8');
  const ast = ts.createSourceFile(path, text, ts.ScriptTarget.Latest, true);
  return ast.statements.filter(s => !ts.isImportDeclaration(s)).map(s => s.getText(ast)).join('\n').replace(/\bexport /g, '');
}
const ast = ts.createSourceFile('types', fs.readFileSync(new URL('types.ts', root), 'utf8'), ts.ScriptTarget.Latest, true);
const enums = ast.statements.filter(n => ts.isEnumDeclaration(n) && ['OrderSide', 'OrderType'].includes(n.name.text)).map(n => n.getText(ast).replace('export ', '')).join('\n');
const context = vm.createContext({});
vm.runInContext(ts.transpileModule(enums + '\n' + source('webworker/twap/TwapInstance.ts') + '\nglobalThis.Instance=TwapInstance;', { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText, context);
const bytes = new DataView(new ArrayBuffer(8));
function bits(n) { if (Number.isNaN(n)) return 'NaN'; bytes.setFloat64(0, n); return bytes.getBigUint64(0).toString(16).padStart(16, '0'); }
function number(n) { bytes.setBigUint64(0, n); return bytes.getFloat64(0); }
const numbers = [0, -0, 1, -1, 0.1, -0.1, 1/3, Number.MIN_VALUE, -Number.MIN_VALUE, Number.MAX_VALUE, -Number.MAX_VALUE, Infinity, -Infinity, NaN, 2**53, 1e-300, 1e300];
const intervals = [];
function interval(duration, ordersCount) {
  const value = context.Instance.prototype.sliceIntervalMs.call({ options: { duration, ordersCount } });
  intervals.push({ duration: bits(duration), ordersCount: bits(ordersCount), value: bits(value) });
}
for (const duration of numbers) for (const count of numbers) interval(duration, count);
let seed = 0x5eed1234abcdef01n;
function next() { seed ^= seed << 13n; seed ^= seed >> 7n; seed ^= seed << 17n; seed &= (1n << 64n) - 1n; return number(seed); }
for (let i = 0; i < 2048; i++) interval(next(), next());
const remainders = [];
async function remainder(amount, side, reduceOnly, symbol, outcome) {
  const orders = [];
  let release;
  const pending = new Promise(resolve => { release = resolve; });
  const exchange = { async placeOrder(order) { orders.push({ ...order, amount: bits(order.amount) }); if (outcome === 'error') throw new Error('placement fixture'); if (outcome === 'pending') await pending; } };
  let settled = false;
  const completion = context.Instance.prototype.fireMarketRemainder.call({ exchange, options: { activeMarket: { symbol }, side, reduceOnly } }, amount).then(() => { settled = true; });
  await new Promise(resolve => setImmediate(resolve));
  const settledBeforeRelease = settled;
  release(); await completion;
  return { amount: bits(amount), side, reduceOnly, symbol, outcome, orders, settledBeforeRelease, settled };
}
for (const amount of numbers) for (const side of ['buy', 'sell']) for (const reduceOnly of [false, true]) for (const outcome of ['ok', 'error', 'pending']) {
  remainders.push(await remainder(amount, side, reduceOnly, ' BTC🚀\t', outcome));
}
const loneSurrogate = await remainder(1, 'buy', false, JSON.parse('"\\ud800"'), 'ok');
process.stdout.write(JSON.stringify({ intervals, remainders, loneSurrogate: { json: JSON.stringify(loneSurrogate.orders[0]), codeUnit: loneSurrogate.orders[0].symbol.charCodeAt(0) } }));
