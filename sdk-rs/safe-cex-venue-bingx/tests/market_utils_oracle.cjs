const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/bingx/bingx.exchange.ts');
const source = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const classes = source.statements.filter(s => ts.isClassDeclaration(s) && s.name?.text === 'BingxExchange');
assert.equal(classes.length, 1);
const declarations = ['toBingxSymbol', 'isLikelyTradfiMarket'].map(name => {
  const fields = classes[0].members.filter(m => ts.isPropertyDeclaration(m) && m.name?.getText(source) === name);
  assert.equal(fields.length, 1, `actual helper ${name}`);
  assert(ts.isArrowFunction(fields[0].initializer));
  return `this.${name} = ${fields[0].initializer.getText(source)};`;
});
const compiled = ts.transpileModule(`(function(){${declarations.join('\n')}return this;}).call({store:{}})`, {
  reportDiagnostics:true, compilerOptions:{target:ts.ScriptTarget.ES2022, module:ts.ModuleKind.CommonJS}
});
assert(!compiled.diagnostics.some(d => d.category === ts.DiagnosticCategory.Error));
const helpers = vm.runInNewContext(compiled.outputText, {}, {filename:file});
const market = (id, base='BTC') => ({id,base,symbol:'unused',quote:'USDT',active:true,
  precision:{price:0.1,amount:1},limits:{amount:{min:1,max:10},leverage:{min:1,max:20}}});
const names = ['', 'USD', 'usd', 'XAUUSD-USDT', 'XAUUSDT-USD', 'XAUUSDC', '-USDT',
 'BTC-USDT', 'xUsD', 'USD\n', 'USD\r\n', 'USD\u2028', 'UſD', 'é🦀USD', '--USD'];
const bases = ['', 'BTC', 'XAUUSD', 'usd', 'USDT'];
const classify = [];
for (const id of names) for (const base of bases) {
  const input = market(id,base);
  classify.push({input,expected:helpers.isLikelyTradfiMarket(input)});
}
const symbols = ['', 'BTCUSDT', 'USDT', 'USDC', 'BTCUSDC', 'BTCusdt', 'BTC-USDT',
 'a-bUSDT', 'USD', 'BTCUSDT\n', 'é🦀USDT', ...Object.getOwnPropertyNames(Object.prototype)];
assert.equal(symbols.length,23, 'symbol input inventory');
const resolve = [];
for (const symbol of symbols) {
  const shapes = [ [[],[]], [[[symbol,market('symbol-id')]],[]], [[],[[symbol,market('id-id')]]],
    [[[symbol,market('symbol-id')]],[[symbol,market('id-id')]]],
    [[[symbol,market('')]],[[symbol,market('id-id')]]],
    [[[symbol,market('first')],[symbol,market('last')]],[]] ];
  for (const [bySymbol,byId] of shapes) {
    helpers.store = {marketsBySymbol:Object.fromEntries(bySymbol),marketsById:Object.fromEntries(byId)};
    resolve.push({symbol,bySymbol,byId,expected:helpers.toBingxSymbol(symbol)});
  }
}
helpers.store = {marketsBySymbol:{},marketsById:{}};
const lone='a\ud800USDT';
const output=helpers.toBingxSymbol(lone);
const boundary={units:Array.from({length:output.length},(_,i)=>output.charCodeAt(i)),
  classify:helpers.isLikelyTradfiMarket(market('a\ud800USD'))};
process.stdout.write(JSON.stringify({runtime:{node:process.version,arch:process.arch,platform:process.platform},classify,resolve,boundary}));
