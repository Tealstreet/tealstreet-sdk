import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {ts, root, load} from './source.mjs';
const file = root + 'packages/safe-cex/src/exchanges/mexc/mexc.ws-public.ts';
const source = fs.readFileSync(file, 'utf8');
const ast = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
const klass = ast.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'MexcPublicWebsocket');
const names = ['addHandler', 'removeHandler', 'handleRoutedMessage', 'onMessage'];
const members = names.map(name => {
  const n = klass.members.find(n => n.name?.getText(ast) === name);
  assert(n, `tracked source member ${name}`);
  return n.getText(ast);
});
const code = ts.transpileModule('class Holder {' + members.join('\n') + '} globalThis.Holder = Holder;',
  {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const context = {bigIntAsStringJsonParse:JSON.parse,
  fromMexcSymbol:load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts').fromMexcSymbol,
  isMexcControlChannel:() => false};
vm.runInNewContext(code, context);
const expected = [
  ['add', ['A','B','A','B','C'], 2, 3, 2, 0],
  ['remove', ['A','B','A'], 2, 1, 2, 0],
  ['recursive', ['A','A','B','C','B'], 2, 3, 2, 0],
  ['throw', ['A'], 2, 3, 1, 0],
  ['duplicates', ['A','C'], 4, 2, 1, 0],
  ['missing', [], null, 0, 1, 1],
  ['remove-all', [], 1, 0, 1, 2],
];
const output = [];
let calls = 0, routes = 0;
for (const [name, wanted, oldLength, currentLength, routeCount, cleanupCount] of expected) {
  const holder = new context.Holder();
  holder.messageHandlers = {};
  holder.isDisposed = false;
  let received = 0, cleanup = 0, changed = false;
  const log = [];
  const error = Object.assign(new Error('handler failure'), {code:'HANDLER_ORIGIN'});
  holder.trackLastReceived = () => { received++; };
  holder.emitReceiveDiagnostics = () => {};
  holder.recordMessageHandlerStats = () => {};
  holder.cleanupMessageHandlerStats = () => { cleanup++; };
  const route = () => holder.onMessage({data:JSON.stringify({channel:'push.deal',symbol:'BTC_USDT'})});
  const b = () => log.push('B'), c = () => log.push('C');
  const a = () => {
    log.push('A');
    if (!changed && ['add','remove','recursive','throw'].includes(name)) {
      changed = true;
      if (name === 'remove') holder.removeHandler('trade.BTCUSDT', b);
      else holder.addHandler('trade.BTCUSDT', c);
      if (name === 'recursive') route();
      if (name === 'throw') throw error;
    }
  };
  if (name !== 'missing') holder.addHandler('trade.BTCUSDT', a);
  if (['add','remove','recursive','throw','duplicates'].includes(name)) holder.addHandler('trade.BTCUSDT', b);
  if (name === 'duplicates') {holder.addHandler('trade.BTCUSDT', b);holder.addHandler('trade.BTCUSDT', c);}
  const old = holder.messageHandlers['trade.BTCUSDT'];
  if (name === 'duplicates') holder.removeHandler('trade.BTCUSDT', b);
  if (name === 'missing') holder.removeHandler('trade.BTCUSDT', b);
  if (name === 'remove-all') {holder.removeHandler('trade.BTCUSDT', a);holder.removeHandler('trade.BTCUSDT', a);}
  let caught;
  try {route();} catch (e) {caught = e;}
  if (['add','remove'].includes(name)) route();
  // Native identities, callbacks and own properties precede the JSON supplement.
  assert.deepEqual(log, wanted, `SOURCE captured handler callbacks ${name}`);
  assert.equal(old?.length ?? null, oldLength, `SOURCE retained old array ${name}`);
  assert.notEqual(holder.messageHandlers['trade.BTCUSDT'], old, `SOURCE array replacement ${name}`);
  assert.equal(holder.messageHandlers['trade.BTCUSDT'].length, currentLength, `SOURCE current array ${name}`);
  assert(Object.hasOwn(holder.messageHandlers, 'trade.BTCUSDT'), `SOURCE own registry key ${name}`);
  assert.equal(received, routeCount, `SOURCE actual route calls ${name}`);
  assert.equal(cleanup, cleanupCount, `SOURCE cleanup calls ${name}`);
  assert.equal(caught, name === 'throw' ? error : undefined, `SOURCE same thrown handler error ${name}`);
  calls += log.length; routes += received;
  output.push({name, log, oldLength, currentLength, routes:received, cleanup, errorCode:caught?.code ?? null});
}
// Retain the inherited-key difference as a finding rather than projecting it away.
for (const name of ['addHandler','removeHandler']) {
  const holder = new context.Holder(); holder.messageHandlers = {};
  assert.throws(() => holder[name]('constructor', () => {}), e => e.name === 'TypeError', `SOURCE inherited constructor ${name}`);
}
assert.equal(calls, 16, 'SOURCE independent callback-call floor');
assert.equal(routes, 10, 'SOURCE independent route-call floor');
console.log(JSON.stringify({cases:output, calls, routes, inheritedConstructorErrors:2}));
