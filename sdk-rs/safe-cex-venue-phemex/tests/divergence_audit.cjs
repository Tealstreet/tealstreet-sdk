const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { createRequire } = require('node:module');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const dependency = createRequire(path.join(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'package.json'));
const root = path.resolve(__dirname, '../../..');
const control = process.env.SAFE_CEX_BLOCK_SOURCE_CONTROL;
assert([undefined, 'symbol-in', 'sync-clear'].includes(control), 'known Phemex audit control');
const cache = new Map();
function load(filename) {
  if (cache.has(filename)) return cache.get(filename);
  const module = { exports: {} };
  cache.set(filename, module.exports);
  const code = ts.transpileModule(fs.readFileSync(filename, 'utf8'), {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  }).outputText;
  vm.runInNewContext(`(function(require,module,exports){${code}\n})`, {}, { filename })(name => {
    if (name.startsWith('.')) return load(path.resolve(path.dirname(filename), name + '.ts'));
    if (name === '@tealstreet/logging/src/LogSeverity') return load(path.join(root, 'packages/logging/src/LogSeverity.ts'));
    return dependency(name);
  }, module, module.exports);
  return module.exports;
}
const constants = load(path.join(root, 'packages/safe-cex/src/exchanges/phemex/phemex.types.ts'));
const getKey = load(path.join(root, 'packages/safe-cex/src/utils/get-key.ts'));
function method(relative, ownerName, name, owner, globals = {}) {
  const filename = path.join(root, 'packages/safe-cex/src/exchanges/phemex', relative);
  const ast = ts.createSourceFile(filename, fs.readFileSync(filename, 'utf8'), ts.ScriptTarget.ES2022, true);
  const classes = ast.statements.filter(node => ts.isClassDeclaration(node) && node.name?.text === ownerName);
  assert.equal(classes.length, 1, 'PHEMEX-BLOCK unique source owner');
  const members = classes[0].members.filter(node => node.name?.getText(ast) === name);
  assert.equal(members.length, 1, 'PHEMEX-BLOCK unique source method');
  let initializer = members[0].initializer.getText(ast);
  if (control === 'symbol-in' && name === 'formatSymbolIn') {
    const before = 'symbol?.toUpperCase()';
    assert.equal(initializer.split(before).length, 2, 'PHEMEX-BLOCK uppercase selector');
    initializer = initializer.replace(before, 'symbol?.toUpperCase()?.replace(/\\ud800/g,"\\ufffd")');
  }
  if (control === 'sync-clear' && name === 'syncPositionsAndBalance') {
    const before = 'this.positionsAndBalanceSyncSymbols.clear();';
    assert.equal(initializer.split(before).length, 2, 'PHEMEX-BLOCK sync selector');
    initializer = initializer.replace(before, 'void 0;');
  }
  const code = ts.transpileModule(`(function(){ return ${initializer}; })`, {
    compilerOptions: { target: ts.ScriptTarget.ES2022 },
  }).outputText;
  return vm.runInNewContext(code, { ENDPOINTS: constants.ENDPOINTS, v: getKey.v, ...globals }).call(owner);
}
const exchangeMethod = (name, owner) => method('phemex.exchange.ts', 'PhemexExchange', name, owner);
const lone = JSON.parse('"\\ud800"');
function observeUnit(value, label, expected = [55296]) {
  assert.equal(typeof value, 'string', label + ' type');
  const units = Array.from({ length: value.length }, (_, index) => value.charCodeAt(index));
  assert.deepEqual(units, expected, label);
  return { type: typeof value, units };
}
async function main() {
  const rows = [];
  const uppercase = exchangeMethod('formatSymbolIn', {})(lone);
  rows.push({ iteration: 7, ...observeUnit(uppercase, 'PHEMEX-BLOCK returned uppercase unit') });
  const symbol = lone + 'USDT';
  const owner = { store: { marketsBySymbol: { [symbol]: { id: 'observed-market' } } } };
  owner.formatSymbolIn = exchangeMethod('formatSymbolIn', owner);
  const tickers = exchangeMethod('parseTickers', owner)({ result: [{ symbol }] });
  assert.equal(tickers.length, 1, 'PHEMEX-BLOCK independent ticker count');
  rows.push({ iteration: 1, ...observeUnit(tickers[0].symbol, 'PHEMEX-BLOCK returned ticker unit', [55296, 85, 83, 68, 84]) });
  const payload = { error: null, result: [{ symbol }] };
  const raw = await exchangeMethod('fetchTickersRaw', {
    xhr: { get: async () => ({ data: payload }) },
    emitAndLogError() { assert.fail('PHEMEX-BLOCK raw response must not emit'); },
  })();
  assert.equal(raw, payload, 'PHEMEX-BLOCK returned raw identity');
  rows.push({ iteration: 2, ...observeUnit(raw.result[0].symbol, 'PHEMEX-BLOCK raw symbol unit', [55296, 85, 83, 68, 84]) });
  const outbound = exchangeMethod('formatSymbolOut', { store: { markets: [{ symbol: 'BTC', info: { symbol: lone } }] } })('BTC');
  rows.push({ iteration: 3, ...observeUnit(outbound, 'PHEMEX-BLOCK outbound symbol unit') });
  const account = await exchangeMethod('getAccount', {
    xhr: { get: async () => ({ data: { data: { account: { userID: lone } } } }) },
    emitAndLogError() { assert.fail('PHEMEX-BLOCK account must not emit'); },
  })();
  rows.push({ iteration: 4, ...observeUnit(account.userId, 'PHEMEX-BLOCK returned user id unit') });
  const validationEmits = [];
  const validation = await exchangeMethod('validateAccount', {
    xhr: { get: async () => { throw { response: { data: { msg: lone } } }; } },
    emitAndLogError(message) { validationEmits.push(message); },
  })();
  assert.deepEqual(validationEmits, [undefined], 'PHEMEX-BLOCK validation emission before return');
  rows.push({ iteration: 11, ...observeUnit(validation.error, 'PHEMEX-BLOCK validation helper output only') });
  const typesFilename = path.join(__dirname, 'types_oracle.cjs');
  let types;
  const proxy = { env: process.env, version: process.version, versions: process.versions, arch: process.arch, platform: process.platform,
    stdout: { write(value) { types = JSON.parse(value); } } };
  vm.runInNewContext(`(function(require,__dirname,process){${fs.readFileSync(typesFilename, 'utf8')}\n})`,
    { Buffer, structuredClone }, { filename: typesFilename })(createRequire(typesFilename), __dirname, proxy);
  assert.equal(types.envelope.msg, 'string', 'PHEMEX-BLOCK actual generic message schema');
  const envelope = { ...JSON.parse(types.cases[0].raw), code: 0, msg: lone };
  rows.push({ iteration: 10, ...observeUnit(envelope.msg, 'PHEMEX-BLOCK envelope metadata unit') });
  rows.push({ iteration: 12, ...observeUnit(JSON.parse(types.surrogateRaw).account.currency, 'PHEMEX-BLOCK positions currency unit') });
  let reads = 0;
  const pending = new Promise(() => {});
  const syncOwner = { positionsAndBalanceSyncTimeout: 'armed', positionsAndBalanceSyncSymbols: new Set(['BTC']), isDisposed: false,
    parent: { fetchPositionsAndBalance() { reads++; return pending; } } };
  const continuation = method('phemex.ws-private.ts', 'BinancePrivateWebsocket', 'syncPositionsAndBalance', syncOwner)();
  assert.equal(typeof continuation.then, 'function', 'PHEMEX-BLOCK source continuation');
  assert.equal(syncOwner.positionsAndBalanceSyncTimeout, undefined, 'PHEMEX-BLOCK source reset timeout before return');
  assert.equal(syncOwner.positionsAndBalanceSyncSymbols.size, 0, 'PHEMEX-BLOCK source cleared symbols before return');
  assert.equal(reads, 1, 'PHEMEX-BLOCK source account read before return');
  rows.push({ iteration: 13, cleared: true, reads });
  rows.sort((left, right) => left.iteration - right.iteration);
  assert.deepEqual(rows.map(row => row.iteration), [1, 2, 3, 4, 7, 10, 11, 12, 13], 'PHEMEX-BLOCK independent observation identities');
  assert.equal(rows.length, 9, 'PHEMEX-BLOCK independent observation count');
  process.stdout.write(JSON.stringify({ profile: { node: process.version, arch: process.arch }, rows }));
}
main().catch(error => { console.error(error); process.exitCode = 1; });
