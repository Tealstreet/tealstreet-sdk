import fs from 'node:fs';
import vm from 'node:vm';
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
if (!process.env.SAFE_CEX_TYPESCRIPT_PATH) throw Error('isolated TypeScript compiler required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = new URL('../../../', import.meta.url);
function statements(path, select) {
  const text = fs.readFileSync(new URL(path, root), 'utf8');
  const ast = ts.createSourceFile(path, text, ts.ScriptTarget.Latest, true);
  const found = [];
  function visit(node) {
    if (select(node, node.getText(ast))) found.push(node.getText(ast));
    else ts.forEachChild(node, visit);
  }
  visit(ast);
  return found.join('\n');
}
const assignment = statements('packages/safe-cex/src/exchanges/kucoin/kucoinspot.exchange.ts',
  (n, text) => ts.isExpressionStatement(n) && text.startsWith('currencies[currency] ='));
const nested = statements('packages/safe-cex/src/exchanges/grvt/grvt.exchange.ts',
  (n, text) => (ts.isIfStatement(n) && text.startsWith('if (!currencies[currency])')) ||
    (ts.isExpressionStatement(n) && /^currencies\[currency\]\.(free|total) \+=/.test(text)));
if (!assignment || !nested.includes('.total += balance')) throw Error('production dictionary write capture missing');
const rows = [];
for (const key of [...Object.getOwnPropertyNames(Object.prototype), 'USDT', '2', '01']) {
  for (const own of [false, true]) for (const mode of ['assign', 'nested']) {
    // Each VM has its own Object.prototype; source effects cannot leak between cases.
    const context = vm.createContext({ currency: key, own });
    const source = `const currencies = {}; const initialPrototype = Object.getPrototypeOf(currencies);
      if (own) Object.defineProperty(currencies, currency, {value:{free:3,total:3},writable:true,enumerable:true,configurable:true});
      const available=2, holds=0, balance=2, account={currency};
      ${mode === 'assign' ? assignment : nested}
      const referent=currencies[currency];
      globalThis.result={key:currency, own:own, mode:'${mode}', keys:Object.keys(currencies),
        serialized:JSON.stringify(currencies), prototypeSame:Object.getPrototypeOf(currencies)===initialPrototype,
        globalFree:Object.hasOwn(Object.prototype,'free'), freshFreeIsNaN:Number.isNaN(({}).free),
        referentFreeIsNaN:Number.isNaN(referent.free), value: Object.hasOwn(currencies,currency) ? referent : null};`;
    vm.runInContext(ts.transpileModule(source, {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
    rows.push(context.result);
  }
}
const replacement = rows.find(r=>r.key==='__proto__'&&!r.own&&r.mode==='assign');
const mutation = rows.find(r=>r.key==='__proto__'&&!r.own&&r.mode==='nested');
if (replacement.keys.length || replacement.prototypeSame || replacement.globalFree) throw Error('whole assignment must replace one map prototype only');
if (mutation.keys.length || !mutation.prototypeSame || !mutation.globalFree || !mutation.freshFreeIsNaN) throw Error('nested mutation must reach unchanged global prototype referent');
process.stdout.write(JSON.stringify(rows));
