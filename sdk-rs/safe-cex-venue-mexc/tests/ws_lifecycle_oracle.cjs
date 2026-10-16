const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'isolated compiler required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/mexc');
const names = { public: ['startHeartbeat', 'stopHeartbeat', 'handleClose', 'onOpen', 'ping', 'dispose'], private: ['startHeartbeat', 'stopHeartbeat', 'handleClose', 'dispose'] };
const constants = fs.readFileSync(path.join(root, 'mexc.types.ts'), 'utf8');
const constantsAst = ts.createSourceFile('mexc.types.ts', constants, ts.ScriptTarget.Latest, true);
const constant = constantsAst.statements.filter(n => ts.isVariableStatement(n) && n.declarationList.declarations.some(d => d.name.getText(constantsAst) === 'MEXC_WS_HEARTBEAT_INTERVAL_MS'));
assert.equal(constant.length, 1, 'source interval declaration');
const bits = number => { const b = Buffer.alloc(8); b.writeDoubleBE(number); return b.toString('hex'); };
const cases = [];
for (const venue of ['public', 'private']) {
  const text = fs.readFileSync(path.join(root, `mexc.ws-${venue}.ts`), 'utf8');
  const ast = ts.createSourceFile('ws.ts', text, ts.ScriptTarget.Latest, true);
  const klass = ast.statements.find(n => ts.isClassDeclaration(n) && n.name.text === `Mexc${venue === 'public' ? 'Public' : 'Private'}Websocket`);
  const methods = names[venue].map(name => {
    const selected = klass.members.filter(n => n.name?.getText(ast) === name);
    assert.equal(selected.length, 1, `source ${venue}.${name}`);
    return selected[0].getText(ast);
  });
  for (const method of names[venue]) for (let scenario = 0; scenario < 10; scenario++) {
    const trace = [], active = new Map(), registered = [];
    const failure = { marker: 47, message: 'original host failure' };
    const initialDisposed = scenario === 2, initialConnected = scenario !== 3;
    const failurePoint = scenario === 4 ? 'clear' : scenario === 5 ? 'ping' : scenario === 6 ? 'interval' : scenario === 7 ? (method === 'onOpen' ? 'reconcile' : method === 'ping' ? 'send' : method === 'handleClose' ? 'onClose' : 'base') : scenario === 8 ? (method === 'onOpen' ? 'resubscribe' : method === 'ping' ? 'now' : method === 'dispose' ? (venue === 'public' ? 'off' : 'timeout') : 'clear') : '';
    const heartbeat = { id: 17 }, refresh = { id: 29 }, handler = { id: 41 }, replacement = { id: 31 };
    let instance, callbackFailure = false;
    function event(tag, extra = {}) { trace.push({ tag, ...extra }); if (failurePoint === tag || (tag === 'ping' && callbackFailure)) throw failure; }
    class Parent { dispose() { event('base'); return { ignored: true }; } }
    const context = { exports: {}, Parent,
      clearInterval(handle) {
        if (handle) assert.equal(handle, instance.heartbeatInterval, 'source original heartbeat handle');
        event('clear', { handle: handle?.id ?? null });
        if (handle) active.delete(handle.id);
        if (scenario === 9) { instance.heartbeatInterval = replacement; instance.refreshBalanceTimeout = replacement; instance.handleActiveTickerSymbolsChanged = replacement; }
      },
      setInterval(callback, period) {
        event('interval', { period });
        assert.equal(period, 8000, 'independent source interval milliseconds');
        const handle = { id: 23 + registered.length }; registered.push({ callback, handle }); active.set(handle.id, callback); return handle;
      },
      clearTimeout(handle) {
        assert.equal(handle, instance.refreshBalanceTimeout, 'source refresh handle read after stop');
        event('timeout', { handle: handle?.id ?? null });
      },
      performance: { now() { event('now'); if (scenario === 9) instance.pingAt = 999; return scenario === 1 ? -0 : 123.75; } },
    };
    const code = `${constant[0].getText(constantsAst)}\nexports.Socket=class extends Parent { ${methods.join('\n')} };`;
    vm.runInNewContext(ts.transpileModule(code, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS } }).outputText, context);
    instance = new context.exports.Socket();
    Object.assign(instance, { isDisposed: initialDisposed, isConnected: initialConnected, heartbeatInterval: scenario === 1 ? undefined : heartbeat, refreshBalanceTimeout: scenario === 1 ? undefined : refresh, handleActiveTickerSymbolsChanged: handler, pingAt: 111.5 });
    const closeEvent = scenario === 1 ? undefined : scenario === 3 ? null : { id: 83, code: 1006 };
    instance.onClose = value => { assert.strictEqual(value, closeEvent, 'source original close event'); event('onClose', { kind: value === undefined ? 'undefined' : value === null ? 'null' : 'record', handle: value?.id ?? null }); return 73; };
    instance.parent = { off(name, actual) { assert.equal(name, 'activeTickerSymbols:changed'); assert.strictEqual(actual, instance.handleActiveTickerSymbolsChanged, 'source handler read after stop'); event('off', { text: name, handle: actual.id }); } };
    instance.reconcileTickerSubscriptions = () => { event('reconcile'); if (scenario === 9) instance.isDisposed = true; };
    instance.resubscribe = () => event('resubscribe'); instance.emitWsReconnected = () => event('reconnected');
    instance.sendWithDiagnostics = message => event('send', { text: message });
    // These rows call the receiver ping method; keep that call boundary opaque.
    // The public ping row itself executes its original body separately.
    if (method !== 'ping') instance.ping = () => event('ping');
    const steps = [];
    for (let iteration = 0; iteration < 2; iteration++) {
      instance.isDisposed = initialDisposed; instance.isConnected = initialConnected;
      let error = false;
      try { assert.equal(instance[method](...(method === 'handleClose' ? [closeEvent] : [])), undefined, 'source lifecycle void'); }
      catch (caught) { assert.strictEqual(caught, failure, 'source original failure identity'); error = true; }
      const ticks = [];
      const callback = active.get(instance.heartbeatInterval?.id);
      if (callback) for (const [disposed, connected] of [[false, true], [false, false], [true, true], [true, false]]) {
        instance.isDisposed = disposed; instance.isConnected = connected;
        const before = trace.length; let tickError = false;
        try { callback(); } catch (caught) { assert.strictEqual(caught, failure); tickError = true; }
        const changed = trace.length - before;
        assert.equal(changed, !disposed && connected ? 1 : 0, 'independent live callback guard');
        ticks.push({ disposed, connected, error: tickError });
      }
      if (callback) {
        instance.isDisposed = false; instance.isConnected = true; callbackFailure = true;
        const before = trace.length;
        try { callback(); assert.fail('scheduled callback failure required'); }
        catch (caught) { assert.strictEqual(caught, failure, 'source scheduled failure identity'); }
        assert.equal(trace.length - before, 1, 'independent scheduled failure call count');
        ticks.push({ disposed: false, connected: true, error: true }); callbackFailure = false;
      }
      steps.push({ trace: structuredClone(trace), error, heartbeat: instance.heartbeatInterval?.id ?? null, refresh: instance.refreshBalanceTimeout?.id ?? null, handler: instance.handleActiveTickerSymbolsChanged.id, pingBits: bits(instance.pingAt), registered: registered.length, ticks });
    }
    cases.push({ venue, method, scenario, initialDisposed, initialConnected, failurePoint, steps });
  }
}
assert.equal(cases.length, 100, 'independent ten-method ten-scenario floor');
assert.equal(cases[99].venue, 'private'); assert.equal(cases[99].method, 'dispose'); assert.equal(cases[99].scenario, 9, 'last lifecycle fixture');
process.stdout.write(JSON.stringify(cases));
