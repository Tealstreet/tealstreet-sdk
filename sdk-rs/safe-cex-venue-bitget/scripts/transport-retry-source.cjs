// Execute maintained retry/adapter bodies. Only provider I/O is supplied here.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const root = path.resolve(__dirname, '../../..');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || path.join(root, 'node_modules/typescript'));
const { isNetworkError } = require('axios-retry');

function declarations(file, names, bindings) {
  const text = fs.readFileSync(path.join(root, file), 'utf8');
  const ast = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
  const nodes = ast.statements.filter(node => ts.isVariableStatement(node) &&
    node.declarationList.declarations.some(decl => names.includes(decl.name.text)));
  assert.equal(nodes.length, names.length, 'each maintained source declaration must exist');
  const exports = {};
  const code = ts.transpileModule(nodes.map(node => node.getText(ast)).join('\n'), {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  }).outputText;
  new Function('exports', ...Object.keys(bindings), code)(exports, ...Object.values(bindings));
  return exports;
}
const reads = declarations('packages/safe-cex/src/utils/retryReadsOnly.ts',
  ['isReadOnlyMethod', 'onlyReads', 'networkErrorOnRead'], { isNetworkError });
const denied = [
  'ERR_CANCELED', 'ECONNABORTED', 'ENOTFOUND', 'ENETUNREACH',
  'UNABLE_TO_GET_ISSUER_CERT', 'UNABLE_TO_GET_CRL', 'UNABLE_TO_DECRYPT_CERT_SIGNATURE',
  'UNABLE_TO_DECRYPT_CRL_SIGNATURE', 'UNABLE_TO_DECODE_ISSUER_PUBLIC_KEY',
  'CERT_SIGNATURE_FAILURE', 'CRL_SIGNATURE_FAILURE', 'CERT_NOT_YET_VALID', 'CERT_HAS_EXPIRED',
  'CRL_NOT_YET_VALID', 'CRL_HAS_EXPIRED', 'ERROR_IN_CERT_NOT_BEFORE_FIELD',
  'ERROR_IN_CERT_NOT_AFTER_FIELD', 'ERROR_IN_CRL_LAST_UPDATE_FIELD',
  'ERROR_IN_CRL_NEXT_UPDATE_FIELD', 'OUT_OF_MEM', 'DEPTH_ZERO_SELF_SIGNED_CERT',
  'SELF_SIGNED_CERT_IN_CHAIN', 'UNABLE_TO_GET_ISSUER_CERT_LOCALLY', 'UNABLE_TO_VERIFY_LEAF_SIGNATURE',
  'CERT_CHAIN_TOO_LONG', 'CERT_REVOKED', 'INVALID_CA', 'PATH_LENGTH_EXCEEDED',
  'INVALID_PURPOSE', 'CERT_UNTRUSTED', 'CERT_REJECTED', 'HOSTNAME_MISMATCH',
];
const cases = denied.map(code => ({ name: `denied-${code}`, code, method: 'get', expected: false }));
for (const code of ['ECONNRESET', 'ECONNREFUSED', 'EPIPE']) {
  cases.push({ name: `allowed-${code}`, code, method: 'get', expected: true });
}
for (const [name, response, expected] of [
  ['null', null, true], ['false', false, true], ['zero', 0, true], ['empty-text', '', true],
  ['object', {}, false], ['array', [], false],
]) cases.push({ name: `response-${name}`, code: 'ECONNRESET', response, method: 'get', expected });
for (const [name, code, expected] of [
  ['null', null, false], ['empty', '', false], ['false', false, false], ['zero', 0, false],
  ['number', 17, true], ['object', {}, true], ['array', [], true],
]) cases.push({ name: `code-${name}`, code, method: 'get', expected });
for (const [method, expected] of [['HEAD', true], ['GeT', true], ['post', false], ['delete', false], ['options', false]]) {
  cases.push({ name: `method-${method}`, code: 'ECONNRESET', method, expected });
}
cases.push({ name: 'code-absent', method: 'get', expected: false });
assert.equal(cases.length, 54, 'independent source retry corpus floor');
for (const row of cases) {
  const error = { config: { method: row.method } };
  if (Object.hasOwn(row, 'code')) error.code = row.code;
  if (Object.hasOwn(row, 'response')) error.response = row.response;
  row.retry = reads.networkErrorOnRead()(error);
  assert.equal(row.retry, row.expected, `maintained source retry ${row.name}`);
}

const installs = [];
for (const [available, remote] of [[true, false], [false, false], [true, true]]) {
  const original = () => {};
  const adapter = () => {};
  const instance = { defaults: { adapter: original } };
  const options = { tealstreetTunnel: true };
  const innerAdapterSlots = new WeakMap();
  const slot = { adapter: undefined };
  if (remote) innerAdapterSlots.set(instance, slot);
  let selected = 0;
  const imports = declarations('packages/safe-cex/src/safeCexAxiosCreate.ts',
    ['setInnerAdapterForRemoteSigning', 'installTransportAdapter'], {
      innerAdapterSlots,
      getTransport(actual) {
        assert.equal(actual, options, 'source options identity');
        return { restAdapter(context) {
          selected++;
          assert.equal(context.options, options);
          assert.equal(context.exchange, 'bitget');
          assert.equal(context.variant, 'default');
          return available ? adapter : undefined;
        } };
      },
    });
  imports.installTransportAdapter(instance, options, 'bitget');
  assert.equal(selected, 1);
  assert.equal(instance.defaults.adapter, available && !remote ? adapter : original);
  if (remote) assert.equal(slot.adapter, adapter, 'source retains remote signing outer adapter');
  installs.push({ available, remote, selected });
}
console.log(JSON.stringify({ cases, installs }));
