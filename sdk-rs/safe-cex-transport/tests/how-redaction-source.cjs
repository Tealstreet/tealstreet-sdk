const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const root = path.resolve(__dirname, '../../..');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || path.join(root, 'node_modules/typescript'));
const file = path.join(root, 'packages/http-over-websocket/src/axiosHttpOverWebsocketAdapter.ts');
const source = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const names = new Set([
  'MAX_LOG_STRING_LENGTH', 'MAX_LOG_ARRAY_LENGTH', 'MAX_LOG_OBJECT_KEYS',
  'MAX_LOG_DEPTH', 'REDACTED', 'TRUNCATED', 'SENSITIVE_LOG_KEY_PATTERN',
]);
const constants = source.statements.filter(node => ts.isVariableStatement(node)
  && node.declarationList.declarations.some(decl => names.has(decl.name.getText(source))));
assert.equal(constants.length, names.size);
const sanitizer = source.statements.find(node => ts.isFunctionDeclaration(node)
  && node.name?.text === 'sanitizeForTradeTunnelLog');
assert(sanitizer);
const code = ts.transpileModule([...constants, sanitizer].map(node => node.getText(source)).join('\n'), {
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
}).outputText;
const sanitize = new Function(code + '\nreturn sanitizeForTradeTunnelLog;')();
const keys = [
  ['apiKey-high-surrogate', 'apiKey\ud800', true],
  ['low-surrogate-authorization', '\udc00Authorization', true],
  ['surrogate-both-sides', '\udfffAPI_KEY\ud800', true],
  ...['Authorization', 'Cookie', 'password', 'passphrase', 'clientSecret', 'token',
    'signature', 'api-key', 'API_KEY', 'apiKey', 'access-key', 'access_key',
    'accessKey', 'x-mbx-apikey'].map(key => [key, key, true]),
  ['embedded-case-insensitive', 'prefix-aPi_KeY-suffix', true],
  ['non-secret', 'symbol', false],
  ['non-secret-surrogate', 'symbol\ud800', false],
  ['surrogate-interrupts-pattern', 'api\ud800Key', false],
  ['non-unicode-regexp-long-s', 'pa\u017f\u017fword', false],
  ['non-unicode-regexp-kelvin', 'to\u212aen', false],
];
const cases = keys.map(([label, key, redacted], index) => {
  const input = { [key]: `public-test-marker-${index}`, ordinary: 'public-control' };
  const output = sanitize(input);
  assert.equal(output[key], redacted ? '[REDACTED]' : input[key], label);
  assert.deepEqual(Object.keys(output), Object.keys(input));
  assert.equal(output.ordinary, input.ordinary);
  return { label, key: Array.from({ length: key.length }, (_, i) => key.charCodeAt(i)),
    raw: JSON.stringify(input), sanitized: JSON.stringify(output) };
});
assert.equal(cases.length, 23);
console.log(JSON.stringify(cases));
