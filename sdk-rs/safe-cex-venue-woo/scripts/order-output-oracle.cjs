// Audit witness, not a claim that the Rust execution handler is implemented.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const runtime=require('./oracle-runtime.cjs');
const root=path.resolve(__dirname,'../../../packages/safe-cex/src');
const cache=new Map();
function load(file){
 if(cache.has(file))return cache.get(file);
 let source=fs.readFileSync(file,'utf8');
 if(file===path.join(root,'types.ts')){const ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);source=ast.statements.filter(ts.isEnumDeclaration).map(n=>n.getText(ast)).join('\n');}
 const exports={};cache.set(file,exports);
 vm.runInNewContext(ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,require:name=>name.startsWith('.')?load(path.resolve(path.dirname(file),name)+'.ts'):name==='@tealstreet/math/src/isNotNumber'?load(path.resolve(root,'../../math/src/isNotNumber.ts')):runtime(name)},{filename:file});
 return exports;
}
const file=path.join(root,'exchanges/woo/woo.exchange.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const cls=ast.statements.find(ts.isClassDeclaration);
const methods=['mapLimitOrder','mapAlgoOrder'];
const members=methods.map(name=>{const member=cls.members.find(n=>n.name?.getText(ast)===name);assert.ok(member,name);return process.argv.includes('--snapshot-info')?member.getText(ast).replace(/info: o,/g,'info: {...o},'):member.getText(ast)});
const constants=load(path.join(root,'exchanges/woo/woo.types.ts'));
if(process.argv.includes('--correct-prototype'))for(const name of ['ORDER_SIDE','ORDER_TYPE','ORDER_TRIGGER_TYPE'])Object.setPrototypeOf(constants[name],null);
const context={...constants,...load(path.join(root,'types.ts')),...load(path.join(root,'utils/get-key.ts')),...load(path.join(root,'utils/safe-math.ts')),...load(path.join(root,'exchanges/woo/woo.utils.ts')),virtualClock:{getCurrentTimeMs:()=>1234}};
vm.runInNewContext(ts.transpileModule('class Witness {'+members.join('\n')+'}\nglobalThis.Witness=Witness;',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const worker=new context.Witness();worker.store={marketsBySymbol:{BTCUSDT:{id:'BTC',symbol:'BTCUSDT'}}};
const names=['constructor','toString','toLocaleString','valueOf','hasOwnProperty','isPrototypeOf','propertyIsEnumerable','__proto__','__defineGetter__','__defineSetter__','__lookupGetter__','__lookupSetter__'];
function pack(v){if(typeof v==='function')return {$type:'function',name:v.name};if(v===undefined)return {$type:'undefined'};if(v!==null&&Object.getPrototypeOf(v)===null)return {$type:'prototype-object'};if(Array.isArray(v))return v.map(pack);if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,v])=>[k,pack(v)]));return v;}
const cases=[];
for(const method of methods)for(const field of method==='mapLimitOrder'?['type','side']:['algoType','side','triggerPriceType'])for(const key of ['LIMIT','BUY','STOP_LOSS','MARK_PRICE','unknown',...names]){
 const row={order_id:'1',symbol:'PERP_BTC_USDT',type:'LIMIT',side:'BUY',quantity:3,executed:1,price:10,reduce_only:false,status:'NEW',algoOrderId:'2',algoType:'STOP_LOSS',triggerPriceType:'MARK_PRICE',triggerPrice:10,totalExecutedQuantity:1};row[field]=key;
 const result=worker[method](row);const output=method==='mapAlgoOrder'?result[0]:result;
 const target=field==='side'?'side':field==='triggerPriceType'?'slTriggerType':'type';
 if(names.includes(key))assert.equal(typeof output[target],key==='__proto__'?'object':'function','ts-bug inherited output '+method+' '+field+' '+key);
 cases.push({method,field,key,input:row,result:pack(result)});
}
const base={order_id:'1',symbol:'PERP_BTC_USDT',type:'LIMIT',side:'BUY',quantity:3,executed:1,price:10,reduce_only:false,status:'NEW',algoOrderId:'2',algoType:'STOP_LOSS',triggerPriceType:'MARK_PRICE',triggerPrice:10,totalExecutedQuantity:1};
const scenarios=[
 ['mapLimitOrder','zero quantity falls back',{...base,quantity:0,order_quantity:7}],
 ['mapLimitOrder','zero price falls back',{...base,price:0,order_price:12}],
 ['mapLimitOrder','zero executed retained',{...base,executed:0}],
 ['mapLimitOrder','false id skips',{...base,order_id:0}],
 ['mapLimitOrder','missing market skips',{...base,symbol:'PERP_MISSING_USDT'}],
 ['mapLimitOrder','cancelled post only',{...base,status:'CANCELLED',type:'POST_ONLY',side:'SELL'}],
 ['mapLimitOrder','string amount is copied',{...base,quantity:'3.00'}],
 ['mapLimitOrder','null reduce only copied',{...base,reduce_only:null}],
 ['mapLimitOrder','missing reduce only copied',Object.fromEntries(Object.entries(base).filter(([k])=>k!=='reduce_only'))],
 ['mapLimitOrder','alias id and symbol',{...base,order_id:null,orderId:'alternate',symbol:'BTC/USDT'}],
 ['mapAlgoOrder','zero trigger filters',{...base,triggerPrice:0}],
 ['mapAlgoOrder','trailing computes price',{...base,algoType:'TRAILING_STOP',triggerPrice:0,extremePrice:15,callbackValue:2}],
 ['mapAlgoOrder','take profit trigger branch',{...base,algoType:'TAKE_PROFIT'}],
 ['mapAlgoOrder','parent id and ordered children',{...base,rootAlgoOrderId:42,childOrders:[{...base,algoOrderId:'first',quantity:5},{...base,algoOrderId:'filtered',triggerPrice:0},{...base,algoOrderId:'last',algoType:'TAKE_PROFIT'}]}],
 ['mapAlgoOrder','empty child array stays empty',{...base,childOrders:[]}],
 ['mapAlgoOrder','null child falls back',{...base,childOrders:null}],
 ['mapAlgoOrder','truthy nonarray throws',{...base,childOrders:{}}],
 ['mapAlgoOrder','missing market skips',{...base,symbol:'PERP_MISSING_USDT'}],
];
for(const [method,label,input] of scenarios){
 let result,error;try{result=pack(worker[method](input))}catch(e){error=e.name+': '+e.message}
 cases.push({method,field:'scenario',key:label,input,...(error?{error}:{result})});
}
const aliases=[];
for(const method of methods){
 const input={...base,...(method==='mapAlgoOrder'?{childOrders:[{...base,algoOrderId:'child'}]}:{})};
 const result=worker[method](input);const order=method==='mapAlgoOrder'?result[0]:result;
 assert.equal(order.info,input,'source info retains parent input identity');
 const before=pack(order.info);input.afterMapping='visible';
 assert.equal(order.info.afterMapping,'visible','source info observes later parent relabel');
 aliases.push({method,before,after:pack(order.info)});
}
const numeric=[];
function bits(value) {
 assert.equal(typeof value,'number','numeric mapper observation before stringify');
 if(Number.isNaN(value))return 'NaN';
 const buffer=Buffer.alloc(8);buffer.writeDoubleBE(value);return buffer.toString('hex');
}
for(const method of methods)for(const field of method==='mapLimitOrder'?['quantity','price','executed']:['quantity','triggerPrice','totalExecutedQuantity'])for(const lexeme of ['-0','-0.0','-1e-4000','1e400','-1e400','9007199254740993']) {
 const input={...base,algoType:"TRAILING_STOP",order_quantity:7,order_price:12,extremePrice:15,callbackValue:2,[field]:JSON.parse(lexeme)};
 const mapped=worker[method](input);const output=method==='mapAlgoOrder'?mapped[0]:mapped;
 numeric.push({method,field,lexeme,bits:Object.fromEntries(['price','amount','filled','remaining','lastUpdate'].map(k=>[k,bits(output[k])]))});
}
const numberTarget=path.join(__dirname,'../tests/fixtures/order-numbers.json');
const numberBytes=JSON.stringify(numeric,null,2)+'\n';
if(process.argv.includes('--check'))assert.equal(fs.readFileSync(numberTarget,'utf8'),numberBytes,'mapper numeric pre-serialization bits drift');
else if(!process.argv.includes('--correct-prototype')&&!process.argv.includes('--snapshot-info'))fs.writeFileSync(numberTarget,numberBytes);
const target=path.join(__dirname,'../tests/fixtures/order-output.json');const bytes=JSON.stringify(cases,null,2)+'\n';
const aliasTarget=path.join(__dirname,'../tests/fixtures/order-alias.json');const aliasBytes=JSON.stringify(aliases,null,2)+'\n';
if(process.argv.includes('--check')){assert.equal(fs.readFileSync(target,'utf8'),bytes,'actual mapper pre-serialization output drift');assert.equal(fs.readFileSync(aliasTarget,'utf8'),aliasBytes,'mapper alias drift')}
else if(!process.argv.includes('--correct-prototype')&&!process.argv.includes('--snapshot-info')){fs.writeFileSync(target,bytes);fs.writeFileSync(aliasTarget,aliasBytes)}
if(process.argv.includes('--check')) {
 for(const [flag, diagnostic] of [
  ['--correct-prototype', 'ts-bug inherited output mapLimitOrder type constructor'],
  ['--snapshot-info', 'source info retains parent input identity'],
 ]) {
  const child=require('node:child_process').spawnSync(process.execPath,[__filename,flag],{encoding:'utf8',env:process.env});
  assert.equal(child.status,1,flag+' must fail, not merely execute');
  assert.ok(child.stderr.includes('AssertionError') && child.stderr.includes(diagnostic),flag+' must fail the named source assertion: '+child.stderr);
 }
}
console.log('WOO mapper pre-serialization cases:',cases.length);
// Filter runs over the complete array before any mapping or clock call.
const priorities=[];
for(const tail of [null,1]){
 let clockCalls=0;
 context.virtualClock.getCurrentTimeMs=()=>{clockCalls++;return 1234};
 const input={...base,childOrders:[{...base,algoType:'constructor'},tail]};
 let result,error;try{result=pack(worker.mapAlgoOrder(input))}catch(e){error={name:e.name,message:e.message}}
 priorities.push({input,result,error,clockCalls});
}
assert.equal(priorities[0].clockCalls,1,'null child is filtered before mapping');
assert.equal(priorities[0].result[0].type.$type,'function','inherited output survives preceding filter');
assert.equal(priorities[1].clockCalls,0,'later filter failure precedes all mapping');
assert.equal(priorities[1].error.name,'TypeError');
const priorityTarget=path.join(__dirname,'../tests/fixtures/algo-batch-priority.json');
const priorityBytes=JSON.stringify(priorities,null,2)+'\n';
if(process.argv.includes('--check'))assert.equal(fs.readFileSync(priorityTarget,'utf8'),priorityBytes,'algo batch error priority drift');
else if(!process.argv.includes('--correct-prototype')&&!process.argv.includes('--snapshot-info'))fs.writeFileSync(priorityTarget,priorityBytes);

