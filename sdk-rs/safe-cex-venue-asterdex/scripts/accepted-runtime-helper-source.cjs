// Execute maintained expressions, including their getters/receivers, before projection.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {ts, root} = require('./source.cjs');
const file = relative => ts.createSourceFile(relative, fs.readFileSync(path.join(root, relative), 'utf8'), ts.ScriptTarget.Latest, true);
const api = file('packages/safe-cex/src/exchanges/asterdex/asterdex.api.ts');
const exchange = file('packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts');
const nodes = (tree, test) => {const out=[]; const visit=n=>{if(test(n))out.push(n);ts.forEachChild(n,visit);};visit(tree);return out;};
const unique = (tree, text) => {
  const matches=nodes(tree,n=>n.getText(tree)===text);
  assert.ok(matches.length, `maintained expression disappeared: ${text}`);
  return matches[0].getText(tree);
};
const evaluate = (expression, names, values) => new Function(...names, ts.transpileModule(`return (${expression});`, {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText)(...values);
const option = unique(api,'options.apiKey');
const optional = unique(api,'exchange?.options');
const clock = Object.fromEntries(['getCurrentTimeMs','needsSync','syncClientServerTime'].map(key=>{
 const call=nodes(api,n=>ts.isCallExpression(n)&&n.expression.getText(api)===`asterDEXVirtualClock.${key}`)[0];
 assert.ok(call,key);return [key,call.getText(api)];
}));
const detailCall=nodes(api,n=>ts.isCallExpression(n)&&n.expression.getText(api)==='getAsterDEXErrorDetail'&&n.arguments[0].getText(api)==='data.msg')[0];
assert.ok(detailCall);
const detailFn=nodes(api,n=>ts.isVariableDeclaration(n)&&n.name.getText(api)==='getAsterDEXErrorDetail')[0].initializer.getText(api);
const getAsterDEXErrorDetail=evaluate(detailFn,[],[]);
const tick=nodes(exchange,n=>ts.isPropertyDeclaration(n)&&n.name.getText(exchange)==='tick')[0];
assert.ok(tick);
const due=nodes(tick,n=>ts.isVariableDeclaration(n)&&n.name.getText(exchange)==='shouldReconcileBalance')[0].initializer.getText(exchange);
const update=nodes(tick,n=>ts.isCallExpression(n)&&n.expression.getText(exchange)==='this.store.update'&&n.arguments[0].getText(exchange).includes('orders,'))[0].getText(exchange);
const interval=nodes(exchange,n=>ts.isVariableDeclaration(n)&&n.name.getText(exchange)==='BALANCE_RECONCILIATION_INTERVAL_MS')[0].initializer.getText(exchange);
const intervalValue=evaluate(interval,[],[]);assert.equal(intervalValue,25000);
const invokeOwner=(expression,owner,names,values)=>evaluate(`function(${names.join(',')}) { ${expression} }`,['BALANCE_RECONCILIATION_INTERVAL_MS'],[intervalValue]).call(owner,...values);
const contracts=require('../../safe-cex-parity/accepted-set/contracts/safe-cex-venue-asterdex.json');
const prefix='safe_cex_venue_asterdex::';
const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
const describe=v=>v===undefined?{kind:'undefined'}:v===null?{kind:'null'}:typeof v==='number'?{kind:'number',bits:bits(v)}:typeof v==='object'?{kind:'object'}:{kind:typeof v,value:v};
let total=0;
for(const owner of ['api_runtime::option','runtime_host::opt','api_runtime::clock','api_runtime::detail','exchange_runtime::due','exchange_runtime::update']){
 for(const row of contracts[prefix+owner].cases){
  const scenario=row.input.case,sentinel={},thrown={};let actual;
  if(owner.endsWith('::option')||owner.endsWith('::opt')){
   const opt=owner.endsWith('::opt'),key=opt?'options':'apiKey';
   let o=scenario==='null-receiver'?null:scenario==='undefined-receiver'?undefined:scenario==='false-receiver'?false:{};
   if(scenario==='undefined')o[key]=undefined;if(scenario==='null')o[key]=null;
   if(scenario==='negative-zero')o[key]=-0;if(scenario==='identity')o[key]=sentinel;
   if(scenario==='inherited')Object.setPrototypeOf(o,{[key]:true});
   if(scenario==='getter-error')Object.defineProperty(o,key,{get(){throw thrown;}});
   try {const value=evaluate(opt?optional:option,[opt?'exchange':'options'],[o]);if(scenario==='identity')assert.equal(value,sentinel);actual=describe(value);}
   catch(error){if(scenario==='getter-error'){assert.equal(error,thrown);actual={error:'original'};}else actual={error:error.name};}
  }else if(owner.endsWith('::clock')){
   let calls=0;const key=scenario==='sync'?'syncClientServerTime':scenario==='needs'?'needsSync':'getCurrentTimeMs';
   const receiver={};if(scenario!=='missing')receiver[key]=function(...args){assert.equal(this,receiver);calls++;if(scenario==='sync'){assert.equal(args.length,2);assert.equal(args[0],sentinel);assert.ok(Object.is(args[1],-0));}else assert.deepEqual(args,[]);if(scenario==='throws')throw thrown;return scenario==='sync'?sentinel:scenario==='needs'?true:-0;};
   let result;try{const v=evaluate(clock[key],['asterDEXVirtualClock','options'],[receiver,{testnet:sentinel,apiVersion:-0}]);if(scenario==='sync')assert.equal(v,sentinel);result=describe(v);}catch(e){if(scenario==='throws'){assert.equal(e,thrown);result={error:'original'};}else result={error:e.name};}
   actual={result,calls};
  }else if(owner.endsWith('::detail')){
   const reads=[],imports=[];
   const data={};Object.defineProperty(data,'msg',{configurable:true,get(){reads.push('msg');if(scenario==='message-error')throw thrown;return 'detail';}});
   Object.defineProperty(data,'code',{get(){reads.push('code');if(scenario==='code-error')throw thrown;return scenario==='builder'?-5025:0;}});
   if(scenario==='false')Object.defineProperty(data,'msg',{value:false});if(scenario==='missing')delete data.msg;
   let result;try{result=describe(evaluate(detailCall.getText(api),['data','getAsterDEXErrorDetail'],[data,(...a)=>{imports.push('getAsterDEXErrorDetail');return getAsterDEXErrorDetail(...a);}]));}catch(e){assert.equal(e,thrown);result={error:'original'};}
   actual={result,reads,imports};
  }else if(owner.endsWith('::due')){
   let reads=0;const owner={get lastBalanceReconciliationAt(){reads++;if(scenario==='first-error'||scenario==='second-error'&&reads===2)throw thrown;return scenario==='zero'?0:scenario==='negative-zero'?-0:scenario==='string'?'1000':scenario==='nan'?NaN:scenario==='null'?null:1000;}};
   const now=scenario==='before'?25999:scenario==='after'?26001:scenario==='null'?25000:26000;
   let result;try{result=invokeOwner(`return (${due});`,owner,['now'],[now]);}catch(e){assert.equal(e,thrown);result={error:'original'};}
   actual={result,reads};
  }else{
   const trace=[],loaded=Object.assign(Object.create({inherited:true}),{balance:false,opaque:undefined}),rows=sentinel;
   const store={get loaded(){trace.push('loaded');if(scenario==='loaded-error')throw thrown;return loaded;},get update(){trace.push('update');if(scenario==='method-error')throw thrown;return function(patch){trace.push('call');assert.equal(this,store);assert.equal(patch.orders,rows);assert.notEqual(patch.loaded,loaded);assert.equal(patch.loaded.orders,true);assert.equal(patch.loaded.balance,false);assert.equal(patch.loaded.opaque,undefined);assert.ok(Object.hasOwn(patch.loaded,'opaque'));assert.equal(patch.loaded.inherited,undefined);if(scenario==='update-error')throw thrown;return 7;};}};
   let result;try{invokeOwner(`${update};`,{store},['orders'],[rows]);result='ok';}catch(e){assert.equal(e,thrown);result={error:'original'};}
   assert.equal(loaded.orders,undefined);actual={result,trace};
  }
  assert.deepEqual(actual,row.output,`${owner}/${scenario}`);total++;
 }
}
assert.equal(total,45);
console.log('Aster runtime helper maintained-source: 6 owners / 45 expectations PASS; identities/getters observed before projection.');
