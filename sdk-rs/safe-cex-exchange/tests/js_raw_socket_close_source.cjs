// Execute real no-super Crypto socket disposal and base forceClose members.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
function member(file, name) {
  const full = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges', file);
  const tree = ts.createSourceFile(full, fs.readFileSync(full, 'utf8'), ts.ScriptTarget.Latest, true);
  assert.equal(tree.parseDiagnostics.length, 0);
  const matches = tree.statements.filter(ts.isClassDeclaration).flatMap(cls => cls.members.filter(n => n.name?.getText(tree) === name));
  assert.equal(matches.length, 1, `${file}:${name}`);
  const result = matches[0];
  assert.ok(result, `${file}:${name}`);
  return result.getText(tree);
}
const force = member('base.ws.ts', 'forceClose');
const rows = [];
for (const side of ['public', 'private']) {
  const dispose = member(`cryptocom/cryptocom.ws-${side}.ts`, 'dispose');
  for (const operation of ['dispose', 'forceClose']) {
    for (const initial of [false, true]) {
      for (const present of [false, true]) {
        const scope = {clearTimeout() {}};
        const compiled = ts.transpileModule(`class Socket {${dispose}; ${force}} globalThis.Socket=Socket;`, {compilerOptions:{target:ts.ScriptTarget.ES2022}});
        vm.runInNewContext(compiled.outputText, scope);
        const socket = new scope.Socket();
        socket.forceClosed = initial;
        socket.subscriptions = new Map([['synthetic', 1]]);
        socket.marketIdsByIndexInstrument = new Map([['synthetic', 2]]);
        let closes = 0;
        socket.ws = present ? {close() {closes++;}} : undefined;
        socket[operation]();
        socket[operation]();
        rows.push({side,operation,initial,present,closes,forceClosed:socket.forceClosed});
      }
    }
  }
}
process.stdout.write(JSON.stringify(rows));
