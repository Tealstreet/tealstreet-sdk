const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const rows = [];
for (const kind of ['Public', 'Private']) {
  const filename = path.resolve(__dirname, `../../../packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.ws-${kind.toLowerCase()}.ts`);
  const source = ts.createSourceFile(filename, fs.readFileSync(filename, 'utf8'), ts.ScriptTarget.Latest, true);
  const name = `Hyperliquid${kind}WebSocket`;
  const owner = source.statements.find(node => ts.isClassDeclaration(node) && node.name?.text === name);
  assert(owner, name);
  const timers = [];
  let fired = 0;
  const context = { exports: {}, BaseWebSocket: class {}, Map, Set,
    setTimeout(callback, delay) {
      const handle = setTimeout(() => { fired++; callback(); }, delay);
      timers.push({handle, delay});
      return handle;
    }, clearTimeout };
  const code = ts.transpileModule(`${owner.getText(source)}; exports.C = ${name};`, {
    compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS},
  }).outputText;
  vm.runInNewContext(code, context);
  const socket = new context.exports.C({});
  for (let i = 0; i < 2; i++) {
    socket.startPingLoop();
    assert.equal(timers.length, i + 1);
    assert.equal(timers[i].delay, 15000);
    assert.equal(timers[i].handle._destroyed, false);
    socket.stopPingLoop();
    assert.equal(socket.hlPingTimeoutId, undefined);
    assert.equal(timers[i].handle._destroyed, true, 'maintained clearTimeout destroys the actual Node timer');
  }
  assert.equal(fired, 0);
  rows.push({kind, timers: timers.length, canceled: timers.every(timer => timer.handle._destroyed), fired});
}
process.stdout.write(JSON.stringify(rows));
