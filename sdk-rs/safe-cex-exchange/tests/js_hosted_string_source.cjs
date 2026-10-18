// Actual remote adapter plus installed Axios on both sides of the signing relay.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript'), axios = require('axios');
const root = path.resolve(__dirname, '../../..');
function load(file, imports) {
  const code = ts.transpileModule(fs.readFileSync(path.join(root, 'packages/safe-cex/src', file), 'utf8'), { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS, esModuleInterop: true } }).outputText;
  const mod = { exports: {} };
  new Function('module', 'exports', 'require', code)(mod, mod.exports, name => {
    assert.ok(Object.hasOwn(imports, name), 'unexpected source import ' + name);
    return imports[name];
  });
  return mod.exports;
}
const parse = load('utils/tryParseJson.ts', {});
const { createRemoteSigningAdapter } = load('remoteSigningAdapter.ts', { axios, nanoid: { nanoid: () => 'source-id' }, './utils/tryParseJson': parse, './utils/venueRestDiagnostics': { markRestSigned: () => {} } });
(async () => {
  const rows = [];
  for (const [data, contentType] of [['size=1.235&price=100.13', 'application/x-www-form-urlencoded'], ['', 'application/x-www-form-urlencoded'], ['{"name":"value"}', 'application/json'], ['123', 'application/json']]) {
    let requestData, wireData;
    const browser = axios.create({ adapter: async config => ({ status: 200, data: null, headers: {}, config }) });
    const signer = { sign: async request => {
      requestData = request.config.data;
      const result = await browser.request({ ...request.config, data: parse.tryParseJson(request.config.data) });
      return { id: request.id, config: { headers: result.config.headers.toJSON(), data: result.config.data } };
    } };
    const adapter = createRemoteSigningAdapter(signer, 'apexomni', () => async config => {
      wireData = config.data;
      return { status: 200, data: null, headers: {}, config };
    }, 'hosted');
    await axios.create({ adapter }).post('/order', data, { headers: { 'Content-Type': contentType } });
    rows.push({ data, contentType, requestData, wireData });
  }
  assert.equal(rows.length, 4);
  process.stdout.write(JSON.stringify(rows));
})().catch(error => { console.error(error); process.exitCode = 1; });
