// Execute the production Store and the live venue method, not a Store-shaped stub.
const fs = require('node:fs'), path = require('node:path');
const {createRequire} = require('node:module');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const runtime = createRequire(require.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript'));
const {root, load} = require('./tables.cjs');
const types = load(path.join(root, 'packages/safe-cex/src/types.ts'));
function moduleFrom(file, deps) {
  let source = fs.readFileSync(path.join(root, 'packages/safe-cex/src', file), 'utf8');
  if (file === 'store/store.base.ts' && process.env.KUCOIN_NOTIFY_MUTATION === 'continue') {
    const original = 'this.listeners.forEach((cb) => cb(this.state, fromWorker));';
    if (!source.includes(original)) throw Error('notification mutation anchor missing');
    source = source.replace(original, 'this.listeners.forEach((cb) => { try { cb(this.state, fromWorker); } catch {} });');
  }
  const module = {exports: {}};
  const code = ts.transpileModule(source, {compilerOptions: {
    target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS, esModuleInterop: true,
  }}).outputText;
  new Function('module', 'exports', 'require', code)(module, module.exports, name => {
    if (!Object.hasOwn(deps, name)) throw Error(`unexpected Store dependency ${name}`);
    return deps[name];
  });
  return module.exports;
}
const {DefaultStore} = moduleFrom('store/store.base.ts', {
  'lodash/uniqBy': runtime(process.env.KUCOIN_NOTIFY_MISSING_RUNTIME ? 'lodash/uniqBy-deliberately-absent' : 'lodash/uniqBy'),
  '../types': types,
  '../utils/clone': moduleFrom('utils/clone.ts', {}),
  '../utils/virtualClock': {virtualClock: {getCurrentTimeMs() { throw Error('unexpected clock use'); }}},
});
const file = path.join(root, 'packages/safe-cex/src/exchanges/kucoin/kucoinspot.exchange.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === 'KucoinSpotExchange');
const member = cls.members.find(n => n.name?.getText(ast) === 'setLeverage');
if (!member?.initializer) throw Error('setLeverage source missing');
const code = ts.transpileModule('module.exports = function(...args) { return (' + member.initializer.getText(ast) + ')(...args); };', {
  compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS},
}).outputText;
const moduleValue = {exports: {}};
new Function('module', code)(moduleValue);
module.exports = {DefaultStore};
if (require.main === module) (async () => {
  const rows = [];
  for (const symbol of ['BTC-USDT', '']) for (const throws of [false, true]) {
    const store = new DefaultStore(), trace = [], sentinel = Error('subscriber failure');
    const first = store.subscribe(state => {
      trace.push(['first', state.fetchedLeverage[symbol].buyLeverage]);
      if (throws) throw sentinel;
    });
    const second = store.subscribe(state => trace.push(['second', state.fetchedLeverage[symbol].sellLeverage]));
    let rejected = false, sameError = false;
    try { await moduleValue.exports.call({store}, symbol, 37); }
    catch (error) { rejected = true; sameError = error === sentinel; }
    rows.push({symbol, throws, trace, rejected, sameError, leverage: store.fetchedLeverage[symbol]});
    first(); second();
  }
  process.stdout.write(JSON.stringify(rows));
})().catch(error => { console.error(error); process.exitCode = 1; });
