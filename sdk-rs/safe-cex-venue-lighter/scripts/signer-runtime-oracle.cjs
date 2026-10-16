const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const assert = require('node:assert/strict'), crypto = require('node:crypto');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..'), dir = 'packages/safe-cex/src/exchanges/lighter/';
const sources = new Map();
function evaluate(file, imports, exposed = [], globals = {}) {
  const source = fs.readFileSync(path.join(root, file), 'utf8'); sources.set(file, source);
  const compiled = ts.transpileModule(source + '\n' + exposed.map(n => `exports.${n}=${n};`).join('\n'), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }, reportDiagnostics: true,
  });
  assert.equal(compiled.diagnostics.filter(d => d.category === ts.DiagnosticCategory.Error).length, 0);
  const output = {};
  vm.runInNewContext(compiled.outputText, { exports: output, Error, Promise, ...globals, require(n) {
    assert.ok(Object.hasOwn(imports, n), n); return imports[n];
  }});
  return output;
}
const domains = fs.readFileSync(path.join(root, 'packages/safe-cex/src/types.ts'), 'utf8');
sources.set('packages/safe-cex/src/types.ts', domains);
const tree = ts.createSourceFile('types', domains, ts.ScriptTarget.Latest, true), enums = {};
vm.runInNewContext(ts.transpileModule(tree.statements.filter(ts.isEnumDeclaration).map(n => n.getText(tree)).join('\n'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS },
}).outputText, { exports: enums });
const deployment = evaluate(dir + 'lighter.types.ts', { '../../types': enums, '../../transport/registry': { withPublicCorsProxy: url => url } });
function enc(v) {
  if (v === undefined) return { kind: 'undefined' };
  if (v === null) return { kind: 'null' };
  if (typeof v === 'boolean') return { kind: 'boolean', value: v };
  if (typeof v === 'number') { const b = Buffer.alloc(8); b.writeDoubleBE(v); return { kind: 'number', bits: b.toString('hex') }; }
  if (typeof v === 'bigint') return { kind: 'bigint', decimal: String(v) };
  if (typeof v === 'string') return { kind: 'string', units: Array.from({ length: v.length }, (_, i) => v.charCodeAt(i)) };
  if (Array.isArray(v)) return { kind: 'array', items: Array.from({ length: v.length }, (_, i) => i in v ? enc(v[i]) : { kind: 'hole' }) };
  return { kind: 'object', fields: Object.entries(v).filter(([, v]) => typeof v !== 'function').map(([k, v]) => [k, enc(v)]) };
}
const names = ['createOrder', 'createOrders', 'createGroupedOrders', 'cancelOrder', 'updateLeverage', 'approveIntegrator'];
const signing = ['getCreateOrderTransaction', 'getCreateOrderTransactions', 'getCreateGroupedOrdersTransaction', 'getCancelOrderTransaction', 'getUpdateLeverageTransaction', 'getApproveIntegratorTransaction'];
const rows = [];
const add = (id, method, extra = {}) => rows.push({ id, method, ...extra });
for (const mode of ['ready', 'load', 'existing', 'missing', 'ordinary', 'race', 'retry-failure', 'init-matching', 'constructor-race', 'testnet', 'robinhood', 'secret-sequence']) add('init-' + mode, 'initialize', { mode });
for (const method of names) for (const mode of ['ok', 'nonce', 'non-nonce', 'retry-failure', 'sign-failure', 'sign-rejection']) add(method + '-' + mode, method, { mode });
for (const mode of ['missing-secret', 'reset-throw', 'classifier-throw', 'delay-throw', 'retry-reset-throw']) add('createOrder-' + mode, 'createOrder', { mode });
for (const mode of ['empty', 'empty-missing-secret', 'length-throw']) add('createOrders-' + mode, 'createOrders', { mode });
for (const mode of ['ok', 'nonce', 'error']) add('queue-' + mode, 'queue', { mode });
for (const mode of ['ok', 'error', 'throw']) add('lock-' + mode, 'lock', { mode });
const floors = { initialize: 12, createOrder: 11, createOrders: 9, createGroupedOrders: 6, cancelOrder: 6, updateLeverage: 6, approveIntegrator: 6, queue: 3, lock: 3 };
for (const [family, count] of Object.entries(floors)) assert.equal(rows.filter(r => r.method === family).length, count);
assert.equal(rows.length, 62);
async function run(row) {
  const events = [], calls = [], constructors = [], counts = {}, originals = new Map();
  const http = { imported: 'http' }, cryptoApi = { imported: 'crypto' };
  const request = { orderId: 9007199254740993123n, marketId: 17, isBuy: true, price: 29n, amount: 31n,
    postOnly: false, reduceOnly: true, marker: row.id, integratorAccountIndex: 43n,
    maxPerpsTakerFee: 1, maxPerpsMakerFee: 2, maxSpotTakerFee: 3, maxSpotMakerFee: 4, approvalExpiry: 47n };
  const batch = [request, { ...request, orderId: -7n, marker: 'second' }];
  const input = row.mode === 'empty' || row.mode === 'empty-missing-secret' ? [] : row.mode === 'length-throw' ? Object.defineProperty({}, 'length', { get() { throw injected('length'); } }) : batch;
  const options = { secret: row.mode.includes('missing') ? '' : 'private', testnet: row.mode === 'testnet' };
  if (row.mode === 'secret-sequence') {
    let i = 0; Object.defineProperty(options, 'secret', { enumerable: true, get() { events.push('secret:' + i); return ['initial', 'stored'][Math.min(i++, 1)]; } });
  }
  const exchangeName = row.mode === 'robinhood' ? 'lighter-robinhood' : 'lighter';
  function injected(stage, message = 'injected:' + stage, code) {
    const e = new Error(message); if (code !== undefined) e.code = code;
    originals.set(e, stage); return e;
  }
  function step(stage, args = []) {
    const n = counts[stage] = (counts[stage] || 0) + 1; events.push(stage);
    if (args.length) calls.push({ name: stage, args, requestIdentity: signing.includes(stage) ? args[0] === (stage === 'getCreateOrderTransactions' ? input : request) : undefined });
    const mode = row.mode;
    if (stage === 'crypto' && mode === 'ordinary') throw injected('crypto');
    if (stage === 'crypto' && (mode === 'race' && n === 1 || mode === 'retry-failure' && row.method === 'initialize')) throw injected('crypto:' + n, 'WASM NOT INITIALIZED');
    if (stage === 'init' && mode === 'init-matching' && n === 1) throw injected('init:1', 'wasm not initialized');
    if (stage === 'TransactionClient' && mode === 'constructor-race' && n === 1) throw injected('construct:1', 'wasm not initialized');
    if (stage === 'resetNonce' && (mode === 'reset-throw' || mode === 'retry-reset-throw' && n === 2)) throw injected('reset:' + n);
    if (stage === 'delay' && mode === 'delay-throw') throw injected('delay');
    if (signing.includes(stage) && (mode === 'sign-failure' || mode === 'sign-rejection')) {
      const e = injected('sign'); if (mode === 'sign-rejection') return Promise.reject(e); throw e;
    }
    if ((stage === 'sendRawTx' || stage === 'sendRawTxBatch') && (mode === 'nonce' || mode === 'retry-failure' || mode === 'retry-reset-throw' || mode === 'delay-throw') && (n === 1 || mode === 'retry-failure' || mode === 'retry-reset-throw')) throw injected('send:' + n, n === 1 ? 'bad nonce' : 'retry rejected', n === 1 ? 21104 : undefined);
    if ((stage === 'sendRawTx' || stage === 'sendRawTxBatch') && (mode === 'non-nonce' || mode === 'reset-throw' || mode === 'classifier-throw' || row.method === 'queue' && mode === 'error') && n === 1) {
      const e = injected('send:1'); if (mode === 'classifier-throw') Object.defineProperty(e, 'code', { get() { throw injected('classifier'); } }); throw e;
    }
  }
  let release, held;
  function send(stage, tx) {
    const finish = () => { step(stage, [tx]); return stage === 'sendRawTxBatch' ? { txHashes: ['batch-a', 'batch-b'], extra: 'retained' } : 'hash:' + counts[stage]; };
    if (row.method === 'queue' && !held) { held = true; events.push('held-send'); return new Promise(resolve => { release = () => { try { resolve(finish()); } catch (e) { resolve(Promise.reject(e)); } }; }); }
    return finish();
  }
  Object.defineProperties(http, { sendRawTx: { value: tx => send('sendRawTx', tx) }, sendRawTxBatch: { value: tx => send('sendRawTxBatch', tx) } });
  class Client {
    constructor(opts) { constructors.push(opts); step('TransactionClient'); this.options = opts; }
    resetNonce() { step('resetNonce'); }
  }
  for (const name of signing) Client.prototype[name] = function(request) {
    const result = step(name, [request]); if (result) return result;
    return Promise.resolve(name === 'getCreateOrderTransactions' ? [{ tx: 'a' }, { tx: 'b' }] : { tx: name });
  };
  const module = evaluate(dir + 'lighter.signer.ts', {
    '@tealstreet/lighter-sdk': { LighterApiClient: class {}, TransactionClient: Client },
    '@tealstreet/poseidon-crypto': { getCryptoApi() { step('crypto'); return cryptoApi; }, isWasmReady() { step('ready'); return !['load', 'init-matching'].includes(row.mode); }, async initWasm() { step('init'); } },
    './lighter.types': deployment,
  }, ['TealstreetLighterSignerAdapter'], { setTimeout(fn, milliseconds) { step('delay', [milliseconds]); fn(); } });
  const adapter = new module.TealstreetLighterSignerAdapter(options, exchangeName, http, 123456789012345678901n, 7);
  if (row.mode === 'existing') adapter.transactionClient = { existing: true };
  function outcome(p) { return p.then(v => ({ kind: 'value', value: enc(v) }), e => ({ kind: 'error', name: e.name, message: enc(e.message), original: originals.get(e) || null })); }
  let beforeRelease, result;
  if (row.method === 'queue') {
    const first = outcome(adapter.createOrder(request)), second = outcome(adapter.cancelOrder(request));
    for (let i = 0; i < 32; i++) await Promise.resolve();
    assert.ok(release); assert.equal(counts.getCancelOrderTransaction || 0, 0);
    beforeRelease = { events: [...events], calls: enc(calls), constructors: enc(constructors) };
    release(); result = [await first, await second];
  } else if (row.method === 'lock') {
    const bound = adapter.runWithSubmissionLock;
    const first = outcome(bound(() => { events.push('callback:first'); if (row.mode === 'throw') throw injected('callback'); return new Promise((resolve, reject) => { release = () => row.mode === 'error' ? reject(injected('callback')) : resolve('first'); }); }));
    const second = outcome(bound(() => { events.push('callback:second'); return Promise.resolve('second'); }));
    for (let i = 0; i < 32; i++) await Promise.resolve();
    beforeRelease = { events: [...events], calls: enc(calls), constructors: enc(constructors) };
    if (release) release(); result = [await first, await second];
  } else result = [await outcome(row.method === 'initialize' ? adapter.initialize() : adapter[row.method](row.method === 'createOrders' ? input : request))];
  return { ...row, options: row.mode === 'secret-sequence' ? enc({ secret: 'initial', testnet: false }) : enc(options), exchangeName: enc(exchangeName), request: enc(request), batch: enc(batch), input: row.mode === 'length-throw' ? undefined : enc(input),
    result, events, calls: enc(calls), constructors: enc(constructors), beforeRelease,
    clientPresent: Boolean(adapter.transactionClient), initCount: counts.init || 0, resetCount: counts.resetNonce || 0 };
}
(async () => {
  const cases = []; for (const row of rows) cases.push(await run(row));
  process.stdout.write(JSON.stringify({ count: 62, sources: Object.fromEntries([...sources].map(([file, value]) => [file, crypto.createHash('sha256').update(value).digest('hex')])), cases }));
})().catch(e => { console.error(e); process.exitCode = 1; });
