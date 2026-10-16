const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/base.ws.ts');
const tree = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const klass = tree.statements.find(node => ts.isClassDeclaration(node) && node.name?.text === 'BaseWebSocket');
const method = klass.members.find(node => node.name?.getText(tree) === 'createWebsocket');
assert.ok(method);
const compiled = ts.transpileModule(`class Slice { ${method.getText(tree)} } globalThis.Slice = Slice;`, {
  compilerOptions: { target: ts.ScriptTarget.ES2022 }, reportDiagnostics: true,
});
assert.equal(compiled.diagnostics.filter(d => d.category === ts.DiagnosticCategory.Error).length, 0);
const dials = [];
class Socket {
  constructor(url, protocols, options = {}) {
    const headers = typeof options.headers === 'function' ? options.headers() : options.headers;
    dials.push({url, protocols, headers, minReconnectionDelay: (() => { const bits = Buffer.alloc(8); bits.writeDoubleBE(options.minReconnectionDelay); return bits.toString('hex'); })()});
  }
  addEventListener() {}
}
class HeaderCapableWebSocketUnavailableError extends Error {
  constructor() { super(); this.name = 'HeaderCapableWebSocketUnavailableError'; }
}
let capable = true;
const scope = { getNodeWebSocket: () => capable ? Socket : undefined,
  getTransport: () => ({wsConnection: () => undefined}), RECONNECTING_WEBSOCKET_TUNING: {},
  ReconnectingWebSocket: Socket, WebSocket: Socket, ENABLE_RECONNECTING_WEBSOCKET: false,
  HeaderCapableWebSocketUnavailableError,
};
vm.runInNewContext(compiled.outputText, scope);
const socket = new scope.Slice(); socket.parent = {options: {}};
let count = 0;
const options = { headersRequired: true, headers: () => ({'ACCESS-TIMESTAMP': String(++count)}), minReconnectionDelay: 17 };
socket.createWebsocket('wss://synthetic.test/private', ['auth-v1'], options);
socket.createWebsocket('wss://synthetic.test/private', ['auth-v1'], options);
options.headers = () => { throw new Error('auth refused'); };
assert.throws(() => socket.createWebsocket('wss://synthetic.test/private', [], options), /auth refused/);
capable = false;
assert.throws(() => socket.createWebsocket('wss://synthetic.test/private', [], options), {name: 'HeaderCapableWebSocketUnavailableError'});
assert.equal(dials.length, 2);
process.stdout.write(JSON.stringify(dials));
