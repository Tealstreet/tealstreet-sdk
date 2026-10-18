// Maintained setLeverage + API + safeCexAxiosCreate with installed Axios/qs.
// Inject only clock, telemetry/transport capabilities and unrelated position-read seam.
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const assert = require('node:assert/strict'), { createRequire } = require('node:module');
const root = process.argv[2], timestamp = Number(process.argv[3] || 1700000000000);
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH is required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const external = createRequire(path.join(root, 'package.json')), axios = external('axios');
const sourceRoot = path.join(root, 'packages/safe-cex/src');
const cache = new Map();
const hosts = {
  './remoteSigningAdapter': { createRemoteSigningAdapter: () => { throw Error('unexpected remote signer'); } },
  './transport/registry': { getTransport: () => ({ rest: ({ config }) => ({ baseURL: config.baseURL, proxied: false }) }) },
  './utils/axiosErrorEnhancer': { enhanceAxiosError: e => e },
  './utils/isDevelopment': { isDevelopment: false },
  './utils/signingCapture': { isSigningCaptureRequest: () => false },
  './utils/venueRestDiagnostics': { markRestRequestStart() {}, noteRestFailure() {}, noteRestSuccess() {} },
};
function load(file) {
  if (cache.has(file)) return cache.get(file).exports;
  const module = { exports: {} }; cache.set(file, module);
  const code = ts.transpileModule(fs.readFileSync(file, 'utf8'), { fileName: file, compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS, esModuleInterop: true } }).outputText;
  new Function('module', 'exports', 'require', code)(module, module.exports, spec => {
    if (file === path.join(sourceRoot, 'safeCexAxiosCreate.ts') && Object.hasOwn(hosts, spec)) return hosts[spec];
    if (spec.endsWith('/virtualClock')) return { virtualClock: { getCurrentTimeMs: () => timestamp } };
    if (spec.startsWith('@tealstreet/')) return load(path.join(root, 'packages', spec.slice(12)) + '.ts');
    if (spec.startsWith('.')) return load(path.resolve(path.dirname(file), spec) + '.ts');
    return external(spec);
  });
  return module.exports;
}
(async () => {
  const { createAPI } = load(path.join(sourceRoot, 'exchanges/bingx/bingx.api.ts'));
  const types = load(path.join(sourceRoot, 'types.ts'));
  const constants = load(path.join(sourceRoot, 'exchanges/bingx/bingx.types.ts'));
  const file = path.join(sourceRoot, 'exchanges/bingx/bingx.exchange.ts');
  const source = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
  const cls = source.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'BingxExchange');
  const members = ['setLeverage', 'toBingxSymbol'].map(name => {
    const matches = cls.members.filter(n => n.name?.getText(source) === name);
    assert.equal(matches.length, 1, name); return matches[0].getText(source);
  });
  const code = ts.transpileModule('class Venue {' + members.join('\n') + '} Venue;', { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText;
  const Venue = vm.runInNewContext(code, { ...types, ...constants });
  const venue = new Venue(), requests = [], reports = [];
  venue.name = 'bingx';
  venue.store = { marketsBySymbol: { BTCUSDT: { id: 'BTC-USDT', limits: { leverage: { min: 1, max: 50 } } } }, addOrUpdatePositions() {}, recordRequestOutcome() {} };
  venue.grafanaEndpointsEvery = []; venue.grafanaEndpointsPeriodic = []; venue.emitter = { emit() {} };
  venue.getOrFetchPositionMode = async () => types.PositionMode.OneWay;
  venue.fetchPositions = async () => [];
  venue.emitAndLogError = e => reports.push(e);
  venue.xhr = createAPI({ key: 'fixture-key', secret: 'fixture-secret', tealstreetTunnel: false }, venue);
  venue.xhr.defaults.adapter = async config => {
    requests.push(config.data);
    return { status: 200, data: JSON.stringify({ code: 0, data: {} }), config, headers: {} };
  };
  await venue.setLeverage('BTCUSDT', NaN);
  assert.equal(requests.length, 1);
  assert.equal(reports.length, 0);
  const body = JSON.parse(requests[0]);
  assert.equal(body.leverage, null);
  const preimage = external('qs').stringify({ leverage: NaN, side: 'BOTH', symbol: 'BTC-USDT', timestamp }, { arrayFormat: 'repeat', sort: (a, b) => a.localeCompare(b) });
  assert.equal(body.signature, external('node:crypto').createHmac('sha256', 'fixture-secret').update(preimage).digest('hex'));
  process.stdout.write(JSON.stringify({ body, preimage }));
})().catch(error => { console.error(error); process.exitCode = 1; });
