const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'WS-POLICY compiler path required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const sourceRoot = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/cryptocom');

// Execute the unchanged complete constants module; no copied threshold expressions.
const typesFile = path.join(sourceRoot, 'cryptocom.types.ts');
const compiledTypes = ts.transpileModule(fs.readFileSync(typesFile, 'utf8'), {
  fileName: typesFile, reportDiagnostics: true,
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
});
assert.equal(compiledTypes.diagnostics.filter(d => d.category === ts.DiagnosticCategory.Error).length, 0);
const types = { exports: {} };
new Function('module', 'exports', 'require', compiledTypes.outputText)(types, types.exports, name => {
  throw new Error('WS-POLICY unexpected constants import ' + name);
});
const reconnect = types.exports.CRYPTOCOM_WS_IDLE_RECONNECT_THRESHOLD_MS;
const ping = types.exports.CRYPTOCOM_WS_IDLE_PING_THRESHOLD_MS;
assert.equal(reconnect, 90_000, 'WS-POLICY independent reconnect constant');
assert.equal(ping, 30_000, 'WS-POLICY independent ping constant');

const methods = ['shouldEnforcePongMissReconnect', 'getWatchdogStaleThresholdMs', 'getWatchdogPingThresholdMs', 'ping'];
const rows = [];
for (const [socket, className] of [['private', 'CryptocomPrivateWebSocket'], ['public', 'CryptocomPublicWebSocket']]) {
  const file = path.join(sourceRoot, 'cryptocom.ws-' + socket + '.ts');
  const source = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
  const classes = source.statements.filter(n => ts.isClassDeclaration(n) && n.name?.text === className);
  assert.equal(classes.length, 1, 'WS-POLICY actual class ' + className);
  for (const method of methods) {
    const fields = classes[0].members.filter(n => n.name?.getText(source) === method);
    assert.equal(fields.length, 1, 'WS-POLICY actual field ' + method);
    assert(ts.isArrowFunction(fields[0].initializer), 'WS-POLICY actual arrow initializer');
    let touches = 0;
    const receiver = new Proxy({}, {
      get() { touches++; throw new Error('stateless policy accessed receiver'); },
      set() { touches++; throw new Error('stateless policy changed receiver'); },
    });
    const compiled = ts.transpileModule(`return (${fields[0].initializer.getText(source)});`, {
      compilerOptions: { target: ts.ScriptTarget.ES2022 },
    }).outputText;
    const factory = new Function('CRYPTOCOM_WS_IDLE_RECONNECT_THRESHOLD_MS', 'CRYPTOCOM_WS_IDLE_PING_THRESHOLD_MS', '"use strict";\n' + compiled);
    const result = factory.call(receiver, reconnect, ping)();
    const kind = typeof result;
    assert(['number', 'boolean', 'undefined'].includes(kind), 'WS-POLICY finite native return vocabulary');
    let bits;
    if (kind === 'number') { const b = Buffer.alloc(8); b.writeDoubleBE(result); bits = b.toString('hex'); }
    assert.equal(touches, 0, 'WS-POLICY no receiver effects ' + socket + '/' + method);
    rows.push({ socket, method, kind, bits, value: kind === 'boolean' ? result : undefined, touches });
  }
}
assert.equal(rows.length, 8, 'WS-POLICY independent source inventory');
process.stdout.write(JSON.stringify({ rows }));
