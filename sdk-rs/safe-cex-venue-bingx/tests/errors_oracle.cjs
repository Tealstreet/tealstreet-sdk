const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/bingx/bingx.exchange.ts');
const source = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const classes = source.statements.filter(s => ts.isClassDeclaration(s) && s.name?.text === 'BingxExchange');
assert.equal(classes.length, 1, 'BingxExchange census');
const names = ['buildTradfiOneWayModeError', 'normalizeOrderPlacementError'];
const declarations = names.map(name => {
  const fields = classes[0].members.filter(m => ts.isPropertyDeclaration(m) && m.name?.getText(source) === name);
  assert.equal(fields.length, 1, `helper census ${name}`);
  assert(fields[0].initializer && ts.isArrowFunction(fields[0].initializer), `helper arrow ${name}`);
  return `this.${name} = ${fields[0].initializer.getText(source)};`;
});
// Arrow functions retain the actual helper-to-helper call through lexical this.
const compiled = ts.transpileModule(`(function(){${declarations.join('\n')}return this;}).call({})`, {
  reportDiagnostics:true, compilerOptions:{ target:ts.ScriptTarget.ES2022, module:ts.ModuleKind.CommonJS }
});
assert(!compiled.diagnostics.some(d=>d.category===ts.DiagnosticCategory.Error), 'helper transpilation');
const helpers = vm.runInNewContext(compiled.outputText, {}, {filename:file});
const phrase = 'non-crypto symbol with one-way mode not support openapi and copy trade';
const symbols = ['', 'BTC-USDT', 'AAPL-USD', 'é🦀', 'x)\n'];
const messages = ['', 'ordinary error', phrase, phrase.toUpperCase(), `prefix ${phrase} suffix`,
  phrase.replace(' ', '  '), phrase.replace('openapi', 'openapı'), phrase.replace('support', 'ſupport'),
  phrase.replace('non-', 'NON-'), 'NaN', '0', 'false', 'é🦀'];
const build = [], normalize = [];
for(const symbol of symbols) {
  build.push({symbol, expected:helpers.buildTradfiOneWayModeError(symbol)});
  for(const message of messages) {
    build.push({symbol, message, expected:helpers.buildTradfiOneWayModeError(symbol,message)});
    normalize.push({symbol, message, expected:helpers.normalizeOrderPlacementError(symbol,message)});
  }
}
const boundary = names.map(name => {
  const symbol='a\ud800b', message=`${phrase} \udfff`;
  const output=helpers[name](symbol,message);
  return {name, symbolUnits:Array.from({length:symbol.length},(_,i)=>symbol.charCodeAt(i)),
    units:Array.from({length:output.length},(_,i)=>output.charCodeAt(i))};
});
process.stdout.write(JSON.stringify({runtime:{node:process.version,arch:process.arch,platform:process.platform},build,normalize,boundary}));
