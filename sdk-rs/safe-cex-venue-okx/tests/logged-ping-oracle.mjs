import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';

function members(file, ownerName, names) {
  const url = new URL('../../../packages/safe-cex/src/exchanges/' + file, import.meta.url);
  const ast = ts.createSourceFile(url.pathname, fs.readFileSync(url, 'utf8'), ts.ScriptTarget.Latest, true);
  const owner = ast.statements.find(n => ts.isClassDeclaration(n) && n.name.text === ownerName);
  assert.ok(owner, ownerName);
  return names.map(name => {
    const member = owner.members.find(n => n.name?.getText(ast) === name);
    assert.ok(member, ownerName + '.' + name);
    return member.getText(ast);
  }).join('\n');
}
const base = members('base.ws.ts', 'BaseWebSocket', ['sendWithLog']);
const bits = value => {
  const buffer = Buffer.alloc(8);
  buffer.writeDoubleBE(value);
  return buffer.toString('hex');
};
const rows = [];
for (const [role, owner] of [['private', 'OKXPrivateWebsocket'], ['business', 'OKXBusinessWebsocket']]) {
  const effects = [];
  let time;
  const compiled = ts.transpileModule('export class Selected {' + base + '\n' +
    members('okx/okx.ws-' + role + '.ts', owner, ['ping']) + '}', {
    compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS},
  }).outputText;
  const module = {exports:{}};
  vm.runInThisContext('(function(module,exports,performance){' + compiled + '\n})')(
    module, module.exports, {now() { effects.push('clock'); return time; }});
  for (time of [0, -0, 1.25, 123, 2000.5]) {
    for (const disposed of [false, true]) {
      for (const outcome of ['success', 'send-error', 'closed-log-error']) {
        effects.length = 0;
        const receiver = new module.exports.Selected();
        receiver.isDisposed = disposed;
        receiver.pingAt = 777;
        receiver.send = data => {
          effects.push(outcome === 'closed-log-error' ? 'log-listener' : 'send:' + data);
          if (outcome === 'send-error') throw new Error('send failure');
          if (outcome === 'closed-log-error') throw new TypeError('log listener failure');
        };
        receiver.emitWsDiagnosticSend = data => assert.equal(data, 'ping', 'heartbeat diagnostic argument');
        let error = null;
        try { receiver.ping(); } catch (e) { error = {name:e.name, message:e.message}; }
        assert.deepEqual(effects, disposed ? [] : ['clock', outcome === 'closed-log-error' ? 'log-listener' : 'send:ping']);
        assert.equal(bits(receiver.pingAt), bits(disposed ? 777 : time));
        rows.push({role, timeBits:bits(time), disposed, outcome, effects:[...effects], error});
      }
    }
  }
}
assert.equal(rows.length, 60, 'two roles times five clocks times two disposal states times three outcomes');
process.stdout.write(JSON.stringify(rows));
