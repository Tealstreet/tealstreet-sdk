// Source-path witness for the Store subscriber error finding; run directly with Node.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';
import chunk from 'lodash/chunk.js';
import flatten from 'lodash/flatten.js';
const root = new URL('../../../', import.meta.url);
function members(path, names) {
  const source = ts.createSourceFile(path, readFileSync(new URL(path, root), 'utf8'), ts.ScriptTarget.Latest, true);
  const found = [];
  function visit(n) {
    if (ts.isClassDeclaration(n)) for (const m of n.members) {
      if (names.includes(m.name?.getText(source))) found.push(m.getText(source));
    }
    ts.forEachChild(n, visit);
  }
  visit(source);
  assert.equal(found.length, names.length, `source members: ${path}`);
  return found.join('\n');
}
const context = vm.createContext({
  WebSocket: { OPEN: 1 }, ReconnectingWebSocket: class {},
  lifecycleLog() {}, LogSeverity: { Info: 1, Warning: 2, Debug: 3 },
  isHeartbeatFrame: (s) => JSON.parse(s).op === 'ping',
  getCloseCodeDescription: () => 'Unknown',
  summarizeSocketDiagnostics: () => ({}),
});
function instance(path, names) {
  return vm.runInContext(ts.transpileModule(`new class { ${members(path, names)} }`, {
    compilerOptions: { target: ts.ScriptTarget.ES2022 },
  }).outputText, context);
}
const store = instance('packages/safe-cex/src/store/store.base.ts', ['updateSocketHealth', 'notify']);
const parent = instance('packages/safe-cex/src/exchanges/base.ts', ['syncSocketHealthToStore', 'syncSocketHealthToStoreInternal']);
const socket = instance('packages/safe-cex/src/exchanges/base.ws.ts', ['send', 'reconnect', 'onClose', 'processClose']);
const seen = [], failure = new Error('first subscriber refused');
store.state = { socketHealth: {} };
store.listeners = new Set([
  () => { seen.push('first'); throw failure; },
  () => { seen.push('second'); },
]);
Object.assign(parent, { store, startWatchdog() {}, getPublicSockets: () => [], getPrivateSockets: () => [], log() {} });
Object.assign(socket, { parent, recordCloseDiagnostics() {}, registerCloseResult() {} });
// Observe the actual async reconnect promise without changing send's unawaited contract.
const reconnect = socket.reconnect;
let tail;
socket.reconnect = () => (tail = reconnect());
socket.send('{"op":"subscribe","args":["order"]}');
assert.equal(store.state.socketHealth.publicHealthy, true, 'state commits before subscriber throw');
assert.deepEqual(seen, ['first'], 'throw skips second subscriber');
await assert.rejects(tail, (error) => error === failure, 'unawaited reconnect retains subscriber rejection');
// Heartbeat guard must not enter the notification path at all.
tail = undefined;
socket.send('{"op":"ping"}');
assert.equal(tail, undefined, 'closed-socket ping does not reconnect');
assert.deepEqual(seen, ['first'], 'closed-socket ping does not notify');
console.log('Store notification: committed state, skipped subscriber, rejected reconnect, heartbeat guard PASS');

// Observe rejection separately: the source caller never awaits these tails.
const delays = [], diagnostics = [], tails = [];
context.chunk = chunk;
context.flatten = flatten;
context.sleep = async (ms) => { delays.push(ms); };
socket.reconnect = () => {
  const promise = reconnect();
  tails.push(promise.then(() => assert.fail('reconnect must reject'), error => error));
  return promise;
};
socket.emitWsDiagnosticSend = message => diagnostics.push(JSON.parse(message));
const withLog = instance('packages/safe-cex/src/exchanges/base.ws.ts', ['sendWithLog']);
Object.assign(withLog, { send: socket.send, emitWsDiagnosticSend: socket.emitWsDiagnosticSend });
const publicCaller = instance('packages/safe-cex/src/exchanges/bybit/bybit.ws-public.ts', ['subscribe']);
Object.assign(publicCaller, {
  topics: { tickers: Array.from({ length: 11 }, (_, i) => `tickers.${i}`) },
  send: socket.send,
});
seen.length = 0;
assert.equal(await publicCaller.subscribe(), undefined, 'public caller fulfills despite reconnect rejection');
assert.deepEqual(delays, [10, 10], 'public caller reaches both subscription delays');
assert.equal(tails.length, 2, 'public caller attempts both chunks');
assert.deepEqual(seen, ['first', 'first'], 'both chunks commit and notify before rejecting their tails');
assert.ok((await Promise.all(tails)).every(error => error === failure), 'both rejected tails retain original error');

const privateCaller = instance('packages/safe-cex/src/exchanges/bybit/bybit.ws-private.ts', ['subscribe']);
privateCaller.sendWithLog = withLog.sendWithLog;
assert.equal(privateCaller.subscribe(), undefined, 'private caller returns normally');
assert.deepEqual(diagnostics[0], { op: 'subscribe', args: ['order', 'position'] }, 'private diagnostic follows failed reconnect');
const diagnosticCaller = instance('packages/safe-cex/src/exchanges/bybit/bybit.ws-public.ts', ['sendOhlcvDiagnosticPayload']);
Object.assign(diagnosticCaller, { send: socket.send, emitWsDiagnosticSend: socket.emitWsDiagnosticSend });
assert.equal(diagnosticCaller.sendOhlcvDiagnosticPayload({ op: 'unsubscribe', args: ['kline.1.BTCUSDT'] }), undefined, 'diagnostic caller returns normally');
assert.deepEqual(diagnostics[1], { op: 'unsubscribe', args: ['kline.1.BTCUSDT'] }, 'diagnostic event follows failed reconnect');
assert.equal(tails.length, 4, 'all covered non-heartbeat sends reach reconnect');
assert.ok((await Promise.all(tails)).every(error => error === failure), 'all source tail failures retain identity');
console.log('Reconnect timing: public two chunks and delays fulfill; private and diagnostic callers return and emit; four tails reject PASS');
