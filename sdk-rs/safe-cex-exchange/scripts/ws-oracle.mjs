// Execute the real TypeScript class/helpers with deterministic host boundaries.
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../..');
const require = createRequire(path.join(root, 'package.json'));
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
let now = 1_000_000;
let nextTimer = 0;
const timers = new Map();
const schedule = (callback, delay) => { const id = ++nextTimer; timers.set(id, {callback, at: now + delay}); return id; };
const advance = delay => {
  now += delay;
  for (const [id, timer] of [...timers]) {
    if (timer.at <= now && timers.delete(id)) timer.callback();
  }
};
const config = {
  ENABLE_RECONNECTING_WEBSOCKET: false, ENABLE_WS_LIFECYCLE_DEBUG_LOGS: false,
  WS_BACKOFF_BASE_DELAY: 2000, WS_BACKOFF_FACTOR: 2, WS_BACKOFF_MAX_DELAY: 30000,
  WS_FAILURE_STREAK_PAUSE_MS: 120000, WS_FAILURE_STREAK_PAUSE_THRESHOLD: 8,
  WS_MIN_RECONNECTION_DELAY: 1000, RECONNECTING_WEBSOCKET_TUNING: {},
};
const load = (relative, imports = {}, random = 0.5) => {
  let source = fs.readFileSync(path.join(root, relative), 'utf8');
  if (relative.endsWith('base.ws.ts') && process.env.SAFE_CEX_WS_ORACLE_NEGATIVE_CONTROL === 'heartbeat') {
    const original = 'if (/^(ping|pong)$/i.test(trimmed)) return true;';
    if (!source.includes(original)) throw new Error('negative-control source target vanished');
    source = source.replace(original, 'if (/^(ping|pong)$/i.test(trimmed)) return false;');
  }
  const js = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText.replaceAll('import.meta.url', JSON.stringify('file:///ws-oracle.ts'));
  const exports = {};
  vm.runInNewContext(js, { process: {env:{}}, Math: Object.assign(Object.create(Math), {random: () => random}), exports, require: (name) => {
    if (!(name in imports)) throw new Error(`unmocked import ${name}`);
    return imports[name];
  }, Date: class extends Date { static now() { return now; } }, performance: { now: () => now },
  setTimeout: schedule, clearTimeout: id => timers.delete(id), console: { log() {}, error() {} } });
  return exports;
};
const diagnostics = load('packages/safe-cex/src/utils/wsDiagnostics.ts');
class Rws { constructor(url, protocols, options) { this._options = options; } addEventListener() {} }
class RingBuffer { constructor(size) { this.size=size; this.items=[]; } add(item) { this.items.push(item); if(this.items.length>this.size)this.items.shift(); } }
const ws = load('packages/safe-cex/src/exchanges/base.ws.ts', {
  '@tealstreet/data-structures/src/RingBuffer': { RingBuffer },
  '@tealstreet/http-over-websocket/src/RelayWebSocket': { RelayWebSocket: class {} },
  '../transport/registry': { getTransport: () => ({wsConnection: () => undefined}) },
  '@tealstreet/reconnecting-websocket/src/ReconnectingWebsocket': { ReconnectingWebSocket: Rws },
  '@tealstreet/sleep/src/sleep': { sleep: () => Promise.resolve() },
  '../config/config': config, '../types': { LogSeverity: {Warning:'warning',Error:'error',Info:'info',Debug:'debug'} },
  '../utils/wsDiagnostics': diagnostics,
});
const heartbeats = ['ping',' PONG ','\ufeffping\ufeff','\u0085ping\u0085','{"op":"server.ping"}','{"ping":null}','{"symbol":"SPONGE"}','{"op":"subscribe","id":"ping-order"}','["ping"]','{"method":"SERVER.PONG"}','{"event":" server.ping "}','null'];
const closeCodes = [1000,1001,1002,1003,1005,1006,1007,1008,1009,1010,1011,1012,1013,1014,1015,4000];
const redactions = [
 '{"token":"secret","op":"login","args":["sensitive",{"notSecret":"still-sensitive"}]}',
 '{"z":1.0,"2":"second","1":"first","a":-0,"tiny":1e-7,"huge":1e22}',
 '{"value":1e400}',
 '?token=secret&foo=bar apiKey=secret',
 'é0123456789abcdef0123456789abcdef0123456789abcdefé',
 '{broken "key":"secret"}',
];
for (const key of ['apiKey','api_key','key','secret','passphrase','sign','signature','wsAuthSignature','jwt','token','sessionToken','session','authKey','listenKey','password']) {
  for (const space of [' ', '\ufeff', '\u00a0']) {
    redactions.push(`{broken "${key.toUpperCase()}"${space}:${space}"escaped\\\"secret"}`);
    redactions.push(`?${key}=first&other=keep ${key.toUpperCase()}=second`);
  }
}
for (const wrapper of ['', '_', 'G', 'é', ' ', 'x']) {
  redactions.push(`${wrapper}0123456789abcdef0123456789abcdef0123456789abcdef${wrapper}`);
}
const parent = { name:'venue', options:{key:'k'}, isDisposed:false, log() {}, emitter:{emit() {}}, store:{update() {}}, getPrivateSockets:()=>[], syncSocketHealthToStore() {} };
const socket = new ws.BaseWebSocket(parent);
socket.messageHandlersStats.partial = {lastMessageTimestamp:1};
socket.messageHandlers['book.BTC'] = [() => {}];
socket.recordMessageHandlerStats('book.BTC');
socket.recordMessageHandlerStats('pong');
delete socket.messageHandlers['book.BTC'];
now += 5000;
socket.recordMessageHandlerStats('pong');
const stats=socket.messageHandlersStats;
// Exercise both production plain-object reads, including pre-serialization
// function identity. Explicit own shadows are installed only in the shadow cases.
const inheritedStats = [];
const proto = Object.getPrototypeOf(socket.messageHandlersStats);
const topics = [...Object.getOwnPropertyNames(proto), 'ordinary'];
for (const topic of topics) for (const ownHandlers of [null, 0, 2]) for (const ownStats of [false, true]) {
  const subject = new ws.BaseWebSocket(parent);
  if (ownHandlers !== null) Object.defineProperty(subject.messageHandlers, topic, {
    value: Array.from({length:ownHandlers}, () => () => {}), configurable:true, enumerable:true,
  });
  if (ownStats) Object.defineProperty(subject.messageHandlersStats, topic, {
    value:{lastMessageTimestamp:1, hadHandlers:true}, configurable:true, enumerable:true,
  });
  const inherited = topic === '__proto__' ? proto : proto[topic];
  const fields = ['lastMessageTimestamp','lastCallbacksCount','hadHandlers'];
  const saved = inherited && fields.map(key => Object.getOwnPropertyDescriptor(inherited, key));
  const capture = () => {
    const entry = subject.messageHandlersStats[topic];
    return { own:Object.hasOwn(subject.messageHandlersStats,topic), type:typeof entry,
      timestamp:entry?.lastMessageTimestamp, count:entry?.lastCallbacksCount,
      had:entry?.hadHandlers };
  };
  try {
    let recordError = null;
    try { subject.recordMessageHandlerStats(topic); }
    catch (error) { recordError = {name:error.name, message:error.message}; }
    const recorded = capture();
    delete subject.messageHandlers[topic];
    now += 5000;
    let reconcileError = null;
    try { subject.reconcileMessageHandlerStats(now); }
    catch (error) { reconcileError = {name:error.name, message:error.message}; }
    const reconciled=capture();
    now += 250;
    let cleanupError=null;
    try {subject.cleanupMessageHandlerStats(topic);} catch(error) {cleanupError={name:error.name,message:error.message};}
    const cleaned=capture();
    if(cleaned.own)throw Error('source cleanup must remove own stats entry');
    if(inherited&&!ownStats) {
      if(subject.messageHandlersStats[topic]!==inherited)throw Error('source cleanup retains inherited native referent');
      if(cleaned.timestamp!==recorded.timestamp)throw Error('source cleanup cannot erase earlier inherited referent mutation');
    }
    inheritedStats.push({topic,ownHandlers,ownStats,recorded,reconciled,cleaned,recordError,reconcileError,cleanupError});
  } finally {
    if (saved) fields.forEach((key,index) => {
      if (saved[index]) Object.defineProperty(inherited,key,saved[index]);
      else delete inherited[key];
    });
  }
}
const backoff = [1006,1006,1013,1013,1000,undefined,1006].map(code=>socket.getReconnectDelay(code));
timers.clear();
const duplicateNativeClose = {misses: [], pings: 0};
const closeParent = { ...parent, emitter: {emit(event, message) {
  if (event === 'grafana-error' && message.includes('pong miss')) duplicateNativeClose.misses.push(message);
}} };
const closeSocket = new ws.BaseWebSocket(closeParent);
closeSocket.ping = () => duplicateNativeClose.pings++;
closeSocket.connectAndSubscribe = () => {};
closeSocket.onClose({code:1000});
closeSocket.handlePongEvent(5000);
closeSocket.onClose({code:1000});
advance(5000);
closeSocket.ws = {close() {}};
advance(2500);
duplicateNativeClose.reconnectCount = closeSocket.reconnectCount;
timers.clear();
const reconnectThrows = [];
for (const transportFirst of [false, true]) {
  timers.clear();
  const logs = [], thrown = [];
  let reconnectListeners = 0;
  const throwParent = {...parent, log(message, severity) { if (severity === 'error') logs.push(message); }};
  const throwSocket = new ws.BaseWebSocket(throwParent);
  throwSocket.connectAndSubscribe = () => {
    // A hook can install a socket and then throw before the core adds its listener.
    throwSocket.ws = {addEventListener() { reconnectListeners++; }};
    const error = new Error('same failure'); error.name = 'ReconnectHookError'; throw error;
  };
  if (transportFirst) throwSocket.handleWebSocketError({message: 'same failure'});
  throwSocket.onClose({code:1000});
  try { advance(1000); } catch(error) { thrown.push(String(error)); }
  if (!transportFirst) throwSocket.handleWebSocketError({message: 'same failure'});
  reconnectThrows.push({transportFirst, thrown, logs, reconnecting: throwSocket.reconnecting, reconnectListeners});
}
timers.clear();
const resyncErrors = [];
for (const [name, message] of [
  ['Error', 'snapshot rejected'],
  ['HeaderCapableWebSocketUnavailableError', 'Required WebSocket upgrade headers need a header-capable ws transport'],
  ['', 'message only'], ['CustomError', ''], ['', ''], ['Error', '\ufeffmessage\u0085'],
]) {
  const error = new Error(message); error.name = name;
  const logs = [];
  const failingParent = { ...parent, log: value => logs.push(value),
    fetchAndSetPrivateSnapshot: () => Promise.reject(error) };
  const failingSocket = new ws.BaseWebSocket(failingParent);
  failingParent.getPrivateSockets = () => [failingSocket];
  failingSocket.trackConnected(); failingSocket.trackConnected();
  // Drain the real source's catch/finally promise chain without a wall-clock timer.
  for (let i = 0; i < 8; i++) await Promise.resolve();
  if (logs.length !== 1) throw new Error(`resync oracle expected one log, got ${logs.length}`);
  resyncErrors.push({name, message, expected: logs[0]});
}
// Exercise base's actual omitted/explicit argument handling and the vendored
// RWS delay method. SDK and library jitter are separate stable module samples.
const actualConfig = load('packages/safe-cex/src/config/config.ts', {}, 0);
const actualRws = load('thirdparty/reconnecting-websocket/src/ReconnectingWebsocket.ts', {'./events': load('thirdparty/reconnecting-websocket/src/events.ts')}, 0.5);
config.ENABLE_RECONNECTING_WEBSOCKET = true;
config.RECONNECTING_WEBSOCKET_TUNING = actualConfig.RECONNECTING_WEBSOCKET_TUNING;
const retryBoundaries = [-2, -1, -0.5, 0, 0.5, 1, 1.5, 2].flatMap(limit => [-1, 0, 1, 2].map(prior => {
  const attempt = Symbol('attempt accepted before network I/O');
  const probe = {_connectLock:false, _shouldReconnect:true, _retryCount:prior,
    _options:{maxRetries:limit, WebSocket:{CLOSING:2}}, _debug(){}, _removeListeners(){},
    _listeners:{close:[]}, _wait(){throw attempt;}};
  let accepted = false;
  try { actualRws.ReconnectingWebSocket.prototype._connect.call(probe); }
  catch(error) { if(error !== attempt) throw error; accepted = true; }
  return {limit, prior, attempt:accepted};
}));
const optionTuning = [undefined, {}, {minReconnectionDelay: 17, maxReconnectionDelay: 53, reconnectionDelayGrowFactor: 2}].map(options => {
  const transport = socket.createWebsocket('wss://fixture', [], options);
  const delays = [1, 2, 8].map(retry => actualRws.ReconnectingWebSocket.prototype._getNextDelay.call({
    _options: transport._options, _retryCount: retry, _debug() {},
  }));
  return {options: options ?? null, delays};
});
process.stdout.write(JSON.stringify({ inheritedStats, retryBoundaries, optionTuning, reconnectThrows, duplicateNativeClose, resyncErrors, heartbeats:heartbeats.map(input=>({input,expected:ws.isHeartbeatFrame(input)})), closeCodes:closeCodes.map(input=>({input,expected:ws.getCloseCodeDescription(input)})), redactions:redactions.map(input=>({input,expected:diagnostics.redactWsDiagnosticPayload(input)})), stats, backoff }));