// Refusal timing must retain all source clocks and later coercion failures.
const timing=[];
for(const [label,children] of [
 ['inherited then ordinary',[{...base,algoType:'constructor'}, {...base,algoOrderId:'later'}]],
 ['inherited then id TypeError',[{...base,algoType:'constructor'}, {...base,algoOrderId:{toString:null}}]],
 ['inherited then side TypeError',[{...base,algoType:'constructor'}, {...base,side:{toString:null}}]],
 ['inherited trigger then id TypeError',[{...base,triggerPriceType:'constructor',algoOrderId:{toString:null}}]],
 ...[null,[],0,false,'',1,true,'x'].map(tail=>['filter tail '+JSON.stringify(tail),[{...base,algoType:'constructor'},tail]]),
]) {
 let clockCalls=0;context.virtualClock.getCurrentTimeMs=()=>{clockCalls++;return 1234};
 const input={...base,childOrders:children};let result,error;
 try { result=pack(worker.mapAlgoOrder(input)); } catch(e) { error={name:e.name,message:e.message}; }
 timing.push({label,input,result,error,clockCalls});
}
assert.equal(timing[0].clockCalls,2,'typed output refusal must not stop later source clocks');
assert.equal(timing[1].clockCalls,1,'later ID failure follows first clock');
assert.equal(timing[1].error.name,'TypeError');
assert.equal(timing[3].clockCalls,0,'ID failure precedes inherited trigger output boundary');
const timingTarget=path.join(__dirname,'../tests/fixtures/algo-timing.json');
const timingBytes=JSON.stringify(timing,null,2)+'\n';
if(process.argv.includes('--check'))assert.equal(fs.readFileSync(timingTarget,'utf8'),timingBytes,'algo refusal timing source drift');
else if(!process.argv.includes('--correct-prototype')&&!process.argv.includes('--snapshot-info'))fs.writeFileSync(timingTarget,timingBytes);
console.log('WOO algo refusal timing:',timing.length);
