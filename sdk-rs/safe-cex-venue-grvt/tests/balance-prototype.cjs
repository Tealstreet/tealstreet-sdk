'use strict';
// ts-bug: actual GRVT fetchBalance mutates inherited referents after its truthy guard.
// This witness does not implement a Rust balance mapper or a dictionary setter.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const compiler = process.env.SAFE_CEX_TYPESCRIPT_PATH;
assert(compiler && path.isAbsolute(compiler), 'absolute isolated compiler required');
const ts = require(compiler);
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/grvt/grvt.exchange.ts');
let source = fs.readFileSync(file, 'utf8');
if (process.env.GRVT_BALANCE_MUTATION === 'own-guard') {
  assert(source.includes('if (!currencies[currency])'), 'own-guard mutation matched');
  source = source.replace('if (!currencies[currency])', 'if (!Object.hasOwn(currencies, currency))');
} else assert(!process.env.GRVT_BALANCE_MUTATION, 'unknown balance mutation');
const tree = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
const declaration = tree.statements.find(s => ts.isClassDeclaration(s) && s.name?.text === 'GrvtExchange');
assert(declaration, 'actual GrvtExchange declaration');
const member = declaration.members.find(m => m.name?.getText(tree) === 'fetchBalance');
assert(member, 'actual fetchBalance member');
const compiled = ts.transpileModule(`class Probe { ${member.getText(tree)} }; globalThis.Probe = Probe;`, {
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
}).outputText;
const names = ['USDT', ...Object.getOwnPropertyNames(Object.prototype)];
assert.equal(names.length, 13, 'stock twelve Object.prototype members');
(async () => {
  const receipts = [];
  for (const key of names) {
    // Each case owns a fresh intrinsic Object.prototype, so pollution cannot leak.
    const context = vm.createContext({ key, LogSeverity: { Debug: 0, Info: 1, Error: 2 } });
    vm.runInContext(compiled, context, { filename: file });
    const receipt = await vm.runInContext(`(async () => {
      const originalPrototype = Object.prototype;
      const inherited = ({})[key];
      const beforeOwn = Object.hasOwn(originalPrototype, 'free');
      const instance = new Probe();
      instance.log = () => {};
      instance.grvtTradingAccountId = '1';
      Object.defineProperty(instance, 'store', { get() { throw Error('unexpected Store access'); } });
      instance.tradingXhr = { post: async (url, body) => {
        if (url !== '/lite/v1/aggregated_account_summary' || Object.keys(body).length) throw Error('REST seam changed');
        return { data: { r: { sb: [{ c: key, b: '5' }], tu: '0', te: '5', ts2: '5' } } };
      } };
      const result = await instance.fetchBalance();
      return {
        key, beforeOwn, own: Object.hasOwn(result.currencies, key),
        prototypeUnchanged: Object.getPrototypeOf(result.currencies) === originalPrototype,
        currencies: JSON.stringify(result.currencies),
        prototypeOwnFree: Object.hasOwn(originalPrototype, 'free'),
        prototypeOwnTotal: Object.hasOwn(originalPrototype, 'total'),
        emptyObjectFreeNaN: Number.isNaN(({}).free),
        unrelatedObjectTotalNaN: Number.isNaN(({ unrelated: true }).total),
        inheritedOwnFree: inherited !== undefined && Object.hasOwn(inherited, 'free'),
        inheritedFreeNaN: inherited !== undefined && Number.isNaN(inherited.free),
        inheritedTotalNaN: inherited !== undefined && Number.isNaN(inherited.total),
        totals: [result.total, result.free, result.used, result.upnl],
      };
    })()`, context);
    assert.equal(receipt.beforeOwn, false, 'fresh realm has no prior pollution');
    assert.equal(receipt.prototypeUnchanged, true, 'GRVT does not replace dictionary prototype');
    assert.equal(JSON.stringify(receipt.totals), '[5,5,0,0]', 'top-level balance survives');
    if (key === 'USDT') {
      assert.equal(receipt.own, true);
      assert.equal(receipt.currencies, '{"USDT":{"free":5,"used":0,"total":5,"upnl":0}}');
    } else {
      assert.equal(receipt.own, false, 'ts-bug: inherited currency is not stored');
      assert.equal(receipt.currencies, '{}', 'ts-bug: currency balance is lost');
      assert(receipt.inheritedOwnFree && receipt.inheritedFreeNaN && receipt.inheritedTotalNaN, 'ts-bug: inherited referent is mutated');
    }
    for (const field of ['prototypeOwnFree', 'prototypeOwnTotal', 'emptyObjectFreeNaN', 'unrelatedObjectTotalNaN']) {
      assert.equal(receipt[field], key === '__proto__', `ts-bug: global prototype pollution ${field}`);
    }
    receipts.push(receipt);
  }
  process.stdout.write(JSON.stringify(receipts));
})().catch(error => { console.error(error); process.exitCode = 1; });
