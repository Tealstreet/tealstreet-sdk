import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';
const url = new URL('../../../packages/safe-cex/src/exchanges/okx/okx.ws-private.ts', import.meta.url);
const ast = ts.createSourceFile(url.pathname, fs.readFileSync(url, 'utf8'), ts.ScriptTarget.Latest, true);
const owner = ast.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'OKXPrivateWebsocket');
const selected = ['parseFinite', 'sumFinite'].map(name => {
  const member = owner.members.find(n => n.name?.getText(ast) === name);
  assert.ok(member, 'maintained ' + name);
  return member.getText(ast);
});
const code = ts.transpileModule('export class Selected {' + selected.join('\n') + '}', {
  compilerOptions: {target:ts.ScriptTarget.ES2022, module:ts.ModuleKind.CommonJS},
}).outputText;
const module = {exports:{}};
vm.runInThisContext('(function(module,exports){' + code + '\n})')(module, module.exports);
const receiver = new module.exports.Selected();
if (process.argv.includes('--runtime-witness')) {
  const object = {toString(){return '7.5';}};
  const parsed = receiver.parseFinite(object);
  const summed = receiver.sumFinite([object]);
  assert.equal(parsed,7.5,'source callable String coercion');
  assert.equal(summed,7.5,'source callable sum coercion');
  process.stdout.write(JSON.stringify({parsed,summed,jsonView:JSON.parse(JSON.stringify(object))}));
  process.exit(0);
}
const cases = JSON.parse(fs.readFileSync(0,'utf8'));
assert.equal(cases.length, 35, 'declared finite helper profile floor');
const captures = cases.map(c => {
  try {
    const value = c.method === 'parse' ? receiver.parseFinite(c.value) : receiver.sumFinite(c.values);
    const buffer = Buffer.alloc(8);buffer.writeDoubleBE(value);
    return {bits:buffer.toString('hex')};
  } catch(error) {return {error:{name:error.name, message:error.message}};}
});
process.stdout.write(JSON.stringify(captures));
