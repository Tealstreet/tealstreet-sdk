const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
const file = path.join(root, 'packages/safe-cex/src/exchanges/phemex/phemex.exchange.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const venue = ast.statements.find(node => ts.isClassDeclaration(node) && node.name.text === 'PhemexExchange');
const body = ['formatSymbolIn', 'formatSymbolOut', 'fetchTickersRaw'].map(name => {
  const found = venue.members.filter(node => node.name?.getText(ast) === name);
  assert.equal(found.length, 1, name);
  return found[0].getText(ast);
}).join('\n');
const helpers = path.join(root, 'packages/safe-cex/src/utils/get-key.ts');
const moduleObject = {exports:{}};
const code = ts.transpileModule(fs.readFileSync(helpers, 'utf8'), {compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,esModuleInterop:true}}).outputText;
vm.runInThisContext('(function(require,module,exports){'+code+'\n})')(name => {
  assert(['lodash/camelCase','lodash/memoize','lodash/snakeCase'].includes(name), name);
  return require(name);
}, moduleObject, moduleObject.exports);
const context = vm.createContext({ENDPOINTS:{TICKERS:'/md/v3/ticker/24hr/all'},v:moduleObject.exports.v});
const Class = vm.runInContext(ts.transpileModule('class NativeMethods {'+body+'}\nNativeMethods', {compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.None}}).outputText, context);
const instance = new Class();
const units = text => Array.from({length:text.length}, (_, i) => text.charCodeAt(i));
(async () => {
  const uppercase = [];
  for (let unit=0xd800; unit<=0xdfff; unit++) uppercase.push(units(instance.formatSymbolIn(String.fromCharCode(unit,97))));
  assert.equal(uppercase.length, 2048, 'independent surrogate census');
  instance.store = {markets:[{symbol:'BTCUSDT',info:{symbol:String.fromCharCode(0xd800)}}]};
  const output = units(instance.formatSymbolOut('BTCUSDT'));
  const raw = {error:null,result:[{symbol:String.fromCharCode(0xd800)+'USDT'}]};
  instance.xhr = {get: async endpoint => {assert.equal(endpoint,'/md/v3/ticker/24hr/all');return {data:raw};}};
  instance.emitAndLogError = () => assert.fail('explicit null error must not emit');
  const returned = await instance.fetchTickersRaw();
  process.stdout.write(JSON.stringify({uppercase,output,raw:units(returned.result[0].symbol),same:returned===raw}));
})().catch(error=>{console.error(error);process.exitCode=1;});
