import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require = createRequire(import.meta.url);
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = new URL('../../../packages/safe-cex/src/exchanges/binance/', import.meta.url);
const parse = file => ts.createSourceFile(file, fs.readFileSync(new URL(file, root), 'utf8'), ts.ScriptTarget.Latest, true);
const output = {dispose:[], margin:[]};
for (const [variant, klass] of [['binance','BinanceExchange'],['binancepm','BinancePMExchange'],['binancespot','BinanceSpotExchange']]) {
  const ast = parse(`${variant}.exchange.ts`);
  const decl = ast.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === klass);
  assert.ok(decl);
  const method = decl.members.find(n => n.name?.getText(ast) === 'dispose');
  assert.ok(method?.initializer);
  const exports = {};
  // Keep the actual arrow field and super binding inside a class; do not rewrite
  // disposal statements or expand the separately ported BaseExchange.dispose.
  vm.runInNewContext(ts.transpileModule(`class BaseExchange { dispose() { this.baseHook(); } }\nclass Subject extends BaseExchange { ${method.getText(ast)} }\nexports.Subject = Subject;`, {compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText, {exports});
  for (const failureAt of [null,'base','public','private']) for (const detached of [false,true]) {
    const visited = [], disposed = {base:false,public:false,private:false};
    const sentinel = Object.assign(RangeError('fixture disposer'),{code:'dispose-code',response:{status:503,data:{msg:'dispose detail'}}});
    const invoke = operation => {visited.push(operation);disposed[operation]=true;if(operation===failureAt) throw sentinel;};
    const instance = new exports.Subject();
    Object.assign(instance,{baseHook:()=>invoke('base'),publicWebsocket:{dispose:()=>invoke('public')},privateWebsocket:{dispose:()=>invoke('private')}});
    let error = null, returnedUndefined = false;
    try {
      const fn = instance.dispose;
      const result = detached ? fn.call({baseHook:()=>assert.fail('arrow rebound')}) : instance.dispose();
      returnedUndefined = result === undefined;
    } catch (e) {error={name:e.name,message:e.message,code:e.code??null,response:e.response??null,same:e===sentinel};}
    const expectedCount = failureAt === 'base' ? 1 : failureAt === 'public' ? 2 : 3;
    assert.equal(visited.length, expectedCount);
    assert.equal(error === null, failureAt === null);
    output.dispose.push({variant,failureAt,detached,visited,disposed,error,returnedUndefined});
  }
}
const ast = parse('binance.exchange.ts');
const decl = ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='BinanceExchange');
const method = decl.members.find(n=>n.name?.getText(ast)==='changeMarginType');
assert.ok(method?.initializer);
const types = parse('binance.types.ts');
const endpoints = types.statements.flatMap(n=>ts.isVariableStatement(n)?[...n.declarationList.declarations]:[]).find(n=>n.name.getText(types)==='ENDPOINTS');
assert.ok(endpoints);
const exports = {};
vm.runInNewContext(ts.transpileModule(`const ENDPOINTS=${endpoints.initializer.getText(types)};exports.fn=function(...args){return (${method.initializer.getText(ast)})(...args);};`,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,{exports});
const fixtures = [];
for (const isolated of [true,false]) fixtures.push({isolated,symbol:'BTCUSDT',postReject:false,emitReject:false});
for (const symbol of [undefined,'']) for (const emitReject of [false,true]) fixtures.push({isolated:false,symbol,postReject:false,emitReject});
const messages = [undefined,'venue detail','',null,false,0,[],{id:'native'},{toString:null}];
for (const isolated of [true,false]) for (const msg of messages) for (const emitReject of [false,true]) fixtures.push({isolated,symbol:'BTCUSDT',postReject:true,emitReject,...(msg===undefined?{}:{msg})});
fixtures.push({isolated:true,symbol:' ',postReject:false,emitReject:false},{isolated:false,symbol:'ETHUSDT',postReject:false,emitReject:false});
assert.equal(fixtures.length,44);
for (let index=0;index<fixtures.length;index++) {
  const fixture = fixtures[index], posts=[], emissions=[], visited=[];
  const postError=Object.assign(RangeError('fixture post'),{code:'post-code',response:{status:503,data:{...(fixture.msg===undefined?{}:{msg:fixture.msg})}}});
  const emitterError=Object.assign(RangeError('fixture emitter'),{code:'emitter-code',response:{status:507,data:{msg:'emitter detail'}}});
  const instance={
    xhr:{post:async(path,payload)=>{visited.push('post');posts.push({path,payload});if(fixture.postReject) throw postError;}},
    emitAndLogError:message=>{visited.push('emit');emissions.push(message);if(fixture.emitReject) throw emitterError;},
  };
  let error=null,returnedUndefined=false;
  try {returnedUndefined=(await exports.fn.call(instance,fixture.isolated,fixture.symbol))===undefined;}
  catch(e){error={name:e.name,message:e.message,code:e.code??null,response:e.response??null,samePost:e===postError,sameEmitter:e===emitterError};}
  const missing=!fixture.symbol;
  assert.equal(posts.length, missing?0:1);
  assert.equal(emissions.length, missing||fixture.postReject?1:0);
  assert.equal(error===null,!missing&&!fixture.postReject);
  output.margin.push({index,...fixture,posts,emissions,visited,error,returnedUndefined});
}
assert.equal(output.dispose.length,24);
process.stdout.write(JSON.stringify(output));
