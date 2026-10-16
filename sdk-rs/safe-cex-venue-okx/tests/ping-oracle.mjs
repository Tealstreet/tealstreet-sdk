import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';
const url = new URL('../../../packages/safe-cex/src/exchanges/okx/okx.ws-public.ts', import.meta.url);
let source = fs.readFileSync(url, 'utf8');
if (process.argv.includes('--mutate-send-first')) {
  source = source.replace("this.pingAt = performance.now();\n      this.send('ping');", "this.send('ping');\n      this.pingAt = performance.now();");
}
const ast = ts.createSourceFile(url.pathname, source, ts.ScriptTarget.Latest, true);
const owner = ast.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'OKXPublicWebsocket');
const member = owner.members.find(n => n.name?.getText(ast) === 'ping');
assert.ok(member, 'maintained ping member');
const code = ts.transpileModule('export class Selected {' + member.getText(ast) + '}', {
  compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS},
}).outputText;
const module = {exports: {}};
const effects = [];
let time;
vm.runInThisContext('(function(module,exports,performance){' + code + '\n})')(module, module.exports, {
  now() { effects.push('clock'); return time; },
});
const bits = value => {
  const buffer = Buffer.alloc(8);
  buffer.writeDoubleBE(value);
  return buffer.toString('hex');
};
if (process.argv.includes('--closed-log-witness')) {
  const baseUrl = new URL('../../../packages/safe-cex/src/exchanges/base.ws.ts', import.meta.url);
  const baseText = fs.readFileSync(baseUrl, 'utf8');
  const baseAst = ts.createSourceFile(baseUrl.pathname, baseText, ts.ScriptTarget.Latest, true);
  const base = baseAst.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'BaseWebSocket');
  const send = base.members.find(n => n.name?.getText(baseAst) === 'send');
  const heartbeat = baseAst.statements.find(n => ts.isFunctionDeclaration(n) && n.name.text === 'isHeartbeatFrame');
  const exchangeUrl = new URL('../../../packages/safe-cex/src/exchanges/base.ts', import.meta.url);
  const exchangeText = fs.readFileSync(exchangeUrl, 'utf8');
  const exchangeAst = ts.createSourceFile(exchangeUrl.pathname, exchangeText, ts.ScriptTarget.Latest, true);
  const exchange = exchangeAst.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'BaseExchange');
  const log = exchange.members.find(n => n.name?.getText(exchangeAst) === 'log');
  assert.ok(send && heartbeat && log, 'maintained heartbeat logging path');
  const selected = ts.transpileModule(heartbeat.getText(baseAst) + '\nexport class Sender {' + send.getText(baseAst) + '}\nexport class Parent {' + log.getText(exchangeAst) + '}', {
    compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS},
  }).outputText;
  const compiled = {exports: {}};
  vm.runInThisContext('(function(module,exports,WebSocket,LogSeverity){' + selected + '\n})')(compiled, compiled.exports, {OPEN: 1}, {Debug:'debug', Warning:'warning', Info:'info'});
  const receiver = new module.exports.Selected();
  const sender = new compiled.exports.Sender();
  const parent = new compiled.exports.Parent();
  parent.options = {tuning: {consoleLogging: false}};
  parent.name = 'okx';
  parent.emitter = {emit() { effects.push('log-listener'); throw new TypeError('log listener failure'); }};
  sender.parent = parent;
  receiver.send = sender.send;
  time = 123;
  let error;
  try { receiver.ping(); } catch (e) { error = {name: e.name, message: e.message}; }
  assert.deepEqual(effects, ['clock', 'log-listener'], 'source skipped heartbeat listener order');
  assert.deepEqual(error, {name:'TypeError', message:'log listener failure'}, 'source skipped heartbeat listener error');
  process.stdout.write(JSON.stringify({effects, error, pingAt: receiver.pingAt}));
  process.exit(0);
}
const cases = [];
for (time of [0, -0, 1.25, 123, 2000.5]) {
  for (const disposed of [false, true]) {
    for (const fail of [false, true]) {
      effects.length = 0;
      const receiver = new module.exports.Selected();
      receiver.isDisposed = disposed;
      receiver.pingAt = 777;
      receiver.send = message => {
        effects.push('send:' + message);
        if (fail) throw new Error('send failure');
      };
      let error = null;
      try { receiver.ping(); } catch (e) { error = {name: e.name, message: e.message}; }
      assert.deepEqual(effects, disposed ? [] : ['clock', 'send:ping'], 'ping clock before send effects');
      assert.equal(bits(receiver.pingAt), bits(disposed ? 777 : time), 'ping timestamp retained after send failure');
      cases.push({capture: {architecture: process.platform + " " + process.arch, nodeMajor: process.versions.node.split(".")[0]}, timeBits: bits(time), disposed, fail, effects: [...effects], error});
    }
  }
}
process.stdout.write(JSON.stringify(cases));
