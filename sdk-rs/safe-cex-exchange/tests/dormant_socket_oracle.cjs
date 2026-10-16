const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { root, ts } = require('../../safe-cex-types/scripts/accounts-source.cjs');

async function witness(kind, nodeCapable, reopens = 1) {
  const timers = new Map(), dials = [];
  let nextTimer = 0, providerCalls = 0;
  const setTimer = (callback, delay) => {
    const id = ++nextTimer;
    timers.set(id, { callback, delay });
    return id;
  };
  class Socket {
    static CLOSING = 2;
    readyState = 0;
    constructor(url, protocols, options) { dials.push({ url, protocols, options }); }
    addEventListener() {}
    removeEventListener() {}
    close() { this.readyState = 3; }
  }
  const modules = new Map();
  function load(file) {
    if (modules.has(file)) return modules.get(file);
    const module = { exports: {} };
    const code = ts.transpileModule(fs.readFileSync(file, 'utf8'), {
      compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
    }).outputText;
    new Function('require', 'module', 'exports', 'setTimeout', 'clearTimeout', 'console', 'WebSocket', code)(
      name => {
        assert.equal(name, './events', 'START_CLOSED source dependency');
        return load(path.join(path.dirname(file), 'events.ts'));
      }, module, module.exports, setTimer, id => timers.delete(id), { log() {} }, Socket,
    );
    modules.set(file, module.exports);
    return module.exports;
  }
  const { ReconnectingWebSocket } = load(path.join(root, 'thirdparty/reconnecting-websocket/src/ReconnectingWebsocket.ts'));
  const file = 'packages/safe-cex/src/exchanges/base.ws.ts';
  const source = ts.createSourceFile(file, fs.readFileSync(path.join(root, file), 'utf8'), ts.ScriptTarget.Latest, true);
  const cls = source.statements.find(node => ts.isClassDeclaration(node) && node.name.text === 'BaseWebSocket');
  const members = cls.members.filter(node => node.name?.getText(source) === 'createWebsocket');
  assert.equal(members.length, 1, 'START_CLOSED source independent constructor-member count');
  const code = ts.transpileModule(`class Witness { ${members[0].getText(source)} }; return Witness;`, {
    compilerOptions: { target: ts.ScriptTarget.ES2022 },
  }).outputText;
  class HeaderError extends Error {}
  const Witness = new Function('ReconnectingWebSocket', 'RelayWebSocket', 'WebSocket', 'getNodeWebSocket',
    'getTransport', 'ENABLE_RECONNECTING_WEBSOCKET', 'HeaderCapableWebSocketUnavailableError', 'RECONNECTING_WEBSOCKET_TUNING', code)(
    ReconnectingWebSocket, Socket, Socket, () => nodeCapable ? Socket : undefined,
    () => ({ wsConnection: () => kind ? { kind, headers: { route: 'kept' } } : undefined }),
    false, HeaderError, {},
  );
  const owner = new Witness();
  owner.parent = { options: {} };
  owner.trackConnected = () => {};
  owner.handleWebSocketError = () => {};
  const socket = owner.createWebsocket('wss://venue', ['orders'], {
    startClosed: true,
    headers: () => { providerCalls++; return { caller: 'kept' }; },
  });
  const managed = Boolean(kind || nodeCapable);
  assert.equal(dials.length, managed ? 0 : 1, 'START_CLOSED source selected branch');
  if (!managed) return { initialDials: 1 };
  assert.equal(socket.readyState, 3, 'START_CLOSED source initial CLOSED');
  assert.equal(providerCalls, 0);
  assert.equal(timers.size, 0, 'START_CLOSED source no scheduled connection work');
  socket.close();
  for (let index = 0; index < reopens; index++) socket.reconnect();
  assert.equal(socket.readyState, 3, 'START_CLOSED source remains CLOSED during activation');
  assert.equal(providerCalls, 0);
  assert.equal(dials.length, 0);
  assert.equal(timers.size, reopens);
  for (const [id, timer] of Array.from(timers)) {
    assert.equal(timer.delay, 0, 'START_CLOSED source initial activation delay');
    timers.delete(id);
    timer.callback();
  }
  for (let turn = 0; turn < 10; turn++) await Promise.resolve();
  assert.equal(providerCalls, reopens, 'START_CLOSED source fresh provider after timer');
  assert.equal(dials.length, reopens);
  assert.deepEqual(dials[0].protocols, ['orders']);
  assert.deepEqual(dials[0].options.headers, kind ? { caller: 'kept', route: 'kept' } : { caller: 'kept' });
  assert.equal(socket.readyState, 0);
  socket.close();
  return { initialDials: 0, initialReady: 3, activatedDials: dials.length, providerCalls };
}

(async () => {
  const cases = [];
  for (const [kind, node] of [['relay', false], ['headers', false], [undefined, true], [undefined, false]]) {
    cases.push(await witness(kind, node));
  }
  assert.equal(cases.length, 4, 'START_CLOSED source independent branch floor');
  const repeated = await witness('relay', false, 2);
  assert.equal(repeated.activatedDials, 2, 'START_CLOSED source repeated-reopen floor');
  process.stdout.write(JSON.stringify({ cases, repeated, branches: 4, microtaskFloor: 10 }));
})().catch(error => { console.error(error); process.exitCode = 1; });
