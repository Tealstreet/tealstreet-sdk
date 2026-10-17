import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {ts, loadSource} from '../../safe-cex-store/tests/source_loader.mjs';
const url = new URL('../../../packages/safe-cex/src/exchanges/lighter/lighter.ws-public.ts', import.meta.url);
const ast = ts.createSourceFile(url.pathname, fs.readFileSync(url, 'utf8'), ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(ts.isClassDeclaration);
const member = cls.members.filter(m => m.name?.getText(ast) === 'onMessage');
assert.equal(member.length, 1);
const parser = loadSource(new URL('../../../packages/safe-cex/src/exchanges/lighter/lighter.ws-public-parsers.ts', import.meta.url));
const json = loadSource(new URL('../../../packages/json-parse/src/jsonParse.ts', import.meta.url));
const module = {exports: {}};
vm.runInNewContext(ts.transpileModule(`export class Selected {${member[0].getText(ast)}}`, {
  compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS},
}).outputText, {module, exports: module.exports, ...parser, ...json});
const results = [];
for (const failsClose of [false, true]) {
  const socket = new module.exports.Selected();
  socket.isDisposed = false;
  socket.hasReceivedMessageSinceConnect = true;
  socket.trackLastReceived = () => {};
  socket.recordMessageHandlerStats = () => {};
  const cause = {tag: 'callback'};
  let closes = 0;
  const handlers = new Set([() => { throw cause; }]).values();
  handlers.return = () => {
    closes++;
    if (failsClose) throw new Error('close');
    return {done: true};
  };
  socket.messageHandlers = {'order_book:1': handlers};
  assert.throws(() => socket.onMessage({data: '{"channel":"order_book:1"}'}), error => error === cause);
  assert.equal(closes, 1);
  results.push({failsClose, closes, original: true});
}
process.stdout.write(JSON.stringify(results));
