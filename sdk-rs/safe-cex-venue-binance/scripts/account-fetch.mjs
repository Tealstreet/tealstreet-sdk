import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require = createRequire(import.meta.url);
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = new URL('../../../packages/safe-cex/src/exchanges/binance/', import.meta.url);
const parse = file => ts.createSourceFile(file, fs.readFileSync(new URL(file, root), 'utf8'), ts.ScriptTarget.Latest, true);
const fixtures = [
  [{accountAlias:'first-alias'}, {accountAlias:'second-alias'}],
  [{accountAlias:113.75}], [{accountAlias:null}], [{accountAlias:false}],
  [{accountAlias:{id:'nested', count:7}}], [{accountAlias:[1,2,3]}],
  [{}], [], [null], [false], [0], ['row'], [[]],
  undefined, null, {}, {code:-42,msg:'venue detail'},
  {message:'fallback detail'}, {code:{toString:null}}, {msg:{toString:null}},
  false, 3, '', '🦀ab',
];
const output = [];
for (const [variant, klass, methodName, client] of [
  ['binance', 'BinanceExchange', 'getAccount', 'xhr'],
  ['binancepm', 'BinancePMExchange', 'getAccount', 'papiXhr'],
  ['binancespot', 'BinanceSpotExchange', 'fetchPositions', null],
]) {
  const ast = parse(`${variant}.exchange.ts`);
  const klassNode = ast.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === klass);
  const method = klassNode.members.find(n => n.name?.getText(ast) === methodName);
  assert.ok(method?.initializer);
  const types = parse(`${variant}.types.ts`);
  const endpoints = types.statements.flatMap(n => ts.isVariableStatement(n) ? [...n.declarationList.declarations] : []).find(n => n.name.getText(types) === 'ENDPOINTS');
  assert.ok(endpoints);
  const helperNames = ['describeBinancePMResponseShape','getBinancePMResponseMessage','requireBinancePMArrayResponse'];
  const helperNodes = variant === 'binancepm' ? ast.statements.filter(n => ts.isVariableStatement(n) && n.declarationList.declarations.some(d => helperNames.includes(d.name.getText(ast)))) : [];
  if (variant === 'binancepm') assert.equal(helperNodes.length, 3);
  const helpers = helperNodes.map(n => n.getText(ast)).join('\n');
  const exports = {};
  vm.runInNewContext(ts.transpileModule(`const ENDPOINTS=${endpoints.initializer.getText(types)};\n${helpers}\nexports.fn=function(){ return (${method.initializer.getText(ast)})(); };`, {compilerOptions:{module:ts.ModuleKind.CommonJS, target:ts.ScriptTarget.ES2022}}).outputText, {exports});
  const inputs = variant === 'binancespot' ? [
    {positions:[{symbol:'BTCUSDT',contracts:3.6},{symbol:'ETHUSDT',contracts:7}]},
    {positions:[]}, {positions:null}, {positions:false}, {positions:113.75},
    {positions:{id:'opaque',count:7}}, {}, undefined, null, [], false, 'text',
  ] : fixtures;
  for (let index = 0; index < inputs.length; index++) for (const reject of [false,true]) {
    const trace = [], data = inputs[index];
    const sentinel = Object.assign(RangeError('fixture request'), {code:'fixture-code',response:{status:503,data:{msg:'venue detail'}}});
    const instance = {
      [client || 'xhr']: {get: async (...args) => {trace.push({client,args});if(reject) throw sentinel;return {data};}},
      fetchPositionsAndBalance: async () => {trace.push({fetch:'fetchPositionsAndBalance'});if(reject) throw sentinel;return data;},
    };
    let result, error = null, sameValue = null;
    try {
      result = await exports.fn.call(instance);
      const inputValue = variant === 'binancespot' ? data?.positions : Array.isArray(data) ? data[0]?.accountAlias : undefined;
      sameValue = variant === 'binancespot' ? result === inputValue : result.userId === inputValue;

    } catch (e) {error = {name:e.name,message:e.message,code:e.code??null,response:e.response??null,same:e===sentinel};}
    if (!error) {
      assert.equal(sameValue, true);
      if (variant !== 'binancespot') assert.ok(Object.hasOwn(result, 'userId'));
    }
    assert.equal(trace.length,1);
    output.push({variant,index,reject,...(data===undefined?{}:{data}),...(result===undefined?{}:{result}),error,sameValue,trace});
  }
}
assert.equal(output.length,120);
process.stdout.write(JSON.stringify(output));
