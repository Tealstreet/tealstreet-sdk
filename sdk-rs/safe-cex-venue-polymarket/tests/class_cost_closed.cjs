const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/polymarket/polymarket.exchange.ts');
const source = fs.readFileSync(file, 'utf8');
const tree = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
const klass = tree.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'PolymarketExchange');
const getter = klass.members.filter(n => ts.isGetAccessor(n) && n.name.text === 'name');
assert.equal(getter.length, 1, 'closed actual getter selection');
const code = ts.transpileModule(`class Selected { ${getter[0].getText(tree)} }; exports.getter=Object.getOwnPropertyDescriptor(Selected.prototype,'name').get;`, {compilerOptions:{target:99,module:1}}).outputText;
const context = {exports:{}}; vm.runInNewContext(code, context);
const cases = [];
for (let index=0; index<512; index++) {
  const receivers = [null, undefined, index, -index, `receiver-${index}`, [index], {index}, true];
  const result = Reflect.apply(context.exports.getter, receivers[index%8], [index]);
  assert.equal(typeof result, 'string', 'closed source native string');
  cases.push({index, units:Array.from({length:result.length},(_,i)=>result.charCodeAt(i))});
}
process.stdout.write(JSON.stringify({cases}));
