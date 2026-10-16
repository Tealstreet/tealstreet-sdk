const assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH), root = path.resolve(__dirname, '../../..');
function parse(file) {
  const source = ts.createSourceFile(file, fs.readFileSync(path.join(root, file), 'utf8'), 99, true);
  assert.equal(source.parseDiagnostics.length, 0, 'state_source_parse');
  return source;
}
function execute(code, globals = {}) {
  const module = { exports: {} };
  const output = ts.transpileModule(code, { reportDiagnostics: true, compilerOptions: { module: 1, target: 9 } });
  assert.equal(output.diagnostics.length, 0, 'state_transpile');
  vm.runInNewContext(output.outputText, { module, exports: module.exports, ...globals });
  return module.exports;
}
const exchange = parse('packages/safe-cex/src/exchanges/paradex/paradex.exchange.ts');
const cls = exchange.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'ParadexExchange');
const members = ['isPublicDataOnly', 'resetSDKStateAfterInitFailure', 'fetchHistoricalPositions'].map(name => {
  const found = cls.members.filter(n => n.name?.getText(exchange) === name);
  assert.equal(found.length, 1, 'state_member_identity');
  return found[0].getText(exchange);
}).join('\n');
const { Harness } = execute(`export class Harness {${members}}`);
const sdk = parse('packages/safe-cex/src/exchanges/paradex/paradex.sdk.ts');
const declarations = sdk.statements.filter(n => ts.isFunctionDeclaration(n) && n.name.text === 'getParadexSDK');
assert.equal(declarations.length, 1, 'state_sdk_identity');
const sdkBody = declarations[0].getText(sdk);
// Replace only the external module-import capability; retain the cache body.
const importSite = "import('@tealstreet/paradex-sdk')";
assert.equal(sdkBody.split(importSite).length, 2, 'state_import_site');
const loaderBody = sdkBody.replace(importSite, 'load()');
const units = text => Array.from({ length: text.length }, (_, i) => text.charCodeAt(i));
async function drain() { for (let i = 0; i < 12; i++) await Promise.resolve(); }
function deferred() {
  let resolve, reject;
  const promise = new Promise((r, j) => { resolve = r; reject = j; });
  return { promise, resolve, reject };
}
function publicCase(flag, mask, failure) {
  const h = new Harness(), trace = [], error = {}, opts = {};
  const fields = ['publicDataOnly', 'key', 'paradexAddress', 'subkeyAddress'];
  const values = [flag === 'absent' ? undefined : flag === 'true', ...[0, 1, 2].map(i => mask & (1 << i) ? `credential-${i}-\ud800` : '')];
  fields.forEach((field, i) => Object.defineProperty(opts, field, { get() { trace.push(i); if (failure === i) throw error; return values[i]; } }));
  h.options = opts;
  let outcome;
  try { outcome = { ok: true, value: h.isPublicDataOnly }; } catch (e) { outcome = { ok: false, same: e === error }; }
  return { flag, mask, failure, credentials: values.slice(1).map(units), trace, outcome };
}
function resetCase(mask, failure) {
  const h = new Harness(), trace = [], error = {}, original = [{}, {}, {}, {}];
  const values = original.map((value, i) => mask & (1 << i) ? value : null);
  ['sdkInitPromise', 'sdkAccount', 'requestSigner', 'jwtManager'].forEach((field, i) => Object.defineProperty(h, field, {
    get() { return values[i]; }, set(value) { trace.push([i, value === null]); if (failure === i) throw error; values[i] = value; }
  }));
  let outcome;
  try { outcome = { ok: true, undefined: h.resetSDKStateAfterInitFailure() === undefined }; } catch (e) { outcome = { ok: false, same: e === error }; }
  return { mask, failure, trace, slots: values.map((value, i) => value === null ? 'null' : value === original[i] ? 'original' : 'wrong'), outcome };
}
async function historyCase(mode) {
  const h = new Harness(), trace = [], error = {}, value = [], opts = { account: 'account-\ud800', from: -0, to: Infinity }, held = deferred();
  h.foldClosedPositionRecords = function(given, flags) {
    trace.push([given === opts, flags.symbolScoped, Object.keys(flags), this === h]);
    if (mode === 'sync-error') throw error;
    return held.promise;
  };
  let settled = false;
  const pending = h.fetchHistoricalPositions(opts).then(out => { settled = true; return { ok: true, same: out === value }; }, e => { settled = true; return { ok: false, same: e === error }; });
  await drain();
  const before = { trace: trace.slice(), settled };
  if (mode === 'async-error') held.reject(error); else held.resolve(value);
  return { mode, opts: { account: units(opts.account), fromNegativeZero: Object.is(opts.from, -0), toInfinity: opts.to === Infinity }, before, outcome: await pending, trace };
}
async function cacheCase(mode) {
  const modules = [{ id: 'cached' }, { id: 'a' }, { id: 'b' }, { id: 'warm' }], error = {}, loads = [], a = deferred(), b = deferred();
  let calls = 0, settled = 0;
  const { get, peek } = execute(`let paradexSdk = initial; ${loaderBody}\nexport const get = getParadexSDK; export const peek = () => paradexSdk;`, {
    initial: mode === 'cached' ? modules[0] : null,
    load() { calls++; loads.push(calls === 1 ? 'a' : calls === 2 && mode.startsWith('race-') ? 'b' : 'warm'); if (calls === 1 && mode === 'sync-error') throw error; return calls === 1 ? a.promise : calls === 2 && mode.startsWith('race-') ? b.promise : Promise.resolve(modules[3]); }
  });
  const observe = promise => promise.then(value => { settled++; return { ok: true, id: value.id, same: modules.includes(value) }; }, e => { settled++; return { ok: false, same: e === error }; });
  const first = observe(get()), second = mode.startsWith('race-') ? observe(get()) : null;
  await drain();
  const before = { loads: loads.slice(), cache: peek()?.id ?? null, settled }, after = [];
  const order = mode === 'race-ba' ? ['b', 'a'] : mode.startsWith('race-') ? ['a', 'b'] : ['a'];
  for (const who of order) {
    if (who === 'a') { if (mode === 'async-error' || mode === 'race-a-error') a.reject(error); else a.resolve(modules[1]); }
    else { if (mode === 'race-b-error') b.reject(error); else b.resolve(modules[2]); }
    after.push({ who, outcome: await (who === 'a' ? first : second), cache: peek()?.id ?? null });
  }
  const warm = await observe(get());
  return { mode, before, after, warm, loads, cache: peek()?.id ?? null };
}
(async () => {
  const publicCases = [], resets = [], histories = [], caches = [];
  for (const flag of ['absent', 'false', 'true']) for (let mask = 0; mask < 8; mask++) for (const failure of [-1, 0, 1, 2, 3]) publicCases.push(publicCase(flag, mask, failure));
  for (let mask = 0; mask < 16; mask++) for (const failure of [-1, 0, 1, 2, 3]) resets.push(resetCase(mask, failure));
  for (const mode of ['ok', 'sync-error', 'async-error']) histories.push(await historyCase(mode));
  for (const mode of ['cached', 'ok', 'sync-error', 'async-error', 'race-ab', 'race-ba', 'race-a-error', 'race-b-error']) caches.push(await cacheCase(mode));
  process.stdout.write(JSON.stringify({ publicCases, resets, histories, caches }));
})().catch(error => { console.error(error); process.exitCode = 1; });
