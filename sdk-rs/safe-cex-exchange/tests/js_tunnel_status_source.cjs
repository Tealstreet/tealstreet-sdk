// Execute maintained HOW adapter and installed Axios; script only socket I/O.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const axios = require('axios');
const filename = path.resolve(__dirname, '../../../packages/http-over-websocket/src/axiosHttpOverWebsocketAdapter.ts');
let status = 200;
let sequence = 0;
class SocketIO {
  isInitialized = true;
  isOpen = true;
  isTimeSynced = true;
  authState = 'Good';
  listeners = new Map();
  once(key, callback) { this.listeners.set(key, callback); }
  off(key) { this.listeners.delete(key); }
  send(frame) {
    assert.equal(frame.type, 'rpc.http');
    const key = 'sub:' + frame.nonce;
    const callback = this.listeners.get(key);
    this.listeners.delete(key);
    callback({status, data: {code: '-1102', msg: 'venue response'}, headers: {}});
  }
}
const imports = {
  axios,
  nanoid: {nanoid: () => String(++sequence)},
  '@tealstreet/compression/src/decompress': {decompress: () => { throw Error('unexpected compression'); }},
  '@tealstreet/sleep/src/sleep': {sleep: () => { throw Error('unexpected wait'); }},
  '@tealstreet/json-parse/src/jsonParse': {bigIntAsStringJsonParse: JSON.parse},
  './config': {getProxyToken: () => { throw Error('already authenticated'); }, isTokenExpired: () => false, shouldSimulateHttpOverWebsocket: () => false},
  './tunnelEndpoints': {SINGAPORE_HOW_EGRESS: 'sg', TOKYO_HOW_EGRESS: 'jp'},
  './egressRegion': {isTokyoEgress: () => false},
  './HOWWebSocket': {HOWWebSocket: SocketIO},
  './types': {HOWWebSocketAuthState: {Good: 'Good', Bad: 'Bad', Unknown: 'Unknown'}},
};
const compiled = ts.transpileModule(fs.readFileSync(filename, 'utf8'), {compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS, esModuleInterop: true}}).outputText;
const moduleValue = {exports: {}};
new Function('module', 'exports', 'require', compiled)(moduleValue, moduleValue.exports, name => {
  assert.ok(Object.hasOwn(imports, name), 'unexpected source import ' + name);
  return imports[name];
});
(async () => {
  const rows = [];
  for (status of [200, 400, 500, 0]) {
    const events = [];
    const xhr = axios.create({adapter: moduleValue.exports.axiosHttpOverWebsocketAdapter, baseURL: 'https://venue.invalid', timeout: 1000});
    xhr.interceptors.response.use(response => { events.push('fulfilled'); return response; }, error => { events.push('rejected'); throw error; });
    const response = await xhr.post('/order', {}, {marker: 'request context'});
    rows.push({status: response.status, events, marker: response.config.marker, data: response.data});
  }
  assert.equal(rows.length, 4);
  process.stdout.write(JSON.stringify(rows));
})().catch(error => { console.error(error); process.exitCode = 1; });
