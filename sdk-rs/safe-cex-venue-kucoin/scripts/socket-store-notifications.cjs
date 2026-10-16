// Fixed source path witness, not a complete socket constructor/lifecycle oracle.
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const {root} = require('./tables.cjs');
const {DefaultStore} = require('./spot-store-notifications.cjs');
function ast(file) {
  return ts.createSourceFile(file, fs.readFileSync(path.join(root, 'packages/safe-cex/src', file), 'utf8'), ts.ScriptTarget.Latest, true);
}
const source = ast('exchanges/base.ws.ts');
const heartbeat = source.statements.find(n => ts.isFunctionDeclaration(n) && n.name?.text === 'isHeartbeatFrame');
if (!heartbeat) throw Error('source heartbeat guard missing');
const context = vm.createContext({
  WebSocket: {OPEN: 1}, ReconnectingWebSocket: class {},
  lifecycleLog() {}, LogSeverity: {Info: 1, Warning: 2, Debug: 3},
  summarizeSocketDiagnostics: () => ({}), Date: {now: () => 1700000000000},
});
vm.runInContext(ts.transpileModule(heartbeat.getText(source).replace(/^export /, ''), {
  compilerOptions: {target: ts.ScriptTarget.ES2022},
}).outputText, context);
function instance(file, names) {
  const source = ast(file), found = [];
  for (const cls of source.statements.filter(ts.isClassDeclaration)) {
    for (const member of cls.members) if (names.includes(member.name?.getText(source))) found.push(member.getText(source));
  }
  if (found.length !== names.length) throw Error(`missing members in ${file}`);
  return vm.runInContext(ts.transpileModule(`new class { ${found.join('\n')} }`, {
    compilerOptions: {target: ts.ScriptTarget.ES2022},
  }).outputText, context);
}
(async () => {
  const rows = [];
  for (const type of ['subscribe', 'unsubscribe']) {
    const store = new DefaultStore(), trace = [], failure = Error('first subscriber refused');
    const first = store.subscribe(() => { trace.push('first'); throw failure; });
    const second = store.subscribe(() => trace.push('second'));
    const parent = instance('exchanges/base.ts', ['syncSocketHealthToStore', 'syncSocketHealthToStoreInternal']);
    Object.assign(parent, {store, startWatchdog() {}, getPublicSockets: () => [], getPrivateSockets: () => [], log() {}});
    const socket = instance('exchanges/base.ws.ts', ['send', 'reconnect', 'onClose', 'processClose']);
    Object.assign(socket, {parent, recordCloseDiagnostics() {}, registerCloseResult() {}, getCloseLogMessage() { throw Error('continued beyond failed health notification'); }});
    const venue = instance('exchanges/kucoin/kucoin.ws-public.ts', ['sendChannelOp']);
    venue.send = socket.send;
    const reconnect = socket.reconnect, tails = [];
    // Observe each source-unawaited rejection without changing send's return value.
    socket.reconnect = () => {
      const promise = reconnect();
      tails.push(promise.then(() => ({rejected:false, sameError:false}), error => ({rejected:true, sameError:error === failure})));
      return promise;
    };
    const returned = venue.sendChannelOp(type, ['/contractMarket/tickerV2'], ['XBTUSDTM']);
    const settled = await Promise.all(tails);
    const health = store.socketHealth;
    const count = tails.length;
    socket.send('{"type":"ping"}');
    rows.push({type, returnedUndefined: returned === undefined, settled, trace,
      publicHealthy: health.publicHealthy, privateHealthy: health.privateHealthy,
      heartbeatDidNotReconnect: tails.length === count});
    first(); second();
  }
  process.stdout.write(JSON.stringify(rows));
})().catch(error => { console.error(error); process.exitCode = 1; });
