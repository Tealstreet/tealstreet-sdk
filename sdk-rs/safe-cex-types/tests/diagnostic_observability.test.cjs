const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const crypto = require('node:crypto');
const { root, ts, dependencies } = require('../scripts/accounts-source.cjs');
const { atom, getDefaultStore } = dependencies('jotai');
const { atomWithStorage } = dependencies('jotai/utils');
const files = {
  ws: 'packages/safe-cex/src/exchanges/base.ws.ts',
  controller: 'apps/web/src/controllers/exchangeController/exchangeAccountController.ts',
  atoms: 'apps/web/src/atoms/dev.atoms.ts',
};
const sources = Object.fromEntries(Object.entries(files).map(([name, file]) => {
  const text = fs.readFileSync(path.join(root, file), 'utf8');
  return [name, ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true)];
}));
function unique(source, predicate) {
  const found = [];
  function visit(node) {
    if (predicate(node)) found.push(node);
    ts.forEachChild(node, visit);
  }
  visit(source);
  assert.equal(found.length, 1, 'DIAGNOSTIC-OBSERVABILITY unique actual source expression');
  return found[0];
}
function execute(expression, environment) {
  const code = ts.transpileModule(`const selected = (${expression});`, {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  }).outputText;
  return new Function(...Object.keys(environment), `${code}\nreturn selected;`)(...Object.values(environment));
}
function initializer(name) {
  return unique(sources.atoms, node => ts.isVariableDeclaration(node) && node.name.getText() === name)
    .initializer.getText(sources.atoms);
}

test('actual WS diagnostic consumer returns the retained unit to a programmatic caller', () => {
  const limit = unique(sources.ws, node => ts.isVariableDeclaration(node)
    && node.name.getText() === 'WS_DIAGNOSTIC_MAX_PAYLOAD_CHARS').initializer;
  assert.equal(limit.getText(), '4096', 'DIAGNOSTIC-OBSERVABILITY source cap budget');
  const method = unique(sources.ws, node => ts.isMethodDeclaration(node)
    && node.name.getText() === 'capWsDiagnosticPayload');
  const subject = execute(`new (class { ${method.getText(sources.ws)} })()`, {
    WS_DIAGNOSTIC_MAX_PAYLOAD_CHARS: 4096,
  });
  const store = getDefaultStore();
  const environment = { atom, atomWithStorage };
  const requestsLogAtom = execute(initializer('requestsLogAtom'), environment);
  const enabled = execute(initializer('enableRequestResponseAndPrivateWsLogAtom'), environment);
  const upload = execute(initializer('enableWsDiagnosticsGrafanaUploadAtom'), environment);
  store.set(enabled, true);
  store.set(upload, false);
  const registration = unique(sources.controller, node => ts.isCallExpression(node)
    && ts.isPropertyAccessExpression(node.expression) && node.expression.name.text === 'on'
    && ts.isStringLiteral(node.arguments[0]) && node.arguments[0].text === 'grafana-pws');
  const receive = execute(registration.arguments[1].getText(sources.controller), {
    getDefaultStore, requestsLogAtom,
    enableRequestResponseAndPrivateWsLogAtom: enabled,
    enableWsDiagnosticsGrafanaUploadAtom: upload,
    virtualClock: { getCurrentTimeMs: () => 123 },
    grafanaLog: () => { throw Error('unexpected upload branch'); },
    LogSeverity: { Info: 'info' }, logPrefix: 'observer',
  });
  const assignment = unique(sources.atoms, node => ts.isBinaryExpression(node)
    && ts.isPropertyAccessExpression(node.left) && node.left.name.text === '__getRequestsLog');
  let condition = assignment;
  while (condition && !ts.isIfStatement(condition)) condition = condition.parent;
  assert.ok(condition, 'DIAGNOSTIC-OBSERVABILITY actual helper installation guard');
  assert.equal(execute(condition.expression.getText(sources.atoms), {
    window: { Cypress: true }, process: { env: { NODE_ENV: 'production' } },
  }), true);
  const read = execute(assignment.right.getText(sources.atoms), { getDefaultStore, requestsLogAtom });
  const input = 'a'.repeat(4095) + '\ud83d\udca9tail';
  receive(subject.capWsDiagnosticPayload(input));
  const entries = read();
  assert.equal(entries.length, 1, 'DIAGNOSTIC-OBSERVABILITY actual consumer entry count');
  const message = entries[0].message;
  assert.equal(typeof message, 'string');
  assert.equal(message.length, 4121, 'DIAGNOSTIC-OBSERVABILITY post-consumer UTF16 length');
  assert.equal(message.charCodeAt(4095), 0xd83d, 'DIAGNOSTIC-OBSERVABILITY post-consumer lone unit');
  console.log(JSON.stringify({ architecture: `${process.platform}-${process.arch}`, node: process.version,
    sourceSha256: Object.fromEntries(Object.entries(files).map(([name, file]) =>
      [name, crypto.createHash('sha256').update(fs.readFileSync(path.join(root, file))).digest('hex')])) }));
});
