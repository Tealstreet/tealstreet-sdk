import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {load, root, ts} from './source.mjs';

const filename = 'packages/safe-cex/src/exchanges/mexc/mexc.ws-private.ts';
const ast = ts.createSourceFile(filename, fs.readFileSync(`${root}/${filename}`, 'utf8'), ts.ScriptTarget.Latest, true);
const owner = ast.statements.find(node => node.name?.text === 'MexcPrivateWebsocket');
const members = ['onMessage', 'emitReceiveDiagnostics'].map(name => {
  const member = owner.members.find(node => node.name?.getText(ast) === name);
  assert(member?.initializer, `maintained ${name} initializer`);
  return `${name} = ${member.initializer.getText(ast)};`;
});
const module = {exports: {}};
const code = ts.transpileModule(`class Receiver {
  isDisposed = false;
  constructor() { this.messages = []; this.errors = []; this.parent = {emitAndLogError: value => this.errors.push(value)}; }
  trackLastReceived() {}
  emitWsDiagnosticReceive(value, options) { this.messages.push(options.format(value)); }
  ${members.join('\n')}
} exports.Receiver = Receiver;`, {compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS}}).outputText;
vm.runInNewContext(code, {exports: module.exports,
  bigIntAsStringJsonParse: load('packages/json-parse/src/jsonParse.ts').bigIntAsStringJsonParse});
const cases = [
  ['1.0', '1'], ['-0.0', '0'], ['1e3', '1000'], ['0.000001', '0.000001'],
  ['1e-7', '1e-7'], ['1e+16', '10000000000000000'], ['1e400', 'null'],
  ['[-0.0,1.0,1e-7]', '[0,1,1e-7]'],
  ['{"2":2.0,"1":1.0,"x":-0.0}', '{"1":1,"2":2,"x":0}'],
  ['12345678901234567', '"12345678901234567"'],
];
let compared = 0;
for (const [input, expected] of cases) {
  const receiver = new module.exports.Receiver();
  receiver.onMessage({data: `{"channel":"rs.login","data":"failure","value":${input}}`});
  assert.equal(receiver.messages.length, 2);
  assert.equal(receiver.messages[1], `[mexc private] [mexc login] failed {"channel":"rs.login","data":"failure","value":${expected}}`);
  assert.equal(receiver.errors[0], 'MEXC private ws login failed, double check your key and secret');
  compared++;
}
assert.equal(compared, 10);
console.log('MEXC-JSON actual source: 10 diagnostic profiles');
